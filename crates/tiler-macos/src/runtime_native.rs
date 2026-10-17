#![forbid(unsafe_code)]

//! Native-mode engine planning (implementation).
//!
//! Split from [`crate::runtime_thread`]: per-domain core sessions over
//! actual work areas, domain retirement on revision drift, merged
//! multi-reply emission, scoped observation handling, and the owner focus
//! fence. Synthetic single-domain planning stays in [`crate::runtime_thread`];
//! both share emission through the helpers re-exported there. No threads are
//! spawned here: the engine owner calls in; owners never block on AX.

use std::sync::{Arc, Mutex};

use tiler_core::{
    boundary::{CoreCommand, CoreEvent, CoreReply},
    directional::WindowId,
    engine::Engine,
    ids::{CorrelationId, GenerationId, OwnerId},
    session::DomainKey,
};

use crate::focus_route::global_focus_agrees;
use crate::runtime::{
    FocusRequest, NativeContext, ObservedWindow, PHASE1_GENERATION, PHASE1_OUTER_GAP, PHASE1_OWNER,
    Shared, emit, lock_shared, window_id,
};
use crate::runtime_core::{
    RetainedApp, RetainedCore, build_native_event, clear_focus_into, domain_member_apps,
    native_domain_views, note_scoped_members, partition_scoped_members,
};
use crate::runtime_thread::{MergedPlan, apply_merged, collect_reply, reply_kind};
use crate::runtime_types::{FocusCommand, ScopedBackendSnapshot};

/// Parked-display check for retirement: a known domain whose output still
/// names a display the current context skipped (unprovable homing or
/// geometry — never absence) is NOT retired. Its engine session, bounds
/// record, and revision cursor stay parked with no views, no reconciles,
/// and no effects until fresh evidence resumes the same key. A truly
/// removed display appears in neither domains nor `skipped`, so its keys
/// still release normally.
pub(crate) fn is_parked_display(ctx: &NativeContext, key: &DomainKey) -> bool {
    ctx.skipped
        .iter()
        .any(|skipped| key.output.0 == crate::topology::output_key(skipped.cg_id))
}
/// Base revision carried by a core reply (`None` for rejections, releases,
/// and read-only resolutions, which advance no cursor).
pub(crate) fn reply_base_revision(reply: &CoreReply) -> Option<u64> {
    match reply {
        CoreReply::Projection(plan) => Some(plan.base_revision),
        CoreReply::Tiled(plan) => Some(plan.base_revision),
        CoreReply::SendWorkspace(plan) | CoreReply::SendOutput(plan) => Some(plan.base_revision),
        CoreReply::MigrateWorkspace(plan) => Some(plan.base_revision),
        CoreReply::MoveDirectional(plan) => Some(plan.base_revision),
        CoreReply::FocusDirectional(plan) => Some(plan.base_revision),
        CoreReply::Resize(plan) => Some(plan.base_revision),
        CoreReply::ActiveGroup(found) => Some(found.base_revision),
        CoreReply::NoGroup { base_revision, .. } => *base_revision,
        CoreReply::DragPreview(plan) => Some(plan.base_revision),
        CoreReply::Released
        | CoreReply::Rejected { .. }
        | CoreReply::SnapshotInvalid { .. }
        | CoreReply::Diverged(_) => None,
    }
}

/// Count one merged native batch: each domain reply under its own
/// `{op}-{kind}` token, so per-domain refusals stay visible instead of
/// collapsing into a single merged count.
fn count_native_replies(
    shared: &Arc<Mutex<Shared>>,
    handled: &[(DomainKey, CoreReply)],
    op: &'static str,
) {
    let mut guard = lock_shared(shared);
    for (_, reply) in handled {
        guard.count_reply(&format!("{op}-{}", reply_kind(reply)));
    }
}

