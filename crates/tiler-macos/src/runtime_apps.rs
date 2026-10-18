#![forbid(unsafe_code)]

//! Per-app lifetime admission (launch, readiness, termination).
//!
//! Split from [`crate::runtime`]: the [`crate::runtime::RuntimeHandle`]
//! methods that admit application PIDs, spawn their owning AX workers,
//! poll backend readiness, and mark termination. Submission, topology,
//! queries, and shutdown stay on `runtime`. Freshness fencing is untouched:
//! every enqueueing path still advances the shared arrival clock before the
//! send (see [`crate::runtime_shared::invalidate_clock`]).

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::runtime::{
    RuntimeHandle, emit, engine_alive, invalidate_clock, lock_focus, lock_shared,
};
use crate::runtime_lifecycle::{LiveHandle, WorkerExit, join_exited};
use crate::runtime_shared::AppRecord;
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot};
use crate::runtime_types::{LifecycleEvent, StartupError, TerminateOutcome, WorkerReadiness};
use crate::runtime_worker::app_worker_main;

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
        // A new lifetime changes future reports (fresh retained rows on
        // its first observation): fence any in-flight reply now rather
        // than when its windows first arrive.
        invalidate_clock(&self.shared);
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
        // Control lane: never dropped by data saturation. Bump before
        // the send (see `invalidate_clock`); a refused send on a dead
        // runtime over-marks harmlessly (no flights can be outstanding).
        invalidate_clock(&self.shared);
        let _ = self
            .inbox
            .send_control(EngineJob::Terminate { pid, generation });
        self.inbox.wake_all();
        TerminateOutcome::Terminated { generation }
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
}
