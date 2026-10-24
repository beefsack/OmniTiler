#![forbid(unsafe_code)]

//! Phase 1 runtime owners and per-owner machinery (implementation).
//!
//! This module holds the thread bodies plus the machinery they own: the
//! wake-driven [`Inbox`] and per-app latest-wins [`FrameSlot`]. Retained
//! core state and the [`tiler_core`] event constructor live in
//! [`crate::runtime_core`]; the public typed surface lives in
//! [`crate::runtime`], which alone spawns and supervises these owners.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

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
    select_command, select_output, send_command,
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
    /// interpreting the ordinal against a different generation. Data lane
    /// (drops counted when full, never blocks AppKit).
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
    /// lane (never silently dropped — the owner always gets its reply or
    /// an engine-dead refusal), strict FIFO with observations, so the reply
    /// reflects every observation submitted before the query. The owner
    /// treats a missing/late reply as unknown facts and refuses without
    /// actuation — native OS reads supplement these facts, never replace
    /// them.
    ManagedFactsQuery {
        window: String,
        reply: mpsc::Sender<Option<crate::runtime_core::EngineManagedFacts>>,
    },
    /// Read-only retained occupancy query for the menu-bar indicator:
    /// snapshot the engine's per-domain managed membership (current and
    /// hidden alike) plus the context revision it was homed under, and
    /// reply with the pure report. Stages nothing, plans nothing, touches
    /// no reconciler state. Control lane (never silently dropped — the
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
    /// terminal, if any). Control lane (never silently dropped — the main
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
    /// `request_id`; dispatch truth only — arrival is verified by later
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
    /// retained transfer target by operation identity and translates the
    /// native SIDs to logical views through its own adoption mapping (a
    /// live core lookup, never adapter mapping); unknown operations or
    /// unmapped SIDs drop without evidence. Staged into the next world
    /// report so the core settles its retained planner-owned view target.
    /// Data lane (drops counted when full, never blocks); a dropped row
    /// merely delays settlement — retained core targets never re-actuate
    /// without fresh proof.
    ViewAttempt {
        operation_id: String,
        setter: tiler_core::world_report::FrameSetter,
        /// Native Space id read before the drain actuated (`None` unread).
        pre_sid: Option<u64>,
        /// Native Space id read after the drain actuated (`None` unread).
        post_sid: Option<u64>,
    },
    /// Raw mechanical membership-attempt evidence from one proven
    /// send-drain outcome (never a verdict): the setter disposition plus
    /// the drain-verified arrival Space. The engine resolves the retained
    /// transfer target (window plus target) by operation identity and
    /// translates the native SID through its own adoption mapping (a live
    /// core lookup, never adapter mapping); unknown operations or
    /// untranslatable rows drop without evidence. Staged into the next
    /// world report so the core settles its retained window transfer.
    MemberAttempt {
        operation_id: String,
        setter: tiler_core::world_report::FrameSetter,
        /// Native Space id of the drain-verified arrival (`None` unless
        /// the drain proved arrival there).
        post_sid: Option<u64>,
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
/// terminates qualify — commands and feedback change planned state, not
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
    /// fault injection) bypass saturation but keep FIFO order with data.
    fn is_data(&self) -> bool {
        matches!(
            self,
            Self::Observation { .. }
                | Self::ScopedObservation { .. }
                | Self::InputAction { .. }
                | Self::IndicatorSelect { .. }
                | Self::ViewAttempt { .. }
                | Self::MemberAttempt { .. }
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
/// earlier job was processed); only data admission is bounded
/// (`try_send_data` drops counted when full) while control admission and
/// stop admission bypass saturation.
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
    /// honest snapshot, and a drop never dispatches a successor — so
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

/// Latest-wins frame slot for one app worker. `set_latest` overwrites the
/// whole pending desired set so rapid re-plans coalesce but never lose the
/// final desired state — including a whole EMPTY desired set, which supersedes
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
}

#[derive(Debug)]
struct FrameSlotState {
    latest: Option<Vec<FrameCommand>>,
    stopped: bool,
    rev: u64,
}

impl FrameSlot {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(FrameSlotState {
                latest: None,
                stopped: false,
                rev: 0,
            }),
            wake: Condvar::new(),
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

    /// Blocking take used by unit tests; production workers multiplex with
    /// [`FrameSlot::wait_versioned_timeout`].
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

    /// Bounded variant for workers that multiplex frame commands with an
    /// observation pump: waits at most `timeout` for a pending set, then
    /// returns `None` so the caller can poll its backend and retry. Command
    /// wake is still immediate (`set_latest` notifies); the bound only caps
    /// observation cadence and stop latency. A `None` is ambiguous by
    /// design — the caller re-checks `stop`/`is_stopped` to tell a quiet
    /// slice from teardown.
    pub(crate) fn wait_versioned_timeout(
        &self,
        stop: &AtomicBool,
        timeout: Duration,
    ) -> Option<(u64, Vec<FrameCommand>)> {
        let mut state = self.state.lock().expect("frame slot mutex");
        if state.stopped || stop.load(Ordering::SeqCst) {
            return None;
        }
        if let Some(cmds) = state.latest.take() {
            return Some((state.rev, cmds));
        }
        let (guard, _) = self
            .wake
            .wait_timeout(state, timeout)
            .expect("frame slot mutex");
        state = guard;
        if state.stopped || stop.load(Ordering::SeqCst) {
            return None;
        }
        state.latest.take().map(|cmds| (state.rev, cmds))
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
/// command (a newer desired focus supersedes — refocusing an already
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
/// so the drain reports its proven native outcome back under the same id.
/// `None` (duplicate path) stores the claim unbound: the earlier dispatch
/// already retains the transfer.
fn store_select_claim(
    shared: &Arc<Mutex<Shared>>,
    claim: crate::runtime_types::ClaimedSelect,
    operation_id: Option<&str>,
) {
    let mut stored = claim;
    stored.operation_id = operation_id.map(str::to_owned);
    // One pending slot for both lanes (the send lane shares the select
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
    guard.select_pending = Some(stored);
    guard.diagnostics.input_select_enqueued += 1;
    drop(guard);
    emit(shared, 0, 0, 0, "input", token, 0);
}

/// Pure engine admission verdict for one armed select claim (pure
/// unit-testable core of the product handoff): unarmed actions refuse
/// (trailing stay-0, unsupported sends,
/// outputs), then a missing native context, an in-progress drain, or an
/// already-pending claim refuses narrowly — concurrent presses
/// serialize/refuse instead of overlapping, and a sent press is never
/// replayed. Returns the claim to store on success. Numbered taps bind at
/// dispatch (admission-time SID plus live revision); indicator clicks ride
/// their immutable render-time binding through to the drain; relative and
/// previous taps ride binding-free.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn admit_select_claim(
    native_present: bool,
    busy: bool,
    has_pending: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    admit_select_claim_with(
        native_present,
        busy,
        has_pending,
        claim,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit admission verdict: the production route above with
/// `trailing_enabled` threaded from the single capability gate (tests pass
/// an explicit local). Trailing `index 0` admits only when enabled.
pub(crate) fn admit_select_claim_with(
    native_present: bool,
    busy: bool,
    has_pending: bool,
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
    if busy || has_pending {
        return Err("refused-select-busy");
    }
    Ok(claim)
}

/// Pure engine admission verdict for one send-shaped claim (pure
/// unit-testable core of the product send handoff). Unsupported send
/// shapes (relative sends, trailing stay-0) refuse; trailing send-0
/// follow admits through the capability gate; armed
/// send-follow 0..9 plus stay 1..9 refuses without native context, while busy, or while
/// a claim pends — concurrent presses serialize/refuse instead of
/// overlapping, and a sent press is never replayed. Returns the claim to
/// bind (target SID, revision, subject) on success. The shape check is
/// [`is_armed_send`] (single source with the catalogue `implemented`
/// flags). Test-only shim over [`admit_send_claim_with`]: capability-explicit
/// tests call the `_with` variant directly with a local `trailing_enabled`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn admit_send_claim(
    native_present: bool,
    busy: bool,
    has_pending: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    admit_send_claim_with(
        native_present,
        busy,
        has_pending,
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
    busy: bool,
    has_pending: bool,
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
    if busy || has_pending {
        return Err("refused-send-busy");
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
    lock_shared(shared).diagnostics.input_forwarded += 1;
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
/// staging, no reconciler writes — a pure read over retained state plus
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
                    // from the dead lifetime drop with it.
                    retained.apps.insert(
                        pid,
                        RetainedApp {
                            generation,
                            seq_last: 0,
                            cmd_counter: 0,
                            windows: Vec::new(),
                            focused_wid: None,
                        },
                    );
                    forget_world_members(session, retained, pid);
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
            let closed: Vec<WindowId> = removed
                .map(|app| {
                    app.windows
                        .iter()
                        .map(|w| WindowId(window_id(pid, w.wid)))
                        .collect()
                })
                .unwrap_or_default();
            // Partition member closes through live homing, plus minimized
            // closes through their entry homing (minimized windows ride no
            // membership, so without this their slots would leak core-side
            // — never silent absence, never an inferred close elsewhere).
            // Both partitions happen BEFORE forgetting either.
            let closed_map = crate::world_runtime::terminate_close_map(retained, pid, &closed);
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
                closed_map.values().map(Vec::len).sum::<usize>() as u64,
            ); // Reconcile the survivors so unaffected apps stay operational.
            // Native mode reports every current domain with the closed ids
            // homed per domain (unknown ids omit there); synthetic mode
            // keeps the skeleton report.
            let dispatch = dispatch_context(shared, session);
            dispatch_report(
                session,
                retained,
                shared,
                inbox,
                focus_queue,
                focus_capacity,
                dispatch,
                closed_map,
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
                BTreeMap::new(),
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
            // drain. Numbered taps bind at dispatch (see
            // [`handle_select_action`]).
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
            lock_shared(shared).diagnostics.input_forwarded += 1;
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
                BTreeMap::new(),
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
        } => {
            // Raw drain evidence staged for the next report (never a
            // verdict: the core derives confirmation, refusal, ambiguity,
            // and supersession). Unknown operations drop without evidence.
            // The ring line makes staging observable end to end.
            session.stage_view_attempt(&operation_id, setter, pre_sid, post_sid);
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
        }
        EngineJob::MemberAttempt {
            operation_id,
            setter,
            post_sid,
        } => {
            session.stage_member_attempt(&operation_id, setter, post_sid);
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
        }
        EngineJob::DragGesture {
            ingress,
            press_revision,
            reply,
        } => {
            // One gesture phase through the world gesture lifecycle
            // (Started/Updated/Released/Cancelled). Every refusal maps to
            // a pure reply — never a dropped reply — so the main thread
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
            // incorporated). Ordinary writes stage nothing — no mark moves.
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
            // homed under. Stages nothing, plans nothing — the main thread
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
/// world command, and — when the reply applies, or when a stale-observation
/// refusal proves the command already executed core-side — one pending
/// claim bound under the stable operation identity for the main-thread
/// drain. Other core refusals drop the claim (never actuated, never
/// replayed). Numbered selects resolve their native target SID exactly once
/// here under the admission topology and dispatch it as an explicit
/// `SelectWorkspace` (never re-resolved core-side through the scoped ring);
/// the bound claim pins the SID plus the live revision so the drain
/// revalidates instead of interpreting the ordinal against a different
/// generation. Relative/previous selects keep their shared core semantics.
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
/// `SelectTrailing` only when enabled; the applied reply's unbound token
/// rides the stored claim so the drain pairs the real token with its
/// exact native append proof.
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
        admit_select_claim_with(
            guard.native_context.is_some(),
            guard.select_busy,
            guard.select_pending.is_some(),
            claim,
            trailing_enabled,
        )
    };
    let mut claim = match verdict {
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
    let Some(output) = select_output(retained, &ctx) else {
        refuse_select_claim(shared, "refused-select-no-view");
        return;
    };
    // Bind the admission-time target SID plus the live revision for numbered
    // selects: the ordinal resolves exactly once here against the native row
    // — never re-resolved core-side through the scoped ring, whose
    // stable-logical order need not match native Desktop order — and the
    // dispatched command carries the pinned native workspace directly.
    // Indicator claims already ride their immutable render-time SID; tap
    // claims bind here. Relative/previous selects keep their shared core
    // semantics and ride binding-free. The drain revalidates the pinned
    // target instead of interpreting the ordinal against a different
    // generation (a moved ordinal refuses instead of selecting the new
    // occupant). The dispatched command is positional (`SelectNumbered`
    // over the adopted ring, whose order tracks native Desktop order):
    // Desktop-N resolves core-side in the same report the adoption
    // converges, so no adapter-side logical mapping is pinned.
    // Trailing `index 0` rides binding-free (no ordinal SID to pin: the
    // native drain ensures reuse-or-append first); the dispatched
    // `SelectTrailing` command resolves core-side. Admission already gated
    // it on the same explicit capability.
    let command = match &claim.action {
        crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 } => {
            match crate::world_command_map::select_command_with_capability(
                &claim.action,
                output.clone(),
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
            if claim.expected_sid.is_none() {
                let Some(sid) = crate::workspace_send::context_target_sid(&ctx, &output, *index)
                else {
                    refuse_select_claim(shared, "refused-select-no-target");
                    return;
                };
                claim.expected_sid = Some(sid);
                claim.topology_revision = Some(ctx.revision);
            }
            match select_command(&claim.action, output.clone()) {
                Some(command) => command,
                None => {
                    refuse_select_claim(shared, "refused-select-unarmed");
                    return;
                }
            }
        }
        _ => match select_command(&claim.action, output.clone()) {
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
        BTreeMap::new(),
        Some(command),
        None,
        true,
        OP_SELECT,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied { select_operation } => {
            // The applied reply's operation identity rides the stored
            // claim so the drain reports its native outcome under that
            // identity (raw attempt evidence, never a verdict).
            store_select_claim(shared, claim, select_operation.as_deref());
        }
        DispatchVerdict::Duplicate => {
            // Identical latest resend (unreachable for fresh command
            // sequences): keep the transfer the earlier dispatch retained
            // and let the drain actuate once.
            store_select_claim(shared, claim, None);
        }
        DispatchVerdict::Refused {
            token,
            select_operation,
        } => {
            // A stale-observation refusal still carries the operation
            // identity: the command already executed core-side
            // (at-most-once), so the claim binds for drain feedback and
            // the racing successor refuses busy instead of replaying.
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
/// counter — one pending slot for both lanes — never dispatched, never
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
/// — when the reply applies, or when a stale-observation refusal proves the
/// command already executed core-side — one bound pending claim for the
/// main-thread send drain. Other core refusals drop the claim (never actuated, never
/// replayed). The subject is observed truth (never planned focus): the
/// focused window must be homed to a currently-actuated domain and must
/// not read native fullscreen (game/fullscreen foregrounds never drag).
/// Numbered target ordinals bind to their admission-time SID plus the current
/// topology revision so the drain revalidates instead of interpreting the
/// ordinal against a different generation; the dispatched command resolves
/// the ordinal core-side against the adopted ring for the explicit source.
/// The trailing `index 0` follow binds NO admission SID (the Space may not
/// exist yet): the drain ensures it natively (reuse-or-append) and binds
/// the exact ensured SID with the post-preparation revision through
/// [`crate::workspace_send::bind_trailing_send_target`]. The subject lifetime
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
    let InputSemanticAction::WorkspaceSend { index, .. } = claim.action.clone() else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    if !is_armed_send_with(&claim.action, trailing_enabled) {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    }
    let verdict = {
        let guard = lock_shared(shared);
        admit_send_claim_with(
            guard.native_context.is_some(),
            guard.select_busy,
            guard.select_pending.is_some(),
            claim,
            trailing_enabled,
        )
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
    // the same publication — never cached topology, static scope, or
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
    // Fullscreen refusal: a fullscreen subject owns its Space — dragging
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
    let Some(output) = select_output(retained, &ctx) else {
        refuse_send_claim(shared, "refused-send-no-view");
        return;
    };
    // Bind the admission-time target SID plus the live revision: the drain
    // refuses a moved ordinal instead of sending to the new occupant, and
    // the dispatched command carries the pinned native workspace directly
    // (the ordinal resolves exactly once here against the native row —
    // never re-resolved core-side through the scoped ring, whose
    // stable-logical order need not match native Desktop order).
    // Admission pins the subject lifetime and source domain from retained
    // engine truth (never re-sampled at dispatch: a PID reuse after
    // relaunch must refuse instead of grabbing the new lifetime's
    // window, and a moved subject must refuse instead of dragging from
    // the wrong domain). Stay execution selects back to the pinned source
    // SID below (exact native SID of the source domain under this
    // admission topology - never a drain-time guess).
    let Some(source_sid) = ctx
        .domains
        .iter()
        .find(|d| d.output == key.output && d.workspace == key.workspace)
        .map(|d| d.space)
    else {
        refuse_send_claim(shared, "refused-send-no-source");
        return;
    };
    // Bind the admission-time target: numbered ordinals resolve exactly once
    // here against the native row (the drain revalidates the pinned SID
    // plus the topology revision instead of interpreting the ordinal
    // against a different generation). The dispatched command is positional
    // (`SendNumbered` over the scoped send ring for the explicit source,
    // whose order tracks native Desktop order) — the pinned native SID
    // stays drain-side for revalidation only, never a logical guess. The
    // trailing `index 0` follow binds NO admission SID (the Space may not
    // exist yet): the drain ensures it natively (reuse-or-append) and binds
    // the exact ensured SID with the post-preparation revision through
    // [`crate::workspace_send::bind_trailing_send_target`].
    let trailing = index == 0;
    if trailing {
        let Some(command) = crate::world_command_map::send_command_with_capability(
            &claim.action,
            &window.0,
            output.clone(),
            trailing_enabled,
        ) else {
            refuse_send_claim(shared, "refused-send-unarmed");
            return;
        };
        claim.expected_sid = None;
        claim.topology_revision = Some(ctx.revision);
        claim.send_subject = Some(window.0.clone());
        claim.send_generation = Some(facts.generation);
        claim.send_source = Some(key.clone());
        claim.send_source_sid = Some(source_sid);
        // Trailing follows store directly (no stay focus pin: stay-0 never
        // admits). A stale-observation refusal still carries the operation
        // identity like the numbered lane; every other refusal binds
        // nothing (the core never ran).
        match dispatch_report(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            focus_capacity,
            DispatchContext::Native(Arc::clone(&ctx)),
            BTreeMap::new(),
            Some(command),
            None,
            true,
            OP_SEND,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { select_operation } => {
                store_select_claim(shared, claim, select_operation.as_deref());
            }
            DispatchVerdict::Duplicate => {
                store_select_claim(shared, claim, None);
            }
            DispatchVerdict::Refused {
                token,
                select_operation,
            } => {
                if let Some(operation) = select_operation {
                    store_select_claim(shared, claim, Some(&operation));
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
        return;
    }
    let Some(target_sid) = crate::workspace_send::context_target_sid(&ctx, &output, index) else {
        refuse_send_claim(shared, "refused-send-no-target");
        return;
    };
    let Some(command) = send_command(&claim.action, &window.0, output.clone()) else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    claim.expected_sid = Some(target_sid);
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
        BTreeMap::new(),
        Some(command),
        None,
        true,
        OP_SEND,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied { select_operation } => {
            // The applied reply's operation identity rides the stored
            // claim so the drain reports its native outcome under that
            // identity (raw attempt evidence, never a verdict).
            if !stay {
                store_select_claim(shared, claim, select_operation.as_deref());
            } else if let Some(op) = select_operation.as_deref() {
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
    fn select_admission_gates_unarmed_topology_and_busy() {
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
            let err = admit_select_claim(true, false, false, claim).expect_err("unarmed refuses");
            assert_eq!(err, "refused-select-unarmed");
        }
        assert_eq!(
            admit_select_claim(
                true,
                false,
                false,
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
                false,
                false,
                ClaimedSelect::tap(InputSemanticAction::WorkspacePrevious),
            )
            .map(|c| c.action),
            Ok(InputSemanticAction::WorkspacePrevious),
        );
        // No published native context refuses before any claim is stored.
        let err =
            admit_select_claim(false, false, false, tap_claim()).expect_err("no-topology refuses");
        assert_eq!(err, "refused-select-no-topology");
        // A drain in progress or an already-pending claim refuses
        // (concurrent presses serialize/refuse instead of overlapping).
        for (busy, pending) in [(true, false), (false, true), (true, true)] {
            let err =
                admit_select_claim(true, busy, pending, tap_claim()).expect_err("busy refuses");
            assert_eq!(err, "refused-select-busy");
            let err = admit_select_claim(true, busy, pending, bound_claim())
                .expect_err("bound busy refuses");
            assert_eq!(err, "refused-select-busy");
        }
        // Open lane admits both lanes verbatim — the indicator binding
        // rides through untouched for the drain to fence.
        assert_eq!(
            admit_select_claim(true, false, false, tap_claim()),
            Ok(tap_claim())
        );
        assert_eq!(
            admit_select_claim(true, false, false, bound_claim()),
            Ok(bound_claim())
        );
    }

    #[test]
    fn trailing_select_admission_armed_in_production() {
        use crate::runtime_types::InputSemanticAction;
        let trailing = ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 });
        // Production (gate `true`): trailing admits with topology and lane
        // open.
        assert_eq!(
            admit_select_claim(true, false, false, trailing.clone()),
            Ok(trailing.clone())
        );
        // Capability-disabled pin through the same production function
        // refuses; busy/topology still fence when armed.
        assert_eq!(
            admit_select_claim_with(true, false, false, trailing.clone(), false),
            Err("refused-select-unarmed")
        );
        assert_eq!(
            admit_select_claim_with(true, false, false, trailing.clone(), true),
            Ok(trailing.clone())
        );
        assert_eq!(
            admit_select_claim_with(false, false, false, trailing.clone(), true),
            Err("refused-select-no-topology")
        );
        assert_eq!(
            admit_select_claim_with(true, true, false, trailing.clone(), true),
            Err("refused-select-busy")
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
            topology_revision: ctx.revision,
            native_context: Some(Arc::clone(&ctx)),
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: None,
            select_busy: false,
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
        }))
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
        let claim = guard.select_pending.clone().expect("claim stored");
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
            guard.select_pending.is_some(),
            "armed production stores the trailing claim"
        );
    }

    #[test]
    fn raw_attempt_jobs_stage_only_against_retained_operations() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Unknown operations (forged, retired, or never issued) drop
        // without evidence — never fabricated rows, never guessed targets.
        for job in [
            EngineJob::ViewAttempt {
                operation_id: "op-404".to_owned(),
                setter: FrameSetter::Accepted,
                pre_sid: Some(1),
                post_sid: Some(277),
            },
            EngineJob::MemberAttempt {
                operation_id: "op-404".to_owned(),
                setter: FrameSetter::Accepted,
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
            BTreeMap::new(),
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
        session.auto_asserted.insert("4242_7".to_owned());
        // Fixed-hint window homed to the live domain: the next real build
        // asserts automatic origin first-sight and records it back.
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
            BTreeMap::new(),
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
        assert!(
            session.auto_asserted.contains("4242_7"),
            "assertion retained (prune runs only on real builds)"
        );
        // Next real dispatch carries every staged row; the fixed-hint
        // window asserts automatic origin and records it back.
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
            BTreeMap::new(),
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
        assert!(
            session.auto_asserted.contains("4242_7"),
            "first-sight fixed assert records back after the gap"
        );
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
    /// in the same pass) — reported core gap, not an adapter guess.
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
            BTreeMap::new(),
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
        let claim = guard.select_pending.clone().expect("send claim stored");
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
            lock_shared(&shared).select_pending.is_some(),
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

        fn empty(&mut self, sid: u64) -> Result<bool, String> {
            Ok(!self.occupied.contains(&sid))
        }

        fn create(&mut self, _before: &SelectionSnapshot) -> Result<(), String> {
            let sid = self.next_sid;
            self.next_sid += 1;
            self.spaces.push(sid);
            self.revision += 1;
            Ok(())
        }

        fn select(&mut self, _before: &SelectionSnapshot, sid: u64) -> Result<(), String> {
            if !self.spaces.contains(&sid) {
                return Err("scripted select of absent Space".to_owned());
            }
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
    /// The select lane settles on the combined report: the native echo of
    /// the appended current supersedes the retained view target, so the
    /// view follows the new native with no retained transfer and the next
    /// observation-only report stays stable with no replay. The send lane
    /// holds safely: the appended native has no core mapping yet (the
    /// empty-creation correlation is parked), so the member attempt stages
    /// an unknown post and the retained window transfer holds without
    /// re-actuation until newer input supersedes it. Neither lane invents
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
                .clone()
                .expect("trailing select claim stored");
            let op = claim.operation_id.clone().expect("operation rides");

            // Real native leg through the injected backend only.
            let mut backend = FakeBackend::new(&[1, 277], 1, &[277]);
            let mut select_state = crate::workspace_select::SelectionState::default();
            let effect = select_state
                .execute(
                    &mut backend,
                    InputSemanticAction::WorkspaceSelect { index: 0 },
                )
                .expect("native append+select proves");
            assert_eq!((effect.created, effect.target), (Some(278), 278));

            // Actual same-tick handoff into the session: the drain-verified
            // before/after native reads ride the raw attempt row.
            process_job(
                EngineJob::ViewAttempt {
                    operation_id: op.clone(),
                    setter: FrameSetter::Accepted,
                    pre_sid: Some(effect.from),
                    post_sid: Some(effect.target),
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
            // native reads translated through the last applied reply: the
            // known source maps to its logical id, the not-yet-adopted
            // append stays unknown (never guessed).
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
                3,
                3,
                None,
                None,
                None,
                "req-3",
            )
            .expect("nonempty context reports");
            assert_eq!(report.view_attempts.len(), 1, "report carries the attempt");
            assert_eq!(report.view_attempts[0].operation_id, op);
            assert_eq!(
                report.view_attempts[0].pre,
                Some("ws-1".to_owned()),
                "known source Space translates through the live reply"
            );
            assert_eq!(
                report.view_attempts[0].post, None,
                "unadopted append stays unknown, never guessed"
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
                Some("ws-4".to_owned()),
                "view follows the appended native current"
            );

            // Next observation: stable, nothing pending, nothing replayed.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
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
                Some("ws-4".to_owned()),
                "adopted view persists without re-binding"
            );
        }

        // -- Send lane: member attempt with an untranslatable post holds. --
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
                .clone()
                .expect("trailing send claim stored");
            let op = claim.operation_id.clone().expect("operation rides");

            // Real native preparation through the injected backend only.
            let mut backend = FakeBackend::new(&[1, 277], 1, &[277]);
            let prep = crate::workspace_send::prepare_trailing_send_target(&mut backend, true)
                .expect("native append proves");
            assert_eq!((prep.target_sid, prep.created), (278, Some(278)));
            let after = backend.spaces.clone();
            let _ = after;
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

            // One dispatched observation carries the staged row. The
            // appended Space has no core mapping yet, so the verified
            // post reads unknown — the row stages honestly without it.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
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
                "unadopted arrival stays unknown, never guessed"
            );
            let reply = match session.engine_mut().handle_report(&report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            // The window transfer holds (never a false confirm, never a
            // blind repeat); the follow view settles to the native echo.
            // The empty-creation correlation to the retained target is the
            // parked neutrality concern — no adapter policy invents it.
            let held: Vec<_> = session
                .engine_mut()
                .retained_transfers()
                .into_iter()
                .filter(|target| target.operation_id == op)
                .collect();
            assert_eq!(held.len(), 1, "window transfer retained");
            assert!(held[0].held, "held without re-actuation");
            assert!(
                reply
                    .transfers
                    .iter()
                    .all(|target| target.operation_id != format!("{op}-follow")),
                "follow view leaves no separate flight"
            );

            // Next observation: stable hold, nothing replayed, no arrival
            // fabricated.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
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
                1,
                "hold persists without re-actuation"
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
            topology_revision: ctx.revision,
            native_context: Some(Arc::clone(&ctx)),
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: None,
            select_busy: false,
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
}
