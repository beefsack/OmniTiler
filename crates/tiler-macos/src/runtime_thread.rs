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

use crate::runtime::{
    AxCompletion, DesiredFrame, FocusCommand, FocusRequest, FrameCommand, InputSemanticAction,
    NativeContext, ObservedWindow, Shared, emit, lock_focus, lock_shared, parse_window_id,
    window_id,
};
use crate::runtime_lifecycle::EngineExit;
use crate::runtime_log::sanitize_backend_reason;
use crate::runtime_types::ScopedBackendSnapshot;

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
    /// engine admits exactly-homed current-user-Space members per domain
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
            Self::Observation { .. }
                | Self::ScopedObservation { .. }
                | Self::InputAction { .. }
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
    fn set_latest(&self, cmd: FocusCommand) -> Option<()> {
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

pub(crate) use crate::runtime_core::{
    RetainedApp, RetainedCore, build_event, build_native_event, clear_focus_into,
    domain_member_apps, forget_app_members, native_domain_views, resize_repeat_next, skeleton_key,
};
use crate::runtime_native::{
    handle_scoped_observation, plan_native_observation, reply_base_revision,
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
            forget_app_members(retained, pid);
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
            // slots for a dead app). Native mode re-plans every current
            // domain (closed IDs ride each domain event; unknown IDs are
            // ignored there); synthetic mode keeps the skeleton event.
            let native_ctx: Option<Arc<NativeContext>> = lock_shared(shared).native_context.clone();
            if let Some(ctx) = native_ctx {
                let views = native_domain_views(&ctx);
                plan_native_observation(
                    engine,
                    retained,
                    &ctx,
                    &views,
                    closed,
                    CoreCommand::Reconcile,
                    shared,
                    focus_queue,
                    focus_capacity,
                    pid,
                    generation,
                    0,
                );
                return;
            }
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
        EngineJob::ScopedObservation {
            pid,
            generation,
            seq,
            snapshot,
        } => {
            handle_scoped_observation(
                engine,
                retained,
                shared,
                focus_queue,
                focus_capacity,
                pid,
                generation,
                seq,
                snapshot,
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
                InputSemanticAction::ResizeStep { direction, mode } => {
                    // Exact Windows/KDE repeat rule: the press index
                    // continues only on a focused-window/direction/mode
                    // match, restarts otherwise — before the Engine
                    // dispatch. The core owns the pixel step
                    // (12/14/16/18/20px capped); refusals (edge no-op,
                    // float/overlay subject) settle with no writes and the
                    // next press restarts or continues per this same rule.
                    let (press_index, repeat) = resize_repeat_next(
                        retained.resize_repeat.clone(),
                        &window.0,
                        direction.as_str(),
                        mode.as_str(),
                    );
                    retained.resize_repeat = Some(repeat);
                    (
                        CoreCommand::Resize {
                            window: window.0.clone(),
                            direction: direction.as_str().to_owned(),
                            mode: mode.as_str().to_owned(),
                            press_index,
                        },
                        "resize",
                    )
                }
                InputSemanticAction::ToggleFloat => (
                    CoreCommand::ToggleFloat {
                        window: window.0.clone(),
                        float_rect: None,
                    },
                    "toggle-float",
                ),
                InputSemanticAction::ToggleOrientation => (
                    CoreCommand::ToggleOrientation {
                        window: window.0.clone(),
                    },
                    "toggle-orientation",
                ),
            };
            lock_shared(shared).diagnostics.input_forwarded += 1;
            emit(shared, 0, 0, 0, "input", op, 0);
            // Native mode resolves the subject's homed domain and plans
            // exactly there (one domain event over that domain's full
            // membership); an unmapped subject refuses instead of planning
            // the skeleton. Synthetic mode keeps the single skeleton event.
            let native_ctx: Option<Arc<NativeContext>> = lock_shared(shared).native_context.clone();
            if let Some(ctx) = native_ctx {
                let Some(key) = retained.window_domains.get(&window.0).cloned() else {
                    lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                    emit(shared, 0, 0, 0, "input", "refused-no-domain", 0);
                    return;
                };
                let views = native_domain_views(&ctx);
                let Some(view) = views.iter().find(|v| v.key == key) else {
                    lock_shared(shared).diagnostics.input_refused_no_focus += 1;
                    emit(shared, 0, 0, 0, "input", "refused-no-domain", 0);
                    return;
                };
                let member_apps = domain_member_apps(retained, &key);
                let revision = retained
                    .domain_revisions
                    .get(&key)
                    .copied()
                    .unwrap_or(retained.last_revision);
                let predicate = engine.fixed_size_predicate();
                let session = engine.session(&key);
                let focus_op = matches!(command, CoreCommand::Focus { .. });
                let event = build_native_event(
                    retained,
                    view,
                    &member_apps,
                    Some(window),
                    Vec::new(),
                    command,
                    session,
                    predicate,
                    revision,
                );
                let reply = engine.handle(&event);
                if let Some(base) = reply_base_revision(&reply) {
                    retained.domain_revisions.insert(key, base);
                }
                lock_shared(shared).count_reply(&format!("{op}-{}", reply_kind(&reply)));
                let mut merged = MergedPlan::default();
                collect_reply(&reply, &mut merged);
                apply_merged(
                    merged,
                    None,
                    op,
                    focus_op,
                    retained,
                    shared,
                    focus_queue,
                    focus_capacity,
                    0,
                    0,
                    0,
                );
                return;
            }
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
            note_completion(shared, "ax-result", pid, generation, cmd_id, &completion);
        }
        EngineJob::FocusResult {
            pid,
            generation,
            cmd_id,
            completion,
        } => {
            note_completion(shared, "focus-result", pid, generation, cmd_id, &completion);
        }
        EngineJob::Barrier(tx) => {
            let _ = tx.send(());
        }
        EngineJob::TestPanic => {
            panic!("phase-1 offline fault injection: engine panic");
        }
    }
}

