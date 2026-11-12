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
use crate::runtime_native::{
    CloseBaselineGate, FocusFence, close_attempt_ms, close_completion_disposition,
    fence_focus_command, gate_close_baseline,
};
use crate::runtime_observe::{BackendSnapshot, PendingObservation, merge_readback, stage_latest};
use crate::runtime_shared::NativeContextCell;
use crate::runtime_shared::close_focus_is_current;
use crate::runtime_thread::{EngineJob, FocusSlot, FrameSlot, FrameWait, Inbox, WorkerDrainMark};
use crate::runtime_types::{
    ArrivalCapture, BracketRead, CloseFocusBinding, FocusCommand, FocusEvidence,
    ScopedBackendSnapshot, TopologyMode, parse_window_id, window_id,
};
use tiler_core::directional::WindowId;
use tiler_core::world_report::{
    AfterShownMember, ArrivalBracket, ArrivalLocation, VisibilityReason, VisibilitySample,
    VisibilitySetter,
};

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
    // Host-change intake: poll-result receipt IS intake (no tap or
    // notification callback exists on this path). One mint off the shared
    // trace counter plus the receipt stamp; untraced (disabled) when `None`.
    let (trace_id, intake_us) = crate::stage_timing::mint_observer_trace().unwrap_or((0, 0));
    // Enqueue stamp immediately before the inbox send so the engine QUEUE
    // record is pure inbox wait.
    let enqueue_us = crate::stage_timing::enqueue_stamp();
    let job = EngineJob::ScopedObservation {
        pid,
        generation,
        seq,
        snapshot: snapshot.clone(),
        trace_id,
        intake_us,
        enqueue_us,
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

/// Owner-thread arrival capture for frozen members, as a pure function over
/// already-read legs so tests exercise the production mapping with scripted
/// backends: the caller (always the owning worker) samples the invalidation
/// watermark and the cell generation around its own
/// [`AxBackend::capture_brackets`] batch and passes the samples in. A moved
/// watermark/generation — or disagreeing before/after legs — marks every
/// row `invalidated` (proof withholds, never dispatches on it). Unreadable
/// legs omit the member (no capture: absence defers downstream, never a
/// guessed home or frame). Only members owned by `pid` are considered; other
/// PIDs' owners capture their own. The original read watermark is retained
/// verbatim (never rebased): the evidence-only notification never bumps the
/// clock, so a newer watermark after stale reads is never assigned.
///
/// Identity: a read attests its window only through the backend's
/// owner join (`owner_pid`): a read naming another pid — or none — omits the
/// member, never a reused wid. `lifetime_matched` is `Some(true)` exactly on
/// that join (the generation pin is the caller's own live lifetime); it is
/// never hardcoded over unattested reads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn capture_member_brackets(
    reads: &[BracketRead],
    members: &[AfterShownMember],
    pid: u32,
    generation: u64,
    revision: u64,
    obs_before: u64,
    obs_after: u64,
    gen_before: u64,
    gen_after: u64,
    focus_id: &str,
    issued_revision: u64,
    adapter_incarnation: &str,
    core_generation: &str,
) -> Vec<ArrivalCapture> {
    let batch_invalidated = obs_before != obs_after || gen_before != gen_after;
    let mut out = Vec::new();
    for member in members {
        let Some((mpid, mwid)) = parse_window_id(&member.win.0) else {
            continue;
        };
        if mpid != pid {
            continue;
        }
        let Some(read) = reads.iter().find(|r| r.wid == mwid) else {
            continue;
        };
        // Lifetime join first: the backend must attest this window still
        // belongs to this owner around the home/frame legs. Visibility legs
        // alone can never attest a reused wid.
        if read.owner_pid != Some(pid) {
            continue;
        }
        let (Some(before_min), Some(before_hidden), Some(after_min), Some(after_hidden)) = (
            read.before_minimized,
            read.before_hidden,
            read.after_minimized,
            read.after_hidden,
        ) else {
            continue;
        };
        let shown_before = !before_min && !before_hidden;
        let shown_after = !after_min && !after_hidden;
        let sample = |shown: bool, minimized: bool, hidden: bool| VisibilitySample {
            obs: obs_after,
            lifetime_matched: Some(true),
            visibility: tiler_core::world_report::VisibilityFact {
                visible: Some(shown),
                suppression_owned: Some(false),
            },
            minimized: Some(minimized),
            host_hidden: Some(hidden),
        };
        let frame = read.frame.and_then(|(x, y, w, h)| {
            if w > 0 && h > 0 {
                Some(tiler_core::geometry::Rect { x, y, w, h })
            } else {
                None
            }
        });
        out.push(ArrivalCapture {
            win: member.win.0.clone(),
            pid,
            generation,
            revision,
            obs: obs_after,
            context_generation: gen_after,
            focus_id: focus_id.to_owned(),
            issued_revision,
            adapter_incarnation: adapter_incarnation.to_owned(),
            core_generation: core_generation.to_owned(),
            before: sample(shown_before, before_min, before_hidden),
            home_sid: if read.spaces.len() == 1 {
                Some(read.spaces[0])
            } else {
                None
            },
            frame,
            after: sample(shown_after, after_min, after_hidden),
            invalidated: batch_invalidated
                || before_min != after_min
                || before_hidden != after_hidden,
        });
    }
    out
}
/// Whether this owner owes a capture for the current pending intent: a
/// pending selection intent exists with at least one frozen member owned by
/// `pid`, and the worker has not yet attempted a capture for exactly this
/// immutable request tuple plus watermark, topology revision, context
/// generation, and app generation. Pure over shared snapshots so the
/// multiplex pump calls it at existing triggers with no polling timer, no
/// retry, and no new work ids.
pub(crate) fn arrival_capture_due(
    pending: Option<(String, u64, String, String)>,
    owned_members: usize,
    obs: u64,
    revision: u64,
    context_generation: u64,
    generation: u64,
    last_attempt: &Option<(String, u64, String, String, u64, u64, u64, u64)>,
) -> Option<(String, u64, String, String, u64, u64, u64, u64)> {
    let (focus_id, issued_revision, adapter_incarnation, core_generation) = pending?;
    if owned_members == 0 {
        return None;
    }
    let key = (
        focus_id,
        issued_revision,
        adapter_incarnation,
        core_generation,
        obs,
        revision,
        context_generation,
        generation,
    );
    if last_attempt.as_ref() == Some(&key) {
        return None;
    }
    Some(key)
}
/// Owner-side capture for the current pending intent: one ordered bracket
/// batch for the frozen members owned by `pid`, mapped with the sampled
/// watermark/cell generations, sent as one evidence-only
/// [`EngineJob::ArrivalCaptureReady`] notification. Never bumps the shared
/// clock (the rows carry the fixed sampled watermark). Bounded: at most the
/// frozen owned members, once per immutable request tuple plus watermark,
/// topology revision, context generation, and app generation — the caller
/// records `last_attempt` BEFORE the native read/enqueue so unreadable
/// legs, empty captures, moved watermarks, and a full queue never retry on
/// a later multiplex. A moved watermark/revision/context around the read
/// rejects without rebasing to the newly sampled stamp. Capture with no
/// owned members skips without recording. Returns true when a notification
/// was enqueued.
#[allow(clippy::too_many_arguments)]
pub(crate) fn maybe_send_arrival_capture(
    backend: &mut dyn AxBackend,
    pid: u32,
    generation: u64,
    shared: &Arc<Mutex<Shared>>,
    cell: &NativeContextCell,
    inbox: &Arc<Inbox>,
    last_attempt: &mut Option<(String, u64, String, String, u64, u64, u64, u64)>,
) -> bool {
    let (focus_id, issued_revision, adapter_incarnation, core_generation, members): (
        String,
        u64,
        String,
        String,
        Vec<AfterShownMember>,
    ) = {
        let guard = lock_shared(shared);
        match guard.selection_focus_pending.as_ref() {
            Some(pending) => (
                pending.focus_id.clone(),
                pending.issued_revision,
                pending.adapter_incarnation.clone(),
                pending.core_generation.clone(),
                pending
                    .after_shown
                    .members
                    .iter()
                    .filter(|m| parse_window_id(&m.win.0).is_some_and(|(mpid, _)| mpid == pid))
                    .cloned()
                    .collect(),
            ),
            None => return false,
        }
    };
    if members.is_empty() {
        return false;
    }
    let clock = lock_shared(shared).clock.clone();
    let obs_before = clock.get();
    let revision_before = lock_shared(shared).topology_revision;
    let (_, gen_before) = cell.load_versioned();
    let due_key = arrival_capture_due(
        Some((
            focus_id.clone(),
            issued_revision,
            adapter_incarnation.clone(),
            core_generation.clone(),
        )),
        members.len(),
        obs_before,
        revision_before,
        gen_before,
        generation,
        last_attempt,
    );
    let Some(key) = due_key else {
        return false;
    };
    // Last-attempt record BEFORE the native read/enqueue: empty reads,
    // moved stamps, and a full queue never retry on a later multiplex.
    // Only a legitimate request/invalidation/context/lifetime change (a new
    // key) allows one fresh attempt.
    *last_attempt = Some(key.clone());
    let wids: Vec<u32> = members
        .iter()
        .filter_map(|m| parse_window_id(&m.win.0).map(|(_, wid)| wid))
        .collect();
    let reads = backend.capture_brackets(&wids);
    let (_, gen_after) = cell.load_versioned();
    let obs_after = clock.get();
    let revision_after = lock_shared(shared).topology_revision;
    if obs_after != obs_before || revision_after != revision_before || gen_after != gen_before {
        // Stamp moved around the read: reject as attempted without
        // rebasing to the newly sampled stamp.
        return false;
    }
    let captures = capture_member_brackets(
        &reads,
        &members,
        pid,
        generation,
        revision_before,
        obs_before,
        obs_after,
        gen_before,
        gen_after,
        &focus_id,
        issued_revision,
        &adapter_incarnation,
        &core_generation,
    );
    if captures.is_empty() {
        return false;
    }
    let job = EngineJob::ArrivalCaptureReady {
        pid,
        generation,
        focus_id: focus_id.clone(),
        issued_revision,
        adapter_incarnation: adapter_incarnation.clone(),
        core_generation: core_generation.clone(),
        obs: obs_before,
        revision: revision_before,
        context_generation: gen_before,
        captures,
    };
    if inbox.try_send_data(job) {
        emit(shared, pid, generation, 0, "arrival", "capture-sent", 1);
        true
    } else {
        lock_shared(shared).diagnostics.result_dropped_full += 1;
        emit(
            shared,
            pid,
            generation,
            0,
            "arrival",
            "queue-full-dropped",
            1,
        );
        false
    }
}

