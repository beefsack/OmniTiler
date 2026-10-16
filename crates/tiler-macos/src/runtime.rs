#![forbid(unsafe_code)]

//! Phase 1 threading/lifecycle runtime (offline-testable skeleton).
//!
//! Threading contract: the main thread runs AppKit/UI only and never blocks
//! on this runtime; one dedicated engine thread owns a single
//! [`tiler_core::engine::Engine`] plus all retained observation state; one
//! dedicated input thread owns its input source; one worker thread per
//! observed application owns its AX backend. Native handles are created,
//! used, and destroyed inside their owning thread (see
//! [`crate::runtime_backend`]); no AX object is ever `Send`.
//!
//! Handoffs are typed: [`LifecycleEvent`], [`AxObservation`] and
//! [`InputSemanticAction`] flow in; [`FrameCommand`] flows to app workers
//! through a latest-wins slot (a saturated queue coalesces but never loses
//! the final desired state); [`FocusRequest`] flows out for native dispatch.
//! A dispatch is never verified focus: only a later [`AxObservation`] is
//! truth. Actuation never waits engine-side for AX: the engine thread only
//! overwrites frame slots and never joins workers.
//!
//! Lifecycle: monotonic per-app generations plus per-app observation
//! sequences fence PID reuse and out-of-order delivery; termination marks
//! the app so queued work is dropped and late results are rejected while
//! unaffected apps keep running. Idle is wake-driven (one `Condvar`, no
//! polling); stop admission bypasses the bounded data queue; one global
//! deadline bounds shutdown; only threads proven exited are joined; a panic
//! is never a clean stop; timed-out handles are retained for [`RuntimeHandle::reap`]
//! and block overlapping replacement generations.
//!
//! The core adapter reuses the existing [`tiler_core::engine::Engine::handle`]
//! entry point with [`tiler_core::boundary::CoreCommand`]. Layout policy is
//! never duplicated here (the engine carries its default policy); the
//! single-domain constants below are skeleton placeholders — platform
//! policy extraction is separate work.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use crate::runtime_backend::{AxBackendFactory, InputSourceFactory};
use crate::runtime_input::input_main_bound;
use crate::runtime_lifecycle::{
    EngineExit, InputExit, LiveHandle, LoggerExit, WorkerExit, classify_shutdown, join_exited,
};
pub use crate::runtime_lifecycle::{ReapReport, ShutdownOutcome, ShutdownReport};
use crate::runtime_log::{LogEvent, LogSink, StderrSink, sanitize_backend_reason, spawn_logger};
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot, Inbox, engine_main};
use crate::runtime_worker::app_worker_main;

pub use crate::focus_route::FocusRoute;
pub use crate::runtime_types::{
    AX_CALL_TIMEOUT, AxCompletion, AxObservation, DesiredFrame, FocusCommand, FocusEvidence,
    FocusRequest, FrameCommand, InputDirection, InputSemanticAction, LifecycleEvent,
    LifecycleLogEntry, NativeContext, NativeDomain, ObservedWindow, PHASE1_BOUNDS, PHASE1_GAP,
    PHASE1_GENERATION, PHASE1_OUTER_GAP, PHASE1_OUTPUT, PHASE1_OWNER, PHASE1_WORKSPACE,
    RuntimeConfig, RuntimeDiagnostics, ScopedBackendSnapshot, ScopedWindowMeta, StartupError,
    SubmitOutcome, TerminateOutcome, TileBinding, TopologyMode, WorkerReadiness, parse_window_id,
    window_id,
};
pub use crate::scope::{AppScope, ProcessIdentity};
pub use crate::topology::TopologySnapshot;
/// Shared fixed-size admission predicate plus hint vector, re-exported so
/// offline tests drive both D1 predicate modes without a core dependency.
pub use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

/// Cap for the structured lifecycle log; oldest entries rotate out.
const LOG_CAP: usize = 512;

/// Recoverable failed start: the startup error plus the retained handle
/// and its bounded cleanup report. The handle stays reachable so the
/// caller can [`RuntimeHandle::reap`] timed-out owners instead of leaking
/// them behind the error. No `Debug` derive on the handle is required:
/// this type formats the error and the report only.
pub struct StartFailure {
    pub error: StartupError,
    pub handle: RuntimeHandle,
    pub report: ShutdownReport,
}

impl StartFailure {
    #[must_use]
    pub fn error(&self) -> &StartupError {
        &self.error
    }

    #[must_use]
    pub fn report(&self) -> &ShutdownReport {
        &self.report
    }

    pub fn handle(&self) -> &RuntimeHandle {
        &self.handle
    }

    pub fn into_handle(self) -> RuntimeHandle {
        self.handle
    }
}

impl std::fmt::Debug for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartFailure")
            .field("error", &self.error)
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime start failed: {}", self.error)
    }
}

impl std::error::Error for StartFailure {}

// -- Scoped observations -------------------------------------------------------

/// Revision-tagged scoped observation for native topology mode. `windows`
/// must already be routed to the current display domain (only exactly-homed
/// current-user-Space windows); `revision` must equal the current snapshot
/// fingerprint or the submission fences stale. Focus travels with its global
/// truth evidence: `frontmost_pid` (workspace frontmost), `ax_focus` (AX
/// focused PID plus window), and `cg_owned` (CG owner join for the focused
/// window). The runtime promotes `focused_wid` to observed truth only when
/// all three agree (see [`crate::focus_route::global_focus_agrees`]).
///
/// `meta` carries each window's live Space membership, without which the
/// engine cannot home the window: Space is never inferred from geometry,
/// so windows without membership evidence omit (never actuate, never
/// close).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedObservation {
    pub pid: u32,
    pub generation: u64,
    pub seq: u64,
    pub windows: Vec<ObservedWindow>,
    pub focused_wid: Option<u32>,
    pub revision: u64,
    pub frontmost_pid: Option<u32>,
    pub ax_focus: Option<(u32, Option<u32>)>,
    pub cg_owned: bool,
    pub meta: Vec<ScopedWindowMeta>,
}

// -- Shared lifecycle state ----------------------------------------------------

pub(crate) struct AppRecord {
    pub(crate) generation: u64,
    pub(crate) terminated: bool,
    stop: Arc<AtomicBool>,
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
    ready: Arc<Mutex<Option<Result<(), String>>>>,
    worker: Option<std::thread::JoinHandle<WorkerExit>>,
    worker_name: String,
}

/// Live native-context cell: the single shared source every setter leg,
// the worker rebind path, and the engine read on every fence. Stored by
// [`RuntimeHandle::update_topology`] (and cleared by
// [`RuntimeHandle::invalidate_topology`]) under the shared lock; cloned
// out as a stable handle for owner threads, which then read it lock-free
// per fence. Replaced wholesale per snapshot, never mutated in place, so
// readers always see one consistent revision. Plain data only.
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