pub(crate) fn reply_kind(reply: &CoreReply) -> &'static str {
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

/// Live per-app emission targets: PID, generation, frame slot, focus slot.
/// One row per non-terminated app, collected without holding the lock
/// during emission.
type LiveSlots = Vec<(u32, u64, Arc<FrameSlot>, Arc<FocusSlot>)>;

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
    let mut merged = MergedPlan::default();
    collect_reply(&reply, &mut merged);
    if let Some(base) = merged.revision {
        retained.last_revision = base;
    }
    // Single-reply focus emission only for explicit focus operations (the
    // merged native caller passes explicit focus replies through this same
    // gate per reply — observations never focus).
    let focus_reply = matches!(reply, CoreReply::FocusDirectional(_));
    apply_merged(
        merged,
        Some(token),
        op,
        focus_reply,
        retained,
        shared,
        focus_queue,
        focus_capacity,
        pid,
        generation,
        seq,
    )
}

/// Merged plan geometry across one or more core replies (one in synthetic
/// mode, one per tile domain in native mode). Geometry stays in plan order
/// per reply; replies concatenate in domain order.
#[derive(Default)]
pub(crate) struct MergedPlan {
    /// Full plan geometry including overlaid members (plan truth; the
    /// emission filter below withholds overlay writes without touching it).
    geometry: Vec<(String, Rect)>,
    /// Planned focus target, when some reply named one.
    focus_to: Option<WindowId>,
    /// Plan revision when some reply carried one (drives empty-set
    /// supersede; refusals carry none and emit nothing).
    revision: Option<u64>,
}

/// Extract plan geometry, focus target, and revision from one core reply.
/// Pure over the reply; the overlay filter and emission live in
/// [`apply_merged`] so synthetic single-reply and native multi-reply paths
/// share them exactly.
pub(crate) fn collect_reply(reply: &CoreReply, out: &mut MergedPlan) {
    // Geometry plus revision carried by success replies.
    let mut geometry: Vec<(String, Rect)> = Vec::new();
    let mut revision: Option<u64> = None;
    let mut focus_to: Option<WindowId> = None;
    match reply {
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
        CoreReply::Resize(plan) => {
            // Keyboard resize reflows exactly the two adjacent shares and
            // retains focus exactly, so the plan carries geometry plus
            // revision with no focus target (unlike moves, nothing
            // dispatches native focus downstream of a resize).
            revision = Some(plan.base_revision);
            geometry = plan
                .geometry
                .iter()
                .filter(|g| !g.client_clamped)
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
        }
        _ => {}
    }
    out.geometry.extend(geometry);
    if out.focus_to.is_none() {
        out.focus_to = focus_to;
    }
    if out.revision.is_none() {
        out.revision = revision;
    }
}

