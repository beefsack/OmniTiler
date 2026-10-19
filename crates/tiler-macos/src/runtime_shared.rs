#![forbid(unsafe_code)]

//! Shared runtime lifecycle state (ownership, watermark, logging).
//!
//! Split from [`crate::runtime`]: the single shared [`Shared`] cell plus
//! its lock helpers, arrival-clock advance, ring logging, and admission
//! liveness — the state every submitter leg, worker rebind path, and engine
//! fence reads. [`crate::runtime`] keeps the client façade
//! ([`crate::runtime::RuntimeHandle`]); per-app lifetime admission lives in
//! [`crate::runtime_apps`]. No threads are spawned here; no behavior changes
//! with the move (call sites keep working through `runtime` re-exports).

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use std::time::Instant;

use crate::runtime_lifecycle::{
    EngineExit, InputExit, LiveHandle, LoggerExit, ShutdownReport, WorkerExit,
};
use crate::runtime_log::LogEvent;
use crate::runtime_thread::{FocusSlot, FrameSlot};
use crate::runtime_types::{
    ClaimedSelect, DesiredFrame, FocusEvidence, FocusRequest, LifecycleLogEntry, NativeContext,
    RuntimeConfig, RuntimeDiagnostics,
};
use crate::topology::TopologySnapshot;
use crate::world_sequencer::ObservationClock;

/// Cap for the structured lifecycle log; oldest entries rotate out.
const LOG_CAP: usize = 512;

pub(crate) struct AppRecord {
    pub(crate) generation: u64,
    pub(crate) terminated: bool,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) frames: Arc<FrameSlot>,
    /// Latest-wins focus slot for the owning AX worker (the SAME owner
    /// that observed the window dispatches focus). Stopped alongside
    /// `frames` on terminate/shutdown/drop so queued focus never outlives
    /// its lifetime.
    pub(crate) focus: Arc<FocusSlot>,
    /// Backend readiness set by the worker inside its owning thread: `None`
    /// while the backend is still being created, `Ok(())` once it exists,
    /// `Err` when creation was refused. Polled by [`RuntimeHandle::launch_app`]
    /// (bounded) and [`RuntimeHandle::worker_ready`].
    pub(crate) ready: Arc<Mutex<Option<Result<(), String>>>>,
    pub(crate) worker: Option<std::thread::JoinHandle<WorkerExit>>,
    pub(crate) worker_name: String,
}

/// Latest-wins AppKit display legs for the send effect owner; published
/// at startup, on screen-parameter notification, and to recover a cleared
/// slot — never on a quiet populated poll. Cleared on invalidation.
#[derive(Debug, Clone, Default)]
pub struct PublishedDisplayInputs {
    pub displays: Vec<crate::topology::DisplayGeometry>,
    pub separate: Option<bool>,
}

/// Live native-context cell: the single shared source every setter leg,
// the worker rebind path, and the engine read on every fence. Replaced
// wholesale per snapshot, never mutated in place, so readers always see
// one consistent revision. Plain data only.
#[derive(Debug, Clone, Default)]
pub struct NativeContextCell(Arc<Mutex<Option<Arc<NativeContext>>>>);