pub(crate) struct Shared {
    pub(crate) config: RuntimeConfig,
    generation_counter: u64,
    log_counter: u64,
    start: Instant,
    pub(crate) apps: BTreeMap<u32, AppRecord>,
    /// Current native topology snapshot (the main-thread controller owns
    /// the cadence). `None` until the first
    /// [`RuntimeHandle::update_topology`]; synthetic mode never sets it.
    pub(crate) topology: Option<TopologySnapshot>,
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
    zombies: Vec<LiveHandle>,
    engine_handle: Option<std::thread::JoinHandle<EngineExit>>,
    input_handle: Option<std::thread::JoinHandle<InputExit>>,
    logger_handle: Option<std::thread::JoinHandle<LoggerExit>>,
    /// Parked input thread for prompt stop wake (`unpark` after `stop_input`
    /// is set; an `unpark` before `park` grants a permit, so no wake is
    /// lost).
    pub(crate) input_thread: Option<std::thread::Thread>,
    /// Cached terminal shutdown report: a repeated stop replays the exact
    /// terminal report instead of hiding a panic/timeout/unclean behind
    /// `AlreadyStopped`.
    terminal: Option<ShutdownReport>,
    admission_closed: bool,
    stop_engine: Arc<AtomicBool>,
    stop_input: Arc<AtomicBool>,
    stop_logger: Arc<AtomicBool>,
    log_tx: Option<std::sync::mpsc::SyncSender<LogEvent>>,
    pub(crate) diagnostics: RuntimeDiagnostics,
    log: VecDeque<LifecycleLogEntry>,
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

// -- Runtime -------------------------------------------------------------------

/// Phase 1 runtime entry point. Construct with [`Runtime::start`]; the main
/// thread stays unblocked and AppKit-only throughout.
pub struct Runtime;

/// Build the reachable handle for a start that never completed: the same
/// shared/inbox/focus/factory quadruple as the success path, so the caller
/// reaps through one uniform type instead of leaking owners behind the error.
fn partial_handle(
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    ax_factory: &Arc<dyn AxBackendFactory>,
) -> RuntimeHandle {
    RuntimeHandle {
        shared: Arc::clone(shared),
        inbox: Arc::clone(inbox),
        focus_queue: Arc::clone(focus_queue),
        ax_factory: Arc::clone(ax_factory),
    }
}

/// Take every live owner handle (engine, input, app workers, logger) plus
/// preexisting zombies for a proven-exit-only join. Call with the lock held;
/// join the result WITHOUT it.
fn take_live_handles(guard: &mut Shared) -> Vec<LiveHandle> {
    let mut pending: Vec<LiveHandle> = Vec::new();
    if let Some(handle) = guard.engine_handle.take() {
        pending.push(LiveHandle::Engine(handle));
    }
    if let Some(handle) = guard.input_handle.take() {
        pending.push(LiveHandle::Input(handle));
    }
    for (pid, record) in guard.apps.iter_mut() {
        if let Some(handle) = record.worker.take() {
            pending.push(LiveHandle::Worker {
                pid: *pid,
                generation: record.generation,
                handle,
            });
        }
    }
    if let Some(handle) = guard.logger_handle.take() {
        pending.push(LiveHandle::Logger(handle));
    }
    pending.extend(std::mem::take(&mut guard.zombies));
    pending
}

impl Runtime {
    /// Start the engine and input owners with the default stderr normal-log
    /// sink. See [`Runtime::start_with_sink`] for the full contract.
    pub fn start(
        config: RuntimeConfig,
        ax_factory: Arc<dyn AxBackendFactory>,
        input_factory: Arc<dyn InputSourceFactory>,
    ) -> Result<RuntimeHandle, StartFailure> {
        Self::start_with_sink(config, ax_factory, input_factory, Arc::new(StderrSink))
    }

    /// Start the engine and input owners. `ax_factory` and `input_factory`
    /// are shared to per-app workers / the input thread, which create their
    /// backends inside their owning threads and signal readiness. `sink`
    /// receives compact structured normal logs asynchronously through the
    /// bounded queue (`log_queue_depth`).
    ///
    /// This waits bounded for engine/input readiness
    /// (`startup_ready_timeout` each): call only from a controller/setup
    /// thread that may block, never from the main (AppKit/UI) thread. A
    /// failed start is recoverable: the error carries the retained handle
    /// plus its bounded cleanup report, so the caller can
    /// [`RuntimeHandle::reap`] timed-out owners instead of leaking them.
    /// Stop-signalled threads that still have not exited after the bound
    /// are retained as zombies (same exposure as post-shutdown zombies)
    /// and the report is dirty, never clean.
    pub fn start_with_sink(
        config: RuntimeConfig,
        ax_factory: Arc<dyn AxBackendFactory>,
        input_factory: Arc<dyn InputSourceFactory>,
        sink: Arc<dyn LogSink>,
    ) -> Result<RuntimeHandle, StartFailure> {
        if let Err(reason) = config.validate() {
            // No threads exist yet: build a detached handle-shaped failure
            // without owners so the caller still gets a uniform type.
            let (log_tx, stop_logger, logger_handle) =
                spawn_logger(sink, config.log_queue_depth.max(1));
            let shared = Arc::new(Mutex::new(Shared {
                config: config.clone(),
                generation_counter: 0,
                log_counter: 0,
                start: Instant::now(),
                apps: BTreeMap::new(),
                topology: None,
                topology_revision: 0,
                native_context: None,
                native_cell: NativeContextCell::new(),
                focus_evidence: None,
                zombies: Vec::new(),
                engine_handle: None,
                input_handle: None,
                logger_handle: Some(logger_handle),
                input_thread: None,
                terminal: None,
                admission_closed: true,
                stop_engine: Arc::new(AtomicBool::new(true)),
                stop_input: Arc::new(AtomicBool::new(true)),
                stop_logger,
                log_tx: Some(log_tx),
                diagnostics: RuntimeDiagnostics::default(),
                log: VecDeque::new(),
                last_desired: Vec::new(),
            }));
            let inbox = Arc::new(Inbox::new(config.engine_queue_depth.max(1)));
            let focus_queue = Arc::new(Mutex::new(Vec::<FocusRequest>::new()));
            let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
            let report =
                handle.shutdown(config.startup_ready_timeout.max(Duration::from_millis(1)));
            return Err(StartFailure {
                error: StartupError::InvalidConfig(reason),
                handle,
                report,
            });
        }
        let ready_bound = config.startup_ready_timeout;
        let (log_tx, stop_logger, logger_handle) =
            spawn_logger(Arc::clone(&sink), config.log_queue_depth);
        let shared = Arc::new(Mutex::new(Shared {
            config: config.clone(),
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            topology: None,
            topology_revision: 0,
            native_context: None,
            native_cell: NativeContextCell::new(),
            focus_evidence: None,
            zombies: Vec::new(),
            engine_handle: None,
            input_handle: None,
            logger_handle: Some(logger_handle),
            input_thread: None,
            terminal: None,
            admission_closed: false,
            stop_engine: Arc::new(AtomicBool::new(false)),
            stop_input: Arc::new(AtomicBool::new(false)),
            stop_logger,
            log_tx: Some(log_tx),
            diagnostics: RuntimeDiagnostics::default(),
            log: VecDeque::new(),
            last_desired: Vec::new(),
        }));
        emit(&shared, 0, 0, 0, "runtime", "starting", 0);
        let inbox = Arc::new(Inbox::new(config.engine_queue_depth));
        let focus_queue = Arc::new(Mutex::new(Vec::<FocusRequest>::new()));
        let focus_capacity = config.focus_outbox_depth;

        // Engine owner first.
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let engine_shared = Arc::clone(&shared);
        let engine_inbox = Arc::clone(&inbox);
        let engine_focus = Arc::clone(&focus_queue);
        let stop_engine = { lock_shared(&shared).stop_engine.clone() };
        let engine_thread = std::thread::Builder::new()
            .name("phase1-engine".to_owned())
            .spawn(move || {
                engine_main(
                    engine_shared,
                    engine_inbox,
                    engine_focus,
                    focus_capacity,
                    stop_engine,
                    ready_tx,
                )
            });
        let engine_thread = match engine_thread {
            Ok(handle) => handle,
            Err(e) => {
                let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
                let report = cleanup_failed_start(&shared, &inbox, ready_bound);
                return Err(StartFailure {
                    error: StartupError::EngineSpawn(e.to_string()),
                    handle,
                    report,
                });
            }
        };
        {
            lock_shared(&shared).engine_handle = Some(engine_thread);
        }
        if ready_rx.recv_timeout(ready_bound).is_err() {
            // The engine never became ready: bounded cleanup keeps the
            // handle reachable for reap instead of leaking behind the error.
            let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
            let report = cleanup_failed_start(&shared, &inbox, ready_bound);
            return Err(StartFailure {
                error: StartupError::EngineNotReady,
                handle,
                report,
            });
        }

        // Dedicated input owner second; failure stops the engine again.
        let (input_ready_tx, input_ready_rx) = mpsc::channel::<Result<(), String>>();
        let input_shared = Arc::clone(&shared);
        let input_inbox = Arc::clone(&inbox);
        let stop_input = { lock_shared(&shared).stop_input.clone() };
        let direction = config.step_direction;
        let modifier = config.modifier;
        // The scoped-tile catalogue replaces the legacy three-chord path
        // when set; otherwise the legacy chords stand in as a three-entry
        // catalogue (identical matching, no behavior change).
        let catalogue = config.tile_bindings.clone().unwrap_or_else(|| {
            vec![
                crate::runtime_types::TileBinding {
                    key_code: config.focus_key,
                    shift: false,
                    alt: false,
                    aux: false,
                    action: crate::runtime_types::InputSemanticAction::FocusStep { direction },
                },
                crate::runtime_types::TileBinding {
                    key_code: config.move_key,
                    shift: false,
                    alt: false,
                    aux: false,
                    action: crate::runtime_types::InputSemanticAction::MoveStep { direction },
                },
                crate::runtime_types::TileBinding {
                    key_code: config.float_key,
                    shift: false,
                    alt: false,
                    aux: false,
                    action: crate::runtime_types::InputSemanticAction::ToggleFloat,
                },
            ]
        });
        let input_thread = std::thread::Builder::new()
            .name("phase1-input".to_owned())
            .spawn(move || {
                input_main_bound(
                    input_shared,
                    input_inbox,
                    stop_input,
                    input_factory,
                    modifier,
                    &catalogue,
                    input_ready_tx,
                )
            });
        let input_thread = match input_thread {
            Ok(handle) => handle,
            Err(e) => {
                let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
                let report = cleanup_failed_start(&shared, &inbox, ready_bound);
                return Err(StartFailure {
                    error: StartupError::InputSpawn(e.to_string()),
                    handle,
                    report,
                });
            }
        };
        {
            lock_shared(&shared).input_handle = Some(input_thread);
        }
        match input_ready_rx.recv_timeout(ready_bound) {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
                let report = cleanup_failed_start(&shared, &inbox, ready_bound);
                // Sanitize the refusal for the typed error: the raw backend
                // string never becomes a log outcome.
                let token = sanitize_backend_reason(&reason);
                emit(&shared, 0, 0, 0, "startup", "input-refused", 0);
                return Err(StartFailure {
                    error: StartupError::InputRefused(token.to_string()),
                    handle,
                    report,
                });
            }
            Err(_) => {
                let handle = partial_handle(&shared, &inbox, &focus_queue, &ax_factory);
                let report = cleanup_failed_start(&shared, &inbox, ready_bound);
                emit(&shared, 0, 0, 0, "startup", "input-not-ready", 0);
                return Err(StartFailure {
                    error: StartupError::InputRefused(
                        "input thread did not signal readiness".to_owned(),
                    ),
                    handle,
                    report,
                });
            }
        }

