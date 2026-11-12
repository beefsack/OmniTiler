#![forbid(unsafe_code)]

//! Phase 1 runtime owners and per-owner machinery (implementation).
//!
//! This module holds the thread bodies plus the machinery they own: the
//! wake-driven [`Inbox`] and per-app latest-wins [`FrameSlot`]. Retained
//! core state and the [`tiler_core`] event constructor live in
//! [`crate::runtime_core`]; the public typed surface lives in
//! [`crate::runtime`], which alone spawns and supervises these owners.

use std::collections::VecDeque;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use tiler_core::directional::WindowId;

use crate::runtime::{
    AxCompletion, FocusCommand, FocusRequest, FrameCommand, InputSemanticAction, ObservedWindow,
    Shared, emit, lock_shared, window_id,
};
use crate::runtime_lifecycle::EngineExit;
use crate::runtime_log::sanitize_backend_reason;
use crate::runtime_types::ScopedBackendSnapshot;
use crate::world_command_map::{
    OP_DRAG_GESTURE, OP_RECONCILE, OP_SELECT, OP_SEND, OP_TERMINATE, operation_command,
    select_command, select_output,
};
use crate::world_dispatch::{DispatchVerdict, dispatch_gesture_report, dispatch_report};
use crate::world_runtime::{
    DispatchContext, WorldSession, forget_world_members, stage_write_feedback,
};

// -- Wake-driven inbox ---------------------------------------------------------

/// Engine-owner job. Control jobs (lifecycle, barriers, fault injection)
/// bypass the bounded data queue; data jobs (observations, input actions,
/// worker results) are bounded and drop counted when full.
pub(crate) enum EngineJob {
    Launch {
        pid: u32,
        generation: u64,
    },
    Terminate {
        pid: u32,
        generation: u64,
    },
    Observation {
        pid: u32,
        generation: u64,
        seq: u64,
        windows: Vec<ObservedWindow>,
        focused_wid: Option<u32>,
    },
    /// Native-mode scoped observation: per-window Space membership plus the
    /// context revision and worker-stamped focus evidence ride along, so the
    /// engine admits exactly-homed retained-domain members per domain
    /// without ever inferring Space from geometry.
    ScopedObservation {
        pid: u32,
        generation: u64,
        seq: u64,
        snapshot: ScopedBackendSnapshot,
        /// Stage-timing correlation for host-driven polls: opaque trace id
        /// minted at poll-result receipt plus the receipt stamp
        /// (`intake_us`) and the admission enqueue stamp (`enqueue_us`),
        /// microseconds since the stage-timing clock origin. Zero when
        /// timing is disabled; the engine treats 0 as untraced (no records,
        /// no clock reads). There is no tap/notification callback on this
        /// path, so poll-result receipt IS intake.
        trace_id: u64,
        intake_us: u64,
        enqueue_us: u64,
    },
    InputAction {
        action: InputSemanticAction,
        /// Stage-timing correlation: opaque trace id minted at intake plus
        /// the callback receipt stamp (`intake_us`) and the admission
        /// enqueue stamp (`enqueue_us`), both microseconds since the
        /// stage-timing clock origin. Zero when timing is disabled; the
        /// engine treats 0 as untraced (no records, no clock reads).
        trace_id: u64,
        intake_us: u64,
        enqueue_us: u64,
    },
    /// Menu-bar segment click: the armed action plus its immutable
    /// render-time binding (expected SID, UI generation, topology
    /// revision). Admitted under the exact same gates as [`EngineJob::InputAction`];
    /// the binding rides into the pending [`ClaimedSelect`] so the
    /// main-thread drain refuses it against the fresh row instead of
    /// interpreting the ordinal against a different generation. Control
    /// lane (never silently dropped: every authorized command reaches the
    /// core FIFO in order; only the drain serializes actuation, never
    /// blocks AppKit).
    IndicatorSelect {
        action: InputSemanticAction,
        expected_sid: u64,
        ui_generation: u64,
        topology_revision: u64,
        /// Stage-timing correlation, as for [`EngineJob::InputAction`].
        trace_id: u64,
        intake_us: u64,
        enqueue_us: u64,
    },
    /// Read-only managed-facts query for the dedicated send-effect owner:
    /// publish the engine's current [`EngineManagedFacts`](
    /// crate::runtime_core::EngineManagedFacts) for one runtime window
    /// identity, or `None` when the engine no longer manages it. Control
    /// lane (never silently dropped - the owner always gets its reply or
    /// an engine-dead refusal), strict FIFO with observations, so the reply
    /// reflects every observation submitted before the query. The owner
    /// treats a missing/late reply as unknown facts and refuses without
    /// actuation - native OS reads supplement these facts, never replace
    /// them.
    ManagedFactsQuery {
        window: String,
        reply: mpsc::Sender<Option<crate::runtime_core::EngineManagedFacts>>,
    },
    /// Read-only retained occupancy query for the menu-bar indicator:
    /// snapshot the engine's per-domain managed membership (current and
    /// hidden alike) plus the context revision it was homed under, and
    /// reply with the pure report. Stages nothing, plans nothing, touches
    /// no reconciler state. Control lane (never silently dropped - the
    /// main thread always gets its reply or an engine-dead refusal),
    /// strict FIFO with observations, so the reply reflects every
    /// observation submitted before the query.
    WorkspaceOccupancyQuery {
        reply: mpsc::Sender<crate::runtime::WorkspaceOccupancyReport>,
    },
    /// Meta+left drag gesture ingress for the tile route: one
    /// [`tiler_core::gesture::GestureIngress`] phase (Started, Updated,
    /// Released, or Cancelled) dispatched through the SAME world gesture
    /// lifecycle the offline harness traverses, with the requester-
    /// correlated transient reply (preview outcome plus the incorporated
    /// terminal, if any). Control lane (never silently dropped - the main
    /// thread always gets its reply or an engine-dead refusal), strict
    /// FIFO with observations. A dropped/closed inbox or a dead engine
    /// surfaces as `None` main-side, which cancels the gesture stale.
    /// `press_revision` is the topology revision proven at press time;
    /// a drifted revision refuses stale before dispatch (never a late
    /// commit). Core owns targeting, preview, and commit; the adapter
    /// never mints hover priors or drop targets.
    DragGesture {
        ingress: tiler_core::gesture::GestureIngress,
        press_revision: Option<u64>,
        reply: mpsc::Sender<DragGestureReply>,
    },
    AxResult {
        pid: u32,
        generation: u64,
        cmd_id: u64,
        completion: AxCompletion,
    },
    /// Honest focus-dispatch completion from the owning AX worker (the SAME
    /// owner that observed the window). `cmd_id` carries the focus
    /// `request_id`; dispatch truth only - arrival is verified by later
    /// observations, never claimed here.
    FocusResult {
        pid: u32,
        generation: u64,
        cmd_id: u64,
        completion: AxCompletion,
    },
    /// Narrowly typed close-focus result from the owning AX worker (the
    /// SAME owner that observed the window): the frozen close stamp
    /// (`close_id`, `target`, `issued_rev`) plus the honest mechanical
    /// outcome. Sent only after the actual owner attempt or owner no-call
    /// (never pre-dispatch, never on queueing): the engine stages it as
    /// the next report's raw close-focus feedback row. No verdict ingress —
    /// the core settles from the row. Data lane (drops counted when full,
    /// never blocking).
    CloseFocusResult {
        pid: u32,
        generation: u64,
        cmd_id: u64,
        close_id: String,
        target: WindowId,
        issued_rev: u64,
        adapter_incarnation: String,
        core_generation: String,
        setter: tiler_core::world_report::VisibilitySetter,
        reason: tiler_core::world_report::VisibilityReason,
        pre: Option<WindowId>,
        post: Option<WindowId>,
        foreground_proven: bool,
        arrival: Vec<tiler_core::world_report::ArrivalBracket>,
        attempt_ms: u64,
    },
    /// Owner-side selection-proof deferral (internal, never a verdict): the
    /// owning worker's pre-setter gate found the frozen proof rows
    /// incomplete or the original proof stamp moved, so no setter ran and
    /// no feedback stages. Carries the immutable request tuple, the owning
    /// app lifetime, and the exact queued command identity (no new wire
    /// id). The engine accepts only the exact still-current queued pending
    /// tuple: it resets `owner_queued` plus the stamp and retains the
    /// original deps and baselines, so evidence refresh or engine
    /// reevaluation can queue the same unstarted work again. Data lane
    /// (drops counted when full, never blocking); a dropped notification
    /// merely defers.
    SelectionFocusDeferred {
        pid: u32,
        generation: u64,
        focus_id: String,
        issued_revision: u64,
        adapter_incarnation: String,
        core_generation: String,
        cmd_id: u64,
        target: WindowId,
    },
    /// Raw mechanical view-attempt evidence from one proven select-drain
    /// outcome (never a verdict): the setter disposition plus the drain's
    /// native Space reads around the actuation. The engine resolves the
    /// retained transfer target by operation identity and stages the raw
    /// native SIDs on the report's dedicated native attempt keys (logical
    /// pre/post/baseline stay absent); the core translates them through
    /// its own adoption mapping at settle time (a live core lookup, never
    /// adapter mapping). `pre_sid`/`post_sid` are the real write-time
    /// reads; `baseline_sid` is the observation-time native current
    /// captured in the reply plan at dispatch. Unknown operations drop
    /// without evidence.
    /// Staged into the next world report so the core settles its retained
    /// planner-owned view target.
    /// Data lane (drops counted when full, never blocks); a dropped row
    /// merely delays settlement - retained core targets never re-actuate
    /// without fresh proof.
    ViewAttempt {
        operation_id: String,
        setter: tiler_core::world_report::FrameSetter,
        /// Native Space id read immediately before the drain actuated
        /// (`None` unread).
        pre_sid: Option<u64>,
        /// Native Space id read immediately after the drain actuated
        /// (`None` unread/unproven).
        post_sid: Option<u64>,
        /// Observation-time native current for the target output captured
        /// at dispatch (`None` unread).
        baseline_sid: Option<u64>,
    },
    /// Raw mechanical membership-attempt evidence from one proven
    /// send-drain outcome (never a verdict): the setter disposition plus
    /// the drain-verified arrival Space. The engine resolves the retained
    /// transfer target (window plus target) by operation identity and
    /// stages the raw arrival SID on the report's dedicated native post
    /// key; the core translates it through its own adoption mapping at
    /// settle time (a live core lookup, never adapter mapping). Only
    /// unknown operations drop without evidence (raw native ids are
    /// core-owned: the adapter never drops an unmapped id). Staged
    /// into the next world report so the core settles its retained window
    /// transfer.
    MemberAttempt {
        operation_id: String,
        setter: tiler_core::world_report::FrameSetter,
        /// Native Space id read immediately before the drain actuated
        /// (`None` unread). Proven refusal needs this unchanged-readable
        /// preimage; anything else holds ambiguous.
        pre_sid: Option<u64>,
        /// Native Space id of the drain-verified arrival (`None` unless
        /// the drain proved arrival there).
        post_sid: Option<u64>,
    },
    /// Raw mechanical native-creation evidence from one select-drain
    /// creation leg (never a verdict): the create leg's setter disposition
    /// plus the genuine native orders around it and, only on a verified
    /// exact append, the host-correlated created desktop id. Correlated by
    /// the desire's operation id plus logical plus output exactly - the
    /// core consumes only the first exact-correlated attempt per op and
    /// latches ambiguity on changed replays, so the drain reports once
    /// under the desire's identity. `order_after` repeats `order_before`
    /// when no verified delta was observed (never a fabricated empty
    /// after-order); `created_sid` is `Some` only for a verified append
    /// (never guessed). Staged into the next world report so the core
    /// binds the appended native to its retained trailing logical.
    /// Data lane (drops counted when full, never blocks).
    NativeCreation {
        operation_id: String,
        logical: String,
        output: tiler_core::directional::OutputId,
        setter: tiler_core::world_report::FrameSetter,
        order_before: Vec<u64>,
        order_after: Vec<u64>,
        created_sid: Option<u64>,
        attempt_ms: u64,
    },
    /// Owner arrival-capture readiness notification (evidence only, never a
    /// verdict): one owning per-app worker's ordered bracket reads for the
    /// frozen members it owns, sampled around one agreed watermark. The
    /// sender never bumps the shared clock for this job (it carries raw
    /// reads of a fixed sampled watermark), so multi-pid captures sharing
    /// one watermark stay jointly current. Data lane (drops counted when
    /// full, never blocking); a dropped notification merely defers — the
    /// pending intent stays bound for a later publication. The engine
    /// validates the exact current request tuple, app generation, watermark,
    /// and revision before retaining, then reevaluates the latest same-id
    /// deferred intent from the current reply.
    ArrivalCaptureReady {
        pid: u32,
        generation: u64,
        focus_id: String,
        issued_revision: u64,
        adapter_incarnation: String,
        core_generation: String,
        obs: u64,
        revision: u64,
        context_generation: u64,
        captures: Vec<crate::runtime_types::ArrivalCapture>,
    },
    /// Rendezvous for deterministic tests: processed in FIFO order, so a
    /// completed barrier proves every earlier job was processed.
    Barrier(mpsc::Sender<()>),
    /// Offline fault injection only: makes the engine thread panic to prove
    /// panic handling. Never sent by production paths.
    TestPanic,
}

/// Coherence-relevant pending input: a queued job whose truth the next
/// report may not incorporate. Only observations (legacy/scoped) and
/// terminates qualify - commands and feedback change planned state, not
/// observed truth, and launches register empty apps that change no plan.
/// Cloned out of the inbox under one lock for the emission gate to
/// classify (see `emission_coherent` in [`crate::world_runtime`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoherencePending {
    Observation {
        pid: u32,
        generation: u64,
        seq: u64,
    },
    ScopedObservation {
        pid: u32,
        generation: u64,
        seq: u64,
        revision: u64,
    },
    Terminate {
        pid: u32,
        generation: u64,
    },
}

impl EngineJob {
    /// Data jobs share the bounded lane; control jobs (lifecycle, barriers,
    /// fault injection, user intent, read-only queries, gestures) bypass
    /// saturation but keep FIFO order with data. User intent
    /// ([`Self::InputAction`], [`Self::IndicatorSelect`]) rides the control
    /// lane so a saturated observation queue can never discard an
    /// authorized command before the core: every admitted command reaches
    /// the core FIFO in order, and only the drain serializes actuation.
    /// Observations and drain-evidence feedback stay bounded (drops
    /// counted, never blocking producers).
    fn is_data(&self) -> bool {
        matches!(
            self,
            Self::Observation { .. }
                | Self::ScopedObservation { .. }
                | Self::ViewAttempt { .. }
                | Self::MemberAttempt { .. }
                | Self::NativeCreation { .. }
                | Self::ArrivalCaptureReady { .. }
                | Self::AxResult { .. }
                | Self::FocusResult { .. }
                | Self::CloseFocusResult { .. }
                | Self::SelectionFocusDeferred { .. }
        )
    }
}

struct InboxState {
    queue: VecDeque<EngineJob>,
    data_len: usize,
    data_capacity: usize,
    closed: bool,
}

/// Wake-driven bounded inbox: one `Mutex` plus one `Condvar`, no polling.
/// One strict FIFO queue keeps submission order (a barrier proves every
/// earlier job was processed); only observation/feedback data admission is
/// bounded (`try_send_data` drops counted when full) while user intent,
/// lifecycle, queries, gestures, barriers, and stop admission bypass
/// saturation (`send_control` fails only when closed).
pub(crate) struct Inbox {
    state: Mutex<InboxState>,
    wake: Condvar,
}

impl Inbox {
    pub(crate) fn new(data_capacity: usize) -> Self {
        Self {
            state: Mutex::new(InboxState {
                queue: VecDeque::new(),
                data_len: 0,
                data_capacity,
                closed: false,
            }),
            wake: Condvar::new(),
        }
    }

    pub(crate) fn send_control(&self, job: EngineJob) -> bool {
        debug_assert!(!job.is_data());
        let mut state = self.state.lock().expect("inbox mutex");
        if state.closed {
            return false;
        }
        state.queue.push_back(job);
        self.wake.notify_one();
        true
    }

    pub(crate) fn try_send_data(&self, job: EngineJob) -> bool {
        debug_assert!(job.is_data());
        let mut state = self.state.lock().expect("inbox mutex");
        if state.closed || state.data_len >= state.data_capacity {
            return false;
        }
        state.data_len += 1;
        state.queue.push_back(job);
        self.wake.notify_one();
        true
    }

    /// Block until a job arrives or stop/close is signalled. Strict FIFO.
    pub(crate) fn recv(&self, stop: &AtomicBool) -> Option<EngineJob> {
        let mut state = self.state.lock().expect("inbox mutex");
        loop {
            if state.closed || stop.load(Ordering::SeqCst) {
                return None;
            }
            if let Some(job) = state.queue.pop_front() {
                if job.is_data() {
                    state.data_len -= 1;
                }
                return Some(job);
            }
            state = self.wake.wait(state).expect("inbox mutex");
        }
    }

    /// Close admission and invalidate queued work. Returns
    /// `(control_dropped, data_dropped)`.
    pub(crate) fn close_and_drain(&self) -> (u64, u64) {
        let mut state = self.state.lock().expect("inbox mutex");
        state.closed = true;
        let mut control = 0u64;
        let mut data = 0u64;
        for job in state.queue.drain(..) {
            if job.is_data() {
                data += 1;
            } else {
                control += 1;
            }
        }
        state.data_len = 0;
        self.wake.notify_all();
        (control, data)
    }

    pub(crate) fn wake_all(&self) {
        self.wake.notify_all();
    }

    /// Snapshot the queued coherence-relevant inputs (observations and
    /// terminates) as small descriptors under one lock. The engine-thread
    /// emission gate classifies each as will-incorporate (blocks actuation)
    /// or will-drop (ignored): stale queued truth must never suppress an
    /// honest snapshot, and a drop never dispatches a successor - so
    /// blocking on it would stall convergence with no recovery. Reads only;
    /// the queue is untouched.
    pub(crate) fn coherence_pending(&self) -> Vec<CoherencePending> {
        let state = self.state.lock().expect("inbox mutex");
        state
            .queue
            .iter()
            .filter_map(|job| match job {
                EngineJob::Observation {
                    pid,
                    generation,
                    seq,
                    ..
                } => Some(CoherencePending::Observation {
                    pid: *pid,
                    generation: *generation,
                    seq: *seq,
                }),
                EngineJob::ScopedObservation {
                    pid,
                    generation,
                    seq,
                    snapshot,
                    ..
                } => Some(CoherencePending::ScopedObservation {
                    pid: *pid,
                    generation: *generation,
                    seq: *seq,
                    revision: snapshot.revision,
                }),
                EngineJob::Terminate { pid, generation } => Some(CoherencePending::Terminate {
                    pid: *pid,
                    generation: *generation,
                }),
                _ => None,
            })
            .collect()
    }
}

/// Test-only worker drain mark: one owning worker's end-of-pass proof that
/// every frame batch through `drained_rev` is terminally accounted (actuated
/// or fenced, including superseded, stale, redundant, and stop-fenced) for
/// the exact `generation`. `sends_attempted` counts every worker-to-engine
/// inbox send attempt so far (observations plus frame and focus completions,
/// including queue-full drops: a dropped attempt can never arrive late, so
/// send-count equality across two passes still proves quiescence).
/// `pending` names retained unsent observations the next pass will retry.
/// `terminal` marks the final pre-exit mark: the worker sends nothing after
/// it, so one engine barrier after a terminal mark settles the lifetime.
/// `dropped_unsent` is only meaningful with `terminal`: retained observations
/// abandoned at exit. They were never enqueued, so no engine tail exists for
/// them. The engine fence plus the two-mark equality check (or the terminal
/// short-circuit) turns these into causal settlement (see
/// [`crate::runtime::RuntimeHandle::wait_app_settled_for_test`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerDrainMark {
    pub(crate) generation: u64,
    pub(crate) drained_rev: u64,
    pub(crate) sends_attempted: u64,
    pub(crate) pending: bool,
    pub(crate) terminal: bool,
    pub(crate) dropped_unsent: bool,
}

/// Atomic outcome of one worker multiplex wait on a [`FrameSlot`]: a taken
/// batch with its revision, a quiet slice with the revision verifiably
/// observed empty, or teardown. `Quiet` carries the observed revision so the
/// worker never names a revision it did not see drained.
#[derive(Debug)]
pub(crate) enum FrameWait {
    Batch(u64, Vec<FrameCommand>),
    Quiet(u64),
    Stopped,
}

impl WorkerDrainMark {
    pub(crate) fn pass(
        generation: u64,
        drained_rev: u64,
        sends_attempted: u64,
        pending: bool,
    ) -> Self {
        Self {
            generation,
            drained_rev,
            sends_attempted,
            pending,
            terminal: false,
            dropped_unsent: false,
        }
    }

    pub(crate) fn terminal(
        generation: u64,
        drained_rev: u64,
        sends_attempted: u64,
        dropped_unsent: bool,
    ) -> Self {
        Self {
            generation,
            drained_rev,
            sends_attempted,
            pending: false,
            terminal: true,
            dropped_unsent,
        }
    }
}

/// Latest-wins frame slot for one app worker. `set_latest` overwrites the
/// whole pending desired set so rapid re-plans coalesce but never lose the
/// final desired state - including a whole EMPTY desired set, which supersedes
/// pending state when windows are removed/closed. Every overwrite bumps a
/// revision token so a worker holding a taken batch can drop commands the
/// newer desired state already superseded. Stopping drops the pending set
/// (queued work is invalidated on terminate).
///
/// Public so the scoped effect hook can name it; construction and mutation
/// stay crate-internal.
#[derive(Debug)]
pub struct FrameSlot {
    state: Mutex<FrameSlotState>,
    wake: Condvar,
    /// Test-only drain-mark gate: unset in production. Only terminal
    /// (pre-exit) marks record unarmed, so a teardown that precedes arming
    /// still proves settlement; ordinary passes early-return on one atomic
    /// load with no lock and no wake. Armed by
    /// [`crate::runtime::RuntimeHandle::wait_app_settled_for_test`] before
    /// waiting; marks never affect scheduling or policy.
    mark_armed: AtomicBool,
}

#[derive(Debug)]
struct FrameSlotState {
    latest: Option<Vec<FrameCommand>>,
    stopped: bool,
    rev: u64,
    mark_seq: u64,
    mark: Option<WorkerDrainMark>,
}

impl FrameSlot {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(FrameSlotState {
                latest: None,
                stopped: false,
                rev: 0,
                mark_seq: 0,
                mark: None,
            }),
            wake: Condvar::new(),
            mark_armed: AtomicBool::new(false),
        }
    }

    /// Overwrite the pending desired set. Returns `None` when stopped
    /// (caller counts the invalidation), else the overwritten command
    /// count for coalescing diagnostics.
    pub(crate) fn set_latest(&self, cmds: Vec<FrameCommand>) -> Option<usize> {
        let mut state = self.state.lock().expect("frame slot mutex");
        if state.stopped {
            return None;
        }
        let coalesced = state.latest.as_ref().map(Vec::len).unwrap_or(0);
        state.latest = Some(cmds);
        state.rev += 1;
        self.wake.notify_one();
        Some(coalesced)
    }

    /// Current revision token: bumped by every accepted `set_latest`.
    pub(crate) fn current_rev(&self) -> u64 {
        self.state.lock().expect("frame slot mutex").rev
    }

    /// Arm test-only drain marking (idempotent). Production never arms:
    /// unarmed passes cost one atomic load and never lock or wake.
    pub(crate) fn arm_drain_mark(&self) {
        self.mark_armed.store(true, Ordering::SeqCst);
    }

    /// Record one worker end-of-pass drain mark. Terminal (pre-exit) marks
    /// record unconditionally so a teardown that precedes arming still
    /// proves settlement; ordinary nonterminal marks early-return unarmed
    /// (one atomic load, no lock, no wake). Only armed passes wake.
    pub(crate) fn note_drain(&self, mark: WorkerDrainMark) {
        if !mark.terminal && !self.mark_armed.load(Ordering::SeqCst) {
            return;
        }
        let mut state = self.state.lock().expect("frame slot mutex");
        state.mark_seq += 1;
        state.mark = Some(mark);
        self.wake.notify_all();
    }

    /// Current mark sequence (baseline for [`FrameSlot::wait_drain`]).
    pub(crate) fn mark_seq(&self) -> u64 {
        self.state.lock().expect("frame slot mutex").mark_seq
    }

    /// Bounded event wait for a drain mark past `baseline_seq` naming
    /// `generation` drained through at least `want_rev`. Check-then-wait
    /// under the slot lock (no lost wake); never holds shared state.
    /// Returns the mark plus its sequence, or `None` on the deadline.
    pub(crate) fn wait_drain(
        &self,
        baseline_seq: u64,
        generation: u64,
        want_rev: u64,
        deadline: Instant,
    ) -> Option<(u64, WorkerDrainMark)> {
        let mut state = self.state.lock().expect("frame slot mutex");
        loop {
            if state.mark_seq > baseline_seq
                && let Some(mark) = state.mark
                && mark.generation == generation
                && mark.drained_rev >= want_rev
            {
                return Some((state.mark_seq, mark));
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self
                .wake
                .wait_timeout(state, deadline - now)
                .expect("frame slot mutex");
            state = guard;
        }
    }

    /// Baseline-independent terminal-mark read: the recorded final mark for
    /// `generation` drained through at least `want_rev`, if any. A slot has
    /// at most one teardown, so a terminal mark is never stale: when the
    /// worker parked in teardown before the caller read its baseline, this
    /// (plus one engine barrier) still settles the lifetime.
    pub(crate) fn terminal_mark(
        &self,
        generation: u64,
        want_rev: u64,
    ) -> Option<(u64, WorkerDrainMark)> {
        let state = self.state.lock().expect("frame slot mutex");
        if let Some(mark) = state.mark
            && mark.generation == generation
            && mark.terminal
            && mark.drained_rev >= want_rev
        {
            return Some((state.mark_seq, mark));
        }
        None
    }

    /// Blocking take used by unit tests; production workers multiplex with
    /// [`FrameSlot::wait_batch_or_quiet`].
    #[cfg_attr(not(test), allow(dead_code))]
    fn take_versioned(&self, stop: &AtomicBool) -> Option<(u64, Vec<FrameCommand>)> {
        let mut state = self.state.lock().expect("frame slot mutex");
        loop {
            if state.stopped || stop.load(Ordering::SeqCst) {
                return None;
            }
            if let Some(cmds) = state.latest.take() {
                return Some((state.rev, cmds));
            }
            state = self.wake.wait(state).expect("frame slot mutex");
        }
    }

    /// Bounded multiplex wait with an atomic observation: waits at most
    /// `timeout` for a pending set, then reports what this call genuinely
    /// saw under one guard. `Quiet(rev)` names a revision observed empty
    /// with the lock held from wake to observation, so no `set_latest` can
    /// land between the wait and the read (a torn separate `current_rev`
    /// could name a queued-but-untaken revision). Command wake is still
    /// immediate (`set_latest` notifies with `notify_one`); the bound only
    /// caps observation cadence and stop latency. A stolen wakeup only
    /// delays the worker until its own timeout: every waiter here is
    /// predicate-gated, so a missed notification can delay but never
    /// falsely satisfy a drain wait.
    pub(crate) fn wait_batch_or_quiet(&self, stop: &AtomicBool, timeout: Duration) -> FrameWait {
        let mut state = self.state.lock().expect("frame slot mutex");
        if state.stopped || stop.load(Ordering::SeqCst) {
            return FrameWait::Stopped;
        }
        if let Some(cmds) = state.latest.take() {
            return FrameWait::Batch(state.rev, cmds);
        }
        let (guard, _) = self
            .wake
            .wait_timeout(state, timeout)
            .expect("frame slot mutex");
        state = guard;
        if state.stopped || stop.load(Ordering::SeqCst) {
            return FrameWait::Stopped;
        }
        if let Some(cmds) = state.latest.take() {
            return FrameWait::Batch(state.rev, cmds);
        }
        FrameWait::Quiet(state.rev)
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.state.lock().expect("frame slot mutex").stopped
    }

    pub(crate) fn stop(&self) {
        let mut state = self.state.lock().expect("frame slot mutex");
        state.stopped = true;
        state.latest = None;
        self.wake.notify_all();
    }
}

/// Latest-wins focus slot for one app worker: at most one pending focus
/// command (a newer desired focus supersedes - refocusing an already
/// superseded target would fight the plan). Stopping drops the pending
/// command (queued focus is invalidated on terminate). Unlike [`FrameSlot`]
/// there is no revision token or blocking take: the worker polls once per
/// multiplex iteration (bounded cadence), which also bounds focus latency
/// without a second condvar to multiplex.
pub(crate) struct FocusSlot {
    state: Mutex<FocusSlotState>,
}

struct FocusSlotState {
    latest: Option<FocusCommand>,
    stopped: bool,
}

impl FocusSlot {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(FocusSlotState {
                latest: None,
                stopped: false,
            }),
        }
    }

    /// Overwrite the pending focus command. Returns `None` when stopped
    /// (caller counts the invalidation).
    pub(crate) fn set_latest(&self, cmd: FocusCommand) -> Option<()> {
        let mut state = self.state.lock().expect("focus slot mutex");
        if state.stopped {
            return None;
        }
        state.latest = Some(cmd);
        Some(())
    }

    /// Take the pending command for dispatch, if any.
    pub(crate) fn take(&self) -> Option<FocusCommand> {
        let mut state = self.state.lock().expect("focus slot mutex");
        if state.stopped {
            return None;
        }
        state.latest.take()
    }

    pub(crate) fn stop(&self) {
        let mut state = self.state.lock().expect("focus slot mutex");
        state.stopped = true;
        state.latest = None;
    }
}

use crate::runtime_core::{RetainedApp, RetainedCore};
use crate::runtime_native::handle_scoped_observation;

// -- Engine event wire (world route) -------------------------------------------

/// Store one admitted select claim under its world operation identity.
/// The claim reaches the main-thread drain with the stable `op-<seq>` id
/// plus the reply-derived target plan (logical target, live native
/// binding or explicit creation desire, observation-time baseline) so the
/// drain actuates exactly the core-resolved target and reports its proven
/// native outcome back under the same id. `None` (duplicate path) stores
/// the claim unbound: the drain refuses without actuation. Every store —
/// bound or unbound — appends to the shared claim queue in admission
/// order and sets the shared claim wake (`select_claim_pending`): even a
/// refusal needs its drain-site settlement, so the main pump reconciles on
/// the event instead of the next 250 ms tick. Refusal paths that store
/// nothing set nothing. The queue is the drain order: the main thread
/// pops the front only when no drain is in progress and releases it after
/// the drain settles (see [`claim_select_for_drain`](crate::runtime::RuntimeHandle::claim_select_for_drain)),
/// so a burst of admitted commands drains oldest-first with no loss, no
/// duplication, and no reordering.
pub(crate) fn store_select_claim(
    shared: &Arc<Mutex<Shared>>,
    claim: crate::runtime_types::ClaimedSelect,
    operation_id: Option<&str>,
) {
    let mut stored = claim;
    stored.operation_id = operation_id.map(str::to_owned);
    // One pending queue for both lanes (the send lane shares the select
    // slot): send-shaped claims enqueue under their own token so the ring
    // distinguishes the lanes. Numbered sends enqueue as sends regardless
    // of follow intent (stays transfer too); everything else enqueues as
    // selects.
    let token = if matches!(
        stored.action,
        crate::runtime_types::InputSemanticAction::WorkspaceSend { .. }
    ) {
        "send-enqueued"
    } else {
        "select-enqueued"
    };
    let mut guard = lock_shared(shared);
    guard.select_pending.push_back(stored);
    // Depth rides the count (numbers only, never content): a burst that
    // queues behind an in-progress drain is observable as queued depth
    // instead of silent, while immediate (depth-1) stores read delivered.
    let depth = guard.select_pending.len() as u64;
    guard.select_claim_pending = true;
    // Flag-set then signal (race-closed with the main park: the main
    // thread re-takes the sticky flag after every park return, so a parked
    // main returns immediately here instead of waiting out the cadence).
    let wake = guard.main_wake.clone();
    guard.diagnostics.input_select_enqueued += 1;
    drop(guard);
    if let Some(wake) = wake {
        wake.signal();
    }
    emit(shared, 0, 0, 0, "input", token, depth);
}

/// Bind the reply-derived select plan into a claim for one operation id:
/// the retained view target (output plus logical workspace) from the
/// engine, the live native binding for that logical from the accepted
/// reply's workspace rows (live lookup only, never a durable table), the
/// explicit trailing creation desire for that logical when the target is
/// unbound, and the observation-time native current for the target output
/// as the attempt baseline. Missing transfers leave every plan field
/// `None` so the drain refuses without actuation (a prior no-op such as
/// previous-without-history binds nothing). Never invents a binding and
/// never reinterprets ordinals: numbered taps resolve core-side.
/// Call only with the currently applied reply's operation identity - never
/// on a sequencer-stale path, where `last_reply` is not current.
pub(crate) fn bind_select_plan(
    session: &mut WorldSession,
    ctx: &Arc<crate::runtime_types::NativeContext>,
    claim: &mut crate::runtime_types::ClaimedSelect,
    operation_id: &str,
) {
    let transfer = session
        .engine_mut()
        .retained_transfers()
        .into_iter()
        .find(|target| {
            target.operation_id == operation_id
                && matches!(target.kind, tiler_core::world_report::TransferKind::View)
        });
    let Some(transfer) = transfer else {
        return;
    };
    let target_output = transfer.target_output.0.clone();
    let target_workspace = transfer.target_workspace.clone();
    let reply = session.sequencer().last_reply();
    let native_binding = reply.as_ref().and_then(|reply| {
        reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == target_output && row.id == target_workspace)
            .and_then(|row| row.native_binding.clone())
    });
    let creation = reply.as_ref().and_then(|reply| {
        reply.native_creations.iter().find(|desire| {
            desire.target_output.0 == target_output && desire.logical_workspace == target_workspace
        })
    });
    let baseline_sid = ctx
        .domains
        .iter()
        .find(|domain| domain.output.0 == target_output && domain.current)
        .map(|domain| domain.space);
    claim.select_target_output = Some(target_output.clone());
    claim.select_target_workspace = Some(target_workspace.clone());
    claim.select_native_binding = native_binding;
    if let Some(desire) = creation {
        claim.select_creation_operation = Some(desire.operation_id.clone());
        claim.select_creation_logical = Some(desire.logical_workspace.clone());
        claim.select_creation_output = Some(desire.target_output.0.clone());
    }
    claim.select_baseline_sid = baseline_sid;
}

/// Parse one `space-<sid>` reply native binding to its live SID (`None` on
/// any malformed, zero, or unparsable binding - never guessed). The drain
/// converts the bound SID to the current live Desktop-N ordinal at dispatch
/// (the native mechanism posts a Desktop-N chord); targeting itself never
/// resolves from an input index.
fn parse_send_binding(binding: &str) -> Option<u64> {
    binding
        .strip_prefix("space-")?
        .parse::<u64>()
        .ok()
        .filter(|sid| *sid != 0)
}

/// Bind the reply-derived send plan into a claim for one operation id: the
/// retained window transfer target (output plus logical workspace) from the
/// the explicit creation desire for that logical when the target is
/// unbound, the follow view flight identity for `follow=true` transfers,
/// the reply-bound target SID as `expected_sid` for the pinned-target
/// owner, and the observation-time native current for the target output as
/// the attempt baseline. Missing transfers leave every plan field `None` so
/// the drain refuses without actuation. Unbound targets with no creation
/// desire bind no SID (missing bindings never create); unbound targets
/// WITH a desire bind the desire but no SID, and the drain refuses them
/// pending the desire-gated native creation seam (prerequisite blocker,
/// not a settled refusal). Never reinterprets ordinals: the numbered index
/// resolved core-side from the dispatched `SendNumbered` command.
/// Call only with the currently applied reply's operation identity - never
/// on a sequencer-stale path, where `last_reply` is not current.
pub(crate) fn bind_send_plan(
    session: &mut WorldSession,
    ctx: &Arc<crate::runtime_types::NativeContext>,
    claim: &mut crate::runtime_types::ClaimedSelect,
    operation_id: &str,
) {
    let transfer = session
        .engine_mut()
        .retained_transfers()
        .into_iter()
        .find(|target| {
            target.operation_id == operation_id
                && matches!(
                    target.kind,
                    tiler_core::world_report::TransferKind::Window(_)
                )
        });
    let Some(transfer) = transfer else {
        return;
    };
    let target_output = transfer.target_output.0.clone();
    let target_workspace = transfer.target_workspace.clone();
    let reply = session.sequencer().last_reply();
    let native_binding = reply.as_ref().and_then(|reply| {
        reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == target_output && row.id == target_workspace)
            .and_then(|row| row.native_binding.clone())
    });
    let creation = reply.as_ref().and_then(|reply| {
        reply.native_creations.iter().find(|desire| {
            desire.target_output.0 == target_output && desire.logical_workspace == target_workspace
        })
    });
    let baseline_sid = ctx
        .domains
        .iter()
        .find(|domain| domain.output.0 == target_output && domain.current)
        .map(|domain| domain.space);
    claim.select_target_output = Some(target_output.clone());
    claim.select_target_workspace = Some(target_workspace.clone());
    claim.select_native_binding = native_binding.clone();
    if let Some(desire) = creation {
        claim.select_creation_operation = Some(desire.operation_id.clone());
        claim.select_creation_logical = Some(desire.logical_workspace.clone());
        claim.select_creation_output = Some(desire.target_output.0.clone());
    }
    claim.select_baseline_sid = baseline_sid;
    // Follow view flight identity, read from the reply's own published
    // transfers: the retained View flight naming the same target as the
    // parent window transfer. The drain stages the follow view attempt
    // under exactly this identity; when the reply publishes no distinct
    // view row (stays, non-follow), nothing is staged and nothing is
    // invented.
    claim.send_follow_view_operation = reply.as_ref().and_then(|reply| {
        reply
            .transfers
            .iter()
            .find(|flight| {
                matches!(flight.kind, tiler_core::world_report::TransferKind::View)
                    && flight.target_output.0 == target_output
                    && flight.target_workspace == target_workspace
                    && flight.operation_id != operation_id
            })
            .map(|flight| flight.operation_id.clone())
    });
    // Reply-bound target SID for the pinned-target owner: parsed from the
    // live binding only. Unbound targets bind no SID even under a creation
    // desire - the desire-gated creation seam is a prerequisite blocker, so
    // the drain refuses as unbound instead of guessing.
    claim.expected_sid = native_binding.as_deref().and_then(parse_send_binding);
}

/// Pure engine admission verdict for one armed select claim (pure
/// unit-testable core of the product handoff): unarmed actions refuse
/// (trailing stay-0, unsupported sends,
/// outputs), then a missing native context refuses narrowly. There is
/// deliberately no busy or already-pending refusal: every admitted command
/// dispatches to the core in engine-FIFO order and queues its drain claim
/// behind any in-progress drain, so a busy lane delays actuation instead
/// of dropping intent (stale pins still refuse at the drain per T3).
/// Concurrent presses queue behind the in-progress drain instead of
/// overlapping, and a sent press is never replayed. Returns the claim to
/// store on success. Numbered taps bind at
/// dispatch (admission-time SID plus live revision); indicator clicks ride
/// their immutable render-time binding through to the drain; relative and
/// previous taps ride binding-free.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn admit_select_claim(
    native_present: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    admit_select_claim_with(
        native_present,
        claim,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit admission verdict: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local). Trailing `index 0` admits only when enabled.
