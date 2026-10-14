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

use tiler_core::{
    boundary::{CoreCommand, CoreReply},
    directional::{SameAxisMove, WindowId},
    engine::Engine,
    geometry::Rect,
    window_policy::geometry_write_allowed,
};

use crate::input::cg_flag_bits;
use crate::input_match::{Chord, TapKeyKind, TapTuning, decide_tap_event};
use crate::runtime::{
    AxCompletion, DesiredFrame, FocusRequest, FrameCommand, InputDirection, InputSemanticAction,
    ObservedWindow, Shared, emit, lock_focus, lock_shared, parse_window_id, window_id,
};
use crate::runtime_backend::{AxBackendFactory, InputSourceFactory};
use crate::runtime_lifecycle::{EngineExit, InputExit, WorkerExit};
use crate::runtime_log::sanitize_backend_reason;
use crate::runtime_observe::{BackendSnapshot, PendingObservation, merge_readback, stage_latest};

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
    InputAction {
        action: InputSemanticAction,
    },
    AxResult {
        pid: u32,
        generation: u64,
        cmd_id: u64,
        completion: AxCompletion,
    },
    /// Rendezvous for deterministic tests: processed in FIFO order, so a
    /// completed barrier proves every earlier job was processed.
    Barrier(mpsc::Sender<()>),
    /// Offline fault injection only: makes the engine thread panic to prove
    /// panic handling. Never sent by production paths.
    TestPanic,
}