/// Plan one native scoped observation across every known tile domain:
/// retire truly-vanished domains on revision drift, converge each current
/// domain with actuation, and reconcile each hidden domain without frames
/// or focus. Only exactly-homed retained-domain members reach the engine;
/// everything else omits (counted, never a close report). Snapshot drift
/// (revision mismatch with the shared context) supersedes instead of
/// planning.
///
/// Retirement fires only for domains the new context no longer knows at
/// all — a deleted Space, a removed display, or a Space that newly proves
/// non-user. A merely unreadable type read retains hidden, and a display
/// the new context skips (unprovable homing/geometry, still present)
/// parks its sessions with no effects instead of releasing. A mere
/// current-Space switch keeps the old Space as
/// a hidden domain, so its independent core session and layout survive
/// untouched until the user returns. Geometry-only drift retires nothing
/// (keys persist).
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_native_observation(
    engine: &mut Engine,
    retained: &mut RetainedCore,
    ctx: &std::sync::Arc<NativeContext>,
    views: &[crate::runtime_core::NativeDomainView],
    closed: Vec<WindowId>,
    command: CoreCommand,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    // Revision drift retires truly-vanished domains FIRST: keys the new
    // context no longer knows at all (deleted Space, removed display, or
    // a Space that newly proves non-user) are released (their sessions
    // removed with zero native writes) before any planning. Keys parked
    // on a still-present but skipped display (see `is_parked_display`)
    // are excluded: unprovable homing or geometry parks with no views,
    // no reconciles, and no effects — never a release. A mere
    // current-Space switch keeps the old Space as a hidden domain,
    // so its session and layout survive — never stranded, never reseeded.
    // Geometry-only drift retires nothing (keys persist).
    if retained.native_revision != ctx.revision {
        let retired: Vec<DomainKey> = retained
            .known_domains
            .keys()
            .filter(|key| {
                !views.iter().any(|view| &view.key == *key) && !is_parked_display(ctx, key)
            })
            .cloned()
            .collect();
        for key in retired {
            let domain = retained
                .known_domains
                .get(&key)
                .cloned()
                .unwrap_or(crate::runtime_core::retired_domain_placeholder(&key));
            let event = CoreEvent {
                owner: OwnerId::parse(PHASE1_OWNER).expect("release owner token"),
                generation: GenerationId::parse(PHASE1_GENERATION)
                    .expect("release generation token"),
                correlation: CorrelationId::parse(&format!(
                    "phase-1-release-{}",
                    retained.correlation
                ))
                .expect("release correlation token"),
                revision: retained.domain_revisions.get(&key).copied().unwrap_or(0),
                fingerprint: 0,
                domain,
                domain_key: key.clone(),
                outer_gap: PHASE1_OUTER_GAP,
                focused_window: WindowId(String::new()),
                windows: Vec::new(),
                closed_windows: Vec::new(),
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                command: CoreCommand::ReleaseDomain,
            };
            let _ = engine.handle(&event);
            retained.known_domains.remove(&key);
            retained.domain_revisions.remove(&key);
            emit(shared, pid, generation, seq, "domain", "retired", 0);
        }
        retained.native_revision = ctx.revision;
    }
    for view in views {
        retained
            .known_domains
            .insert(view.key.clone(), view.domain.clone());
    }
    // One event per domain over that domain's FULL retained membership
    // (every app, not just the reporter — a partial observation must never
    // read as mass removal). Focused override names the global focus only
    // inside its own domain; every other domain converges focus-none so a
    // foreign focus can never leak across domains. Current domains merge
    // into one emission (frames plus at most one focus dispatch); hidden
    // domains converge silently (cursor advance only — never frames, never
    // focus, never desired-geometry writes into an off-current Space).
    let predicate = engine.fixed_size_predicate();
    let mut handled: Vec<(DomainKey, CoreReply)> = Vec::with_capacity(views.len());
    let mut hidden: Vec<(DomainKey, CoreReply)> = Vec::new();
    for view in views {
        let member_apps = domain_member_apps(retained, &view.key);
        let focused_override = match &retained.observed_focus {
            Some(focus)
                if retained
                    .window_domains
                    .get(&focus.0)
                    .is_some_and(|k| k == &view.key) =>
            {
                Some(focus.clone())
            }
            _ => Some(WindowId(String::new())),
        };
        let revision = retained
            .domain_revisions
            .get(&view.key)
            .copied()
            .unwrap_or(retained.last_revision);
        let session = engine.session(&view.key);
        let event = build_native_event(
            retained,
            view,
            &member_apps,
            focused_override,
            closed.clone(),
            command.clone(),
            session,
            predicate,
            revision,
        );
        let reply = engine.handle(&event);
        if ctx.is_current(&view.key) {
            handled.push((view.key.clone(), reply));
        } else {
            hidden.push((view.key.clone(), reply));
        }
    }
    // Hidden sessions advance independently too: their cursors move with
    // their own replies, so a return converges instead of diverging on a
    // stale cursor. Counted under the same reconcile tokens (they are
    // reconciles), but never merged into actuation.
    // Merge all CURRENT domain replies into one emission and record each
    // domain's cursor separately (sessions advance independently; a shared
    // cursor would misreport every domain but one). Hidden replies advance
    // their own cursors exactly the same way, without emission.
    let mut merged = MergedPlan::default();
    for (key, reply) in &handled {
        if let Some(base) = reply_base_revision(reply) {
            retained.domain_revisions.insert(key.clone(), base);
        }
        collect_reply(reply, &mut merged);
    }
    for (key, reply) in &hidden {
        if let Some(base) = reply_base_revision(reply) {
            retained.domain_revisions.insert(key.clone(), base);
        }
    }
    count_native_replies(shared, &handled, "reconcile");
    count_native_replies(shared, &hidden, "reconcile");
    apply_merged(
        merged,
        None,
        "reconcile",
        false,
        retained,
        shared,
        focus_queue,
        focus_capacity,
        pid,
        generation,
        seq,
    )
}
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
/// the legacy arm (lifetime, termination, sequence); membership-scoped
/// planning converges each current tile domain independently.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_scoped_observation(
    engine: &mut Engine,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
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
    // converges); it never plans stale domains.
    let ctx: Option<Arc<NativeContext>> = lock_shared(shared).native_context.clone();
    let Some(ctx) = ctx else {
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
    let (members, omitted) = partition_scoped_members(&snapshot.windows, &snapshot.meta, &ctx);
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
    // Retained windows are members only: the wire, the overlay
    // filter, and close computation all see exactly what the
    // domains plan (nothing else can leak into a domain event).
    // The borrow ends here so focus bookkeeping below can hold
    // `retained` again.
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
            snapshot.focus_evidence.and_then(|evidence| {
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
    let views = native_domain_views(&ctx);
    plan_native_observation(
        engine,
        retained,
        &ctx,
        &views,
        Vec::new(),
        CoreCommand::Reconcile,
        shared,
        focus_queue,
        focus_capacity,
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
