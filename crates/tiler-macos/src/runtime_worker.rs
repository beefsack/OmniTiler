#![forbid(unsafe_code)]

//! Per-app AX worker (implementation).
//!
//! Split from [`crate::runtime_thread`]: the owning worker that creates its
//! backend inside its own thread, publishes worker-stamped observations,
//! multiplexes latest-wins frame commands with observation pumps, dispatches
//! owner focus commands, and tears down honestly. Never touches the core.
//! Mode branch (synthetic legacy vs native scoped reads) selects the
//! observation path; frame actuation and fencing are mode-agnostic.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::runtime::{Shared, emit, lock_shared};
use crate::runtime_backend::{AxBackend, AxBackendFactory};
use crate::runtime_lifecycle::WorkerExit;
use crate::runtime_log::sanitize_backend_reason;
use crate::runtime_native::{FocusFence, fence_focus_command};
use crate::runtime_observe::{BackendSnapshot, PendingObservation, merge_readback, stage_latest};
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot, FrameWait, Inbox, WorkerDrainMark};
use crate::runtime_types::{FocusEvidence, ScopedBackendSnapshot, TopologyMode, parse_window_id};

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
    // Bump before enqueue (see `invalidate_clock` in runtime): the engine
    // may dequeue the instant the job lands.
    crate::runtime::invalidate_clock(shared);
    if inbox.try_send_data(job) {
        {
            let mut guard = lock_shared(shared);
            guard.diagnostics.observations_published += 1;
            guard.notify_test_waiters();
        }
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

/// Publish one worker-stamped scoped observation. Evidence stamps at send
/// time (freshest); staging equality ignores evidence, so a fresh stamp
/// can never manufacture a republish. Returns false under backpressure
/// (caller retains the latest snapshot and retries — the final state is
/// never silently lost).
fn send_observation_scoped(
    inbox: &Arc<Inbox>,
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    seq: u64,
    snapshot: &ScopedBackendSnapshot,
) -> bool {
    let job = EngineJob::ScopedObservation {
        pid,
        generation,
        seq,
        snapshot: snapshot.clone(),
    };
    // Bump before enqueue (see `invalidate_clock` in runtime): the engine
    // may dequeue the instant the job lands.
    crate::runtime::invalidate_clock(shared);
    if inbox.try_send_data(job) {
        {
            let mut guard = lock_shared(shared);
            guard.diagnostics.observations_published += 1;
            guard.notify_test_waiters();
        }
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

/// Stamp worker-send focus evidence: the main-thread frontmost legs plus the
/// owner's bounded selected-oracle read (`sys_focus`) over the `ax_focus`
/// slot when the owner read succeeded. The tile controller never supplies
/// the AX leg (the main thread performs no AX reads), so live the owner
/// truth wins whenever present. An absent owner leg keeps the stamped
/// evidence untouched (legacy/manual fixtures attest their own legs); it
/// never invents a read. Staging equality ignores evidence (and `sys_focus`),
/// so a fresh stamp never manufactures a republish.
pub(crate) fn stamp_owner_focus(
    snapshot: &mut ScopedBackendSnapshot,
    evidence: Option<FocusEvidence>,
) {
    snapshot.focus_evidence = evidence;
    if let (Some(stamped), Some(sys)) = (snapshot.focus_evidence.as_mut(), snapshot.sys_focus) {
        stamped.ax_focus = Some(sys);
    }
}

/// Stage one fresh scoped snapshot against the last SENT state and the
/// retained pending state. Same exactly-one-outcome contract as
/// [`stage_latest`]: equal-to-sent clears stale pending (no busy
/// republish), otherwise the newest replaces. Equality ignores focus
/// evidence (stamped per send) so evidence refresh alone never republishes.
fn stage_scoped(
    last: &Option<ScopedBackendSnapshot>,
    pending: &mut Option<ScopedBackendSnapshot>,
    snapshot: ScopedBackendSnapshot,
) {
    if last
        .as_ref()
        .is_some_and(|prev| crate::runtime_observe::scoped_snapshots_equal(prev, &snapshot))
    {
        *pending = None;
        return;
    }
    *pending = Some(snapshot);
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
/// Outcome of the worker-side same-app intervention check for one
/// arrival-bound deferred selection focus command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PriorFocusGate {
    /// The agreed selected focus already names the target: satisfied, no
    /// dispatch needed.
    Satisfied,
    /// The agreed selected focus still names the bind-time prior: the user
    /// did not move since the select dispatched; the focus write proceeds.
    Proceed,
    /// The agreed selected focus names a third window: the user focused
    /// elsewhere after the select — drop instead of fighting newer input.
    Moved,
    /// The agreed selected focus is unreadable or unparsable: arrival is
    /// unproven at dispatch — drop instead of dispatching blind.
    Unproven,
}

/// Gate one deferred selection focus dispatch against the worker's fresh
/// agreed selected focus (`None` = unreadable). The bind-time prior is the
/// engine-observed focus at select dispatch; the target is the
/// core-correlated remembered window. Only `Proceed` actuates: `Satisfied`
/// skips a pointless write, anything else drops counted. Pure over plain
/// data so offline tests prove the matrix without live AX.
pub(crate) fn gate_deferred_focus(
    target_pid: u32,
    target_wid: u32,
    prior: &str,
    current: Option<(u32, Option<u32>)>,
) -> PriorFocusGate {
    let Some((current_pid, current_wid)) = current else {
        return PriorFocusGate::Unproven;
    };
    if current_wid == Some(target_wid) && current_pid == target_pid {
        return PriorFocusGate::Satisfied;
    }
    match (parse_window_id(prior), current_wid) {
        (Some(prior_pair), Some(wid)) if prior_pair == (current_pid, wid) => {
            PriorFocusGate::Proceed
        }
        (Some(_), _) => PriorFocusGate::Moved,
        (None, _) => PriorFocusGate::Unproven,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn app_worker_main(
    pid: u32,
    generation: u64,
    factory: Arc<dyn AxBackendFactory>,
    frames: Arc<FrameSlot>,
    focus: Arc<FocusSlot>,
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
        let reason = match (stop.load(Ordering::SeqCst), frames.is_stopped()) {
            (true, true) => "stop-and-frames",
            (true, false) => "stop",
            (false, true) => "frames-stopped",
            (false, false) => "unreachable",
        };
        frames.note_drain(WorkerDrainMark::terminal(
            generation,
            frames.current_rev(),
            0,
            false,
        ));
        return teardown_worker(&mut backend, &shared, pid, generation, 0, reason);
    }
    let native_mode = matches!(
        lock_shared(&shared).config.topology_mode,
        TopologyMode::Native
    );
    let mut seq: u64 = 0;
    let mut last: Option<BackendSnapshot> = None;
    let mut pending = PendingObservation::new();
    let mut last_scoped: Option<ScopedBackendSnapshot> = None;
    let mut pending_scoped: Option<ScopedBackendSnapshot> = None;
    // Worker-to-engine inbox send attempts so far (observations plus frame
    // and focus completions, including queue-full drops, which can never
    // arrive late). The settlement mark pairs this count with the drain
    // watermark so a test fence can prove every attempt predating a mark is
    // either engine-processed or dropped.
    let mut sends_attempted: u64 = 0;
    // Slot revision drained through: taking a batch drains everything at or
    // below its revision (older untaken batches are superseded, never
    // actuated later); quiet passes drain through the observed current
    // revision. Teardown freezes the slot, draining it by definition.
    let mut drained: u64 = 0;
    let mut focus_hint: Option<u32> = None;
    // Seen frontmost for the change-triggered refresh below: initialized to
    // the currently published value so worker start alone never re-reads.
    let mut seen_frontmost = lock_shared(&shared)
        .focus_evidence
        .as_ref()
        .and_then(|evidence| evidence.frontmost_pid);
    // Bind the current native context before any scoped read so effects
    // fence against a current revision from the first call on.
    let bind_context = |backend: &mut Box<dyn AxBackend>, shared: &Arc<Mutex<Shared>>| {
        let guard = lock_shared(shared);
        let cell = guard.native_cell.clone();
        let frontmost = guard.focus_evidence.as_ref().and_then(|evidence| {
            evidence
                .frontmost_pid
                .map(|pid| (pid, evidence.frontmost_bundle.clone()))
        });
        drop(guard);
        let hook = crate::runtime_backend::EffectHook {
            cell,
            stop: stop.clone(),
            frames: frames.clone(),
            frontmost,
        };
        backend.bind_effect_hook(&hook);
    };
    // Startup observation: the first truth the engine ever sees for this
    // lifetime. A failure publishes nothing (retained state starts empty
    // and stays empty until a good read); it never publishes an empty list
    // in place of the failure.
    if native_mode {
        bind_context(&mut backend, &shared);
        match backend.snapshot_scoped() {
            Ok(snapshot) => {
                let mut snapshot = snapshot;
                stamp_owner_focus(&mut snapshot, lock_shared(&shared).focus_evidence.clone());
                seq += 1;
                sends_attempted += 1;
                if send_observation_scoped(&inbox, &shared, pid, generation, seq, &snapshot) {
                    focus_hint = snapshot.focused_wid;
                    last_scoped = Some(snapshot);
                } else {
                    pending_scoped = Some(snapshot);
                }
            }
            Err(reason) => note_observation_failure(&shared, pid, generation, seq + 1, &reason),
        }
    } else {
        match backend.snapshot() {
            Ok(windows) => {
                let snapshot = BackendSnapshot {
                    windows,
                    focused_wid: None,
                };
                seq += 1;
                sends_attempted += 1;
                if send_observation(&inbox, &shared, pid, generation, seq, &snapshot) {
                    last = Some(snapshot);
                } else {
                    pending.set(snapshot);
                }
            }
            Err(reason) => note_observation_failure(&shared, pid, generation, seq + 1, &reason),
        }
    }
    loop {
        if stop.load(Ordering::SeqCst) || frames.is_stopped() {
            break;
        }
        if native_mode {
            // Re-bind EVERY multiplex iteration (not just on revision
            // drift): the hook carries a copy of the main-thread frontmost
            // plus the live cell, so a cleared context or a frontmost change
            // must propagate to the very next fence even when the revision
            // is identical. Re-binding is cheap (Arc clones plus one lock);
            // the backend fails closed on a cleared cell instead of
            // actuating blind on a retained hook.
            bind_context(&mut backend, &shared);
            // Autonomous scoped observation (bounded; `None` covers both
            // "no scoped source" and "nothing new since the last pump").
            if let Some(result) = backend.poll_scoped(&stop) {
                match result {
                    Ok(snapshot) => {
                        let mut snapshot = snapshot;
                        stamp_owner_focus(
                            &mut snapshot,
                            lock_shared(&shared).focus_evidence.clone(),
                        );
                        stage_scoped(&last_scoped, &mut pending_scoped, snapshot);
                    }
                    Err(reason) => {
                        note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                    }
                }
            }
            // Frontmost-change refresh: a workspace frontmost change alone
            // (no AX notification, no frames) still alters the global-focus
            // verdict, so a changed frontmost since the last seen value
            // forces one bounded re-read and stages it for the flush below.
            // Value-compared, not cadence-compared: an unchanged frontmost
            // never re-reads, and one send happens per change at most — no
            // busy reassertion. Staging is bypassed deliberately: the
            // verdict must re-resolve (clear or promote) even when every
            // compared leg is identical, because the engine never saw the
            // new frontmost. A failed refresh preserves retained state and
            // still advances `seen_frontmost` (one attempt per value, never
            // a failure storm).
            if pending_scoped.is_none() {
                let fresh_frontmost = lock_shared(&shared)
                    .focus_evidence
                    .as_ref()
                    .and_then(|evidence| evidence.frontmost_pid);
                if fresh_frontmost != seen_frontmost {
                    seen_frontmost = fresh_frontmost;
                    match backend.snapshot_scoped() {
                        Ok(snapshot) => {
                            let mut snapshot = snapshot;
                            stamp_owner_focus(
                                &mut snapshot,
                                lock_shared(&shared).focus_evidence.clone(),
                            );
                            pending_scoped = Some(snapshot);
                        }
                        Err(reason) => {
                            note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                        }
                    }
                }
            }
        } else {
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
        }
        if stop.load(Ordering::SeqCst) || frames.is_stopped() {
            break;
        }
        // Flush the latest retained observation (backpressure: retained and
        // retried, never silently lost).
        if native_mode {
            if let Some(mut snapshot) = pending_scoped.take() {
                // Evidence stamps at send time (freshest), never at stage
                // time: staging equality ignores evidence, so a fresh stamp
                // can never manufacture a republish. The AX leg is the
                // owner's bounded selected-oracle read, never a main-thread
                // read and never the diagnostic system-wide leg.
                stamp_owner_focus(&mut snapshot, lock_shared(&shared).focus_evidence.clone());
                seq += 1;
                sends_attempted += 1;
                if send_observation_scoped(&inbox, &shared, pid, generation, seq, &snapshot) {
                    focus_hint = snapshot.focused_wid;
                    last_scoped = Some(snapshot);
                } else {
                    pending_scoped = Some(snapshot);
                }
            }
        } else if let Some(snapshot) = pending.take() {
            seq += 1;
            sends_attempted += 1;
            if send_observation(&inbox, &shared, pid, generation, seq, &snapshot) {
                focus_hint = snapshot.focused_wid;
                last = Some(snapshot);
            } else {
                pending.set(snapshot);
            }
        }
        // Owner-dispatched focus (native mode only): the latest-wins slot
        // command dispatches here, on the SAME owner that observed the
        // window. Fences mirror the frame path (stop, generation, PID,
        // revision); a refusal publishes a `FocusResult` and never retries.
        if native_mode && let Some(cmd) = focus.take() {
            let current_revision = lock_shared(&shared).topology_revision;
            match fence_focus_command(&cmd, pid, generation, current_revision) {
                FocusFence::Stale => {
                    lock_shared(&shared).diagnostics.stale_dropped += 1;
                    emit(
                        &shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "stale-rejected",
                        0,
                    );
                }
                FocusFence::Superseded => {
                    lock_shared(&shared).diagnostics.stale_dropped += 1;
                    emit(
                        &shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "revision-superseded",
                        0,
                    );
                }
                FocusFence::Dispatch => {
                    // Arrival-bound deferred selection focus only: re-check
                    // the bind-time prior against the worker's fresh agreed
                    // selected focus. Explicit focus carries no prior and
                    // dispatches legacy-unchanged. Satisfied skips a
                    // pointless write; Moved/Unproven drop counted instead
                    // of fighting newer user input or dispatching blind.
                    // No FocusResult rides the skip/drop paths (that lane
                    // is diagnostics-only; nothing settles on it).
                    let proceed = match cmd.expect_prior.as_deref() {
                        None => true,
                        Some(prior) => {
                            let current = crate::ffi::ax_selected_focus_evidence().ok().and_then(
                                |evidence| {
                                    u32::try_from(evidence.pid)
                                        .ok()
                                        .map(|pid| (pid, evidence.wid))
                                },
                            );
                            match gate_deferred_focus(cmd.pid, cmd.wid, prior, current) {
                                PriorFocusGate::Proceed => true,
                                PriorFocusGate::Satisfied => {
                                    emit(
                                        &shared,
                                        pid,
                                        generation,
                                        cmd.request_id,
                                        "focus",
                                        "already-satisfied",
                                        0,
                                    );
                                    false
                                }
                                PriorFocusGate::Moved => {
                                    lock_shared(&shared).diagnostics.stale_dropped += 1;
                                    emit(
                                        &shared,
                                        pid,
                                        generation,
                                        cmd.request_id,
                                        "focus",
                                        "superseded-observed",
                                        1,
                                    );
                                    false
                                }
                                PriorFocusGate::Unproven => {
                                    lock_shared(&shared).diagnostics.stale_dropped += 1;
                                    emit(
                                        &shared,
                                        pid,
                                        generation,
                                        cmd.request_id,
                                        "focus",
                                        "unproven-read",
                                        1,
                                    );
                                    false
                                }
                            }
                        }
                    };
                    if !proceed {
                        // Deferred-focus skip: the batch below stays queued
                        // for the next pass, so the drain watermark does not
                        // move; the mark still proves the send count.
                        frames.note_drain(WorkerDrainMark::pass(
                            generation,
                            drained,
                            sends_attempted,
                            !pending.is_empty() || pending_scoped.is_some(),
                        ));
                        continue;
                    }
                    emit(
                        &shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "dispatched",
                        0,
                    );
                    let completion = backend.dispatch_focus(&cmd);
                    let job = EngineJob::FocusResult {
                        pid,
                        generation,
                        cmd_id: cmd.request_id,
                        completion,
                    };
                    sends_attempted += 1;
                    if !inbox.try_send_data(job) {
                        lock_shared(&shared).diagnostics.result_dropped_full += 1;
                        emit(
                            &shared,
                            pid,
                            generation,
                            cmd.request_id,
                            "focus-result",
                            "queue-full-dropped",
                            0,
                        );
                    }
                }
            }
        }
        if stop.load(Ordering::SeqCst) || frames.is_stopped() {
            break;
        }
        let (taken_rev, cmds) = match frames.wait_batch_or_quiet(&stop, OBSERVE_SLICE) {
            FrameWait::Stopped => break,
            FrameWait::Quiet(observed_rev) => {
                // The slot held nothing at the observed revision, so
                // everything through it is drained. The revision comes from
                // the same guard as the wait: no queued-but-untaken batch
                // can hide between the wake and this read.
                drained = observed_rev;
                frames.note_drain(WorkerDrainMark::pass(
                    generation,
                    drained,
                    sends_attempted,
                    !pending.is_empty() || pending_scoped.is_some(),
                ));
                continue;
            }
            FrameWait::Batch(taken_rev, cmds) => (taken_rev, cmds),
        };
        // Taking a batch drains everything at or below its revision: older
        // untaken batches are superseded by definition, never actuated later.
        drained = taken_rev;
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
            // Topology revision fence: a display/Space change mid-plan
            // supersedes taken commands exactly like a newer desired set.
            // The remainder of the batch never starts; the next plan (or
            // the next observation) converges instead. No retry here.
            if cmd.topology_revision != lock_shared(&shared).topology_revision {
                let skipped = (cmds.len() - index) as u64;
                lock_shared(&shared).diagnostics.frames_coalesced += skipped;
                emit(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    "frame",
                    "revision-superseded",
                    skipped,
                );
                break;
            }
            // Redundant once observers report back: desired geometry already
            // matching the last observed geometry needs no native write.
            // Identity is wid plus full rect; anything else still actuates.
            // Native mode reads the last scoped snapshot; synthetic mode
            // reads the last legacy snapshot. Either way the comparison is
            // against actually observed geometry, never the plan.
            let redundant = if native_mode {
                last_scoped
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
            } else {
                last.as_ref()
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
            };
            if redundant {
                {
                    let mut guard = lock_shared(&shared);
                    guard.diagnostics.frames_redundant_skipped += 1;
                    guard.notify_test_waiters();
                }
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
            sends_attempted += 1;
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
        // batch — consumed even when the batch went partial (the fresh
        // readback IS the partial truth converging, not a retry). Unchanged
        // readbacks are skipped (a native clamp is not replayed endlessly);
        // failures preserve retained state.
        if native_mode {
            match backend.snapshot_scoped() {
                Ok(snapshot) => {
                    let mut snapshot = snapshot;
                    stamp_owner_focus(&mut snapshot, lock_shared(&shared).focus_evidence.clone());
                    stage_scoped(&last_scoped, &mut pending_scoped, snapshot);
                }
                Err(reason) => {
                    note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                }
            }
        } else {
            match backend.snapshot() {
                Ok(windows) => {
                    let snapshot = merge_readback(windows, focus_hint);
                    stage_latest(&last, &mut pending, snapshot);
                }
                Err(reason) => {
                    note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                }
            }
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        // End-of-pass mark: the taken batch is terminally accounted (every
        // command actuated or fenced above), its completion attempts were
        // made ahead of this mark, and the readback staged (or failed)
        // honestly.
        frames.note_drain(WorkerDrainMark::pass(
            generation,
            drained,
            sends_attempted,
            !pending.is_empty() || pending_scoped.is_some(),
        ));
    }
    // Final mark: teardown stops the slot, invalidating anything still
    // queued, so the frozen current revision is drained by definition and
    // nothing further will be sent. Retained observations die with the
    // worker: they were never enqueued, so `dropped_unsent` final-accounts
    // them instead of claiming an empty retry queue.
    frames.note_drain(WorkerDrainMark::terminal(
        generation,
        frames.current_rev(),
        sends_attempted,
        !pending.is_empty() || pending_scoped.is_some(),
    ));
    let reason = match (stop.load(Ordering::SeqCst), frames.is_stopped()) {
        (true, true) => "stop-and-frames",
        (true, false) => "stop",
        (false, true) => "frames-stopped",
        (false, false) => "loop-exit",
    };
    teardown_worker(&mut backend, &shared, pid, generation, seq, reason)
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
    reason: &'static str,
) -> WorkerExit {
    // Correlated teardown metadata: the stop reason (stop vs frames vs
    // both) rides alongside the existing ring outcome, so a live
    // `engine-facts-noreply` orders approximately against the observer exit —
    // teardown-start before the query suggests re-keying loss, while
    // query-before-teardown-start suggests the query failed while the
    // observer still lived. This line marks teardown start only; the ring's
    // `torn-down` vs `teardown-failed` below stays the authoritative outcome.
    // Fixed tokens plus IDs only; never window content. Logging only.
    eprintln!(
        "{}",
        format_observer_teardown_line(pid, generation, seq, reason)
    );
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

/// One bounded metadata-only observer teardown-start line: PID, generation,
/// sequence, plus the fixed stop reason. Pure (the worker above is the
/// only emitter). Start marker only — the ring's `torn-down` vs
/// `teardown-failed` outcome below stays authoritative.
pub(crate) fn format_observer_teardown_line(
    pid: u32,
    generation: u64,
    seq: u64,
    reason: &str,
) -> String {
    format!(
        "tiler-macos observer tearing-down pid={pid} generation={generation} seq={seq} reason={reason}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(sys_focus: Option<(u32, Option<u32>)>) -> ScopedBackendSnapshot {
        ScopedBackendSnapshot {
            windows: Vec::new(),
            focused_wid: Some(7),
            focus_cg_owned: true,
            focus_evidence: None,
            sys_focus,
            global_focus: None,
            meta: Vec::new(),
            excluded: 0,
            excluded_reasons: Vec::new(),
            cg_complete: true,
            cg_owner_wids: Vec::new(),
            cg_alive: Vec::new(),
            revision: 9,
        }
    }

    fn evidence(ax_focus: Option<(u32, Option<u32>)>) -> FocusEvidence {
        FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: Some("com.example.app".to_owned()),
            ax_focus,
        }
    }

    #[test]
    fn owner_sys_focus_wins_over_main_ax_slot() {
        // The tile controller never supplies the AX leg (it performs no AX
        // reads): a present owner read overwrites even a stale `Some`
        // there, while the frontmost legs ride through.
        let mut snapshot = snapshot(Some((4242, Some(7))));
        stamp_owner_focus(&mut snapshot, Some(evidence(Some((1, Some(2))))));
        let stamped = snapshot.focus_evidence.expect("stamped");
        assert_eq!(stamped.frontmost_pid, Some(4242));
        assert_eq!(stamped.ax_focus, Some((4242, Some(7))));
    }

    #[test]
    fn absent_owner_read_keeps_stamped_evidence() {
        // `None` sys (unreadable on the owner thread) invents nothing: the
        // stamped legs (legacy/manual fixtures attest their own) ride
        // through untouched.
        let mut snapshot = snapshot(None);
        stamp_owner_focus(&mut snapshot, Some(evidence(Some((4242, Some(7))))));
        let stamped = snapshot.focus_evidence.expect("stamped");
        assert_eq!(stamped.ax_focus, Some((4242, Some(7))));
        assert_eq!(stamped.frontmost_pid, Some(4242));
    }

    #[test]
    fn no_evidence_stays_unfocused() {
        // No controller evidence yet: nothing stamps, focus stays unfocused
        // (never guessed).
        let mut snapshot = snapshot(Some((4242, Some(7))));
        stamp_owner_focus(&mut snapshot, None);
        assert_eq!(snapshot.focus_evidence, None);
        // The raw leg still rides the snapshot for direct readers.
        assert_eq!(snapshot.sys_focus, Some((4242, Some(7))));
    }

    #[test]
    fn observer_teardown_line_names_ids_and_stop_reason() {
        // Correlated teardown-start metadata for the inverse `engine-facts-noreply`:
        // PID, generation, sequence, plus the fixed stop reason — so a live
        // run orders observer exit start against the engine-facts query.
        // IDs and tokens only; never titles, bundles, or paths.
        for reason in ["stop", "frames-stopped", "stop-and-frames", "loop-exit"] {
            let line = format_observer_teardown_line(4242, 3, 11, reason);
            assert_eq!(
                line,
                format!(
                    "tiler-macos observer tearing-down pid=4242 generation=3 seq=11 reason={reason}"
                ),
                "{reason}",
            );
        }
    }

    #[test]
    fn deferred_focus_gate_matrix() {
        // Same-app intervention matrix over the worker's fresh agreed
        // focus: already-on-target skips the write, unchanged-prior
        // proceeds, a third window refuses (user moved), and an unreadable
        // read refuses (unproven). Malformed priors never compare equal.
        assert_eq!(
            gate_deferred_focus(4242, 7, "4242_7", Some((4242, Some(7)))),
            PriorFocusGate::Satisfied,
            "already there: no write needed"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "4242_7", Some((4242, Some(7)))),
            PriorFocusGate::Proceed,
            "user did not move since bind"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "4242_7", Some((4242, Some(8)))),
            PriorFocusGate::Moved,
            "same-app user focus move never gets fought"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "4242_7", Some((9999, Some(9)))),
            PriorFocusGate::Moved,
            "other-app focus is a move too"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "4242_7", Some((4242, None))),
            PriorFocusGate::Moved,
            "windowless agreement still differs from the prior window"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "4242_7", None),
            PriorFocusGate::Unproven,
            "unreadable read dispatches nothing blind"
        );
        assert_eq!(
            gate_deferred_focus(4242, 9, "nope", Some((4242, Some(8)))),
            PriorFocusGate::Unproven,
            "malformed prior proves nothing"
        );
        // Prior equal to target with a moved current still refuses: the
        // comparison is against the fresh read, never the prior alone.
        assert_eq!(
            gate_deferred_focus(4242, 7, "4242_7", Some((4242, Some(8)))),
            PriorFocusGate::Moved,
        );
    }
}

/// Settlement-fence regressions: `wait_app_settled_for_test` must not return
/// before blocked actuation completes, and must settle (not hang) on the
/// terminal no-frame paths: redundant-skip, latest-wins supersession, and
/// stop. Offline only: gated fakes, no live desktop.
#[cfg(test)]
mod settled_fence_tests {
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use crate::runtime::{
        AxCompletion, AxObservation, FrameCommand, ObservedWindow, Runtime, RuntimeConfig,
        ShutdownOutcome, SubmitOutcome, WindowSizeHints,
    };
    use crate::runtime_backend::{AxBackend, AxBackendFactory, NativeInputStub};

    const PID: u32 = 4242;
    const SETTLED: Duration = Duration::from_secs(5);

    struct GateState {
        applied: Vec<(String, (i32, i32, i32, i32))>,
        entered: u64,
        open: bool,
        windows: Vec<ObservedWindow>,
        shutdown_open: bool,
        shutdown_entered: u64,
    }

    struct GateFactory {
        state: Arc<(Mutex<GateState>, Condvar)>,
    }

    impl AxBackendFactory for GateFactory {
        fn create(&self, _pid: u32) -> Result<Box<dyn AxBackend>, String> {
            Ok(Box::new(GateBackend {
                state: Arc::clone(&self.state),
            }))
        }
    }

    struct GateBackend {
        state: Arc<(Mutex<GateState>, Condvar)>,
    }

    impl AxBackend for GateBackend {
        fn apply_frame(&mut self, cmd: &FrameCommand) -> AxCompletion {
            let (lock, wake) = &*self.state;
            let mut guard = lock.lock().expect("mutex");
            // Entry signal first: a test waits for this to prove the worker
            // took a batch and parked inside actuation (event, not sleep).
            guard.entered += 1;
            wake.notify_all();
            // Bounded gate: proceeds when the test opens, or after the bound
            // (never strands teardown on a forgotten open).
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !guard.open {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                guard = wake.wait_timeout(guard, deadline - now).expect("mutex").0;
            }
            guard
                .applied
                .push((cmd.window.clone(), (cmd.x, cmd.y, cmd.w, cmd.h)));
            AxCompletion::Applied { elapsed_ms: 0 }
        }

        fn snapshot(&mut self) -> Result<Vec<ObservedWindow>, String> {
            Ok(self.state.0.lock().expect("mutex").windows.clone())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            let (lock, wake) = &*self.state;
            let mut guard = lock.lock().expect("mutex");
            // Teardown-entry signal: proves the worker reached teardown
            // (its terminal mark is already recorded) before the test
            // fences. Bounded like the apply gate.
            guard.shutdown_entered += 1;
            wake.notify_all();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !guard.shutdown_open {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                guard = wake.wait_timeout(guard, deadline - now).expect("mutex").0;
            }
            Ok(())
        }
    }

    fn plain(wid: u32, x: i32, y: i32, w: i32, h: i32) -> ObservedWindow {
        ObservedWindow {
            wid,
            x,
            y,
            w,
            h,
            floating: false,
            fullscreen: false,
            hints: WindowSizeHints {
                min_w: None,
                min_h: None,
                max_w: None,
                max_h: None,
            },
        }
    }

    fn start_gate(
        windows: Vec<ObservedWindow>,
        open: bool,
    ) -> (
        crate::runtime::RuntimeHandle,
        Arc<(Mutex<GateState>, Condvar)>,
    ) {
        let state = Arc::new((
            Mutex::new(GateState {
                applied: Vec::new(),
                entered: 0,
                open,
                windows,
                shutdown_open: true,
                shutdown_entered: 0,
            }),
            Condvar::new(),
        ));
        let handle = Runtime::start(
            RuntimeConfig::default(),
            Arc::new(GateFactory {
                state: Arc::clone(&state),
            }),
            Arc::new(NativeInputStub),
        )
        .expect("runtime starts offline");
        // The worker publishes its startup snapshot as sequence 1, so the
        // test owns the sequence space from 2 on.
        (handle, state)
    }

    fn open(state: &Arc<(Mutex<GateState>, Condvar)>) {
        let (lock, wake) = &**state;
        lock.lock().expect("mutex").open = true;
        wake.notify_all();
    }

    fn hold_shutdown(state: &Arc<(Mutex<GateState>, Condvar)>) {
        let (lock, _) = &**state;
        lock.lock().expect("mutex").shutdown_open = false;
    }

    fn release_shutdown(state: &Arc<(Mutex<GateState>, Condvar)>) {
        let (lock, wake) = &**state;
        lock.lock().expect("mutex").shutdown_open = true;
        wake.notify_all();
    }

    fn applied_for(
        state: &Arc<(Mutex<GateState>, Condvar)>,
        window: &str,
    ) -> Vec<(i32, i32, i32, i32)> {
        state
            .0
            .lock()
            .expect("mutex")
            .applied
            .iter()
            .filter(|(w, _)| w == window)
            .map(|(_, rect)| *rect)
            .collect()
    }

    /// Bounded event wait for the worker to enter actuation `want` times.
    fn wait_entered(state: &Arc<(Mutex<GateState>, Condvar)>, want: u64) -> bool {
        let (lock, wake) = &**state;
        let deadline = std::time::Instant::now() + SETTLED;
        let mut guard = lock.lock().expect("mutex");
        while guard.entered < want {
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            guard = wake.wait_timeout(guard, deadline - now).expect("mutex").0;
        }
        true
    }

    /// Bounded event wait for the worker to enter backend teardown.
    fn wait_shutdown_entered(state: &Arc<(Mutex<GateState>, Condvar)>) -> bool {
        let (lock, wake) = &**state;
        let deadline = std::time::Instant::now() + SETTLED;
        let mut guard = lock.lock().expect("mutex");
        while guard.shutdown_entered < 1 {
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            guard = wake.wait_timeout(guard, deadline - now).expect("mutex").0;
        }
        true
    }

    #[test]
    fn fence_does_not_return_before_blocked_actuation() {
        // The startup snapshot is empty, so the test's batch is the only
        // one carrying commands: exactly one actuation follows.
        let (handle, state) = start_gate(Vec::new(), false);
        let generation = handle.launch_app(PID).expect("launch");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 2,
                windows: vec![plain(7, 0, 0, 960, 1080)],
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        // Prove the worker took the batch and parked inside the closed
        // gate before asserting the fence times out.
        assert!(wait_entered(&state, 1), "worker never entered actuation");
        // No mark can drain through the dispatched revision while the
        // worker is parked: the fence must time out, not return early.
        assert!(
            !handle.wait_app_settled_for_test(PID, generation, Duration::from_millis(300)),
            "fence returned before blocked actuation completed"
        );
        assert!(
            applied_for(&state, "4242_7").is_empty(),
            "closed gate must withhold actuation"
        );
        open(&state);
        assert!(
            handle.wait_app_settled_for_test(PID, generation, SETTLED),
            "fence did not settle after the gate opened"
        );
        assert_eq!(applied_for(&state, "4242_7").len(), 1);
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    #[test]
    fn fence_settles_redundant_no_frame_path() {
        // Backend truth already matches the plan: the worker must skip every
        // command (no AxResult, no applied frame) and still settle.
        let windows = vec![plain(7, 0, 0, 1920, 1080)];
        let (handle, state) = start_gate(windows.clone(), true);
        let generation = handle.launch_app(PID).expect("launch");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 2,
                windows,
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        assert!(
            handle.wait_app_settled_for_test(PID, generation, SETTLED),
            "fence did not settle the redundant path"
        );
        assert!(
            applied_for(&state, "4242_7").is_empty(),
            "redundant desired geometry must take no write"
        );
        assert!(
            handle.diagnostics().frames_redundant_skipped >= 1,
            "expected a worker redundant skip: {:?}",
            handle.diagnostics()
        );
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    #[test]
    fn fence_settles_latest_wins_supersession() {
        // The worker parks inside the first command of a two-command batch
        // while a newer plan lands: the started command finishes, the rest
        // of the taken batch fences as superseded, and the latest plan
        // actuates. Entry order is deterministic: exactly two writes total,
        // the last one for the first window matching the latest plan.
        let (handle, state) = start_gate(Vec::new(), false);
        let generation = handle.launch_app(PID).expect("launch");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 2,
                windows: vec![plain(7, 0, 0, 960, 1080), plain(8, 960, 0, 960, 1080)],
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        // Parked inside the taken batch: the next plan supersedes the rest.
        assert!(wait_entered(&state, 1), "worker never entered actuation");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 3,
                windows: vec![plain(7, 0, 0, 960, 1080)],
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        open(&state);
        assert!(
            handle.wait_app_settled_for_test(PID, generation, SETTLED),
            "fence did not settle supersession"
        );
        // One started write from the superseded batch plus the latest plan:
        // two writes total, and the first window ends on the latest geometry.
        let total = applied_for(&state, "4242_7").len() + applied_for(&state, "4242_8").len();
        assert_eq!(total, 2, "started write plus latest plan, nothing more");
        let latest = handle
            .last_desired_geometry()
            .iter()
            .find(|f| f.window == "4242_7")
            .map(|f| (f.x, f.y, f.w, f.h))
            .expect("latest plan names 4242_7");
        assert_eq!(applied_for(&state, "4242_7").last(), Some(&latest));
        assert!(
            handle.diagnostics().frames_coalesced >= 1,
            "expected a supersede fence: {:?}",
            handle.diagnostics()
        );
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    #[test]
    fn fence_settles_stop_with_queued_work() {
        let (handle, state) = start_gate(vec![plain(7, 0, 0, 960, 1080)], false);
        let generation = handle.launch_app(PID).expect("launch");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 2,
                windows: vec![plain(7, 0, 0, 960, 1080)],
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        handle.terminate_app(PID);
        // The worker is still gated; opening lets it observe the stop,
        // fence the queued batch, and tear down with a final mark.
        open(&state);
        assert!(
            handle.wait_app_settled_for_test(PID, generation, SETTLED),
            "fence did not settle the stopped worker"
        );
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    #[test]
    fn fence_settles_parked_teardown_terminal_mark() {
        // The worker parks inside backend teardown AFTER recording its
        // terminal mark: a fence baseline read now sits past the final
        // mark, so only the baseline-independent terminal rule (plus one
        // barrier) can settle. Without it the fence waits for a second mark
        // that never comes and times out despite nothing outstanding.
        let (handle, state) = start_gate(Vec::new(), false);
        hold_shutdown(&state);
        let generation = handle.launch_app(PID).expect("launch");
        assert_eq!(
            handle.submit_observation(AxObservation {
                pid: PID,
                generation,
                seq: 2,
                windows: vec![plain(7, 0, 0, 960, 1080)],
                focused_wid: None,
            }),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        assert!(wait_entered(&state, 1), "worker never entered actuation");
        handle.terminate_app(PID);
        open(&state);
        // Parked in teardown with the terminal mark already recorded.
        assert!(
            wait_shutdown_entered(&state),
            "worker never entered teardown"
        );
        assert!(
            handle.wait_app_settled_for_test(PID, generation, SETTLED),
            "fence did not settle the parked teardown"
        );
        release_shutdown(&state);
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    /// Factory whose backend creation blocks until released: the launch
    /// times out waiting for readiness, the record is removed, and the
    /// worker survives briefly as an exact-generation zombie.
    struct HangingFactory {
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl AxBackendFactory for HangingFactory {
        fn create(&self, _pid: u32) -> Result<Box<dyn AxBackend>, String> {
            let (lock, wake) = &*self.release;
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut guard = lock.lock().expect("mutex");
            while !*guard {
                let now = std::time::Instant::now();
                if now >= deadline {
                    break;
                }
                guard = wake.wait_timeout(guard, deadline - now).expect("mutex").0;
            }
            // Creation succeeds once released: the worker sends its stale
            // launch, reports ready, then observes the removal stop and
            // tears down through the early-stop path.
            Ok(Box::new(GateBackend {
                state: Arc::new((
                    Mutex::new(GateState {
                        applied: Vec::new(),
                        entered: 0,
                        open: true,
                        windows: Vec::new(),
                        shutdown_open: true,
                        shutdown_entered: 0,
                    }),
                    Condvar::new(),
                )),
            }))
        }
    }

    #[test]
    fn fence_refuses_live_zombie_worker() {
        // Backend creation blocks: the launch times out waiting for
        // readiness, the record is removed, and the worker survives as an
        // exact-generation zombie that could still actuate after this
        // return. The fence must refuse, not claim settled.
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let config = RuntimeConfig {
            worker_ready_timeout: Duration::from_millis(100),
            ..RuntimeConfig::default()
        };
        let handle = Runtime::start(
            config,
            Arc::new(HangingFactory {
                release: Arc::clone(&release),
            }),
            Arc::new(NativeInputStub),
        )
        .expect("runtime starts offline");
        // The single launch mints generation 1 before the worker spawns.
        match handle.launch_app(PID) {
            Err(crate::runtime_types::StartupError::WorkerNotReady) => {}
            other => panic!("expected WorkerNotReady, got {other:?}"),
        }
        assert!(
            !handle.wait_app_settled_for_test(PID, 1, Duration::from_secs(2)),
            "fence claimed settled with a live exact-generation zombie"
        );
        // Release creation: the worker reports ready, observes the removal
        // stop, and tears down through the early-stop path. Shutdown joins
        // it, proving a hang-free exit.
        {
            let (lock, wake) = &*release;
            *lock.lock().expect("mutex") = true;
            wake.notify_all();
        }
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }

    #[test]
    fn fence_settles_unknown_pid_trivially() {
        // No record and no exact-generation zombie: nothing outstanding can
        // arrive late, so one barrier proves it.
        let (handle, _) = start_gate(Vec::new(), true);
        assert!(
            handle.wait_app_settled_for_test(9999, 1, SETTLED),
            "fence did not settle an unknown pid"
        );
        let report = handle.shutdown(Duration::from_secs(5));
        assert_eq!(report.outcome, ShutdownOutcome::Clean);
    }
}
