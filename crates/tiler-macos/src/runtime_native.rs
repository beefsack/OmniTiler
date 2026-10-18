#![forbid(unsafe_code)]

//! Native-mode observation ingest over the world route (implementation).
//!
//! Split from [`crate::runtime_thread`]: scoped observation admission,
//! retained homing/focus bookkeeping, and the owner focus fence. Planning
//! itself is one complete world report per job (see
//! [`crate::world_runtime`]): per-domain core sessions over actual work
//! areas, domain retirement on revision drift, and multi-reply emission
//! are retired with the legacy path. No threads are spawned here: the
//! engine owner calls in; owners never block on AX.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use tiler_core::directional::WindowId;
use tiler_core::session::DomainKey;

use crate::focus_route::global_focus_agrees;
use crate::runtime::{
    FocusRequest, NativeContext, ObservedWindow, Shared, emit, lock_shared, window_id,
};
use crate::runtime_core::{
    RetainedApp, RetainedCore, clear_focus_into, note_scoped_members, partition_scoped_members,
};
use crate::runtime_types::{FocusCommand, ScopedBackendSnapshot};

/// Pure fence for one taken focus command: PID/generation staleness
/// first (a relaunched lifetime must never dispatch), then topology
/// revision drift (a display/Space change mid-plan supersedes). Only
/// `Dispatch` may call the backend; the worker counts and emits the rest
/// without dispatching and never retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusFence {
    Stale,
    Superseded,
    Dispatch,
}

pub(crate) fn fence_focus_command(
    cmd: &FocusCommand,
    pid: u32,
    generation: u64,
    current_revision: u64,
) -> FocusFence {
    if cmd.generation != generation || cmd.pid != pid {
        FocusFence::Stale
    } else if cmd.revision != current_revision {
        FocusFence::Superseded
    } else {
        FocusFence::Dispatch
    }
}