impl EngineJob {
    /// Data jobs share the bounded lane; control jobs (lifecycle, barriers,
    /// fault injection) bypass saturation but keep FIFO order with data.
    fn is_data(&self) -> bool {
        matches!(
            self,
            Self::Observation { .. } | Self::InputAction { .. } | Self::AxResult { .. }
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
}

/// Latest-wins frame slot for one app worker. `set_latest` overwrites the
/// whole pending desired set so rapid re-plans coalesce but never lose the
/// final desired state — including a whole EMPTY desired set, which supersedes
/// pending state when windows are removed/closed. Every overwrite bumps a
/// revision token so a worker holding a taken batch can drop commands the
/// newer desired state already superseded. Stopping drops the pending set
/// (queued work is invalidated on terminate).
pub(crate) struct FrameSlot {
    state: Mutex<FrameSlotState>,
    wake: Condvar,
}

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
    fn set_latest(&self, cmds: Vec<FrameCommand>) -> Option<usize> {
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
    fn current_rev(&self) -> u64 {
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
    fn wait_versioned_timeout(
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

pub(crate) use crate::runtime_core::{
    RetainedApp, RetainedCore, build_event, clear_focus_into, skeleton_key,
};

// -- Engine event wire ---------------------------------------------------------

/// Build one core event with wire normalization over core-session lifetime:
/// the session query and the fixed predicate travel with the retained state
/// so [`build_event`] can hold born arrivals and assert first-sight fixed
/// origins without adapter lifetime maps.
fn wire_event(
    engine: &Engine,
    retained: &mut RetainedCore,
    focused_override: Option<WindowId>,
    closed: Vec<WindowId>,
    command: CoreCommand,
) -> tiler_core::boundary::CoreEvent {
    let session = engine.session(&skeleton_key());
    let predicate = engine.fixed_size_predicate();
    build_event(
        retained,
        focused_override,
        closed,
        command,
        session,
        predicate,
    )
}

// -- Thread bodies -------------------------------------------------------------

/// Engine owner: the sole owner of the [`Engine`] and retained observation
/// state. Never waits for AX; frame emission is a non-blocking slot
/// overwrite and focus emission is a bounded `try_send`.
pub(crate) fn engine_main(
    shared: Arc<Mutex<Shared>>,
    inbox: Arc<Inbox>,
    focus_queue: Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<()>,
) -> EngineExit {
    let mut engine = Engine::new();
    let mut retained = RetainedCore::default();
    // The macOS skeleton opts into the shared fixed-size admission policy
    // (D1-D8): hintless traffic is unaffected, while fixed-hint arrivals
    // classify admission-side through the configured predicate. Lifetimes
    // (born hold, first exit, restore) stay core-owned; this only selects
    // the shared policy the normalization feeds.
    {
        let guard = lock_shared(&shared);
        engine.set_fixed_size_admission(true);
        engine.set_fixed_size_predicate(guard.config.fixed_size_predicate);
    }
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
            &mut engine,
            &mut retained,
            &shared,
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

fn process_job(
    job: EngineJob,
    engine: &mut Engine,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
) {
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
                    clear_focus_into(retained, pid);
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
            clear_focus_into(retained, pid);
            let closed: Vec<WindowId> = removed
                .map(|app| {
                    app.windows
                        .iter()
                        .map(|w| WindowId(window_id(pid, w.wid)))
                        .collect()
                })
                .unwrap_or_default();
            emit(
                shared,
                pid,
                generation,
                0,
                "terminate",
                "retained-removed",
                closed.len() as u64,
            );
            // Reconcile the survivors so unaffected apps stay operational;
            // the closed windows ride as explicit closes (never minimize
            // slots for a dead app).
            let event = wire_event(engine, retained, None, closed, CoreCommand::Reconcile);
            let reply = engine.handle(&event);
            apply_reply(
                reply,
                "reconcile",
                retained,
                shared,
                focus_queue,
                focus_capacity,
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
            match observed_target {
                Some(window) => retained.observed_focus = Some(window),
                None => clear_focus_into(retained, pid),
            }
            emit(
                shared,
                pid,
                generation,
                seq,
                "observation",
                "applied",
                window_count,
            );
            let event = wire_event(engine, retained, None, Vec::new(), CoreCommand::Reconcile);
            let reply = engine.handle(&event);
            apply_reply(
                reply,
                "reconcile",
                retained,
                shared,
                focus_queue,
                focus_capacity,
                pid,
                generation,
                seq,
            );
        }
        EngineJob::InputAction { action } => {
            // The subject is observed truth, never planned focus.
            let focused = retained.observed_focus.clone();
            let Some(window) = focused else {
                lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                emit(shared, 0, 0, 0, "input", "refused-no-focus", 0);
                return;
            };
            let (command, op) = match action {
                InputSemanticAction::FocusStep { direction } => (
                    CoreCommand::Focus {
                        window: window.0.clone(),
                        direction: direction.as_str().to_owned(),
                        cross_output_transfer: false,
                        float_subject: false,
                    },
                    "focus",
                ),
                InputSemanticAction::MoveStep { direction } => (
                    CoreCommand::Move {
                        window: window.0.clone(),
                        direction: direction.as_str().to_owned(),
                        cross_output_transfer: false,
                        same_axis_move: SameAxisMove::default(),
                    },
                    "move",
                ),
            };
            lock_shared(shared).diagnostics.input_forwarded += 1;
            emit(shared, 0, 0, 0, "input", op, 0);
            let event = wire_event(engine, retained, Some(window), Vec::new(), command);
            let reply = engine.handle(&event);
            apply_reply(
                reply,
                op,
                retained,
                shared,
                focus_queue,
                focus_capacity,
                0,
                0,
                0,
            );
        }
        EngineJob::AxResult {
            pid,
            generation,
            cmd_id,
            completion,
        } => {
            // Preserve dispatch detail without leaking strings: Applied and
            // Timeout carry elapsed millis in the count; Failed maps its
            // reason to a fixed sanitized token.
            let (token, count): (&'static str, u64) = match &completion {
                AxCompletion::Applied { elapsed_ms } => ("applied", *elapsed_ms),
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
                // exclusively through `AxObservation`.
                emit(shared, pid, generation, cmd_id, "ax-result", token, count);
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
                        "ax-result",
                        "terminated-invalidated",
                        0,
                    );
                } else {
                    lock_shared(shared).diagnostics.stale_dropped += 1;
                    emit(
                        shared,
                        pid,
                        generation,
                        cmd_id,
                        "ax-result",
                        "stale-rejected",
                        0,
                    );
                }
            }
        }
        EngineJob::Barrier(tx) => {
            let _ = tx.send(());
        }
        EngineJob::TestPanic => {
            panic!("phase-1 offline fault injection: engine panic");
        }
    }
}

fn reply_kind(reply: &CoreReply) -> &'static str {
    match reply {
        CoreReply::Projection(_) => "projection",
        CoreReply::Tiled(_) => "tiled",
        CoreReply::SendWorkspace(_) => "send-workspace",
        CoreReply::SendOutput(_) => "send-output",
        CoreReply::MigrateWorkspace(_) => "migrate-workspace",
        CoreReply::MoveDirectional(_) => "move-directional",
        CoreReply::FocusDirectional(_) => "focus-directional",
        CoreReply::Resize(_) => "resize",
        CoreReply::ActiveGroup(_) => "active-group",
        CoreReply::NoGroup { .. } => "no-group",
        CoreReply::DragPreview(_) => "drag-preview",
        CoreReply::Released => "released",
        CoreReply::Rejected { .. } => "rejected",
        CoreReply::SnapshotInvalid { .. } => "snapshot-invalid",
        CoreReply::Diverged(_) => "diverged",
    }
}

/// Whether frame emission for `window` is withheld: the retained observation
/// still carries a native overlay flag for it. The reserved slot/allocation
/// stays plan truth; only actuation is filtered, through the shared
/// [`geometry_write_allowed`] gate. The skeleton performs no maximized
/// inference, so only fullscreen can hold here.
fn overlay_holds_frame(retained: &RetainedCore, window: &str) -> bool {
    let Some((pid, wid)) = parse_window_id(window) else {
        return false;
    };
    retained
        .apps
        .get(&pid)
        .and_then(|app| app.windows.iter().find(|w| w.wid == wid))
        .is_some_and(|w| !geometry_write_allowed(w.fullscreen, false))
}

/// Translate one core reply into frame commands and focus requests.
/// Frame emission overwrites per-app latest-wins slots without waiting;
/// focus emission is a bounded outbox `try_send` with a counted drop.
#[allow(clippy::too_many_arguments)]
fn apply_reply(
    reply: CoreReply,
    op: &'static str,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let token: &'static str = reply_kind(&reply);
    // Geometry plus revision carried by success replies.
    let mut geometry: Vec<(String, Rect)> = Vec::new();
    let mut revision: Option<u64> = None;
    let mut focus_to: Option<WindowId> = None;
    match &reply {
        CoreReply::Projection(plan) => {
            revision = Some(plan.base_revision);
            geometry = plan
                .geometry
                .iter()
                .filter(|g| !g.client_clamped)
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
        }
        CoreReply::Tiled(plan) => {
            revision = Some(plan.base_revision);
            geometry = plan
                .geometry
                .iter()
                .filter(|g| !g.client_clamped)
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
            // Fresh-admission plans carry the focused leaf; resolve it to
            // the authoritative window through the plan geometry.
            if let Some(leaf) = plan.focus_leaf.clone() {
                focus_to = plan
                    .geometry
                    .iter()
                    .find(|g| g.leaf == leaf)
                    .map(|g| g.window.clone());
            }
        }
        CoreReply::MoveDirectional(plan) => {
            revision = Some(plan.base_revision);
            geometry = plan
                .geometry
                .iter()
                .filter(|g| !g.client_clamped)
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
            focus_to = plan
                .geometry
                .iter()
                .find(|g| g.leaf == plan.focus_leaf)
                .map(|g| g.window.clone());
        }
        CoreReply::FocusDirectional(plan) => {
            revision = Some(plan.base_revision);
            geometry = plan
                .geometry
                .iter()
                .filter(|g| !g.client_clamped)
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
            focus_to = Some(plan.to_window.clone());
        }
        _ => {}
    }
    if let Some(base) = revision {
        retained.last_revision = base;
    }
    // Snapshot desired geometry for tests and diagnostics. Built from the
    // FULL plan geometry: overlaid members stay plan truth even while
    // emission below withholds their writes.
    let desired: Vec<DesiredFrame> = geometry
        .iter()
        .map(|(window, rect)| DesiredFrame {
            window: window.clone(),
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        })
        .collect();
    // Overlay emission filter: retained fullscreen members keep their
    // reserved slot/allocation in plan truth but take no geometry write
    // while overlaid. Withheld frames are counted, never dropped silently;
    // only actuation is filtered here, never plan truth above.
    let mut overlay_skipped = 0u64;
    let geometry: Vec<(String, Rect)> = geometry
        .into_iter()
        .filter(|(window, _)| {
            if overlay_holds_frame(retained, window) {
                overlay_skipped += 1;
                false
            } else {
                true
            }
        })
        .collect();
    // Planned focus never becomes observed truth here: `focus_to` travels
    // only into the focus request below, and the input subject stays on
    // `observed_focus` until an observation confirms arrival. A dropped
    // request therefore never advances focus.
    // Collect live per-app slots without holding the lock during emission.
    let slots: Vec<(u32, u64, Arc<FrameSlot>)> = {
        let guard = lock_shared(shared);
        guard
            .apps
            .iter()
            .filter(|(_, r)| !r.terminated)
            .map(|(pid, r)| (*pid, r.generation, Arc::clone(&r.frames)))
            .collect()
    };
    // Group the plan by app so each worker receives its whole desired
    // set in one latest-wins handoff.
    let mut by_app: BTreeMap<u32, Vec<FrameCommand>> = BTreeMap::new();
    let mut unparseable = 0u64;
    for (window, rect) in &geometry {
        let Some((wpid, wid)) = parse_window_id(window) else {
            unparseable += 1;
            continue;
        };
        let generation = slots
            .iter()
            .find(|(p, _, _)| *p == wpid)
            .map(|(_, generation, _)| *generation)
            .unwrap_or(0);
        // Per-app command sequence comes from retained state.
        let cmd_id = retained
            .apps
            .get_mut(&wpid)
            .map(|app| {
                app.cmd_counter += 1;
                app.cmd_counter
            })
            .unwrap_or(0);
        by_app.entry(wpid).or_default().push(FrameCommand {
            pid: wpid,
            generation,
            cmd_id,
            window: window.clone(),
            wid,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        });
    }
    let mut emitted = 0u64;
    let mut coalesced = 0u64;
    let mut dead = unparseable;
    for (wpid, cmds) in &by_app {
        let Some((_, _, slot)) = slots.iter().find(|(p, _, _)| *p == *wpid) else {
            dead += cmds.len() as u64;
            continue;
        };
        match slot.set_latest(cmds.clone()) {
            Some(overwritten) => {
                emitted += cmds.len() as u64;
                coalesced += overwritten as u64;
            }
            None => dead += cmds.len() as u64,
        }
    }
    // A whole empty desired set supersedes pending state: live apps with no
    // windows in this plan converge to empty instead of actuating a stale
    // pending set. Refusals carry no plan (`revision` is `None`) and emit
    // nothing.
    if revision.is_some() {
        for (wpid, _, slot) in &slots {
            if !by_app.contains_key(wpid)
                && let Some(overwritten) = slot.set_latest(Vec::new())
            {
                coalesced += overwritten as u64;
            }
        }
    }
    // Focus requests out: only explicit focus operations dispatch native
    // focus. Reconcile/admit/move re-plan geometry (and track focus
    // internally) without moving native focus.
    let mut focus_dispatched = 0u64;
    let mut focus_dropped = 0u64;
    if matches!(reply, CoreReply::FocusDirectional(_))
        && let Some(to_window) = focus_to
        && let Some((fpid, fwid)) = parse_window_id(&to_window.0)
    {
        let generation = slots
            .iter()
            .find(|(p, _, _)| *p == fpid)
            .map(|(_, generation, _)| *generation)
            .unwrap_or(0);
        let request_id = {
            retained.focus_seq += 1;
            retained.focus_seq
        };
        let request = FocusRequest {
            pid: fpid,
            generation,
            request_id,
            wid: fwid,
            window: to_window.0.clone(),
            dispatched: true,
            verified: false,
        };
        let mut queue = lock_focus(focus_queue);
        if queue.len() < focus_capacity {
            queue.push(request);
            focus_dispatched = 1;
        } else {
            focus_dropped = 1;
        }
    }
    {
        let mut guard = lock_shared(shared);
        guard.count_reply(&format!("{op}-{token}"));
        guard.diagnostics.frames_emitted += emitted;
        guard.diagnostics.frames_coalesced += coalesced;
        guard.diagnostics.frames_overlay_skipped += overlay_skipped;
        guard.diagnostics.terminated_invalidated += dead;
        guard.diagnostics.focus_dispatched += focus_dispatched;
        guard.diagnostics.focus_dropped_full += focus_dropped;
        // Snapshot the latest plan, including empty plans (e.g. every
        // window closed is honest truth, never stale). Refusals carry no
        // plan and must not clobber the last one.
        if revision.is_some() {
            guard.last_desired = desired;
        }
    }
    // Retire note: when the core retires the skeleton domain (empty plan
    // with the session removed), the next fresh seed adopts the carried
    // revision (verified against the core: fresh sessions start clean
    // regardless of the cursor), so no cursor reset is needed here. Held
    // and minimize-slot sessions persist with their cursor untouched.
    emit(shared, pid, generation, seq, "core-reply", token, emitted);
    if emitted > 0 {
        emit(shared, pid, generation, seq, "frame", "emitted", emitted);
    }
    if focus_dropped > 0 {
        emit(shared, pid, generation, seq, "focus", "dropped-full", 1);
    }
}

/// Bound for one worker multiplex slice: frame waits and backend
/// observation pumps share this cadence. Command wake stays immediate
/// (slot `Condvar`); autonomous fake observations wake immediately (channel
/// `recv_timeout`); native notifications interrupt the runloop pump. The
/// bound only caps quiet-slice latency — correctness never depends on it.
const OBSERVE_SLICE: Duration = Duration::from_millis(20);

/// Publish one worker-stamped observation. Sequence and generation are
/// stamped here, never by the backend. Returns false under backpressure
/// (caller retains the latest snapshot and retries — the final state is
/// never silently lost).
fn send_observation(
    inbox: &Arc<Inbox>,
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    seq: u64,
    snapshot: &BackendSnapshot,
) -> bool {
    let job = EngineJob::Observation {
        pid,
        generation,
        seq,
        windows: snapshot.windows.clone(),
        focused_wid: snapshot.focused_wid,
    };
    if inbox.try_send_data(job) {
        lock_shared(shared).diagnostics.observations_published += 1;
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "published",
            snapshot.windows.len() as u64,
        );
        true
    } else {
        false
    }
}

fn note_observation_failure(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    seq: u64,
    reason: &str,
) {
    // Failure preserves retained engine state: nothing is published, the
    // last good observation stands, and the failure is counted. The outcome
    // is the sanitized reason token (allowlisted OS names only), so the why
    // is preserved without leaking arbitrary backend strings; an error is
    // never an empty observation.
    let token = sanitize_backend_reason(reason);
    lock_shared(shared).diagnostics.observation_failures += 1;
    emit(shared, pid, generation, seq, "observation", token, 0);
}

/// Per-app AX worker: creates the backend inside this owning thread,
/// reports its own `Launch` to the engine once the backend exists (so backend
/// readiness can never stall the spawner), publishes a startup observation,
/// then multiplexes autonomous backend observations, latest-wins frame
/// commands, and post-apply readbacks. Never touches the core. A panic
/// here is supervision evidence, never a clean outcome.
///
/// Observation rules: unchanged snapshots are skipped (no busy feedback when
/// native clamps or refuses); failures preserve retained state; a saturated
/// inbox retains the latest snapshot for retry instead of dropping the final
/// state. Readbacks merge the last known focus so a focus-blind frame read
/// never clears observed focus truth.
///
/// Command fence: NO command from a taken batch starts once stop is set or
/// a newer desired set supersedes the batch — including the first command.
/// Calls already inside the backend may finish (taken is not inflight), but
/// nothing taken-but-unstarted begins after the fence.
#[allow(clippy::too_many_arguments)]
pub(crate) fn app_worker_main(
    pid: u32,
    generation: u64,
    factory: Arc<dyn AxBackendFactory>,
    frames: Arc<FrameSlot>,
    inbox: Arc<Inbox>,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    ready: Arc<Mutex<Option<Result<(), String>>>>,
) -> WorkerExit {
    let mut backend = match factory.create(pid) {
        Ok(backend) => backend,
        Err(reason) => {
            *ready.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(Err(reason));
            return WorkerExit::Clean;
        }
    };
    // Backend exists: queue admission before marking ready, so `Ready`
    // implies the engine `Launch` is already in the control lane. Dropped on
    // a stopped runtime (the entry is gone; the engine rejects it as stale).
    let _ = inbox.send_control(EngineJob::Launch { pid, generation });
    *ready.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(Ok(()));
    // A stop that raced creation must not begin AX work: readiness was
    // already reported, so fall through to honest teardown instead.
    if stop.load(Ordering::SeqCst) || frames.is_stopped() {
        return teardown_worker(&mut backend, &shared, pid, generation, 0);
    }
    let mut seq: u64 = 0;
    let mut last: Option<BackendSnapshot> = None;
    let mut pending = PendingObservation::new();
    let mut focus_hint: Option<u32> = None;
    // Startup observation: the first truth the engine ever sees for this
    // lifetime. A failure publishes nothing (retained state starts empty
    // and stays empty until a good read); it never publishes an empty list
    // in place of the failure.
    match backend.snapshot() {
        Ok(windows) => {
            let snapshot = BackendSnapshot {
                windows,
                focused_wid: None,
            };
            seq += 1;
            if send_observation(&inbox, &shared, pid, generation, seq, &snapshot) {
                last = Some(snapshot);
            } else {
                pending.set(snapshot);
            }
        }
        Err(reason) => note_observation_failure(&shared, pid, generation, seq + 1, &reason),
    }
    loop {
        if stop.load(Ordering::SeqCst) || frames.is_stopped() {
            break;
        }
        // Autonomous backend observation (bounded; `None` covers both "no
        // autonomous source" and "nothing new since the last pump").
        if let Some(result) = backend.poll_observation(&stop) {
            match result {
                Ok(snapshot) => stage_latest(&last, &mut pending, snapshot),
                Err(reason) => {
                    note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                }
            }
        }
        if stop.load(Ordering::SeqCst) || frames.is_stopped() {
            break;
        }
        // Flush the latest retained observation (backpressure: retained and
        // retried, never silently lost).
        if let Some(snapshot) = pending.take() {
            seq += 1;
            if send_observation(&inbox, &shared, pid, generation, seq, &snapshot) {
                focus_hint = snapshot.focused_wid;
                last = Some(snapshot);
            } else {
                pending.set(snapshot);
            }
        }
        let Some((taken_rev, cmds)) = frames.wait_versioned_timeout(&stop, OBSERVE_SLICE) else {
            continue;
        };
        // An empty taken batch is still a cancellation: a whole empty
        // desired set (window removal) supersedes pending frame state, and
        // the readback below confirms the removal. It must flow through,
        // never short-circuit.
        for (index, cmd) in cmds.iter().enumerate() {
            // Fence every command including the first: a taken batch is not
            // inflight work, so nothing in it may start after stop or after
            // a superseding desired set. Calls already inside the backend
            // finish on their own bound.
            if stop.load(Ordering::SeqCst) {
                let skipped = (cmds.len() - index) as u64;
                lock_shared(&shared).diagnostics.terminated_invalidated += skipped;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "frame",
                    "stop-fenced",
                    skipped,
                );
                break;
            }
            if frames.current_rev() != taken_rev {
                let skipped = (cmds.len() - index) as u64;
                lock_shared(&shared).diagnostics.frames_coalesced += skipped;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "frame",
                    "superseded",
                    skipped,
                );
                break;
            }
            if cmd.generation != generation || cmd.pid != pid {
                lock_shared(&shared).diagnostics.stale_dropped += 1;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "frame",
                    "stale-rejected",
                    0,
                );
                continue;
            }
            // Redundant once observers report back: desired geometry already
            // matching the last observed geometry needs no native write.
            // Identity is wid plus full rect; anything else still actuates.
            if last
                .as_ref()
                .and_then(|s| {
                    s.windows.iter().find(|w| {
                        w.wid == cmd.wid
                            && w.x == cmd.x
                            && w.y == cmd.y
                            && w.w == cmd.w
                            && w.h == cmd.h
                    })
                })
                .is_some()
            {
                lock_shared(&shared).diagnostics.frames_redundant_skipped += 1;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "frame",
                    "redundant-skipped",
                    0,
                );
                continue;
            }
            // The backend bounds each call with its own finite messaging
            // timeout; completion is honest dispatch truth only. Frame
            // dispatch is logged worker-side (the engine logs the async
            // completion separately), so the normal log reconstructs
            // core -> frame -> completion.
            emit(
                &shared,
                pid,
                generation,
                cmd.cmd_id,
                "frame",
                "dispatched",
                0,
            );
            let completion = backend.apply_frame(cmd);
            let job = EngineJob::AxResult {
                pid,
                generation,
                cmd_id: cmd.cmd_id,
                completion,
            };
            if !inbox.try_send_data(job) {
                lock_shared(&shared).diagnostics.result_dropped_full += 1;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "ax-result",
                    "queue-full-dropped",
                    0,
                );
            }
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // Post-apply readback: what the desktop actually shows after this
        // batch. Unchanged readbacks are skipped (a native clamp is not
        // replayed endlessly); failures preserve retained state.
        match backend.snapshot() {
            Ok(windows) => {
                let snapshot = merge_readback(windows, focus_hint);
                stage_latest(&last, &mut pending, snapshot);
            }
            Err(reason) => note_observation_failure(&shared, pid, generation, seq + 1, &reason),
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
    }
    teardown_worker(&mut backend, &shared, pid, generation, seq)
}