/// Apply one merged plan: snapshot desired geometry, filter overlay writes,
/// and hand per-app latest-wins frame sets plus at most one focus dispatch
/// to the owning workers. Shared exactly by the synthetic single-reply path
/// and the native multi-reply path so emission semantics cannot diverge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_merged(
    merged: MergedPlan,
    token: Option<&'static str>,
    op: &'static str,
    focus_reply: bool,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let MergedPlan {
        geometry,
        focus_to,
        revision,
    } = merged;
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
    // Collect live per-app slots plus the current topology revision fence
    // and focus route without holding the lock during emission.
    let (slots, topology_revision, focus_route, native_mode): (
        LiveSlots,
        u64,
        crate::focus_route::FocusRoute,
        bool,
    ) = {
        let guard = lock_shared(shared);
        (
            guard
                .apps
                .iter()
                .filter(|(_, r)| !r.terminated)
                .map(|(pid, r)| {
                    (
                        *pid,
                        r.generation,
                        Arc::clone(&r.frames),
                        Arc::clone(&r.focus),
                    )
                })
                .collect(),
            guard.topology_revision,
            guard.config.focus_route,
            matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            ),
        )
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
            .find(|(p, _, _, _)| *p == wpid)
            .map(|(_, generation, _, _)| *generation)
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
            topology_revision,
            // Engine-homed target domain at plan time: the backend pins
            // effects to exactly this domain, so a window that crosses
            // displays mid-plan refuses the remaining legs instead of
            // actuating outside its planned domain. `None` for unmapped
            // (synthetic/legacy) windows, which derive live instead.
            domain: retained.window_domains.get(window).cloned(),
        });
    }
    let mut emitted = 0u64;
    let mut coalesced = 0u64;
    let mut dead = unparseable;
    for (wpid, cmds) in &by_app {
        let Some((_, _, slot, _)) = slots.iter().find(|(p, _, _, _)| *p == *wpid) else {
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
        for (wpid, _, slot, _) in &slots {
            if !by_app.contains_key(wpid)
                && let Some(overwritten) = slot.set_latest(Vec::new())
            {
                coalesced += overwritten as u64;
            }
        }
    }
    // Focus requests out: only explicit focus operations dispatch native
    // focus. Reconcile/admit/move re-plan geometry (and track focus
    // internally) without moving native focus. Each dispatch travels two
    // paths: the bounded controller outbox (drained record, existing API)
    // and the owning app's latest-wins focus slot (actuation — the SAME
    // per-app AX owner that observed the window dispatches it). Both share
    // one `request_id` so dispatch truth correlates across them.
    let mut focus_dispatched = 0u64;
    let mut focus_dropped = 0u64;
    if focus_reply
        && let Some(to_window) = focus_to
        && let Some((fpid, fwid)) = parse_window_id(&to_window.0)
    {
        let (generation, focus_slot) = slots
            .iter()
            .find(|(p, _, _, _)| *p == fpid)
            .map(|(_, generation, _, focus)| (*generation, Some(Arc::clone(focus))))
            .unwrap_or((0, None));
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
            topology_revision,
        };
        let mut queue = lock_focus(focus_queue);
        if queue.len() < focus_capacity {
            queue.push(request);
            focus_dispatched = 1;
        } else {
            focus_dropped = 1;
        }
        drop(queue);
        // Owner actuation path, native mode only (the synthetic skeleton
        // has no per-app focus dispatch; its workers never read the slot,
        // so emitting there would only perturb the exact outbox counters
        // the legacy tests pin). Latest-wins: a newer desired focus
        // supersedes. A stopped slot means the app is terminating: the
        // command dies with its lifetime, counted as dropped.
        if native_mode && let Some(slot) = focus_slot {
            let cmd = FocusCommand {
                pid: fpid,
                generation,
                request_id,
                wid: fwid,
                window: to_window.0,
                revision: topology_revision,
                route: focus_route,
            };
            if slot.set_latest(cmd).is_some() {
                focus_dispatched += 1;
            } else {
                focus_dropped += 1;
            }
        } else if native_mode {
            focus_dropped += 1;
        }
    }
    {
        let mut guard = lock_shared(shared);
        if let Some(token) = token {
            guard.count_reply(&format!("{op}-{token}"));
        }
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
    //
    // `token` is `None` only for merged native applies, whose caller
    // counts each reply separately; the ring entry below then carries the
    // merged op name instead of a single reply kind.
    let token = token.unwrap_or("merged");
    emit(shared, pid, generation, seq, "core-reply", token, emitted);
    if emitted > 0 {
        emit(shared, pid, generation, seq, "frame", "emitted", emitted);
    }
    if focus_dropped > 0 {
        emit(shared, pid, generation, seq, "focus", "dropped-full", 1);
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
}
