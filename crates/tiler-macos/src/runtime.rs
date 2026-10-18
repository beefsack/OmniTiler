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
use crate::runtime_lifecycle::{LiveHandle, classify_shutdown, join_exited};
pub use crate::runtime_lifecycle::{ReapReport, ShutdownOutcome, ShutdownReport};
use crate::runtime_log::{LogSink, StderrSink, sanitize_backend_reason, spawn_logger};
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot, Inbox, engine_main};
/// Stable pre-move path for the product select outcome: the handoff lane
/// lives in [`crate::select_feedback`]; this re-export keeps drain and
/// test call sites unchanged.
pub use crate::select_feedback::SelectDrainOutcome;
use crate::world_sequencer::ObservationClock;

pub use crate::focus_route::FocusRoute;
pub use crate::runtime_types::{
    AX_CALL_TIMEOUT, AxCompletion, AxObservation, ClaimedSelect, DesiredFrame, FocusCommand,
    FocusEvidence, FocusRequest, FrameCommand, InputDirection, InputSemanticAction, LifecycleEvent,
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
// The single shared cell plus its lock/clock/log helpers live in
// [`crate::runtime_shared`] (re-exported here so every existing
// `crate::runtime::{Shared, ...}` path keeps working); per-app lifetime
// admission lives in [`crate::runtime_apps`].
pub use crate::runtime_core::EngineFactsRead;
pub use crate::runtime_core::EngineManagedFacts;
pub use crate::runtime_shared::{
    MidflightHook, NativeContextCell, PostAcceptHook, PublishedDisplayInputs,
};
pub(crate) use crate::runtime_shared::{
    Shared, emit, engine_alive, invalidate_clock, lock_focus, lock_shared,
};

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
                clock: Arc::new(ObservationClock::new()),
                midflight_hook: Mutex::new(None),
                post_accept_hook: Mutex::new(None),
                topology: None,
                display_inputs: None,
                topology_revision: 0,
                native_context: None,
                select_pending: None,
                select_busy: false,
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
            clock: Arc::new(ObservationClock::new()),
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            topology_revision: 0,
            native_context: None,
            select_pending: None,
            select_busy: false,
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
#[derive(Clone)]
pub struct RuntimeHandle {
    pub(crate) shared: Arc<Mutex<Shared>>,
    pub(crate) inbox: Arc<Inbox>,
    pub(crate) focus_queue: Arc<Mutex<Vec<FocusRequest>>>,
    pub(crate) ax_factory: Arc<dyn AxBackendFactory>,
}

// -- Read-only retained workspace occupancy -------------------------------------

/// One engine-retained tile domain's managed membership: the core workspace
/// key plus the exactly-homed retained `(pid, wid)` members the engine
/// plans from (current and hidden domains alike). No titles, no bundles,
/// no paths — the caller maps representative PIDs to bundle IDs itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOccupancyDomain {
    /// Core workspace key (`space-{sid}` for user Spaces).
    pub workspace: String,
    /// True for the display's current user-Space domain (the single
    /// actuating domain per display); hidden domains reconcile silently.
    pub current: bool,
    /// Exactly-homed retained members, sorted `(pid, wid)`.
    pub members: Vec<(u32, u32)>,
}

/// Read-only retained occupancy snapshot answered by the engine owner over
/// the control lane (strict FIFO: the reply reflects every observation
/// submitted before the query). `context_present == false` (or a
/// `context_revision` that no longer matches the controller's live
/// snapshot revision) means completeness is unproven — the caller must
/// render every row `Unknown`, never `Empty`. Memberless known domains are
/// honestly empty only when complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOccupancyReport {
    pub context_present: bool,
    pub context_revision: u64,
    pub domains: Vec<WorkspaceOccupancyDomain>,
}