/// Honest teardown: the outcome is logged and returned, never a silent
/// clean. (A panic inside still propagates and the shutdown report stays
/// dirty via join accounting; a returned `Unclean` is distinct join
/// evidence that also poisons the terminal shutdown.) Teardown accounting
/// lives on the join side only: this records the live error log plus the
/// operational failure count, never `worker_unclean_observed` (the join
/// adds that exactly once), so the numerical report stays trustworthy.
fn teardown_worker(
    backend: &mut Box<dyn crate::runtime_backend::AxBackend>,
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    seq: u64,
) -> WorkerExit {
    match backend.shutdown() {
        Ok(()) => {
            emit(shared, pid, generation, seq, "observer", "torn-down", 0);
            WorkerExit::Clean
        }
        Err(_) => {
            lock_shared(shared).diagnostics.observation_failures += 1;
            emit(
                shared,
                pid,
                generation,
                seq,
                "observer",
                "teardown-failed",
                0,
            );
            WorkerExit::Unclean {
                reason: "teardown-failed",
            }
        }
    }
}

/// Exact-match tuning for one configured chord: the existing
/// [`TapTuning`] mask semantics, so extra modifiers fail the match.
fn tuning_for(modifier: crate::config::ProbeModifier, key_code: u32) -> TapTuning {
    let (want_flags, flags_mask) = cg_flag_bits(modifier);
    TapTuning {
        want_flags,
        flags_mask,
        want_code: i64::from(key_code),
    }
}