pub(crate) fn admit_select_claim_with(
    native_present: bool,
    claim: crate::runtime_types::ClaimedSelect,
    trailing_enabled: bool,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    use crate::workspace_input::{is_armed_select_with, workspace_unsupported_outcome_with};
    if workspace_unsupported_outcome_with(&claim.action, trailing_enabled).is_some() {
        return Err("refused-select-unarmed");
    }
    if !is_armed_select_with(&claim.action, trailing_enabled) {
        return Err("refused-select-unarmed");
    }
    if !native_present {
        return Err("refused-select-no-topology");
    }
    Ok(claim)
}

/// Pure engine admission verdict for one send-shaped claim (pure
/// unit-testable core of the product send handoff). Unsupported send
/// shapes (relative sends, trailing stay-0) refuse; trailing send-0
/// follow admits through the capability gate; armed
/// send-follow 0..9 plus stay 1..9 refuses without native context only —
/// there is deliberately no busy or already-pending refusal (see
/// [`admit_select_claim`]: admitted commands queue behind the drain).
/// Returns the claim to
/// bind (target SID, revision, subject) on success. The shape check is
/// [`is_armed_send`] (single source with the catalogue `implemented`
/// flags). Test-only shim over [`admit_send_claim_with`]: capability-explicit
/// tests call the `_with` variant directly with a local `trailing_enabled`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn admit_send_claim(
    native_present: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    admit_send_claim_with(
        native_present,
        claim,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit send admission: the production route above with
/// `trailing_enabled` threaded from the single capability gate. Trailing
/// send-0 follow admits only when enabled; stay-0 never admits (out of
/// scope, no approved form).
pub(crate) fn admit_send_claim_with(
    native_present: bool,
    claim: crate::runtime_types::ClaimedSelect,
    trailing_enabled: bool,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    use crate::workspace_input::is_armed_send_with;
    if !is_armed_send_with(&claim.action, trailing_enabled) {
        return Err("refused-send-unarmed");
    }
    if !native_present {
        return Err("refused-send-no-topology");
    }
    Ok(claim)
}

/// Refuse one select claim narrowly (counted, never dispatched, never
/// replayed). Shared by tap and indicator lanes.
fn refuse_select_claim(shared: &Arc<Mutex<Shared>>, token: &'static str) {
    lock_shared(shared).diagnostics.input_select_refused += 1;
    emit(shared, 0, 0, 0, "input", token, 0);
}

/// Requester-correlated transient reply for one Meta+left gesture
/// ingress: the read-only preview outcome plus the incorporated terminal,
/// if any (committed only on a real core move/resize commit).
pub struct DragGestureReply {
    pub preview: crate::drag_drop::PreviewOutcome,
    pub settled: Option<tiler_core::gesture::GestureSettled>,
}

/// Engine-side Meta+left gesture handoff (engine thread only): fence the
/// press-proven subject against retained truth, then dispatch one gesture
/// report through the world gesture lifecycle and return the correlated
/// transient reply. Unknown-window and hidden-domain subjects cancel dead
/// (the main thread already fenced focus/scope/revision before sending,
/// so these legs are backstops, never the primary gate); a drifted press
/// revision refuses stale (never a late commit, never a replay).
/// Synthetic mode (no native context) resolves over the skeleton report.
#[allow(clippy::too_many_arguments)]
fn handle_drag_gesture(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    ingress: tiler_core::gesture::GestureIngress,
    press_revision: Option<u64>,
) -> DragGestureReply {
    use crate::drag_drop::PreviewOutcome;
    let window = ingress.window.0.clone();
    let dispatch = dispatch_context(shared, session);
    if session.is_native() && lock_shared(shared).native_context.is_some() {
        let Some(key) = retained.window_domains.get(&window).cloned() else {
            return DragGestureReply {
                preview: PreviewOutcome::CancelDead {
                    reason: "unknown-window",
                },
                settled: None,
            };
        };
        let current = lock_shared(shared)
            .native_context
            .clone()
            .is_some_and(|ctx| ctx.is_current(&key));
        if !current {
            return DragGestureReply {
                preview: PreviewOutcome::CancelDead {
                    reason: "hidden-domain",
                },
                settled: None,
            };
        }
        if let Some(press_revision) = press_revision
            && press_revision != retained.native_revision
        {
            return DragGestureReply {
                preview: PreviewOutcome::CancelDead {
                    reason: "stale-press",
                },
                settled: None,
            };
        }
        let live = retained
            .window_domains
            .get(&window)
            .is_some_and(|k| k == &key);
        if !live {
            return DragGestureReply {
                preview: PreviewOutcome::CancelDead {
                    reason: "closed-target",
                },
                settled: None,
            };
        }
    } else if let Some(press_revision) = press_revision
        && press_revision != retained.native_revision
    {
        return DragGestureReply {
            preview: PreviewOutcome::CancelDead {
                reason: "stale-press",
            },
            settled: None,
        };
    }
    {
        let mut guard = lock_shared(shared);
        guard.diagnostics.input_forwarded += 1;
        guard.notify_test_waiters();
    }
    emit(shared, 0, 0, 0, "input", OP_DRAG_GESTURE, 0);
    let (_verdict, preview, settled) = dispatch_gesture_report(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        dispatch,
        ingress,
        OP_DRAG_GESTURE,
        0,
        0,
        0,
    );
    DragGestureReply { preview, settled }
}

// -- Thread bodies -------------------------------------------------------------

/// Engine owner: the sole owner of the [`WorldSession`] (retained world
/// engine plus adapter-side sequencing) and retained observation state.
/// Never waits for AX; frame emission is a non-blocking slot overwrite
/// and focus emission is a bounded outbox push.
pub(crate) fn engine_main(
    shared: Arc<Mutex<Shared>>,
    inbox: Arc<Inbox>,
    focus_queue: Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<()>,
) -> EngineExit {
    // The watermark clock is the shared
    // input mark (submitters advance it; dispatches only stamp it).
    let (native_mode, focus_route, clock, core_settings) = {
        let guard = lock_shared(&shared);
        (
            matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            ),
            guard.config.focus_route,
            Arc::clone(&guard.clock),
            guard.config.core_settings.clone(),
        )
    };
    let mut session = WorldSession::new(native_mode, focus_route, clock, &core_settings);
    let reported = session
        .initial_settings_reply()
        .map(crate::world_connection::format_setting_errors)
        .unwrap_or_default();
    if !reported.is_empty() {
        eprintln!("settings: core diagnostics: {reported}");
    }
    // Publish the effective core-owned border numbers to the shared
    // presentation carrier before signalling readiness, so every dispatch
    // sync reads core-effective values (never the adapter-resolved style).
    {
        let style = session.core_border_presentation();
        lock_shared(&shared).presentation_style = style;
    }
    let mut retained = RetainedCore::default();
    let _ = ready_tx.send(());
    loop {
        let Some(job) = inbox.recv(&stop) else {
            break;
        };
        {
            let mut guard = lock_shared(&shared);
            guard.diagnostics.engine_jobs_processed += 1;
        }
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            focus_capacity,
        );
    }
    // An exit that was never asked for (inbox closed without stop) is
    // observable evidence, not a silent idle end: counted, logged, and
    // later folded into the shutdown outcome (unclean, never clean).
    if !stop.load(Ordering::SeqCst) {
        lock_shared(&shared).diagnostics.unexpected_eos += 1;
        emit(&shared, 0, 0, 0, "engine", "eos-unexpected", 0);
    }
    EngineExit::Done
}

/// Read-only retained occupancy report for the menu-bar indicator (engine
/// owner only): per known domain (current and hidden alike) the
/// exactly-homed retained `(pid, wid)` members, sorted, plus the context
/// presence/revision the homing was derived under. No planning, no
/// staging, no reconciler writes - a pure read over retained state plus
/// the shared context flags. Memberless known domains ride with empty
/// membership (honestly empty only when the caller proves completeness by
/// matching `context_revision` against its live snapshot revision).
fn occupancy_report(
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
) -> crate::runtime::WorkspaceOccupancyReport {
    use crate::runtime::{WorkspaceOccupancyDomain, WorkspaceOccupancyReport};
    let members = crate::runtime_core::retained_occupancy(retained);
    let guard = lock_shared(shared);
    let Some(ctx) = guard.native_context.clone() else {
        return WorkspaceOccupancyReport {
            context_present: false,
            context_revision: guard.topology_revision,
            domains: Vec::new(),
        };
    };
    let revision = ctx.revision;
    let mut domains: Vec<WorkspaceOccupancyDomain> = members
        .into_iter()
        .map(|(key, members)| WorkspaceOccupancyDomain {
            workspace: key.workspace.0.clone(),
            current: ctx.is_current(&key),
            members,
        })
        .collect();
    domains.sort_by(|a, b| a.workspace.cmp(&b.workspace));
    WorkspaceOccupancyReport {
        context_present: true,
        context_revision: revision,
        domains,
    }
}

/// Shared dispatch-truth accounting for [`EngineJob::AxResult`] and
/// [`EngineJob::FocusResult`]: completion tokens counted plus generation
/// freshness fenced, so late results from dead lifetimes never pollute.
/// `kind` names the ring lane (`ax-result` for frames, `focus-result` for
/// owner-dispatched focus).
fn note_completion(
    shared: &Arc<Mutex<Shared>>,
    kind: &'static str,
    pid: u32,
    generation: u64,
    cmd_id: u64,
    completion: &AxCompletion,
) {
    // Preserve dispatch detail without leaking strings: Applied and
    // Timeout carry elapsed millis in the count; Failed maps its
    // reason to a fixed sanitized token.
    let (token, count): (&'static str, u64) = match completion {
        AxCompletion::Applied { elapsed_ms } => ("applied", *elapsed_ms),
        AxCompletion::PartialApplied { elapsed_ms, .. } => ("partial-applied", *elapsed_ms),
        AxCompletion::Failed { reason } => (sanitize_backend_reason(reason), 0),
        AxCompletion::Timeout { timeout_ms } => ("timeout", *timeout_ms),
    };
    lock_shared(shared).count_completion(token);
    let valid = lock_shared(shared)
        .apps
        .get(&pid)
        .is_some_and(|r| r.generation == generation && !r.terminated);
    if valid {
        // Honest completion only: backend dispatch truth is logged
        // here and never substitutes observed truth, which arrives
        // exclusively through observations.
        emit(shared, pid, generation, cmd_id, kind, token, count);
    } else {
        let terminated = lock_shared(shared)
            .apps
            .get(&pid)
            .is_some_and(|r| r.terminated || r.generation != generation);
        if terminated {
            lock_shared(shared).diagnostics.terminated_invalidated += 1;
            emit(
                shared,
                pid,
                generation,
                cmd_id,
                kind,
                "terminated-invalidated",
                0,
            );
        } else {
            lock_shared(shared).diagnostics.stale_dropped += 1;
            emit(shared, pid, generation, cmd_id, kind, "stale-rejected", 0);
        }
    }
}

pub(crate) fn process_job(
    job: EngineJob,
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
) {
    use crate::workspace_input::{is_armed_select, workspace_unsupported_outcome};
    match job {
        EngineJob::Launch { pid, generation } => {
            let valid = {
                let guard = lock_shared(shared);
                guard
                    .apps
                    .get(&pid)
                    .is_some_and(|r| r.generation == generation && !r.terminated)
            };
            if valid {
                let fresh = match retained.apps.get(&pid) {
                    Some(app) => app.generation != generation,
                    None => true,
                };
                if fresh {
                    // Never reuse old-lifetime core state: a new generation
                    // starts with empty retained state, and observed focus
                    // into this PID is authoritatively lost until observed.
                    // Homing, minimized lifetimes, and write correlation
                    // from the dead lifetime drop with it. Stale pending
                    // closes for the old lifetime drop too, or a reused
                    // same-PID/WID would falsely close the new lifetime.
                    retained.apps.insert(
                        pid,
                        RetainedApp {
                            generation,
                            seq_last: 0,
                            cmd_counter: 0,
                            windows: Vec::new(),
                            focused_wid: None,
                            roster: std::collections::BTreeMap::new(),
                            roster_incomplete: false,
                            roster_cause: String::new(),
                        },
                    );
                    forget_world_members(session, retained, pid);
                    let prefix = format!("{pid}_");
                    session
                        .pending_closes
                        .retain(|id| !id.0.starts_with(&prefix));
                }
                emit(shared, pid, generation, 0, "launch", "registered", 0);
            } else {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(shared, pid, generation, 0, "launch", "stale-rejected", 0);
            }
        }
        EngineJob::Terminate { pid, generation } => {
            // Fence PID reuse: a stale terminate from an old lifetime must
            // not close new-lifetime core state.
            let current = retained.apps.get(&pid).map(|app| app.generation);
            if current.is_some_and(|current| current != generation) {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(shared, pid, generation, 0, "terminate", "stale-rejected", 0);
                return;
            }
            let removed = retained.apps.remove(&pid);
            let (closed, roster_ids): (Vec<WindowId>, Vec<WindowId>) = removed
                .map(|app| {
                    let members: Vec<WindowId> = app
                        .windows
                        .iter()
                        .map(|w| WindowId(window_id(pid, w.wid)))
                        .collect();
                    let roster: Vec<WindowId> = app
                        .roster
                        .keys()
                        .map(|wid| WindowId(window_id(pid, *wid)))
                        .collect();
                    (members, roster)
                })
                .unwrap_or_default();
            // Flat close authority: every previously known roster id plus
            // minimized entries, no homing needed. Roster ids ride from the
            // removed app (already taken), minimized from retained state.
            let mut flat_set: std::collections::BTreeSet<WindowId> =
                std::collections::BTreeSet::new();
            for id in closed.iter().chain(roster_ids.iter()) {
                flat_set.insert(id.clone());
            }
            let prefix = format!("{pid}_");
            for (id, _) in retained
                .minimized
                .iter()
                .filter(|(id, _)| id.starts_with(&prefix))
            {
                flat_set.insert(WindowId(id.clone()));
            }
            let flat: Vec<WindowId> = flat_set.into_iter().collect();
            let flat_count = flat.len() as u64;
            for id in &flat {
                session.pending_closes.insert(id.clone());
            }
            // Forget homing/minimized/correlation before dispatch so the
            // closed windows ride as explicit closes, never members.
            forget_world_members(session, retained, pid);
            emit(
                shared,
                pid,
                generation,
                0,
                "terminate",
                "retained-removed",
                flat_count,
            ); // Reconcile the survivors so unaffected apps stay operational.
            // Flat ids ride explicitly on both routes; synthetic included.
            let dispatch = dispatch_context(shared, session);
            dispatch_report(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                dispatch,
                flat.clone(),
                None,
                None,
                false,
                OP_TERMINATE,
                pid,
                generation,
                0,
            );
        }
        EngineJob::Observation {
            pid,
            generation,
            seq,
            windows,
            focused_wid,
        } => {
            let valid = {
                let guard = lock_shared(shared);
                guard
                    .apps
                    .get(&pid)
                    .is_some_and(|r| r.generation == generation && !r.terminated)
            };
            if !valid {
                let terminated = lock_shared(shared)
                    .apps
                    .get(&pid)
                    .is_some_and(|r| r.terminated || r.generation != generation);
                if terminated {
                    lock_shared(shared).diagnostics.terminated_invalidated += 1;
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "observation",
                        "terminated-invalidated",
                        0,
                    );
                } else {
                    lock_shared(shared).diagnostics.stale_dropped += 1;
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "observation",
                        "stale-rejected",
                        0,
                    );
                }
                return;
            }
            // Per-app sequence fence with generation reset on PID reuse.
            let fresh = match retained.apps.get(&pid) {
                Some(app) if app.generation == generation => seq > app.seq_last,
                _ => true,
            };
            if !fresh {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(
                    shared,
                    pid,
                    generation,
                    seq,
                    "observation",
                    "out-of-order-rejected",
                    0,
                );
                return;
            }
            let entry = retained.apps.entry(pid).or_insert(RetainedApp {
                generation,
                seq_last: 0,
                cmd_counter: 0,
                windows: Vec::new(),
                focused_wid: None,
                roster: std::collections::BTreeMap::new(),
                roster_incomplete: false,
                roster_cause: String::new(),
            });
            if entry.generation != generation {
                entry.generation = generation;
                entry.seq_last = 0;
                entry.cmd_counter = 0;
            }
            entry.seq_last = seq;
            entry.windows = windows;
            entry.focused_wid = focused_wid;
            // Observed truth only: focus is set when the reported window is
            // present, and cleared when this app no longer reports it
            // (authoritative loss). Observations from other apps never move
            // focus out from under their owner.
            let observed_target = match focused_wid {
                Some(wid) if entry.windows.iter().any(|w| w.wid == wid) => {
                    Some(WindowId(window_id(pid, wid)))
                }
                _ => None,
            };
            let window_count = entry.windows.len() as u64;
            let homed_windows = entry.windows.clone();
            match observed_target {
                Some(window) => {
                    retained.observed_focus = Some(window);
                    retained.observed_focus_known = true;
                }
                None => crate::runtime_core::clear_focus_into(retained, pid),
            }
            // Synthetic homing: every retained window belongs to the
            // skeleton domain (legacy observations carry no Space
            // membership), so reports home, focus, and close exactly.
            crate::runtime_core::note_synthetic_homing(retained, pid, &homed_windows);
            emit(
                shared,
                pid,
                generation,
                seq,
                "observation",
                "applied",
                window_count,
            );
            session.clear_coverage_gap();
            // Legacy (metadataless) observations carry no Space membership:
            // the skeleton report, exactly as the legacy skeleton event.
            dispatch_report(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                DispatchContext::Synthetic,
                Vec::new(),
                None,
                None,
                false,
                OP_RECONCILE,
                pid,
                generation,
                seq,
            );
        }
        EngineJob::ScopedObservation {
            pid,
            generation,
            seq,
            snapshot,
            trace_id,
            intake_us,
            enqueue_us,
        } => {
            // Host-change correlation: the worker minted the trace at
            // poll-result receipt (intake), so the engine span rejoins it
            // across the inbox queue exactly like the input-tap path.
            // `trace_id` 0 (disabled or manual submit) skips every record
            // below after one flag load each. The INTAKE record is the
            // notification-receipt marker (host-change token, count always
            // 0); QUEUE is pure enqueue-to-dequeue wait. Poll acquisition
            // itself (AX/CG reads inside the poll) is NOT clocked here: the
            // worker accumulation window cannot attribute those reads to the
            // poll, so the host total runs poll-result receipt to dispatch
            // and excludes acquisition. The span
            // installs thread-locally so the synchronous reconcile below
            // records its phases under "host-change" (coalesced outcome:
            // the polled snapshot exposes no create/destroy type).
            let span = (trace_id != 0).then_some(crate::stage_timing::ActionSpan {
                id: trace_id,
                action: "host-change",
                t0_us: intake_us,
            });
            let _trace_guard = span.as_ref().map(|held| crate::stage_timing::enter(*held));
            if let Some(span) = span {
                crate::stage_timing::record(
                    shared,
                    span.id,
                    crate::stage_timing::stage::INTAKE,
                    span.action,
                    0,
                );
                crate::stage_timing::record(
                    shared,
                    span.id,
                    crate::stage_timing::stage::QUEUE,
                    span.action,
                    crate::stage_timing::now_us().saturating_sub(enqueue_us),
                );
            }
            handle_scoped_observation(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                pid,
                generation,
                seq,
                snapshot,
            );
        }
        EngineJob::InputAction {
            action,
            trace_id,
            intake_us,
            enqueue_us,
        } => {
            // Stage timing rejoins the intake stamp across the inbox queue:
            // `trace_id` 0 (disabled) skips every record below after one
            // flag load each. The span installs thread-locally so the
            // synchronous dispatch path records its phases under it. QUEUE
            // is pure inbox wait (enqueue to dequeue); receipt-to-enqueue
            // rides the input-side `timing-input-map` record.
            let span = (trace_id != 0).then(|| crate::stage_timing::ActionSpan {
                id: trace_id,
                action: crate::stage_timing::action_token(&action),
                t0_us: intake_us,
            });
            let _trace_guard = span.as_ref().map(|held| crate::stage_timing::enter(*held));
            let mapping_clock = crate::stage_timing::PhaseClock::start();
            if let Some(span) = span {
                crate::stage_timing::record(
                    shared,
                    span.id,
                    crate::stage_timing::stage::QUEUE,
                    span.action,
                    crate::stage_timing::now_us().saturating_sub(enqueue_us),
                );
            }
            // Unsupported workspace actions (trailing stay-0,
            // relative sends, outputs) are recognized but
            // undispatched: no unsupported transfer and no output-send
            // route runs from the product tile (see
            // `workspace_unsupported_reason` for the exact seam). Counted
            // and logged with a static token, never faked.
            if let Some(outcome) = workspace_unsupported_outcome(&action) {
                lock_shared(shared).diagnostics.input_refused_unsupported += 1;
                emit(shared, 0, 0, 0, "input", outcome, 0);
                crate::stage_timing::finish(shared, span, outcome);
                return;
            }
            // Armed select actions dispatch with no focused-window
            // subject (empty Spaces navigate): one planner-owned world
            // command plus one pending claim for the main-thread tile
            // drain. Targeting resolves core-side; the accepted reply
            // binds the drain plan.
            if is_armed_select(&action) {
                if let Some(span) = span {
                    crate::stage_timing::record(
                        shared,
                        span.id,
                        crate::stage_timing::stage::MAPPING,
                        span.action,
                        mapping_clock.elapsed(),
                    );
                }
                handle_select_action(
                    session,
                    retained,
                    shared,
                    inbox,
                    focus_queue,
                    focus_capacity,
                    crate::runtime_types::ClaimedSelect::tap(action),
                );
                crate::stage_timing::finish(shared, span, crate::stage_timing::outcome::DISPATCHED);
                return;
            }
            // Armed numbered send-follow 1..9 dispatches through the
            // managed-send lane (see [`admit_send_claim`],
            // [`handle_send_action`]).
            if crate::workspace_input::is_armed_send(&action) {
                if let Some(span) = span {
                    crate::stage_timing::record(
                        shared,
                        span.id,
                        crate::stage_timing::stage::MAPPING,
                        span.action,
                        mapping_clock.elapsed(),
                    );
                }
                handle_send_action(
                    session,
                    retained,
                    shared,
                    inbox,
                    focus_queue,
                    focus_capacity,
                    crate::runtime_types::ClaimedSelect::tap(action),
                );
                crate::stage_timing::finish(shared, span, crate::stage_timing::outcome::DISPATCHED);
                return;
            }
            // The subject is observed truth, never planned focus.
            let Some(window) = retained.observed_focus.clone() else {
                lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                emit(shared, 0, 0, 0, "input", "refused-no-focus", 0);
                crate::stage_timing::finish(shared, span, "refused-no-focus");
                return;
            };
            let Some((command, op)) = operation_command(&action, &window.0, retained) else {
                // Unarmed workspace actions refuse in the guard above and
                // armed selects enqueue in their lane too (sends refuse in
                // the guard as well), so this is unreachable (adding an
                // action variant fails the guard's exhaustive match first).
                unreachable!("workspace actions refuse before dispatch");
            };
            {
                let mut guard = lock_shared(shared);
                guard.diagnostics.input_forwarded += 1;
                guard.notify_test_waiters();
            }
            emit(shared, 0, 0, 0, "input", op, 0);
            // Native mode resolves the subject's homed domain: an unmapped
            // subject refuses instead of planning the skeleton, and a
            // hidden-domain subject never dispatches (no input writes or
            // focus moves into an off-current Space). Synthetic mode keeps
            // the skeleton report.
            let dispatch = dispatch_context(shared, session);
            if session.is_native() && lock_shared(shared).native_context.is_some() {
                let Some(key) = retained.window_domains.get(&window.0).cloned() else {
                    lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                    emit(shared, 0, 0, 0, "input", "refused-no-domain", 0);
                    crate::stage_timing::finish(shared, span, "refused-no-domain");
                    return;
                };
                let current = lock_shared(shared)
                    .native_context
                    .clone()
                    .is_some_and(|ctx| ctx.is_current(&key));
                if !current {
                    lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                    emit(shared, 0, 0, 0, "input", "refused-hidden-domain", 0);
                    crate::stage_timing::finish(shared, span, "refused-hidden-domain");
                    return;
                }
            }
            if let Some(span) = span {
                crate::stage_timing::record(
                    shared,
                    span.id,
                    crate::stage_timing::stage::MAPPING,
                    span.action,
                    mapping_clock.elapsed(),
                );
            }
            dispatch_report(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                dispatch,
                Vec::new(),
                Some(command),
                None,
                false,
                op,
                0,
                0,
                0,
            );
            crate::stage_timing::finish(shared, span, crate::stage_timing::outcome::DISPATCHED);
        }
        EngineJob::IndicatorSelect {
            action,
            expected_sid,
            ui_generation,
            topology_revision,
            trace_id,
            intake_us,
            enqueue_us,
        } => {
            // Stage timing rejoins the indicator intake stamp across the
            // queue exactly like the tap lane above (pure enqueue-to-dequeue
            // wait).
            let span = (trace_id != 0).then(|| crate::stage_timing::ActionSpan {
                id: trace_id,
                action: crate::stage_timing::action_token(&action),
                t0_us: intake_us,
            });
            let _trace_guard = span.as_ref().map(|held| crate::stage_timing::enter(*held));
            if let Some(span) = span {
                crate::stage_timing::record(
                    shared,
                    span.id,
                    crate::stage_timing::stage::QUEUE,
                    span.action,
                    crate::stage_timing::now_us().saturating_sub(enqueue_us),
                );
            }
            // Indicator segment clicks are select-only by construction:
            // the immutable render-time binding rides through admission
            // into the pending claim (see [`admit_select_claim`]);
            // anything unarmed refuses inside without touching the core.
            handle_select_action(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                crate::runtime_types::ClaimedSelect::indicator(
                    action,
                    expected_sid,
                    ui_generation,
                    topology_revision,
                ),
            );
            crate::stage_timing::finish(shared, span, crate::stage_timing::outcome::DISPATCHED);
        }
        EngineJob::ViewAttempt {
            operation_id,
            setter,
            pre_sid,
            post_sid,
            baseline_sid,
        } => {
            // Raw drain evidence staged for the next report (never a
            // verdict: the core derives confirmation, refusal, ambiguity,
            // and supersession). Unknown operations drop without evidence.
            // A follow send's view evidence arrives under the core-derived
            // `{op}-follow` identity (the retained follow view flight the
            // reply publishes); the standalone stage below correlates it
            // against that retained View target exactly like any other
            // view. The ring line makes staging observable end to end.
            session.stage_view_attempt(&operation_id, setter, pre_sid, post_sid, baseline_sid);
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
            // Drain settlement is the select per-action TOTAL (trace origin
            // to staging, including the lane's fixed waits unsplit).
            if crate::stage_timing::enabled()
                && let Some(op_seq) = crate::stage_timing::parse_op_seq(&operation_id)
                && let Some((trace, t0_us, _)) = crate::stage_timing::trace_for_op(op_seq)
            {
                crate::stage_timing::record(
                    shared,
                    trace,
                    crate::stage_timing::stage::OP_SETTLE,
                    "view-staged",
                    crate::stage_timing::now_us().saturating_sub(t0_us),
                );
            }
        }
        EngineJob::MemberAttempt {
            operation_id,
            setter,
            pre_sid,
            post_sid,
        } => {
            session.stage_member_attempt(&operation_id, setter, pre_sid, post_sid);
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
            // Drain settlement is the send per-action TOTAL (trace origin to
            // staging, including the drag fallback's fixed holds unsplit).
            if crate::stage_timing::enabled()
                && let Some(op_seq) = crate::stage_timing::parse_op_seq(&operation_id)
                && let Some((trace, t0_us, _)) = crate::stage_timing::trace_for_op(op_seq)
            {
                crate::stage_timing::record(
                    shared,
                    trace,
                    crate::stage_timing::stage::OP_SETTLE,
                    "member-staged",
                    crate::stage_timing::now_us().saturating_sub(t0_us),
                );
            }
        }
        EngineJob::NativeCreation {
            operation_id,
            logical,
            output,
            setter,
            order_before,
            order_after,
            created_sid,
            attempt_ms,
        } => {
            // Raw creation-leg evidence staged for the next report (never
            // a verdict: the core owns binding and settlement). One row
            // per desire op; exact correlation happens core-side.
            // The ring line makes staging observable end to end.
            session.stage_native_creation(
                &operation_id,
                &logical,
                output,
                setter,
                &order_before,
                &order_after,
                created_sid,
                attempt_ms,
            );
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
        }
        EngineJob::DragGesture {
            ingress,
            press_revision,
            reply,
        } => {
            // One gesture phase through the world gesture lifecycle
            // (Started/Updated/Released/Cancelled). Every refusal maps to
            // a pure reply - never a dropped reply - so the main thread
            // always unblocks by its timeout.
            let outcome = handle_drag_gesture(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                ingress,
                press_revision,
            );
            let _ = reply.send(outcome);
        }
        EngineJob::AxResult {
            pid,
            generation,
            cmd_id,
            completion,
        } => {
            note_completion(shared, "ax-result", pid, generation, cmd_id, &completion);
            // Frame settlement joins the dispatch trace through the bounded
            // cmd map (keyed by the full app lifetime plus `cmd_id`, so two
            // apps reusing one counter never join): `ref` is the trace, the
            // outcome is the honest completion class, `count` is the
            // per-action TOTAL (trace origin to result arrival). Unmapped
            // completions (synthetic traffic, evicted binds) emit nothing.
            // This is the action's native settlement edge - never the engine
            // `timing-done` record, which stops at dispatch return.
            if crate::stage_timing::enabled()
                && let Some((trace, t0_us, _)) =
                    crate::stage_timing::trace_for_cmd(pid, generation, cmd_id)
            {
                crate::stage_timing::record(
                    shared,
                    trace,
                    crate::stage_timing::stage::FRAME_SETTLE,
                    crate::stage_timing::completion_token(&completion),
                    crate::stage_timing::now_us().saturating_sub(t0_us),
                );
            }
            // A staged placement row changes the next report: advance the
            // shared mark now (the completion itself is already
            // incorporated). Ordinary writes stage nothing - no mark moves.
            if stage_write_feedback(session, retained, pid, generation, cmd_id, &completion) {
                crate::runtime::invalidate_clock(shared);
            }
        }
        EngineJob::FocusResult {
            pid,
            generation,
            cmd_id,
            completion,
        } => {
            note_completion(shared, "focus-result", pid, generation, cmd_id, &completion);
            // Focus settlement: same per-action TOTAL convention as frame
            // settle above (origin to arrival, never dispatch return).
            if crate::stage_timing::enabled()
                && let Some((trace, t0_us, _)) =
                    crate::stage_timing::trace_for_focus(pid, generation, cmd_id)
            {
                crate::stage_timing::record(
                    shared,
                    trace,
                    crate::stage_timing::stage::FOCUS_SETTLE,
                    crate::stage_timing::completion_token(&completion),
                    crate::stage_timing::now_us().saturating_sub(t0_us),
                );
            }
        }
        EngineJob::CloseFocusResult {
            pid,
            generation,
            cmd_id,
            close_id,
            target,
            issued_rev,
            adapter_incarnation,
            core_generation,
            setter,
            reason,
            pre,
            post,
            foreground_proven,
            arrival,
            attempt_ms,
        } => {
            // Narrowly typed close result: diagnostics plus the truthful
            // staged row under the original stamp. First-wins bounded;
            // later rows drop until the next dispatch drains. Diagnostics
            // mirror `note_completion` freshness without inventing a
            // backend completion: the setter names the token. The row
            // stages only for the exact still-current request: a result
            // whose lifetime died or whose identity no longer names the
            // current record (null/newer supersession, session alias) is
            // stale evidence, counted and never staged. The setter, reason,
            // and pre/post legs stage verbatim: raw facts only, never an
            // adapter verdict.
            let (token, count): (&'static str, u64) = match setter {
                tiler_core::world_report::VisibilitySetter::Accepted => ("applied", attempt_ms),
                tiler_core::world_report::VisibilitySetter::Refused => ("refused", 0),
                tiler_core::world_report::VisibilitySetter::Threw => ("threw", 0),
                tiler_core::world_report::VisibilitySetter::NotCalled => ("not-called", 0),
            };
            lock_shared(shared).count_completion(token);
            // Focus settlement: same per-action TOTAL convention as frame
            // settle and the legacy focus settle above (origin to arrival,
            // never dispatch return). Joins the dispatch bind under the
            // typed command identity (pid/generation/cmd_id); unmapped
            // completions emit nothing.
            if crate::stage_timing::enabled()
                && let Some((trace, t0_us, _)) =
                    crate::stage_timing::trace_for_focus(pid, generation, cmd_id)
            {
                crate::stage_timing::record(
                    shared,
                    trace,
                    crate::stage_timing::stage::FOCUS_SETTLE,
                    token,
                    crate::stage_timing::now_us().saturating_sub(t0_us),
                );
            }
            // Terminal owner completion (attempt or no-call alike): clear
            // only the exactly matching deferred intent — same request
            // tuple plus the exact queued command identity — before the
            // normal first-feedback staging below. Anything else stays
            // bound; a null/newer publication cancels through the dispatch
            // legs, never here.
            {
                let mut guard = lock_shared(shared);
                let matches = guard
                    .selection_focus_pending
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.focus_id == close_id
                            && pending.issued_revision == issued_rev
                            && pending.adapter_incarnation == adapter_incarnation
                            && pending.core_generation == core_generation
                            && pending.command_seq == cmd_id
                            && pending.window == target
                    });
                if matches {
                    guard.selection_focus_pending = None;
                }
            }
            // Terminal owner completion retires the exact outbox row (or
            // rows, when deferral recovery re-queued the same identity): the
            // terminal result ends the command either way, so a consumed row
            // must never keep occupying the bounded outbox. Runs on the
            // staged and the stale-rejected legs alike; feedback rides the
            // staged row, never the queue, so this drops no core input.
            crate::world_dispatch::retire_focus_request_exact(
                focus_queue,
                cmd_id,
                &target.0,
                &close_id,
            );
            let live = lock_shared(shared)
                .apps
                .get(&pid)
                .is_some_and(|r| r.generation == generation && !r.terminated);
            let current = session.sequencer().is_current_close_request(
                &close_id,
                &target,
                issued_rev,
                &adapter_incarnation,
                &core_generation,
            ) && crate::runtime_shared::close_focus_is_current(
                shared,
                &adapter_incarnation,
                &core_generation,
                &close_id,
                issued_rev,
            );
            if live && current {
                emit(
                    shared,
                    pid,
                    generation,
                    cmd_id,
                    "focus-result",
                    token,
                    count,
                );
                session.stage_close_feedback(tiler_core::world_report::FocusFeedback {
                    id: close_id,
                    target,
                    revision: issued_rev,
                    setter,
                    reason,
                    pre,
                    post,
                    foreground_proven,
                    arrival,
                    attempt_ms,
                });
                emit(shared, 0, 0, 0, "feedback", "staged", 1);
            } else {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(
                    shared,
                    pid,
                    generation,
                    cmd_id,
                    "focus-result",
                    "stale-rejected",
                    0,
                );
            }
        }
        EngineJob::SelectionFocusDeferred {
            pid,
            generation,
            focus_id,
            issued_revision,
            adapter_incarnation,
            core_generation,
            cmd_id,
            target,
        } => {
            // Owner pre-setter deferral: no setter ran, no feedback stages.
            // Accept only the exact still-current queued pending tuple and
            // app lifetime: reset the queue mark plus the stamp and retain
            // the original deps and baselines, so evidence refresh or engine
            // reevaluation can queue the same unstarted work again. No
            // setter is ever retried from here.
            let accept = {
                let guard = lock_shared(shared);
                guard
                    .selection_focus_pending
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.owner_queued
                            && pending.focus_id == focus_id
                            && pending.issued_revision == issued_revision
                            && pending.adapter_incarnation == adapter_incarnation
                            && pending.core_generation == core_generation
                            && pending.command_seq == cmd_id
                            && pending.window == target
                    })
                    && guard
                        .apps
                        .get(&pid)
                        .is_some_and(|record| record.generation == generation && !record.terminated)
            };
            if accept {
                let mut guard = lock_shared(shared);
                if let Some(pending) = guard.selection_focus_pending.as_mut() {
                    pending.owner_queued = false;
                    pending.proven_stamp = None;
                }
                drop(guard);
                emit(
                    shared,
                    pid,
                    generation,
                    cmd_id,
                    "focus",
                    "deferred-owner",
                    1,
                );
            } else {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(
                    shared,
                    pid,
                    generation,
                    cmd_id,
                    "focus-result",
                    "stale-rejected",
                    0,
                );
            }
        }
        EngineJob::ArrivalCaptureReady {
            pid,
            generation,
            focus_id,
            issued_revision,
            adapter_incarnation,
            core_generation,
            obs,
            revision,
            context_generation,
            captures,
        } => {
            // Evidence-only capture completion: validate the exact current
            // tuple, app lifetime, watermark, and revision, retain the
            // bounded per-member rows, then reevaluate the latest same-id
            // deferred intent from the current reply. Never bumps the clock,
            // never reissues a report, never warms or skips publishing: the
            // reevaluation releases only through the existing live legs.
            crate::world_dispatch::apply_arrival_capture(
                session,
                retained,
                shared,
                focus_queue,
                pid,
                generation,
                &focus_id,
                issued_revision,
                &adapter_incarnation,
                &core_generation,
                obs,
                revision,
                context_generation,
                captures,
            );
            emit(shared, pid, generation, 0, "arrival", "captured", 1);
        }
        EngineJob::ManagedFactsQuery { window, reply } => {
            // Current engine-managed truth for one subject window: the
            // dedicated effect owner proves against this per edge. The
            // context rides the shared cell (same-thread read); a missing
            // context withholds actuation inside the publication.
            // Correlated receive/reply pair for the requester's query line:
            // `received` marks the query dequeued on the engine (queue wait
            // over), while `replied-managed` vs `replied-unmanaged` marks a
            // delivered verdict; `reply-dropped` marks the requester went
            // away before delivery. A requester `noreply-timeout` with no
            // matching `received` for the same window suggests queue blocking
            // ahead of the query; `received` without a timely delivered reply
            // suggests engine-side stall instead. The window id rides
            // `ref_id` so the exact subject correlates with the requester's
            // `window=<pid>_<wid>` line. Ring logs only (numbers plus fixed
            // tokens); never window content.
            let (pid, wid) = crate::runtime_types::parse_window_id(&window).unwrap_or((0, 0));
            let wid_ref = u64::from(wid);
            emit(shared, pid, 0, wid_ref, "engine-facts", "received", 0);
            let ctx = lock_shared(shared).native_context.clone();
            let facts = retained.engine_managed_facts_for(&window, ctx.as_deref());
            let (generation, verdict) = match &facts {
                Some(facts) => (facts.generation, "replied-managed"),
                None => (0, "replied-unmanaged"),
            };
            if reply.send(facts).is_ok() {
                emit(shared, pid, generation, wid_ref, "engine-facts", verdict, 0);
            } else {
                emit(
                    shared,
                    pid,
                    generation,
                    wid_ref,
                    "engine-facts",
                    "reply-dropped",
                    0,
                );
            }
        }
        EngineJob::WorkspaceOccupancyQuery { reply } => {
            // Read-only retained snapshot: per-domain managed membership
            // (current and hidden alike) plus the context revision it was
            // homed under. Stages nothing, plans nothing - the main thread
            // proves completeness by comparing the revision against its
            // live snapshot and renders Unknown on any drift.
            let report = occupancy_report(retained, shared);
            let _ = reply.send(report);
        }
        EngineJob::Barrier(tx) => {
            let _ = tx.send(());
        }
        EngineJob::TestPanic => {
            panic!("phase-1 offline fault injection: engine panic");
        }
    }
}