/// Owner-thread arrival rows for one close-focus result: every actually
/// proven frozen member row (all PIDs, not just the dispatching owner),
/// built from the retained captures the engine validated as current plus a
/// fresh target-owner bracket batch when this owner's rows are missing or
/// stale. Facts only — the core settles from the rows with genuine
/// brackets/watermarks. `None`/empty (no `afterShown`, moved watermark,
/// stale tuple, unattested owner, unmappable native home, unreadable legs)
/// omits rows, never fabricates them; a partial set stays partial. Called
/// on the SAME owner thread that dispatches. Returns the rows plus the
/// agreed watermark: the caller withholds the rows when the watermark moved
/// after the dispatch.
///
/// The native home output comes from the observed native domain for the
/// captured Space (revision-fenced live lookup), never from the frozen
/// member row: a home whose Space has no live domain omits the row. The
/// capture's singleton home must equal the release-stamped expected SID
/// and the live domain for that Space must be current, readable, and
/// output-identical to the frozen member scope: a replacement capture on
/// another Space — or a domain that moved output — under the same
/// obs/cell stamp omits the row so the owner gate defers.
pub(crate) fn close_arrival_brackets(
    backend: &mut dyn AxBackend,
    close: &crate::runtime_types::CloseFocusBinding,
    pid: u32,
    generation: u64,
    shared: &Arc<Mutex<Shared>>,
    revision: u64,
) -> (Vec<ArrivalBracket>, u64) {
    let Some(after) = close.after_shown.as_ref() else {
        return (Vec::new(), lock_shared(shared).clock.get());
    };
    let live_obs = lock_shared(shared).clock.get();
    // Fresh target-owner bracket batch when this owner's retained rows are
    // missing or stale: one ordered read around the dispatch (facts only).
    // Other PIDs' rows come from the retained captures only — this thread
    // never reads another owner's windows.
    let owned_members: Vec<AfterShownMember> = after
        .members
        .iter()
        .filter(|m| parse_window_id(&m.win.0).is_some_and(|(mpid, _)| mpid == pid))
        .cloned()
        .collect();
    let owned_current = {
        let guard = lock_shared(shared);
        !owned_members.is_empty()
            && owned_members.iter().all(|m| {
                guard.arrival_captures.get(&m.win.0).is_some_and(|cap| {
                    crate::world_dispatch::arrival_capture_current(
                        &guard,
                        cap,
                        &close.close_id,
                        close.issued_revision,
                        &close.adapter_incarnation,
                        &close.core_generation,
                    ) && cap.obs == live_obs
                })
            })
    };
    // Selection-proof path (a frozen stamp rides the binding): never take
    // fresh reads here — only retained captures already validated under the
    // frozen proof stamp prove the rows. Unvalidated fresh rows must not
    // replace them when current facts or homes differ; a missing or stale
    // retained row omits the member below so the owner gate defers.
    let stamped = close.proven_stamp.is_some() && close.after_shown.is_some();
    let fresh: Vec<ArrivalCapture> = if stamped || owned_members.is_empty() || owned_current {
        Vec::new()
    } else {
        let wids: Vec<u32> = owned_members
            .iter()
            .filter_map(|m| parse_window_id(&m.win.0).map(|(_, wid)| wid))
            .collect();
        let clock = lock_shared(shared).clock.clone();
        let cell = lock_shared(shared).native_cell.clone();
        let obs_before = clock.get();
        let (_, gen_before) = cell.load_versioned();
        let reads = backend.capture_brackets(&wids);
        let (_, gen_after) = cell.load_versioned();
        let obs_after = clock.get();
        capture_member_brackets(
            &reads,
            &owned_members,
            pid,
            generation,
            revision,
            obs_before,
            obs_after,
            gen_before,
            gen_after,
            &close.close_id,
            close.issued_revision,
            &close.adapter_incarnation,
            &close.core_generation,
        )
    };
    let guard = lock_shared(shared);
    let now = guard.clock.get();
    if now != live_obs {
        return (Vec::new(), now);
    }
    let mut out = Vec::new();
    for member in &after.members {
        let cap = fresh
            .iter()
            .find(|c| c.win == member.win.0)
            .or_else(|| guard.arrival_captures.get(&member.win.0));
        let Some(cap) = cap else {
            continue;
        };
        if !crate::world_dispatch::arrival_capture_current(
            &guard,
            cap,
            &close.close_id,
            close.issued_revision,
            &close.adapter_incarnation,
            &close.core_generation,
        ) || cap.obs != now
            || close
                .after_shown
                .as_ref()
                .is_some_and(|after| cap.obs < after.obs)
        {
            continue;
        }
        // Genuine brackets: before/after equality already enforced at
        // capture (`invalidated`), visibility must be shown both legs.
        if cap.invalidated {
            continue;
        }
        let visible = |s: &VisibilitySample| {
            s.visibility.visible == Some(true) && s.lifetime_matched == Some(true)
        };
        if !visible(&cap.before) || !visible(&cap.after) {
            continue;
        }
        let Some(home_sid) = cap.home_sid else {
            continue;
        };
        // Stamped expected-SID recheck (replacement-capture guard): the
        // capture's singleton home must equal the release-stamped expected
        // Space verbatim. A replacement capture naming another Space under
        // the same obs/cell stamp omits the row so the owner gate defers —
        // never a cross-Space dispatch. `None` (bindings predating the
        // stamp, never selection releases) omits: no expected proof, no row.
        let Some(expected_sid) = close.expected_sid else {
            continue;
        };
        if home_sid != expected_sid {
            continue;
        }
        // Null frozen frame still needs readable live geometry: a missing
        // live frame omits the row; an exact frame must match.
        match (member.frame, cap.frame) {
            (None, Some(_)) => {}
            (Some(frozen), Some(live))
                if live.x == frozen.x
                    && live.y == frozen.y
                    && live.w == frozen.w
                    && live.h == frozen.h => {}
            _ => continue,
        }
        let Some(native) = tiler_core::ids::NativeWorkspaceId::parse(
            crate::topology::workspace_key(home_sid).as_str(),
        ) else {
            continue;
        };
        let home_output = match guard.native_context.clone() {
            Some(ctx) if ctx.revision == revision => ctx
                .domains
                .iter()
                .find(|d| d.space == home_sid)
                .filter(|d| d.output.0 == member.out.0 && d.current && d.readable)
                .map(|d| d.output.clone()),
            _ => None,
        };
        let Some(home_output) = home_output else {
            continue;
        };
        out.push(ArrivalBracket {
            window: member.win.clone(),
            obs: cap.obs,
            before: cap.before.clone(),
            location: ArrivalLocation::Native {
                home: Some(tiler_core::world_report::WorkspaceRef::Native {
                    output: home_output,
                    workspace: native,
                }),
            },
            frame: cap.frame,
            after: cap.after.clone(),
        });
    }
    (out, now)
}
/// Owner close-focus route as a callable unit returning the raw
/// `CloseFocusResult` job. Production `app_worker_main` calls this on the
/// SAME owner thread then sends the returned job once on the data lane.
/// Tests call it directly with a scripted fake backend so the counted
/// `dispatch_focus` mock call is exercised, never just the slot take. All
/// fences, diagnostics, and emissions mirror the inlined worker path.
pub(crate) fn handle_close_focus_command(
    backend: &mut dyn AxBackend,
    cmd: &FocusCommand,
    close: &CloseFocusBinding,
    pid: u32,
    generation: u64,
    current_revision: u64,
    shared: &Arc<Mutex<Shared>>,
) -> EngineJob {
    let fresh: Option<(u32, Option<u32>)> = backend.selected_focus_evidence();
    let build = |setter: VisibilitySetter,
                 reason: VisibilityReason,
                 pre: Option<WindowId>,
                 post: Option<WindowId>,
                 foreground_proven: bool,
                 arrival: Vec<ArrivalBracket>,
                 attempt_ms: u64| {
        crate::diag_cause::emit_focus_feedback(
            pid,
            cmd.request_id,
            &close.close_id,
            close.target.0.as_str(),
            setter,
            reason,
            pre.as_ref().map(|w| w.0.as_str()),
            post.as_ref().map(|w| w.0.as_str()),
            foreground_proven,
            attempt_ms,
        );
        EngineJob::CloseFocusResult {
            pid,
            generation,
            cmd_id: cmd.request_id,
            close_id: close.close_id.clone(),
            target: close.target.clone(),
            issued_rev: close.issued_revision,
            adapter_incarnation: close.adapter_incarnation.clone(),
            core_generation: close.core_generation.clone(),
            setter,
            reason,
            pre,
            post,
            foreground_proven,
            arrival,
            attempt_ms,
        }
    };
    match fence_focus_command(cmd, pid, generation, current_revision) {
        FocusFence::Stale => {
            lock_shared(shared).diagnostics.stale_dropped += 1;
            emit(
                shared,
                pid,
                generation,
                cmd.request_id,
                "focus",
                "stale-rejected",
                0,
            );
            build(
                VisibilitySetter::NotCalled,
                VisibilityReason::IdentityStale,
                None,
                None,
                false,
                Vec::new(),
                0,
            )
        }
        FocusFence::Superseded => {
            lock_shared(shared).diagnostics.stale_dropped += 1;
            emit(
                shared,
                pid,
                generation,
                cmd.request_id,
                "focus",
                "revision-superseded",
                0,
            );
            build(
                VisibilitySetter::NotCalled,
                VisibilityReason::Superseded,
                None,
                None,
                false,
                Vec::new(),
                0,
            )
        }
        FocusFence::Dispatch => {
            if !close_focus_is_current(
                shared,
                &close.adapter_incarnation,
                &close.core_generation,
                &close.close_id,
                close.issued_revision,
            ) {
                lock_shared(shared).diagnostics.stale_dropped += 1;
                emit(
                    shared,
                    pid,
                    generation,
                    cmd.request_id,
                    "focus",
                    "superseded",
                    0,
                );
                return build(
                    VisibilitySetter::NotCalled,
                    VisibilityReason::Superseded,
                    None,
                    None,
                    false,
                    Vec::new(),
                    0,
                );
            }
            match gate_close_baseline(
                close.fallback_known,
                close.fallback_pre.as_ref(),
                fresh,
                &close.target,
            ) {
                CloseBaselineGate::AlreadyTarget => {
                    emit(
                        shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "already-satisfied",
                        0,
                    );
                    let proven = Some(close.target.clone());
                    build(
                        VisibilitySetter::NotCalled,
                        VisibilityReason::Empty,
                        proven.clone(),
                        proven,
                        true,
                        Vec::new(),
                        0,
                    )
                }
                CloseBaselineGate::Changed { fresh } => {
                    lock_shared(shared).diagnostics.stale_dropped += 1;
                    emit(
                        shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "superseded-observed",
                        1,
                    );
                    let foreground_proven = fresh.is_some();
                    build(
                        VisibilitySetter::NotCalled,
                        VisibilityReason::ForegroundChanged,
                        close.fallback_pre.clone(),
                        fresh,
                        foreground_proven,
                        Vec::new(),
                        0,
                    )
                }
                CloseBaselineGate::Unproven => {
                    lock_shared(shared).diagnostics.stale_dropped += 1;
                    emit(
                        shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "unproven-read",
                        1,
                    );
                    build(
                        VisibilitySetter::NotCalled,
                        VisibilityReason::FocusUnproven,
                        close.fallback_pre.clone(),
                        None,
                        false,
                        Vec::new(),
                        0,
                    )
                }
                CloseBaselineGate::Proceed { pre } => {
                    // Selection-proof owner gate (frozen intent only): when
                    // the binding carries the frozen arrival set, the setter
                    // fires only on current complete rows — one row per frozen
                    // member (each with current/actual observed home, frame,
                    // visibility, lifetime, and watermark) with the original
                    // stamp still live immediately before the setter. Anything
                    // else returns the internal deferral (never a terminal
                    // no-call, never feedback): the pending intent stays
                    // bound for evidence refresh. Bare closes (`afterShown`
                    // none) dispatch unchanged; any `afterShown` binding
                    // without a live stamp defers.
                    let after_shown = close.after_shown.as_ref().cloned();
                    // Owner-thread arrival brackets for the frozen members:
                    // one ordered bracket batch around the dispatch (facts
                    // only; the core settles from the rows). Read BEFORE the
                    // dispatch; the watermark recheck below withholds rows
                    // that a concurrent invalidation moved mid-dispatch.
                    let (rows, rows_obs) = close_arrival_brackets(
                        backend,
                        close,
                        pid,
                        generation,
                        shared,
                        current_revision,
                    );
                    if let Some(after) = after_shown {
                        let gate_ok = match close.proven_stamp.as_ref() {
                            Some(stamp) => {
                                rows.len() == after.members.len() && {
                                    let guard = lock_shared(shared);
                                    crate::world_dispatch::proven_stamp_live(&guard, stamp)
                                        && guard.clock.get() == rows_obs
                                }
                            }
                            None => false,
                        };
                        if !gate_ok {
                            emit(
                                shared,
                                pid,
                                generation,
                                cmd.request_id,
                                "focus",
                                "deferred-proof",
                                0,
                            );
                            return EngineJob::SelectionFocusDeferred {
                                pid,
                                generation,
                                focus_id: close.close_id.clone(),
                                issued_revision: close.issued_revision,
                                adapter_incarnation: close.adapter_incarnation.clone(),
                                core_generation: close.core_generation.clone(),
                                cmd_id: cmd.request_id,
                                target: close.target.clone(),
                            };
                        }
                    }
                    emit(
                        shared,
                        pid,
                        generation,
                        cmd.request_id,
                        "focus",
                        "dispatched",
                        0,
                    );
                    // Scope this dispatch's native window: drop any idle
                    // periodic accumulation so it is never billed to this
                    // focus attempt.
                    crate::stage_timing::reset_native_agg();
                    let focus_clock = crate::stage_timing::PhaseClock::start();
                    let completion = backend.dispatch_focus(cmd);
                    if crate::stage_timing::enabled() {
                        crate::stage_timing::record_for(
                            shared,
                            pid,
                            generation,
                            cmd.request_id,
                            crate::stage_timing::stage::FOCUS_APPLY,
                            crate::stage_timing::completion_token(&completion),
                            focus_clock.elapsed(),
                        );
                        crate::stage_timing::flush_native_agg(
                            shared,
                            pid,
                            generation,
                            cmd.request_id,
                        );
                    }
                    let (setter, reason) = close_completion_disposition(&completion);
                    let attempt_ms = close_attempt_ms(&completion);
                    crate::diag_cause::emit_focus_done(pid, cmd.request_id, &completion);
                    let post =
                        backend
                            .selected_focus_evidence()
                            .and_then(|(fresh_pid, fresh_wid)| {
                                fresh_wid.map(|wid| WindowId(window_id(fresh_pid, wid)))
                            });
                    let foreground_proven = post.is_some();
                    let arrival = if lock_shared(shared).clock.get() == rows_obs {
                        rows
                    } else {
                        Vec::new()
                    };
                    build(
                        setter,
                        reason,
                        pre,
                        post,
                        foreground_proven,
                        arrival,
                        attempt_ms,
                    )
                }
            }
        }
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
    // Last arrival-capture attempt this owner already made: one bounded
    // evidence-only attempt per immutable request tuple plus watermark,
    // topology revision, context generation, and app generation, never a
    // polling timer or retry.
    let mut last_capture_sent: Option<(String, u64, String, String, u64, u64, u64, u64)> = None;
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
        // Owner arrival capture for the current pending selection intent
        // (native mode only): at the existing multiplex triggers, notice a
        // new pending tuple, watermark, revision, context, or lifetime and
        // make one bounded evidence-only attempt. Never a clock bump,
        // never a timer, never a retry: an attempt (even unreadable, empty,
        // moved, or queue-full) never repeats until a legitimate key change.
        if native_mode {
            let cell = lock_shared(&shared).native_cell.clone();
            maybe_send_arrival_capture(
                &mut *backend,
                pid,
                generation,
                &shared,
                &cell,
                &inbox,
                &mut last_capture_sent,
            );
        }
        // Owner-dispatched focus (native mode only): the latest-wins slot
        // command dispatches here, on the SAME owner that observed the
        // window. Fences mirror the frame path (stop, generation, PID,
        // revision); a refusal publishes a `FocusResult` and never retries.
        if native_mode && let Some(cmd) = focus.take() {
            let current_revision = lock_shared(&shared).topology_revision;
            // Typed close-focus route (`close: Some`): its own owner fence
            // (lifetime plus topology revision), a latest-only epoch check
            // (a null or newer publication cancels this taken-but-unstarted
            // command), and a fresh selected-oracle baseline recheck. EVERY
            // outcome sends exactly one narrowly typed `CloseFocusResult`:
            // a no-call sends `NotCalled` (never `Accepted` without an owner
            // attempt); a dispatch sends the honest mechanical disposition.
            // Legacy explicit/deferred commands (`close: None`) dispatch
            // unchanged in the `else` below.
            if let Some(close) = cmd.close.clone() {
                // Typed close-focus route via the callable owner unit: the
                // SAME owner thread performs the fresh baseline recheck and
                // the counted dispatch, returning the raw result job. One
                // data-lane send here, never inside the helper.
                let job = handle_close_focus_command(
                    &mut *backend,
                    &cmd,
                    &close,
                    pid,
                    generation,
                    current_revision,
                    &shared,
                );
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
            } else {
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
                                let current = crate::ffi::ax_selected_focus_evidence()
                                    .ok()
                                    .and_then(|evidence| {
                                        u32::try_from(evidence.pid)
                                            .ok()
                                            .map(|pid| (pid, evidence.wid))
                                    });
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
                            // for the next pass, so the drain watermark does
                            // not move; the mark still proves the send count.
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
                        // Scope this dispatch's native window (see above):
                        // idle accumulation never bills to this attempt.
                        crate::stage_timing::reset_native_agg();
                        let focus_clock = crate::stage_timing::PhaseClock::start();
                        let completion = backend.dispatch_focus(&cmd);
                        if crate::stage_timing::enabled() {
                            crate::stage_timing::record_for(
                                &shared,
                                pid,
                                generation,
                                cmd.request_id,
                                crate::stage_timing::stage::FOCUS_APPLY,
                                crate::stage_timing::completion_token(&completion),
                                focus_clock.elapsed(),
                            );
                            crate::stage_timing::flush_native_agg(
                                &shared,
                                pid,
                                generation,
                                cmd.request_id,
                            );
                        }
                        crate::diag_cause::emit_focus_done(pid, cmd.request_id, &completion);
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
        // Scope this batch's native window: drop any idle periodic
        // accumulation so it is never billed to this batch.
        crate::stage_timing::reset_native_agg();
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
            let apply_clock = crate::stage_timing::PhaseClock::start();
            let completion = backend.apply_frame(cmd);
            if crate::stage_timing::enabled() {
                crate::stage_timing::record_for(
                    &shared,
                    pid,
                    generation,
                    cmd.cmd_id,
                    crate::stage_timing::stage::FRAME_APPLY,
                    crate::stage_timing::completion_token(&completion),
                    apply_clock.elapsed(),
                );
            }
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
        // failures preserve retained state. The timer wraps the whole
        // readback (one aggregate record per batch, never per window: the
        // backend-internal per-window scan stays unlogged).
        let observe_clock = crate::stage_timing::PhaseClock::start();
        let mut readback_ok = true;
        if native_mode {
            match backend.snapshot_scoped() {
                Ok(snapshot) => {
                    let mut snapshot = snapshot;
                    stamp_owner_focus(&mut snapshot, lock_shared(&shared).focus_evidence.clone());
                    stage_scoped(&last_scoped, &mut pending_scoped, snapshot);
                }
                Err(reason) => {
                    readback_ok = false;
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
                    readback_ok = false;
                    note_observation_failure(&shared, pid, generation, seq + 1, &reason);
                }
            }
        }
        if crate::stage_timing::enabled() {
            let observe_outcome: &'static str = if readback_ok { "snapshot" } else { "failed" };
            crate::stage_timing::record_for(
                &shared,
                pid,
                generation,
                cmds.first().map(|cmd| cmd.cmd_id).unwrap_or(0),
                crate::stage_timing::stage::OBSERVE,
                observe_outcome,
                observe_clock.elapsed(),
            );
            crate::stage_timing::flush_native_agg(
                &shared,
                pid,
                generation,
                cmds.first().map(|cmd| cmd.cmd_id).unwrap_or(0),
            );
            // Post-batch observed readback total: input-to-post-batch
            // snapshot-ready per unique parent trace in this batch. The
            // `AxResult`s above queue before this snapshot, and
            // redundant-skipped commands emit no `AxResult` at all, so this
            // endpoint - not the settle total - carries their ready total.
            // One clock read, no new syscall/read; aggregate emit only.
            // Dispatch success stays distinct from convergence/paint:
            // readiness only, never target equality.
            let readback_outcome: &'static str = if readback_ok { "snapshot" } else { "failed" };
            let readback_now = crate::stage_timing::now_us();
            let mut seen_traces: Vec<u64> = Vec::new();
            for cmd in cmds.iter() {
                if let Some((trace, t0, _)) =
                    crate::stage_timing::trace_for_cmd(pid, generation, cmd.cmd_id)
                    && !seen_traces.contains(&trace)
                {
                    seen_traces.push(trace);
                    crate::stage_timing::record(
                        &shared,
                        trace,
                        crate::stage_timing::stage::READBACK_TOTAL,
                        readback_outcome,
                        readback_now.saturating_sub(t0),
                    );
                }
            }
            let mut traces_csv = String::new();
            for (index, trace) in seen_traces.iter().enumerate() {
                if index > 0 {
                    traces_csv.push(',');
                }
                traces_csv.push_str(&trace.to_string());
            }
            crate::diag_cause::emit_batch_done(
                pid,
                seq,
                cmds.first().map(|cmd| cmd.cmd_id).unwrap_or(0),
                cmds.len() as u64,
                readback_outcome,
                &traces_csv,
            );
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

#[cfg(test)]
mod arrival_capture_dedupe_tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use std::time::Instant;

    use super::*;
    use crate::runtime_backend::AxBackend;
    use crate::world_sequencer::ObservationClock;

    struct ScriptBackend {
        reads: Vec<BracketRead>,
        calls: usize,
    }

    impl AxBackend for ScriptBackend {
        fn apply_frame(
            &mut self,
            _cmd: &crate::runtime_types::FrameCommand,
        ) -> crate::runtime_types::AxCompletion {
            crate::runtime_types::AxCompletion::Failed {
                reason: "unused".to_owned(),
            }
        }

        fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
            Ok(Vec::new())
        }

        fn capture_brackets(&mut self, wids: &[u32]) -> Vec<BracketRead> {
            self.calls += 1;
            self.reads
                .iter()
                .filter(|r| wids.contains(&r.wid))
                .cloned()
                .collect()
        }
    }

    fn good_read() -> BracketRead {
        BracketRead {
            wid: 7,
            owner_pid: Some(4242),
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces: vec![3],
            frame: Some((10, 20, 300, 200)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }
    }

    fn pending_for(adapter: &str, core: &str) -> crate::runtime_shared::DeferredSelectionFocus {
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::world_report::{AfterShown, AfterShownMember, AfterShownScope};
        let out = OutputId("macos-display-1".to_owned());
        let member = AfterShownMember {
            win: WindowId("4242_7".to_owned()),
            out: out.clone(),
            ws: "ws-a".to_owned(),
            frame: None,
        };
        let scope = AfterShownScope {
            out: out.clone(),
            ws: "ws-a".to_owned(),
        };
        crate::runtime_shared::DeferredSelectionFocus {
            operation_id: "op-3".to_owned(),
            command_seq: 3,
            focus_id: "focus-3".to_owned(),
            issued_revision: 3,
            adapter_incarnation: adapter.to_owned(),
            core_generation: core.to_owned(),
            window: WindowId("4242_7".to_owned()),
            output: out,
            workspace: "ws-a".to_owned(),
            after_shown: AfterShown {
                op: "op-3".to_owned(),
                obs: 0,
                members: vec![member],
                views: vec![scope.clone()],
                selected: vec![scope],
                departing: Vec::new(),
            },
            route: crate::focus_route::FocusRoute::Slps,
            expected_sid: 3,
            app_generation: Some(1),
            prior_focus: Some("4242_7".to_owned()),
            prior_frontmost: Some(4242),
            proven_stamp: None,
            owner_queued: false,
        }
    }

    fn test_shared(pending: crate::runtime_shared::DeferredSelectionFocus) -> Arc<Mutex<Shared>> {
        let shared = Arc::new(Mutex::new(Shared {
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
            select_wake: None,
            main_wake: None,
            topology_revision: 9,
            native_context: None,
            native_cell: NativeContextCell::new(),
            select_pending: VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: Some(pending),
            arrival_captures: BTreeMap::new(),
            selection_focus_floor_seq: 0,
            close_focus_current: None,
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
            log: VecDeque::new(),
            test_wake: None,
            test_seq: 0,
            last_desired: Vec::new(),
            last_geometry_key: None,
            last_presentation: crate::runtime::empty_world_presentation_v4(),
            last_pager_bindings: Vec::new(),
            last_workspace_desires: Vec::new(),
            last_core_generation: String::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }));
        shared
    }

    #[test]
    fn static_repeat_captures_once() {
        let shared = test_shared(pending_for("adapter-1", "gen-1"));
        let cell = lock_shared(&shared).native_cell.clone();
        let inbox = Arc::new(Inbox::new(8));
        let mut backend = ScriptBackend {
            reads: vec![good_read()],
            calls: 0,
        };
        let mut last = None;
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert!(last.is_some());
        assert_eq!(backend.calls, 1);
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 1, "repeat never re-reads native legs");
    }

    #[test]
    fn empty_capture_attempts_once() {
        let shared = test_shared(pending_for("adapter-1", "gen-1"));
        let cell = lock_shared(&shared).native_cell.clone();
        let inbox = Arc::new(Inbox::new(8));
        let mut backend = ScriptBackend {
            reads: Vec::new(),
            calls: 0,
        };
        let mut last = None;
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert!(last.is_some(), "empty attempt still records the key");
        assert_eq!(backend.calls, 1);
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 1, "empty never retries on later multiplex");
    }

    #[test]
    fn full_queue_capture_attempts_once() {
        let shared = test_shared(pending_for("adapter-1", "gen-1"));
        let cell = lock_shared(&shared).native_cell.clone();
        let inbox = Arc::new(Inbox::new(0));
        let mut backend = ScriptBackend {
            reads: vec![good_read()],
            calls: 0,
        };
        let mut last = None;
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert!(last.is_some(), "full-queue attempt still records the key");
        assert_eq!(backend.calls, 1);
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(
            backend.calls, 1,
            "full queue never retries on later multiplex"
        );
    }

    #[test]
    fn context_change_same_revision_allows_fresh() {
        let shared = test_shared(pending_for("adapter-1", "gen-1"));
        let cell = lock_shared(&shared).native_cell.clone();
        let inbox = Arc::new(Inbox::new(8));
        let mut backend = ScriptBackend {
            reads: vec![good_read()],
            calls: 0,
        };
        let mut last = None;
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 1);
        cell.request_reread();
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(
            backend.calls, 2,
            "context generation change allows one fresh"
        );
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 2);
    }

    #[test]
    fn tuple_change_allows_fresh() {
        let shared = test_shared(pending_for("adapter-1", "gen-1"));
        let cell = lock_shared(&shared).native_cell.clone();
        let inbox = Arc::new(Inbox::new(8));
        let mut backend = ScriptBackend {
            reads: vec![good_read()],
            calls: 0,
        };
        let mut last = None;
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 1);
        lock_shared(&shared)
            .selection_focus_pending
            .as_mut()
            .expect("pending")
            .core_generation = "gen-2".to_owned();
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 2, "core generation change allows one fresh");
        lock_shared(&shared)
            .selection_focus_pending
            .as_mut()
            .expect("pending")
            .adapter_incarnation = "adapter-2".to_owned();
        assert!(maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(
            backend.calls, 3,
            "adapter incarnation change allows one fresh"
        );
        assert!(!maybe_send_arrival_capture(
            &mut backend,
            4242,
            1,
            &shared,
            &cell,
            &inbox,
            &mut last
        ));
        assert_eq!(backend.calls, 3);
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
        ScopedObservation, ShutdownOutcome, SubmitOutcome, WindowSizeHints,
    };
    use crate::runtime_backend::{AxBackend, AxBackendFactory, NativeInputStub};
    use crate::runtime_types::ScopedWindowMeta;
    use crate::runtime_types::{ScopedBackendSnapshot, TopologyMode};

    const PID: u32 = 4242;
    const SETTLED: Duration = Duration::from_secs(5);

    struct GateState {
        applied: Vec<(String, (i32, i32, i32, i32))>,
        entered: u64,
        open: bool,
        windows: Vec<ObservedWindow>,
        meta: Vec<ScopedWindowMeta>,
        revision: u64,
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

        fn snapshot_scoped(&mut self) -> Result<ScopedBackendSnapshot, String> {
            // Fixture-only scoped read: actual state windows plus the
            // handbuilt ordinary neutral meta and the published topology
            // revision. Never a live SLS/CG read; the resolver is scripted
            // at `update_topology_with`. The oracle is complete over the
            // fixture's own inventory (sorted owner wids), so a removal the
            // test submits proves absence instead of retaining unknown.
            let guard = self.state.0.lock().expect("mutex");
            let mut owner_wids: Vec<u32> = guard.windows.iter().map(|w| w.wid).collect();
            owner_wids.sort_unstable();
            Ok(ScopedBackendSnapshot {
                windows: guard.windows.clone(),
                focused_wid: None,
                focus_cg_owned: true,
                focus_evidence: None,
                sys_focus: None,
                global_focus: None,
                meta: guard.meta.clone(),
                excluded: 0,
                excluded_reasons: Vec::new(),
                cg_complete: true,
                cg_owner_wids: owner_wids,
                cg_alive: Vec::new(),
                revision: guard.revision,
            })
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
                meta: Vec::new(),
                revision: 0,
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

    /// Single-display native fixture topology: one 1440x900 display whose
    /// SLS row homes the single Space `SID` as current, so the derived
    /// context actuates exactly that domain with work area (0,0,1440,900).
    const SID: u64 = 277;
    const UUID: &str = "37AC4E19-0000-4000-8000-123456789ABC";

    fn scoped_topology() -> crate::topology::TopologySnapshot {
        use crate::ffi::{DisplayRect, NsDisplay, SlsDisplayRow};
        let ns = vec![NsDisplay {
            cg_id: 1,
            uuid: Some(UUID.to_owned()),
            frame: DisplayRect {
                x: 0.0,
                y: 0.0,
                w: 1440.0,
                h: 900.0,
            },
            visible: DisplayRect {
                x: 0.0,
                y: 0.0,
                w: 1440.0,
                h: 900.0,
            },
            scale: 2.0,
        }];
        let rows = vec![SlsDisplayRow {
            identifier: Some(UUID.to_owned()),
            spaces: vec![SID],
            current: Some(SID),
        }];
        crate::topology::build_snapshot(&ns, &rows, Some(false))
    }

    /// Ordinary neutral raw facts for one fixture window, homed to the
    /// fixture Space: `AXWindow`/`AXStandardWindow` classifies Normal,
    /// non-modal, resizable, layer 0, regular policy — the sanctioned
    /// roster shape, so the core holds nothing.
    fn scoped_meta(wid: u32) -> ScopedWindowMeta {
        ScopedWindowMeta {
            wid,
            spaces: vec![SID],
            fullscreen: false,
            minimized: false,
            hidden: false,
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        }
    }

    /// Native-mode gate: same GateBackend fixture, but the runtime runs the
    /// native topology (scripted `User` Space types, no live SLS reads) and
    /// the backend serves scoped snapshots with the ordinary meta above.
    /// `windows` preload the fixture truth before launch, so the worker's
    /// startup snapshot (sequence 1) already carries them.
    fn start_scoped_gate(
        windows: Vec<ObservedWindow>,
        open: bool,
    ) -> (
        crate::runtime::RuntimeHandle,
        Arc<(Mutex<GateState>, Condvar)>,
    ) {
        let snapshot = scoped_topology();
        let revision = snapshot.revision;
        let meta = windows.iter().map(|w| scoped_meta(w.wid)).collect();
        let state = Arc::new((
            Mutex::new(GateState {
                applied: Vec::new(),
                entered: 0,
                open,
                windows,
                meta,
                revision,
                shutdown_open: true,
                shutdown_entered: 0,
            }),
            Condvar::new(),
        ));
        let config = RuntimeConfig {
            topology_mode: TopologyMode::Native,
            ..RuntimeConfig::default()
        };
        let handle = Runtime::start(
            config,
            Arc::new(GateFactory {
                state: Arc::clone(&state),
            }),
            Arc::new(NativeInputStub),
        )
        .expect("runtime starts offline");
        handle.update_topology_with(snapshot, &|_| Some(crate::ffi::SlsSpaceType::User));
        // The worker publishes its startup snapshot as sequence 1, so the
        // test owns the sequence space from 2 on.
        (handle, state)
    }

    /// Native-mode observation submit: routes to the current fixture domain
    /// with the ordinary meta, the published revision, and no focus legs
    /// (focus stays unfocused, never guessed). Mirrors the fixture
    /// `snapshot_scoped` fields so worker readbacks compare equal; the
    /// oracle is complete over the submitted inventory (sorted owner
    /// wids), so a submitted removal proves absence.
    fn submit_scoped(
        handle: &crate::runtime::RuntimeHandle,
        pid: u32,
        generation: u64,
        seq: u64,
        windows: Vec<ObservedWindow>,
        revision: u64,
    ) -> SubmitOutcome {
        let meta = windows.iter().map(|w| scoped_meta(w.wid)).collect();
        let mut owner_wids: Vec<u32> = windows.iter().map(|w| w.wid).collect();
        owner_wids.sort_unstable();
        handle.submit_scoped_observation(ScopedObservation {
            pid,
            generation,
            seq,
            windows,
            focused_wid: None,
            revision,
            frontmost_pid: None,
            ax_focus: None,
            cg_owned: true,
            meta,
            cg_complete: true,
            cg_owner_wids: owner_wids,
        })
    }

    fn scoped_revision(state: &Arc<(Mutex<GateState>, Condvar)>) -> u64 {
        state.0.lock().expect("mutex").revision
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
        let (handle, state) = start_scoped_gate(Vec::new(), false);
        let generation = handle.launch_app(PID).expect("launch");
        let revision = scoped_revision(&state);
        assert_eq!(
            submit_scoped(
                &handle,
                PID,
                generation,
                2,
                vec![plain(7, 0, 0, 960, 1080)],
                revision
            ),
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
        // Single-window plan on the fixture work area (0,0,1440,900) with
        // Single-window plan on the fixture work area (0,0,1440,900) with
        // the Phase 1 outer gap (8 each side): (8,8,1424,884).
        let windows = vec![plain(7, 8, 8, 1424, 884)];
        let (handle, state) = start_scoped_gate(windows.clone(), true);
        let generation = handle.launch_app(PID).expect("launch");
        let revision = scoped_revision(&state);
        assert_eq!(
            submit_scoped(&handle, PID, generation, 2, windows, revision),
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
        let (handle, state) = start_scoped_gate(Vec::new(), false);
        let generation = handle.launch_app(PID).expect("launch");
        let revision = scoped_revision(&state);
        assert_eq!(
            submit_scoped(
                &handle,
                PID,
                generation,
                2,
                vec![plain(7, 0, 0, 960, 1080), plain(8, 960, 0, 960, 1080)],
                revision
            ),
            SubmitOutcome::Accepted
        );
        assert!(handle.flush(Duration::from_secs(5)));
        // Parked inside the taken batch: the next plan supersedes the rest.
        assert!(wait_entered(&state, 1), "worker never entered actuation");
        assert_eq!(
            submit_scoped(
                &handle,
                PID,
                generation,
                3,
                vec![plain(7, 0, 0, 960, 1080)],
                revision
            ),
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
        let (handle, state) = start_scoped_gate(Vec::new(), false);
        hold_shutdown(&state);
        let generation = handle.launch_app(PID).expect("launch");
        let revision = scoped_revision(&state);
        assert_eq!(
            submit_scoped(
                &handle,
                PID,
                generation,
                2,
                vec![plain(7, 0, 0, 960, 1080)],
                revision
            ),
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
                        meta: Vec::new(),
                        revision: 0,
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
