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
    // One pending slot for both lanes (offline send seam shares the
    // select slot): send-shaped claims would enqueue under their own token
    // so the ring distinguishes the lanes — unreachable in product, where
    // the unsupported guard refuses every send first.
    let token = if crate::workspace_input::is_armed_send(&stored.action) {
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

/// Pure engine admission verdict for one armed select claim (offline
/// unit-testable core of the product handoff): unarmed actions refuse
/// (recognized-but-undispatched trailing/select-0, sends, outputs), then a
/// missing native context, an in-progress drain, or an already-pending
/// claim refuses narrowly — concurrent presses serialize/refuse instead of
/// overlapping, and a sent press is never replayed. Returns the claim to
/// store on success. Tap chords ride binding-free; indicator clicks ride
/// their immutable render-time binding through to the drain.
pub(crate) fn admit_select_claim(
    native_present: bool,
    busy: bool,
    has_pending: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    use crate::workspace_input::{is_armed_select, workspace_unsupported_outcome};
    if workspace_unsupported_outcome(&claim.action).is_some() {
        return Err("refused-select-unarmed");
    }
    if !is_armed_select(&claim.action) {
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

/// Pure engine admission verdict for one send-shaped claim (offline
/// unit-testable seam for the future product send handoff, NOT product
/// behavior: the product engine refuses every send at the
/// [`workspace_unsupported_outcome`] guard in [`process_job`] before any
/// lane runs, so this is exercised only by the offline tests and the
/// retained-but-unreachable send lane below). Send-shaped actions refuse
/// without native context, while busy, or while a claim pends — concurrent
/// presses serialize/refuse instead of overlapping, and a sent press is
/// never replayed. Returns the claim to bind (target SID, revision,
/// subject) on success. The shape check is [`is_armed_send`] (never the
/// catalogue `implemented` flags: the catalogue marks every send
/// unimplemented this milestone).
pub(crate) fn admit_send_claim(
    native_present: bool,
    busy: bool,
    has_pending: bool,
    claim: crate::runtime_types::ClaimedSelect,
) -> Result<crate::runtime_types::ClaimedSelect, &'static str> {
    use crate::workspace_input::is_armed_send;
    if !is_armed_send(&claim.action) {
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
            // Unarmed workspace actions (trailing select-0, every send/stay,
            // every output) are recognized but undispatched: no trailing
            // create, no transfer, no output-send route runs from the
            // product tile (see `workspace_unsupported_reason` for the exact
            // seam). Counted and logged with a static token, never faked.
            if let Some(outcome) = workspace_unsupported_outcome(&action) {
                lock_shared(shared).diagnostics.input_refused_unsupported += 1;
                emit(shared, 0, 0, 0, "input", outcome, 0);
                return;
            }
            // Armed select-only actions dispatch with no focused-window
            // subject (empty Spaces navigate): one planner-owned world
            // command plus one pending claim for the main-thread tile
            // drain. Tap chords ride binding-free (see
            // [`admit_select_claim`]).
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
            // Send-shaped actions never reach the lane below: the
            // unsupported guard above refuses every send (offline
            // preparation — no product send dispatch this milestone), so
            // this branch is unreachable product code, retained only as the
            // offline-tested seam for the live step (see
            // [`admit_send_claim`]).
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
/// replayed).
fn handle_select_action(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
) {
    let verdict = {
        let guard = lock_shared(shared);
        admit_select_claim(
            guard.native_context.is_some(),
            guard.select_busy,
            guard.select_pending.is_some(),
            claim,
        )
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
    let Some(output) = select_output(retained, &ctx) else {
        refuse_select_claim(shared, "refused-select-no-view");
        return;
    };
    let Some(command) = select_command(&claim.action, output) else {
        refuse_select_claim(shared, "refused-select-unarmed");
        return;
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
        DispatchVerdict::Applied { select_operation } => {
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

/// Armed-send lane (offline seam, unreachable in product this milestone:
/// the unsupported guard refuses every send before this runs; exercised
/// only by the offline tests and retained for the live step): admission
/// planner-owned `SendNumbered` world command, and — when the reply
/// applies, or when a stale-observation refusal proves the command already
/// executed core-side — one bound pending claim for the main-thread send
/// drain. Other core refusals drop the claim (never actuated, never
/// replayed). The subject is observed truth (never planned focus): the
/// focused window must be homed to a currently-actuated domain and must
/// not read native fullscreen (game/fullscreen foregrounds never drag).
/// The target ordinal binds to its admission-time SID plus the current
/// topology revision so the drain revalidates instead of interpreting the
/// ordinal against a different generation. The subject lifetime
/// (`send_generation`) and source domain (`send_source`) pin here from
/// retained engine truth — never re-sampled at dispatch.
fn handle_send_action(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    claim: crate::runtime_types::ClaimedSelect,
) {
    use crate::runtime_types::InputSemanticAction;
    use crate::workspace_input::is_armed_send;
    let InputSemanticAction::WorkspaceSend { index, .. } = claim.action.clone() else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    if !is_armed_send(&claim.action) {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    }
    let verdict = {
        let guard = lock_shared(shared);
        admit_send_claim(
            guard.native_context.is_some(),
            guard.select_busy,
            guard.select_pending.is_some(),
            claim,
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
    // Managed-current homing: an unmapped subject refuses instead of
    // planning the skeleton, and a hidden-domain subject never dispatches
    // (no drag out of an off-current Space).
    let Some(key) = retained.window_domains.get(&window.0).cloned() else {
        refuse_send_claim(shared, "refused-send-no-domain");
        return;
    };
    if !ctx.is_current(&key) {
        refuse_send_claim(shared, "refused-send-hidden-domain");
        return;
    }
    // Fullscreen refusal: a fullscreen subject owns its Space — dragging
    // it would tear the Space. Read from retained observation truth (the
    // drain re-proves this fresh before any post).
    let fullscreen = retained
        .apps
        .iter()
        .flat_map(|(pid, app)| app.windows.iter().map(move |w| (*pid, w)))
        .any(|(pid, w)| crate::runtime::window_id(pid, w.wid) == window.0 && w.fullscreen);
    if fullscreen {
        refuse_send_claim(shared, "refused-send-fullscreen");
        return;
    }
    let Some(output) = select_output(retained, &ctx) else {
        refuse_send_claim(shared, "refused-send-no-view");
        return;
    };
    let Some(command) = send_command(&claim.action, &window.0, output.clone()) else {
        refuse_send_claim(shared, "refused-send-unarmed");
        return;
    };
    // Bind the admission-time target SID plus the live revision: the drain
    // refuses a moved ordinal instead of sending to the new occupant.
    // Admission pins the subject lifetime and source domain from retained
    // engine truth (never re-sampled at dispatch: a PID reuse after
    // relaunch must refuse instead of grabbing the new lifetime's
    // window, and a moved subject must refuse instead of dragging from
    // the wrong domain).
    let Some(target_sid) = crate::workspace_send::context_target_sid(&ctx, &output, index) else {
        refuse_send_claim(shared, "refused-send-no-target");
        return;
    };
    let Some((subject_pid, _)) = crate::runtime_types::parse_window_id(&window.0) else {
        refuse_send_claim(shared, "refused-send-bad-subject");
        return;
    };
    let Some(subject_generation) = retained.apps.get(&subject_pid).map(|app| app.generation) else {
        refuse_send_claim(shared, "refused-send-no-lifetime");
        return;
    };
    claim.expected_sid = Some(target_sid);
    claim.topology_revision = Some(ctx.revision);
    claim.send_subject = Some(window.0.clone());
    claim.send_generation = Some(subject_generation);
    claim.send_source = Some(key.clone());
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
        // Unarmed actions (trailing select-0, sends, outputs) refuse even
        // with a bound click payload — a forged binding never widens the
        // lane. The armed previous-toggle admits binding-free.
        for action in [
            InputSemanticAction::WorkspaceSelect { index: 0 },
            InputSemanticAction::WorkspaceSend {
                index: 1,
                follow: true,
            },
        ] {
            let claim = ClaimedSelect::indicator(action, 3, 7, 9);
            let err = admit_select_claim(true, false, false, claim).expect_err("unarmed refuses");
            assert_eq!(err, "refused-select-unarmed");
        }
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
}