impl NativeContextCell {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish a freshly derived context (or clear it with `None`).
    pub fn store(&self, ctx: Option<Arc<NativeContext>>) {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner()) = ctx;
    }

    /// Load the current context, if any.
    #[must_use]
    pub fn load(&self) -> Option<Arc<NativeContext>> {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

/// Deterministic-test rendezvous invoked by the engine thread once per
/// opened world flight (after the ticket stamps the current watermark,
/// before the report builds), with no locks held. A test uses it to
/// enqueue racing input mid-flight over channels — no sleeps, no live
/// desktop — proving the current-watermark fence refuses the stale reply
/// and the queued successor applies newest-only with no stale emission.
/// Production never sets this; unset with
/// [`RuntimeHandle::set_midflight_hook_for_test`]`(None)`.
pub type MidflightHook = Arc<dyn Fn() + Send + Sync>;

/// Deterministic-test rendezvous invoked by the engine thread once per
/// accepted world reply (after sequencer acceptance, before the
/// emission-coherence gate), with no locks held. A test uses it to advance
/// the watermark between acceptance and emission — no sleeps, no live
/// desktop — proving accepted-but-superseded replies never reach native
/// setters. Production never sets this; unset with
/// [`RuntimeHandle::set_post_accept_hook_for_test`]`(None)`.
pub type PostAcceptHook = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct Shared {
    pub(crate) config: RuntimeConfig,
    /// Live [`RuntimeHandle`] count, held solely by handles (clones bump,
    /// drops release; supervised threads never touch it). The final owning
    /// handle drop tears down; borrowed clone drops are no-ops — unlike a
    /// `shared` strong-count, which threads also hold.
    pub(crate) handles_live: usize,
    pub(crate) generation_counter: u64,
    pub(crate) log_counter: u64,
    pub(crate) start: Instant,
    pub(crate) apps: BTreeMap<u32, AppRecord>,
    /// Shared world input watermark: every invalidating input submitter
    /// (AX workers, input thread, main-thread controller) advances this
    /// clock when its input is actually enqueued — including while a world
    /// request is outstanding — so an in-flight reply whose stamped
    /// observation no longer matches refuses as stale instead of actuating
    /// over newer input. Never advanced per dispatched job (a dispatch with
    /// no new input must not move the mark); reports stamp only actually
    /// submitted input.
    pub(crate) clock: Arc<ObservationClock>,
    /// Deterministic-test rendezvous: when set, the engine thread invokes
    /// the hook once per opened flight (after the ticket stamps the current
    /// watermark, before the report builds) with no locks held, so a test
    /// can enqueue racing input mid-flight over channels. Production never
    /// sets this; see `set_midflight_hook_for_test`.
    pub(crate) midflight_hook: Mutex<Option<MidflightHook>>,
    /// Deterministic-test rendezvous: when set, the engine thread invokes
    /// the hook once per accepted world reply (after sequencer acceptance,
    /// before the emission-coherence gate spends or emits anything) with
    /// no locks held, so a test can advance the watermark between
    /// acceptance and emission over channels. Production never sets this;
    /// see `set_post_accept_hook_for_test`.
    pub(crate) post_accept_hook: Mutex<Option<PostAcceptHook>>,
    /// Current native topology snapshot (the main-thread controller owns
    /// the cadence). `None` until the first
    /// [`RuntimeHandle::update_topology`]; synthetic mode never sets it.
    pub(crate) topology: Option<TopologySnapshot>,
    /// Latest-wins AppKit display legs; see [`PublishedDisplayInputs`].
    pub(crate) display_inputs: Option<PublishedDisplayInputs>,
    /// Latest-wins active-Space wake shared with off-main effect owners
    /// (select settlement and the send-stay return wait on its generation).
    /// Published once after the main-thread display guard installs;
    /// `None` until then (both paths refuse honestly, never guessed).
    pub(crate) select_wake: Option<crate::ffi::SpaceSelectWake>,
    /// Current display/Space revision fence. Synthetic mode holds 0 so every
    /// legacy command (revision 0) passes; native mode tracks the latest
    /// snapshot fingerprint.
    pub(crate) topology_revision: u64,
    /// Derived native tile context (per-display current-user-Space domains
    /// in integer work areas), published by the main-thread supplier
    /// alongside the snapshot. Retired by [`NativeContextCell`] below;
    /// kept here so the engine can fence without an extra hop.
    /// `None` until the first successful derivation; scoped observations
    /// and effects fence while it is absent.
    pub(crate) native_context: Option<Arc<NativeContext>>,
    /// Live cell handle cloned into per-app backends at bind time. The
    /// single shared source for every setter-leg fence read; written
    /// synchronously by [`RuntimeHandle::update_topology`] (and cleared by
    /// [`RuntimeHandle::invalidate_topology`]), so a topology update
    /// mid-plan fences the very next leg — not just the next command.
    pub(crate) native_cell: NativeContextCell,
    /// One pending product select claim (armed select-only actions): set by
    /// the engine without blocking, claimed and drained by the main-thread
    /// tile controller. `select_busy` marks a drain in progress so
    /// concurrent presses serialize/refuse instead of overlapping; both are
    /// short critical sections only (never held across the blocking effect).
    /// Indicator clicks ride as full [`ClaimedSelect`] bindings (expected
    /// SID plus UI generation plus topology revision); tap chords ride
    /// binding-free. The drain refuses a bound claim whose ordinal no
    /// longer names its SID or whose revision drifted.
    pub(crate) select_pending: Option<ClaimedSelect>,
    pub(crate) select_busy: bool,
    /// Main-thread-supplied global focus evidence (workspace-frontmost PID
    /// plus bundle, plain IDs — never AX reads: the main thread performs no
    /// AX under any cadence). The AX focused leg (`ax_focus`) is NOT read on
    /// the main thread: each owning per-app AX worker reads the bounded
    /// systemwide leg on its own thread (`sys_focus`) and stamps it over
    /// `ax_focus` at send time, so a hung app can never stall the
    /// controller. The engine promotes an app-local focused window to
    /// observed truth only on full agreement; `None` (no evidence yet)
    /// withholds promotion, never guesses.
    pub(crate) focus_evidence: Option<FocusEvidence>,
    pub(crate) zombies: Vec<LiveHandle>,
    pub(crate) engine_handle: Option<std::thread::JoinHandle<EngineExit>>,
    pub(crate) input_handle: Option<std::thread::JoinHandle<InputExit>>,
    pub(crate) logger_handle: Option<std::thread::JoinHandle<LoggerExit>>,
    /// Parked input thread for prompt stop wake (`unpark` after `stop_input`
    /// is set; an `unpark` before `park` grants a permit, so no wake is
    /// lost).
    pub(crate) input_thread: Option<std::thread::Thread>,
    /// Cached terminal shutdown report: a repeated stop replays the exact
    /// terminal report instead of hiding a panic/timeout/unclean behind
    /// `AlreadyStopped`.
    pub(crate) terminal: Option<ShutdownReport>,
    pub(crate) admission_closed: bool,
    pub(crate) stop_engine: Arc<AtomicBool>,
    pub(crate) stop_input: Arc<AtomicBool>,
    pub(crate) stop_logger: Arc<AtomicBool>,
    pub(crate) log_tx: Option<std::sync::mpsc::SyncSender<LogEvent>>,
    pub(crate) diagnostics: RuntimeDiagnostics,
    pub(crate) log: VecDeque<LifecycleLogEntry>,
    pub(crate) last_desired: Vec<DesiredFrame>,
}

/// Lock the shared state, recovering from poisoning so stop/reap/diagnostics
/// stay usable after a supervised thread panic. Panics never hold this lock
/// (workers panic inside backends, the engine panics outside the lock), so
/// recovery only inherits already-committed counters.
pub(crate) fn lock_shared(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Lock the focus outbox with the same poison recovery.
pub(crate) fn lock_focus(
    queue: &Mutex<Vec<FocusRequest>>,
) -> std::sync::MutexGuard<'_, Vec<FocusRequest>> {
    queue.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Advance the shared world input watermark. Call immediately BEFORE
/// enqueueing invalidating input (never after: the engine may dequeue the
/// moment the job lands, so the mark must already name it — otherwise the
/// job's own dispatch tickets a stale mark and refuses itself). Never on
/// refusal paths that enqueue nothing. On a saturated queue the mark may
/// over-advance for a dropped job; that direction is conservative (one
/// extra observation-only refusal, never stale actuation), and the
/// submitter is told to retry. Lock-free; safe mid-flight from any thread.
pub(crate) fn invalidate_clock(shared: &Mutex<Shared>) {
    lock_shared(shared).clock.invalidate();
}

impl Shared {
    /// Push one ring entry and forward its copy to the bounded async sink
    /// under the SAME lock. Sequence allocation, ring order, and channel
    /// enqueue order are therefore identical, so the sink observes the same
    /// monotonic correlation sequence as the ring. The enqueue is a single
    /// `try_send` does not wait for queue space or sink I/O: a full or gone
    /// queue drops the async copy counted. Only the dedicated logger thread
    /// calls the sink's `write`. The ring entry is always kept. See [`emit`].
    pub(crate) fn log(
        &mut self,
        pid: u32,
        generation: u64,
        ref_id: u64,
        kind: &'static str,
        outcome: &'static str,
        count: u64,
    ) {
        self.log_counter += 1;
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
            self.diagnostics.log_rotated += 1;
        }
        let timestamp_ms = self.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let entry = LifecycleLogEntry {
            seq: self.log_counter,
            timestamp_ms,
            pid,
            generation,
            ref_id,
            kind,
            outcome,
            count,
        };
        self.log.push_back(entry);
        let event = LogEvent {
            seq: self.log_counter,
            timestamp_ms,
            pid,
            generation,
            ref_id,
            kind,
            outcome,
            count,
        };
        if let Some(tx) = self.log_tx.clone()
            && tx.try_send(event).is_err()
        {
            self.diagnostics.logger_dropped += 1;
        }
    }

    pub(crate) fn count_reply(&mut self, token: &str) {
        *self
            .diagnostics
            .core_replies
            .entry(token.to_owned())
            .or_insert(0) += 1;
    }

    pub(crate) fn count_completion(&mut self, token: &str) {
        *self
            .diagnostics
            .completions
            .entry(token.to_owned())
            .or_insert(0) += 1;
    }
}

/// Append one ring entry and forward its copy to the bounded async sink
/// under the SAME shared lock. Sequence allocation, ring order, and channel
/// enqueue order are therefore identical, so the sink observes the same
/// monotonic correlation sequence as the ring. The enqueue is a single
/// `try_send` (does not wait for queue space: a full or gone queue drops the
/// async copy counted via `logger_dropped`); the ring entry is always kept.
/// Only the dedicated logger thread ever blocks, in `write` — never an
/// engine/input/app owner holding shared state.
pub(crate) fn emit(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    kind: &'static str,
    outcome: &'static str,
    count: u64,
) {
    lock_shared(shared).log(pid, generation, ref_id, kind, outcome, count);
}

/// True while the engine owner is alive (spawned and not proven exited).
/// Admission consults this so work is refused — never queued — once the
/// engine is dead.
pub(crate) fn engine_alive(shared: &Mutex<Shared>) -> bool {
    let guard = lock_shared(shared);
    if guard.admission_closed {
        return false;
    }
    guard
        .engine_handle
        .as_ref()
        .is_none_or(|h| !h.is_finished())
}