/// Dedicated input owner: creates, drives, and destroys its input source
/// inside this thread. Exact chord matches on key down only are forwarded
/// as semantic actions; this thread never touches the core. An unexpected
/// source EOS (`None` without stop) is counted, logged, and exits — never
/// spun on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn input_main(
    shared: Arc<Mutex<Shared>>,
    inbox: Arc<Inbox>,
    stop: Arc<AtomicBool>,
    factory: Arc<dyn InputSourceFactory>,
    focus_chord: Chord,
    move_chord: Chord,
    direction: InputDirection,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> InputExit {
    // Retain the parked thread so stop/shutdown can `unpark` it awake.
    lock_shared(&shared).input_thread = Some(std::thread::current());
    let mut source = match factory.create() {
        Ok(source) => {
            let _ = ready_tx.send(Ok(()));
            source
        }
        Err(reason) => {
            let _ = ready_tx.send(Err(reason));
            return InputExit::Done;
        }
    };
    let focus_tuning = tuning_for(focus_chord.modifier, focus_chord.key_code);
    let move_tuning = tuning_for(move_chord.modifier, move_chord.key_code);
    // The source is destroyed here on thread exit.
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let Some(event) = source.next(&stop) else {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            // Unexpected EOS (disconnect/exhaustion): counted, logged, and
            // folded into the shutdown outcome (unclean, never clean) —
            // never spun on.
            lock_shared(&shared).diagnostics.unexpected_eos += 1;
            emit(&shared, 0, 0, 0, "input", "eos-unexpected", 0);
            break;
        };
        // Forward semantic actions only on key down with an exact chord
        // match. Key-up, inexact modifiers (including extras), and unrelated
        // keys are ignored; the core is never consulted here.
        if event.kind != TapKeyKind::KeyDown {
            lock_shared(&shared).diagnostics.input_ignored += 1;
            continue;
        }
        let mut pending = false;
        let focus_hit = decide_tap_event(
            TapKeyKind::KeyDown,
            event.flags,
            i64::from(event.key_code),
            focus_tuning,
            false,
            &mut pending,
        )
        .matched;
        let move_hit = decide_tap_event(
            TapKeyKind::KeyDown,
            event.flags,
            i64::from(event.key_code),
            move_tuning,
            false,
            &mut pending,
        )
        .matched;
        let action = if focus_hit {
            Some(InputSemanticAction::FocusStep { direction })
        } else if move_hit {
            Some(InputSemanticAction::MoveStep { direction })
        } else {
            None
        };
        let Some(action) = action else {
            lock_shared(&shared).diagnostics.input_ignored += 1;
            continue;
        };
        if !inbox.try_send_data(EngineJob::InputAction { action }) {
            lock_shared(&shared).diagnostics.data_dropped_full += 1;
        }
    }
    InputExit::Done
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
}