/// Handle one native scoped observation job: admission fences mirror
/// the legacy arm (lifetime, termination, sequence, context revision);
/// membership-scoped planning converges through one complete world report.
/// Only exactly-homed retained-domain members reach the engine (current
/// members actuate, hidden members reconcile silently, unreadable scopes
/// hold without write authority); everything else omits (counted, never
/// a close report). Snapshot drift (revision mismatch with the shared
/// context) supersedes instead of planning.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_scoped_observation(
    session: &mut crate::world_runtime::WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<crate::runtime_thread::Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    pid: u32,
    generation: u64,
    seq: u64,
    snapshot: ScopedBackendSnapshot,
) {
    // Admission fences mirror the legacy observation arm (lifetime,
    // termination, sequence): only the membership-scoped planning
    // below is native.
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
    // Context fence: the snapshot's revision must equal the live
    // shared revision with a derived context present. Drift
    // supersedes (the next snapshot under the new revision
    // converges); it never plans stale domains. Refusals mark the
    // coverage gap so the next report publishes incomplete inventory.
    let ctx: Option<Arc<NativeContext>> = lock_shared(shared).native_context.clone();
    let Some(ctx) = ctx else {
        session.mark_coverage_gap();
        lock_shared(shared).diagnostics.stale_dropped += 1;
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "no-context-rejected",
            0,
        );
        return;
    };
    if snapshot.revision != ctx.revision {
        session.mark_coverage_gap();
        lock_shared(shared).diagnostics.stale_dropped += 1;
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "stale-revision-rejected",
            0,
        );
        return;
    }
    // Member partition by live Space membership (never geometry):
    // only exactly-homed retained-domain windows reach the engine
    // (current members actuate, hidden members reconcile silently).
    // Omission is counted evidence, never destruction.
    let (mut members, omitted) = partition_scoped_members(&snapshot.windows, &snapshot.meta, &ctx);
    if omitted > 0 {
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "non-domain-omitted",
            omitted as u64,
        );
    }
    if snapshot.excluded > 0 {
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "classifier-omitted",
            snapshot.excluded as u64,
        );
        // Per-reason breakdown, diagnostic only (wid counts per status
        // name; never titles/content): names which classifier gate fired
        // so a persistent omitted/applied gap is attributable from the
        // log without a live re-read. Fixed variant order keeps the lines
        // deterministic; the aggregate token above is unchanged.
        for reason in [
            crate::classifier::ExcludeReason::OwnBundle,
            crate::classifier::ExcludeReason::AccessoryPolicy,
            crate::classifier::ExcludeReason::ProhibitedPolicy,
            crate::classifier::ExcludeReason::Minimized,
            crate::classifier::ExcludeReason::HiddenApp,
            crate::classifier::ExcludeReason::NonordinaryLayer,
            crate::classifier::ExcludeReason::UnknownRole,
        ] {
            let count = snapshot
                .excluded_reasons
                .iter()
                .filter(|o| o.reason == reason)
                .count();
            if count > 0 {
                emit(
                    shared,
                    pid,
                    generation,
                    seq,
                    "observation",
                    reason.log_token(),
                    count as u64,
                );
            }
        }
    }
    // Blind-hidden preservation: live AX reads return no windows for apps
    // whose windows all sit on hidden Spaces (successful empty snapshots),
    // so such a snapshot says nothing about still-known hidden domains and
    // must not read as mass removal (the core would converge by retiring
    // the retained tree, scrambling per-window slots on return). Preserve
    // this PID's retained rows homed to still-known non-current domains.
    // Observed windows always re-home below, so a natively moved window
    // still follows its observed membership; retired Spaces still drop via
    // the live-context fence; current domains stay authoritative-loss, so a
    // genuinely emptied current Space still converges to empty. A
    // relaunched lifetime preserves nothing. A close-while-hidden is
    // observationally identical to this blindness, so hidden-close stays
    // deliberately non-authoritative: it converges on the next observation
    // covering the domain (when the Space becomes current again), never by
    // immediate pruning of an uncovered domain.
    let mut preserved: Vec<(DomainKey, ObservedWindow)> = Vec::new();
    {
        let same_lifetime = retained
            .apps
            .get(&pid)
            .is_some_and(|app| app.generation == generation);
        if same_lifetime && let Some(app) = retained.apps.get(&pid) {
            let observed_ids: BTreeSet<String> = snapshot
                .windows
                .iter()
                .map(|w| window_id(pid, w.wid))
                .collect();
            for w in &app.windows {
                let id = window_id(pid, w.wid);
                if observed_ids.contains(&id) {
                    continue;
                }
                let Some(key) = retained.window_domains.get(&id) else {
                    continue;
                };
                if ctx.is_current(key) {
                    continue;
                }
                let known = ctx
                    .domains
                    .iter()
                    .any(|d| d.output == key.output && d.workspace == key.workspace);
                if !known {
                    continue;
                }
                preserved.push((key.clone(), *w));
            }
        }
    }
    for (key, w) in preserved {
        members.entry(key).or_default().push(w);
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
    // Retained windows are members only: the wire, the hold filter, and
    // close computation all see exactly what the domains plan (nothing
    // else can leak into a domain report). The borrow ends here so focus
    // bookkeeping below can hold `retained` again.
    let member_windows: Vec<ObservedWindow> = members.values().flatten().copied().collect();
    let member_count = member_windows.len() as u64;
    entry.windows = member_windows;
    // Global-focus truth gate on the worker-stamped evidence: the
    // app-local window promotes only on full workspace+AX+CG agreement AND
    // current-domain membership (a hidden-Space window can never hold
    // global focus: the user is looking at the current Space). Anything
    // else arrives unfocused (authoritative loss for exactly this PID when
    // it previously held focus).
    let candidate = match snapshot.focused_wid {
        Some(wid)
            if members.iter().any(|(key, windows)| {
                ctx.is_current(key) && windows.iter().any(|w| w.wid == wid)
            }) =>
        {
            snapshot.focus_evidence.as_ref().and_then(|evidence| {
                global_focus_agrees(
                    pid,
                    wid,
                    evidence.frontmost_pid,
                    evidence.ax_focus,
                    snapshot.focus_cg_owned,
                )
                .then_some(wid)
            })
        }
        _ => None,
    };
    entry.focused_wid = candidate;
    // `entry` is not used below: the borrow ends here (NLL), so
    // focus bookkeeping and member mapping may hold `retained`.
    match candidate {
        Some(wid) => {
            retained.observed_focus = Some(WindowId(window_id(pid, wid)));
        }
        None => clear_focus_into(retained, pid),
    }
    // Dispatch context first (fresh known domains for home validity),
    // then minimized lifetimes from this snapshot's exclusions under
    // PREVIOUS homing (the mapping update below prunes this PID first),
    // then the homing update itself.
    if crate::runtime_core::note_dispatch_context(retained, &ctx) {
        emit(
            shared,
            pid,
            generation,
            seq,
            "domain",
            "topology-adopted",
            ctx.domains.len() as u64,
        );
    }
    // Minimized lifetimes refresh from this snapshot's classifier
    // exclusions (last-known homing above); unminimized windows drop out.
    crate::world_runtime::refresh_minimized(retained, pid, &snapshot);
    note_scoped_members(retained, pid, &members);
    emit(
        shared,
        pid,
        generation,
        seq,
        "observation",
        "applied",
        member_count,
    );
    session.clear_coverage_gap();
    crate::world_dispatch::dispatch_report(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
        std::collections::BTreeMap::new(),
        None,
        false,
        crate::world_command_map::OP_RECONCILE,
        pid,
        generation,
        seq,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus_route::FocusRoute;

    fn focus_cmd(pid: u32, generation: u64, revision: u64) -> FocusCommand {
        FocusCommand {
            pid,
            generation,
            request_id: 1,
            wid: 7,
            window: format!("{pid}_7"),
            revision,
            route: FocusRoute::Slps,
        }
    }

    #[test]
    fn focus_fence_matrix() {
        // Fresh command dispatches.
        assert_eq!(
            fence_focus_command(&focus_cmd(9, 1, 3), 9, 1, 3),
            FocusFence::Dispatch
        );
        // Relaunched lifetime (generation or PID drift): stale, never
        // dispatch — a stale request can never fire after relaunch.
        assert_eq!(
            fence_focus_command(&focus_cmd(9, 1, 3), 9, 2, 3),
            FocusFence::Stale
        );
        assert_eq!(
            fence_focus_command(&focus_cmd(8, 1, 3), 9, 1, 3),
            FocusFence::Stale
        );
        // Display/Space change mid-plan: superseded, never dispatched.
        assert_eq!(
            fence_focus_command(&focus_cmd(9, 1, 3), 9, 1, 4),
            FocusFence::Superseded
        );
    }
}