/// Dispatch context for one job: native when the mode is native and a
/// derived context is published; the skeleton otherwise (legacy parity:
/// inputs planned the skeleton before any snapshot published).
fn dispatch_context(shared: &Arc<Mutex<Shared>>, session: &WorldSession) -> DispatchContext {
    if session.is_native()
        && let Some(ctx) = lock_shared(shared).native_context.clone()
    {
        return DispatchContext::Native(ctx);
    }
    DispatchContext::Synthetic
}

/// Armed-select lane (tap and indicator): admission gates, planner-owned
/// world command, and — only when the reply applies against the current
/// watermark — one pending claim bound under the stable operation identity
/// for the main-thread drain, carrying the reply-derived target plan.
/// Sequencer-stale refusals still carry the operation identity (the command
/// already executed core-side at-most-once) but bind NO plan: the stored
/// claim keeps the op for feedback while the drain refuses without
/// actuation rather than executing on the previous reply's bindings.
/// Other core refusals drop the claim (never actuated, never replayed).
/// Ordinary tap commands resolve their output core-side (`None`: cursor /
/// focus target); indicator taps that carry an explicit semantic output
/// pass `Some`. The adapter never mints logical ids or re-resolves
/// ordinals.
fn handle_select_action(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
) {
    handle_select_action_with(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        claim,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit select lane: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local). Trailing `index 0` admits and dispatches
/// `SelectTrailing` only when enabled. Only the accepted reply's retained
/// view target plus its live native binding (or its explicit creation
/// desire) ride the stored claim, so the drain actuates exactly the
/// core-resolved target through a live SID lookup - never a pre-reply
/// ordinal reinterpretation. No adapter-side ordinal-to-SID pinning
/// happens here: numbered taps ride binding-free like
/// relative/previous/trailing, and the core owns targeting, history, and
/// reuse-vs-create.
#[allow(clippy::too_many_arguments)]
fn handle_select_action_with(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
    trailing_enabled: bool,
) {
    let verdict = {
        let guard = lock_shared(shared);
        admit_select_claim_with(guard.native_context.is_some(), claim, trailing_enabled)
    };
    let claim = match verdict {
        Ok(claim) => claim,
        Err(token) => {
            refuse_select_claim(shared, token);
            return;
        }
    };
    let Some(ctx) = lock_shared(shared).native_context.clone() else {
        refuse_select_claim(shared, "refused-select-no-topology");
        return;
    };
    // Ordinary taps resolve their output core-side (`None`); indicator
    // taps that already carry a render-time binding keep their explicit
    // semantic output (`Some`). The indicator exact-id action carries its
    // verified core output directly (drain-side side-table, checked
    // against the coherent latest reply before submit). No pre-reply
    // ordinal claims: every armed select dispatches its positional core
    // command and only the accepted reply resolves the target. Indicator
    // taps keep their immutable render-time SID only as a drain-side
    // staleness fence - never as the actuation target.
    let tap = claim.expected_sid.is_none();
    let command_output: Option<tiler_core::directional::OutputId> = match &claim.action {
        crate::runtime_types::InputSemanticAction::IndicatorSelectWorkspace { output, .. } => {
            Some(tiler_core::directional::OutputId(output.clone()))
        }
        _ if tap => None,
        _ => {
            let Some(output) = select_output(retained, &ctx) else {
                refuse_select_claim(shared, "refused-select-no-view");
                return;
            };
            Some(output)
        }
    };
    let command = match &claim.action {
        crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 } => {
            match crate::world_command_map::select_command_with_capability(
                &claim.action,
                command_output.clone(),
                trailing_enabled,
            ) {
                Some(command) => command,
                None => {
                    refuse_select_claim(shared, "refused-select-unarmed");
                    return;
                }
            }
        }
        crate::runtime_types::InputSemanticAction::WorkspaceSelect { index }
            if (1..=9).contains(index) =>
        {
            match select_command(&claim.action, command_output.clone()) {
                Some(command) => command,
                None => {
                    refuse_select_claim(shared, "refused-select-unarmed");
                    return;
                }
            }
        }
        _ => match select_command(&claim.action, command_output.clone()) {
            Some(command) => command,
            None => {
                refuse_select_claim(shared, "refused-select-unarmed");
                return;
            }
        },
    };
    match dispatch_report(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        DispatchContext::Native(Arc::clone(&ctx)),
        Vec::new(),
        Some(command),
        None,
        true,
        OP_SELECT,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied { select_operation } => {
            // Only the currently applied reply binds a plan: its operation
            // identity plus its retained view target, live native binding
            // (or explicit creation desire), and observation-time baseline
            // ride the stored claim so the drain actuates exactly the
            // core-resolved target through a live SID lookup - never a
            // pre-reply ordinal reinterpretation (raw attempt evidence,
            // never a verdict).
            let mut claim = claim;
            if let Some(op) = select_operation.as_deref() {
                bind_select_plan(session, &ctx, &mut claim, op);
            }
            store_select_claim(shared, claim, select_operation.as_deref());
        }
        DispatchVerdict::Duplicate => {
            // Identical latest resend (unreachable for fresh command
            // sequences): no new plan binds; the drain refuses without
            // actuation rather than reinterpreting an ordinal.
            store_select_claim(shared, claim, None);
        }
        DispatchVerdict::Refused {
            token,
            select_operation,
        } => {
            // A sequencer-stale refusal still carries the operation
            // identity (the command already executed core-side
            // at-most-once) but binds NO plan: the sequencer retains the
            // previous reply, so binding here would execute on stale
            // bindings/creations. The claim stores the op for feedback
            // while the drain refuses without actuation; a later honest
            // reply rebinds the plan (see `settle_select_transfers`).
            // Every other refusal binds nothing (the core never ran).
            if let Some(operation) = select_operation {
                store_select_claim(shared, claim, Some(&operation));
            } else {
                lock_shared(shared).diagnostics.input_select_refused += 1;
                emit(shared, 0, 0, 0, "input", "select-refused", 0);
                let _ = token;
            }
        }
        DispatchVerdict::NoReport => {
            refuse_select_claim(shared, "refused-select-no-topology");
        }
    }
}

/// Refuse one send claim narrowly (counted on the shared select-refused
/// counter - one pending slot for both lanes - never dispatched, never
/// replayed).
fn refuse_send_claim(shared: &Arc<Mutex<Shared>>, token: &'static str) {
    lock_shared(shared).diagnostics.input_select_refused += 1;
    emit(shared, 0, 0, 0, "input", token, 0);
}

/// Pin the core-proven post-send source focus into a stay claim after
/// dispatch and before the claim store (pure mapping; the caller queries
/// [`tiler_core::world_engine::WorldEngine::stay_source_focus`] on committed
/// state). `Resident` pins the window, `Empty` binds the proven-empty `None`
/// fallback, and `Unknown` refuses: the drain must never synthesize a
/// fallback from the mover, global focus, or target. Follows never reach
/// here.
pub(crate) fn pin_stay_claim(
    mut claim: crate::runtime_types::ClaimedSelect,
    focus: tiler_core::world_engine::StaySourceFocus,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    match focus {
        tiler_core::world_engine::StaySourceFocus::Resident(window) => {
            claim.send_source_focus = Some(window.0);
            claim.send_source_focus_empty = false;
            Ok(claim)
        }
        tiler_core::world_engine::StaySourceFocus::Empty => {
            claim.send_source_focus = None;
            claim.send_source_focus_empty = true;
            Ok(claim)
        }
        tiler_core::world_engine::StaySourceFocus::Unknown => {
            Err("refused-send-unknown-source-focus")
        }
    }
}

/// Armed-send lane (numbered sends 1..9 product path plus the
/// capability-gated trailing send-0 follow): admission, a positional
/// planner-owned `SendNumbered` world command for the explicit source, and
/// when the reply applies, or when a stale-observation refusal proves the
/// command already executed core-side — one bound pending claim for the
/// main-thread send drain. Other core refusals drop the claim (never actuated, never
/// replayed). The subject is observed truth (never planned focus): the
/// focused window must be homed to a currently-actuated domain and must
/// not read native fullscreen (game/fullscreen foregrounds never drag).
/// No admission-time ordinal-to-SID pin and no adapter output decision:
/// the dispatched command resolves the ordinal core-side against the
/// adopted ring for the core-owned implicit source, and the accepted
/// reply's retained window transfer plus its live native binding (or
/// explicit creation desire when unbound) binds the claim plan through
/// [`bind_send_plan`] — the drain actuates exactly that core-resolved
/// target through a live SID lookup, never an input-ordinal
/// reinterpretation. Unbound trailing without a creation desire binds no
/// SID (missing bindings never create); unbound trailing WITH a desire
/// binds the desire but no SID, refused pending the desire-gated native
/// creation seam (prerequisite blocker, not a settled refusal). The
/// subject lifetime
/// (`send_generation`) and source domain (`send_source`) pin here from
/// retained engine truth - never re-sampled at dispatch. Stay claims pin
/// the core-proven post-send source focus after dispatch and before the
/// claim store through [`pin_stay_claim`] (unknown refuses and stores
/// nothing); dormant while stays are unarmed in
/// [`crate::workspace_input::is_armed_send`].
fn handle_send_action(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
) {
    handle_send_action_with(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        claim,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit send lane: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local). Trailing send-0 follow admits only when enabled;
/// stay-0 never admits (out of scope, no approved form).
#[allow(clippy::too_many_arguments)]
fn handle_send_action_with(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
    trailing_enabled: bool,
) {
    use crate::runtime_types::InputSemanticAction;
    use crate::workspace_input::is_armed_send_with;
    let InputSemanticAction::WorkspaceSend { .. } = claim.action.clone() else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    if !is_armed_send_with(&claim.action, trailing_enabled) {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    }
    let verdict = {
        let guard = lock_shared(shared);
        admit_send_claim_with(guard.native_context.is_some(), claim, trailing_enabled)
    };
    let mut claim = match verdict {
        Ok(claim) => claim,
        Err(token) => {
            refuse_send_claim(shared, token);
            return;
        }
    };
    let Some(window) = retained.observed_focus.clone() else {
        refuse_send_claim(shared, "refused-send-no-focus");
        return;
    };
    let Some(ctx) = lock_shared(shared).native_context.clone() else {
        refuse_send_claim(shared, "refused-send-no-topology");
        return;
    };
    // Managed-current homing from the narrow retained publication: an
    // unmapped subject refuses instead of planning the skeleton, and a
    // hidden-domain subject never dispatches (no drag out of an off-current
    // Space). The fullscreen, minimized-exclusion, and lifetime pins ride
    // the same publication - never cached topology, static scope, or
    // dispatch-time re-sampling.
    let Some((subject_pid, _)) = crate::runtime_types::parse_window_id(&window.0) else {
        refuse_send_claim(shared, "refused-send-bad-subject");
        return;
    };
    let Some(facts) = retained.engine_managed_facts_for(&window.0, Some(ctx.as_ref())) else {
        if !retained.apps.contains_key(&subject_pid) {
            refuse_send_claim(shared, "refused-send-no-lifetime");
        } else {
            refuse_send_claim(shared, "refused-send-no-domain");
        }
        return;
    };
    let key = facts.source.clone();
    if !facts.source_current {
        refuse_send_claim(shared, "refused-send-hidden-domain");
        return;
    }
    // Fullscreen refusal: a fullscreen subject owns its Space - dragging
    // it would tear the Space. A classifier-excluded minimized subject has
    // no input-reaching titlebar. Both read from retained observation
    // truth (the drain re-proves them fresh before any post).
    if facts.fullscreen {
        refuse_send_claim(shared, "refused-send-fullscreen");
        return;
    }
    if facts.minimized {
        refuse_send_claim(shared, "refused-send-minimized");
        return;
    }
    // The trailing `index 0` follow binds NO admission SID (the Space may
    // not exist yet): only an accepted reply's retained transfer plus its
    // live binding binds a plan through `bind_send_plan`. Unbound trailing
    // without an explicit creation desire binds nothing (missing bindings
    // never create); unbound trailing WITH a desire binds the desire but no
    // SID, and the drain refuses it pending the desire-gated native creation
    // seam (prerequisite blocker, not a settled refusal — see the drain).
    let Some(command) = crate::world_command_map::send_command_implicit_with_capability(
        &claim.action,
        &window.0,
        trailing_enabled,
    ) else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    // Stay-return pin: the exact native SID of the admission-pinned source
    // domain under this admission topology (never a drain-time guess). This
    // is native freshness for the return-leg actuation pin, not a policy
    // decision: targeting itself resolved core-side above.
    let Some(source_sid) = ctx
        .domains
        .iter()
        .find(|d| d.output == key.output && d.workspace == key.workspace)
        .map(|d| d.space)
    else {
        refuse_send_claim(shared, "refused-send-no-source");
        return;
    };
    claim.expected_sid = None;
    claim.topology_revision = Some(ctx.revision);
    claim.send_subject = Some(window.0.clone());
    claim.send_generation = Some(facts.generation);
    claim.send_source = Some(key.clone());
    claim.send_source_sid = Some(source_sid);
    // Stays pin the core-proven post-send source focus after dispatch and
    // before the claim store; follows store directly. Dormant while stays
    // are unarmed (`is_armed_send` refuses above): no product stay reaches
    // dispatch until a reviewed arming.
    let stay = matches!(
        &claim.action,
        InputSemanticAction::WorkspaceSend { follow: false, .. }
    );
    match dispatch_report(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        DispatchContext::Native(Arc::clone(&ctx)),
        Vec::new(),
        Some(command),
        None,
        true,
        OP_SEND,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied { select_operation } => {
            // The applied reply's operation identity plus its retained
            // transfer plan ride the stored claim so the drain actuates
            // exactly the core-resolved target through a live SID lookup
            // and reports its native outcome under that identity (raw
            // attempt evidence, never a verdict).
            if !stay {
                let mut claim = claim;
                if let Some(op) = select_operation.as_deref() {
                    bind_send_plan(session, &ctx, &mut claim, op);
                }
                store_select_claim(shared, claim, select_operation.as_deref());
            } else if let Some(op) = select_operation.as_deref() {
                let mut claim = claim;
                bind_send_plan(session, &ctx, &mut claim, op);
                let focus = session.engine_mut().stay_source_focus(op, &key);
                match pin_stay_claim(claim, focus) {
                    Ok(pinned) => store_select_claim(shared, pinned, Some(op)),
                    Err(token) => refuse_send_claim(shared, token),
                }
            } else {
                refuse_send_claim(shared, "refused-send-unknown-source-focus");
            }
        }
        DispatchVerdict::Duplicate => {
            if !stay {
                store_select_claim(shared, claim, None);
            } else {
                refuse_send_claim(shared, "refused-send-unknown-source-focus");
            }
        }
        DispatchVerdict::Refused {
            token,
            select_operation,
        } => {
            if let Some(operation) = select_operation {
                if !stay {
                    store_select_claim(shared, claim, Some(&operation));
                } else {
                    let focus = session.engine_mut().stay_source_focus(&operation, &key);
                    match pin_stay_claim(claim, focus) {
                        Ok(pinned) => store_select_claim(shared, pinned, Some(&operation)),
                        Err(refusal) => refuse_send_claim(shared, refusal),
                    }
                }
            } else {
                lock_shared(shared).diagnostics.input_select_refused += 1;
                emit(shared, 0, 0, 0, "input", "send-refused", 0);
                let _ = token;
            }
        }
        DispatchVerdict::NoReport => {
            refuse_send_claim(shared, "refused-send-no-topology");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_control_bypasses_data_saturation() {
        let inbox = Inbox::new(1);
        let data = || EngineJob::Observation {
            pid: 9,
            generation: 1,
            seq: 1,
            windows: Vec::new(),
            focused_wid: None,
        };
        // Fill the single data slot.
        assert!(inbox.try_send_data(data()));
        assert!(!inbox.try_send_data(data()));
        // Control admission is independent of data saturation, and the
        // queue stays strict FIFO.
        let (ctx, _crx) = mpsc::channel::<()>();
        assert!(inbox.send_control(EngineJob::Barrier(ctx)));
        let stop = AtomicBool::new(false);
        assert!(matches!(
            inbox.recv(&stop),
            Some(EngineJob::Observation { .. })
        ));
        assert!(matches!(inbox.recv(&stop), Some(EngineJob::Barrier(_))));
        // Stop admission is independent too.
        let (control_dropped, data_dropped) = inbox.close_and_drain();
        assert_eq!((control_dropped, data_dropped), (0, 0));
    }

    #[test]
    fn frame_slot_is_latest_wins() {
        let slot = Arc::new(FrameSlot::new());
        let stop = AtomicBool::new(false);
        let cmd = |id| FrameCommand {
            pid: 7,
            generation: 1,
            cmd_id: id,
            window: "7_1".to_owned(),
            wid: 1,
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            topology_revision: 0,
            domain: None,
        };
        assert_eq!(slot.set_latest(vec![cmd(1)]), Some(0));
        // A whole newer desired set overwrites the pending one; the
        // overwritten command count is reported for diagnostics. An empty
        // set supersedes too (window removal converges to empty).
        assert_eq!(slot.set_latest(vec![cmd(2), cmd(3)]), Some(1));
        let (rev, taken) = slot.take_versioned(&stop).expect("pending set");
        assert_eq!(
            taken.iter().map(|c| c.cmd_id).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(slot.current_rev(), rev);
        assert_eq!(slot.set_latest(Vec::new()), Some(0));
        let (rev2, emptied) = slot.take_versioned(&stop).expect("empty set");
        assert!(emptied.is_empty());
        assert!(rev2 > rev);
        slot.stop();
        assert_eq!(slot.set_latest(vec![cmd(4)]), None);
        assert_eq!(slot.take_versioned(&stop), None);
    }

    fn focus_cmd(pid: u32, generation: u64, revision: u64) -> FocusCommand {
        FocusCommand {
            pid,
            generation,
            request_id: 1,
            wid: 7,
            window: format!("{pid}_7"),
            revision,
            route: crate::focus_route::FocusRoute::Slps,
            expect_prior: None,
            close: None,
        }
    }

    #[test]
    fn focus_slot_is_latest_wins_and_dies_with_lifetime() {
        let slot = Arc::new(FocusSlot::new());
        // Newer desired focus supersedes: only the latest dispatches, so a
        // refocus never fights the plan.
        assert!(slot.set_latest(focus_cmd(9, 1, 3)).is_some());
        assert!(slot.set_latest(focus_cmd(9, 1, 4)).is_some());
        let taken = slot.take().expect("pending focus");
        assert_eq!(taken.revision, 4);
        assert!(slot.take().is_none(), "take drains");
        // Stop invalidates: queued focus never outlives its lifetime.
        assert!(slot.set_latest(focus_cmd(9, 1, 5)).is_some());
        slot.stop();
        assert!(slot.set_latest(focus_cmd(9, 1, 6)).is_none());
        assert!(slot.take().is_none());
    }

    use crate::runtime_types::{ClaimedSelect, InputSemanticAction};

    fn tap_claim() -> ClaimedSelect {
        ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 })
    }

    fn bound_claim() -> ClaimedSelect {
        ClaimedSelect::indicator(InputSemanticAction::WorkspaceSelect { index: 2 }, 3, 7, 9)
    }

    #[test]
    fn select_admission_gates_unarmed_and_topology() {
        // Sends and outputs refuse on the select lane even with a bound
        // click payload - a forged binding never widens the lane. The
        // armed previous-toggle admits binding-free, and the armed
        // trailing select-0 admits (its binding still fences fresh at the
        // drain).
        {
            let action = InputSemanticAction::WorkspaceSend {
                index: 1,
                follow: true,
            };
            let claim = ClaimedSelect::indicator(action, 3, 7, 9);
            let err = admit_select_claim(true, claim).expect_err("unarmed refuses");
            assert_eq!(err, "refused-select-unarmed");
        }
        assert_eq!(
            admit_select_claim(
                true,
                ClaimedSelect::indicator(
                    InputSemanticAction::WorkspaceSelect { index: 0 },
                    3,
                    7,
                    9
                ),
            )
            .map(|c| c.action),
            Ok(InputSemanticAction::WorkspaceSelect { index: 0 }),
            "armed trailing select-0 admits (drain fences the binding)"
        );
        assert_eq!(
            admit_select_claim(
                true,
                ClaimedSelect::tap(InputSemanticAction::WorkspacePrevious),
            )
            .map(|c| c.action),
            Ok(InputSemanticAction::WorkspacePrevious),
        );
        // No published native context refuses before any claim is stored.
        let err = admit_select_claim(false, tap_claim()).expect_err("no-topology refuses");
        assert_eq!(err, "refused-select-no-topology");
        // No busy or already-pending refusal: admission carries no busy
        // gate. Queued FIFO serialization behind an in-progress drain is
        // covered by the engine-lane tests below; the drain (not
        // admission) orders actuation.
        // Open lane admits both lanes verbatim - the indicator binding
        // rides through untouched for the drain to fence.
        assert_eq!(admit_select_claim(true, tap_claim()), Ok(tap_claim()));
        assert_eq!(admit_select_claim(true, bound_claim()), Ok(bound_claim()));
    }

    #[test]
    fn trailing_select_admission_armed_in_production() {
        use crate::runtime_types::InputSemanticAction;
        let trailing = ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 });
        // Production (gate `true`): trailing admits with topology and lane
        // open.
        assert_eq!(
            admit_select_claim(true, trailing.clone()),
            Ok(trailing.clone())
        );
        // Capability-disabled pin through the same production function
        // refuses; topology still fences when armed. There is no busy
        // gate: every armed command admits and queues FIFO.
        assert_eq!(
            admit_select_claim_with(true, trailing.clone(), false),
            Err("refused-select-unarmed")
        );
        assert_eq!(
            admit_select_claim_with(true, trailing.clone(), true),
            Ok(trailing.clone())
        );
        assert_eq!(
            admit_select_claim_with(false, trailing.clone(), true),
            Err("refused-select-no-topology")
        );
        assert_eq!(
            admit_select_claim_with(true, trailing.clone(), true),
            Ok(trailing.clone()),
            "no busy refusal: armed admits regardless of drain state"
        );
    }

    #[test]
    fn stay_pin_maps_resident_empty_and_unknown() {
        use tiler_core::directional::WindowId;
        use tiler_core::world_engine::StaySourceFocus;
        let stay = || {
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
                index: 2,
                follow: false,
            })
        };
        // Resident pins the window with the empty flag clear.
        let pinned = pin_stay_claim(
            stay(),
            StaySourceFocus::Resident(WindowId("4242_9".to_owned())),
        )
        .expect("resident pins");
        assert_eq!(pinned.send_source_focus.as_deref(), Some("4242_9"));
        assert!(!pinned.send_source_focus_empty);
        // Proven-empty binds the None fallback.
        let empty = pin_stay_claim(stay(), StaySourceFocus::Empty).expect("empty binds");
        assert_eq!(empty.send_source_focus, None);
        assert!(empty.send_source_focus_empty);
        // Unknown refuses and pins nothing.
        let err = pin_stay_claim(stay(), StaySourceFocus::Unknown).expect_err("unknown refuses");
        assert_eq!(err, "refused-send-unknown-source-focus");
    }
}