impl RuntimeHandle {
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
        // Bump before enqueue (see `invalidate_clock`): the engine may
        // dequeue the instant the job lands.
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(job) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
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
            let changed = guard.topology_revision != revision;
            guard.topology = Some(snapshot);
            guard.topology_revision = revision;
            guard.native_context = context.clone();
            guard.native_cell.store(context);
            let mut queue = lock_focus(&self.focus_queue);
            let before = queue.len();
            queue.retain(|request| request.topology_revision == revision);
            ((before - queue.len()) as u64, changed)
        };
        let (purged, changed) = purged;
        if changed {
            // New display/Space facts change future reports (domains,
            // current flags, homing): fence any in-flight reply so it
            // cannot actuate over superseded topology. Republishing an
            // identical revision moves nothing.
            //
            // Mark-without-job audit: this advances the clock with no
            // queued job. Safety comes from the emission gate instead: a
            // post-stamp update fails the watermark equality, and a
            // pre-stamp update after the dispatch snapshot fails the
            // topology-revision revalidation — either way no stale frame
            // reaches a native setter. The next dispatch re-snapshots the
            // fresh context and converges.
            invalidate_clock(&self.shared);
        }
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
            // Mark-without-job audit (see `update_topology_with`): the
            // engine never reads shared focus evidence at dispatch — each
            // owning worker stamps it into its snapshots — so there is no
            // incorporable truth to count here. The mark only fences
            // in-flight replies; the next snapshot-carrying job converges.
            invalidate_clock(&self.shared);
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
            guard.display_inputs = None;
            guard.native_context = None;
            guard.native_cell.store(None);
        }
        // Mark-without-job audit (see `update_topology_with`): clearing
        // the context fails the emission gate's revision revalidation for
        // any dispatch holding the retired snapshot, so no frame actuates
        // under it; the next dispatch refuses no-context instead.
        invalidate_clock(&self.shared);
        emit(&self.shared, 0, 0, 0, "topology", "invalidated", 0);
    }

    /// Current topology revision fence (`None` until the first snapshot).
    pub fn topology_revision(&self) -> Option<u64> {
        lock_shared(&self.shared)
            .topology
            .as_ref()
            .map(|t| t.revision)
    }

    /// Publish the AppKit display legs for the send effect owner from a
    /// main-thread snapshot. Call at startup, on screen-parameter
    /// notification, and to recover a cleared slot — never from a quiet
    /// poll with a populated slot, which must not move it. Brief lock;
    /// cleared by [`RuntimeHandle::invalidate_topology`].
    pub fn publish_display_inputs(&self, snapshot: &TopologySnapshot) {
        let inputs = PublishedDisplayInputs {
            displays: snapshot.displays.clone(),
            separate: snapshot.separate_spaces,
        };
        lock_shared(&self.shared).display_inputs = Some(inputs);
    }

    /// Latest-wins display legs (`None` until first publish or after
    /// invalidation — both refuse the send). Brief lock clone only.
    pub fn published_display_inputs(&self) -> Option<PublishedDisplayInputs> {
        lock_shared(&self.shared).display_inputs.clone()
    }

    /// True when the send display slot holds a publication. The tile
    /// controller checks this per good snapshot to republish after a
    /// cleared slot without waiting for the next screen notification.
    pub fn has_display_inputs(&self) -> bool {
        lock_shared(&self.shared).display_inputs.is_some()
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
                excluded_reasons: Vec::new(),
                revision: obs.revision,
            },
        };
        // Bump before enqueue (see `invalidate_clock`): the engine may
        // dequeue the instant the job lands.
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(job) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Submit one tap/keyboard select through the engine lane (no
    /// render-time binding: the drain maps the ordinal fresh). The engine
    /// applies the unarmed/busy/no-topology gates and the main-thread
    /// drain applies the fresh fences and verified arrival. Nonblocking
    /// (data lane); a full queue drops counted, never blocks input.
    pub fn submit_input_action(&self, action: InputSemanticAction) -> SubmitOutcome {
        if !engine_alive(&self.shared) {
            return SubmitOutcome::AdmissionClosed;
        }
        // Bump before enqueue (see `invalidate_clock`).
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(EngineJob::InputAction { action }) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Submit one menu-bar segment click through the engine lane: the
    /// clicked segment already resolved to an armed select action over its
    /// render-time row (see [`crate::workspace_indicator::resolve_click`]);
    /// the immutable render binding (`expected_sid`, `ui_generation`,
    /// `topology_revision`) rides through engine admission into the
    /// pending claim, and the main-thread drain refuses it against the
    /// fresh row instead of interpreting the ordinal against a different
    /// generation. Same unarmed/busy/no-topology gates as a tap select.
    /// Nonblocking (data lane); a full queue drops counted, never blocks
    /// AppKit.
    pub fn submit_indicator_select(
        &self,
        action: InputSemanticAction,
        expected_sid: u64,
        ui_generation: u64,
        topology_revision: u64,
    ) -> SubmitOutcome {
        if !engine_alive(&self.shared) {
            return SubmitOutcome::AdmissionClosed;
        }
        // Bump before enqueue (see `invalidate_clock`).
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(EngineJob::IndicatorSelect {
            action,
            expected_sid,
            ui_generation,
            topology_revision,
        }) {
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
        // Bump before enqueue (see `invalidate_clock`).
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(EngineJob::InputAction {
            action: InputSemanticAction::ToggleFloat,
        }) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Submit one Meta+left drop for the press-proven window at the final
    /// pointer snapshot. The worker homes the subject to its current
    /// domain, validates the press revision plus live membership, and
    /// commits through the current Engine; the returned plan geometry
    /// flows to the native backends through the existing desired-frame
    /// application. Nonblocking (data lane, latest-wins coalescing happens
    /// engine-side through re-plans).
    pub fn submit_drag_drop(&self, request: crate::drag_session::DragDropRequest) -> SubmitOutcome {
        if !engine_alive(&self.shared) {
            return SubmitOutcome::AdmissionClosed;
        }
        // Bump before enqueue (see `invalidate_clock`).
        invalidate_clock(&self.shared);
        if self.inbox.try_send_data(EngineJob::InputAction {
            action: InputSemanticAction::DragDrop {
                window: request.window,
                x: request.x,
                y: request.y,
                hover_prior: request.hover_prior.map(Box::new),
                press_revision: request.press_revision,
            },
        }) {
            SubmitOutcome::Accepted
        } else {
            lock_shared(&self.shared).diagnostics.data_dropped_full += 1;
            SubmitOutcome::EngineQueueFull
        }
    }

    /// Resolve one read-only drag preview against the current Engine.
    /// Control lane (never silently dropped); waits at most `timeout`
    /// for the worker reply. `None` means the engine is gone, the inbox
    /// is closed, or the bound elapsed — the caller cancels the gesture
    /// stale. Never blocks the tap (the tap never calls this).
    pub fn preview_drag(
        &self,
        request: &crate::drag_session::DragPreviewQuery,
        timeout: Duration,
    ) -> Option<crate::drag_drop::PreviewOutcome> {
        if !engine_alive(&self.shared) {
            return None;
        }
        let (tx, rx) = mpsc::channel::<crate::drag_drop::PreviewOutcome>();
        if !self.inbox.send_control(EngineJob::DragPreviewQuery {
            window: request.window.clone(),
            x: request.x,
            y: request.y,
            hover_prior: request.hover_prior.clone(),
            reply: tx,
        }) {
            return None;
        }
        rx.recv_timeout(timeout).ok()
    }

    /// Offline fault injection only: makes the engine thread panic on its
    /// next job, proving panic handling is honest. Never used by
    /// production paths.
    pub fn inject_engine_panic_for_test(&self) -> bool {
        self.inbox.send_control(EngineJob::TestPanic)
    }

    /// Deterministic-test rendezvous only (see [`MidflightHook`]): install
    /// or clear the once-per-flight engine hook. Never used by production
    /// paths; unset before shutdown so no dispatch can block on a test.
    pub fn set_midflight_hook_for_test(&self, hook: Option<MidflightHook>) {
        *lock_shared(&self.shared)
            .midflight_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = hook;
    }

    /// Deterministic-test rendezvous only (see [`PostAcceptHook`]): install
    /// or clear the once-per-accepted-reply engine hook. Never used by
    /// production paths; unset before shutdown so no dispatch can block on
    /// a test.
    pub fn set_post_accept_hook_for_test(&self, hook: Option<PostAcceptHook>) {
        *lock_shared(&self.shared)
            .post_accept_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = hook;
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

    /// Current engine-managed facts for one subject window (control lane,
    /// never silently dropped; waits at most `timeout` for the owner
    /// reply). `None` means the engine is gone, the inbox is closed, the
    /// bound elapsed, or the engine no longer manages the window — every
    /// one of those refuses without actuation on the effect owner. Never
    /// blocks input or observation owners (only the caller waits).
    pub fn query_managed_facts(
        &self,
        window: &str,
        timeout: std::time::Duration,
    ) -> Option<crate::runtime_core::EngineManagedFacts> {
        match self.query_managed_facts_read(window, timeout) {
            crate::runtime_core::EngineFactsRead::Managed(facts) => Some(facts),
            crate::runtime_core::EngineFactsRead::Unmanaged
            | crate::runtime_core::EngineFactsRead::NoReply => None,
        }
    }

    /// Discriminated engine-facts read for the dedicated effect owner:
    /// `Managed` carries fresh engine authority; `Unmanaged` is a proven
    /// engine reply that the subject is no longer managed; `NoReply` is an
    /// unproven read (engine down, inbox closed, or the bound elapsed).
    /// Both non-managed variants refuse without actuation — never a guess,
    /// never an admission-pin substitute. Never blocks input or observation
    /// owners (only the caller waits).
    ///
    /// Each query emits one bounded metadata-only line on the caller
    /// (effect owner): window identity plus the discriminated outcome plus
    /// elapsed milliseconds. The engine owner emits the matching
    /// receive/reply pair (see `ManagedFactsQuery` handling), so a live
    /// `noreply-timeout` with no engine `received` for the same window
    /// suggests queue blocking ahead of the query, while `received` without
    /// a timely delivered reply suggests engine-side stall — never inferred
    /// from the refusal token alone. Same-window queries are serialized by
    /// the single effect owner, so window identity plus log order locates
    /// the pair; stderr/ring timestamps order it only approximately.
    /// Logging only; never a gate, timeout, or retry change.
    pub fn query_managed_facts_read(
        &self,
        window: &str,
        timeout: std::time::Duration,
    ) -> crate::runtime_core::EngineFactsRead {
        use crate::runtime_shared::engine_alive;
        let start = std::time::Instant::now();
        if !engine_alive(&self.shared) {
            eprintln!(
                "{}",
                Self::format_engine_facts_query_line(
                    window,
                    "noreply-engine-down",
                    start.elapsed()
                )
            );
            return crate::runtime_core::EngineFactsRead::NoReply;
        }
        let (tx, rx) = mpsc::channel::<Option<crate::runtime_core::EngineManagedFacts>>();
        if !self.inbox.send_control(EngineJob::ManagedFactsQuery {
            window: window.to_owned(),
            reply: tx,
        }) {
            eprintln!(
                "{}",
                Self::format_engine_facts_query_line(
                    window,
                    "noreply-inbox-closed",
                    start.elapsed()
                )
            );
            return crate::runtime_core::EngineFactsRead::NoReply;
        }
        let (outcome, token) = match rx.recv_timeout(timeout) {
            Ok(Some(facts)) => (
                crate::runtime_core::EngineFactsRead::Managed(facts),
                "managed",
            ),
            Ok(None) => (crate::runtime_core::EngineFactsRead::Unmanaged, "unmanaged"),
            Err(err) => (
                crate::runtime_core::EngineFactsRead::NoReply,
                Self::engine_facts_noreply_token(&err),
            ),
        };
        eprintln!(
            "{}",
            Self::format_engine_facts_query_line(window, token, start.elapsed())
        );
        outcome
    }

    /// Discriminate a failed engine-facts wait: a bound elapse stays
    /// `noreply-timeout`, while a dropped reply sender (engine received then
    /// exited without replying) reports `noreply-disconnected`. Pure mapping;
    /// the read stays `NoReply` either way.
    pub(crate) fn engine_facts_noreply_token(
        err: &std::sync::mpsc::RecvTimeoutError,
    ) -> &'static str {
        match err {
            std::sync::mpsc::RecvTimeoutError::Timeout => "noreply-timeout",
            std::sync::mpsc::RecvTimeoutError::Disconnected => "noreply-disconnected",
        }
    }

    /// One bounded metadata-only engine-facts query line for the dedicated
    /// effect owner: window identity plus the discriminated outcome plus
    /// elapsed milliseconds. Pure (the query above is the only emitter).
    /// Window is the `"<pid>_<wid>"` identity — never titles, paths, or
    /// posted values. Outcome is one fixed token from the query above.
    pub(crate) fn format_engine_facts_query_line(
        window: &str,
        outcome: &str,
        elapsed: std::time::Duration,
    ) -> String {
        let flat: String = window
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let collapsed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
        let safe = if collapsed.is_empty() {
            "unspecified".to_owned()
        } else if collapsed.chars().count() > 32 {
            collapsed.chars().take(32).collect()
        } else {
            collapsed
        };
        format!(
            "tiler-macos send engine-facts query window={safe} outcome={outcome} elapsed_ms={}",
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        )
    }

    /// Query the engine's retained workspace occupancy (control lane, never
    /// silently dropped; waits at most `timeout` for the owner reply).
    /// `None` means the engine is gone, the inbox is closed, or the bound
    /// elapsed — the caller renders `Unknown`, never a stale report. Never
    /// blocks input or observation owners (only the caller waits).
    pub fn query_workspace_occupancy(
        &self,
        timeout: std::time::Duration,
    ) -> Option<WorkspaceOccupancyReport> {
        if !engine_alive(&self.shared) {
            return None;
        }
        let (tx, rx) = mpsc::channel::<WorkspaceOccupancyReport>();
        if !self
            .inbox
            .send_control(EngineJob::WorkspaceOccupancyQuery { reply: tx })
        {
            return None;
        }
        rx.recv_timeout(timeout).ok()
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

    #[test]
    fn engine_facts_query_line_names_window_outcome_and_elapsed() {
        // Correlated requester line for the inverse `engine-facts-noreply`:
        // window identity plus the discriminated outcome plus elapsed ms.
        // Token-only (never titles, paths, or posted values).
        for outcome in [
            "managed",
            "unmanaged",
            "noreply-engine-down",
            "noreply-inbox-closed",
            "noreply-timeout",
            "noreply-disconnected",
        ] {
            let line = RuntimeHandle::format_engine_facts_query_line(
                "4242_7",
                outcome,
                std::time::Duration::from_millis(12),
            );
            assert!(
                line.contains("tiler-macos send engine-facts query"),
                "{line}"
            );
            assert!(line.contains("window=4242_7"), "{line}");
            assert!(line.contains(&format!("outcome={outcome}")), "{line}");
            assert!(line.contains("elapsed_ms=12"), "{line}");
        }
        // Control chars collapse; empty withholds instead of guessing.
        let line = RuntimeHandle::format_engine_facts_query_line(
            "4242_7\ninjected",
            "noreply-timeout",
            std::time::Duration::from_millis(0),
        );
        assert!(!line.contains('\n'), "{line}");
        assert!(line.contains("window=4242_7 injected"), "{line}");
        let line = RuntimeHandle::format_engine_facts_query_line(
            "",
            "managed",
            std::time::Duration::from_millis(1),
        );
        assert!(line.contains("window=unspecified"), "{line}");
    }

    #[test]
    fn engine_facts_noreply_token_discriminates_timeout_from_disconnect() {
        // Transport semantic: an empty wait times out; a dropped sender
        // disconnects. The query maps each to its own requester token while
        // both read `NoReply`.
        let (_held, empty) =
            std::sync::mpsc::channel::<Option<crate::runtime_core::EngineManagedFacts>>();
        let err = empty
            .recv_timeout(std::time::Duration::from_millis(0))
            .expect_err("empty wait must time out");
        assert_eq!(
            RuntimeHandle::engine_facts_noreply_token(&err),
            "noreply-timeout"
        );
        let (tx, dropped) =
            std::sync::mpsc::channel::<Option<crate::runtime_core::EngineManagedFacts>>();
        drop(tx);
        let err = dropped
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect_err("dropped sender must disconnect");
        assert_eq!(
            RuntimeHandle::engine_facts_noreply_token(&err),
            "noreply-disconnected"
        );
    }
}
