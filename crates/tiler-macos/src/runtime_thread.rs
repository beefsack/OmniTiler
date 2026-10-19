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
    OP_RECONCILE, OP_SELECT, OP_SEND, OP_TERMINATE, operation_command, select_command,
    select_output, send_command,
};
use crate::world_dispatch::{DispatchVerdict, dispatch_report};
use crate::world_runtime::{
    DispatchContext, WorldSession, forget_world_members, preview_world, stage_write_feedback,
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
    /// Read-only drag preview query for the tile Meta+left route: resolve
    /// one [`tiler_core::boundary::CoreCommand::DragPreview`] against the
    /// current Engine and reply with the pure outcome. Stages nothing and
    /// touches no reconciler state (the Engine resolves on a working
    /// clone); the main thread feeds the outcome to its session and maps
    /// it to one carrier intent. Control lane (never silently dropped —
    /// the main thread always gets its reply or an engine-dead refusal),
    /// strict FIFO with observations. A dropped/closed inbox or a dead
    /// engine surfaces as `None` main-side, which cancels the gesture
    /// stale. The reply carries no new revision: the main thread fences
    /// revision before querying, and the sticky prior revalidates
    /// capture-side inside the Engine.
    DragPreviewQuery {
        window: String,
        x: i32,
        y: i32,
        hover_prior: Option<tiler_core::session::DragHoverPrior>,
        reply: mpsc::Sender<crate::drag_drop::PreviewOutcome>,
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
    /// Per-operation actuation feedback from proven setter/drain
    /// outcomes (never false success): staged into the next world report
    /// so the core settles retained transfers, views, and placements.
    /// Data lane (drops counted when full, never blocks); a dropped row
    /// merely delays settlement — retained core targets never re-actuate
    /// without fresh proof.
    AttemptFeedback {
        operation_id: String,
        kind: tiler_core::world_report::AttemptKind,
        outcome: tiler_core::world_report::AttemptOutcome,
        hold: Option<tiler_core::world_report::HoldReason>,
    },
    /// Proven trailing append proof from a main-thread (select) or
    /// effect-owner (send) drain: the engine-minted pending token from the
    /// drained claim paired with the exact native append rows. The engine
    /// records it through
    /// [`WorldSession::adopt_trailing_binding`](crate::world_runtime::WorldSession::adopt_trailing_binding)
    /// (exact same-output append only - drift, guesses, and duplicates
    /// refuse without mutation) and carries the binding on every full
    /// report until the core acknowledges it. Data lane (drops counted
    /// when full, never blocks); a dropped proof merely leaves the core
    /// pending outstanding - never a false resolution, never a replay.
    TrailingProof {
        logical_id: String,
        /// SLS in-row display identifier from the proven snapshot (the
        /// engine resolves the output against its native context - never
        /// a positional guess).
        display_id: String,
        created_sid: u64,
        before: Vec<u64>,
        after: Vec<u64>,
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
                | Self::AttemptFeedback { .. }
                | Self::TrailingProof { .. }
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

/// Read-only drag preview against the current world session (engine thread
/// only): home the press-proven subject to its current domain and resolve
/// one `Operation(DragPreview)` through the world route. Hidden-domain
/// subjects (off-current Space) and unmapped subjects cancel dead — the
/// main thread already fenced focus/scope/revision before querying, so
/// these legs are backstops, never the primary gate. Synthetic mode
/// (no native context) resolves over the skeleton report. Read-only:
/// never commits, never advances the revision.
fn resolve_drag_preview(
    session: &mut WorldSession,
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    window: &str,
    x: i32,
    y: i32,
    hover_prior: Option<tiler_core::session::DragHoverPrior>,
) -> crate::drag_drop::PreviewOutcome {
    let outcome = preview_world(session, retained, shared, window, x, y, hover_prior);
    lock_shared(shared).diagnostics.input_forwarded += 1;
    emit(shared, 0, 0, 0, "input", "drag-preview", 0);
    outcome
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
            // The subject is observed truth, never planned focus — except
            // the drag drop, whose subject is the press-proven window from
            // the tile drag session (focus-at-press was fenced main-side
            // with selected-oracle agreement; the drop never inherits a
            // drifted focus).
            let drag_subject: Option<WindowId> = match &action {
                InputSemanticAction::DragDrop { window, .. } => Some(WindowId(window.clone())),
                _ => None,
            };
            // Press topology revision for drag drops (stale pid_wid reuse
            // never mutates; validated against retained revision plus live
            // membership below).
            let drag_press_revision: Option<u64> = match &action {
                InputSemanticAction::DragDrop { press_revision, .. } => Some(*press_revision),
                _ => None,
            };
            let focused = match drag_subject {
                Some(subject) => Some(subject),
                None => retained.observed_focus.clone(),
            };
            let Some(window) = focused else {
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
                // Drag-drop staleness: the press revision must still equal
                // the retained topology revision (a mid-gesture resnapshot
                // drifted the pid_wid), and the subject must still ride
                // live membership (a missing/closed target refuses instead
                // of mutating).
                if let Some(press_revision) = drag_press_revision {
                    if press_revision != retained.native_revision {
                        lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                        emit(shared, 0, 0, 0, "input", "refused-stale-press", 0);
                        return;
                    }
                    let live = retained
                        .window_domains
                        .get(&window.0)
                        .is_some_and(|k| k == &key);
                    if !live {
                        lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                        emit(shared, 0, 0, 0, "input", "refused-closed-target", 0);
                        return;
                    }
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
        EngineJob::AttemptFeedback {
            operation_id,
            kind,
            outcome,
            hold,
        } => {
            // Proven setter/drain truth staged for the next report (never
            // false success: only recorded outcomes stage). The ring line
            // makes staging observable end to end (drain, discard, and
            // placement completions all land here).
            session.stage_feedback(tiler_core::world_report::AttemptFeedback {
                operation_id,
                kind,
                outcome,
                hold,
            });
            emit(shared, 0, 0, 0, "feedback", "staged", 1);
        }
        EngineJob::TrailingProof {
            logical_id,
            display_id,
            created_sid,
            before,
            after,
        } => {
            // Resolve the proven SLS display identifier to exactly one
            // output under the published native context (never a
            // positional guess), then record the exact append: drift,
            // guesses, and duplicates refuse inside without mutation, and
            // a dropped proof merely leaves the core pending outstanding.
            let output = {
                let guard = lock_shared(shared);
                crate::trailing_binding::resolve_trailing_output(
                    guard.native_context.as_deref(),
                    &display_id,
                )
            };
            let Some(output) = output else {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(
                    shared,
                    0,
                    0,
                    0,
                    "trailing-proof",
                    "refused-unknown-display",
                    0,
                );
                return;
            };
            match session.adopt_trailing_binding(&logical_id, &output, created_sid, &before, &after)
            {
                Ok(()) => {
                    emit(shared, 0, 0, 0, "trailing-proof", "adopted", 0);
                }
                Err(_) => {
                    lock_shared(shared).diagnostics.stale_dropped += 1;
                    emit(shared, 0, 0, 0, "trailing-proof", "refused-drift", 0);
                }
            }
        }
        EngineJob::DragPreviewQuery {
            window,
            x,
            y,
            hover_prior,
            reply,
        } => {
            // Read-only preview through the world route (never commits,
            // never advances the revision). Every refusal maps to a pure
            // outcome — never a dropped reply — so the main thread always
            // unblocks by its timeout.
            let outcome =
                resolve_drag_preview(session, retained, shared, &window, x, y, hover_prior);
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
            if stage_write_feedback(session, pid, cmd_id, &completion) {
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
    // occupant).
    // Trailing `index 0` rides binding-free (no ordinal SID to pin: the
    // native drain ensures reuse-or-append first); the dispatched
    // `SelectTrailing` command resolves core-side. Admission already gated
    // it on the same explicit capability.
    let command = match &claim.action {
        crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 } => {
            match crate::world_command_map::select_command_with_capability(
                &claim.action,
                output.clone(),
                "",
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
            let workspace = if let Some(sid) = claim.expected_sid {
                crate::topology::workspace_key(sid)
            } else {
                let Some(sid) = crate::workspace_send::context_target_sid(&ctx, &output, *index)
                else {
                    refuse_select_claim(shared, "refused-select-no-target");
                    return;
                };
                claim.expected_sid = Some(sid);
                claim.topology_revision = Some(ctx.revision);
                crate::topology::workspace_key(sid)
            };
            match select_command(&claim.action, output.clone(), &workspace) {
                Some(command) => command,
                None => {
                    refuse_select_claim(shared, "refused-select-unarmed");
                    return;
                }
            }
        }
        _ => match select_command(&claim.action, output.clone(), "") {
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
        true,
        OP_SELECT,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied {
            select_operation,
            trailing_token,
        } => {
            // Trailing lanes pair the engine-minted token with the exact
            // native append proof at drain (never a guess); other lanes
            // never carry one (the dispatch helper yields `None` there).
            claim.trailing_token = trailing_token;
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
/// capability-gated trailing send-0 follow): admission, a planner-owned
/// explicit-`Send` world command carrying the admission-resolved native
/// workspace, and — when the reply
/// applies, or when a stale-observation refusal proves the command already
/// executed core-side — one bound pending claim for the main-thread send
/// drain. Other core refusals drop the claim (never actuated, never
/// replayed). The subject is observed truth (never planned focus): the
/// focused window must be homed to a currently-actuated domain and must
/// not read native fullscreen (game/fullscreen foregrounds never drag).
/// Numbered target ordinals bind to their admission-time SID plus the current
/// topology revision so the drain revalidates instead of interpreting the
/// ordinal against a different generation; the dispatched command names the
/// pinned workspace directly and never re-resolves the ordinal core-side.
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
    // here against the native row (never re-resolved core-side through the
    // scoped ring). The trailing `index 0` follow binds NO admission SID (the
    // Space may not exist yet — the drain ensures reuse-or-append natively
    // and binds the exact ensured SID with the post-preparation revision);
    // only the admission revision rides the claim so the drain can detect a
    // pre-preparation topology move.
    let trailing = index == 0;
    if trailing {
        let Some(command) = crate::world_command_map::send_command_with_capability(
            &claim.action,
            &window.0,
            output.clone(),
            "",
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
            true,
            OP_SEND,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied {
                select_operation,
                trailing_token,
            } => {
                // Trailing follow pairs the engine-minted token with the
                // owner-side exact append proof (never a guess).
                claim.trailing_token = trailing_token;
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
    let target_workspace = crate::topology::workspace_key(target_sid);
    let Some(command) = send_command(&claim.action, &window.0, output.clone(), &target_workspace)
    else {
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
        true,
        OP_SEND,
        0,
        0,
        0,
    ) {
        DispatchVerdict::Applied {
            select_operation,
            trailing_token: _,
        } => {
            // Numbered sends resolve to an explicit native at admission:
            // no pending token ever rides here (the dispatch helper
            // yields a token for trailing commands only).
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
    use tiler_core::world_report::{AttemptKind, AttemptOutcome};

    use crate::focus_route::FocusRoute;
    use crate::runtime::{Shared, lock_shared};
    use crate::runtime_core::{RetainedApp, RetainedCore};
    use crate::runtime_types::{
        ClaimedSelect, InputSemanticAction, NativeContext, NativeDomain, ObservedWindow,
    };
    use crate::trailing_binding::unbound_trailing_token;
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
    fn trailing_select_dispatch_binds_real_engine_token_to_claim() {
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
        let token = claim
            .trailing_token
            .clone()
            .expect("real engine token binds");
        assert!(
            token.starts_with("wl-"),
            "engine-minted token, never a guess"
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
    fn trailing_proof_job_records_exact_append_and_refuses_drift() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), RetainedCore::default());
        // Exact append records: the next report (under the post-create
        // topology that now shows the Space) carries the binding.
        process_job(
            EngineJob::TrailingProof {
                logical_id: "wl-1".to_owned(),
                display_id: UUID_1.to_owned(),
                created_sid: 278,
                before: vec![1, 277],
                after: vec![1, 277, 278],
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let created = Arc::new(NativeContext {
            revision: 10,
            domains: vec![domain(1, false), domain(277, false), domain(278, true)],
            skipped: Vec::new(),
        });
        let staged = session.trailing_report_bindings(&created);
        assert_eq!(staged.len(), 1, "exact proof records");
        assert_eq!(staged[0].logical_id, "wl-1");
        assert_eq!(staged[0].native_id, "space-278");
        // Drifted rows refuse without mutation (stale counted, never a
        // false binding).
        let stale_before = lock_shared(&shared).diagnostics.stale_dropped;
        process_job(
            EngineJob::TrailingProof {
                logical_id: "wl-2".to_owned(),
                display_id: UUID_1.to_owned(),
                created_sid: 279,
                before: vec![1, 277],
                after: vec![1, 279, 278],
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(session.trailing_report_bindings(&created).len(), 1);
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 1
        );
        // Unknown display identifier refuses the same way.
        process_job(
            EngineJob::TrailingProof {
                logical_id: "wl-2".to_owned(),
                display_id: "uuid-unknown".to_owned(),
                created_sid: 279,
                before: vec![1, 277],
                after: vec![1, 277, 279],
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert_eq!(session.trailing_report_bindings(&created).len(), 1);
        assert_eq!(
            lock_shared(&shared).diagnostics.stale_dropped,
            stale_before + 2
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

    #[test]
    fn trailing_send_dispatch_binds_subject_pins_and_real_token() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
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
        // moves. The engine-minted pending token rides alongside.
        assert_eq!(claim.expected_sid, None);
        assert_eq!(claim.topology_revision, Some(9));
        let token = claim.trailing_token.clone().expect("pending token binds");
        assert!(
            token.starts_with("wl-"),
            "engine-minted token, never a guess"
        );
        assert_eq!(guard.diagnostics.input_select_enqueued, 1);
    }

    #[test]
    fn trailing_send_production_wrapper_armed() {
        let ctx = ctx();
        let (mut session, mut retained, shared, inbox, focus_queue) =
            harness(Arc::clone(&ctx), retained_send_subject());
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
    /// same-tick jobs (`EngineJob::TrailingProof` plus the Applied
    /// `EngineJob::AttemptFeedback` the outcome reporters enqueue)
    /// processed through the real `process_job` arms into the same
    /// `WorldSession`, then one real `build_dispatch_report` observation
    /// carrying both staged rows into the real engine. Covers the select
    /// lane (View Applied) and the send lane (Transfer Applied) with
    /// fresh harnesses: each combined report settles its lane with no
    /// retained transfer, and the following observation-only report stays
    /// stable with no replay. The select lane leaves no unbound row (the
    /// new trailing stays reusable); the settled send occupies the new
    /// trailing, so the core mints a fresh onward token there (the
    /// settled pending resolved, never retained).
    ///
    /// The native proof rows come from the real select/send preparation
    /// over the scripted backend above (the established injection seam);
    /// job processing, display resolution, feedback staging, report
    /// building, and the core settle are the production functions, never
    /// substitutes. Capability stays `false` outside the explicit `true`
    /// locals: nothing arms.
    #[test]
    fn trailing_jobs_settle_combined_report_through_real_dispatch() {
        // -- Select lane: proof job plus View Applied in one report. --
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
            let token = claim.trailing_token.clone().expect("engine token rides");
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
            let after = backend.spaces.clone();

            // Actual same-tick handoff into the session.
            process_job(
                EngineJob::TrailingProof {
                    logical_id: token.clone(),
                    display_id: UUID_1.to_owned(),
                    created_sid: 278,
                    before: vec![1, 277],
                    after,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            process_job(
                EngineJob::AttemptFeedback {
                    operation_id: op.clone(),
                    kind: AttemptKind::View,
                    outcome: AttemptOutcome::Applied,
                    hold: None,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let created = settled_ctx();
            assert_eq!(
                session.trailing_report_bindings(&created).len(),
                1,
                "proof job recorded the exact append"
            );

            // One dispatched observation carries both staged rows.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
                3,
                3,
                None,
                None,
                "req-3",
            )
            .expect("nonempty context reports");
            assert_eq!(report.bindings.len(), 1, "report carries the binding");
            assert_eq!(
                report.bindings[0].logical_id, token,
                "binding names the engine token"
            );
            assert_eq!(report.feedback.len(), 1, "report carries the outcome");
            assert_eq!(report.feedback[0].operation_id, op);
            assert_eq!(report.feedback[0].kind, AttemptKind::View);
            let reply = match session.engine_mut().handle_report(&report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(
                session.engine_mut().retained_transfers().is_empty(),
                "logical View settles on the combined report"
            );
            assert!(
                unbound_trailing_token(&reply, &output()).is_none(),
                "pending token resolved with no onward desire"
            );
            assert!(
                reply.workspaces.iter().any(|row| row.id == "space-278"),
                "mapping adopts the new current"
            );
            assert_eq!(session.note_trailing_ack(&reply), vec![token]);
            assert!(session.trailing_report_bindings(&created).is_empty());

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
                "req-4",
            )
            .expect("nonempty context reports");
            assert!(report.bindings.is_empty() && report.feedback.is_empty());
            let reply = match session.engine_mut().handle_report(&report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(session.engine_mut().retained_transfers().is_empty());
            assert!(unbound_trailing_token(&reply, &output()).is_none());
            assert!(
                reply.workspaces.iter().any(|row| row.id == "space-278"),
                "adopted native persists without re-binding"
            );
        }

        // -- Send lane: proof job plus Transfer Applied in one report. --
        {
            let ctx = ctx();
            let (mut session, mut retained, shared, inbox, focus_queue) =
                harness(Arc::clone(&ctx), retained_send_subject());
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
            let token = claim.trailing_token.clone().expect("engine token rides");
            let op = claim.operation_id.clone().expect("operation rides");

            // Real native preparation through the injected backend only.
            let mut backend = FakeBackend::new(&[1, 277], 1, &[277]);
            let prep = crate::workspace_send::prepare_trailing_send_target(&mut backend, true)
                .expect("native append proves");
            assert_eq!((prep.target_sid, prep.created), (278, Some(278)));
            let after = backend.spaces.clone();
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
                EngineJob::TrailingProof {
                    logical_id: token.clone(),
                    display_id: UUID_1.to_owned(),
                    created_sid: prep.target_sid,
                    before: vec![1, 277],
                    after,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            process_job(
                EngineJob::AttemptFeedback {
                    operation_id: op.clone(),
                    kind: AttemptKind::Transfer,
                    outcome: AttemptOutcome::Applied,
                    hold: None,
                },
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                8,
            );
            let created = settled_ctx();
            assert_eq!(
                session.trailing_report_bindings(&created).len(),
                1,
                "proof job recorded the exact append"
            );

            // One dispatched observation carries both staged rows.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
                3,
                3,
                None,
                None,
                "req-3",
            )
            .expect("nonempty context reports");
            assert_eq!(report.bindings.len(), 1, "report carries the binding");
            assert_eq!(
                report.bindings[0].logical_id, token,
                "binding names the engine token"
            );
            assert_eq!(report.feedback.len(), 1, "report carries the outcome");
            assert_eq!(report.feedback[0].operation_id, op);
            assert_eq!(report.feedback[0].kind, AttemptKind::Transfer);
            let reply = match session.engine_mut().handle_report(&report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(
                session.engine_mut().retained_transfers().is_empty(),
                "arrival plus Applied settles on the combined report"
            );
            // The settled send occupies the new trailing, so the core
            // desires onward trailing under a FRESH token: the settled
            // pending resolved, never retained.
            let fresh =
                unbound_trailing_token(&reply, &output()).expect("occupied trailing mints onward");
            assert_ne!(
                fresh, token,
                "settled pending resolved; only the next desire rides unbound"
            );
            assert!(
                reply.workspaces.iter().any(|row| {
                    row.logical_id.as_deref() == Some(token.as_str())
                        && row.native_binding.as_deref() == Some("space-278")
                }),
                "core publishes the bound row"
            );
            assert_eq!(session.note_trailing_ack(&reply), vec![token]);
            assert!(session.trailing_report_bindings(&created).is_empty());

            // Next observation: stable onward desire, nothing replayed.
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &crate::world_runtime::DispatchContext::Native(Arc::clone(&created)),
                BTreeMap::new(),
                4,
                4,
                None,
                None,
                "req-4",
            )
            .expect("nonempty context reports");
            assert!(report.bindings.is_empty() && report.feedback.is_empty());
            let reply = match session.engine_mut().handle_report(&report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
            assert!(session.engine_mut().retained_transfers().is_empty());
            assert_eq!(
                unbound_trailing_token(&reply, &output()),
                Some(fresh),
                "onward desire stable without re-binding"
            );
            assert!(
                reply.workspaces.iter().any(|row| row.id == "space-278"),
                "settled native persists without re-binding"
            );
        }
    }
}