#[cfg(test)]
mod trailing_join_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::session::DomainKey;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::FrameSetter;

    use crate::focus_route::FocusRoute;
    use crate::runtime::{Shared, lock_shared};
    use crate::runtime_core::{RetainedApp, RetainedCore, RosterEntry};
    use crate::runtime_types::{
        ClaimedSelect, InputSemanticAction, NativeContext, NativeDomain, ObservedWindow,
    };
    use crate::topology::workspace_key;
    use crate::workspace_select::{SelectionBackend, SelectionSnapshot};
    use crate::world_runtime::{WorldSession, build_dispatch_report};
    use crate::world_sequencer::ObservationClock;

    const UUID_1: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";

    fn output() -> OutputId {
        OutputId(crate::topology::output_key(1))
    }

    fn domain(space: u64, current: bool) -> NativeDomain {
        NativeDomain {
            cg_id: 1,
            uuid: Some(UUID_1.to_owned()),
            output: output(),
            workspace: WorkspaceId(crate::topology::workspace_key(space)),
            space,
            current,
            readable: true,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            visible: Rect {
                x: 0,
                y: 25,
                w: 1440,
                h: 875,
            },
        }
    }

    fn ctx() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 9,
            domains: vec![domain(1, true), domain(277, false)],
            skipped: Vec::new(),
        })
    }

    fn test_shared(ctx: Arc<NativeContext>) -> Arc<Mutex<Shared>> {
        Arc::new(Mutex::new(Shared {
            config: crate::runtime_types::RuntimeConfig::default(),
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            clock: Arc::new(ObservationClock::new()),
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            main_wake: None,
            topology_revision: ctx.revision,
            native_context: Some(Arc::clone(&ctx)),
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
            arrival_captures: std::collections::BTreeMap::new(),
            selection_focus_floor_seq: 0,
            close_focus_current: None,
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: None,
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(AtomicBool::new(false)),
            stop_input: Arc::new(AtomicBool::new(false)),
            stop_logger: Arc::new(AtomicBool::new(false)),
            log_tx: None,
            diagnostics: crate::runtime_types::RuntimeDiagnostics::default(),
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: crate::runtime::empty_world_presentation_v4(),
            last_pager_bindings: Vec::new(),
            last_workspace_desires: Vec::new(),
            last_core_generation: String::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }))
    }

    #[test]
    fn engine_claim_store_sets_flag_then_signals_wake() {
        // Production engine publisher: `store_select_claim` appends the
        // claim in admission order, sets the sticky slot flag, then signals
        // the published main wake (flag-set then signal) — the exact
        // protocol the parked main thread relies on. Deterministic: no
        // threads, no timing; queue, flag, and generation state are the
        // assertions.
        let wake = crate::ffi::MainWakeSignal::detached();
        let shared = test_shared(ctx());
        lock_shared(&shared).main_wake = Some(wake.clone());
        let before = wake.generation();
        store_select_claim(
            &shared,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 }),
            None,
        );
        let guard = lock_shared(&shared);
        assert!(
            guard.select_claim_pending,
            "claim store sets the sticky flag"
        );
        assert_eq!(guard.select_pending.len(), 1, "claim store enqueues FIFO");
        assert_eq!(
            wake.generation(),
            before + 1,
            "claim store signals exactly once"
        );
        drop(guard);
        // A store racing publication (wake not yet published) still sets
        // the flag and enqueues: the main loop takes the flag before its
        // first park, so nothing is lost.
        let bare = test_shared(ctx());
        store_select_claim(
            &bare,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 3 }),
            None,
        );
        let guard = lock_shared(&bare);
        assert!(guard.select_claim_pending);
        assert_eq!(guard.select_pending.len(), 1);
    }

    #[test]
    fn close_focus_epoch_latest_only_cancels_taken_unstarted() {
        // The dispatch half publishes the latest coherent close envelope;
        // the owning worker refuses a taken-but-unstarted command whose
        // bound id/revision/session no longer matches. A newer publication
        // — or a null one (explicit focus, user command) — cancels the old
        // epoch. The session legs fence handshake aliases reusing the same
        // id/revision.
        let shared = test_shared(ctx());
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-1",
                "gen-1",
                "close-focus-3",
                3
            ),
            "empty epoch matches nothing"
        );
        crate::runtime_shared::publish_close_focus_current(
            &shared,
            Some(crate::runtime_shared::CloseFocusEpoch {
                id: "close-focus-3".to_owned(),
                revision: 3,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
            }),
        );
        assert!(crate::runtime_shared::close_focus_is_current(
            &shared,
            "adapter-1",
            "gen-1",
            "close-focus-3",
            3
        ));
        // Same id but a newer issuing revision is a different epoch.
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-1",
                "gen-1",
                "close-focus-3",
                4
            ),
            "revision is part of the epoch, never id alone"
        );
        // Same id/revision on a foreign session never matches (handshake
        // alias: a new handshake reusing close-focus-3/3 must not satisfy
        // the old taken command).
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-2",
                "gen-1",
                "close-focus-3",
                3
            ),
            "incarnation is part of the epoch"
        );
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-1",
                "gen-2",
                "close-focus-3",
                3
            ),
            "generation is part of the epoch"
        );
        // A newer publication supersedes the old taken-but-unstarted one.
        crate::runtime_shared::publish_close_focus_current(
            &shared,
            Some(crate::runtime_shared::CloseFocusEpoch {
                id: "close-focus-4".to_owned(),
                revision: 4,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
            }),
        );
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-1",
                "gen-1",
                "close-focus-3",
                3
            ),
            "newer envelope cancels the old command"
        );
        assert!(crate::runtime_shared::close_focus_is_current(
            &shared,
            "adapter-1",
            "gen-1",
            "close-focus-4",
            4
        ));
        // A null publication cancels everything taken-but-unstarted.
        crate::runtime_shared::publish_close_focus_current(&shared, None);
        assert!(
            !crate::runtime_shared::close_focus_is_current(
                &shared,
                "adapter-1",
                "gen-1",
                "close-focus-4",
                4
            ),
            "null cancels the current epoch"
        );
    }

    fn session() -> WorldSession {
        WorldSession::new(
            true,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        )
    }

    fn close_app(shared: &Arc<Mutex<Shared>>, pid: u32, generation: u64) {
        use crate::runtime_shared::AppRecord;
        lock_shared(shared).apps.insert(
            pid,
            AppRecord {
                generation,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::new(FrameSlot::new()),
                focus: Arc::new(FocusSlot::new()),
                ready: Arc::new(Mutex::new(Some(Ok(())))),
                worker: None,
                worker_name: "test-close".to_owned(),
            },
        );
    }

    fn prime_close_current(
        session: &WorldSession,
        shared: &Arc<Mutex<Shared>>,
        id: &str,
        rev: u64,
        wid: u32,
    ) {
        use tiler_core::directional::WindowId;
        let incarnation = session.incarnation().to_owned();
        let generation = session.sequencer().bound_generation();
        session.sequencer().observe_close_envelope(
            Some(&tiler_core::world_report::FocusRequest {
                id: id.to_owned(),
                target: WindowId(format!("4242_{wid}")),
                issued_revision: rev,
                cause: tiler_core::world_report::FocusRequestCause::PositiveClose,
                output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
                workspace: "ws-a".to_owned(),
                cmd_seq: None,
                after_shown: None,
            }),
            &incarnation,
            &generation,
        );
        assert!(
            session.sequencer().is_current_close_request(
                id,
                &WindowId(format!("4242_{wid}")),
                rev,
                &incarnation,
                &generation,
            ),
            "primed request must read current"
        );
        crate::runtime_shared::publish_close_focus_current(
            shared,
            Some(crate::runtime_shared::CloseFocusEpoch {
                id: id.to_owned(),
                revision: rev,
                adapter_incarnation: incarnation,
                core_generation: generation,
            }),
        );
    }

    fn close_result(
        session: &WorldSession,
        pid: u32,
        generation: u64,
        id: &str,
        rev: u64,
        wid: u32,
    ) -> EngineJob {
        EngineJob::CloseFocusResult {
            pid,
            generation,
            cmd_id: 5,
            close_id: id.to_owned(),
            target: tiler_core::directional::WindowId(format!("4242_{wid}")),
            issued_rev: rev,
            adapter_incarnation: session.incarnation().to_owned(),
            core_generation: session.sequencer().bound_generation(),
            setter: tiler_core::world_report::VisibilitySetter::NotCalled,
            reason: tiler_core::world_report::VisibilityReason::IdentityStale,
            pre: None,
            post: None,
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        }
    }

    #[test]
    fn close_result_invalid_lifetime_stages_nothing() {
        // Dead lifetimes stage nothing, even for the current request:
        // generation drift first, then termination. The live lifetime
        // for the same request still stages under its original stamp.
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        close_app(&shared, 4242, 1);
        prime_close_current(&session, &shared, "close-focus-7", 7, 9);
        let stale_before = lock_shared(&shared).diagnostics.stale_dropped;
        process_job(
            close_result(&session, 4242, 2, "close-focus-7", 7, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "drifted lifetime stages nothing"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 1
        );
        lock_shared(&shared)
            .apps
            .get_mut(&4242)
            .expect("app")
            .terminated = true;
        process_job(
            close_result(&session, 4242, 1, "close-focus-7", 7, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "terminated lifetime stages nothing"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 2
        );
        lock_shared(&shared)
            .apps
            .get_mut(&4242)
            .expect("app")
            .terminated = false;
        process_job(
            close_result(&session, 4242, 1, "close-focus-7", 7, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let staged = session.peek_close_feedback().expect("live stages");
        assert_eq!(staged.id, "close-focus-7");
        assert_eq!(staged.revision, 7);
    }

    #[test]
    fn close_result_stale_identity_never_supersedes_fresh() {
        // Old result before fresh row: stale identity rejects and the
        // fresh row still stages under its original stamp. A session
        // alias and a superseded predecessor both reject; the first
        // staged row wins and keeps its stamp.
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        close_app(&shared, 4242, 1);
        prime_close_current(&session, &shared, "close-focus-7", 7, 9);
        let stale_before = lock_shared(&shared).diagnostics.stale_dropped;
        // Old result arrives first: rejected, nothing staged, so the
        // fresh row is not suppressed.
        process_job(
            close_result(&session, 4242, 1, "close-focus-6", 6, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "old result stages nothing"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 1
        );
        // Fresh result stages under its original stamp.
        process_job(
            close_result(&session, 4242, 1, "close-focus-7", 7, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let staged = session.peek_close_feedback().expect("fresh stages");
        assert_eq!(staged.id, "close-focus-7");
        assert_eq!(staged.revision, 7);
        // Same id/revision on an aliased session: rejected, original kept.
        let mut alias = close_result(&session, 4242, 1, "close-focus-7", 7, 9);
        if let EngineJob::CloseFocusResult {
            ref mut core_generation,
            ..
        } = alias
        {
            *core_generation = "gen-alias".to_owned();
        }
        process_job(
            alias,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("original kept").id,
            "close-focus-7"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 2
        );
        // Same-lifetime new request supersedes: the predecessor result
        // rejects, and the first staged row keeps its original stamp even
        // when the current request's own result correlates.
        prime_close_current(&session, &shared, "close-focus-8", 8, 9);
        process_job(
            close_result(&session, 4242, 1, "close-focus-7", 7, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("original kept").id,
            "close-focus-7"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 3
        );
        let stale_before_current = lock_shared(&shared).diagnostics.stale_dropped;
        process_job(
            close_result(&session, 4242, 1, "close-focus-8", 8, 9),
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("first wins").id,
            "close-focus-7",
            "original stamp preserved, never re-stamped"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before_current,
            "current request correlates: first-wins drops it, not the stale fence"
        );
    }

    #[test]
    fn directional_focus_bare_explicit_dispatches_one_native_call() {
        // Responsiveness-rerun repro: a bare explicit directional focus
        // accepted by the core must bind exactly one native focus call
        // between the two tiled windows. The real `process_job`
        // InputAction path (real codec plus real core) drives the bind;
        // the mocked backend drives the owner leg; the typed result
        // stages the Accepted feedback. A bind-time no-call stages its
        // reason here instead of the capture's reason-less row.
        let ctx = ctx();
        let mut seed = retained_send_subject();
        seed.observed_focus_known = true;
        let mut neighbor = observed(8);
        neighbor.x = 700;
        let subject = seed.apps.get_mut(&4242).expect("subject app retained");
        subject.windows.push(neighbor);
        subject.roster.insert(8, roster_entry(neighbor, vec![1]));
        seed.window_domains.insert(
            crate::runtime::window_id(4242, 8),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
            },
        );
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), seed);
        lock_shared(&shared).config.topology_mode = crate::runtime_types::TopologyMode::Native;
        close_app(&shared, 4242, 7);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert_fixture_admitted(&session, &retained);
        assert!(
            lock_shared(&shared).native_context.is_some(),
            "native scope present: logical ids must resolve to native"
        );
        process_job(
            EngineJob::InputAction {
                action: InputSemanticAction::FocusStep {
                    direction: crate::runtime_types::InputDirection::Left,
                },
                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        // The core names logical workspaces while the native scope is
        // native-keyed: the applied reply must carry both vocabularies.
        let req = session
            .sequencer()
            .last_reply()
            .and_then(|reply| reply.focus_request.clone())
            .expect("accepted focus mints a request");
        assert_eq!(req.target.0, "4242_8", "Left steps onto the neighbor");
        assert_ne!(
            req.workspace,
            crate::topology::workspace_key(1),
            "request names the logical workspace, never the native key"
        );
        if let Some(staged) = session.peek_close_feedback() {
            panic!(
                "bind-time no-call staged reason={:?} setter={:?} target={}",
                staged.reason, staged.setter, staged.target.0
            );
        }
        assert_eq!(
            crate::runtime_shared::lock_focus(&focus_queue).len(),
            1,
            "accepted focus queues exactly one row"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dispatched,
            1,
            "accepted focus dispatches once"
        );
        let cmd = lock_shared(&shared)
            .apps
            .get(&4242)
            .and_then(|app| app.focus.take())
            .expect("owner slot carries the release");
        assert_eq!(
            cmd.window, "4242_8",
            "release names the Left neighbor, distinct from focus 4242_7"
        );
        let close = cmd.close.clone().expect("unified close route");
        let revision = lock_shared(&shared).topology_revision;
        let mut fake = FakeFocusBackend {
            fresh: Some((4242, Some(7))),
            post: Some((4242, Some(8))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut fake, &cmd, &close, 4242, 7, revision, &shared,
        );
        assert_eq!(fake.dispatches, 1, "exactly one native focus call");
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let staged = session.peek_close_feedback().expect("accepted stages");
        assert_eq!(
            staged.setter,
            tiler_core::world_report::VisibilitySetter::Accepted
        );
    }

    #[test]
    fn directional_focus_repeated_roundtrips_never_exhaust_outbox() {
        // Outbox-exhaustion repro (responsiveness rerun tile-a: 16
        // dispatched-native then no-call-outbox-full-superseded from
        // focus-40): 18 sequential directional explicit focus commands
        // alternating between the two tiled windows. Every round drives the
        // real `process_job` InputAction path (real codec plus real core),
        // the mocked backend actuates exactly once, the typed result stages
        // Accepted, and a genuine target observation settles the core. Each
        // terminal owner completion must retire its exact outbox row, so the
        // bounded queue never counts settled work and no round wedges.
        let ctx = ctx();
        let mut seed = retained_send_subject();
        seed.observed_focus_known = true;
        let mut neighbor = observed(8);
        neighbor.x = 700;
        let subject = seed.apps.get_mut(&4242).expect("subject app retained");
        subject.windows.push(neighbor);
        subject.roster.insert(8, roster_entry(neighbor, vec![1]));
        seed.window_domains.insert(
            crate::runtime::window_id(4242, 8),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
            },
        );
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), seed);
        lock_shared(&shared).config.topology_mode = crate::runtime_types::TopologyMode::Native;
        close_app(&shared, 4242, 7);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert_fixture_admitted(&session, &retained);
        const ROUNDS: usize = 18;
        const CAPACITY: usize = 16;
        let mut queued_lens: Vec<usize> = Vec::with_capacity(ROUNDS);
        let mut refusals: Vec<bool> = Vec::with_capacity(ROUNDS);
        let mut settled_lens: Vec<usize> = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let before = retained.observed_focus.clone().expect("focus observed");
            let target_wid = if before.0 == crate::runtime::window_id(4242, 7) {
                8u32
            } else {
                7u32
            };
            let target =
                tiler_core::directional::WindowId(crate::runtime::window_id(4242, target_wid));
            let direction = if round % 2 == 0 {
                crate::runtime_types::InputDirection::Left
            } else {
                crate::runtime_types::InputDirection::Right
            };
            process_job(
                EngineJob::InputAction {
                    action: InputSemanticAction::FocusStep { direction },
                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                CAPACITY,
            );
            let req = session
                .sequencer()
                .last_reply()
                .and_then(|reply| reply.focus_request.clone())
                .expect("accepted focus mints a request");
            assert_eq!(
                req.target, target,
                "round {round}: focus steps onto the other window"
            );
            // Record-only per round: a bind-time no-call (outbox full)
            // stages here instead of dispatching, so stop driving and let
            // the terminal assertions below report the refusal round.
            refusals.push(session.peek_close_feedback().is_some());
            queued_lens.push(crate::runtime_shared::lock_focus(&focus_queue).len());
            if session.peek_close_feedback().is_some() {
                break;
            }
            let cmd = lock_shared(&shared)
                .apps
                .get(&4242)
                .and_then(|app| app.focus.take())
                .expect("owner slot carries the release");
            let close = cmd.close.clone().expect("unified close route");
            let revision = lock_shared(&shared).topology_revision;
            let mut fake = FakeFocusBackend {
                fresh: Some((4242, Some(if target_wid == 8 { 7 } else { 8 }))),
                post: Some((4242, Some(target_wid))),
                reads: 0,
                dispatches: 0,
                completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
            };
            let job = crate::runtime_worker::handle_close_focus_command(
                &mut fake, &cmd, &close, 4242, 7, revision, &shared,
            );
            assert_eq!(fake.dispatches, 1, "round {round}: exactly one native call");
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                CAPACITY,
            );
            let staged = session
                .peek_close_feedback()
                .expect("actuated feedback stages");
            assert_eq!(
                staged.setter,
                tiler_core::world_report::VisibilitySetter::Accepted,
                "round {round}: every actuated feedback reports Accepted"
            );
            assert_eq!(
                staged.target, target,
                "round {round}: feedback names target"
            );
            // Genuine target observation: promotes observed focus, drains
            // the staged row through a transacted report, and clears the
            // core pending request via target equality.
            let meta = |wid: u32| crate::runtime_types::ScopedWindowMeta {
                wid,
                spaces: vec![1],
                fullscreen: false,
                minimized: false,
                hidden: false,
                role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                modal: Some(false),
                resizable: Some(true),
                layer: Some(0),
                app_id: Some("com.example.app".to_owned()),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            };
            let seq_last = retained
                .apps
                .get(&4242)
                .map(|app| app.seq_last)
                .unwrap_or(0);
            let snapshot_windows = [7u32, 8u32]
                .iter()
                .map(|wid| {
                    let mut window = observed(*wid);
                    if *wid == 8 {
                        window.x = 700;
                    }
                    window
                })
                .collect::<Vec<_>>();
            process_job(
                EngineJob::ScopedObservation {
                    pid: 4242,
                    generation: 7,
                    seq: seq_last + 1,
                    snapshot: crate::runtime_types::ScopedBackendSnapshot {
                        windows: snapshot_windows,
                        focused_wid: Some(target_wid),
                        focus_cg_owned: true,
                        focus_evidence: Some(crate::runtime_types::FocusEvidence {
                            frontmost_pid: Some(4242),
                            frontmost_bundle: None,
                            ax_focus: Some((4242, Some(target_wid))),
                        }),
                        sys_focus: Some((4242, Some(target_wid))),
                        global_focus: Some((4242, target_wid)),
                        meta: vec![meta(7), meta(8)],
                        excluded: 0,
                        excluded_reasons: Vec::new(),
                        cg_complete: true,
                        cg_owner_wids: vec![7, 8],
                        cg_alive: Vec::new(),
                        revision,
                    },
                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                CAPACITY,
            );
            assert_eq!(
                retained.observed_focus,
                Some(target.clone()),
                "round {round}: target observation promotes focus"
            );
            assert!(
                session
                    .sequencer()
                    .last_reply()
                    .expect("settlement keeps the latest reply")
                    .focus_request
                    .is_none(),
                "round {round}: target equality clears the core request"
            );
            assert!(
                session.peek_close_feedback().is_none(),
                "round {round}: settled feedback drains through the report"
            );
            settled_lens.push(crate::runtime_shared::lock_focus(&focus_queue).len());
        }
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dispatched,
            ROUNDS as u64,
            "every round dispatches once, none refused full"
        );
        assert!(
            queued_lens.iter().all(|len| *len <= 1),
            "per-round queue depths stay bounded: {queued_lens:?}"
        );
        assert!(
            settled_lens.iter().all(|len| *len == 0),
            "every round retires its consumed row: {settled_lens:?}"
        );
        assert!(
            refusals.iter().all(|refused| !refused),
            "no round refuses with a bind-time no-call"
        );
        assert!(
            crate::runtime_shared::lock_focus(&focus_queue).is_empty(),
            "final outbox drains"
        );
    }

    #[test]
    #[ignore]
    fn close_result_emits_focus_settle_joining_typed_command_identity() {
        // Enabled-only (`TILER_MACOS_STAGE_TIMING=1`, run explicitly so the
        // default suite stays untraced): the typed `CloseFocusResult`
        // receipt closes the origin-to-feedback endpoint
        // (`timing-focus-settle`) joining the dispatch bind under the typed
        // command identity (pid/generation/cmd_id) - the same TOTAL
        // convention as the legacy `FocusResult` arm. No live calls: the
        // dispatch bind is simulated with `note_focus`.
        if !crate::stage_timing::enabled() {
            eprintln!("skipped without TILER_MACOS_STAGE_TIMING=1");
            return;
        }
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        close_app(&shared, 4242, 1);
        prime_close_current(&session, &shared, "close-focus-7", 7, 9);
        struct SettleSink {
            events: Mutex<Vec<crate::runtime_log::LogEvent>>,
        }

        impl crate::runtime_log::LogSink for SettleSink {
            fn write(&self, event: &crate::runtime_log::LogEvent) {
                self.events.lock().expect("sink").push(event.clone());
            }
        }

        let sink = Arc::new(SettleSink {
            events: Mutex::new(Vec::new()),
        });
        let (tx, stop_logger, logger_handle) = crate::runtime_log::spawn_logger(
            Arc::clone(&sink) as Arc<dyn crate::runtime_log::LogSink>,
            256,
        );
        lock_shared(&shared).log_tx = Some(tx);
        // Simulate the dispatch bind under the typed command identity
        // (`cmd_id: 5` matches `close_result` below).
        let t0_us = crate::stage_timing::now_us();
        crate::stage_timing::note_focus(4242, 1, 5, 778_001, t0_us);
        let mut job = close_result(&session, 4242, 1, "close-focus-7", 7, 9);
        if let EngineJob::CloseFocusResult {
            ref mut setter,
            ref mut attempt_ms,
            ..
        } = job
        {
            *setter = tiler_core::world_report::VisibilitySetter::Accepted;
            *attempt_ms = 3;
        }
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            session.peek_close_feedback().is_some(),
            "typed result stages"
        );
        lock_shared(&shared).log_tx = None;
        stop_logger.store(true, std::sync::atomic::Ordering::SeqCst);
        logger_handle.join().expect("logger joins");
        let events = sink.events.lock().expect("sink").clone();
        let mut settle = events.iter().filter_map(|event| {
            let line = crate::stage_timing::format_record(event);
            let parsed = crate::stage_timing::parse_record(&line).expect("sink line parses");
            (parsed.stage == crate::stage_timing::stage::FOCUS_SETTLE).then_some(parsed)
        });
        let parsed = settle
            .next()
            .expect("typed result emits timing-focus-settle");
        assert_eq!(parsed.ref_id, 778_001, "settle joins the dispatch trace");
        assert_eq!(parsed.outcome, "applied", "settle names the setter token");
    }

    fn observed(wid: u32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x: 100,
            y: 100,
            w: 800,
            h: 600,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    /// Retained truth with window B homed to space-277 (occupied
    /// trailing) so the core mints a pending token on the trailing
    /// command. The roster carries the genuine neutral raw capture
    /// (ordinary AXWindow/AXStandardWindow, non-modal, resizable, base
    /// layer, regular app) so the v4 builder emits a classifiable live
    /// fact; a missing raw entry would report nulls and the core would
    /// refuse `send-window-unknown`.
    fn roster_entry(window: ObservedWindow, spaces: Vec<u64>) -> RosterEntry {
        RosterEntry {
            observed: Some(window),
            spaces,
            fullscreen_meta: false,
            minimized_meta: false,
            hidden_meta: false,
            excluded: None,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        }
    }

    fn retained_occupied_trailing() -> RetainedCore {
        let mut retained = RetainedCore::default();
        let window = observed(9);
        retained.apps.insert(
            4243,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![window],
                focused_wid: None,
                roster: std::collections::BTreeMap::from([(9, roster_entry(window, vec![277]))]),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        retained.window_domains.insert(
            crate::runtime::window_id(4243, 9),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(277)),
            },
        );
        retained
    }

    #[allow(clippy::type_complexity)]
    fn harness(
        ctx: Arc<NativeContext>,
        retained: RetainedCore,
    ) -> (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Inbox>,
        Arc<Mutex<Vec<FocusRequest>>>,
    ) {
        (
            session(),
            retained,
            test_shared(ctx),
            Arc::new(Inbox::new(16)),
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    /// Fixture admission gate: the genuine handshake binds the session
    /// (adapter-native incarnation, production macOS caps, minted
    /// generation) and the handbuilt roster carries explicit native-shaped
    /// ordinary raw facts homed to the test context. A report `Accepted`
    /// alone never proves admission; the caller must also observe the
    /// Controlled row (desired window, retained transfer, or preview slot).
    fn assert_fixture_admitted(session: &WorldSession, retained: &RetainedCore) {
        assert!(session.connection_bound(), "genuine handshake binds");
        assert!(
            session.incarnation().starts_with("macos-"),
            "bound incarnation is adapter-native"
        );
        assert!(
            crate::world_connection::macos_capabilities().host_owned,
            "macOS Spaces stay host-owned"
        );
        assert!(
            !session.sequencer().bound_generation().is_empty(),
            "handshake mints a bound generation"
        );
        for (pid, wid, sid) in [(4243u32, 9u32, 277u64)] {
            let app = retained.apps.get(&pid).expect("fixture app retained");
            let entry = app.roster.get(&wid).expect("fixture roster row retained");
            assert_eq!(entry.spaces, vec![sid], "fixture row homed to its Space");
            assert_eq!(entry.role.as_deref(), Some(crate::classifier::ROLE_WINDOW));
            assert_eq!(
                entry.subrole.as_deref(),
                Some(crate::classifier::SUBROLE_STANDARD)
            );
            assert!(!app.roster_incomplete, "fixture holds nothing");
        }
        if let Some(app) = retained.apps.get(&4242) {
            let entry = app
                .roster
                .get(&7)
                .expect("send subject roster row retained");
            assert_eq!(entry.spaces, vec![1], "send subject homed current");
            assert_eq!(entry.role.as_deref(), Some(crate::classifier::ROLE_WINDOW));
        }
    }

    #[test]
    fn trailing_select_dispatch_binds_operation_identity_to_claim() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_occupied_trailing());
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 }),
            true,
        );
        let guard = lock_shared(&shared);
        let claim = guard.select_pending.front().cloned().expect("claim stored");
        assert!(
            claim
                .operation_id
                .as_deref()
                .is_some_and(|op| op.starts_with("op-")),
            "planner-owned operation identity binds"
        );
        assert_eq!(guard.diagnostics.input_select_enqueued, 1);
    }

    #[test]
    fn published_claim_arms_pump_wake_and_take_consumes_once() {
        // Core 1-8 ms -> claim-drain wait: the engine admission path sets the
        // shared claim wake exactly when it stores an actionable claim, so the
        // main pump drains on the next 50 ms iteration instead of the next
        // 250 ms tick. Fully synchronous (direct engine call plus the real
        // handle take, no sleeps): the flag is the publication signal, not a
        // timing guess. The actual publication flag matters here, not just a
        // gate boolean: this test drives the real `store` and the real take.
        struct RefusingAx;
        impl crate::runtime_backend::AxBackendFactory for RefusingAx {
            fn create(
                &self,
                _pid: u32,
            ) -> Result<Box<dyn crate::runtime_backend::AxBackend>, String> {
                Err("test stub creates nothing".to_owned())
            }
        }
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_occupied_trailing());
        let handle = crate::runtime::RuntimeHandle {
            shared: Arc::clone(&shared),
            inbox: Arc::clone(&inbox),
            focus_queue: Arc::clone(&focus_queue),
            ax_factory: Arc::new(RefusingAx),
        };
        assert!(
            !handle.take_select_claim_pending(),
            "idle slot wakes nothing: steady state runs no extra body"
        );
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 }),
            true,
        );
        let first_op = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("claim stored")
            .operation_id
            .clone();
        assert!(
            first_op.as_deref().is_some_and(|op| op.starts_with("op-")),
            "stored claim carries its operation identity"
        );
        assert!(
            handle.take_select_claim_pending(),
            "storing an actionable claim arms the pump wake once"
        );
        assert!(
            !handle.take_select_claim_pending(),
            "wake consumed at most once per publication: no spurious extra body"
        );
        // FIFO: a concurrent press while the first claim is queued
        // enqueues behind it (never refuses, never replaces), so the
        // first claim (and its already-consumed wake) stands undisturbed
        // at the front with its own operation identity.
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 1 }),
            true,
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_refused,
            0,
            "concurrent press queues: no busy admission loss"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_enqueued,
            2,
            "both presses enqueue in admission order"
        );
        {
            let guard = lock_shared(&shared);
            assert_eq!(guard.select_pending.len(), 2, "queue holds both claims");
            assert_eq!(
                guard
                    .select_pending
                    .front()
                    .expect("first claim kept")
                    .operation_id,
                first_op,
                "FIFO keeps the first claim at the front: no silent replacement"
            );
            let second_op = guard
                .select_pending
                .back()
                .expect("second claim queued")
                .operation_id
                .clone();
            assert!(
                second_op.as_deref().is_some_and(|op| op.starts_with("op-")),
                "queued press carries its own operation identity"
            );
            assert_ne!(second_op, first_op, "each command mints a unique op id");
        }
        // Draining dequeues oldest-first: the first claim drains, the
        // second stays queued with the wake re-armed, then drains next.
        let claimed = handle
            .claim_select_for_drain()
            .expect("pending claim drains");
        assert_eq!(claimed.operation_id, first_op);
        assert!(
            handle.take_select_claim_pending(),
            "queued remainder keeps the wake armed past the claim"
        );
        handle.release_select_after_drain();
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 1 }),
            true,
        );
        assert!(
            !lock_shared(&shared).select_pending.is_empty(),
            "lane free after drain admits again"
        );
        assert!(
            handle.take_select_claim_pending(),
            "later publication re-arms the wake"
        );
    }

    #[test]
    fn mixed_burst_true_ingress_queues_fifo_behind_saturated_data_lane() {
        // Ordered-ingress regression: a saturated bounded data lane must
        // never discard authorized user intent. Selects/sends ride the
        // control lane (same strict FIFO queue, bypassing data
        // saturation); observations stay bounded and drop counted. Only
        // the drain serializes actuation. Deterministic: no threads, no
        // sleeps — the real public submit paths enqueue, then jobs drain
        // in FIFO recv order through the real `process_job`.
        struct StubAx;
        impl crate::runtime_backend::AxBackendFactory for StubAx {
            fn create(
                &self,
                _pid: u32,
            ) -> Result<Box<dyn crate::runtime_backend::AxBackend>, String> {
                Err("test stub creates nothing".to_owned())
            }
        }
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        // Saturate the bounded data lane with stale observations (unknown
        // app: they will-drop at the emission gate and stale-reject on
        // processing, so they prove saturation without disturbing the
        // burst).
        for seq in 0..16 {
            assert!(inbox.try_send_data(EngineJob::Observation {
                pid: 9999,
                generation: 1,
                seq,
                windows: Vec::new(),
                focused_wid: None,
            }));
        }
        assert!(
            !inbox.try_send_data(EngineJob::Observation {
                pid: 9999,
                generation: 1,
                seq: 999,
                windows: Vec::new(),
                focused_wid: None,
            }),
            "data lane stays bounded: observations drop counted when full"
        );
        let handle = crate::runtime::RuntimeHandle {
            shared: Arc::clone(&shared),
            inbox: Arc::clone(&inbox),
            focus_queue: Arc::clone(&focus_queue),
            ax_factory: Arc::new(StubAx),
        };
        // Mixed burst through the REAL public ingress (the exact functions
        // production calls): taps, a focus step, a directional move step,
        // and a send.
        use crate::runtime_types::SubmitOutcome;
        assert_eq!(
            handle.submit_input_action(InputSemanticAction::WorkspaceSelect { index: 2 }),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            handle.submit_input_action(InputSemanticAction::FocusStep {
                direction: crate::runtime_types::InputDirection::Left,
            }),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            handle.submit_input_action(InputSemanticAction::MoveStep {
                direction: crate::runtime_types::InputDirection::Right,
            }),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            handle.submit_input_action(InputSemanticAction::WorkspaceSelect { index: 1 }),
            SubmitOutcome::Accepted
        );
        assert_eq!(
            handle.submit_input_action(InputSemanticAction::WorkspaceSend {
                // Window 4242_7 lives on workspace 1: send-to-2 is the
                // meaningful move (send-to-1 would be a truthful core
                // unchanged, not an ingress loss). Covers the real move
                // plus follow-view legs behind selects in one FIFO drain.
                index: 2,
                follow: true,
            }),
            SubmitOutcome::Accepted
        );
        // Drain the saturated observations plus the FIRST select through
        // the real engine job processor, then pop/hold its claim so the
        // first drain is already in progress (select_busy) while the rest
        // of the burst still arrives. The remainder must still reach the
        // core in FIFO order while busy - the drain serializes actuation
        // only, never admission.
        let stop = AtomicBool::new(false);
        let mut kinds: Vec<&str> = Vec::new();
        for _ in 0..17 {
            let job = inbox.recv(&stop).expect("queued job dequeues");
            kinds.push(match &job {
                EngineJob::Observation { .. } => "obs",
                EngineJob::InputAction { .. } => "input",
                _ => "other",
            });
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            kinds,
            vec![
                "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs",
                "obs", "obs", "obs", "obs", "input",
            ],
            "first select arrives FIFO behind the saturated lane"
        );
        assert_eq!(
            lock_shared(&shared).select_pending.len(),
            1,
            "first select claim queued before its drain starts"
        );
        // Pop/hold the first claim: the drain is now in progress.
        let held = handle.claim_select_for_drain().expect("first drains");
        let held_op = held.operation_id.clone().expect("operation rides");
        assert!(
            lock_shared(&shared).select_busy,
            "popped claim marks the drain in progress"
        );
        assert!(
            handle.claim_select_for_drain().is_none(),
            "concurrent pop serializes while the first drain is held"
        );
        // Process the remaining mixed inputs WHILE the first drain is
        // held: the real engine path must still deliver every command to
        // the core (selects queue behind the busy lane, steps forward).
        for _ in 0..4 {
            let job = inbox.recv(&stop).expect("queued job dequeues");
            kinds.push(match &job {
                EngineJob::Observation { .. } => "obs",
                EngineJob::InputAction { .. } => "input",
                _ => "other",
            });
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            kinds,
            vec![
                "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs", "obs",
                "obs", "obs", "obs", "obs", "input", "input", "input", "input", "input",
            ],
            "engine processes the burst FIFO behind the saturated lane"
        );
        // No admission loss while busy: every authorized command reached
        // the core even though the first drain never released.
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_refused,
            0,
            "no busy admission loss"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_enqueued,
            3,
            "both taps and the send enqueue in admission order while busy"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.input_forwarded,
            2,
            "focus and move steps dispatch to core (non-select lane) in FIFO order while busy"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.data_dropped_full,
            0,
            "direct-inbox saturation proves boundedness via try_send_data=false above; submit-path drop accounting stays untouched"
        );
        // Sequence/order/no loss/no duplication across the held claim plus
        // the busy-arrived queue: oldest-first, each with a unique
        // increasing operation identity, actions in submission order.
        let queued: Vec<ClaimedSelect> = lock_shared(&shared)
            .select_pending
            .iter()
            .cloned()
            .collect();
        assert_eq!(queued.len(), 2, "remainder queues behind the held drain");
        assert_eq!(
            held.action,
            InputSemanticAction::WorkspaceSelect { index: 2 }
        );
        let mut ops: Vec<String> = vec![held_op.clone()];
        ops.extend(
            queued
                .iter()
                .map(|claim| claim.operation_id.clone().expect("operation rides")),
        );
        let mut seqs: Vec<u64> = ops
            .iter()
            .map(|op| {
                op.strip_prefix("op-")
                    .expect("op-<seq> identity")
                    .parse()
                    .expect("numeric seq")
            })
            .collect();
        let sorted = {
            let mut s = seqs.clone();
            s.sort_unstable();
            s
        };
        assert_eq!(seqs, sorted, "operation identities increase in queue order");
        seqs.dedup();
        assert_eq!(seqs.len(), 3, "no loss, no duplication");
        assert_eq!(
            queued
                .iter()
                .map(|claim| claim.action.clone())
                .collect::<Vec<_>>(),
            vec![
                InputSemanticAction::WorkspaceSelect { index: 1 },
                InputSemanticAction::WorkspaceSend {
                    index: 2,
                    follow: true,
                },
            ],
            "mixed-source FIFO preserves submission order behind the held drain"
        );
        // Still busy: no second pop until the held drain releases.
        assert!(
            handle.claim_select_for_drain().is_none(),
            "busy lane holds the remainder until release"
        );
        assert_eq!(
            held.operation_id.as_deref(),
            Some(ops[0].as_str()),
            "held first claim preserved while the burst arrived"
        );
        // Release unlocks the oldest pending order with no replay.
        handle.release_select_after_drain();
        assert!(
            !lock_shared(&shared).select_busy,
            "release clears the in-progress drain"
        );
        let second = handle.claim_select_for_drain().expect("second drains");
        assert_eq!(second.operation_id.as_deref(), Some(ops[1].as_str()));
        handle.release_select_after_drain();
        let third = handle.claim_select_for_drain().expect("third drains");
        assert_eq!(third.operation_id.as_deref(), Some(ops[2].as_str()));
        handle.release_select_after_drain();
        assert!(handle.claim_select_for_drain().is_none(), "nothing replays");
        assert!(
            lock_shared(&shared).select_pending.is_empty(),
            "queue fully drained"
        );
    }

    #[test]
    fn saturated_lane_indicator_submit_rides_control_lane() {
        // Saturated data lane must not discard an authorized indicator
        // click: the segment click rides the control lane through the real
        // public submit path, then the real engine job processor enqueues
        // its claim.
        struct StubAx;
        impl crate::runtime_backend::AxBackendFactory for StubAx {
            fn create(
                &self,
                _pid: u32,
            ) -> Result<Box<dyn crate::runtime_backend::AxBackend>, String> {
                Err("test stub creates nothing".to_owned())
            }
        }
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        for seq in 0..16 {
            assert!(inbox.try_send_data(EngineJob::Observation {
                pid: 9999,
                generation: 1,
                seq,
                windows: Vec::new(),
                focused_wid: None,
            }));
        }
        assert!(
            !inbox.try_send_data(EngineJob::Observation {
                pid: 9999,
                generation: 1,
                seq: 999,
                windows: Vec::new(),
                focused_wid: None,
            }),
            "data lane stays bounded"
        );
        let handle = crate::runtime::RuntimeHandle {
            shared: Arc::clone(&shared),
            inbox: Arc::clone(&inbox),
            focus_queue: Arc::clone(&focus_queue),
            ax_factory: Arc::new(StubAx),
        };
        use crate::runtime_types::SubmitOutcome;
        assert_eq!(
            handle.submit_indicator_select(
                InputSemanticAction::WorkspaceSelect { index: 2 },
                277,
                1,
                9,
            ),
            SubmitOutcome::Accepted,
            "indicator click bypasses data saturation"
        );
        let stop = AtomicBool::new(false);
        for _ in 0..17 {
            let job = inbox.recv(&stop).expect("queued job dequeues");
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_refused,
            0,
            "indicator admitted while saturated"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.input_select_enqueued,
            1,
            "indicator claim queued"
        );
        let queued: Vec<ClaimedSelect> = lock_shared(&shared)
            .select_pending
            .iter()
            .cloned()
            .collect();
        assert_eq!(queued.len(), 1, "one indicator claim stored");
        assert_eq!(
            queued[0].action,
            InputSemanticAction::WorkspaceSelect { index: 2 }
        );
        assert_eq!(queued[0].expected_sid, Some(277));
    }

    #[test]
    fn trailing_select_production_wrapper_armed() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_occupied_trailing());
        handle_select_action(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 }),
        );
        let guard = lock_shared(&shared);
        assert!(
            !guard.select_pending.is_empty(),
            "armed production stores the trailing claim"
        );
    }

    #[test]
    fn selection_focus_binds_through_real_dispatch_with_live_binding() {
        // Positive real-routing proof: a remembered-leaf view change mints
        // a retained `focusRequest` through the real core, and the real
        // dispatch binds the deferred intent with the genuine expected SID
        // taken from the reply's live native binding (never a guessed
        // Space). The full production cycle runs between the legs: the away
        // select drains through the scripted native row, stages its proven
        // attempt, and settles on the moved observation — then the return
        // mints its own remembered focus and supersedes the away intent.
        use crate::workspace_select::{SelectGates, SelectionState, drain_claimed_select_with};
        use tiler_core::world_report::TransferKind;
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        // Fixture admission precedes commands: report Accepted is not
        // enough, the Controlled row below (retained View transfer plus
        // live native binding) proves the core admitted the fixture.
        assert_fixture_admitted(&session, &retained);
        // Away: numbered 2 moves the view to the other adopted workspace.
        // Its sole window is remembered there, so the reply mints the
        // away outcome and the dispatch binds the away intent with the
        // genuine SID. Foreground evidence rides the shared cell exactly
        // like the tile pump publishes it every tick (bind reads it as
        // its stability baseline). The drain + settle below proves the
        // away arrival; the return leg then supersedes this intent
        // (counted) with its own remembered focus.
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 }),
            true,
        );
        let away = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("away select stores its operation");
        let away_op = away.operation_id.clone().expect("operation rides");
        let away_pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("away remembered window binds deferred focus");
        assert_eq!(away_pending.operation_id, away_op);
        assert_eq!(away_pending.window.0, "4243_9");
        assert_eq!(away_pending.expected_sid, 277);
        assert_eq!(away_pending.prior_focus.as_deref(), Some("4242_7"));
        assert_eq!(away_pending.prior_frontmost, Some(4242));
        // Drain the away move through the scripted native row and stage the
        // proven raw attempt, exactly like the main-thread drain. An
        // unsettled outstanding flight is not a proven arrival, so the
        // return leg only runs after the settle below.
        let mut backend = FakeBackend::new(&[1, 277], 1, &[]);
        let mut state = SelectionState::default();
        let gates = SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        };
        let moved = drain_claimed_select_with(&mut state, &mut backend, away, &gates, true)
            .expect("away drains to its binding");
        assert_eq!((moved.pre, moved.post), (Some(1), Some(277)));
        process_job(
            EngineJob::ViewAttempt {
                operation_id: away_op.clone(),
                setter: FrameSetter::Accepted,
                pre_sid: moved.pre,
                post_sid: moved.post,
                baseline_sid: moved.effect.baseline,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        // Settle the moved observation through the production dispatch path.
        let moved_ctx = Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, true)],
            skipped: Vec::new(),
        });
        lock_shared(&shared).native_context = Some(Arc::clone(&moved_ctx));
        lock_shared(&shared).topology_revision = moved_ctx.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&moved_ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("moved observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "native echo settles the away transfer"
        );
        // The main-thread drain claims the pending select, freeing the lane
        // for the next arm exactly like production. Foreground evidence is
        // unchanged since the away dispatch (same tick shape).
        lock_shared(&shared).select_pending.clear();
        lock_shared(&shared).select_busy = false;
        // Back: numbered 1 returns to the remembered home. The newer
        // switch supersedes the away intent (counted — no completion ever
        // released it in this lane) and binds its own with the genuine
        // home SID.
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 1 }),
            true,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("remembered-leaf return binds deferred focus");
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dropped_full,
            1,
            "unreleased away intent superseded by the newer switch"
        );
        // The bound op names a retained View transfer in the live session.
        let transfer = session
            .engine_mut()
            .retained_transfers()
            .into_iter()
            .find(|target| {
                target.operation_id == pending.operation_id
                    && matches!(target.kind, TransferKind::View)
            })
            .expect("bound op retains its View transfer");
        // The expected SID is exactly the reply's live native binding for
        // the request scope — parsed, never guessed.
        let reply = session
            .sequencer()
            .last_reply()
            .expect("applied reply retained");
        let req = reply
            .focus_request
            .as_ref()
            .expect("reply carries the retained focus request");
        use tiler_core::world_report::FocusRequestCause;
        assert_eq!(req.cause, FocusRequestCause::ExplicitCommand);
        let binding = reply
            .workspaces
            .iter()
            .find(|row| row.output == req.output && row.id == req.workspace)
            .and_then(|row| row.native_binding.clone())
            .expect("reply publishes the live native binding");
        let expected: u64 = binding
            .strip_prefix("space-")
            .expect("space-<sid> vocabulary")
            .parse()
            .expect("numeric SID");
        assert_eq!(pending.expected_sid, expected);
        assert_eq!(
            pending.prior_focus.as_deref(),
            Some("4242_7"),
            "bind-time observed focus rides as the worker baseline"
        );
        assert_eq!(
            pending.prior_frontmost,
            Some(4242),
            "bind-time foreground rides as the stability baseline"
        );
        // The window is exactly the reply's correlated retained request.
        let seq: u64 = pending
            .operation_id
            .strip_prefix("op-")
            .expect("op-<seq> identity")
            .parse()
            .expect("numeric seq");
        assert_eq!(pending.command_seq, seq);
        assert_eq!(req.cmd_seq, Some(seq));
        assert_eq!(req.target, pending.window);
        assert_eq!(pending.focus_id, req.id);
        assert_eq!(pending.issued_revision, req.issued_revision);
        assert_eq!(pending.output, req.output);
        assert_eq!(pending.workspace, req.workspace);
        let after = req
            .after_shown
            .as_ref()
            .expect("selection request carries frozen afterShown");
        assert_eq!(after.op, pending.operation_id);
        assert_eq!(&pending.after_shown, after);
        // The away leg's superseded floor never blocks the newer bind.
        assert!(
            lock_shared(&shared).selection_focus_floor_seq <= seq,
            "floor tracks the binding command"
        );
    }

    struct FakeFocusBackend {
        fresh: Option<(u32, Option<u32>)>,
        post: Option<(u32, Option<u32>)>,
        reads: usize,
        dispatches: usize,
        completion: crate::runtime_types::AxCompletion,
    }

    impl crate::runtime_backend::AxBackend for FakeFocusBackend {
        fn apply_frame(
            &mut self,
            _cmd: &crate::runtime::FrameCommand,
        ) -> crate::runtime_types::AxCompletion {
            crate::runtime_types::AxCompletion::Failed {
                reason: "fake no frames".to_owned(),
            }
        }
        fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
            Ok(Vec::new())
        }
        fn selected_focus_evidence(&mut self) -> Option<(u32, Option<u32>)> {
            let value = if self.reads == 0 {
                self.fresh
            } else {
                self.post
            };
            self.reads += 1;
            value
        }
        fn dispatch_focus(
            &mut self,
            _cmd: &crate::runtime_types::FocusCommand,
        ) -> crate::runtime_types::AxCompletion {
            self.dispatches += 1;
            self.completion.clone()
        }
    }

    /// Genuine away-bind fixture for the retained-selection focus tests:
    /// Native config (arms the owner slot) plus the live owner app record
    /// (bind carries `Some` generation so the release matches it) with
    /// bind-time foreground evidence, then one observation bootstrap and
    /// one away select through the production producer (`handle_select_action_with`
    /// into `dispatch_report`, whose byte codec round-trips both directions
    /// before the sequencer accepts). Returns the bound away state.
    #[allow(clippy::type_complexity)]
    fn selection_away_fixture() -> (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Inbox>,
        Arc<Mutex<Vec<FocusRequest>>>,
        Arc<NativeContext>,
    ) {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        lock_shared(&shared).config.topology_mode = crate::runtime_types::TopologyMode::Native;
        close_app(&shared, 4243, 1);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert_fixture_admitted(&session, &retained);
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 }),
            true,
        );
        (session, retained, shared, inbox, focus_queue, ctx)
    }

    /// Genuine arrival settle for one bound away select: the real claim
    /// drains through the scripted native row, the proven attempt stages
    /// through the production job arm, and the moved observation settles
    /// through the production dispatch path. Returns the away operation id
    /// plus the drain-verified arrival Space.
    fn drain_settle_away(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    ) -> (String, Option<u64>) {
        use crate::workspace_select::{SelectGates, SelectionState, drain_claimed_select_with};
        use tiler_core::world_report::FrameSetter;
        let away = lock_shared(shared)
            .select_pending
            .front()
            .cloned()
            .expect("away select stored");
        let away_op = away.operation_id.clone().expect("operation rides");
        let mut backend = FakeBackend::new(&[1, 277], 1, &[]);
        let mut state = SelectionState::default();
        let gates = SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        };
        let moved = drain_claimed_select_with(&mut state, &mut backend, away, &gates, true)
            .expect("away drains to its binding");
        assert_eq!((moved.pre, moved.post), (Some(1), Some(277)));
        process_job(
            EngineJob::ViewAttempt {
                operation_id: away_op.clone(),
                setter: FrameSetter::Accepted,
                pre_sid: moved.pre,
                post_sid: moved.post,
                baseline_sid: moved.effect.baseline,
            },
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
        );
        let moved_ctx = Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, true)],
            skipped: Vec::new(),
        });
        lock_shared(shared).native_context = Some(Arc::clone(&moved_ctx));
        lock_shared(shared).topology_revision = moved_ctx.revision;
        match dispatch_report(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&moved_ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("moved observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "native echo settles the away transfer"
        );
        (away_op, moved.post)
    }

    struct RefusingFocusAx;

    impl crate::runtime_backend::AxBackendFactory for RefusingFocusAx {
        fn create(&self, _pid: u32) -> Result<Box<dyn crate::runtime_backend::AxBackend>, String> {
            Err("test stub creates nothing".to_owned())
        }
    }

    fn release_handle(
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    ) -> crate::runtime::RuntimeHandle {
        crate::runtime::RuntimeHandle {
            shared: Arc::clone(shared),
            inbox: Arc::clone(inbox),
            focus_queue: Arc::clone(focus_queue),
            ax_factory: Arc::new(RefusingFocusAx),
        }
    }

    /// Genuine skipped issuing plus two unchanged publications for the
    /// numbered away select: each report is built plus core-transacted
    /// through the byte codec yet never accepted/consumed, so the adapter
    /// binds nothing while the core retains the identical focus request.
    /// Returns the core-issued frozen request. Shared by the latest-only
    /// and settled-parent proofs (exactly these two uses).
    fn skipped_issuing_identical(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        ctx: &Arc<NativeContext>,
    ) -> tiler_core::world_report::FocusRequest {
        use crate::world_dispatch::codec_transact;
        let action = InputSemanticAction::WorkspaceSelect { index: 2 };
        let command = crate::world_command_map::select_command(&action, None)
            .expect("numbered select maps without an output hint");
        let clock_before = lock_shared(shared).clock.get();
        let cell_gen_before = lock_shared(shared).native_cell.generation();
        let issuing_req = {
            let ticket = session
                .sequencer()
                .try_begin_request(Some(command.clone()))
                .expect("flight opens for the skipped issuing");
            let report = build_dispatch_report(
                &mut *session,
                retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(ctx)),
                &[],
                ticket.observation_seq,
                ticket.request_seq,
                ticket.command_seq,
                ticket.command.clone(),
                None,
                &ticket.request_id,
            )
            .expect("skipped issuing builds");
            let outcome =
                codec_transact(&mut *session, &report).expect("skipped issuing transacts");
            let reply = match outcome {
                WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
                WorldOutcome::Duplicate(reply) => reply,
                WorldOutcome::Rejected { kind, .. } => panic!("skipped issuing rejected: {kind}"),
            };
            let req = reply
                .focus_request
                .clone()
                .expect("issuing reply carries the retained focus request");
            session.sequencer().clear_flight();
            req
        };
        for _ in 0..2 {
            let ticket = session
                .sequencer()
                .try_begin_request(None)
                .expect("flight opens for a skipped unchanged publication");
            let report = build_dispatch_report(
                &mut *session,
                retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(ctx)),
                &[],
                ticket.observation_seq,
                ticket.request_seq,
                ticket.command_seq,
                ticket.command.clone(),
                None,
                &ticket.request_id,
            )
            .expect("skipped unchanged builds");
            let outcome =
                codec_transact(&mut *session, &report).expect("skipped unchanged transacts");
            let reply = match outcome {
                WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
                WorldOutcome::Duplicate(reply) => reply,
                WorldOutcome::Rejected { kind, .. } => {
                    panic!("skipped unchanged rejected: {kind}")
                }
            };
            let req = reply
                .focus_request
                .clone()
                .expect("unchanged reply retains the focus request");
            assert_eq!(
                req.id, issuing_req.id,
                "skipped unchanged keeps the identical core focus id"
            );
            assert_eq!(
                req.target, issuing_req.target,
                "skipped unchanged keeps the identical core focus target"
            );
            assert_eq!(
                req, issuing_req,
                "skipped unchanged re-emits the identical retained request"
            );
            session.sequencer().clear_flight();
        }
        assert_eq!(lock_shared(shared).clock.get(), clock_before);
        assert_eq!(
            lock_shared(shared).native_cell.generation(),
            cell_gen_before
        );
        issuing_req
    }

    #[test]
    fn selection_focus_latest_only_releases_once_through_real_owner() {
        // Genuine latest-only proof: the producer select builds issuing plus
        // two unchanged reports as bytes and skips all three (never
        // dispatch_ticket/accept/consume), the first real dispatch binds the
        // deferred intent, a real owner arrival capture stamps it, the
        // production owner route dispatches the mocked setter exactly once
        // from the generated completion, and a target observation settles
        // the truthful feedback.
        use std::sync::atomic::AtomicBool;
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        lock_shared(&shared).config.topology_mode = crate::runtime_types::TopologyMode::Native;
        close_app(&shared, 4243, 1);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert_fixture_admitted(&session, &retained);
        let issuing_req = skipped_issuing_identical(&mut session, &mut retained, &shared, &ctx);
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "skipped publications bind nothing"
        );
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        // First latest delivery: the only publication that ever runs the
        // production accept/consume path (observation-only, no new command).
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("latest observation must apply, got {other:?}"),
        }
        // Production claim store for the bound operation so the drain below
        // settles the core-issued parent transfer.
        {
            let op = lock_shared(&shared)
                .selection_focus_pending
                .clone()
                .expect("latest binds the deferred intent")
                .operation_id
                .clone();
            let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 });
            bind_select_plan(&mut session, &ctx, &mut claim, &op);
            store_select_claim(&shared, claim, Some(&op));
        }
        let away_op = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("away select stored")
            .operation_id
            .expect("operation rides");
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("first dispatch binds the deferred intent");
        assert_eq!(pending.operation_id, away_op);
        assert_eq!(pending.window.0, "4243_9");
        assert_eq!(pending.after_shown.op, away_op);
        assert_eq!(pending.adapter_incarnation, session.incarnation());
        assert_eq!(
            pending.core_generation,
            session.sequencer().bound_generation()
        );
        assert_eq!(lock_shared(&shared).topology_revision, ctx.revision);
        let reply = session
            .sequencer()
            .last_reply()
            .expect("applied reply retained");
        let req = reply
            .focus_request
            .as_ref()
            .expect("reply carries the retained focus request");
        assert_eq!(req.id, pending.focus_id);
        assert_eq!(req.target, pending.window);
        assert_eq!(
            req.id, issuing_req.id,
            "latest delivers the identical core-issued focus id"
        );
        assert_eq!(
            req.target, issuing_req.target,
            "latest delivers the identical core-issued focus target"
        );
        assert_eq!(
            req, &issuing_req,
            "latest delivers the identical core-issued request"
        );
        assert!(reply.based_on_observation_seq >= pending.after_shown.obs);
        // Genuine arrival settle through the real drain plus native echo.
        let (settled_op, post) =
            drain_settle_away(&mut session, &mut retained, &shared, &inbox, &focus_queue);
        assert_eq!(settled_op, away_op);
        assert_eq!(post, Some(277));
        // Mock applies the actual produced layout frames, then the
        // production scoped-observation handler observes that mock native
        // geometry so the roster-observed live rects match the frozen
        // actual layout rects for the exact-frame gate. No direct roster
        // mutation: the production `ScopedObservation` path converges the
        // roster, then dispatches the genuine reconciling report.
        {
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let live_reply_now = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply");
            let mut per_pid: std::collections::BTreeMap<u32, Vec<crate::runtime::ObservedWindow>> =
                std::collections::BTreeMap::new();
            let mut meta_per_pid: std::collections::BTreeMap<
                u32,
                Vec<crate::runtime_types::ScopedWindowMeta>,
            > = std::collections::BTreeMap::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                let live_sid = live_reply_now
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("produced layout carries the frozen geometry");
                per_pid
                    .entry(mpid)
                    .or_default()
                    .push(crate::runtime::ObservedWindow {
                        wid: mwid,
                        x,
                        y,
                        w,
                        h,
                        floating: false,
                        fullscreen: false,
                        hints: WindowSizeHints::none(),
                    });
                meta_per_pid.entry(mpid).or_default().push(
                    crate::runtime_types::ScopedWindowMeta {
                        wid: mwid,
                        spaces: vec![live_sid],
                        fullscreen: false,
                        minimized: false,
                        hidden: false,
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                    },
                );
            }
            for (mpid, windows) in per_pid {
                let meta = meta_per_pid.remove(&mpid).unwrap_or_default();
                let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
                let (generation, seq_last, revision) = {
                    let guard = lock_shared(&shared);
                    let app = guard.apps.get(&mpid).expect("capture owner stays live");
                    (
                        app.generation,
                        retained
                            .apps
                            .get(&mpid)
                            .map(|app| app.seq_last)
                            .unwrap_or(0),
                        guard.topology_revision,
                    )
                };
                process_job(
                    EngineJob::ScopedObservation {
                        pid: mpid,
                        generation,
                        seq: seq_last + 1,
                        snapshot: crate::runtime_types::ScopedBackendSnapshot {
                            windows,
                            focused_wid: None,
                            focus_cg_owned: false,
                            focus_evidence: None,
                            sys_focus: None,
                            global_focus: None,
                            meta,
                            excluded: 0,
                            excluded_reasons: Vec::new(),
                            cg_complete: true,
                            cg_owner_wids: wids,
                            cg_alive: Vec::new(),
                            revision,
                        },

                        trace_id: 0,
                        intake_us: 0,
                        enqueue_us: 0,
                    },
                    &mut session,
                    &mut retained,
                    &shared,
                    &inbox,
                    &focus_queue,
                    8,
                );
            }
        }
        // Real owner arrival capture for every frozen member, through the
        // production worker/engine path.
        struct LocalCaptureBackend {
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl crate::runtime_backend::AxBackend for LocalCaptureBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture gate never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let live_reply = session
            .sequencer()
            .last_reply()
            .expect("settle keeps the latest reply");
        let mut reads = Vec::new();
        for member in &pending.after_shown.members {
            let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0) else {
                panic!("frozen member parses");
            };
            let live_sid = live_reply
                .workspaces
                .iter()
                .find(|ws| ws.output == member.out && ws.id == member.ws)
                .and_then(|ws| ws.native_binding.clone())
                .and_then(|binding| crate::workspace_select::parse_native_binding(binding.as_str()))
                .expect("latest publishes the live member binding");
            let frame = retained
                .apps
                .get(&mpid)
                .and_then(|app| app.roster.get(&mwid))
                .and_then(|entry| entry.observed)
                .map(|live| (live.x, live.y, live.w, live.h))
                .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                .expect("frozen member has readable live geometry");
            reads.push(crate::runtime_types::BracketRead {
                wid: mwid,
                owner_pid: Some(mpid),
                before_minimized: Some(false),
                before_hidden: Some(false),
                spaces: vec![live_sid],
                frame: Some(frame),
                after_minimized: Some(false),
                after_hidden: Some(false),
            });
        }
        let capture_inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<(String, u64, String, String, u64, u64, u64, u64)> = None;
        let mut pids: Vec<u32> = reads
            .iter()
            .filter_map(|read| read.owner_pid)
            .collect::<std::collections::BTreeSet<u32>>()
            .into_iter()
            .collect();
        pids.sort();
        let mut notified = 0;
        for pid in pids {
            let generation = lock_shared(&shared)
                .apps
                .get(&pid)
                .map(|record| record.generation)
                .expect("capture owner stays live");
            let mut backend = LocalCaptureBackend {
                reads: reads.clone(),
            };
            if crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                pid,
                generation,
                &shared,
                &cell,
                &capture_inbox,
                &mut last_attempt,
            ) {
                notified += 1;
            }
        }
        assert!(notified > 0, "worker owes at least one genuine capture");
        for _ in 0..notified {
            let stop = AtomicBool::new(false);
            let job = capture_inbox.recv(&stop).expect("capture queued");
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        let stamped = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("capture keeps the intent for release");
        let stamp = stamped
            .proven_stamp
            .clone()
            .expect("capture stamps the intent");
        {
            let guard = lock_shared(&shared);
            assert_eq!(stamp.obs, guard.clock.get());
            assert_eq!(stamp.revision, guard.topology_revision);
            assert_eq!(stamp.context_generation, guard.native_cell.generation());
        }
        for member in &pending.after_shown.members {
            assert!(
                lock_shared(&shared)
                    .arrival_captures
                    .contains_key(&member.win.0),
                "every frozen member retains a genuine capture"
            );
        }
        // Arrival capture auto-releases by design (apply_arrival_capture ->
        // reevaluate_selection_release): the bound intent stays with
        // owner_queued set until the real owner result retires it, so the
        // drain-verified callback below must report NoPending, never a
        // second release.
        {
            let auto = lock_shared(&shared)
                .selection_focus_pending
                .clone()
                .expect("auto-release keeps the intent bound");
            assert!(auto.owner_queued, "auto-release marks the intent queued");
            assert_eq!(auto.operation_id, away_op);
            assert_eq!(auto.focus_id, pending.focus_id);
            assert!(
                auto.proven_stamp.is_some(),
                "auto-release keeps the proven stamp"
            );
        }
        {
            let queued = crate::runtime_shared::lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "auto-release queues exactly once");
            assert_eq!((queued[0].pid, queued[0].wid), (4243, 9));
            assert_eq!(queued[0].generation, 1);
        }
        // Duplicate drain-verified callback reports NoPending instead of a
        // second release; the queued intent stays bound for the owner leg.
        let handle = release_handle(&shared, &inbox, &focus_queue);
        assert_eq!(
            handle.release_selection_focus(Some(&away_op), post, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "duplicate never retires the queued intent"
        );
        assert_eq!(
            crate::runtime_shared::lock_focus(&focus_queue).len(),
            1,
            "duplicate queues nothing more"
        );
        let cmd = lock_shared(&shared)
            .apps
            .get(&4243)
            .and_then(|app| app.focus.take())
            .expect("owner slot carries the release");
        assert_eq!(cmd.window, "4243_9");
        let close = cmd.close.clone().expect("unified close route");
        assert_eq!(close.close_id, pending.focus_id);
        assert_eq!(close.issued_revision, pending.issued_revision);
        // Actual mocked owner setter plus its generated completion.
        let revision = lock_shared(&shared).topology_revision;
        let mut fake = FakeFocusBackend {
            fresh: Some((4242, Some(7))),
            post: Some((4243, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 3 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut fake, &cmd, &close, 4243, 1, revision, &shared,
        );
        assert_eq!(fake.dispatches, 1, "owner dispatches exactly once");
        assert_eq!(fake.reads, 2, "fresh baseline plus post read");
        match &job {
            EngineJob::CloseFocusResult {
                setter,
                reason,
                pre,
                post,
                foreground_proven,
                close_id,
                target,
                issued_rev,
                ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::Accepted
                );
                assert_eq!(*reason, tiler_core::world_report::VisibilityReason::Empty);
                assert_eq!(
                    *pre,
                    Some(tiler_core::directional::WindowId("4242_7".to_owned()))
                );
                assert_eq!(
                    *post,
                    Some(tiler_core::directional::WindowId("4243_9".to_owned()))
                );
                assert!(*foreground_proven);
                assert_eq!(close_id, &pending.focus_id);
                assert_eq!(*issued_rev, pending.issued_revision);
                assert_eq!(*target, pending.window);
            }
            _ => panic!("expected CloseFocusResult"),
        }
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("accepted stages").id,
            pending.focus_id
        );
        // The staged row is truthful generated feedback: exact stamp plus
        // the full frozen arrival set with valid visible rows. This row
        // alone never settles the core (Accepted/Proven retains until
        // target equality); the genuine target observation below does.
        {
            let staged = session
                .peek_close_feedback()
                .expect("accepted stages")
                .clone();
            assert_eq!(staged.target, pending.window);
            assert_eq!(staged.revision, pending.issued_revision);
            assert_eq!(staged.post, Some(pending.window.clone()));
            assert!(staged.foreground_proven);
            let mut frozen: Vec<String> = pending
                .after_shown
                .members
                .iter()
                .map(|m| m.win.0.clone())
                .collect();
            frozen.sort();
            let mut arrived: Vec<String> =
                staged.arrival.iter().map(|b| b.window.0.clone()).collect();
            arrived.sort();
            assert_eq!(
                arrived, frozen,
                "generated feedback covers the full frozen set"
            );
            assert!(
                !staged.arrival.is_empty(),
                "generated feedback carries valid rows"
            );
            for bracket in &staged.arrival {
                assert!(
                    bracket.before.visibility.visible == Some(true)
                        && bracket.before.lifetime_matched == Some(true),
                    "arrival before leg is valid"
                );
                assert!(
                    bracket.after.visibility.visible == Some(true)
                        && bracket.after.lifetime_matched == Some(true),
                    "arrival after leg is valid"
                );
            }
        }
        // Genuine target observation through the production ScopedObservation
        // path: the owner thread reports the target focused with full
        // workspace+AX+CG agreement on the current domain, so the engine
        // promotes `retained.observed_focus` to the target (the
        // CloseFocusResult handler above never touches observed focus; only
        // this observation does). Its inner dispatch then drains the staged
        // row with `focused_global == target`, which is what actually
        // clears the core pending request via target equality.
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4243),
            frontmost_bundle: None,
            ax_focus: None,
        });
        {
            let live_reply = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply")
                .clone();
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let mut windows = Vec::new();
            let mut meta = Vec::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                if mpid != 4243 {
                    continue;
                }
                let live_sid = live_reply
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("frozen member has readable live geometry");
                windows.push(crate::runtime::ObservedWindow {
                    wid: mwid,
                    x,
                    y,
                    w,
                    h,
                    floating: false,
                    fullscreen: false,
                    hints: WindowSizeHints::none(),
                });
                meta.push(crate::runtime_types::ScopedWindowMeta {
                    wid: mwid,
                    spaces: vec![live_sid],
                    fullscreen: false,
                    minimized: false,
                    hidden: false,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                });
            }
            assert!(!windows.is_empty(), "target owner holds frozen members");
            let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
            assert!(wids.contains(&9), "target wid rides the frozen owner set");
            let (generation, seq_last, revision) = {
                let guard = lock_shared(&shared);
                let generation = guard
                    .apps
                    .get(&4243)
                    .expect("capture owner stays live")
                    .generation;
                let seq_last = retained
                    .apps
                    .get(&4243)
                    .map(|app| app.seq_last)
                    .unwrap_or(0);
                (generation, seq_last, guard.topology_revision)
            };
            process_job(
                EngineJob::ScopedObservation {
                    pid: 4243,
                    generation,
                    seq: seq_last + 1,
                    snapshot: crate::runtime_types::ScopedBackendSnapshot {
                        windows,
                        focused_wid: Some(9),
                        focus_cg_owned: true,
                        focus_evidence: Some(crate::runtime_types::FocusEvidence {
                            frontmost_pid: Some(4243),
                            frontmost_bundle: None,
                            ax_focus: Some((4243, Some(9))),
                        }),
                        sys_focus: Some((4243, Some(9))),
                        global_focus: Some((4243, 9)),
                        meta,
                        excluded: 0,
                        excluded_reasons: Vec::new(),
                        cg_complete: true,
                        cg_owner_wids: wids,
                        cg_alive: Vec::new(),
                        revision,
                    },

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            retained.observed_focus,
            Some(pending.window.clone()),
            "target observation promotes the observed focus"
        );
        assert!(
            session
                .sequencer()
                .last_reply()
                .expect("settlement keeps the latest reply")
                .focus_request
                .is_none(),
            "target equality clears the core pending request"
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "target observation settles the staged feedback"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "completion retires the intent"
        );
        assert_eq!(
            handle.release_selection_focus(Some(&away_op), post, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending
        );
        assert!(
            lock_shared(&shared)
                .apps
                .get(&4243)
                .and_then(|app| app.focus.take())
                .is_none(),
            "duplicate latest carries no new command"
        );
        // The terminal owner completion retires its outbox row: a consumed
        // round leaves the bounded queue empty instead of leaking one row.
        assert_eq!(crate::runtime_shared::lock_focus(&focus_queue).len(), 0);
        assert_eq!(fake.dispatches, 1);
        assert!(session.peek_close_feedback().is_none());
    }

    #[test]
    fn selection_focus_settled_parent_releases_from_latest_arrival_proof() {
        // Genuine settled-parent proof: skipped issuing plus two unchanged
        // core replies retain the identical frozen request, the parent View
        // settles core-side (truthful staged attempt plus a skipped moved
        // report) BEFORE the first adapter focus dispatch, the first latest
        // then delivers the identical request with no parent transfer and
        // releases immediately from the latest (no old callback/ticket), the
        // mocked owner dispatches exactly once from the generated
        // completion, and a genuine target observation settles the core.
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        lock_shared(&shared).config.topology_mode = crate::runtime_types::TopologyMode::Native;
        close_app(&shared, 4243, 1);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        assert_fixture_admitted(&session, &retained);
        let issuing_req = skipped_issuing_identical(&mut session, &mut retained, &shared, &ctx);
        // No prior adapter binding, outbox row, or owner dispatch: the
        // skipped publications bind nothing core-side visible here.
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "skipped publications bind nothing"
        );
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        assert!(lock_shared(&shared).select_pending.is_empty());
        assert!(lock_shared(&shared).arrival_captures.is_empty());
        // Parent View operation minted core-side by the skipped issuing.
        let parent_op = {
            let views: Vec<String> = session
                .engine_mut()
                .retained_transfers()
                .into_iter()
                .filter(|t| matches!(t.kind, tiler_core::world_report::TransferKind::View))
                .map(|t| t.operation_id)
                .collect();
            assert_eq!(views.len(), 1, "skipped issuing mints one parent View");
            views.into_iter().next().expect("parent View rides")
        };
        // Truthful settle SIDs read from the fixture contexts: pre/baseline
        // is the bootstrap current, post is the moved current.
        let pre_sid = ctx.domains.iter().find(|d| d.current).map(|d| d.space);
        assert_eq!(pre_sid, Some(1));
        let moved_ctx = Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, true)],
            skipped: Vec::new(),
        });
        let post_sid = moved_ctx
            .domains
            .iter()
            .find(|d| d.current)
            .map(|d| d.space);
        assert_eq!(post_sid, Some(277));
        // Stage the proven raw attempt through the production job arm.
        process_job(
            EngineJob::ViewAttempt {
                operation_id: parent_op.clone(),
                setter: tiler_core::world_report::FrameSetter::Accepted,
                pre_sid,
                post_sid,
                baseline_sid: pre_sid,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        // Publish the moved native truth exactly like the main-thread pump
        // before the settling report, then settle core-side via a skipped
        // moved report (bytes inside the codec, never accepted/consumed, so
        // the adapter still binds nothing).
        lock_shared(&shared).native_context = Some(Arc::clone(&moved_ctx));
        lock_shared(&shared).topology_revision = moved_ctx.revision;
        crate::runtime_core::note_dispatch_context(&mut retained, &moved_ctx);
        {
            use crate::world_dispatch::codec_transact;
            let ticket = session
                .sequencer()
                .try_begin_request(None)
                .expect("flight opens for the skipped settle");
            let report = build_dispatch_report(
                &mut session,
                &mut retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&moved_ctx)),
                &[],
                ticket.observation_seq,
                ticket.request_seq,
                ticket.command_seq,
                ticket.command.clone(),
                None,
                &ticket.request_id,
            )
            .expect("skipped settle builds");
            let outcome = codec_transact(&mut session, &report).expect("skipped settle transacts");
            let reply = match outcome {
                WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
                WorldOutcome::Duplicate(reply) => reply,
                WorldOutcome::Rejected { kind, .. } => panic!("skipped settle rejected: {kind}"),
            };
            let req = reply
                .focus_request
                .clone()
                .expect("settle reply keeps the frozen focus request");
            assert_eq!(
                req, issuing_req,
                "parent settle keeps the identical frozen request"
            );
            assert!(
                reply.transfers.iter().all(|t| t.operation_id != parent_op),
                "settle reply carries no parent transfer row"
            );
            session.sequencer().clear_flight();
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "truthful echo settles the parent core-side"
        );
        // Still no adapter binding, outbox row, or owner dispatch after the
        // core-side settle: the first latest below is the first consumer.
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "core settle binds nothing adapter-side"
        );
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        assert!(lock_shared(&shared).select_pending.is_empty());
        // First latest delivery: the only publication that ever runs the
        // production accept/consume path (observation-only, no new command),
        // now over the moved native truth with the parent already settled.
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&moved_ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("settled latest observation must apply, got {other:?}"),
        }
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("settled latest binds the deferred intent");
        assert_eq!(pending.operation_id, parent_op);
        assert_eq!(pending.window.0, "4243_9");
        assert_eq!(pending.after_shown.op, parent_op);
        assert_eq!(pending.adapter_incarnation, session.incarnation());
        assert_eq!(
            pending.core_generation,
            session.sequencer().bound_generation()
        );
        assert_eq!(lock_shared(&shared).topology_revision, moved_ctx.revision);
        // The settled latest binds but defers: the frozen proof still needs
        // the arrival-capture legs, so no release yet (never an early
        // dispatch on unproven arrival).
        assert!(
            !pending.owner_queued,
            "settled latest defers until the arrival capture stamps it"
        );
        assert!(
            pending.proven_stamp.is_none(),
            "deferred bind stays unstamped"
        );
        assert_eq!(
            lock_shared(&shared).selection_focus_floor_seq,
            pending.command_seq
        );
        let reply = session
            .sequencer()
            .last_reply()
            .expect("applied reply retained");
        let req = reply
            .focus_request
            .as_ref()
            .expect("reply carries the retained focus request");
        assert_eq!(
            req.id, issuing_req.id,
            "settled latest delivers the identical core focus id"
        );
        assert_eq!(
            req.target, issuing_req.target,
            "settled latest delivers the identical core focus target"
        );
        assert_eq!(
            req, &issuing_req,
            "settled latest delivers the identical core-issued request"
        );
        assert_eq!(req.id, pending.focus_id);
        assert_eq!(req.target, pending.window);
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "first latest retains no parent View transfer"
        );
        assert!(
            reply.transfers.iter().all(|t| t.operation_id != parent_op),
            "first latest carries no parent transfer row"
        );
        assert!(
            !reply.windows.is_empty(),
            "production reply proves strictly"
        );
        assert!(reply.based_on_observation_seq >= pending.after_shown.obs);
        // No old callback, ticket, or claim: the adapter never stored a
        // select plan, the outbox stays empty, and the owner never
        // dispatched — the unstamped intent cannot release early.
        assert!(
            lock_shared(&shared).select_pending.is_empty(),
            "deferred latest stores no select claim"
        );
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        let handle = release_handle(&shared, &inbox, &focus_queue);
        assert_eq!(
            handle.release_selection_focus(Some(&parent_op), post_sid, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::Deferred,
            "unstamped intent never releases early"
        );
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        // Mock applies the actual produced layout frames, then the
        // production scoped-observation handler observes that mock native
        // geometry so the roster-observed live rects match the frozen
        // actual layout rects for the exact-frame gate. No direct roster
        // mutation: the production `ScopedObservation` path converges the
        // roster, then dispatches the genuine reconciling report.
        {
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let live_reply_now = session
                .sequencer()
                .last_reply()
                .expect("latest keeps the reply");
            let mut per_pid: std::collections::BTreeMap<u32, Vec<crate::runtime::ObservedWindow>> =
                std::collections::BTreeMap::new();
            let mut meta_per_pid: std::collections::BTreeMap<
                u32,
                Vec<crate::runtime_types::ScopedWindowMeta>,
            > = std::collections::BTreeMap::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                let live_sid = live_reply_now
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("produced layout carries the frozen geometry");
                per_pid
                    .entry(mpid)
                    .or_default()
                    .push(crate::runtime::ObservedWindow {
                        wid: mwid,
                        x,
                        y,
                        w,
                        h,
                        floating: false,
                        fullscreen: false,
                        hints: WindowSizeHints::none(),
                    });
                meta_per_pid.entry(mpid).or_default().push(
                    crate::runtime_types::ScopedWindowMeta {
                        wid: mwid,
                        spaces: vec![live_sid],
                        fullscreen: false,
                        minimized: false,
                        hidden: false,
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                    },
                );
            }
            for (mpid, windows) in per_pid {
                let meta = meta_per_pid.remove(&mpid).unwrap_or_default();
                let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
                let (generation, seq_last, revision) = {
                    let guard = lock_shared(&shared);
                    let app = guard.apps.get(&mpid).expect("capture owner stays live");
                    (
                        app.generation,
                        retained
                            .apps
                            .get(&mpid)
                            .map(|app| app.seq_last)
                            .unwrap_or(0),
                        guard.topology_revision,
                    )
                };
                process_job(
                    EngineJob::ScopedObservation {
                        pid: mpid,
                        generation,
                        seq: seq_last + 1,
                        snapshot: crate::runtime_types::ScopedBackendSnapshot {
                            windows,
                            focused_wid: None,
                            focus_cg_owned: false,
                            focus_evidence: None,
                            sys_focus: None,
                            global_focus: None,
                            meta,
                            excluded: 0,
                            excluded_reasons: Vec::new(),
                            cg_complete: true,
                            cg_owner_wids: wids,
                            cg_alive: Vec::new(),
                            revision,
                        },

                        trace_id: 0,
                        intake_us: 0,
                        enqueue_us: 0,
                    },
                    &mut session,
                    &mut retained,
                    &shared,
                    &inbox,
                    &focus_queue,
                    8,
                );
            }
        }
        // Real owner arrival capture for every frozen member, through the
        // production worker/engine path. The capture stamps the deferred
        // intent; the same-id reevaluation against the settled latest then
        // releases exactly once (no drain callback exists on this path).
        use std::sync::atomic::AtomicBool;
        struct SettledCaptureBackend {
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl crate::runtime_backend::AxBackend for SettledCaptureBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture gate never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let live_reply = session
            .sequencer()
            .last_reply()
            .expect("convergence keeps the latest reply")
            .clone();
        let mut reads = Vec::new();
        for member in &pending.after_shown.members {
            let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0) else {
                panic!("frozen member parses");
            };
            let live_sid = live_reply
                .workspaces
                .iter()
                .find(|ws| ws.output == member.out && ws.id == member.ws)
                .and_then(|ws| ws.native_binding.clone())
                .and_then(|binding| crate::workspace_select::parse_native_binding(binding.as_str()))
                .expect("latest publishes the live member binding");
            let frame = retained
                .apps
                .get(&mpid)
                .and_then(|app| app.roster.get(&mwid))
                .and_then(|entry| entry.observed)
                .map(|live| (live.x, live.y, live.w, live.h))
                .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                .expect("converged roster carries the frozen geometry");
            reads.push(crate::runtime_types::BracketRead {
                wid: mwid,
                owner_pid: Some(mpid),
                before_minimized: Some(false),
                before_hidden: Some(false),
                spaces: vec![live_sid],
                frame: Some(frame),
                after_minimized: Some(false),
                after_hidden: Some(false),
            });
        }
        let capture_inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<(String, u64, String, String, u64, u64, u64, u64)> = None;
        let mut pids: Vec<u32> = reads
            .iter()
            .filter_map(|read| read.owner_pid)
            .collect::<std::collections::BTreeSet<u32>>()
            .into_iter()
            .collect();
        pids.sort();
        let mut notified = 0;
        for pid in pids {
            let generation = lock_shared(&shared)
                .apps
                .get(&pid)
                .map(|record| record.generation)
                .expect("capture owner stays live");
            let mut backend = SettledCaptureBackend {
                reads: reads.clone(),
            };
            if crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                pid,
                generation,
                &shared,
                &cell,
                &capture_inbox,
                &mut last_attempt,
            ) {
                notified += 1;
            }
        }
        assert!(notified > 0, "worker owes at least one genuine capture");
        for _ in 0..notified {
            let stop = AtomicBool::new(false);
            let job = capture_inbox.recv(&stop).expect("capture queued");
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        // Arrival capture auto-releases by design: the bound intent stays
        // with owner_queued set until the real owner result retires it, so
        // any old-callback duplicate below must report NoPending.
        {
            let auto = lock_shared(&shared)
                .selection_focus_pending
                .clone()
                .expect("capture keeps the intent bound for release");
            assert!(auto.owner_queued, "capture auto-releases the intent");
            assert_eq!(auto.operation_id, parent_op);
            assert_eq!(auto.focus_id, pending.focus_id);
            assert!(auto.proven_stamp.is_some(), "capture stamps the intent");
        }
        {
            let queued = crate::runtime_shared::lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "capture release queues exactly once");
            assert_eq!((queued[0].pid, queued[0].wid), (4243, 9));
            assert_eq!(queued[0].generation, 1);
        }
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 2);
        assert_eq!(
            handle.release_selection_focus(Some(&parent_op), post_sid, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending,
            "old callback never duplicates the capture release"
        );
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 2);
        let cmd = lock_shared(&shared)
            .apps
            .get(&4243)
            .and_then(|app| app.focus.take())
            .expect("owner slot carries the release");
        assert_eq!(cmd.window, "4243_9");
        assert_eq!(
            lock_shared(&shared).selection_focus_floor_seq,
            pending.command_seq
        );
        let close = cmd.close.clone().expect("unified close route");
        assert_eq!(close.close_id, pending.focus_id);
        assert_eq!(close.issued_revision, pending.issued_revision);
        assert_eq!(
            close.after_shown.as_ref().map(|a| a.op.as_str()),
            Some(parent_op.as_str())
        );
        assert_eq!(
            lock_shared(&shared)
                .close_focus_current
                .as_ref()
                .map(|e| e.id.clone()),
            Some(pending.focus_id.clone())
        );
        // Actual mocked owner setter plus its generated completion.
        let revision = lock_shared(&shared).topology_revision;
        let mut fake = FakeFocusBackend {
            fresh: Some((4242, Some(7))),
            post: Some((4243, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 3 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut fake, &cmd, &close, 4243, 1, revision, &shared,
        );
        assert_eq!(fake.dispatches, 1, "owner dispatches exactly once");
        assert_eq!(fake.reads, 2, "fresh baseline plus post read");
        match &job {
            EngineJob::CloseFocusResult {
                setter,
                reason,
                pre,
                post,
                foreground_proven,
                close_id,
                target,
                issued_rev,
                ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::Accepted
                );
                assert_eq!(*reason, tiler_core::world_report::VisibilityReason::Empty);
                assert_eq!(
                    *pre,
                    Some(tiler_core::directional::WindowId("4242_7".to_owned()))
                );
                assert_eq!(
                    *post,
                    Some(tiler_core::directional::WindowId("4243_9".to_owned()))
                );
                assert!(*foreground_proven);
                assert_eq!(close_id, &pending.focus_id);
                assert_eq!(*issued_rev, pending.issued_revision);
                assert_eq!(*target, pending.window);
            }
            _ => panic!("expected CloseFocusResult"),
        }
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("accepted stages").id,
            pending.focus_id
        );
        // The staged row is truthful generated feedback: exact stamp plus
        // the full frozen arrival set with valid visible rows. This row
        // alone never settles the core (Accepted/Proven retains until
        // target equality); the genuine target observation below does.
        {
            let staged = session
                .peek_close_feedback()
                .expect("accepted stages")
                .clone();
            assert_eq!(staged.target, pending.window);
            assert_eq!(staged.revision, pending.issued_revision);
            assert_eq!(staged.post, Some(pending.window.clone()));
            assert!(staged.foreground_proven);
            let mut frozen: Vec<String> = pending
                .after_shown
                .members
                .iter()
                .map(|m| m.win.0.clone())
                .collect();
            frozen.sort();
            let mut arrived: Vec<String> =
                staged.arrival.iter().map(|b| b.window.0.clone()).collect();
            arrived.sort();
            assert_eq!(
                arrived, frozen,
                "generated feedback covers the full frozen set"
            );
            assert!(
                !staged.arrival.is_empty(),
                "generated feedback carries valid rows"
            );
            for bracket in &staged.arrival {
                assert!(
                    bracket.before.visibility.visible == Some(true)
                        && bracket.before.lifetime_matched == Some(true),
                    "arrival before leg is valid"
                );
                assert!(
                    bracket.after.visibility.visible == Some(true)
                        && bracket.after.lifetime_matched == Some(true),
                    "arrival after leg is valid"
                );
            }
        }
        // Genuine target observation through the production ScopedObservation
        // path: the owner thread reports the target focused with full
        // workspace+AX+CG agreement on the current domain, so the engine
        // promotes `retained.observed_focus` to the target (the
        // CloseFocusResult handler above never touches observed focus; only
        // this observation does). Its inner dispatch then drains the staged
        // row with `focused_global == target`, which is what actually
        // clears the core pending request via target equality.
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4243),
            frontmost_bundle: None,
            ax_focus: None,
        });
        {
            let live_reply = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply")
                .clone();
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let mut windows = Vec::new();
            let mut meta = Vec::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                if mpid != 4243 {
                    continue;
                }
                let live_sid = live_reply
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("frozen member has readable live geometry");
                windows.push(crate::runtime::ObservedWindow {
                    wid: mwid,
                    x,
                    y,
                    w,
                    h,
                    floating: false,
                    fullscreen: false,
                    hints: WindowSizeHints::none(),
                });
                meta.push(crate::runtime_types::ScopedWindowMeta {
                    wid: mwid,
                    spaces: vec![live_sid],
                    fullscreen: false,
                    minimized: false,
                    hidden: false,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                });
            }
            assert!(!windows.is_empty(), "target owner holds frozen members");
            let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
            assert!(wids.contains(&9), "target wid rides the frozen owner set");
            let (generation, seq_last, revision) = {
                let guard = lock_shared(&shared);
                let generation = guard
                    .apps
                    .get(&4243)
                    .expect("capture owner stays live")
                    .generation;
                let seq_last = retained
                    .apps
                    .get(&4243)
                    .map(|app| app.seq_last)
                    .unwrap_or(0);
                (generation, seq_last, guard.topology_revision)
            };
            process_job(
                EngineJob::ScopedObservation {
                    pid: 4243,
                    generation,
                    seq: seq_last + 1,
                    snapshot: crate::runtime_types::ScopedBackendSnapshot {
                        windows,
                        focused_wid: Some(9),
                        focus_cg_owned: true,
                        focus_evidence: Some(crate::runtime_types::FocusEvidence {
                            frontmost_pid: Some(4243),
                            frontmost_bundle: None,
                            ax_focus: Some((4243, Some(9))),
                        }),
                        sys_focus: Some((4243, Some(9))),
                        global_focus: Some((4243, 9)),
                        meta,
                        excluded: 0,
                        excluded_reasons: Vec::new(),
                        cg_complete: true,
                        cg_owner_wids: wids,
                        cg_alive: Vec::new(),
                        revision,
                    },

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            retained.observed_focus,
            Some(pending.window.clone()),
            "target observation promotes the observed focus"
        );
        assert!(
            session
                .sequencer()
                .last_reply()
                .expect("settlement keeps the latest reply")
                .focus_request
                .is_none(),
            "target equality clears the core pending request"
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "target observation settles the staged feedback"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "completion retires the intent"
        );
        assert_eq!(
            handle.release_selection_focus(Some(&parent_op), post_sid, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending
        );
        assert!(
            lock_shared(&shared)
                .apps
                .get(&4243)
                .and_then(|app| app.focus.take())
                .is_none(),
            "duplicate latest carries no new command"
        );
        // The terminal owner completion retires its outbox row: a consumed
        // round leaves the bounded queue empty instead of leaking one row.
        assert_eq!(crate::runtime_shared::lock_focus(&focus_queue).len(), 0);
        assert_eq!(fake.dispatches, 1);
        assert!(session.peek_close_feedback().is_none());
    }

    #[test]
    fn selection_focus_frozen_late_admission_excluded() {
        // Frozen-set exclusion: the away intent proves its frozen projection
        // through the genuine production legs (scoped-geometry convergence
        // plus an owner arrival capture with same-id auto-release), releases
        // exactly once, actuates once through the mocked owner while
        // current, and settles on the genuine target observation. A
        // genuinely late-admitted fixed-size window (absent from the frozen
        // members) arrives WHILE the old request is still pending — before
        // the capture stamps it — and is then excluded: the core admits it
        // as an automatic float whose birth spends without emitting, so it
        // mints no new request, arms no new queue row, no new slot
        // command, and no staged feedback, while the frozen set stays
        // byte-identical and the capture plus generated feedback cover
        // only the original set. The capture below reads ONLY the frozen
        // members.
        use std::sync::atomic::AtomicBool;
        let (mut session, mut retained, shared, inbox, focus_queue, _ctx) =
            selection_away_fixture();
        let (away_op, post) =
            drain_settle_away(&mut session, &mut retained, &shared, &inbox, &focus_queue);
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("away binds the deferred intent");
        assert_eq!(pending.operation_id, away_op);
        // Mock applies the actual produced layout frames, then the
        // production scoped-observation handler observes that mock native
        // geometry so the roster-observed live rects match the frozen
        // actual layout rects for the exact-frame gate. No direct roster
        // mutation: the production `ScopedObservation` path converges the
        // roster, then dispatches the genuine reconciling report.
        {
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let live_reply_now = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply");
            let mut per_pid: std::collections::BTreeMap<u32, Vec<crate::runtime::ObservedWindow>> =
                std::collections::BTreeMap::new();
            let mut meta_per_pid: std::collections::BTreeMap<
                u32,
                Vec<crate::runtime_types::ScopedWindowMeta>,
            > = std::collections::BTreeMap::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                let live_sid = live_reply_now
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("produced layout carries the frozen geometry");
                per_pid
                    .entry(mpid)
                    .or_default()
                    .push(crate::runtime::ObservedWindow {
                        wid: mwid,
                        x,
                        y,
                        w,
                        h,
                        floating: false,
                        fullscreen: false,
                        hints: WindowSizeHints::none(),
                    });
                meta_per_pid.entry(mpid).or_default().push(
                    crate::runtime_types::ScopedWindowMeta {
                        wid: mwid,
                        spaces: vec![live_sid],
                        fullscreen: false,
                        minimized: false,
                        hidden: false,
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                    },
                );
            }
            for (mpid, windows) in per_pid {
                let meta = meta_per_pid.remove(&mpid).unwrap_or_default();
                let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
                let (generation, seq_last, revision) = {
                    let guard = lock_shared(&shared);
                    let app = guard.apps.get(&mpid).expect("capture owner stays live");
                    (
                        app.generation,
                        retained
                            .apps
                            .get(&mpid)
                            .map(|app| app.seq_last)
                            .unwrap_or(0),
                        guard.topology_revision,
                    )
                };
                process_job(
                    EngineJob::ScopedObservation {
                        pid: mpid,
                        generation,
                        seq: seq_last + 1,
                        snapshot: crate::runtime_types::ScopedBackendSnapshot {
                            windows,
                            focused_wid: None,
                            focus_cg_owned: false,
                            focus_evidence: None,
                            sys_focus: None,
                            global_focus: None,
                            meta,
                            excluded: 0,
                            excluded_reasons: Vec::new(),
                            cg_complete: true,
                            cg_owner_wids: wids,
                            cg_alive: Vec::new(),
                            revision,
                        },

                        trace_id: 0,
                        intake_us: 0,
                        enqueue_us: 0,
                    },
                    &mut session,
                    &mut retained,
                    &shared,
                    &inbox,
                    &focus_queue,
                    8,
                );
            }
        }
        // Late fixed-size newcomer admitted WHILE the old request is still
        // pending (before the capture stamps it): window 10 rides plain
        // with fixed-equal hints on the arrival Space through the genuine
        // scoped-observation plus production-dispatch path. The core admits
        // it as an automatic float whose birth spends without emitting, so
        // the old core request survives under its own id and the adapter
        // intent keeps its frozen set byte-identical.
        let focus_before = session
            .sequencer()
            .last_reply()
            .expect("settle keeps the latest reply")
            .focus_request
            .clone();
        assert_eq!(
            focus_before.as_ref().map(|req| req.id.as_str()),
            Some("focus-2"),
            "old explicit request still current while pending"
        );
        {
            let live_reply = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply")
                .clone();
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let live_sid = pending
                .after_shown
                .members
                .iter()
                .find_map(|member| {
                    let (mpid, _) = crate::runtime_types::parse_window_id(&member.win.0)?;
                    if mpid != 4243 {
                        return None;
                    }
                    live_reply
                        .workspaces
                        .iter()
                        .find(|ws| ws.output == member.out && ws.id == member.ws)
                        .and_then(|ws| ws.native_binding.clone())
                        .and_then(|binding| {
                            crate::workspace_select::parse_native_binding(binding.as_str())
                        })
                })
                .expect("latest publishes the live arrival binding");
            let (x, y, w, h) = by_window
                .get("4243_9")
                .copied()
                .expect("produced layout carries the frozen target geometry");
            let windows = vec![
                crate::runtime::ObservedWindow {
                    wid: 9,
                    x,
                    y,
                    w,
                    h,
                    floating: false,
                    fullscreen: false,
                    hints: WindowSizeHints::none(),
                },
                crate::runtime::ObservedWindow {
                    wid: 10,
                    x: 100,
                    y: 100,
                    w: 800,
                    h: 600,
                    floating: false,
                    fullscreen: false,
                    hints: WindowSizeHints {
                        min_w: Some(800),
                        min_h: Some(600),
                        max_w: Some(800),
                        max_h: Some(600),
                    },
                },
            ];
            let meta = vec![
                crate::runtime_types::ScopedWindowMeta {
                    wid: 9,
                    spaces: vec![live_sid],
                    fullscreen: false,
                    minimized: false,
                    hidden: false,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                },
                crate::runtime_types::ScopedWindowMeta {
                    wid: 10,
                    spaces: vec![live_sid],
                    fullscreen: false,
                    minimized: false,
                    hidden: false,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(false),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                },
            ];
            let (generation, seq_last, revision) = {
                let guard = lock_shared(&shared);
                (
                    guard
                        .apps
                        .get(&4243)
                        .expect("admission owner stays live")
                        .generation,
                    retained
                        .apps
                        .get(&4243)
                        .map(|app| app.seq_last)
                        .unwrap_or(0),
                    guard.topology_revision,
                )
            };
            process_job(
                EngineJob::ScopedObservation {
                    pid: 4243,
                    generation,
                    seq: seq_last + 1,
                    snapshot: crate::runtime_types::ScopedBackendSnapshot {
                        windows,
                        focused_wid: None,
                        focus_cg_owned: false,
                        focus_evidence: None,
                        sys_focus: None,
                        global_focus: None,
                        meta,
                        excluded: 0,
                        excluded_reasons: Vec::new(),
                        cg_complete: true,
                        cg_owner_wids: vec![9, 10],
                        cg_alive: Vec::new(),
                        revision,
                    },

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        // Order + pending-identity fence: the newcomer is already admitted
        // (latest roster AND latest reply) while the old request is still
        // pending under its original id with its frozen set untouched.
        assert!(
            retained
                .apps
                .get(&4243)
                .and_then(|app| app.roster.get(&10))
                .is_some(),
            "fixed newcomer admitted to the latest roster while pending"
        );
        assert!(
            session
                .sequencer()
                .last_reply()
                .expect("admission publishes")
                .windows
                .iter()
                .any(|row| row.lifetime.0 == "4243_10" && row.floating),
            "fixed newcomer rides the latest reply as an automatic float"
        );
        {
            let focus_after = session
                .sequencer()
                .last_reply()
                .expect("admission publishes")
                .focus_request
                .clone();
            assert_eq!(
                focus_after.as_ref().map(|req| req.id.clone()),
                focus_before.as_ref().map(|req| req.id.clone()),
                "withheld birth mints nothing: the old request id survives"
            );
            assert_eq!(
                focus_after.as_ref().map(|req| req.target.0.clone()),
                Some("4243_9".to_owned()),
                "old target undisturbed by the fixed newcomer"
            );
        }
        {
            let current = lock_shared(&shared)
                .selection_focus_pending
                .clone()
                .expect("newcomer never cancels the bound intent");
            assert_eq!(current.operation_id, away_op, "same pending operation");
            assert_eq!(current.focus_id, pending.focus_id, "same pending request");
            assert_eq!(
                current.after_shown.members, pending.after_shown.members,
                "frozen set byte-identical despite the live newcomer"
            );
            assert!(
                !current
                    .after_shown
                    .members
                    .iter()
                    .any(|m| m.win.0 == "4243_10"),
                "late member excluded from the frozen set"
            );
        }
        assert!(
            crate::runtime_shared::lock_focus(&focus_queue).is_empty(),
            "withheld birth dispatches nothing new"
        );
        assert!(
            lock_shared(&shared)
                .apps
                .get(&4243)
                .and_then(|app| app.focus.take())
                .is_none(),
            "withheld birth arms no slot command"
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "withheld birth stages no feedback"
        );
        // Real owner arrival capture for every frozen member, through the
        // production worker/engine path. The reads cover ONLY the frozen
        // members (never the pending-admitted fixed newcomer above, which
        // the release below must not need).
        struct LocalCaptureBackend {
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl crate::runtime_backend::AxBackend for LocalCaptureBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture gate never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let live_reply = session
            .sequencer()
            .last_reply()
            .expect("settle keeps the latest reply");
        let mut reads = Vec::new();
        for member in &pending.after_shown.members {
            let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0) else {
                panic!("frozen member parses");
            };
            let live_sid = live_reply
                .workspaces
                .iter()
                .find(|ws| ws.output == member.out && ws.id == member.ws)
                .and_then(|ws| ws.native_binding.clone())
                .and_then(|binding| crate::workspace_select::parse_native_binding(binding.as_str()))
                .expect("latest publishes the live member binding");
            let frame = retained
                .apps
                .get(&mpid)
                .and_then(|app| app.roster.get(&mwid))
                .and_then(|entry| entry.observed)
                .map(|live| (live.x, live.y, live.w, live.h))
                .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                .expect("frozen member has readable live geometry");
            reads.push(crate::runtime_types::BracketRead {
                wid: mwid,
                owner_pid: Some(mpid),
                before_minimized: Some(false),
                before_hidden: Some(false),
                spaces: vec![live_sid],
                frame: Some(frame),
                after_minimized: Some(false),
                after_hidden: Some(false),
            });
        }
        let capture_inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<(String, u64, String, String, u64, u64, u64, u64)> = None;
        let mut pids: Vec<u32> = reads
            .iter()
            .filter_map(|read| read.owner_pid)
            .collect::<std::collections::BTreeSet<u32>>()
            .into_iter()
            .collect();
        pids.sort();
        let mut notified = 0;
        for pid in pids {
            let generation = lock_shared(&shared)
                .apps
                .get(&pid)
                .map(|record| record.generation)
                .expect("capture owner stays live");
            let mut backend = LocalCaptureBackend {
                reads: reads.clone(),
            };
            if crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                pid,
                generation,
                &shared,
                &cell,
                &capture_inbox,
                &mut last_attempt,
            ) {
                notified += 1;
            }
        }
        assert!(notified > 0, "worker owes at least one genuine capture");
        for _ in 0..notified {
            let stop = AtomicBool::new(false);
            let job = capture_inbox.recv(&stop).expect("capture queued");
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        let stamped = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("capture keeps the intent for release");
        let stamp = stamped
            .proven_stamp
            .clone()
            .expect("capture stamps the intent");
        {
            let guard = lock_shared(&shared);
            assert_eq!(stamp.obs, guard.clock.get());
            assert_eq!(stamp.revision, guard.topology_revision);
            assert_eq!(stamp.context_generation, guard.native_cell.generation());
        }
        for member in &pending.after_shown.members {
            assert!(
                lock_shared(&shared)
                    .arrival_captures
                    .contains_key(&member.win.0),
                "every frozen member retains a genuine capture"
            );
        }
        // Arrival capture auto-releases by design (apply_arrival_capture ->
        // reevaluate_selection_release): the bound intent stays with
        // owner_queued set until the real owner result retires it, so the
        // drain-verified callback below must report NoPending, never a
        // second release.
        {
            let auto = lock_shared(&shared)
                .selection_focus_pending
                .clone()
                .expect("auto-release keeps the intent bound");
            assert!(auto.owner_queued, "auto-release marks the intent queued");
            assert_eq!(auto.operation_id, away_op);
            assert_eq!(auto.focus_id, pending.focus_id);
            assert!(
                auto.proven_stamp.is_some(),
                "auto-release keeps the proven stamp"
            );
        }
        {
            let queued = crate::runtime_shared::lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "auto-release queues exactly once");
            assert_eq!((queued[0].pid, queued[0].wid), (4243, 9));
            assert_eq!(queued[0].generation, 1);
        }
        // Duplicate drain-verified callback reports NoPending instead of a
        // second release; the queued intent stays bound for the owner leg.
        let handle = release_handle(&shared, &inbox, &focus_queue);
        assert_eq!(
            handle.release_selection_focus(Some(&away_op), post, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "duplicate never retires the queued intent"
        );
        assert_eq!(
            crate::runtime_shared::lock_focus(&focus_queue).len(),
            1,
            "duplicate queues nothing more"
        );
        // The frozen command actuates exactly once through the mocked owner
        // while its release is still current (the pending-admitted fixed
        // newcomer above spent without touching it). Taken once for the
        // owner actuation, so the
        // single-threaded harness observes no interim.
        let cmd = lock_shared(&shared)
            .apps
            .get(&4243)
            .and_then(|app| app.focus.take())
            .expect("owner slot carries the release");
        let close = cmd.close.clone().expect("unified close route");
        assert_eq!(close.close_id, "focus-2");
        assert_eq!(
            close.after_shown.as_ref().map(|a| a.op.as_str()),
            Some(away_op.as_str())
        );
        let revision = lock_shared(&shared).topology_revision;
        let mut fake = FakeFocusBackend {
            fresh: Some((4242, Some(7))),
            post: Some((4243, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 3 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut fake, &cmd, &close, 4243, 1, revision, &shared,
        );
        assert_eq!(fake.dispatches, 1, "owner dispatches exactly once");
        assert_eq!(fake.reads, 2, "fresh baseline plus post read");
        match &job {
            EngineJob::CloseFocusResult { setter, .. } => assert_eq!(
                *setter,
                tiler_core::world_report::VisibilitySetter::Accepted
            ),
            _ => panic!("expected CloseFocusResult"),
        }
        process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session.peek_close_feedback().expect("accepted stages").id,
            "focus-2"
        );
        // The staged row is truthful generated feedback: exact stamp plus
        // the full frozen arrival set with valid visible rows. This row
        // alone never settles the core (Accepted/Proven retains until
        // target equality); the genuine target observation below does.
        {
            let staged = session
                .peek_close_feedback()
                .expect("accepted stages")
                .clone();
            assert_eq!(staged.target, pending.window);
            assert_eq!(staged.revision, pending.issued_revision);
            assert_eq!(staged.post, Some(pending.window.clone()));
            assert!(staged.foreground_proven);
            let mut frozen: Vec<String> = pending
                .after_shown
                .members
                .iter()
                .map(|m| m.win.0.clone())
                .collect();
            frozen.sort();
            let mut arrived: Vec<String> =
                staged.arrival.iter().map(|b| b.window.0.clone()).collect();
            arrived.sort();
            assert_eq!(
                arrived, frozen,
                "generated feedback covers the full frozen set"
            );
            assert!(
                !staged.arrival.is_empty(),
                "generated feedback carries valid rows"
            );
            for bracket in &staged.arrival {
                assert!(
                    bracket.before.visibility.visible == Some(true)
                        && bracket.before.lifetime_matched == Some(true),
                    "arrival before leg is valid"
                );
                assert!(
                    bracket.after.visibility.visible == Some(true)
                        && bracket.after.lifetime_matched == Some(true),
                    "arrival after leg is valid"
                );
            }
        }
        // Genuine target observation through the production ScopedObservation
        // path: the owner thread reports the target focused with full
        // workspace+AX+CG agreement on the current domain, so the engine
        // promotes `retained.observed_focus` to the target (the
        // CloseFocusResult handler above never touches observed focus; only
        // this observation does). The pending-admitted fixed newcomer rides
        // along unfocused with its fixed hints, so it stays live (never a
        // close) without disturbing the settle. Its inner dispatch then
        // drains the staged row with `focused_global == target`, which is
        // what actually clears the core pending request via target equality.
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4243),
            frontmost_bundle: None,
            ax_focus: None,
        });
        {
            let live_reply = session
                .sequencer()
                .last_reply()
                .expect("settle keeps the latest reply")
                .clone();
            let desired = lock_shared(&shared).last_desired.clone();
            let by_window: std::collections::BTreeMap<String, (i32, i32, i32, i32)> = desired
                .into_iter()
                .map(|row| (row.window, (row.x, row.y, row.w, row.h)))
                .collect();
            let mut windows = Vec::new();
            let mut meta = Vec::new();
            for member in &pending.after_shown.members {
                let Some((mpid, mwid)) = crate::runtime_types::parse_window_id(&member.win.0)
                else {
                    panic!("frozen member parses");
                };
                if mpid != 4243 {
                    continue;
                }
                let live_sid = live_reply
                    .workspaces
                    .iter()
                    .find(|ws| ws.output == member.out && ws.id == member.ws)
                    .and_then(|ws| ws.native_binding.clone())
                    .and_then(|binding| {
                        crate::workspace_select::parse_native_binding(binding.as_str())
                    })
                    .expect("latest publishes the live member binding");
                let (x, y, w, h) = by_window
                    .get(&member.win.0)
                    .copied()
                    .or_else(|| member.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)))
                    .expect("frozen member has readable live geometry");
                windows.push(crate::runtime::ObservedWindow {
                    wid: mwid,
                    x,
                    y,
                    w,
                    h,
                    floating: false,
                    fullscreen: false,
                    hints: WindowSizeHints::none(),
                });
                meta.push(crate::runtime_types::ScopedWindowMeta {
                    wid: mwid,
                    spaces: vec![live_sid],
                    fullscreen: false,
                    minimized: false,
                    hidden: false,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                });
            }
            // The pending-admitted fixed newcomer rides along unfocused:
            // it stays live with its fixed hints (never a close) while the
            // target focus legs settle the old request.
            let newcomer_sid = meta
                .iter()
                .find(|m| m.wid == 9)
                .and_then(|m| m.spaces.first().copied())
                .expect("target member carries the live arrival Space");
            windows.push(crate::runtime::ObservedWindow {
                wid: 10,
                x: 100,
                y: 100,
                w: 800,
                h: 600,
                floating: false,
                fullscreen: false,
                hints: WindowSizeHints {
                    min_w: Some(800),
                    min_h: Some(600),
                    max_w: Some(800),
                    max_h: Some(600),
                },
            });
            meta.push(crate::runtime_types::ScopedWindowMeta {
                wid: 10,
                spaces: vec![newcomer_sid],
                fullscreen: false,
                minimized: false,
                hidden: false,
                role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                modal: Some(false),
                resizable: Some(false),
                layer: Some(0),
                app_id: Some("com.example.app".to_owned()),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
            });
            assert!(!windows.is_empty(), "target owner holds frozen members");
            let wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
            assert!(wids.contains(&9), "target wid rides the frozen owner set");
            let (generation, seq_last, revision) = {
                let guard = lock_shared(&shared);
                let generation = guard
                    .apps
                    .get(&4243)
                    .expect("capture owner stays live")
                    .generation;
                let seq_last = retained
                    .apps
                    .get(&4243)
                    .map(|app| app.seq_last)
                    .unwrap_or(0);
                (generation, seq_last, guard.topology_revision)
            };
            process_job(
                EngineJob::ScopedObservation {
                    pid: 4243,
                    generation,
                    seq: seq_last + 1,
                    snapshot: crate::runtime_types::ScopedBackendSnapshot {
                        windows,
                        focused_wid: Some(9),
                        focus_cg_owned: true,
                        focus_evidence: Some(crate::runtime_types::FocusEvidence {
                            frontmost_pid: Some(4243),
                            frontmost_bundle: None,
                            ax_focus: Some((4243, Some(9))),
                        }),
                        sys_focus: Some((4243, Some(9))),
                        global_focus: Some((4243, 9)),
                        meta,
                        excluded: 0,
                        excluded_reasons: Vec::new(),
                        cg_complete: true,
                        cg_owner_wids: wids,
                        cg_alive: Vec::new(),
                        revision,
                    },

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert_eq!(
            retained.observed_focus,
            Some(pending.window.clone()),
            "target observation promotes the observed focus"
        );
        assert!(
            session
                .sequencer()
                .last_reply()
                .expect("settlement keeps the latest reply")
                .focus_request
                .is_none(),
            "target equality clears the core pending request"
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "target observation settles the staged feedback"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "completion retires the intent"
        );
        assert_eq!(
            handle.release_selection_focus(Some(&away_op), post, Some(4242)),
            crate::world_dispatch::SelectionFocusRelease::NoPending
        );
        assert!(
            lock_shared(&shared)
                .apps
                .get(&4243)
                .and_then(|app| app.focus.take())
                .is_none(),
            "duplicate latest carries no new command"
        );
        // The terminal owner completion retires its outbox row: a consumed
        // round leaves the bounded queue empty instead of leaking one row.
        assert_eq!(crate::runtime_shared::lock_focus(&focus_queue).len(), 0);
        assert_eq!(fake.dispatches, 1);
        assert!(session.peek_close_feedback().is_none());
        // The pending-admitted fixed newcomer stays live through the settle
        // without ever joining the frozen set or disturbing the release.
        assert!(
            retained
                .apps
                .get(&4243)
                .and_then(|app| app.roster.get(&10))
                .is_some(),
            "excluded newcomer stays admitted after the settle"
        );
    }

    #[test]
    fn selection_focus_null_and_newer_epoch_cancel_without_setter() {
        // Latest/null epoch cancellation: a genuine newer non-explicit
        // publication cancels the unstarted deferred intent, and a genuine
        // null (focus-satisfied) publication retires the epoch — the old
        // operation never reaches any setter (zero queue rows, zero slot
        // commands, zero dispatches throughout).
        let (mut session, mut retained, shared, inbox, focus_queue, _ctx) =
            selection_away_fixture();
        let away_op = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("away binds")
            .operation_id;
        let dropped_before = lock_shared(&shared).diagnostics.focus_dropped_full;
        // Newer non-explicit publication: ordinary admission of window 10
        // mints an AutomaticAdmission request for the new window, cancelling
        // the old intent counted while dispatching nothing itself.
        let meta = |wid: u32, minimized: bool| crate::runtime_types::ScopedWindowMeta {
            wid,
            spaces: vec![277],
            fullscreen: false,
            minimized,
            hidden: false,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        };
        let scoped = |windows: Vec<ObservedWindow>,
                      focused: Option<u32>,
                      min10: bool,
                      seq: u64,
                      rev: u64| {
            crate::runtime_types::ScopedBackendSnapshot {
                windows,
                focused_wid: focused,
                focus_cg_owned: false,
                focus_evidence: None,
                sys_focus: None,
                global_focus: None,
                meta: vec![meta(9, false), meta(10, min10)],
                excluded: 0,
                excluded_reasons: Vec::new(),
                cg_complete: true,
                cg_owner_wids: vec![9, 10],
                cg_alive: Vec::new(),
                revision: rev,
            }
        };
        process_job(
            EngineJob::ScopedObservation {
                pid: 4243,
                generation: 1,
                seq: 2,
                snapshot: scoped(vec![observed(9), observed(10)], None, false, 2, 9),

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let late_req = session
            .sequencer()
            .last_reply()
            .expect("reply retained")
            .focus_request
            .expect("admission mints its own request");
        assert_eq!(late_req.target.0, "4243_10");
        assert_ne!(late_req.id, "focus-2");
        assert!(lock_shared(&shared).selection_focus_pending.is_none());
        {
            let after = lock_shared(&shared).diagnostics.focus_dropped_full;
            // Two counted events, both genuine: the bind refusal drops the
            // one deferred intent (op-1 cancelled), and the refusal arm
            // stages the admission's own no-call row with its own count.
            assert_eq!(
                after,
                dropped_before + 2,
                "admission cancels the deferred intent and no-calls its own (before={dropped_before} after={after})"
            );
        }
        assert_eq!(
            lock_shared(&shared)
                .close_focus_current
                .as_ref()
                .map(|e| e.id.clone()),
            Some(late_req.id.clone())
        );
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert!(
            lock_shared(&shared)
                .apps
                .get(&4243)
                .and_then(|app| app.focus.take())
                .is_none(),
            "cancelled intent arms no slot command"
        );
        // Null publication: the owner reports the old target focused, so
        // the core retires the lane and the adapter epoch retires to None.
        process_job(
            EngineJob::ScopedObservation {
                pid: 4243,
                generation: 1,
                seq: 3,
                snapshot: scoped(vec![observed(9), observed(10)], Some(9), false, 3, 9),

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(
            session
                .sequencer()
                .last_reply()
                .expect("reply retained")
                .focus_request,
            None,
            "target equality retires the lane"
        );
        assert!(lock_shared(&shared).close_focus_current.is_none());
        assert!(lock_shared(&shared).selection_focus_pending.is_none());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 0);
        assert!(crate::runtime_shared::lock_focus(&focus_queue).is_empty());
        assert!(session.peek_close_feedback().is_none());
        assert_eq!(away_op, "op-1");
    }

    #[test]
    fn previous_toggle_binds_core_history_and_drains_reply_driven() {
        // Core previous outcome exercised end to end: dispatch numbered 2
        // (moves the view), drain it reply-driven, stage the proven
        // attempt, settle the moved observation (history records the
        // proven arrival - never the dispatch), then dispatch previous
        // (flips back through core history) and drain that too, then
        // previous again (flips forward). No local history, no ordinal
        // reinterpretation - every drain actuates the reply-issued
        // bindings with truthful baselines.
        use crate::workspace_select::{SelectGates, SelectionState, drain_claimed_select_with};
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Numbered 2 first (binds the second adopted logical).
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 }),
            true,
        );
        let first = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("numbered claim stored");
        let first_op = first.operation_id.clone().expect("operation rides");
        assert!(
            first.select_native_binding.is_some(),
            "numbered binds a live native"
        );
        assert_eq!(first.select_baseline_sid, Some(1));
        // Drain the numbered move through the scripted native row.
        let mut backend = FakeBackend::new(&[1, 277], 1, &[]);
        let mut state = SelectionState::default();
        let gates = SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        };
        let moved = drain_claimed_select_with(&mut state, &mut backend, first, &gates, true)
            .expect("numbered drains to its binding");
        assert_eq!(moved.setter, FrameSetter::Accepted);
        assert!(moved.effect.changed);
        assert_eq!(moved.effect.baseline, Some(1));
        assert_eq!((moved.pre, moved.post), (Some(1), Some(277)));
        // Stage the proven raw attempt and settle the moved observation
        // through the production dispatch path: the native echo (current
        // 277) confirms the view and records core history. The shared
        // topology publishes first, exactly like the main-thread pump,
        // so the dispatch snapshot is honestly fenced.
        process_job(
            EngineJob::ViewAttempt {
                operation_id: first_op,
                setter: FrameSetter::Accepted,
                pre_sid: moved.pre,
                post_sid: moved.post,
                baseline_sid: moved.effect.baseline,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let moved_ctx = Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, true)],
            skipped: Vec::new(),
        });
        lock_shared(&shared).native_context = Some(Arc::clone(&moved_ctx));
        lock_shared(&shared).topology_revision = moved_ctx.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&moved_ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("moved observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "native echo settles the numbered transfer"
        );
        // Previous flips back through proven core history (not local
        // state): the plan binds the original Space with the moved
        // observation-time baseline.
        lock_shared(&shared).select_pending.clear();
        lock_shared(&shared).select_busy = false;
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspacePrevious),
            true,
        );
        let prev = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("previous claim stored with proven history");
        let prev_op = prev.operation_id.clone().expect("operation rides");
        assert_eq!(
            prev.select_native_binding.as_deref(),
            Some(crate::topology::workspace_key(1).as_str()),
            "previous binds the proven prior Space"
        );
        assert_eq!(prev.select_baseline_sid, Some(277));
        let back = drain_claimed_select_with(&mut state, &mut backend, prev, &gates, true)
            .expect("previous drains to its binding");
        assert_eq!((back.effect.from, back.effect.target), (277, 1));
        assert_eq!(back.effect.baseline, Some(277));
        // Settle the return arrival, then previous again flips forward -
        // history advances only on proven arrivals, both directions.
        process_job(
            EngineJob::ViewAttempt {
                operation_id: prev_op,
                setter: FrameSetter::Accepted,
                pre_sid: back.pre,
                post_sid: back.post,
                baseline_sid: back.effect.baseline,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let home_ctx = Arc::new(NativeContext {
            revision: 11,
            domains: vec![domain(1, true), domain(277, false)],
            skipped: Vec::new(),
        });
        lock_shared(&shared).native_context = Some(Arc::clone(&home_ctx));
        lock_shared(&shared).topology_revision = home_ctx.revision;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&home_ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("return observation must apply, got {other:?}"),
        }
        assert!(
            session.engine_mut().retained_transfers().is_empty(),
            "native echo settles the previous transfer"
        );
        lock_shared(&shared).select_pending.clear();
        lock_shared(&shared).select_busy = false;
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspacePrevious),
            true,
        );
        let again = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("second previous claim stored");
        assert_eq!(
            again.select_native_binding.as_deref(),
            Some(crate::topology::workspace_key(277).as_str()),
            "previous again binds the forward Space"
        );
        let fwd = drain_claimed_select_with(&mut state, &mut backend, again, &gates, true)
            .expect("second previous drains to its binding");
        assert_eq!((fwd.effect.from, fwd.effect.target), (1, 277));
        assert!(state.take_trailing_proof().is_none());
    }

    #[test]
    fn stale_dispatch_binds_no_plan_and_drain_touches_nothing() {
        // Watermark-move regression: a racing input advances the shared
        // watermark between acceptance and emission, so the emission gate
        // refuses the reply as stale-observation even though the sequencer
        // applied it. The command already executed core-side, so the claim
        // stores with its operation identity - but binds NO plan from any
        // reply: a stale path must never execute using old (or even fresh
        // but superseded) bindings/creations. The drain then refuses
        // without any native effect or creation, and the staged refusal
        // holds (never a false settle).
        use crate::workspace_select::{SelectGates, SelectionState};
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Queue a live terminate ahead of the dispatch so the deferred
        // observation-only retry is not worthwhile (its truth is missing
        // from the stale snapshot): the stale refusal stays plain.
        assert!(inbox.send_control(EngineJob::Terminate {
            pid: 4242,
            generation: 1
        }));
        // Between acceptance and emission, a racing input advances the
        // shared watermark: the accepted reply no longer incorporates
        // every submitted input, so emission refuses it as stale even
        // though the sequencer applied it. The command already executed
        // core-side, so the claim stores with its operation identity -
        // but binds NO plan.
        let hook: crate::runtime_shared::PostAcceptHook = Arc::new({
            let shared = Arc::clone(&shared);
            move || {
                crate::runtime::lock_shared(&shared).clock.invalidate();
            }
        });
        lock_shared(&shared)
            .post_accept_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .replace(hook);
        handle_select_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 2 }),
            true,
        );
        lock_shared(&shared)
            .post_accept_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        let stale = lock_shared(&shared)
            .select_pending
            .front()
            .cloned()
            .expect("stale command still binds its operation identity");
        assert!(
            stale
                .operation_id
                .as_deref()
                .is_some_and(|op| op.starts_with("op-")),
            "at-most-once identity rides for feedback"
        );
        assert!(
            stale.select_target_output.is_none()
                && stale.select_native_binding.is_none()
                && stale.select_creation_operation.is_none(),
            "stale refusal binds no plan from the previous reply: {:?}",
            (
                &stale.select_target_output,
                &stale.select_native_binding,
                &stale.select_creation_operation
            )
        );
        // The plan-less drain refuses before any native touch: no select
        // posts, no Space is created, nothing replays.
        let mut backend = FakeBackend::new(&[1, 277], 1, &[]);
        let mut state = SelectionState::default();
        let gates = SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        };
        let failure = crate::workspace_select::drain_claimed_select_with(
            &mut state,
            &mut backend,
            stale.clone(),
            &gates,
            true,
        )
        .expect_err("plan-less stale claim refuses");
        assert!(
            failure.reason.contains("no retained view target"),
            "{}",
            failure.reason
        );
        assert_eq!(failure.setter, FrameSetter::Refused);
        assert!(backend.selects.is_empty(), "no native select posted");
        assert_eq!(backend.creates, 0, "no native creation");
        assert!(state.take_trailing_proof().is_none());
        // Staging the drain's honest refusal (no reads - the drain never
        // touched the desktop) holds the retained transfer: the core
        // settles nothing on unreadable evidence.
        process_job(
            EngineJob::ViewAttempt {
                operation_id: stale.operation_id.clone().expect("operation rides"),
                setter: FrameSetter::Refused,
                pre_sid: None,
                post_sid: None,
                baseline_sid: None,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            &[],
            3,
            3,
            None,
            None,
            None,
            "req-3",
        )
        .expect("nonempty context reports");
        match session.engine_mut().handle_report(&report, &[]).0 {
            WorldOutcome::Accepted(_) => {}
            other => panic!("hold observation must accept, got {other:?}"),
        }
        assert_eq!(
            session.engine_mut().retained_transfers().len(),
            1,
            "unreadable refusal holds the transfer instead of settling it"
        );
    }

    #[test]
    fn raw_attempt_jobs_stage_only_against_retained_operations() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Unknown operations (forged, retired, or never issued) drop
        // without evidence - never fabricated rows, never guessed targets.
        for job in [
            EngineJob::ViewAttempt {
                operation_id: "op-404".to_owned(),
                setter: FrameSetter::Accepted,
                pre_sid: Some(1),
                post_sid: Some(277),
                baseline_sid: Some(1),
            },
            EngineJob::MemberAttempt {
                operation_id: "op-404".to_owned(),
                setter: FrameSetter::Accepted,
                pre_sid: None,
                post_sid: Some(277),
            },
        ] {
            process_job(
                job,
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            &[],
            1,
            1,
            None,
            None,
            None,
            "req-1",
        )
        .expect("nonempty context reports");
        assert!(
            report.view_attempts.is_empty() && report.member_attempts.is_empty(),
            "unknown operations stage no raw evidence"
        );
    }

    /// No-report dispatches drain nothing: staged raw attempts of every
    /// kind and the automatic-float assertions survive an empty native
    /// context and ride the next real report (latest-wins keys intact).
    /// A dropped row would silently lose proven native actuation, so the
    /// guard runs before any take.
    #[test]
    fn no_report_dispatch_retains_staged_attempts_and_assertions() {
        use tiler_core::world_report::{FrameAttempt, MemberAttempt, ViewAttempt};

        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Stage one row of every raw kind plus one automatic-float
        // assertion, bypassing resolution (latest-wins keys are what the
        // guard must preserve, not the evidence content).
        session.pending_views.insert(
            "op-9".to_owned(),
            ViewAttempt {
                operation_id: "op-9".to_owned(),
                target_output: output(),
                target_workspace: "ws-9".to_owned(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                baseline: None,
                attempt_ms: 0,
            },
        );
        session.pending_members.insert(
            ("op-9".to_owned(), "4242_7".to_owned()),
            MemberAttempt {
                operation_id: "op-9".to_owned(),
                window: WindowId("4242_7".to_owned()),
                target_output: output(),
                target_workspace: "ws-9".to_owned(),
                setter: FrameSetter::Threw,
                pre: None,
                post: None,
                attempt_ms: 0,
            },
        );
        session.pending_frames.insert(
            "4242_7".to_owned(),
            FrameAttempt {
                window: WindowId("4242_7".to_owned()),
                target: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                setter: FrameSetter::Accepted,
                readback: None,
                dpi_before: None,
                dpi_after: None,
                attempt_ms: 0,
                operation_id: Some("op-7".to_owned()),
                prewrite: None,
            },
        );
        // Fixed-hint window homed to the live domain: the next real build
        // rides it plain on the wire (the core owns admission).
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![ObservedWindow {
                    wid: 7,
                    x: 0,
                    y: 0,
                    w: 640,
                    h: 480,
                    floating: false,
                    fullscreen: false,
                    hints: tiler_core::size_hints::WindowSizeHints {
                        min_w: Some(640),
                        min_h: Some(480),
                        max_w: Some(640),
                        max_h: Some(480),
                    },
                }],
                focused_wid: None,
                roster: std::collections::BTreeMap::new(),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        retained.window_domains.insert(
            crate::runtime::window_id(4242, 7),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
            },
        );
        // Empty native context: nothing to report, nothing drained.
        let empty = Arc::new(NativeContext {
            revision: 1,
            domains: Vec::new(),
            skipped: Vec::new(),
        });
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(empty),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::NoReport => {}
            DispatchVerdict::Applied { .. } => panic!("empty context must not apply"),
            DispatchVerdict::Duplicate => panic!("empty context must not duplicate"),
            DispatchVerdict::Refused { token, .. } => {
                panic!("empty context must not refuse, got {token}")
            }
        }
        assert_eq!(session.pending_views.len(), 1, "view row retained");
        assert_eq!(session.pending_members.len(), 1, "member row retained");
        assert_eq!(session.pending_frames.len(), 1, "frame row retained");
        // Next real dispatch carries every staged row; the fixed-hint
        // window rides plain with no adapter automatic echo.
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            &[],
            1,
            1,
            None,
            None,
            None,
            "req-1",
        )
        .expect("nonempty context reports");
        assert_eq!(report.view_attempts.len(), 1, "view row rides next report");
        assert_eq!(
            report.view_attempts[0].operation_id, "op-9",
            "latest-wins identity preserved"
        );
        assert_eq!(
            report.member_attempts.len(),
            1,
            "member row rides next report"
        );
        assert_eq!(report.frames.len(), 1, "frame row rides next report");
        let wire = report
            .windows
            .iter()
            .find(|w| w.window.0 == "4242_7")
            .expect("homed window rides");
        // V4 carries no adapter float verdict: the fixed-hint fact rides
        // plain (exact geometry + hints, no float claim) and the core owns
        // admission. Prove the neutral input plus the core desired-row
        // membership over that same input.
        assert_eq!(
            wire.hints,
            Some(tiler_core::size_hints::WindowSizeHints {
                min_w: Some(640),
                min_h: Some(480),
                max_w: Some(640),
                max_h: Some(480),
            }),
            "fixed hints ride plain; the core owns admission"
        );
        // The prove engine binds to this session's actual
        // incarnation/generation with production macOS caps, so the wire
        // tokens admit core-side.
        let bound_generation = session.sequencer().bound_generation();
        let incarnation = session.incarnation().to_owned();
        let mut prove_engine =
            tiler_core::world_engine::WorldEngine::new_test_bound_with_capabilities(
                &bound_generation,
                &incarnation,
                crate::world_connection::macos_capabilities(),
            )
            .expect("bound test engine");
        match prove_engine.handle_report(&report, &[]).0 {
            tiler_core::world_engine::WorldOutcome::Accepted(reply)
            | tiler_core::world_engine::WorldOutcome::Duplicate(reply) => {
                let desired = reply
                    .windows
                    .iter()
                    .find(|w| w.lifetime.0 == "4242_7")
                    .expect("core desires the homed window");
                assert!(
                    !desired.floating,
                    "fixed hints do not force float; the core owns admission"
                );
            }
            other => panic!("fixed-hint report must accept, got {other:?}"),
        }
    }

    /// Fresh Launch drops the old lifetime's pending closes through the
    /// real engine queue: an old close staged but refused (or never
    /// reported) must not falsely close a reused same-PID/WID after
    /// relaunch. Stage an old close, Launch the new generation, observe
    /// the same WID as a new fact, and prove the report carries no stale
    /// close; a stale old-generation read then mutates nothing.
    #[test]
    fn fresh_launch_drops_stale_pending_closes_for_reused_wid() {
        use crate::runtime_shared::AppRecord;
        use crate::runtime_types::{ScopedBackendSnapshot, ScopedWindowMeta};

        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Old lifetime: PID 1001 generation 1 owns WID 7; its close was
        // staged (refused/no-report path) but never incorporated.
        lock_shared(&shared).apps.insert(
            1001,
            AppRecord {
                generation: 1,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::new(FrameSlot::new()),
                focus: Arc::new(FocusSlot::new()),
                ready: Arc::new(Mutex::new(Some(Ok(())))),
                worker: None,
                worker_name: "test-1001".to_owned(),
            },
        );
        retained.apps.insert(
            1001,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: Vec::new(),
                focused_wid: None,
                roster: BTreeMap::from([(
                    7,
                    crate::runtime_core::RosterEntry {
                        observed: Some(observed(7)),
                        spaces: vec![1],
                        fullscreen_meta: false,
                        minimized_meta: false,
                        hidden_meta: false,
                        excluded: None,

                        role: None,

                        subrole: None,
                        modal: None,
                        resizable: None,
                        layer: None,
                        app_id: None,
                        activation_policy: None,
                    },
                )]),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        session.pending_closes.insert(WindowId("1001_7".to_owned()));
        session.pending_closes.insert(WindowId("1002_8".to_owned()));
        // Relaunch admits generation 2 in shared state, then the engine
        // owns the Launch job (real queue order, not the helper).
        lock_shared(&shared)
            .apps
            .get_mut(&1001)
            .expect("old app")
            .generation = 2;
        process_job(
            EngineJob::Launch {
                pid: 1001,
                generation: 2,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            !session
                .pending_closes
                .iter()
                .any(|id| id.0.starts_with("1001_")),
            "fresh Launch drops the old lifetime's pending closes"
        );
        assert!(
            session
                .pending_closes
                .contains(&WindowId("1002_8".to_owned())),
            "unrelated PID closes survive another PID relaunch"
        );
        assert!(
            retained
                .apps
                .get(&1001)
                .expect("relaunched app")
                .roster
                .is_empty(),
            "new generation starts with an empty roster"
        );
        // New lifetime observes the same WID as a live homed fact.
        let snapshot = ScopedBackendSnapshot {
            windows: vec![observed(7)],
            focused_wid: None,
            focus_cg_owned: false,
            focus_evidence: None,
            sys_focus: None,
            global_focus: None,
            meta: vec![ScopedWindowMeta {
                wid: 7,
                spaces: vec![1],
                fullscreen: false,
                minimized: false,
                hidden: false,

                role: None,

                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            }],
            excluded: 0,
            excluded_reasons: Vec::new(),
            cg_complete: true,
            cg_owner_wids: vec![7],
            cg_alive: Vec::new(),
            revision: 9,
        };
        process_job(
            EngineJob::ScopedObservation {
                pid: 1001,
                generation: 2,
                seq: 1,
                snapshot,

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            &[],
            2,
            2,
            None,
            None,
            None,
            "req-2",
        )
        .expect("nonempty context reports");
        assert!(
            !report.closed.iter().any(|id| id.0 == "1001_7"),
            "reused WID carries no stale close"
        );
        assert!(
            report.windows.iter().any(|w| w.window.0 == "1001_7"),
            "new lifetime fact rides the report"
        );
        // Stale old-generation read mutates nothing: roster, hold, and
        // pending closes all keep the new lifetime's truth.
        let stale = ScopedBackendSnapshot {
            windows: vec![observed(7)],
            focused_wid: None,
            focus_cg_owned: false,
            focus_evidence: None,
            sys_focus: None,
            global_focus: None,
            meta: vec![ScopedWindowMeta {
                wid: 7,
                spaces: vec![1],
                fullscreen: false,
                minimized: false,
                hidden: false,

                role: None,

                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            }],
            excluded: 0,
            excluded_reasons: Vec::new(),
            cg_complete: true,
            cg_owner_wids: vec![7],
            cg_alive: Vec::new(),
            revision: 9,
        };
        let pending_before = session.pending_closes.clone();
        let incomplete_before = retained
            .apps
            .get(&1001)
            .expect("new lifetime")
            .roster_incomplete;
        process_job(
            EngineJob::ScopedObservation {
                pid: 1001,
                generation: 1,
                seq: 99,
                snapshot: stale,

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let app = retained.apps.get(&1001).expect("new lifetime");
        assert_eq!(app.generation, 2, "stale read never rolls generation back");
        assert!(app.roster.contains_key(&7), "new fact survives stale read");
        assert_eq!(
            app.roster_incomplete, incomplete_before,
            "stale read clears no new-lifetime gap"
        );
        assert_eq!(
            session.pending_closes, pending_before,
            "stale read stages no close"
        );
    }

    /// Blocker end-to-end through the real dispatch path: known window
    /// homed current, then an AX-empty snapshot with the window server
    /// still listing it. The retained id survives with a hold and no
    /// pending close stages (the old fence pruned it as a current loss).
    /// Same-shape CG-absent proof closes it instead.
    #[test]
    fn cg_alive_empty_observation_retains_current_without_pending_close() {
        use crate::runtime_shared::AppRecord;
        use crate::runtime_types::ScopedBackendSnapshot;

        for (cg_complete, cg_owner, retains) in [
            (true, vec![7u32], true),
            (true, Vec::new(), false),
            (false, Vec::new(), true),
        ] {
            let ctx = ctx();
            let mut seed = RetainedCore::default();
            seed.apps.insert(
                1001,
                RetainedApp {
                    generation: 1,
                    seq_last: 1,
                    cmd_counter: 0,
                    windows: Vec::new(),
                    focused_wid: None,
                    roster: BTreeMap::from([(
                        7,
                        crate::runtime_core::RosterEntry {
                            observed: Some(observed(7)),
                            spaces: vec![1],
                            fullscreen_meta: false,
                            minimized_meta: false,
                            hidden_meta: false,
                            excluded: None,

                            role: None,

                            subrole: None,
                            modal: None,
                            resizable: None,
                            layer: None,
                            app_id: None,
                            activation_policy: None,
                        },
                    )]),
                    roster_incomplete: false,
                    roster_cause: String::new(),
                },
            );
            seed.window_domains.insert(
                crate::runtime::window_id(1001, 7),
                DomainKey {
                    output: output(),
                    workspace: WorkspaceId(crate::topology::workspace_key(1)),
                },
            );
            let (mut session, mut retained, shared, inbox, focus_queue) =
                harness(Arc::clone(&ctx), seed);
            lock_shared(&shared).apps.insert(
                1001,
                AppRecord {
                    generation: 1,
                    terminated: false,
                    stop: Arc::new(AtomicBool::new(false)),
                    frames: Arc::new(FrameSlot::new()),
                    focus: Arc::new(FocusSlot::new()),
                    ready: Arc::new(Mutex::new(Some(Ok(())))),
                    worker: None,
                    worker_name: "test-1001".to_owned(),
                },
            );
            let snapshot = ScopedBackendSnapshot {
                windows: Vec::new(),
                focused_wid: None,
                focus_cg_owned: false,
                focus_evidence: None,
                sys_focus: None,
                global_focus: None,
                meta: Vec::new(),
                excluded: 0,
                excluded_reasons: Vec::new(),
                cg_complete,
                cg_owner_wids: cg_owner,
                cg_alive: Vec::new(),
                revision: 9,
            };
            process_job(
                EngineJob::ScopedObservation {
                    pid: 1001,
                    generation: 1,
                    seq: 2,
                    snapshot,

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let app = retained.apps.get(&1001).expect("app");
            assert_eq!(
                app.roster.contains_key(&7),
                retains,
                "cg_complete={cg_complete} retains={retains}"
            );
            if retains {
                // Staged nothing: the live id is unknown-held, never an
                // explicit close.
                assert!(
                    !session
                        .pending_closes
                        .contains(&WindowId("1001_7".to_owned())),
                    "CG-alive/unreadable stages no close"
                );
                assert!(app.roster_incomplete, "retained unknown holds the PID");
            } else {
                // Proven absence pruned the roster and staged the flat
                // close (incorporation may already have drained the
                // outbox into the dispatched report, so the covering
                // emission is the stable witness).
                assert!(
                    lock_shared(&shared).log.iter().any(|entry| {
                        entry.kind == "observation"
                            && entry.outcome == "covering-close"
                            && entry.count == 1
                    }),
                    "proven absence emits the covering close"
                );
            }
        }
    }

    /// Retained truth for the trailing send lane: subject window A on the
    /// current Space (observed focus) plus an occupied trailing, so the
    /// core pends the send under a fresh token. Both rosters carry the
    /// genuine neutral raw capture (see `retained_occupied_trailing`) so
    /// the send subject admits instead of refusing `send-window-unknown`.
    fn retained_send_subject() -> RetainedCore {
        let mut retained = retained_occupied_trailing();
        let subject = observed(7);
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 7,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![subject],
                focused_wid: Some(7),
                roster: std::collections::BTreeMap::from([(7, roster_entry(subject, vec![1]))]),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        retained.window_domains.insert(
            crate::runtime::window_id(4242, 7),
            DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
            },
        );
        retained.observed_focus = Some(tiler_core::directional::WindowId(
            crate::runtime::window_id(4242, 7),
        ));
        retained
    }

    /// Bootstrap one observation-only report through the real dispatch
    /// path: adoption converges and the trailing lifecycle mints the
    /// spare with full staged state, exactly like production observations
    /// precede input. A trailing send dispatched as the very first report
    /// would refuse (the lifecycle-minted spare has no staged domain yet
    /// in the same pass) - reported core gap, not an adapter guess.
    fn bootstrap_observation(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        ctx: &Arc<NativeContext>,
    ) {
        match dispatch_report(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } | DispatchVerdict::Duplicate => {}
            DispatchVerdict::Refused { token, .. } => {
                panic!("bootstrap observation must apply, got {token}")
            }
            DispatchVerdict::NoReport => panic!("bootstrap observation must report"),
        }
    }

    #[test]
    fn trailing_send_dispatch_binds_subject_pins() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        handle_send_action_with(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            }),
            true,
        );
        let guard = lock_shared(&shared);
        let claim = guard
            .select_pending
            .front()
            .cloned()
            .expect("send claim stored");
        // Original admitted pins survive verbatim (never re-sampled): the
        // owner binds its prepared target to exactly these.
        assert_eq!(claim.send_subject.as_deref(), Some("4242_7"));
        assert_eq!(claim.send_generation, Some(7));
        assert_eq!(
            claim.send_source,
            Some(DomainKey {
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
            })
        );
        // No admission SID (the Space may not exist yet) - only the
        // admission revision rides so the owner detects pre-preparation
        // moves.
        assert_eq!(claim.expected_sid, None);
        assert_eq!(claim.topology_revision, Some(9));
        assert_eq!(guard.diagnostics.input_select_enqueued, 1);
    }

    #[test]
    fn trailing_send_production_wrapper_armed() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
        bootstrap_observation(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            &ctx,
        );
        handle_send_action(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: true,
            }),
        );
        assert!(
            !lock_shared(&shared).select_pending.is_empty(),
            "armed production stores the trailing send claim"
        );
    }

    /// Slice-1 ordinary admission regression through the production path:
    /// injected native read (three explicit synthetic ordinary captures)
    /// -> production `build_scoped_snapshot` -> retained roster/raw ->
    /// genuine `WorldSession` handshake -> `process_job` scoped
    /// observation (real `dispatch_report`: production `codec_transact`
    /// report encode/decode, real core plus reply encode/decode,
    /// sequencer, mock frame-sink apply) — then an incomplete-read hold
    /// with complete recovery, then close-with-reflow. No live AX.
    #[test]
    fn ordinary_three_window_admission_close_reflow_through_production_dispatch() {
        use crate::classifier::{ActivationPolicy, ROLE_WINDOW, SUBROLE_STANDARD};
        use crate::ffi::{AxFrame, CgBounds};
        use crate::native_facts::{
            CgWindowEntry, NativeAppFacts, NativeReadOutcome, NativeWindowFacts,
        };
        use crate::native_tile_backend::build_scoped_snapshot;
        use crate::runtime_shared::AppRecord;
        use tiler_protocol::world_protocol::{
            CreationReceiptWire, CursorHitWire, DecodedReportV4, GestureIngressWire,
            ReportWireOptionals, decode_report_v4, encode_report_v4,
        };

        const PID: u32 = 4242;
        const GENERATION: u64 = 1;

        fn ax_frame(x: i32, y: i32, w: i32, h: i32) -> AxFrame {
            AxFrame {
                x: x as f64,
                y: y as f64,
                w: w as f64,
                h: h as f64,
            }
        }
        // Explicit synthetic ordinary capture: AXWindow/AXStandardWindow,
        // non-modal, resizable, base layer, regular app. `minimized` stays
        // a parameter so the incomplete leg can poison exactly one leg.
        fn window_facts(wid: u32, frame: AxFrame, minimized: Option<bool>) -> NativeWindowFacts {
            NativeWindowFacts {
                wid,
                role: Some(ROLE_WINDOW.to_owned()),
                subrole: Some(SUBROLE_STANDARD.to_owned()),
                fullscreen: Some(false),
                minimized,
                modal: Some(false),
                size_settable: Some(true),
                min_size: None,
                max_size: None,
                frame: Some(frame),
                cg_layer: Some(0),
                cg_onscreen: Some(true),
            }
        }
        fn cg_entry(wid: u32, frame: &AxFrame) -> CgWindowEntry {
            CgWindowEntry {
                wid,
                layer: 0,
                onscreen: true,
                bounds: CgBounds {
                    x: frame.x,
                    y: frame.y,
                    w: frame.w,
                    h: frame.h,
                },
            }
        }
        // Three nonoverlapping ordinary windows inside the 1440x900
        // display, all homed to the current Space (sid 1).
        fn three_frames() -> [AxFrame; 3] {
            [
                ax_frame(10, 30, 450, 800),
                ax_frame(470, 30, 450, 800),
                ax_frame(930, 30, 450, 800),
            ]
        }
        fn complete_outcome() -> NativeReadOutcome {
            let frames = three_frames();
            NativeReadOutcome {
                pid: PID,
                requested: 3,
                observed: 3,
                failed: 0,
                first_cause: String::new(),
                windows: vec![
                    window_facts(1, frames[0], Some(false)),
                    window_facts(2, frames[1], Some(false)),
                    window_facts(3, frames[2], Some(false)),
                ],
                cg_complete: true,
                cg_owner_wids: vec![1, 2, 3],
                cg_entries: vec![
                    cg_entry(1, &frames[0]),
                    cg_entry(2, &frames[1]),
                    cg_entry(3, &frames[2]),
                ],
            }
        }
        fn app_facts() -> NativeAppFacts {
            NativeAppFacts {
                pid: PID,
                hidden: false,
                activation_policy: ActivationPolicy::Regular,
                is_own_bundle: false,
                app_id: Some("com.example.app".to_owned()),
            }
        }
        fn snapshot_of(outcome: &NativeReadOutcome) -> crate::runtime_types::ScopedBackendSnapshot {
            build_scoped_snapshot(
                PID,
                9,
                outcome,
                &app_facts(),
                &|_| Ok(vec![1u64]),
                None,
                false,
                false,
            )
            .expect("complete ordinary read snapshots")
        }
        fn assert_nonoverlapping(desired: &[crate::runtime_types::DesiredFrame]) {
            for (i, a) in desired.iter().enumerate() {
                assert!(
                    a.w > 0 && a.h > 0,
                    "positive plan geometry for {}",
                    a.window
                );
                for b in &desired[i + 1..] {
                    let disjoint = a.x + a.w <= b.x
                        || b.x + b.w <= a.x
                        || a.y + a.h <= b.y
                        || b.y + b.h <= a.y;
                    assert!(
                        disjoint,
                        "plan geometry overlaps: {} vs {}",
                        a.window, b.window
                    );
                }
            }
        }
        fn desired_ids(desired: &[crate::runtime_types::DesiredFrame]) -> Vec<String> {
            let mut ids: Vec<String> = desired.iter().map(|d| d.window.clone()).collect();
            ids.sort();
            ids
        }
        fn covering_closes(shared: &Arc<Mutex<Shared>>) -> usize {
            lock_shared(shared)
                .log
                .iter()
                .filter(|entry| entry.outcome == "covering-close")
                .count()
        }

        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Genuine handshake plus initial settings through the protocol
        // connection route with production macOS capabilities (never a
        // test-host-owned all-supported engine).
        assert!(
            session.connection_bound(),
            "genuine handshake binds the session"
        );
        assert!(
            session.incarnation().starts_with("macos-"),
            "bound incarnation is adapter-native"
        );
        assert!(
            crate::world_connection::macos_capabilities().host_owned,
            "macOS Spaces stay host-owned"
        );
        let bound = session.sequencer().bound_generation();
        assert!(!bound.is_empty(), "handshake mints a bound generation");
        let settings = session
            .initial_settings_reply()
            .expect("initial settings ride the handshake");
        assert_eq!(settings.revision, 1);
        assert_eq!(
            settings.status,
            tiler_core::connection::SettingsStatus::Accepted
        );
        assert!(
            crate::world_connection::hard_setting_errors(settings).is_empty(),
            "empty acquisition rides core defaults"
        );
        let frames = Arc::new(FrameSlot::new());
        lock_shared(&shared).apps.insert(
            PID,
            AppRecord {
                generation: GENERATION,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::clone(&frames),
                focus: Arc::new(FocusSlot::new()),
                ready: Arc::new(Mutex::new(Some(Ok::<(), String>(())))),
                worker: None,
                worker_name: "test-4242".to_owned(),
            },
        );

        // Leg A: admit windows 1->2->3 as sequential captured snapshots
        // through the exact production read -> snapshot -> roster ->
        // dispatch -> apply path: each leg captures its own complete
        // `NativeReadOutcome`, snapshots via production
        // `build_scoped_snapshot`, and dispatches through `process_job`
        // (real `dispatch_report`: production `codec_transact` report
        // encode/decode, real core plus reply encode/decode, sequencer,
        // mock frame-sink apply).
        fn outcome_of(wids: &[u32]) -> NativeReadOutcome {
            let frames = three_frames();
            NativeReadOutcome {
                pid: PID,
                requested: wids.len(),
                observed: wids.len(),
                failed: 0,
                first_cause: String::new(),
                windows: wids
                    .iter()
                    .copied()
                    .map(|wid| window_facts(wid, frames[(wid - 1) as usize], Some(false)))
                    .collect(),
                cg_complete: true,
                cg_owner_wids: wids.to_vec(),
                cg_entries: wids
                    .iter()
                    .copied()
                    .map(|wid| cg_entry(wid, &frames[(wid - 1) as usize]))
                    .collect(),
            }
        }
        let mut seq = 0u64;
        let mut admitted: Vec<u32> = Vec::new();
        let mut rev_prev = frames.current_rev();
        for wid in [1u32, 2, 3] {
            admitted.push(wid);
            seq += 1;
            let outcome = outcome_of(&admitted);
            let snap = snapshot_of(&outcome);
            assert_eq!(
                snap.windows.len(),
                admitted.len(),
                "leg admits exactly {admitted:?}"
            );
            assert_eq!(snap.excluded, 0);
            assert!(snap.cg_alive.is_empty());
            for meta in &snap.meta {
                assert_eq!(meta.role.as_deref(), Some(ROLE_WINDOW));
                assert_eq!(meta.subrole.as_deref(), Some(SUBROLE_STANDARD));
                assert_eq!(meta.modal, Some(false));
                assert_eq!(meta.resizable, Some(true));
                assert_eq!(meta.layer, Some(0));
                assert_eq!(meta.app_id.as_deref(), Some("com.example.app"));
                assert_eq!(
                    meta.activation_policy,
                    Some(tiler_core::world_report::ActivationPolicy::Regular)
                );
                assert_eq!(meta.spaces, vec![1u64]);
            }
            process_job(
                EngineJob::ScopedObservation {
                    pid: PID,
                    generation: GENERATION,
                    seq,
                    snapshot: snap,

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let app = retained.apps.get(&PID).expect("roster owner");
            assert_eq!(app.generation, GENERATION);
            assert_eq!(app.roster.len(), admitted.len(), "roster grows by capture");
            assert!(!app.roster_incomplete, "complete capture holds nothing");
            for w in &admitted {
                assert!(app.roster.contains_key(w), "wid {w} retained");
                assert!(
                    retained
                        .window_domains
                        .contains_key(&crate::runtime::window_id(PID, *w)),
                    "wid {w} homed"
                );
            }
            assert!(session.pending_closes.is_empty());
            let desired = lock_shared(&shared).last_desired.clone();
            assert_eq!(
                desired.len(),
                admitted.len(),
                "publication carries {admitted:?}"
            );
            let mut expected: Vec<String> = admitted.iter().map(|w| format!("4242_{w}")).collect();
            expected.sort();
            assert_eq!(desired_ids(&desired), expected);
            assert_nonoverlapping(&desired);
            assert_eq!(
                lock_shared(&shared).last_core_generation,
                bound,
                "publication binds the handshake generation"
            );
            assert!(
                frames.current_rev() > rev_prev,
                "capture {admitted:?} emits a fresh frame"
            );
            rev_prev = frames.current_rev();
            assert_eq!(covering_closes(&shared), 0);
        }
        let desired_a = lock_shared(&shared).last_desired.clone();
        assert_eq!(
            desired_ids(&desired_a),
            vec![
                "4242_1".to_owned(),
                "4242_2".to_owned(),
                "4242_3".to_owned()
            ]
        );
        let rev_a = frames.current_rev();

        // Producer-bytes pin: the same retained truth rebuilds through the
        // production builder and round-trips the genuine v4 codec (wire
        // optionals inferred canonically, exactly like `codec_transact`).
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("sequencer flight free after apply");
        let verify = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            &[],
            ticket.observation_seq,
            ticket.request_seq,
            ticket.command_seq,
            ticket.command.clone(),
            None,
            &ticket.request_id,
        )
        .expect("verification report builds from admitted truth");
        session.sequencer().clear_flight();
        let mono_present = verify.monotonic_ms != 0;
        let decoded_input = DecodedReportV4 {
            report: verify,
            wire: ReportWireOptionals {
                cursor_hit: CursorHitWire::Absent,
                monotonic_ms_present: mono_present,
                frame_attempts_present: false,
                member_attempts_present: false,
                view_attempts_present: false,
                clear_attempts_present: false,
                creation_attempts_present: false,
                gesture_ingress: GestureIngressWire::Absent,
            },
            creation_receipts: Vec::<CreationReceiptWire>::new(),
        };
        let wire = encode_report_v4(&decoded_input).expect("producer bytes encode");
        let back = decode_report_v4(&wire).expect("producer bytes decode");
        assert_eq!(back.report.windows.len(), 3);
        for row in &back.report.windows {
            assert_eq!(row.role.as_deref(), Some("AXWindow"));
            assert_eq!(
                row.window_type,
                Some(tiler_core::world_report::WindowType::Normal)
            );
            assert_eq!(row.modal, Some(false));
            assert_eq!(row.resizable, Some(true));
            assert_eq!(row.layer, Some(0));
            assert_eq!(row.app_id.as_deref(), Some("com.example.app"));
            assert_eq!(
                row.activation_policy,
                Some(tiler_core::world_report::ActivationPolicy::Regular)
            );
            assert_eq!(row.process_id.as_deref(), Some("4242"));
            assert_eq!(row.host_hidden, Some(false));
        }
        assert_eq!(back.report.adapter_incarnation, session.incarnation());
        assert_eq!(back.report.core_generation, bound);

        // Leg B1: native read fence proof only — an incomplete read
        // (wid 2 minimized unknown) refuses the whole snapshot at the
        // producer, so no snapshot exists and nothing dispatches. The
        // assertions below only prove the refusal path constructs
        // nothing (vacuous for dispatch readiness: no call stages no
        // hold — the real hold proof is Leg B2 below).
        let mut bad = complete_outcome();
        bad.windows[1].minimized = None;
        let err = build_scoped_snapshot(
            PID,
            9,
            &bad,
            &app_facts(),
            &|_| Ok(vec![1u64]),
            None,
            false,
            false,
        )
        .expect_err("incomplete read refuses whole snapshot");
        assert!(err.contains("refusing whole snapshot"), "{err}");
        assert!(
            session.connection_bound(),
            "refusal path never unbinds (fence only)"
        );
        assert_eq!(
            session.sequencer().bound_generation(),
            bound,
            "refusal mints nothing"
        );
        let app = retained.apps.get(&PID).expect("roster owner");
        assert_eq!(app.generation, GENERATION, "refusal touches no lifetime");
        assert_eq!(app.roster.len(), 3, "refusal retains lifetimes");
        assert!(
            !app.roster_incomplete,
            "refusal stages no hold (it dispatches nothing)"
        );
        assert!(
            session.pending_closes.is_empty(),
            "nothing dispatched, nothing staged"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_a,
            "no dispatch writes no geometry"
        );
        assert_eq!(frames.current_rev(), rev_a, "no dispatch emits no frame");
        // Leg B2: actual incomplete inventory through the production
        // dispatch — AX captures wids 1,2 while the window-server oracle
        // is unreadable, so the covering absence of wid 3 proves
        // nothing: `process_job` retains wid 3 under a narrow
        // `cg-unreadable` hold, stages no close, emits no covering
        // close, authorizes no native writes, and never re-handshakes.
        // The whole-report hold (every scope `IncompleteInventory` until
        // complete truth lands) authorizes no native writes and preserves
        // the last stable plan truth — never a guessed close or write.
        let mut partial = outcome_of(&[1, 2]);
        partial.cg_complete = false;
        partial.cg_owner_wids = Vec::new();
        partial.cg_entries = Vec::new();
        let snap_b = snapshot_of(&partial);
        assert!(!snap_b.cg_complete, "unreadable oracle rides the snapshot");
        assert_eq!(snap_b.windows.len(), 2);
        seq += 1;
        let frames_emitted_before = lock_shared(&shared).diagnostics.frames_emitted;
        process_job(
            EngineJob::ScopedObservation {
                pid: PID,
                generation: GENERATION,
                seq,
                snapshot: snap_b,

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let app = retained.apps.get(&PID).expect("roster owner");
        assert_eq!(app.generation, GENERATION, "hold changes no lifetime");
        assert_eq!(app.roster.len(), 3, "hold retains every lifetime");
        for wid in [1u32, 2, 3] {
            assert!(app.roster.contains_key(&wid), "hold retains wid {wid}");
        }
        assert!(app.roster_incomplete, "unreadable absence stages the hold");
        assert_eq!(app.roster_cause, "cg-unreadable");
        assert!(session.pending_closes.is_empty(), "hold stages no close");
        assert_eq!(
            session.sequencer().bound_generation(),
            bound,
            "hold never re-handshakes"
        );
        assert_eq!(covering_closes(&shared), 0, "hold closes nothing");
        assert!(
            lock_shared(&shared).log.iter().any(|entry| {
                entry.kind == "scope" && entry.outcome == "held-incomplete-inventory"
            }),
            "incomplete inventory dispatches as a whole-report hold"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            frames_emitted_before,
            "hold authorizes no native writes"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_a,
            "hold preserves stable plan truth (no geometry churn)"
        );
        // Complete recovery re-admits the same lifetimes, unchanged.
        seq += 1;
        process_job(
            EngineJob::ScopedObservation {
                pid: PID,
                generation: GENERATION,
                seq,
                snapshot: snapshot_of(&complete_outcome()),

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let app = retained.apps.get(&PID).expect("roster owner");
        assert_eq!(app.generation, GENERATION);
        assert_eq!(app.roster.len(), 3, "recovery keeps the same lifetimes");
        assert_eq!(
            session.sequencer().bound_generation(),
            bound,
            "recovery never re-handshakes"
        );
        assert_eq!(covering_closes(&shared), 0, "recovery closes nothing");
        assert_eq!(
            desired_ids(&lock_shared(&shared).last_desired),
            vec![
                "4242_1".to_owned(),
                "4242_2".to_owned(),
                "4242_3".to_owned()
            ]
        );

        // Leg C: wid 3 vanishes with a complete oracle proving absence
        // (positive capture proof) — close stages, survivors reflow.
        let mut gone = complete_outcome();
        gone.requested = 2;
        gone.observed = 2;
        gone.windows.remove(2);
        gone.cg_owner_wids = vec![1, 2];
        gone.cg_entries.remove(2);
        let snap_c = snapshot_of(&gone);
        assert!(snap_c.cg_complete, "close needs a complete capture oracle");
        assert!(
            !snap_c.cg_owner_wids.contains(&3),
            "positive capture proof: oracle complete without wid 3"
        );
        seq += 1;
        process_job(
            EngineJob::ScopedObservation {
                pid: PID,
                generation: GENERATION,
                seq,
                snapshot: snap_c,

                trace_id: 0,
                intake_us: 0,
                enqueue_us: 0,
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let app = retained.apps.get(&PID).expect("roster owner");
        assert!(!app.roster.contains_key(&3), "proven-gone window drops");
        assert!(
            !retained
                .window_domains
                .contains_key(&crate::runtime::window_id(PID, 3)),
            "proven-gone homing drops"
        );
        assert_eq!(covering_closes(&shared), 1, "exactly one covering close");
        let desired_c = lock_shared(&shared).last_desired.clone();
        assert_eq!(
            desired_ids(&desired_c),
            vec!["4242_1".to_owned(), "4242_2".to_owned()]
        );
        assert_nonoverlapping(&desired_c);
        assert!(
            frames.current_rev() > rev_a,
            "close/reflow emits a fresh frame"
        );
        let area = |d: &[crate::runtime_types::DesiredFrame]| -> i64 {
            d.iter().map(|f| f.w as i64 * f.h as i64).sum()
        };
        assert!(
            area(&desired_c) > area(&desired_a),
            "reflow grows total tiled area into freed space"
        );
        assert!(
            ["4242_1", "4242_2"].iter().any(|id| {
                desired_a.iter().find(|d| &d.window == id)
                    != desired_c.iter().find(|d| &d.window == id)
            }),
            "at least one survivor takes new geometry after the close"
        );
    }

    /// Slice-1 pending-unpersisted-intent hold through the production
    /// dispatch: explicit synthetic ordinary captures (production caps) admit
    /// three windows, then a real `ToggleFloat` core command dispatches
    /// through genuine `dispatch_report`/`codec_transact`. The first
    /// intent-write reply holds native-dependent effects, geometry,
    /// presentation, and outbox; a later empty-writes reply still cannot
    /// bypass the session latch. A separate save-free control still applies.
    #[test]
    fn unpersisted_intent_hold_through_production_dispatch() {
        use crate::classifier::{ActivationPolicy, ROLE_WINDOW, SUBROLE_STANDARD};
        use crate::ffi::{AxFrame, CgBounds};
        use crate::native_facts::{NativeAppFacts, NativeReadOutcome, NativeWindowFacts};
        use crate::native_tile_backend::build_scoped_snapshot;
        use crate::runtime_shared::AppRecord;

        const PID: u32 = 4242;
        const GENERATION: u64 = 1;

        fn ax_frame(x: i32, y: i32, w: i32, h: i32) -> AxFrame {
            AxFrame {
                x: x as f64,
                y: y as f64,
                w: w as f64,
                h: h as f64,
            }
        }
        fn window_facts(wid: u32, frame: AxFrame) -> NativeWindowFacts {
            NativeWindowFacts {
                wid,
                role: Some(ROLE_WINDOW.to_owned()),
                subrole: Some(SUBROLE_STANDARD.to_owned()),
                fullscreen: Some(false),
                minimized: Some(false),
                modal: Some(false),
                size_settable: Some(true),
                min_size: None,
                max_size: None,
                frame: Some(frame),
                cg_layer: Some(0),
                cg_onscreen: Some(true),
            }
        }
        fn cg_entry(wid: u32, frame: &AxFrame) -> crate::native_facts::CgWindowEntry {
            crate::native_facts::CgWindowEntry {
                wid,
                layer: 0,
                onscreen: true,
                bounds: CgBounds {
                    x: frame.x,
                    y: frame.y,
                    w: frame.w,
                    h: frame.h,
                },
            }
        }
        fn three_frames() -> [AxFrame; 3] {
            [
                ax_frame(10, 30, 450, 800),
                ax_frame(470, 30, 450, 800),
                ax_frame(930, 30, 450, 800),
            ]
        }
        fn outcome_of(wids: &[u32]) -> NativeReadOutcome {
            let frames = three_frames();
            NativeReadOutcome {
                pid: PID,
                requested: wids.len(),
                observed: wids.len(),
                failed: 0,
                first_cause: String::new(),
                windows: wids
                    .iter()
                    .copied()
                    .map(|wid| window_facts(wid, frames[(wid - 1) as usize]))
                    .collect(),
                cg_complete: true,
                cg_owner_wids: wids.to_vec(),
                cg_entries: wids
                    .iter()
                    .copied()
                    .map(|wid| cg_entry(wid, &frames[(wid - 1) as usize]))
                    .collect(),
            }
        }
        fn app_facts() -> NativeAppFacts {
            NativeAppFacts {
                pid: PID,
                hidden: false,
                activation_policy: ActivationPolicy::Regular,
                is_own_bundle: false,
                app_id: Some("com.example.app".to_owned()),
            }
        }
        fn snapshot_of(outcome: &NativeReadOutcome) -> crate::runtime_types::ScopedBackendSnapshot {
            build_scoped_snapshot(
                PID,
                9,
                outcome,
                &app_facts(),
                &|_| Ok(vec![1u64]),
                None,
                false,
                false,
            )
            .expect("complete ordinary read snapshots")
        }

        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        assert!(
            session.connection_bound(),
            "genuine handshake binds the session"
        );
        let bound = session.sequencer().bound_generation();
        assert!(!bound.is_empty());
        let frames = Arc::new(FrameSlot::new());
        lock_shared(&shared).apps.insert(
            PID,
            AppRecord {
                generation: GENERATION,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::clone(&frames),
                focus: Arc::new(FocusSlot::new()),
                ready: Arc::new(Mutex::new(Some(Ok::<(), String>(())))),
                worker: None,
                worker_name: "test-4242".to_owned(),
            },
        );
        // Admit three explicit synthetic ordinary captures through the real
        // scoped observation path (production snapshot + dispatch + codec).
        let mut seq = 0u64;
        for wid in [1u32, 2, 3] {
            seq += 1;
            let outcome = outcome_of(&[1, 2, 3][..wid as usize]);
            process_job(
                EngineJob::ScopedObservation {
                    pid: PID,
                    generation: GENERATION,
                    seq,
                    snapshot: snapshot_of(&outcome),

                    trace_id: 0,
                    intake_us: 0,
                    enqueue_us: 0,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
        }
        assert!(
            !session.unpersisted_intent_hold(),
            "save-free admission must not latch"
        );
        let desired_base = lock_shared(&shared).last_desired.clone();
        assert_eq!(desired_base.len(), 3, "admission publishes plan truth");
        let presentation_base = lock_shared(&shared).last_presentation.clone();
        let workspaces_base = lock_shared(&shared).last_workspace_desires.clone();
        let generation_base = lock_shared(&shared).last_core_generation.clone();
        assert_eq!(generation_base, bound);
        let rev_base = frames.current_rev();
        let emitted_base = lock_shared(&shared).diagnostics.frames_emitted;
        assert!(
            focus_queue.lock().expect("focus queue").is_empty(),
            "admission moves no focus"
        );

        // First production intent-write reply: real ToggleFloat command for
        // an admitted window through genuine dispatch_report/codec_transact.
        seq += 1;
        let toggle = tiler_core::world_report::WorldCommand::Operation(
            tiler_core::boundary::CoreCommand::ToggleFloat {
                window: "4242_1".to_owned(),
                float_rect: None,
            },
        );
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            Some(toggle),
            None,
            false,
            crate::world_command_map::OP_TOGGLE_FLOAT,
            PID,
            GENERATION,
            seq,
        ) {
            DispatchVerdict::Refused { token, .. } => assert_eq!(
                token, "intent-write-unpersisted",
                "first intent-write reply must hold"
            ),
            other => panic!("toggle-float dispatch must hold, got {other:?}"),
        }
        assert!(
            session.unpersisted_intent_hold(),
            "write reply latches the session"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_base,
            "held reply publishes no geometry"
        );
        assert_eq!(
            lock_shared(&shared).last_presentation,
            presentation_base,
            "held reply publishes no presentation"
        );
        assert_eq!(
            lock_shared(&shared).last_workspace_desires,
            workspaces_base,
            "held reply publishes no workspace desires"
        );
        assert_eq!(
            lock_shared(&shared).last_core_generation,
            generation_base,
            "held reply publishes no generation"
        );
        assert!(
            focus_queue.lock().expect("focus queue").is_empty(),
            "held reply enqueues no focus outbox"
        );
        assert_eq!(
            frames.current_rev(),
            rev_base,
            "held reply emits no frame slot write"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            emitted_base,
            "held reply spends no native effect"
        );

        // Later empty-writes reply still cannot bypass the latch.
        seq += 1;
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            PID,
            GENERATION,
            seq,
        ) {
            DispatchVerdict::Refused { token, .. } => assert_eq!(
                token, "intent-write-unpersisted",
                "latched session must hold the later empty reply"
            ),
            other => panic!("latched dispatch must still hold, got {other:?}"),
        }
        assert!(
            session.unpersisted_intent_hold(),
            "latch persists across dispatches"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_base,
            "latched hold still publishes no geometry"
        );
        assert_eq!(
            lock_shared(&shared).last_presentation,
            presentation_base,
            "latched hold still publishes no presentation"
        );
        assert!(
            focus_queue.lock().expect("focus queue").is_empty(),
            "latched hold still enqueues no focus"
        );
        assert_eq!(frames.current_rev(), rev_base);
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            emitted_base,
            "latched hold still spends nothing"
        );

        // Separate save-free control: ordinary observation stays open.
        let (mut c_session, mut c_retained, c_shared, c_inbox, c_focus) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        assert!(!c_session.unpersisted_intent_hold());
        match dispatch_report(
            &mut c_session,
            &mut c_retained,
            &c_shared,
            &c_inbox,
            &c_focus,
            8,
            crate::world_runtime::DispatchContext::Synthetic,
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("save-free control must apply, got {other:?}"),
        }
        assert!(
            !c_session.unpersisted_intent_hold(),
            "save-free dispatch must not latch"
        );
    }

    /// Scripted native leg for the handoff regression below: the ONLY
    /// fake there. Models one display's user-Space row with an
    /// exact-append create and a verified select over explicit proof
    /// sets (same established shape as the producer binding tests).
    struct FakeBackend {
        display: String,
        spaces: Vec<u64>,
        current: u64,
        revision: u64,
        occupied: BTreeSet<u64>,
        next_sid: u64,
        selects: Vec<u64>,
        creates: usize,
    }

    impl FakeBackend {
        fn new(spaces: &[u64], current: u64, occupied: &[u64]) -> Self {
            Self {
                display: "display-1".to_owned(),
                spaces: spaces.to_vec(),
                current,
                revision: 9,
                occupied: occupied.iter().copied().collect(),
                next_sid: spaces.iter().copied().max().unwrap_or(0) + 1,
                selects: Vec::new(),
                creates: 0,
            }
        }
    }

    impl SelectionBackend for FakeBackend {
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            Ok(SelectionSnapshot {
                display: self.display.clone(),
                spaces: self.spaces.clone(),
                current: self.current,
            })
        }

        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }

        fn revision(&mut self) -> Option<u64> {
            Some(self.revision)
        }

        fn check_output(&mut self, output: &str) -> Result<(), String> {
            if output == "macos-display-1" {
                Ok(())
            } else {
                Err(format!(
                    "product select refuses: reply target output {output} is not the live display macos-display-1; no actuation"
                ))
            }
        }

        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            Ok(!self.occupied.contains(&sid))
        }

        fn create(
            &mut self,
            _before: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.creates += 1;
            let sid = self.next_sid;
            self.next_sid += 1;
            self.spaces.push(sid);
            self.revision += 1;
            Ok(())
        }

        fn select(
            &mut self,
            _before: &SelectionSnapshot,
            sid: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            if !self.spaces.contains(&sid) {
                return Err(crate::workspace_select::DispatchError::refused(
                    "scripted select of absent Space",
                ));
            }
            self.selects.push(sid);
            self.current = sid;
            self.revision += 1;
            Ok(())
        }
    }

    /// Post-append settle context: the appended row with the new Space
    /// current (follow arrival for sends, select arrival for selects).
    fn settled_ctx() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, false), domain(278, true)],
            skipped: Vec::new(),
        })
    }

    /// Actual producer handoff for one trailing append: the main thread's
    /// same-tick raw-attempt jobs (the `ViewAttempt`/`MemberAttempt` rows
    /// the outcome reporters enqueue) processed through the real
    /// `process_job` arms into the same `WorldSession`, then one real
    /// `build_dispatch_report` observation carrying the staged rows into
    /// the real engine. Covers the select lane (view attempt with the
    /// drain-verified Space reads) and the send lane (member attempt with
    /// the verified arrival Space) with fresh harnesses.
    ///
    /// The select lane settles on the combined report: the host-correlated
    /// creation receipt for the pending core authorization binds the
    /// appended native to the retained trailing logical, so the native echo
    /// of the appended current supersedes the retained view target, the
    /// view follows the new native with no retained transfer and the next
    /// observation-only report stays stable with no replay. The receipt is
    /// exact (operation plus logical plus output, named created desktop):
    /// the scripted backend proves the create, and the staged row carries
    /// that proof here because `build_dispatch_report` still carries no
    /// creation rows (the Stage3 drain reports them). The send lane settles
    /// on proven arrival: the correlated receipt binds the appended native
    /// to the retained target, so the staged arrival homing confirms the
    /// window transfer with no retained hold and the follow view settles to
    /// the native echo with no separate flight. Neither lane invents
    /// a binding or a verdict.
    ///
    /// The native proof rows come from the real select/send preparation
    /// over the scripted backend above (the established injection seam);
    /// job processing, feedback staging, report building, and the core
    /// settle are the production functions, never substitutes. Capability
    /// stays `false` outside the explicit `true` locals: nothing arms.
    #[test]
    fn trailing_jobs_settle_combined_report_through_real_dispatch() {
        // -- Select lane: view attempt plus native echo in one report. --
        {
            let ctx = ctx();
            let (mut session, mut retained, shared, inbox, focus_queue) =
                harness(Arc::clone(&ctx), retained_occupied_trailing());
            // Fixture admission precedes commands: Accepted alone never
            // proves admission; the Controlled row (retained View transfer
            // plus creation desire below) does.
            assert_fixture_admitted(&session, &retained);
            handle_select_action_with(
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
                ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 }),
                true,
            );
            let claim = lock_shared(&shared)
                .select_pending
                .front()
                .cloned()
                .expect("trailing select claim stored");
            let op = claim.operation_id.clone().expect("operation rides");
            // Stage1b authorization the trailing dispatch minted: the viewed
            // trailing logical needs a native backing desktop. The combined
            // report must carry the host-correlated receipt for exactly this
            // authorization, or the core holds the unmapped append without
            // adopting it. Read off the public reply surface (never engine
            // internals): the last dispatch reply publishes the desire.
            let desire = session
                .sequencer()
                .last_reply()
                .expect("select dispatch replied")
                .native_creations
                .into_iter()
                .find(|desire| desire.target_output == output())
                .expect("trailing creation desired");

            // Real native leg through the injected backend only, driven by
            // the reply plan the engine just bound (no ordinal
            // reinterpretation, single authorized creation).
            let mut backend = FakeBackend::new(&[1, 277], 1, &[277]);
            let mut select_state = crate::workspace_select::SelectionState::default();
            let success = crate::workspace_select::drain_claimed_select_with(
                &mut select_state,
                &mut backend,
                claim.clone(),
                &crate::workspace_select::SelectGates {
                    session_unlocked: true,
                    secure_active: false,
                    lock_screen: false,
                },
                true,
            )
            .expect("native append+select proves");
            assert_eq!(
                (success.effect.created, success.effect.target),
                (Some(278), 278)
            );
            assert_eq!(success.setter, FrameSetter::Accepted);
            let creation = success.creation.as_ref().expect("creation reported");
            assert_eq!(creation.operation_id, desire.operation_id);
            assert_eq!(creation.created, Some(278));

            // Actual same-tick handoff into the session: the drain-verified
            // before/after native reads ride the raw attempt row, plus the
            // drain's creation report rides the creation row - exactly once
            // under the desire's identity.
            process_job(
                EngineJob::ViewAttempt {
                    operation_id: op.clone(),
                    setter: success.setter,
                    pre_sid: success.pre,
                    post_sid: success.post,
                    baseline_sid: success.effect.baseline,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            process_job(
                EngineJob::NativeCreation {
                    operation_id: creation.operation_id.clone(),
                    logical: creation.logical.clone(),
                    output: creation.output.clone(),
                    setter: creation.setter,
                    order_before: creation.order_before.clone(),
                    order_after: creation.order_after.clone(),
                    created_sid: creation.created,
                    attempt_ms: creation.attempt_ms,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let created = settled_ctx();

            // One dispatched observation carries the staged rows with the
            // drain-verified native reads on the dedicated native keys:
            // the known source read and the not-yet-adopted append ride
            // `native_pre`/`native_post` (logical pre/post stay absent,
            // never translated or guessed adapter-side), and the drain's
            // creation report rides the creation row exactly once under
            // the desire's identity. The core translates them through its
            // own bindings at settle time.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                &[],
                3,
                3,
                None,
                None,
                None,
                "req-3",
            )
            .expect("nonempty context reports");
            // The drain's creation report rides the creation row: exactly
            // one exact-correlated receipt for the pending authorization
            // (operation plus logical plus output). M5: the observed
            // appended order alone is not causal proof, so the production
            // v4 attempt keeps `created` null even though the legacy drain
            // exposed the appended candidate.
            assert_eq!(
                report.creation_attempts.len(),
                1,
                "report carries exactly one creation attempt"
            );
            assert_eq!(
                report.creation_attempts[0].operation_id,
                desire.operation_id
            );
            assert_eq!(
                report.creation_attempts[0].logical_workspace, desire.logical_workspace,
                "receipt names the authorized logical, never a guess"
            );
            assert_eq!(
                report.creation_attempts[0].created_native.as_deref(),
                None,
                "appended observed SID never claims causal created identity"
            );
            assert_eq!(
                report.creation_attempts[0].order_before,
                ctx.domains
                    .iter()
                    .map(|domain| domain.workspace.0.clone())
                    .collect::<Vec<_>>(),
            );
            assert_eq!(
                report.creation_attempts[0].order_after,
                created
                    .domains
                    .iter()
                    .map(|domain| domain.workspace.0.clone())
                    .collect::<Vec<_>>(),
            );
            assert_eq!(report.view_attempts.len(), 1, "report carries the attempt");
            assert_eq!(report.view_attempts[0].operation_id, op);
            assert!(
                matches!(
                    report.view_attempts[0].pre.as_ref(),
                    Some(tiler_core::world_report::WorkspaceRef::Native { workspace, .. })
                        if workspace.as_str() == workspace_key(success.pre.expect("pre read"))
                ),
                "drain-verified before read rides the native pre key"
            );
            assert!(
                matches!(
                    report.view_attempts[0].post.as_ref(),
                    Some(tiler_core::world_report::WorkspaceRef::Native { workspace, .. })
                        if workspace.as_str() == workspace_key(success.post.expect("post read"))
                ),
                "drain-verified after read rides the native post key"
            );
            let reply = match session.engine_mut().handle_report(&report, &[]).0 {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(
                session.engine_mut().retained_transfers().is_empty(),
                "native echo supersedes the retained view on the combined report"
            );
            assert_eq!(
                reply
                    .outputs
                    .first()
                    .and_then(|out| out.logical_view.clone()),
                Some("ws-3".to_owned()),
                "view follows the appended native current"
            );

            // Next observation: stable, nothing pending, nothing replayed.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                &[],
                4,
                4,
                None,
                None,
                None,
                "req-4",
            )
            .expect("nonempty context reports");
            assert!(report.view_attempts.is_empty() && report.member_attempts.is_empty());
            let reply = match session.engine_mut().handle_report(&report, &[]).0 {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(session.engine_mut().retained_transfers().is_empty());
            assert_eq!(
                reply
                    .outputs
                    .first()
                    .and_then(|out| out.logical_view.clone()),
                Some("ws-3".to_owned()),
                "adopted view persists without re-binding"
            );
        }

        // -- Send lane: member attempt with the drain-verified native post. --
        // The bootstrap observation precedes the send (production order):
        // a same-report lifecycle spare has no staged domain yet (reported
        // core gap), so the dispatch runs only once the spare is staged.
        {
            let ctx = ctx();
            let (mut session, mut retained, shared, inbox, focus_queue) =
                harness(Arc::clone(&ctx), retained_send_subject());
            bootstrap_observation(
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                &ctx,
            );
            // Fixture admission precedes the send: Accepted alone never
            // proves it; the Controlled row (retained Window transfer
            // below) does.
            assert_fixture_admitted(&session, &retained);
            handle_send_action_with(
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
                ClaimedSelect::tap(InputSemanticAction::WorkspaceSend {
                    index: 0,
                    follow: true,
                }),
                true,
            );
            let claim = lock_shared(&shared)
                .select_pending
                .front()
                .cloned()
                .expect("trailing send claim stored");
            let op = claim.operation_id.clone().expect("operation rides");
            // Stage1b authorization the send dispatch minted for its trailing
            // target: the combined report must carry the host-correlated
            // receipt, or the core holds the unmapped append without adopting
            // it and the follow view cannot settle to the echo.
            let transfer_target = session
                .engine_mut()
                .retained_transfers()
                .into_iter()
                .find(|target| target.operation_id == op)
                .expect("send transfer retained")
                .target_workspace
                .clone();
            let desire = session
                .sequencer()
                .last_reply()
                .expect("send dispatch replied")
                .native_creations
                .into_iter()
                .find(|desire| {
                    desire.target_output == output() && desire.logical_workspace == transfer_target
                })
                .expect("send trailing creation desired");

            // Real native preparation through the injected backend only.
            let mut backend = FakeBackend::new(&[1, 277], 1, &[277]);
            let prep =
                crate::workspace_send::prepare_trailing_send_target(&mut backend, true, false)
                    .expect("native append proves");
            assert_eq!((prep.target_sid, prep.created), (278, Some(278)));
            // Observed arrival: the mover now homes to the new trailing.
            retained.window_domains.insert(
                crate::runtime::window_id(4242, 7),
                DomainKey {
                    output: output(),
                    workspace: WorkspaceId(crate::topology::workspace_key(278)),
                },
            );

            // Actual same-tick handoff into the session.
            process_job(
                EngineJob::MemberAttempt {
                    operation_id: op.clone(),
                    setter: FrameSetter::Accepted,
                    pre_sid: None,
                    post_sid: Some(prep.target_sid),
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let created = settled_ctx();

            // One dispatched observation carries the staged row with the
            // drain-verified arrival Space on the dedicated native post
            // key (the appended Space has no core mapping yet at stage
            // time; the core translates it at settle time once the
            // creation receipt below binds it).
            // The desire-gated creation receipt rides the production
            // staging path (not a hand-built row): the genuine pre row,
            // the owner's verified post-append row, the verified created
            // SID, and a genuine dispatch-time stamp. A zero stamp or a
            // before-echo post-order would fabricate evidence and hold
            // the transfer under ambiguity.
            process_job(
                EngineJob::NativeCreation {
                    operation_id: desire.operation_id.clone(),
                    logical: desire.logical_workspace.clone(),
                    output: output(),
                    setter: FrameSetter::Accepted,
                    order_before: vec![1, 277],
                    order_after: backend.spaces.clone(),
                    created_sid: prep.created,
                    attempt_ms: crate::input_eligibility::now_ms(),
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                &[],
                3,
                3,
                None,
                None,
                None,
                "req-3",
            )
            .expect("nonempty context reports");
            assert_eq!(
                report.member_attempts.len(),
                1,
                "report carries the attempt"
            );
            assert_eq!(report.member_attempts[0].operation_id, op);
            assert!(
                matches!(
                    report.member_attempts[0].post.as_ref(),
                    Some(tiler_core::world_report::WorkspaceRef::Native { workspace, .. })
                        if workspace.as_str() == workspace_key(prep.target_sid)
                ),
                "drain-verified arrival rides the native post key"
            );
            let reply = match session.engine_mut().handle_report(&report, &[]).0 {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            // The window transfer settles on proven arrival: the correlated
            // receipt binds the appended native to the retained target, and
            // the staged arrival homing proves the mover there (exact host
            // proof - never a false confirm, never a blind repeat). The
            // follow view settles to the native echo with no separate
            // flight.
            let held: Vec<_> = session
                .engine_mut()
                .retained_transfers()
                .into_iter()
                .filter(|target| target.operation_id == op)
                .collect();
            assert!(held.is_empty(), "window transfer settled on proven arrival");
            assert!(
                reply
                    .transfers
                    .iter()
                    .all(|target| target.operation_id != format!("{op}-follow")),
                "follow view leaves no separate flight"
            );

            // Next observation: stable settle, nothing replayed, no arrival
            // fabricated.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                &[],
                4,
                4,
                None,
                None,
                None,
                "req-4",
            )
            .expect("nonempty context reports");
            assert!(report.member_attempts.is_empty() && report.view_attempts.is_empty());
            let reply = match session.engine_mut().handle_report(&report, &[]).0 {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert_eq!(
                session
                    .engine_mut()
                    .retained_transfers()
                    .iter()
                    .filter(|target| target.operation_id == op)
                    .count(),
                0,
                "settle persists without re-actuation"
            );
            let _ = reply;
        }
    }
}

#[cfg(test)]
mod drag_gesture_handoff_tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::gesture::GesturePhase;
    use tiler_core::session::DomainKey;
    use tiler_core::size_hints::WindowSizeHints;

    use crate::config::ProbeModifier;
    use crate::drag_drop::{DragFences, DragOptions, PreviewOutcome};
    use crate::drag_session::{DragSession, PressOwner, gesture_ingress};
    use crate::focus_route::FocusRoute;
    use crate::runtime::Shared;
    use crate::runtime_core::{RetainedApp, RetainedCore};
    use crate::runtime_types::{NativeContext, NativeDomain, ObservedWindow, window_id};
    use crate::world_runtime::WorldSession;
    use crate::world_sequencer::ObservationClock;

    const UUID_1: &str = "A1B2C3D4-E5F6-47A8-B9C0-D1E2F3A4B5C6";

    fn output() -> OutputId {
        OutputId(crate::topology::output_key(1))
    }

    fn domain_key() -> DomainKey {
        DomainKey {
            output: output(),
            workspace: WorkspaceId(crate::topology::workspace_key(11)),
        }
    }

    fn ctx_current() -> Arc<NativeContext> {
        Arc::new(NativeContext {
            revision: 9,
            domains: vec![NativeDomain {
                cg_id: 1,
                uuid: Some(UUID_1.to_owned()),
                output: output(),
                workspace: WorkspaceId(crate::topology::workspace_key(11)),
                space: 11,
                current: true,
                readable: true,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                },
                visible: Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                },
            }],
            skipped: Vec::new(),
        })
    }

    fn observed_a() -> ObservedWindow {
        ObservedWindow {
            wid: 7,
            x: 100,
            y: 100,
            w: 400,
            h: 300,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn observed_b() -> ObservedWindow {
        ObservedWindow {
            wid: 9,
            x: 0,
            y: 400,
            w: 400,
            h: 300,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints::none(),
        }
    }

    fn retained_two_windows() -> RetainedCore {
        let window_a = observed_a();
        let window_b = observed_b();
        let mut retained = RetainedCore::default();
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![window_a],
                focused_wid: Some(7),
                roster: std::collections::BTreeMap::from([(
                    7,
                    crate::runtime_core::RosterEntry {
                        observed: Some(window_a),
                        spaces: vec![11],
                        fullscreen_meta: false,
                        minimized_meta: false,
                        hidden_meta: false,
                        excluded: None,
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                    },
                )]),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        retained.apps.insert(
            4243,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![window_b],
                focused_wid: None,
                roster: std::collections::BTreeMap::from([(
                    9,
                    crate::runtime_core::RosterEntry {
                        observed: Some(window_b),
                        spaces: vec![11],
                        fullscreen_meta: false,
                        minimized_meta: false,
                        hidden_meta: false,
                        excluded: None,
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                    },
                )]),
                roster_incomplete: false,
                roster_cause: String::new(),
            },
        );
        retained
            .window_domains
            .insert(window_id(4242, 7), domain_key());
        retained
            .window_domains
            .insert(window_id(4243, 9), domain_key());
        retained.observed_focus = Some(tiler_core::directional::WindowId(window_id(4242, 7)));
        retained.native_revision = 9;
        retained
    }

    fn test_shared(ctx: Arc<NativeContext>) -> Arc<Mutex<Shared>> {
        Arc::new(Mutex::new(Shared {
            config: crate::runtime_types::RuntimeConfig::default(),
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            clock: Arc::new(ObservationClock::new()),
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            main_wake: None,
            topology_revision: ctx.revision,
            native_context: Some(Arc::clone(&ctx)),
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
            arrival_captures: std::collections::BTreeMap::new(),
            selection_focus_floor_seq: 0,
            close_focus_current: None,
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: None,
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(AtomicBool::new(false)),
            stop_input: Arc::new(AtomicBool::new(false)),
            stop_logger: Arc::new(AtomicBool::new(false)),
            log_tx: None,
            diagnostics: crate::runtime_types::RuntimeDiagnostics::default(),
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: crate::runtime::empty_world_presentation_v4(),
            last_pager_bindings: Vec::new(),
            last_workspace_desires: Vec::new(),
            last_core_generation: String::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }))
    }

    type Harness = (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Inbox>,
        Arc<Mutex<Vec<FocusRequest>>>,
    );

    struct CollectSink {
        events: Mutex<Vec<crate::runtime_log::LogEvent>>,
    }

    impl crate::runtime_log::LogSink for CollectSink {
        fn write(&self, event: &crate::runtime_log::LogEvent) {
            self.events.lock().expect("sink").push(event.clone());
        }
    }

    #[test]
    #[ignore]
    fn enabled_shared_join_smoke_through_async_sink() {
        // Enabled-only (`TILER_MACOS_STAGE_TIMING=1`, run explicitly so the
        // default suite stays untraced): end-to-end join through the real
        // shared registries plus the real bounded async sink, with no native
        // calls. A worker thread notes all three maps (the production write
        // side); the main thread joins them (the engine-feedback plus
        // main-lane read side), emits bind/settle/lane-split records through
        // `emit`, and asserts the async sink observes machine-parseable
        // lines joining on one trace with input-to-feedback-arrival totals.
        // Durations mix measured clock deltas with synthetic lane counters
        // (labelled below), never native work.
        if !crate::stage_timing::enabled() {
            eprintln!("skipped without TILER_MACOS_STAGE_TIMING=1");
            return;
        }
        let ctx = ctx_current();
        let shared = test_shared(ctx);
        let sink = Arc::new(CollectSink {
            events: Mutex::new(Vec::new()),
        });
        let (tx, stop_logger, logger_handle) = crate::runtime_log::spawn_logger(
            Arc::clone(&sink) as Arc<dyn crate::runtime_log::LogSink>,
            256,
        );
        lock_shared(&shared).log_tx = Some(tx);
        let t0_us = crate::stage_timing::now_us();
        std::thread::spawn(move || {
            crate::stage_timing::note_op(11, 9001, t0_us);
            crate::stage_timing::note_cmd(4242, 1, 31, 9001, t0_us);
            crate::stage_timing::note_focus(4242, 1, 41, 9001, t0_us);
        })
        .join()
        .expect("notes join");
        let (_, op_t0, _) = crate::stage_timing::trace_for_op(11).expect("op join");
        let (_, cmd_t0, _) = crate::stage_timing::trace_for_cmd(4242, 1, 31).expect("cmd join");
        let (_, focus_t0, _) =
            crate::stage_timing::trace_for_focus(4242, 1, 41).expect("focus join");
        assert_eq!(
            (op_t0, cmd_t0, focus_t0),
            (t0_us, t0_us, t0_us),
            "shared maps reuse the intake origin across threads"
        );
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::OP_BIND,
            "select",
            11,
        );
        crate::stage_timing::record_for(
            &shared,
            4242,
            1,
            31,
            crate::stage_timing::stage::FRAME_BIND,
            "frame",
            9001,
        );
        crate::stage_timing::record_for(
            &shared,
            4242,
            1,
            41,
            crate::stage_timing::stage::FOCUS_BIND,
            "focus",
            9001,
        );
        // Intake-to-report stages under the SAME trace with the real
        // action-class token (schema/join exercise only: synthetic stage
        // times below are not comparable to the real elapsed totals).
        // One class (`focus-step`) is sufficient for the join.
        for (stage, us) in [
            (crate::stage_timing::stage::INTAKE, 0),
            (crate::stage_timing::stage::INPUT_MAP, 17),
            (crate::stage_timing::stage::QUEUE, 23),
            (crate::stage_timing::stage::REPORT, 31),
        ] {
            crate::stage_timing::record(&shared, 9001, stage, "focus-step", us);
        }
        // Worker apply + post-batch observe under the command identity
        // (synthetic durations, same join exercise), plus the readback
        // total under the trace.
        crate::stage_timing::record_for(
            &shared,
            4242,
            1,
            31,
            crate::stage_timing::stage::FRAME_APPLY,
            "applied",
            180,
        );
        crate::stage_timing::record_for(
            &shared,
            4242,
            1,
            31,
            crate::stage_timing::stage::OBSERVE,
            "snapshot",
            90,
        );
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::READBACK_TOTAL,
            "snapshot",
            310,
        );
        // Honest completion classes with measured input-to-feedback-arrival
        // totals (synthetic workload, real clock): settle ref is the trace,
        // count is origin-to-settle.
        let settle_us = crate::stage_timing::now_us().saturating_sub(t0_us);
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::FRAME_SETTLE,
            "applied",
            settle_us,
        );
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::FOCUS_SETTLE,
            "applied",
            settle_us,
        );
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::OP_SETTLE,
            "view-staged",
            settle_us,
        );
        // Select-lane split under the same trace (synthetic counters, no
        // native calls): accumulation plus the lane exec record.
        crate::stage_timing::emit_native_agg_for_trace(
            &shared,
            9001,
            crate::stage_timing::NativeTimingSummary {
                ax_read_us: 120,
                ax_read_n: 2,
                wait_us: [0, 0, 0, 400],
                wait_n: [0, 0, 0, 1],
                ..Default::default()
            },
        );
        crate::stage_timing::record(
            &shared,
            9001,
            crate::stage_timing::stage::LANE_EXEC,
            "select-exec",
            520,
        );
        lock_shared(&shared).log_tx = None;
        stop_logger.store(true, std::sync::atomic::Ordering::SeqCst);
        logger_handle.join().expect("logger joins");
        let events = sink.events.lock().expect("sink").clone();
        assert!(
            events.len() >= 16,
            "sink observes every emitted record, got {}",
            events.len()
        );
        let mut kinds = std::collections::BTreeSet::new();
        for event in &events {
            let line = crate::stage_timing::format_record(event);
            let parsed = crate::stage_timing::parse_record(&line).expect("sink line parses");
            // Worker-side bind lines join by command identity (ref is the
            // cmd/request id, count carries the trace); every other line
            // joins by trace ref directly.
            if parsed.stage == crate::stage_timing::stage::FRAME_BIND
                || parsed.stage == crate::stage_timing::stage::FOCUS_BIND
                || parsed.stage == crate::stage_timing::stage::FRAME_APPLY
                || parsed.stage == crate::stage_timing::stage::OBSERVE
            {
                assert_eq!((parsed.pid, parsed.generation), (4242, 1));
                if parsed.stage == crate::stage_timing::stage::FRAME_BIND
                    || parsed.stage == crate::stage_timing::stage::FOCUS_BIND
                {
                    assert_eq!(parsed.count, 9001, "bind maps to the trace: {line}");
                } else {
                    assert_eq!(
                        parsed.ref_id, 31,
                        "worker record keeps cmd identity: {line}"
                    );
                }
            } else {
                assert_eq!(parsed.ref_id, 9001, "line joins the trace: {line}");
            }
            // Intake-to-report stages carry the real action-class token.
            if parsed.stage == crate::stage_timing::stage::INTAKE
                || parsed.stage == crate::stage_timing::stage::INPUT_MAP
                || parsed.stage == crate::stage_timing::stage::QUEUE
                || parsed.stage == crate::stage_timing::stage::REPORT
            {
                assert_eq!(
                    parsed.outcome, "focus-step",
                    "stage names the class: {line}"
                );
            }
            kinds.insert(parsed.stage.clone());
            eprintln!("smoke {line}");
        }
        eprintln!("smoke sample is a schema/join exercise only; stage times are synthetic");
        for want in [
            crate::stage_timing::stage::INTAKE,
            crate::stage_timing::stage::INPUT_MAP,
            crate::stage_timing::stage::QUEUE,
            crate::stage_timing::stage::REPORT,
            crate::stage_timing::stage::OP_BIND,
            crate::stage_timing::stage::FRAME_BIND,
            crate::stage_timing::stage::FOCUS_BIND,
            crate::stage_timing::stage::FRAME_APPLY,
            crate::stage_timing::stage::OBSERVE,
            crate::stage_timing::stage::READBACK_TOTAL,
            crate::stage_timing::stage::FRAME_SETTLE,
            crate::stage_timing::stage::FOCUS_SETTLE,
            crate::stage_timing::stage::OP_SETTLE,
            crate::stage_timing::stage::NATIVE_AX,
            crate::stage_timing::stage::NATIVE_WAIT,
            crate::stage_timing::stage::NATIVE_WAIT_N,
            crate::stage_timing::stage::LANE_EXEC,
        ] {
            assert!(kinds.contains(want), "sink observes {want}");
        }
    }

    fn harness() -> Harness {
        let ctx = ctx_current();
        let shared = test_shared(ctx);
        // The session must share the submitter watermark clock (the
        // production route): invalidating input and the dispatch ticket
        // stamp the SAME current mark, or every reply refuses as stale.
        let clock = Arc::clone(&lock_shared(&shared).clock);
        (
            WorldSession::new(true, FocusRoute::Slps, clock, &[]),
            retained_two_windows(),
            shared,
            Arc::new(Inbox::new(16)),
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    /// Fixture admission gate: the genuine handshake binds the session
    /// (adapter-native incarnation, production macOS caps, minted
    /// generation) and both handbuilt rosters carry explicit native-shaped
    /// ordinary raw facts homed to space-11. A report `Accepted` alone
    /// never proves admission; the caller must also observe the Controlled
    /// row (preview `Show` slot or committed terminal below).
    fn assert_fixture_admitted(session: &WorldSession, retained: &RetainedCore) {
        assert!(session.connection_bound(), "genuine handshake binds");
        assert!(
            session.incarnation().starts_with("macos-"),
            "bound incarnation is adapter-native"
        );
        assert!(
            crate::world_connection::macos_capabilities().host_owned,
            "macOS Spaces stay host-owned"
        );
        assert!(
            !session.sequencer().bound_generation().is_empty(),
            "handshake mints a bound generation"
        );
        for (pid, wid) in [(4242u32, 7u32), (4243u32, 9u32)] {
            let app = retained.apps.get(&pid).expect("fixture app retained");
            let entry = app.roster.get(&wid).expect("fixture roster row retained");
            assert_eq!(entry.spaces, vec![11], "fixture row homed to space-11");
            assert_eq!(entry.role.as_deref(), Some(crate::classifier::ROLE_WINDOW));
            assert_eq!(
                entry.subrole.as_deref(),
                Some(crate::classifier::SUBROLE_STANDARD)
            );
            assert!(!app.roster_incomplete, "fixture holds nothing");
        }
    }

    fn fences_ok() -> DragFences {
        DragFences {
            enabled: true,
            session_unlocked: true,
            secure_input_active: false,
            scope_allowed: true,
            fullscreen_active: false,
            revision_fresh: true,
            focus_matches: true,
        }
    }

    const CONTROL_FLAGS: u64 = 0x0004_0000;

    fn press_owner() -> PressOwner {
        PressOwner {
            pid: 4242,
            wid: 7,
            window: window_id(4242, 7),
            fullscreen: false,
            revision: 9,
            start_frame: Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        }
    }

    fn down_hit(x: i32, y: i32) -> crate::ffi::DragMouseHit {
        crate::ffi::DragMouseHit {
            kind: crate::ffi::DragMouseKind::Down,
            x,
            y,
            flags: CONTROL_FLAGS,
        }
    }

    fn arm(session: &DragSession) {
        match session.on_down(down_hit(300, 250), &Ok(press_owner()), fences_ok()) {
            crate::drag_session::HitIntent::Down(crate::drag_session::Downcome::Armed {
                window,
            }) => {
                assert_eq!(window, "4242_7");
            }
            other => panic!("press must arm, got {other:?}"),
        }
    }

    /// Mirror the product submit bump: each gesture ingress is invalidating
    /// input, so the ticket stamps the bumped watermark.
    fn submit(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        inbox: &Arc<Inbox>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        ingress: tiler_core::gesture::GestureIngress,
        press_revision: Option<u64>,
    ) -> DragGestureReply {
        crate::runtime_shared::invalidate_clock(shared);
        handle_drag_gesture(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            8,
            ingress,
            press_revision,
        )
    }

    #[test]
    fn gesture_start_binds_then_update_previews_core_slot() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        // Fixture admission precedes the gesture: Accepted alone never
        // proves it; the Controlled row (preview Show slot below) does.
        assert_fixture_admitted(&session, &retained);
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        // Started binds only: no overlay at the down point itself.
        let started = drag.start_ingress().expect("armed gesture offers Started");
        assert_eq!(started.phase, GesturePhase::Started);
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        assert!(matches!(
            reply.preview,
            PreviewOutcome::HideTransient { .. }
        ));
        assert!(reply.settled.is_none(), "start never settles");
        // Moved sample resolves the core insertion slot through the SAME
        // gesture lifecycle the offline harness traverses.
        match drag.on_drag(10, 450, fences_ok()) {
            crate::drag_session::HitIntent::Sample(
                crate::drag_session::SampleIntent::Preview { ingress },
            ) => {
                assert_eq!(ingress.phase, GesturePhase::Updated);
                let reply = submit(
                    &mut session,
                    &mut retained,
                    &shared,
                    &inbox,
                    &focus_queue,
                    ingress,
                    Some(9),
                );
                match reply.preview {
                    PreviewOutcome::Show { rect, target, .. } => {
                        assert!(rect.w > 0 && rect.h > 0, "slot rect is real geometry");
                        assert_eq!(target.0, "4243_9", "core-resolved neighbor, never a guess");
                        assert_ne!(target.0, "4242_7", "target is never the mover");
                        drag.note_preview(&PreviewOutcome::Show {
                            rect,
                            hover_prior: None,
                            target: target.clone(),
                        });
                        // Above-target carrier order verifies against the
                        // fake window-server order (carrier above target).
                        let overlay = drag.overlay_for(rect);
                        match overlay {
                            crate::drag_drop::DragOverlay::ShowFillAbove { ordering, .. } => {
                                assert_eq!(ordering, crate::drag_drop::DragOrdering::AboveTarget);
                            }
                            other => panic!("preview must show above, got {other:?}"),
                        }
                        assert!(
                            crate::drag_session::carrier_above_verified(&[55, 9, 7], 55, 9),
                            "carrier above the joined target"
                        );
                    }
                    other => panic!("moved pointer must resolve a slot, got {other:?}"),
                }
                assert!(reply.settled.is_none(), "update never settles");
            }
            other => panic!("moved sample must preview, got {other:?}"),
        }
        assert!(drag.active());
    }

    #[test]
    fn gesture_release_commits_once_and_replay_stays_dead() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        assert_fixture_admitted(&session, &retained);
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        let started = drag.start_ingress().expect("Started");
        let _ = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        match drag.on_drag(10, 450, fences_ok()) {
            crate::drag_session::HitIntent::Sample(
                crate::drag_session::SampleIntent::Preview { ingress },
            ) => {
                let _ = submit(
                    &mut session,
                    &mut retained,
                    &shared,
                    &inbox,
                    &focus_queue,
                    ingress,
                    Some(9),
                );
            }
            other => panic!("preview expected, got {other:?}"),
        }
        // Native post-gesture observation: the window server moved the
        // window with the gesture (same size, new origin), exactly like the
        // live AX readback the release intent derives from.
        retained.apps.get_mut(&4242).expect("mover").windows[0].x = 100;
        retained.apps.get_mut(&4242).expect("mover").windows[0].y = 400;
        let (ingress, press_revision) = match drag.on_up(10, 450) {
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Drop {
                ingress,
                press_revision,
            }) => (ingress, press_revision),
            other => panic!("release must drop, got {other:?}"),
        };
        assert_eq!(press_revision, 9);
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            ingress.clone(),
            Some(press_revision),
        );
        let settled = reply.settled.expect("release incorporates a terminal");
        assert!(settled.committed, "a moved release commits");
        // Exactly-once main-side: the trailing release ignores.
        assert!(matches!(
            drag.on_up(10, 450),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        ));
        // Replay of the same terminal never commits again (at-most-once by
        // stable gesture id).
        let replay = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            ingress,
            Some(press_revision),
        );
        assert!(
            replay.settled.is_none(),
            "consumed id replays dead with no second terminal"
        );
    }

    #[test]
    fn gesture_cancel_clears_without_commit() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        let started = drag.start_ingress().expect("Started");
        let _ = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        let cancelled = drag.on_escape().expect("Escape cancels the held gesture");
        assert_eq!(cancelled.phase, GesturePhase::Cancelled);
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            cancelled,
            None,
        );
        if let Some(settled) = reply.settled {
            assert!(!settled.committed, "cancel clears with no commit");
        }
        assert!(!drag.active());
        assert!(matches!(
            drag.on_up(10, 450),
            crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
        ));
    }

    #[test]
    fn stale_press_hidden_domain_and_unknown_window_cancel_dead() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        let started = drag.start_ingress().expect("Started");
        // Drifted press revision refuses stale before dispatch (never a
        // late commit, never a replay).
        let stale = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started.clone(),
            Some(8),
        );
        assert!(matches!(stale.preview, PreviewOutcome::CancelDead { .. }));
        assert!(stale.settled.is_none());
        // Hidden-domain subject cancels dead: the current Space proof is
        // valid and current, never a guessed target.
        let mut hidden_ctx = (*ctx_current()).clone();
        hidden_ctx.domains[0].current = false;
        let hidden_shared = test_shared(Arc::new(hidden_ctx));
        let hidden = submit(
            &mut session,
            &mut retained,
            &hidden_shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        assert!(matches!(
            hidden.preview,
            PreviewOutcome::CancelDead {
                reason: "hidden-domain"
            }
        ));
        assert!(hidden.settled.is_none());
        // Unknown window cancels dead without touching the core.
        let unknown = gesture_ingress(
            999,
            "9999_1",
            GesturePhase::Updated,
            Some((10, 450)),
            (300, 250),
            Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            unknown,
            Some(9),
        );
        assert!(matches!(
            reply.preview,
            PreviewOutcome::CancelDead {
                reason: "unknown-window"
            }
        ));
        assert!(reply.settled.is_none());
    }

    #[test]
    fn stale_watermark_suppresses_preview_and_new_generation_refuses() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        let started = drag.start_ingress().expect("Started");
        let _ = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        // Racing input lands mid-flight: the in-flight preview refuses as
        // stale-observation and actuates nothing (no slot, no commit).
        let clock = Arc::clone(&lock_shared(&shared).clock);
        *lock_shared(&shared)
            .midflight_hook
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(move || {
            clock.invalidate();
        }));
        let ingress = match drag.on_drag(10, 450, fences_ok()) {
            crate::drag_session::HitIntent::Sample(
                crate::drag_session::SampleIntent::Preview { ingress },
            ) => ingress,
            other => panic!("preview expected, got {other:?}"),
        };
        let stale = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            ingress,
            Some(9),
        );
        assert!(
            matches!(stale.preview, PreviewOutcome::HideTransient { .. }),
            "stale preview hides, never shows a stale slot"
        );
        assert!(stale.settled.is_none());
        *lock_shared(&shared)
            .midflight_hook
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        // A new generation refuses without actuation (no late commit on a
        // superseded generation).
        session.sequencer().bind_generation("gen-2");
        let fresh = gesture_ingress(
            4242,
            "4242_7",
            GesturePhase::Updated,
            Some((10, 450)),
            (300, 250),
            Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        );
        let refused = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            fresh,
            Some(9),
        );
        assert!(refused.settled.is_none(), "new generation settles nothing");
    }

    #[test]
    fn orphan_cancelled_consumes_without_influence_and_new_gesture_binds_clean() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        assert_fixture_admitted(&session, &retained);
        let frame = Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        };
        // Started never reached core: the orphan Cancelled for a fresh id
        // consumes its identity and records an uncommitted
        // terminal with no retained session, no slot, and no commit.
        let orphan = gesture_ingress(
            50,
            "4242_7",
            GesturePhase::Cancelled,
            Some((300, 250)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            orphan.clone(),
            Some(9),
        );
        assert!(
            matches!(reply.preview, PreviewOutcome::HideTransient { .. }),
            "orphan cancel hides, never shows a stale slot"
        );
        match reply.settled {
            Some(settled) => {
                assert_eq!(settled.id, 50);
                assert!(!settled.committed, "orphan cancel never commits");
            }
            None => panic!("orphan cancel records its uncommitted terminal"),
        }
        // Replay of the consumed orphan stays dead with no second terminal.
        let replay = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            orphan,
            Some(9),
        );
        assert!(replay.settled.is_none(), "consumed orphan replays dead");
        assert!(matches!(
            replay.preview,
            PreviewOutcome::HideTransient { .. }
        ));
        // A later real gesture binds clean on a fresh id: no stale preview,
        // commit, or influence from the orphan.
        let started = gesture_ingress(
            51,
            "4242_7",
            GesturePhase::Started,
            Some((300, 250)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            started,
            Some(9),
        );
        assert!(
            matches!(reply.preview, PreviewOutcome::HideTransient { .. }),
            "start binds only, never previews"
        );
        assert!(reply.settled.is_none(), "start never settles");
        let updated = gesture_ingress(
            51,
            "4242_7",
            GesturePhase::Updated,
            Some((10, 450)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            updated,
            Some(9),
        );
        match reply.preview {
            PreviewOutcome::Show { target, .. } => {
                assert_eq!(target.0, "4243_9", "slot resolves the live neighbor");
            }
            other => panic!("moved sample must preview, got {other:?}"),
        }
        assert!(reply.settled.is_none(), "update never settles");
    }

    #[test]
    fn late_old_terminal_cannot_mutate_newer_gesture_and_capture_releases() {
        let (mut session, mut retained, shared, inbox, focus_queue) = harness();
        assert_fixture_admitted(&session, &retained);
        let frame = Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        };
        // Old gesture binds and previews its slot.
        let old_started = gesture_ingress(
            61,
            "4242_7",
            GesturePhase::Started,
            Some((300, 250)),
            (300, 250),
            frame,
        );
        let _ = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            old_started,
            Some(9),
        );
        let old_updated = gesture_ingress(
            61,
            "4242_7",
            GesturePhase::Updated,
            Some((10, 450)),
            (300, 250),
            frame,
        );
        let _ = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            old_updated,
            Some(9),
        );
        // A newer gesture supersedes the old session before any old
        // terminal arrives.
        let new_started = gesture_ingress(
            62,
            "4242_7",
            GesturePhase::Started,
            Some((300, 250)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            new_started,
            Some(9),
        );
        assert!(
            matches!(reply.preview, PreviewOutcome::HideTransient { .. }),
            "superseding start binds only"
        );
        assert!(reply.settled.is_none(), "superseding start settles nothing");
        // Native post-gesture observation: the window server moved the
        // window with the gesture (same size, new origin).
        retained.apps.get_mut(&4242).expect("mover").windows[0].x = 100;
        retained.apps.get_mut(&4242).expect("mover").windows[0].y = 400;
        // Late terminal for the OLD id while the newer session is
        // retained: held with an uncommitted drain record, never a commit
        // and never a stale slot. The newer session is untouched.
        let late = gesture_ingress(
            61,
            "4242_7",
            GesturePhase::Released,
            Some((10, 450)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            late,
            Some(9),
        );
        match reply.settled {
            Some(settled) => {
                assert_eq!(settled.id, 61);
                assert!(!settled.committed, "late old terminal never commits");
            }
            None => panic!("late old terminal records its uncommitted drain"),
        }
        assert!(
            matches!(reply.preview, PreviewOutcome::HideTransient { .. }),
            "late old terminal shows no stale slot"
        );
        // The newer gesture still previews its own slot, then commits its
        // own release exactly once.
        let new_updated = gesture_ingress(
            62,
            "4242_7",
            GesturePhase::Updated,
            Some((10, 450)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            new_updated,
            Some(9),
        );
        assert!(
            matches!(reply.preview, PreviewOutcome::Show { .. }),
            "newer gesture previews intact after the late old terminal"
        );
        let new_released = gesture_ingress(
            62,
            "4242_7",
            GesturePhase::Released,
            Some((10, 450)),
            (300, 250),
            frame,
        );
        let reply = submit(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            new_released,
            Some(9),
        );
        let settled = reply.settled.expect("newer release settles its terminal");
        assert!(settled.committed, "newer gesture commits exactly once");
        assert_eq!(settled.id, 62);
        // Main-side capture/carrier release through the production session:
        // Escape clears the capture, the trailing up balances to idle, and
        // a degenerate preview rect hides the carrier instead of rendering.
        let drag = DragSession::new(DragOptions::default(), ProbeModifier::Ctrl);
        arm(&drag);
        assert!(drag.active());
        let cancelled = drag.on_escape().expect("Escape cancels the held gesture");
        assert_eq!(cancelled.phase, GesturePhase::Cancelled);
        assert!(!drag.active(), "cancel releases the single-flight capture");
        assert!(
            matches!(
                drag.on_up(10, 450),
                crate::drag_session::HitIntent::Release(crate::drag_session::ReleaseIntent::Ignore)
            ),
            "post-cancel up balances to idle, never a drop"
        );
        assert!(drag.cancel_ingress().is_none());
        assert!(drag.start_ingress().is_none());
        assert!(
            crate::drag_session::preview_outer_appkit(
                Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 10,
                },
                Some(900.0)
            )
            .is_none(),
            "degenerate rect hides the carrier"
        );
        assert!(
            !crate::drag_session::carrier_above_verified(&[9, 7], 55, 9),
            "absent carrier never verifies above the target"
        );
    }
}
