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
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot, Inbox};
use crate::runtime_types::{FocusEvidence, ScopedBackendSnapshot, TopologyMode};

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
                if send_observation_scoped(&inbox, &shared, pid, generation, seq, &snapshot) {
                    focus_hint = snapshot.focused_wid;
                    last_scoped = Some(snapshot);
                } else {
                    pending_scoped = Some(snapshot);
                }
            }
        } else if let Some(snapshot) = pending.take() {
            seq += 1;
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
    }
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
    // `engine-facts-noreply` can be ordered against the observer exit —
    // teardown-before-query proves re-keying loss, query-before-teardown
    // proves the query failed while the observer still lived. Fixed
    // tokens plus IDs only; never window content. Logging only.
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

/// One bounded metadata-only observer teardown line: PID, generation,
/// sequence, plus the fixed stop reason. Pure (the worker above is the
/// only emitter).
pub(crate) fn format_observer_teardown_line(
    pid: u32,
    generation: u64,
    seq: u64,
    reason: &str,
) -> String {
    format!(
        "tiler-macos observer torn-down pid={pid} generation={generation} seq={seq} reason={reason}"
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
        // Correlated teardown metadata for the inverse `engine-facts-noreply`:
        // PID, generation, sequence, plus the fixed stop reason — so a live
        // run orders observer exit against the engine-facts query. IDs and
        // tokens only; never titles, bundles, or paths.
        for reason in ["stop", "frames-stopped", "stop-and-frames", "loop-exit"] {
            let line = format_observer_teardown_line(4242, 3, 11, reason);
            assert_eq!(
                line,
                format!(
                    "tiler-macos observer torn-down pid=4242 generation=3 seq=11 reason={reason}"
                ),
                "{reason}",
            );
        }
    }
}
