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
    },
    InputAction {
        action: InputSemanticAction,
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
                | Self::AxResult { .. }
                | Self::FocusResult { .. }
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
    fn recv(&self, stop: &AtomicBool) -> Option<EngineJob> {
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
    // The world engine carries the shared fixed-size admission opt-in
    // (D1-D8) via its constructor; the configured predicate rides every
    // report ingress and applies to subsequent admissions only, never
    // reclassifying retained windows. The watermark clock is the shared
    // input mark (submitters advance it; dispatches only stamp it).
    let (predicate, native_mode, focus_route, clock) = {
        let guard = lock_shared(&shared);
        (
            guard.config.fixed_size_predicate,
            matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            ),
            guard.config.focus_route,
            Arc::clone(&guard.clock),
        )
    };
    let mut session = WorldSession::new(predicate, native_mode, focus_route, clock);
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

fn process_job(
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
                Some(window) => retained.observed_focus = Some(window),
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
        } => {
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
        EngineJob::InputAction { action } => {
            // Unsupported workspace actions (trailing stay-0,
            // relative sends, outputs) are recognized but
            // undispatched: no unsupported transfer and no output-send
            // route runs from the product tile (see
            // `workspace_unsupported_reason` for the exact seam). Counted
            // and logged with a static token, never faked.
            if let Some(outcome) = workspace_unsupported_outcome(&action) {
                lock_shared(shared).diagnostics.input_refused_unsupported += 1;
                emit(shared, 0, 0, 0, "input", outcome, 0);
                return;
            }
            // Armed select actions dispatch with no focused-window
            // subject (empty Spaces navigate): one planner-owned world
            // command plus one pending claim for the main-thread tile
            // drain. Targeting resolves core-side; the accepted reply
            // binds the drain plan.
            if is_armed_select(&action) {
                handle_select_action(
                    session,
                    retained,
                    shared,
                    inbox,
                    focus_queue,
                    focus_capacity,
                    crate::runtime_types::ClaimedSelect::tap(action),
                );
                return;
            }
            // Armed numbered send-follow 1..9 dispatches through the
            // managed-send lane (see [`admit_send_claim`],
            // [`handle_send_action`]).
            if crate::workspace_input::is_armed_send(&action) {
                handle_send_action(
                    session,
                    retained,
                    shared,
                    inbox,
                    focus_queue,
                    focus_capacity,
                    crate::runtime_types::ClaimedSelect::tap(action),
                );
                return;
            }
            // The subject is observed truth, never planned focus.
            let Some(window) = retained.observed_focus.clone() else {
                lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                emit(shared, 0, 0, 0, "input", "refused-no-focus", 0);
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
                    return;
                };
                let current = lock_shared(shared)
                    .native_context
                    .clone()
                    .is_some_and(|ctx| ctx.is_current(&key));
                if !current {
                    lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                    emit(shared, 0, 0, 0, "input", "refused-hidden-domain", 0);
                    return;
                }
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
                Some(tiler_core::world_report::WorldCommand::Operation(command)),
                None,
                false,
                op,
                0,
                0,
                0,
            );
        }
        EngineJob::IndicatorSelect {
            action,
            expected_sid,
            ui_generation,
            topology_revision,
        } => {
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
        }
        EngineJob::MemberAttempt {
            operation_id,
            setter,
            pre_sid,
            post_sid,
        } => {
            session.stage_member_attempt(&operation_id, setter, pre_sid, post_sid);
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
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
    use crate::runtime_core::{RetainedApp, RetainedCore};
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
            selection_focus_floor_seq: 0,
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
            last_presentation: tiler_core::world_presentation::WorldPresentation::default(),
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

    fn session() -> WorldSession {
        WorldSession::new(
            tiler_core::size_hints::FixedSizePredicate::BothAxes,
            true,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
        )
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
    /// command.
    fn retained_occupied_trailing() -> RetainedCore {
        let mut retained = RetainedCore::default();
        retained.apps.insert(
            4243,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(9)],
                focused_wid: None,
                roster: std::collections::BTreeMap::new(),
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
        // `fx-<seq>` through the real core, and the real dispatch binds the
        // deferred intent with the genuine expected SID taken from the
        // reply's live native binding (never a guessed Space). The full
        // production cycle runs between the legs: the away select drains
        // through the scripted native row, stages its proven attempt, and
        // settles on the moved observation — then the return mints its own
        // remembered focus and supersedes the away intent.
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
        // that retained target — parsed, never guessed.
        let reply = session
            .sequencer()
            .last_reply()
            .expect("applied reply retained");
        let binding = reply
            .workspaces
            .iter()
            .find(|row| {
                row.output.0 == transfer.target_output.0 && row.id == transfer.target_workspace
            })
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
        // The window is exactly the reply's correlated focus outcome.
        let seq: u64 = pending
            .operation_id
            .strip_prefix("op-")
            .expect("op-<seq> identity")
            .parse()
            .expect("numeric seq");
        assert_eq!(pending.command_seq, seq);
        let effect = reply
            .effects
            .iter()
            .find(|effect| effect.is_explicit_focus_for(seq))
            .expect("reply carries the correlated focus outcome");
        assert_eq!(pending.window, effect.target);
        // The away leg's superseded floor never blocks the newer bind.
        assert!(
            lock_shared(&shared).selection_focus_floor_seq <= seq,
            "floor tracks the binding command"
        );
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
        match session.engine_mut().handle_report(&report) {
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
                pre: Some("ws-1".to_owned()),
                post: None,
                baseline: None,
                native_pre: None,
                native_post: None,
                native_baseline: None,
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
                native_pre: None,
                native_post: None,
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
        assert!(
            !wire.floating && !wire.fixed_auto,
            "fixed hints ride plain; the core owns admission"
        );
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
    /// core pends the send under a fresh token.
    fn retained_send_subject() -> RetainedCore {
        let mut retained = retained_occupied_trailing();
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 7,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed(7)],
                focused_wid: Some(7),
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
            // (operation plus logical plus output, named created desktop).
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
                Some(workspace_key(success.effect.target).as_str()),
                "receipt names the verified appended desktop"
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
            assert_eq!(
                report.view_attempts[0].pre, None,
                "logical pre stays absent, never translated adapter-side"
            );
            assert_eq!(
                report.view_attempts[0].post, None,
                "logical post stays absent, never guessed adapter-side"
            );
            assert_eq!(
                report.view_attempts[0].native_pre.as_deref(),
                Some(workspace_key(success.pre.expect("pre read")).as_str()),
                "drain-verified before read rides the native pre key"
            );
            assert_eq!(
                report.view_attempts[0].native_post.as_deref(),
                Some(workspace_key(success.post.expect("post read")).as_str()),
                "drain-verified after read rides the native post key"
            );
            let reply = match session.engine_mut().handle_report(&report) {
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
            let reply = match session.engine_mut().handle_report(&report) {
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
            assert_eq!(
                report.member_attempts[0].post, None,
                "logical post stays absent, never guessed adapter-side"
            );
            assert_eq!(
                report.member_attempts[0].native_post.as_deref(),
                Some(workspace_key(prep.target_sid).as_str()),
                "drain-verified arrival rides the native post key"
            );
            let reply = match session.engine_mut().handle_report(&report) {
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
            let reply = match session.engine_mut().handle_report(&report) {
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
        let mut retained = RetainedCore::default();
        retained.apps.insert(
            4242,
            RetainedApp {
                generation: 1,
                seq_last: 1,
                cmd_counter: 0,
                windows: vec![observed_a()],
                focused_wid: Some(7),
                roster: std::collections::BTreeMap::new(),
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
                windows: vec![observed_b()],
                focused_wid: None,
                roster: std::collections::BTreeMap::new(),
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
            selection_focus_floor_seq: 0,
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
            last_presentation: tiler_core::world_presentation::WorldPresentation::default(),
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

    fn harness() -> Harness {
        let ctx = ctx_current();
        let shared = test_shared(ctx);
        // The session must share the submitter watermark clock (the
        // production route): invalidating input and the dispatch ticket
        // stamp the SAME current mark, or every reply refuses as stale.
        let clock = Arc::clone(&lock_shared(&shared).clock);
        (
            WorldSession::new(
                tiler_core::size_hints::FixedSizePredicate::BothAxes,
                true,
                FocusRoute::Slps,
                clock,
            ),
            retained_two_windows(),
            shared,
            Arc::new(Inbox::new(16)),
            Arc::new(Mutex::new(Vec::new())),
        )
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