        emit(&shared, 0, 0, 0, "runtime", "started", 0);
        Ok(RuntimeHandle {
            shared,
            inbox,
            focus_queue,
            ax_factory,
        })
    }
}

/// Bounded cleanup for a runtime that never became reachable: close
/// admission, signal every owner (including the logger, last), unpark the
/// input thread, close the inbox, then join only threads proven exited.
/// Anything still blocking stays stop-signalled and retained in
/// `zombies` for [`RuntimeHandle::reap`]. Returns the honest cleanup
/// report (dirty on any panic, retained handle, unclean teardown, or
/// unexpected EOS — never a pretend clean).
fn cleanup_failed_start(
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    bound: Duration,
) -> ShutdownReport {
    let start = Instant::now();
    emit(shared, 0, 0, 0, "startup", "cleaning", 0);
    let (stop_logger, input_thread) = {
        let mut guard = lock_shared(shared);
        guard.admission_closed = true;
        guard.stop_engine.store(true, Ordering::SeqCst);
        guard.stop_input.store(true, Ordering::SeqCst);
        (guard.stop_logger.clone(), guard.input_thread.clone())
    };
    if let Some(thread) = input_thread {
        thread.unpark();
    }
    inbox.close_and_drain();
    inbox.wake_all();
    let deadline = Instant::now() + bound;
    loop {
        let done = {
            let guard = lock_shared(shared);
            let engine_done = guard.engine_handle.as_ref().is_none_or(|h| h.is_finished());
            let input_done = guard.input_handle.as_ref().is_none_or(|h| h.is_finished());
            engine_done && input_done
        };
        if done || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // The logger stays alive for the teardown logs, then stops under the
    // same bound.
    stop_logger.store(true, Ordering::SeqCst);
    let logger_done_at = Instant::now() + Duration::from_millis(200).min(bound);
    while Instant::now() < logger_done_at {
        let done = lock_shared(shared)
            .logger_handle
            .as_ref()
            .is_none_or(|h| h.is_finished());
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // Join only proven exits; the rest stay stop-signalled as zombies.
    // Joins happen without holding the shared lock.
    let pending: Vec<LiveHandle> = take_live_handles(&mut lock_shared(shared));
    let (joined, panics, unclean, retained) = join_exited(pending);
    {
        let mut guard = lock_shared(shared);
        guard.zombies.extend(retained);
        if panics > 0 {
            guard.diagnostics.worker_panics_observed += panics as u64;
        }
        if unclean > 0 {
            guard.diagnostics.worker_unclean_observed += unclean as u64;
        }
        let eos = guard.diagnostics.unexpected_eos;
        let outcome = classify_shutdown(
            guard.diagnostics.worker_panics_observed,
            guard.zombies.len(),
            guard.diagnostics.worker_unclean_observed,
            eos,
        );
        let outcome_token: &'static str = match outcome {
            ShutdownOutcome::Clean => "clean",
            ShutdownOutcome::DirtyTimeout => "dirty-timeout",
            ShutdownOutcome::Unclean => "unclean",
            ShutdownOutcome::Panic => "panic",
            ShutdownOutcome::AlreadyStopped => "already-stopped",
        };
        let report = ShutdownReport {
            outcome,
            elapsed_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            joined,
            retained: guard.zombies.len(),
            panics: guard.diagnostics.worker_panics_observed as usize,
            unclean: guard.diagnostics.worker_unclean_observed as usize,
        };
        guard.terminal = Some(report.clone());
        // Best effort under the same lock: the logger may already be gone;
        // the ring entry is the durable copy either way.
        guard.log(0, 0, 0, "startup", outcome_token, joined as u64);
        drop(guard);
        report
    }
}

/// Live handle to a started runtime. All methods are non-blocking except
/// [`RuntimeHandle::flush`], [`RuntimeHandle::shutdown`] and
/// [`RuntimeHandle::reap`], which wait bounded. Safe to use from the main
/// thread; never activates the desktop.
pub struct RuntimeHandle {
    shared: Arc<Mutex<Shared>>,
    inbox: Arc<Inbox>,
    focus_queue: Arc<Mutex<Vec<FocusRequest>>>,
    ax_factory: Arc<dyn AxBackendFactory>,
}

impl RuntimeHandle {
    /// Admit one application PID: allocate the next monotonic generation and
    /// spawn its worker, which creates the AX backend inside the owning
    /// thread and reports its own `Launch` to the engine once the backend
    /// exists. Returns the generation immediately without waiting for
    /// backend readiness: this is the nonblocking path for the main/UI
    /// thread. Poll [`RuntimeHandle::worker_ready`] for backend readiness;
    /// backend creation can never stall the main thread, the engine, or
    /// another app. Refuses overlapping replacement while the previous
    /// generation still executes. No extra thread is created: the worker
    /// itself is the only new thread.
    pub fn launch_app_async(&self, pid: u32) -> Result<u64, StartupError> {
        if pid <= 1 {
            return Err(StartupError::InvalidConfig(format!(
                "refusing reserved pid {pid}"
            )));
        }
        if !engine_alive(&self.shared) {
            return Err(StartupError::AdmissionClosed);
        }
        // Reap any already-exited worker for this PID so a finished
        // generation never blocks replacement; a still-executing one
        // refuses the overlap. The join happens outside the shared lock.
        let finished_worker = {
            let guard = lock_shared(&self.shared);
            if guard.admission_closed {
                return Err(StartupError::AdmissionClosed);
            }
            let prior = guard.apps.get(&pid).map(|record| {
                let finished = record.worker.as_ref().is_some_and(|h| h.is_finished());
                (record.generation, finished)
            });
            if let Some((generation, false)) = prior {
                return Err(StartupError::PreviousGenerationRunning { pid, generation });
            }
            prior.map(|(generation, _)| generation)
        };
        if let Some(generation) = finished_worker {
            // Proven exited above; join outside the lock, then remove only if
            // the generation did not change under us.
            let worker = {
                let mut guard = lock_shared(&self.shared);
                guard
                    .apps
                    .get_mut(&pid)
                    .filter(|record| record.generation == generation)
                    .and_then(|record| record.worker.take())
            };
            if let Some(worker) = worker {
                let (panicked, unclean) = match worker.join() {
                    Err(_) => (true, false),
                    Ok(WorkerExit::Clean) => (false, false),
                    Ok(WorkerExit::Unclean { .. }) => (false, true),
                };
                // Persistent evidence: a reaped panic or unclean teardown
                // still poisons the terminal shutdown.
                if panicked {
                    lock_shared(&self.shared).diagnostics.worker_panics_observed += 1;
                    emit(
                        &self.shared,
                        pid,
                        generation,
                        0,
                        "worker",
                        "panic-reaped",
                        0,
                    );
                } else if unclean {
                    lock_shared(&self.shared)
                        .diagnostics
                        .worker_unclean_observed += 1;
                    emit(
                        &self.shared,
                        pid,
                        generation,
                        0,
                        "worker",
                        "unclean-reaped",
                        0,
                    );
                }
                let mut guard = lock_shared(&self.shared);
                let current = guard.apps.get(&pid).map(|record| record.generation);
                if current == Some(generation) {
                    guard.apps.remove(&pid);
                }
            }
            // Re-check admission and overlap after the join.
            let guard = lock_shared(&self.shared);
            if guard.admission_closed {
                return Err(StartupError::AdmissionClosed);
            }
            if let Some(record) = guard.apps.get(&pid) {
                return Err(StartupError::PreviousGenerationRunning {
                    pid,
                    generation: record.generation,
                });
            }
        }
        let (frames, focus, stop, ready, generation) = {
            let mut guard = lock_shared(&self.shared);
            // A retained (timed-out) handle for this PID also blocks overlap.
            let blocked = guard.zombies.iter().any(|z| match z {
                LiveHandle::Worker {
                    pid: zpid,
                    generation: _,
                    handle,
                } => *zpid == pid && !handle.is_finished(),
                _ => false,
            });
            if blocked {
                let generation = guard
                    .zombies
                    .iter()
                    .filter_map(|z| match z {
                        LiveHandle::Worker {
                            pid: zpid,
                            generation,
                            ..
                        } if *zpid == pid => Some(*generation),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0);
                return Err(StartupError::PreviousGenerationRunning { pid, generation });
            }
            guard.generation_counter += 1;
            let generation = guard.generation_counter;
            let frames = Arc::new(FrameSlot::new());
            let focus = Arc::new(FocusSlot::new());
            let stop = Arc::new(AtomicBool::new(false));
            let ready = Arc::new(Mutex::new(None));
            guard.apps.insert(
                pid,
                AppRecord {
                    generation,
                    terminated: false,
                    stop: Arc::clone(&stop),
                    frames: Arc::clone(&frames),
                    focus: Arc::clone(&focus),
                    ready: Arc::clone(&ready),
                    worker: None,
                    worker_name: format!("phase1-ax-{pid}"),
                },
            );
            (frames, focus, stop, ready, generation)
        };

        let worker_shared = Arc::clone(&self.shared);
        let worker_inbox = Arc::clone(&self.inbox);
        let worker_factory = Arc::clone(&self.ax_factory);
        let worker_name = format!("phase1-ax-{pid}");
        let worker = std::thread::Builder::new()
            .name(worker_name.clone())
            .spawn(move || {
                app_worker_main(
                    pid,
                    generation,
                    worker_factory,
                    frames,
                    focus,
                    worker_inbox,
                    worker_shared,
                    stop,
                    ready,
                )
            })
            .map_err(|e| {
                lock_shared(&self.shared).apps.remove(&pid);
                StartupError::WorkerSpawn(e.to_string())
            })?;
        {
            let mut guard = lock_shared(&self.shared);
            if let Some(record) = guard.apps.get_mut(&pid) {
                record.worker = Some(worker);
                record.worker_name = worker_name;
            }
        }
        emit(&self.shared, pid, generation, 0, "lifecycle", "launched", 0);
        Ok(generation)
    }

    /// Synchronous launch convenience: [`RuntimeHandle::launch_app_async`]
    /// plus a bounded wait for backend readiness. Offline/controller use
    /// only — never call from the main (AppKit/UI) thread, because a slow
    /// backend blocks the caller until `worker_ready_timeout`. Main/UI code
    /// uses `launch_app_async` and polls [`RuntimeHandle::worker_ready`].
    pub fn launch_app(&self, pid: u32) -> Result<u64, StartupError> {
        let generation = self.launch_app_async(pid)?;
        let ready_timeout = { lock_shared(&self.shared).config.worker_ready_timeout };
        let deadline = Instant::now() + ready_timeout;
        loop {
            let state = self.worker_ready(pid);
            match state {
                WorkerReadiness::Ready => {
                    // The worker reported its own Launch before marking
                    // ready, so admission is already queued. A concurrent
                    // stop still tears this down instead of leaking it.
                    if lock_shared(&self.shared).admission_closed {
                        self.remove_app_entry(pid);
                        return Err(StartupError::AdmissionClosed);
                    }
                    return Ok(generation);
                }
                WorkerReadiness::Failed(reason) => {
                    self.remove_app_entry(pid);
                    return Err(StartupError::BackendRefused(reason));
                }
                WorkerReadiness::UnknownPid => {
                    // Removed concurrently (stop raced launch).
                    return Err(StartupError::AdmissionClosed);
                }
                WorkerReadiness::Pending => {
                    if Instant::now() >= deadline {
                        self.remove_app_entry(pid);
                        return Err(StartupError::WorkerNotReady);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// Backend readiness for the nonblocking launch path: `Pending` while
    /// the worker is still creating its backend inside the owning thread,
    /// `Ready` once it exists (and its engine `Launch` is queued),
    /// `Failed` with the refusal reason, `UnknownPid` for no such app.
    pub fn worker_ready(&self, pid: u32) -> WorkerReadiness {
        let guard = lock_shared(&self.shared);
        let Some(record) = guard.apps.get(&pid) else {
            return WorkerReadiness::UnknownPid;
        };
        match record
            .ready
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
        {
            None => WorkerReadiness::Pending,
            Some(Ok(())) => WorkerReadiness::Ready,
            Some(Err(reason)) => WorkerReadiness::Failed(reason),
        }
    }

    fn remove_app_entry(&self, pid: u32) {
        let (zombie, generation) = {
            let mut guard = lock_shared(&self.shared);
            let generation = guard.apps.get(&pid).map(|r| r.generation).unwrap_or(0);
            let zombie = guard.apps.remove(&pid).and_then(|record| {
                record.stop.store(true, Ordering::SeqCst);
                record.frames.stop();
                record.focus.stop();
                record.worker.map(|handle| LiveHandle::Worker {
                    pid,
                    generation: record.generation,
                    handle,
                })
            });
            (zombie, generation)
        };
        // Join only a proven exit, outside the shared lock; a still-blocking
        // backend stays reaped later via `reap`.
        if let Some(handle) = zombie {
            let (_, panics, unclean, retained) = join_exited(vec![handle]);
            if panics > 0 {
                lock_shared(&self.shared).diagnostics.worker_panics_observed += panics as u64;
                emit(
                    &self.shared,
                    pid,
                    generation,
                    0,
                    "worker",
                    "panic-reaped",
                    0,
                );
            }
            if unclean > 0 {
                lock_shared(&self.shared)
                    .diagnostics
                    .worker_unclean_observed += unclean as u64;
                emit(
                    &self.shared,
                    pid,
                    generation,
                    0,
                    "worker",
                    "unclean-reaped",
                    0,
                );
            }
            if !retained.is_empty() {
                emit(
                    &self.shared,
                    pid,
                    generation,
                    0,
                    "worker",
                    "not-ready-retained",
                    0,
                );
                lock_shared(&self.shared).zombies.extend(retained);
            }
        }
        self.inbox.wake_all();
    }

    /// Mark an app terminated: queued frame work is dropped, queued focus
    /// requests for the app are purged, the worker is signalled, and late
    /// results are rejected by generation. Unaffected apps keep running.
    pub fn terminate_app(&self, pid: u32) -> TerminateOutcome {
        let (generation, frames) = {
            let mut guard = lock_shared(&self.shared);
            if guard.admission_closed {
                return TerminateOutcome::RuntimeStopped;
            }
            let state = guard.apps.get(&pid).map(|record| {
                (
                    record.generation,
                    record.terminated,
                    Arc::clone(&record.stop),
                    Arc::clone(&record.frames),
                )
            });
            let Some((generation, terminated, stop, frames)) = state else {
                return TerminateOutcome::UnknownPid;
            };
            if terminated {
                return TerminateOutcome::AlreadyTerminated;
            }
            if let Some(record) = guard.apps.get_mut(&pid) {
                record.terminated = true;
            }
            stop.store(true, Ordering::SeqCst);
            (generation, frames)
        };
        emit(
            &self.shared,
            pid,
            generation,
            0,
            "lifecycle",
            "terminated",
            0,
        );
        frames.stop();
        if let Some(record) = lock_shared(&self.shared).apps.get(&pid) {
            record.focus.stop();
        }
        // Purge queued focus for the dead app: dispatch must never outlive
        // its lifetime. Sequential locks only (focus, then shared): holding
        // the focus guard across a shared lock would invert the
        // shared-then-focus order used by shutdown and deadlock.
        let purged = {
            let mut queue = lock_focus(&self.focus_queue);
            let before = queue.len();
            queue.retain(|request| request.pid != pid);
            (before - queue.len()) as u64
        };
        if purged > 0 {
            lock_shared(&self.shared).diagnostics.terminated_invalidated += purged;
            emit(
                &self.shared,
                pid,
                0,
                0,
                "focus",
                "terminated-purged",
                purged,
            );
        }
        // Control lane: never dropped by data saturation.
        let _ = self
            .inbox
            .send_control(EngineJob::Terminate { pid, generation });
        self.inbox.wake_all();
        TerminateOutcome::Terminated { generation }
    }

    /// Submit one typed AX observation. The observation carries its
    /// originating app lifetime: a generation that does not match the live
    /// one is refused as `StaleGeneration` (counted) instead of being
    /// stamped with the current generation, so late observations from a
    /// previous lifetime can never pollute a relaunched PID. Admission is
    /// bounded: a saturated engine queue refuses with a counted outcome
    /// instead of blocking the caller. Sequence staleness is decided
    /// engine-side in FIFO order.
    ///
    /// Native topology mode refuses legacy (metadataless) observations as
    /// `RequiresScoped`: without per-window Space membership the engine
    /// cannot home a single window (Space is never inferred from geometry),
    /// so admitting them would tile outside the actual work areas. Scoped
    /// observations travel via [`RuntimeHandle::submit_scoped_observation`].
    pub fn submit_observation(&self, obs: AxObservation) -> SubmitOutcome {
        let generation = {
            let mut guard = lock_shared(&self.shared);
            if guard.admission_closed {
                return SubmitOutcome::AdmissionClosed;
            }
            if guard
                .engine_handle
                .as_ref()
                .is_some_and(|h| h.is_finished())
            {
                return SubmitOutcome::AdmissionClosed;
            }
            if matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            ) {
                guard.diagnostics.stale_dropped += 1;
                drop(guard);
                emit(
                    &self.shared,
                    obs.pid,
                    obs.generation,
                    obs.seq,
                    "observation",
                    "native-requires-scoped",
                    0,
                );
                return SubmitOutcome::RequiresScoped;
            }
            let Some(record) = guard.apps.get(&obs.pid) else {
                return SubmitOutcome::UnknownApp;
            };
            if record.terminated {
                return SubmitOutcome::AppTerminated;
            }
            if obs.generation != record.generation {
                guard.diagnostics.stale_dropped += 1;
                drop(guard);
                emit(
                    &self.shared,
                    obs.pid,
                    obs.generation,
                    obs.seq,
                    "observation",
                    "stale-generation-rejected",
                    0,
                );
                return SubmitOutcome::StaleGeneration;
            }
            record.generation
        };
        let job = EngineJob::Observation {
            pid: obs.pid,
            generation,
            seq: obs.seq,
            windows: obs.windows,
            focused_wid: obs.focused_wid,
        };
        if self.inbox.try_send_data(job) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Emit the lifecycle event for tests and future platform shims.
    /// Currently `Launch` maps to [`RuntimeHandle::launch_app`] and
    /// `Terminate` to [`RuntimeHandle::terminate_app`].
    pub fn submit_lifecycle(&self, event: LifecycleEvent) -> Result<u64, StartupError> {
        match event {
            LifecycleEvent::Launch { pid } => self.launch_app(pid),
            LifecycleEvent::Terminate { pid } => match self.terminate_app(pid) {
                TerminateOutcome::Terminated { generation } => Ok(generation),
                TerminateOutcome::UnknownPid => Err(StartupError::InvalidConfig(format!(
                    "terminate: unknown pid {pid}"
                ))),
                TerminateOutcome::AlreadyTerminated => Err(StartupError::InvalidConfig(format!(
                    "terminate: pid {pid} already terminated"
                ))),
                TerminateOutcome::RuntimeStopped => Err(StartupError::AdmissionClosed),
            },
        }
    }

    /// Publish a native topology snapshot from the main-thread controller.
    /// Stores the snapshot, fences the revision (fingerprint), purges queued
    /// focus requests from superseded revisions (counted), and emits the
    /// change. Returns the new revision. Synthetic mode ignores snapshots
    /// (legacy fixtures keep revision 0); native mode requires them before
    /// scoped observations admit.
    ///
    /// In native mode this also derives the shared tile context with the
    /// live read-only Space-type read (main thread): skipped displays are
    /// emitted with reasons and tile nothing. See
    /// [`RuntimeHandle::update_topology_with`] for the scripted-type form
    /// that portable tests use.
    pub fn update_topology(&self, snapshot: TopologySnapshot) -> u64 {
        self.update_topology_with(snapshot, &|sid| crate::ffi::sls_space_type(sid).ok())
    }

    /// Publish a native topology snapshot with an injected Space-type
    /// resolver (`None` = unreadable type). The live side passes the
    /// existing `sls_space_type` read (see [`RuntimeHandle::update_topology`]);
    /// tests inject scripted types. Other than the resolver this is
    /// identical to [`RuntimeHandle::update_topology`].
    pub fn update_topology_with(
        &self,
        snapshot: TopologySnapshot,
        space_type: &dyn Fn(u64) -> Option<crate::ffi::SlsSpaceType>,
    ) -> u64 {
        let revision = snapshot.revision;
        let native = matches!(
            lock_shared(&self.shared).config.topology_mode,
            crate::runtime_types::TopologyMode::Native
        );
        let (context, skipped) = if native {
            let (ctx, skipped) = NativeContext::derive(&snapshot, space_type);
            (Some(Arc::new(ctx)), skipped.len() as u64)
        } else {
            (None, 0)
        };
        if skipped > 0 {
            // Skipped displays tile nothing (reasons ride the derived
            // context's skipped list for controller inspection via
            // diagnostics-free means; the count here keeps the ring typed).
            emit(
                &self.shared,
                0,
                0,
                revision,
                "topology",
                "displays-skipped",
                skipped,
            );
        }
        let purged = {
            let mut guard = lock_shared(&self.shared);
            guard.topology = Some(snapshot);
            guard.topology_revision = revision;
            guard.native_context = context.clone();
            guard.native_cell.store(context);
            let mut queue = lock_focus(&self.focus_queue);
            let before = queue.len();
            queue.retain(|request| request.topology_revision == revision);
            (before - queue.len()) as u64
        };
        if purged > 0 {
            lock_shared(&self.shared).diagnostics.stale_dropped += purged;
            emit(&self.shared, 0, 0, 0, "topology", "focus-purged", purged);
        }
        emit(&self.shared, 0, 0, revision, "topology", "updated", 0);
        revision
    }

    /// Publish main-thread-supplied global focus evidence (workspace
    /// frontmost PID plus bundle, plain IDs — never AX reads: the main
    /// thread performs no AX under any cadence). The controller owns the
    /// cadence (alongside topology pumps); per-app owners read the bounded
    /// systemwide focus truth on their own threads and stamp it over
    /// `ax_focus` at send time, and the promotion rule admits an app-local
    /// focused window only on full agreement, so a background app's focused
    /// window never becomes global focus.
    ///
    /// Change-only: republishing identical evidence neither restores state
    /// nor emits. The controller pumps this every cadence, so an
    /// unconditional emit is idle trace spam (one ring line per cadence
    /// forever); the worker's frontmost-change refresh is value-compared
    /// downstream and needs no redundant wake.
    pub fn update_focus_evidence(&self, evidence: FocusEvidence) {
        let changed = {
            let mut guard = lock_shared(&self.shared);
            if guard.focus_evidence.as_ref() == Some(&evidence) {
                false
            } else {
                guard.focus_evidence = Some(evidence);
                true
            }
        };
        if changed {
            emit(&self.shared, 0, 0, 0, "focus", "evidence-updated", 0);
        }
    }

    /// Invalidate the published native context after an unreadable topology
    /// snapshot read: clears the live cell and the stored snapshot alike so
    /// no stale revision keeps authorizing effects. Scoped submissions
    /// refuse (`StaleGeneration`), the engine fences (`no-context`), and
    /// bound backends fail every scoped call closed until the next
    /// successful [`RuntimeHandle::update_topology`]. Supplied for the main
    /// tile controller (next unit), which owns the snapshot cadence.
    pub fn invalidate_topology(&self) {
        {
            let mut guard = lock_shared(&self.shared);
            guard.topology = None;
            guard.native_context = None;
            guard.native_cell.store(None);
        }
        emit(&self.shared, 0, 0, 0, "topology", "invalidated", 0);
    }

    /// Current topology revision fence (`None` until the first snapshot).
    pub fn topology_revision(&self) -> Option<u64> {
        lock_shared(&self.shared)
            .topology
            .as_ref()
            .map(|t| t.revision)
    }

    /// Submit one revision-tagged scoped observation. The windows must
    /// already be routed to the current display domain by the controller
    /// (see [`crate::native_tile_backend::route_display_domain`]); this
    /// checks the revision fence and the global-focus truth gate, then
    /// forwards through the scoped engine lane (per-domain sessions over
    /// the actual work areas — never the synthetic skeleton).
    /// A superseded revision is refused as `StaleGeneration` (counted);
    /// native mode with no snapshot yet refuses the same way. Focus is
    /// accepted only when the workspace-frontmost plus AX preimage agree
    /// on the reporting app (see [`crate::focus_route::global_focus_agrees`]):
    /// an app-local focused window from a background app arrives as
    /// unfocused instead of becoming global truth. Windows without
    /// membership evidence (`meta`) omit engine-side: Space is never
    /// inferred from geometry.
    pub fn submit_scoped_observation(&self, obs: ScopedObservation) -> SubmitOutcome {
        let (generation, native) = {
            let mut guard = lock_shared(&self.shared);
            if guard.admission_closed {
                return SubmitOutcome::AdmissionClosed;
            }
            if guard
                .engine_handle
                .as_ref()
                .is_some_and(|h| h.is_finished())
            {
                return SubmitOutcome::AdmissionClosed;
            }
            let Some(record) = guard.apps.get(&obs.pid) else {
                return SubmitOutcome::UnknownApp;
            };
            if record.terminated {
                return SubmitOutcome::AppTerminated;
            }
            if obs.generation != record.generation {
                guard.diagnostics.stale_dropped += 1;
                drop(guard);
                emit(
                    &self.shared,
                    obs.pid,
                    obs.generation,
                    obs.seq,
                    "observation",
                    "stale-generation-rejected",
                    0,
                );
                return SubmitOutcome::StaleGeneration;
            }
            let native = matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            );
            if native {
                // Absent topology refuses first (before the revision
                // compare): without a published snapshot there is no fence
                // to match, only an unscoped past.
                if guard.topology.is_none() {
                    guard.diagnostics.stale_dropped += 1;
                    drop(guard);
                    emit(
                        &self.shared,
                        obs.pid,
                        obs.generation,
                        obs.seq,
                        "observation",
                        "no-topology-rejected",
                        0,
                    );
                    return SubmitOutcome::StaleGeneration;
                }
                if guard.topology_revision != obs.revision {
                    guard.diagnostics.stale_dropped += 1;
                    drop(guard);
                    emit(
                        &self.shared,
                        obs.pid,
                        obs.generation,
                        obs.seq,
                        "observation",
                        "stale-revision-rejected",
                        0,
                    );
                    return SubmitOutcome::StaleGeneration;
                }
            }
            (record.generation, native)
        };
        let _ = native;
        // Global-focus verdict on the caller-supplied legs (single shared
        // rule — see `resolve_global_focus`): only full selected-oracle
        // (SLPS + workspace + direct AX + CG) agreement promotes the
        // app-local window. The engine additionally requires domain
        // membership before promoting.
        let global_focus = crate::focus_route::resolve_global_focus(
            obs.pid,
            obs.focused_wid,
            obs.ax_focus,
            obs.cg_owned,
            obs.frontmost_pid,
        );
        if obs.focused_wid.is_some() && global_focus.is_none() {
            lock_shared(&self.shared).diagnostics.stale_dropped += 1;
            emit(
                &self.shared,
                obs.pid,
                obs.generation,
                obs.seq,
                "observation",
                "focus-disagreement-cleared",
                0,
            );
        }
        let focused_wid = global_focus.map(|(_, wid)| wid);
        let job = EngineJob::ScopedObservation {
            pid: obs.pid,
            generation,
            seq: obs.seq,
            snapshot: ScopedBackendSnapshot {
                windows: obs.windows,
                focused_wid,
                focus_cg_owned: obs.cg_owned,
                focus_evidence: Some(FocusEvidence {
                    frontmost_pid: obs.frontmost_pid,
                    frontmost_bundle: None,
                    ax_focus: obs.ax_focus,
                }),
                sys_focus: obs.ax_focus,
                global_focus,
                meta: obs.meta,
                excluded: 0,
                revision: obs.revision,
            },
        };
        if self.inbox.try_send_data(job) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Request the minimal float toggle for the observed-focus window: sends
    /// a toggle intent through the engine lane, resolved there to the
    /// existing core `ToggleFloat` command with no explicit rect. The
    /// subject is observed truth, never planned focus; with no observed
    /// focus this is refused engine-side (counted), never queued blindly.
    pub fn request_float_toggle(&self) -> SubmitOutcome {
        if !engine_alive(&self.shared) {
            return SubmitOutcome::AdmissionClosed;
        }
        if self.inbox.try_send_data(EngineJob::InputAction {
            action: InputSemanticAction::ToggleFloat,
        }) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Offline fault injection only: makes the engine thread panic on its
    /// next job, proving panic handling is honest. Never used by
    /// production paths.
    pub fn inject_engine_panic_for_test(&self) -> bool {
        self.inbox.send_control(EngineJob::TestPanic)
    }

    /// Wait until the engine has processed every job submitted before this
    /// call (FIFO barrier). Returns false on timeout or after stop.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (tx, rx) = mpsc::channel::<()>();
        if !self.inbox.send_control(EngineJob::Barrier(tx)) {
            return false;
        }
        rx.recv_timeout(timeout).is_ok()
    }

    /// Drain queued focus requests (dispatched, never verified). Freshness
    /// is re-checked here: requests whose app was terminated or relaunched
    /// since dispatch are dropped counted instead of delivered, closing the
    /// race between dispatch and terminate. Revision staleness is fenced
    /// too: requests from a superseded topology revision drain stale after
    /// a display/Space change mid-plan instead of dispatching.
    pub fn drain_focus_requests(&self) -> Vec<FocusRequest> {
        // Snapshot live generations without holding shared across the focus
        // lock (order is shared-then-focus; here sequential is enough).
        let (live, revision): (BTreeMap<u32, (u64, bool)>, u64) = {
            let guard = lock_shared(&self.shared);
            (
                guard
                    .apps
                    .iter()
                    .map(|(pid, record)| (*pid, (record.generation, record.terminated)))
                    .collect(),
                guard.topology_revision,
            )
        };
        let mut queue = lock_focus(&self.focus_queue);
        let requests = std::mem::take(&mut *queue);
        drop(queue);
        let mut fresh = Vec::with_capacity(requests.len());
        let mut stale = 0u64;
        for request in requests {
            let live_match = live
                .get(&request.pid)
                .is_some_and(|(generation, terminated)| {
                    *generation == request.generation && !terminated
                });
            // Synthetic commands carry revision 0 against revision 0 and
            // pass; native commands must match the live fence.
            let revision_match = request.topology_revision == revision;
            if live_match && revision_match {
                fresh.push(request);
            } else {
                stale += 1;
            }
        }
        if stale > 0 {
            lock_shared(&self.shared).diagnostics.stale_dropped += stale;
            emit(&self.shared, 0, 0, 0, "focus", "stale-drained", stale);
        }
        fresh
    }

    /// Snapshot of structured diagnostics.
    pub fn diagnostics(&self) -> RuntimeDiagnostics {
        let mut guard = lock_shared(&self.shared);
        guard.diagnostics.apps_live = guard.apps.values().filter(|r| !r.terminated).count();
        guard.diagnostics.clone()
    }

    /// Snapshot of the structured lifecycle log.
    pub fn log_snapshot(&self) -> Vec<LifecycleLogEntry> {
        lock_shared(&self.shared).log.iter().cloned().collect()
    }

    /// Snapshot of the last desired geometry the core planned.
    pub fn last_desired_geometry(&self) -> Vec<DesiredFrame> {
        lock_shared(&self.shared).last_desired.clone()
    }

    /// Current generation for a live app, if any.
    pub fn app_generation(&self, pid: u32) -> Option<u64> {
        lock_shared(&self.shared)
            .apps
            .get(&pid)
            .map(|r| r.generation)
    }

    /// Whether the runtime can still admit work. False once stopped and
    /// false when an engine/input owner unexpectedly died: admission to a
    /// dead engine is closed.
    pub fn is_running(&self) -> bool {
        let guard = lock_shared(&self.shared);
        if guard.admission_closed {
            return false;
        }
        let engine_dead = guard
            .engine_handle
            .as_ref()
            .is_some_and(|h| h.is_finished());
        let input_dead = guard.input_handle.as_ref().is_some_and(|h| h.is_finished());
        !(engine_dead || input_dead)
    }

    /// Stop everything under one global deadline: close admission,
    /// invalidate queued work (including queued focus), signal every owner
    /// (logger last, so teardown logs still land), then join only threads
    /// proven exited. A panic, a missed deadline, a returned unclean
    /// teardown, or an unexpected EOS is dirty, never clean — and so is any
    /// previously reaped panic/unclean or any retained preexisting zombie.
    /// Timed-out handles are retained for [`RuntimeHandle::reap`] and block
    /// overlapping replacement generations until reaped. The terminal report
    /// is cached: a repeated stop replays the exact terminal report instead
    /// of hiding a panic/timeout/unclean behind `AlreadyStopped`.
    pub fn shutdown(&self, deadline: Duration) -> ShutdownReport {
        let start = Instant::now();
        // Fast path: replay the exact cached terminal report.
        {
            let guard = lock_shared(&self.shared);
            if let Some(terminal) = guard.terminal.clone() {
                return terminal;
            }
        }
        {
            let mut guard = lock_shared(&self.shared);
            guard.admission_closed = true;
            guard.stop_engine.store(true, Ordering::SeqCst);
            guard.stop_input.store(true, Ordering::SeqCst);
            let slots: Vec<Arc<FrameSlot>> =
                guard.apps.values().map(|r| Arc::clone(&r.frames)).collect();
            let focus_slots: Vec<Arc<FocusSlot>> =
                guard.apps.values().map(|r| Arc::clone(&r.focus)).collect();
            let stops: Vec<Arc<AtomicBool>> =
                guard.apps.values().map(|r| Arc::clone(&r.stop)).collect();
            for slot in &slots {
                slot.stop();
            }
            for slot in &focus_slots {
                slot.stop();
            }
            for stop in &stops {
                stop.store(true, Ordering::SeqCst);
            }
            let (control_dropped, data_dropped) = self.inbox.close_and_drain();
            guard.diagnostics.invalidated_on_stop += control_dropped + data_dropped;
            // Queued focus dies with the runtime; it must never dispatch
            // after stop.
            let dropped = {
                let mut queue = lock_focus(&self.focus_queue);
                let dropped = queue.len() as u64;
                queue.clear();
                dropped
            };
            guard.diagnostics.invalidated_on_stop += dropped;
        }
        emit(&self.shared, 0, 0, 0, "shutdown", "stop-signalled", 0);
        // Wake parked owners without holding the shared lock.
        if let Some(thread) = lock_shared(&self.shared).input_thread.clone() {
            thread.unpark();
        }
        self.inbox.wake_all();

        // Bounded wait for actual exits; 1 ms slices only bound join
        // latency, they do not gate correctness. Exit is observed on the
        // handles directly, never under the shared lock. The logger is
        // excluded here: it stops after the rest so teardown logs land.
        let deadline_at = start + deadline;
        loop {
            let done = {
                let guard = lock_shared(&self.shared);
                let engine_done = guard.engine_handle.as_ref().is_none_or(|h| h.is_finished());
                let input_done = guard.input_handle.as_ref().is_none_or(|h| h.is_finished());
                let apps_done = guard
                    .apps
                    .values()
                    .all(|r| r.worker.as_ref().is_some_and(|h| h.is_finished()));
                let zombies_done = guard.zombies.iter().all(|z| z.is_finished());
                engine_done && input_done && apps_done && zombies_done
            };
            if done || Instant::now() >= deadline_at {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        // Stop the logger now that teardown logs are queued; it drains under
        // the remaining deadline.
        lock_shared(&self.shared)
            .stop_logger
            .store(true, Ordering::SeqCst);
        let logger_grace = deadline_at.saturating_duration_since(Instant::now());
        let logger_wait = logger_grace.min(Duration::from_millis(200));
        let logger_until = Instant::now() + logger_wait;
        while Instant::now() < logger_until {
            let done = lock_shared(&self.shared)
                .logger_handle
                .as_ref()
                .is_none_or(|h| h.is_finished());
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        // Take every handle under the lock, then join only proven exits
        // without holding it. Preexisting zombies ride along so finished ones
        // join now and the rest keep counting toward the dirty outcome.
        let pending: Vec<LiveHandle> = take_live_handles(&mut lock_shared(&self.shared));
        let (joined, panics, unclean, retained) = join_exited(pending);
        {
            let mut guard = lock_shared(&self.shared);
            guard.zombies.extend(retained);
            if panics > 0 {
                guard.diagnostics.worker_panics_observed += panics as u64;
            }
            if unclean > 0 {
                guard.diagnostics.worker_unclean_observed += unclean as u64;
            }
            // Persistent honest evidence: previously reaped panics/unclean,
            // retained handles, and unexpected EOS all poison the outcome.
            let outcome = classify_shutdown(
                guard.diagnostics.worker_panics_observed,
                guard.zombies.len(),
                guard.diagnostics.worker_unclean_observed,
                guard.diagnostics.unexpected_eos,
            );
            let outcome_token: &'static str = match outcome {
                ShutdownOutcome::Clean => "clean",
                ShutdownOutcome::DirtyTimeout => "dirty-timeout",
                ShutdownOutcome::Unclean => "unclean",
                ShutdownOutcome::Panic => "panic",
                ShutdownOutcome::AlreadyStopped => "already-stopped",
            };
            let report = ShutdownReport {
                outcome,
                elapsed_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                joined,
                retained: guard.zombies.len(),
                panics: guard.diagnostics.worker_panics_observed as usize,
                unclean: guard.diagnostics.worker_unclean_observed as usize,
            };
            guard.terminal = Some(report.clone());
            // Best effort under the same lock: the logger may already be
            // gone; the ring entry above is the durable copy.
            guard.log(0, 0, 0, "shutdown", outcome_token, joined as u64);
            drop(guard);
            emit(&self.shared, 0, 0, 0, "shutdown", "reported", 0);
            report
        }
    }

    /// Reap handles retained after a dirty shutdown. Takes the retained set
    /// under the lock, joins only proven exits outside it; the rest stay
    /// retained and keep blocking overlapping replacement generations.
    /// Panic and unclean evidence persists in diagnostics, so a later
    /// shutdown still reports honestly.
    pub fn reap(&self) -> ReapReport {
        let zombies = std::mem::take(&mut lock_shared(&self.shared).zombies);
        let (reaped, panics, unclean, remaining) = join_exited(zombies);
        if panics > 0 {
            lock_shared(&self.shared).diagnostics.worker_panics_observed += panics as u64;
        }
        if unclean > 0 {
            lock_shared(&self.shared)
                .diagnostics
                .worker_unclean_observed += unclean as u64;
        }
        // Reap-side evidence is observable in the ring too.
        if panics > 0 || unclean > 0 {
            emit(
                &self.shared,
                0,
                0,
                0,
                "reap",
                if panics > 0 {
                    "panic-reaped"
                } else {
                    "unclean-reaped"
                },
                reaped as u64,
            );
        }
        let mut guard = lock_shared(&self.shared);
        guard.zombies.extend(remaining);
        let remaining_count = guard.zombies.len();
        ReapReport {
            reaped,
            panics,
            unclean,
            remaining: remaining_count,
        }
    }
}

/// Dropping the handle never leaves owners running forever: it signals
/// every stop flag — engine, input, logger, AND each app worker (whose stop
/// flag plus frame-slot stop wakes a worker parked in its multiplex wait)
/// — unparks the input thread, and wakes the inbox without blocking or
/// joining. A forgotten or failed-start handle therefore still tears down;
/// [`RuntimeHandle::shutdown`] remains the bounded, report-carrying path.
/// Never panics.
impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        let (input_thread, stop_logger) = match self.shared.lock() {
            Ok(guard) => {
                guard.stop_engine.store(true, Ordering::SeqCst);
                guard.stop_input.store(true, Ordering::SeqCst);
                // Signal every app worker and wake its frame slot: without
                // this a worker parked in `wait_versioned_timeout` (or in a
                // backend pump) would retain its `Shared` Arc forever after
                // the handle is gone. No join here — exit is asynchronous.
                for record in guard.apps.values() {
                    record.stop.store(true, Ordering::SeqCst);
                    record.frames.stop();
                    record.focus.stop();
                }
                (guard.input_thread.clone(), Some(guard.stop_logger.clone()))
            }
            Err(poison) => {
                let guard = poison.into_inner();
                guard.stop_engine.store(true, Ordering::SeqCst);
                guard.stop_input.store(true, Ordering::SeqCst);
                for record in guard.apps.values() {
                    record.stop.store(true, Ordering::SeqCst);
                    record.frames.stop();
                    record.focus.stop();
                }
                (guard.input_thread.clone(), Some(guard.stop_logger.clone()))
            }
        };
        if let Some(stop) = stop_logger {
            stop.store(true, Ordering::SeqCst);
        }
        if let Some(thread) = input_thread {
            thread.unpark();
        }
        self.inbox.wake_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_id_round_trip() {
        let id = window_id(1001, 77);
        assert_eq!(id, "1001_77");
        assert_eq!(parse_window_id(&id), Some((1001, 77)));
        assert_eq!(parse_window_id("nope"), None);
        assert_eq!(parse_window_id("1_2_3"), None);
        // The minted identity must satisfy the core opaque-ID alphabet
        // used by the focus/move validation gates.
        assert!(tiler_core::bounds::is_opaque_id(&id));
    }

    #[test]
    fn config_validation_fails_closed() {
        let mut cfg = RuntimeConfig::default();
        cfg.validate().unwrap();
        cfg.engine_queue_depth = 0;
        assert!(cfg.validate().is_err());
        cfg.engine_queue_depth = 8;
        cfg.focus_key = cfg.move_key;
        assert!(cfg.validate().is_err());
    }
}
