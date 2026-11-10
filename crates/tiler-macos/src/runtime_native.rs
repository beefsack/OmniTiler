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

use std::sync::{Arc, Mutex};

use tiler_core::directional::WindowId;

use crate::focus_route::global_focus_agrees;
use crate::runtime::{
    FocusRequest, NativeContext, ObservedWindow, Shared, emit, lock_shared, window_id,
};
use crate::runtime_core::{
    RetainedApp, RetainedCore, clear_focus_into, note_scoped_members, partition_scoped_members,
};
use crate::runtime_types::{
    CloseFocusBinding, FocusCommand, ScopedBackendSnapshot, parse_window_id,
};

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

/// Outcome of the owner-side fresh-baseline check for one close-focus
/// command: the captured same-report fallback rechecked against the
/// worker's fresh selected-oracle read (`None` = unreadable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloseBaselineGate {
    /// Fresh read already names the target: no write needed.
    AlreadyTarget,
    /// Captured baseline still holds: dispatch with this proven pre.
    Proceed { pre: Option<WindowId> },
    /// Fresh read moved since the bind: newer user input intervened.
    Changed { fresh: Option<WindowId> },
    /// Baseline unknown or the fresh read unreadable: never dispatch blind.
    Unproven,
}

/// Pure fresh-baseline gate for one close-focus command. `captured_known`
/// plus `captured_pre` is the same-report fallback (proven bound window or
/// proven windowless when known, unknown otherwise); `fresh` is the
/// worker's fresh selected-oracle read; `target` the frozen close target.
/// An unknown captured baseline refuses as `Unproven` even when the fresh
/// read looks stable — a known-windowless prior is checked fresh here,
/// never bypassed. Pure over plain data so offline tests prove the matrix
/// without live AX.
pub(crate) fn gate_close_baseline(
    captured_known: bool,
    captured_pre: Option<&WindowId>,
    fresh: Option<(u32, Option<u32>)>,
    target: &WindowId,
) -> CloseBaselineGate {
    let Some((fresh_pid, fresh_wid)) = fresh else {
        return CloseBaselineGate::Unproven;
    };
    let fresh_id = fresh_wid.map(|wid| WindowId(window_id(fresh_pid, wid)));
    if fresh_id.as_ref() == Some(target) {
        return CloseBaselineGate::AlreadyTarget;
    }
    if !captured_known {
        return CloseBaselineGate::Unproven;
    }
    match captured_pre {
        Some(bound) => {
            if fresh_id.as_ref() == Some(bound) {
                CloseBaselineGate::Proceed {
                    pre: Some(bound.clone()),
                }
            } else {
                CloseBaselineGate::Changed { fresh: fresh_id }
            }
        }
        None => {
            // Known-windowless baseline: only a still-windowless fresh read
            // (no focused window anywhere) proceeds with an empty pre.
            if fresh_wid.is_none() {
                CloseBaselineGate::Proceed { pre: None }
            } else {
                CloseBaselineGate::Changed { fresh: fresh_id }
            }
        }
    }
}

/// Owner-side retained no-call verdict for one close-focus binding: `None`
/// passes, `Some(reason)` must not call. Checks the bound lifetime,
/// topology revision, frozen home, shown/current readable Space, and the
/// visible/non-minimized/non-fullscreen legs over retained truth plus the
/// live native context. Game defers to the core pre-publish gate (no
/// adapter-side game fact exists). Pure over already-read state; the fresh
/// foreground recheck rides [`gate_close_baseline`], never here.
pub(crate) fn gate_close_retained(
    close: &CloseFocusBinding,
    retained: &RetainedCore,
    ctx: Option<&NativeContext>,
    current_revision: u64,
    app_generation: Option<u64>,
    app_terminated: bool,
) -> Option<tiler_core::world_report::VisibilityReason> {
    use tiler_core::world_report::VisibilityReason;
    if close.topology_revision != current_revision {
        return Some(VisibilityReason::Superseded);
    }
    let Some(ctx) = ctx else {
        return Some(VisibilityReason::Unreadable);
    };
    if ctx.revision != current_revision {
        return Some(VisibilityReason::Superseded);
    }
    if app_terminated || app_generation != Some(close.generation) {
        return Some(VisibilityReason::IdentityStale);
    }
    let target_str = &close.target.0;
    let Some((target_pid, target_wid)) = parse_window_id(target_str) else {
        return Some(VisibilityReason::IdentityStale);
    };
    if retained.window_domains.get(target_str) != Some(&close.domain) {
        return Some(VisibilityReason::IdentityStale);
    }
    let Some(domain_row) = ctx
        .domains
        .iter()
        .find(|domain| domain.output == close.output && domain.workspace == close.workspace)
    else {
        return Some(VisibilityReason::IdentityStale);
    };
    if !domain_row.current || !domain_row.readable {
        return Some(VisibilityReason::Unreadable);
    }
    if retained.minimized.contains_key(target_str) {
        return Some(VisibilityReason::ShowBlocked);
    }
    let roster = retained
        .apps
        .get(&target_pid)
        .and_then(|app| app.roster.get(&target_wid));
    let Some(entry) = roster else {
        return Some(VisibilityReason::IdentityStale);
    };
    if entry.minimized_meta || entry.hidden_meta {
        return Some(VisibilityReason::ShowBlocked);
    }
    match entry.excluded {
        Some(crate::classifier::ExcludeReason::Minimized)
        | Some(crate::classifier::ExcludeReason::HiddenApp) => {
            return Some(VisibilityReason::ShowBlocked);
        }
        Some(_) => return Some(VisibilityReason::Unsupported),
        None => {}
    }
    // Fullscreen holds the foreground: the target itself or any same-domain
    // sibling holding a retained fullscreen window refuses the steal.
    let sibling_fullscreen = retained.apps.iter().any(|(pid, app)| {
        app.windows.iter().any(|w| {
            w.fullscreen
                && retained
                    .window_domains
                    .get(&window_id(*pid, w.wid))
                    .is_some_and(|home| home == &close.domain)
        })
    });
    if sibling_fullscreen {
        return Some(VisibilityReason::Fullscreen);
    }
    None
}

/// Full owner pre-call verdict for one close-focus command: the retained
/// no-call legs first, then the fresh baseline recheck. Pure over
/// already-read state plus one fresh read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloseOwnerGate {
    AlreadyTarget,
    Proceed {
        pre: Option<WindowId>,
    },
    NoCall {
        reason: tiler_core::world_report::VisibilityReason,
    },
}

/// Combine [`gate_close_retained`] with [`gate_close_baseline`]: retained
/// refusal wins (no fresh read needed), otherwise the fresh baseline
/// decides. Unknown or changed baselines refuse without calling.
pub(crate) fn gate_close_owner(
    close: &CloseFocusBinding,
    retained: &RetainedCore,
    ctx: Option<&NativeContext>,
    current_revision: u64,
    app_generation: Option<u64>,
    app_terminated: bool,
    fresh: Option<(u32, Option<u32>)>,
) -> CloseOwnerGate {
    if let Some(reason) = gate_close_retained(
        close,
        retained,
        ctx,
        current_revision,
        app_generation,
        app_terminated,
    ) {
        return CloseOwnerGate::NoCall { reason };
    }
    match gate_close_baseline(
        close.fallback_known,
        close.fallback_pre.as_ref(),
        fresh,
        &close.target,
    ) {
        CloseBaselineGate::AlreadyTarget => CloseOwnerGate::AlreadyTarget,
        CloseBaselineGate::Proceed { pre } => CloseOwnerGate::Proceed { pre },
        CloseBaselineGate::Changed { .. } => CloseOwnerGate::NoCall {
            reason: tiler_core::world_report::VisibilityReason::ForegroundChanged,
        },
        CloseBaselineGate::Unproven => CloseOwnerGate::NoCall {
            reason: tiler_core::world_report::VisibilityReason::FocusUnproven,
        },
    }
}

/// Map one close-focus AX completion to its honest mechanical disposition:
/// only a reported success claims `Accepted` (arrival is never claimed
/// here — later observations prove it); partial writes and timeouts are
/// throwing/ambiguous; proven failures are explicit refusals. Mirrors
/// [`crate::world_runtime::completion_setter`] for the close lane.
pub(crate) fn close_completion_disposition(
    completion: &crate::runtime_types::AxCompletion,
) -> (
    tiler_core::world_report::VisibilitySetter,
    tiler_core::world_report::VisibilityReason,
) {
    use tiler_core::world_report::{VisibilityReason, VisibilitySetter};
    match completion {
        crate::runtime_types::AxCompletion::Applied { .. } => {
            (VisibilitySetter::Accepted, VisibilityReason::Empty)
        }
        crate::runtime_types::AxCompletion::Failed { .. } => {
            (VisibilitySetter::Refused, VisibilityReason::NativeRefused)
        }
        crate::runtime_types::AxCompletion::PartialApplied { .. }
        | crate::runtime_types::AxCompletion::Timeout { .. } => {
            (VisibilitySetter::Threw, VisibilityReason::NativeRefused)
        }
    }
}

/// Honest attempt duration for one close-focus completion: the backend's
/// measured leg millis where the completion carries them, else zero (never
/// guessed).
pub(crate) fn close_attempt_ms(completion: &crate::runtime_types::AxCompletion) -> u64 {
    match completion {
        crate::runtime_types::AxCompletion::Applied { elapsed_ms } => *elapsed_ms,
        crate::runtime_types::AxCompletion::PartialApplied { elapsed_ms, .. } => *elapsed_ms,
        crate::runtime_types::AxCompletion::Failed { .. } => 0,
        crate::runtime_types::AxCompletion::Timeout { timeout_ms } => *timeout_ms,
    }
}

/// Build one truthful close-focus feedback row under the original stamp:
/// the frozen close id, target, and issuing revision ride verbatim; the
/// mechanical setter/reason plus the genuine pre/post reads ride alongside.
/// `arrival` stays empty (a focus write proves no Space arrival); the
/// caller supplies the proven post read (`None` unknown, never invented).
/// Never stage an `Accepted` row before the actual owner attempt — this
/// constructor runs only on the result path.
pub(crate) fn close_feedback_row(
    close_id: &str,
    target: &WindowId,
    issued_revision: u64,
    setter: tiler_core::world_report::VisibilitySetter,
    reason: tiler_core::world_report::VisibilityReason,
    pre: Option<WindowId>,
    post: Option<WindowId>,
    foreground_proven: bool,
    attempt_ms: u64,
) -> tiler_core::world_report::FocusFeedback {
    tiler_core::world_report::FocusFeedback {
        id: close_id.to_owned(),
        target: target.clone(),
        revision: issued_revision,
        setter,
        reason,
        pre,
        post,
        foreground_proven,
        arrival: Vec::new(),
        attempt_ms,
    }
}

/// Pure proven-windowless verdict over one scoped snapshot: true only when
/// the production oracle proved no-client (`sys_focus` is `Some((pid, None))`,
/// which the backend only mints for genuine NoValue on both AX legs), the
/// app-local leg reports no focused window, and the worker-stamped frontmost
/// names the same PID. Anything else (unknown oracle, main fallback already
/// filtered to unknown upstream, foreign frontmost, app-local disagreement)
/// refuses. No AX query; pure over already-read evidence.
#[must_use]
pub(crate) fn proven_windowless(pid: u32, snapshot: &ScopedBackendSnapshot) -> bool {
    if snapshot.focused_wid.is_some() {
        return false;
    }
    let Some((sys_pid, None)) = snapshot.sys_focus else {
        return false;
    };
    if sys_pid != pid {
        return false;
    }
    snapshot
        .focus_evidence
        .as_ref()
        .and_then(|evidence| evidence.frontmost_pid)
        == Some(sys_pid)
}

/// Fresh-frontmost contradiction over already-read evidence: true when the
/// retained global focus names a pid `r` and the worker-stamped frontmost
/// names a different valid pid `f` (`f > 1`; reserved frontmost reads as
/// unknown, never a contradiction). A background app-local unknown never
/// contradicts on its own — frontmost agreeing with retained, or absent,
/// preserves — while an affirmative frontmost move clears even when the
/// selected oracle is unknown, unbound, or foreign. No new reads: only the
/// retained focus and the stamped frontmost leg.
#[must_use]
pub(crate) fn frontmost_contradicts_retained(
    retained: &RetainedCore,
    snapshot: &ScopedBackendSnapshot,
) -> bool {
    use crate::runtime_types::parse_window_id as parse_id;
    let Some(retained_pid) = retained
        .observed_focus
        .as_ref()
        .and_then(|w| parse_id(&w.0))
        .map(|(fpid, _)| fpid)
    else {
        return false;
    };
    let Some(frontmost) = snapshot
        .focus_evidence
        .as_ref()
        .and_then(|evidence| evidence.frontmost_pid)
    else {
        return false;
    };
    frontmost > 1 && frontmost != retained_pid
}

/// Unknown/read-failure ingress for one unpromoted snapshot (no candidate,
/// not proven windowless). Only a clearing loss transitions to unknown-empty:
/// the owning PID reporting unknown clears its own focus, and a fresh
/// frontmost move clears stale cross-PID focus (see
/// [`frontmost_contradicts_retained`); a background unknown whose frontmost
/// still agrees with (or withholds) retained truth preserves it. A
/// frontmost owner reporting unknown with empty focus marks unknown
/// explicitly (covers initial and post-windowless gaps).
pub(crate) fn apply_unpromoted_focus(
    retained: &mut RetainedCore,
    pid: u32,
    snapshot: &ScopedBackendSnapshot,
) {
    use crate::runtime_types::parse_window_id as parse_id;
    let had_inside = retained
        .observed_focus
        .as_ref()
        .and_then(|w| parse_id(&w.0))
        .is_some_and(|(fpid, _)| fpid == pid);
    clear_focus_into(retained, pid);
    let frontmost_is_self = snapshot
        .focus_evidence
        .as_ref()
        .and_then(|evidence| evidence.frontmost_pid)
        == Some(pid);
    if had_inside
        || (retained.observed_focus.is_none() && frontmost_is_self && snapshot.sys_focus.is_none())
    {
        retained.observed_focus_known = false;
    }
    if frontmost_contradicts_retained(retained, snapshot) {
        retained.observed_focus = None;
        retained.observed_focus_known = false;
    }
    // Known-empty carries no source PID: a fresh unproven (unpromoted) read
    // under an affirmative frontmost cannot sustain known-none — ownership
    // is unknown, so fail conservative to unknown-empty. Absent/reserved
    // frontmost withholds (no movement evidenced); proven windowless never
    // reaches this unpromoted path, and proven bound focus is untouched.
    if retained.observed_focus.is_none()
        && retained.observed_focus_known
        && snapshot
            .focus_evidence
            .as_ref()
            .and_then(|evidence| evidence.frontmost_pid)
            .is_some_and(|frontmost| frontmost > 1)
    {
        retained.observed_focus_known = false;
    }
}

/// Observed-focus ingress for one scoped snapshot: a promoted candidate
/// binds known, genuine windowless binds known-empty, and anything else
/// rides [`apply_unpromoted_focus`]. Pure over the already-computed
/// candidate plus existing reads/stamps; the caller owns membership truth.
pub(crate) fn update_observed_focus(
    retained: &mut RetainedCore,
    pid: u32,
    snapshot: &ScopedBackendSnapshot,
    candidate: Option<u32>,
) {
    match candidate {
        Some(wid) => {
            retained.observed_focus = Some(WindowId(window_id(pid, wid)));
            retained.observed_focus_known = true;
        }
        None if proven_windowless(pid, snapshot) => {
            retained.observed_focus = None;
            retained.observed_focus_known = true;
        }
        None => {
            apply_unpromoted_focus(retained, pid, snapshot);
        }
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
        crate::runtime_core::mark_roster_incomplete(retained, pid, "no-context");
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
        crate::runtime_core::mark_roster_incomplete(retained, pid, "stale-revision");
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
    // Omission is counted evidence, never destruction. Unknown,
    // multi-Space, and other-Space windows stay in the per-app roster
    // and surface downstream as explicit unknown facts.
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
    // Covering gate: a successful but empty snapshot (hidden-Space
    // blindness) is non-covering for hidden and unknown homes. It stays
    // authoritative for currently-actuated domains: an empty read proves
    // the app holds no windows there, so current-homed rows prune as
    // closes while hidden rows preserve. Only covering snapshots prune
    // hidden homes.
    let covering = crate::runtime_core::snapshot_is_covering(
        snapshot.windows.is_empty(),
        snapshot.excluded > 0,
    );
    // Ensure the per-app roster owner exists before any update. A
    // generation change (relaunch) clears roster, holds, and homing for
    // exactly this PID; other PIDs never clear it. Pending closes for a
    // relaunched PID drop so an old lifetime never closes the new one.
    let relaunched = retained
        .apps
        .get(&pid)
        .is_some_and(|app| app.generation != generation);
    retained.apps.entry(pid).or_insert(RetainedApp {
        generation,
        seq_last: 0,
        cmd_counter: 0,
        windows: Vec::new(),
        focused_wid: None,
        roster: std::collections::BTreeMap::new(),
        roster_incomplete: false,
        roster_cause: String::new(),
    });
    if relaunched {
        if let Some(app) = retained.apps.get_mut(&pid) {
            app.generation = generation;
            app.seq_last = 0;
            app.cmd_counter = 0;
            app.roster.clear();
            app.roster_incomplete = false;
            app.roster_cause.clear();
            app.windows.clear();
            app.focused_wid = None;
        }
        let prefix = format!("{pid}_");
        retained
            .window_domains
            .retain(|id, _| !id.starts_with(&prefix));
        retained.minimized.retain(|id, _| !id.starts_with(&prefix));
        session
            .pending_closes
            .retain(|id| !id.0.starts_with(&prefix));
        crate::runtime_core::clear_focus_into(retained, pid);
    }
    // Ensure owner exists for the covering probe below.
    retained.apps.entry(pid).or_insert(RetainedApp {
        generation,
        seq_last: 0,
        cmd_counter: 0,
        windows: Vec::new(),
        focused_wid: None,
        roster: std::collections::BTreeMap::new(),
        roster_incomplete: false,
        roster_cause: String::new(),
    });
    if !covering {
        // Non-covering empty read: authoritative loss for current domains
        // only (an empty read proves the app holds nothing there), while
        // hidden and unknown homes retain with a per-PID hold. The CG
        // oracle fences even the current prune: a CG-alive or
        // CG-unreadable id retains unknown (never an explicit close). A
        // fully emptied roster is a covering truth for this scope: it
        // clears the PID hold and the session gap instead of fencing.
        if let Some(app) = retained.apps.get_mut(&pid) {
            app.seq_last = seq;
        }
        let pruned =
            crate::runtime_core::prune_current_homed_with_oracle(retained, pid, &snapshot, &ctx);
        for id in &pruned {
            session.pending_closes.insert(id.clone());
        }
        if !pruned.is_empty() {
            emit(
                shared,
                pid,
                generation,
                seq,
                "observation",
                "covering-close",
                pruned.len() as u64,
            );
        }
        // Blind-hidden preservation: re-home this PID's retained rows from
        // still-known non-current domains (production helper: excluded rows
        // and fresh-evidence wids never preserve stale homes).
        for (key, w) in
            crate::runtime_core::preserve_non_covering_hidden(retained, pid, &snapshot, &ctx)
        {
            members.entry(key).or_default().push(w);
        }
        let remainder = retained
            .apps
            .get(&pid)
            .map(|app| app.roster.len())
            .unwrap_or(0);
        if remainder > 0 {
            crate::runtime_core::mark_roster_incomplete(retained, pid, "blank-hidden");
            session.mark_coverage_gap();
        } else if let Some(app) = retained.apps.get_mut(&pid) {
            app.roster_incomplete = false;
            app.roster_cause.clear();
            session.clear_coverage_gap();
        }
        emit(
            shared,
            pid,
            generation,
            seq,
            "observation",
            "non-covering-retained",
            remainder as u64,
        );
    }
    // Accepted covering snapshot: update the authoritative roster first.
    // Pruned ids become flat pending closes (no invented domain key).
    // Non-covering reads pruned current-homed rows above instead.
    if covering {
        let pruned = crate::runtime_core::apply_roster_covering(retained, pid, &snapshot, &ctx);
        for id in &pruned {
            session.pending_closes.insert(id.clone());
        }
        if !pruned.is_empty() {
            emit(
                shared,
                pid,
                generation,
                seq,
                "observation",
                "covering-close",
                pruned.len() as u64,
            );
        }
    }
    // Fresh CG-alive re-homing (owning-worker evidence, never a stale
    // home, hidden domains only): AX-absent CG-owner windows with an exact
    // fresh singleton SLS membership homing a hidden domain plus live CG
    // frame evidence re-home here, so an off-current window the AX legs
    // missed stays homed instead of fencing the whole report. Current
    // targets, shared-output SIDs, and genuinely unknown details
    // (unreadable/multi/unhomed/degenerate) drop the stale home and keep
    // the hold pending fresh AX; actual closes already pruned above.
    // Fresh wins over the stale non-covering preservation (same function,
    // above): drop any same-wid preserved row before pushing the fresh home.
    for (key, windows) in crate::runtime_core::rehydrate_cg_alive(retained, pid, &snapshot, &ctx) {
        for w in windows {
            for members_of_domain in members.values_mut() {
                members_of_domain.retain(|other| other.wid != w.wid);
            }
            members.entry(key.clone()).or_default().push(w);
        }
    }
    let entry = retained.apps.entry(pid).or_insert(RetainedApp {
        generation,
        seq_last: 0,
        cmd_counter: 0,
        windows: Vec::new(),
        focused_wid: None,
        roster: std::collections::BTreeMap::new(),
        roster_incomplete: false,
        roster_cause: String::new(),
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
    update_observed_focus(retained, pid, &snapshot, candidate);
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
    // Re-audit the hold after homing: unrepresentable roster rows (no
    // frame, or a frame no single FULL output uniquely owns) keep this
    // PID incomplete; grounded rows and homed rows do not hold.
    crate::runtime_core::audit_roster_hold(retained, pid, &ctx);
    emit(
        shared,
        pid,
        generation,
        seq,
        "observation",
        "applied",
        member_count,
    );
    // Covering reads prove fresh truth: the session gap clears. A
    // non-covering read with retained remainder keeps the gap it marked;
    // a fully emptied scope clears it as covering truth.
    if covering
        || !retained
            .apps
            .get(&pid)
            .is_some_and(|app| app.roster_incomplete)
    {
        session.clear_coverage_gap();
    }
    crate::world_dispatch::dispatch_report(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        crate::world_runtime::DispatchContext::Native(Arc::clone(&ctx)),
        Vec::new(),
        None,
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
            expect_prior: None,
            close: None,
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

    fn windowless_snapshot(
        pid: u32,
        sys_focus: Option<(u32, Option<u32>)>,
        frontmost_pid: Option<u32>,
        focused_wid: Option<u32>,
    ) -> ScopedBackendSnapshot {
        let mut snapshot = ScopedBackendSnapshot::from_unscoped(Vec::new(), focused_wid, 9);
        snapshot.sys_focus = sys_focus;
        snapshot.focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid,
            frontmost_bundle: None,
            ax_focus: sys_focus,
        });
        let _ = pid;
        snapshot
    }

    #[test]
    fn proven_windowless_needs_same_pid_frontmost_and_no_local_focus() {
        // Genuine windowless: oracle proved no-client for this PID, the
        // app-local leg reports no focused window, and the stamped frontmost
        // names the same PID.
        assert!(proven_windowless(
            100,
            &windowless_snapshot(100, Some((100, None)), Some(100), None)
        ));
    }

    #[test]
    fn proven_windowless_refuses_unknown_foreign_and_disagreement() {
        // Unsupported/unreadable oracle: unknown, never known-empty.
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, None, Some(100), None)
        ));
        // Bound wid is not windowless (separate bound path).
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, Some((100, Some(7))), Some(100), Some(7))
        ));
        // Foreign: oracle names another PID (main fallback already filters
        // to unknown upstream; a foreign wid never proves this PID empty).
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, Some((200, None)), Some(200), None)
        ));
        // Foreign frontmost: oracle PID disagrees with stamped frontmost.
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, Some((100, None)), Some(200), None)
        ));
        // Stale/missing frontmost leg: unknown.
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, Some((100, None)), None, None)
        ));
        // App-local disagreement: local focused window against a windowless
        // oracle is unknown, never known-empty.
        assert!(!proven_windowless(
            100,
            &windowless_snapshot(100, Some((100, None)), Some(100), Some(7))
        ));
    }

    use crate::runtime_core::{MinimizedRecord, RetainedApp, RosterEntry};
    use crate::runtime_types::{AxCompletion, NativeDomain};
    use std::collections::BTreeMap;
    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::session::DomainKey;
    use tiler_core::size_hints::WindowSizeHints;
    use tiler_core::world_report::{VisibilityReason, VisibilitySetter};

    const CLOSE_PID: u32 = 4242;
    const CLOSE_WID: u32 = 7;
    const CLOSE_GEN: u64 = 1;
    const CLOSE_REV: u64 = 9;

    fn close_key() -> DomainKey {
        DomainKey {
            output: OutputId(crate::topology::output_key(1)),
            workspace: WorkspaceId(crate::topology::workspace_key(277)),
        }
    }

    fn close_domain(current: bool, readable: bool) -> NativeDomain {
        let key = close_key();
        NativeDomain {
            cg_id: 1,
            uuid: Some("37AC4E19-0000-4000-8000-123456789ABC".to_owned()),
            output: key.output,
            workspace: key.workspace,
            space: 277,
            current,
            readable,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 1440,
                h: 900,
            },
        }
    }

    fn close_ctx(revision: u64, current: bool, readable: bool) -> NativeContext {
        NativeContext {
            revision,
            domains: vec![close_domain(current, readable)],
            skipped: Vec::new(),
        }
    }

    fn close_roster_entry() -> RosterEntry {
        RosterEntry {
            observed: Some(ObservedWindow {
                wid: CLOSE_WID,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: false,
                hints: WindowSizeHints::none(),
            }),
            spaces: vec![277],
            fullscreen_meta: false,
            minimized_meta: false,
            hidden_meta: false,
            excluded: None,
            role: Some("AXWindow".to_owned()),
            subrole: Some("AXStandardWindow".to_owned()),
            modal: Some(false),
            resizable: Some(true),
            layer: Some(0),
            app_id: Some("com.example.app".to_owned()),
            activation_policy: None,
        }
    }

    fn close_retained() -> RetainedCore {
        let mut retained = RetainedCore::default();
        let mut app = RetainedApp {
            generation: CLOSE_GEN,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        };
        app.roster.insert(CLOSE_WID, close_roster_entry());
        retained.apps.insert(CLOSE_PID, app);
        retained
            .window_domains
            .insert(window_id(CLOSE_PID, CLOSE_WID), close_key());
        retained
    }

    fn close_binding() -> CloseFocusBinding {
        let key = close_key();
        CloseFocusBinding {
            close_id: "close-focus-3".to_owned(),
            target: WindowId(window_id(CLOSE_PID, CLOSE_WID)),
            issued_revision: 3,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            domain: key,
            generation: CLOSE_GEN,
            topology_revision: CLOSE_REV,
            fallback_pre: Some(WindowId(window_id(CLOSE_PID, 9))),
            fallback_known: true,
        }
    }

    #[test]
    fn close_baseline_matrix() {
        let target = WindowId(window_id(CLOSE_PID, CLOSE_WID));
        let bound = WindowId(window_id(CLOSE_PID, 9));
        // Fresh already names the target: no write needed (even with an
        // unknown captured baseline — a stable target read is itself proof).
        assert_eq!(
            gate_close_baseline(false, None, Some((CLOSE_PID, Some(CLOSE_WID))), &target),
            CloseBaselineGate::AlreadyTarget,
        );
        // Captured bound still holds: dispatch with the proven pre.
        assert_eq!(
            gate_close_baseline(true, Some(&bound), Some((CLOSE_PID, Some(9))), &target),
            CloseBaselineGate::Proceed {
                pre: Some(bound.clone()),
            },
        );
        // Known-windowless baseline with a still-windowless fresh read:
        // proceed with an empty pre.
        assert_eq!(
            gate_close_baseline(true, None, Some((CLOSE_PID, None)), &target),
            CloseBaselineGate::Proceed { pre: None },
        );
        // Fresh moved to a third window: newer user input intervened.
        let other = WindowId(window_id(CLOSE_PID, 8));
        assert_eq!(
            gate_close_baseline(true, Some(&bound), Some((CLOSE_PID, Some(8))), &target),
            CloseBaselineGate::Changed { fresh: Some(other) },
        );
        // Known-windowless baseline with a freshly focused window: changed.
        let elsewhere = WindowId(window_id(CLOSE_PID, 8));
        assert_eq!(
            gate_close_baseline(true, None, Some((CLOSE_PID, Some(CLOSE_WID))), &elsewhere),
            CloseBaselineGate::Changed {
                fresh: Some(target.clone()),
            },
        );
        // Unknown captured baseline: unproven even when the fresh read
        // looks stable — never dispatch blind on an unproven baseline.
        assert_eq!(
            gate_close_baseline(false, None, Some((CLOSE_PID, Some(9))), &target),
            CloseBaselineGate::Unproven,
        );
        // Unreadable fresh read: unproven even with a proven baseline.
        assert_eq!(
            gate_close_baseline(true, Some(&bound), None, &target),
            CloseBaselineGate::Unproven,
        );
    }

    #[test]
    fn close_retained_pass_and_revision_fences() {
        let retained = close_retained();
        let ctx = close_ctx(CLOSE_REV, true, true);
        // Fully coherent binding passes every retained leg.
        assert_eq!(
            gate_close_retained(
                &close_binding(),
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            None,
        );
        // Binding revision drifted against the live topology: superseded.
        let mut drifted = close_binding();
        drifted.topology_revision = CLOSE_REV - 1;
        assert_eq!(
            gate_close_retained(
                &drifted,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Superseded),
        );
        // Context revision drifted against the live revision: superseded.
        let stale_ctx = close_ctx(CLOSE_REV - 1, true, true);
        assert_eq!(
            gate_close_retained(
                &close_binding(),
                &retained,
                Some(&stale_ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Superseded),
        );
        // No live context: unreadable, never a blind call.
        assert_eq!(
            gate_close_retained(
                &close_binding(),
                &retained,
                None,
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Unreadable),
        );
    }

    #[test]
    fn close_retained_identity_and_home_fences() {
        let retained = close_retained();
        let ctx = close_ctx(CLOSE_REV, true, true);
        let binding = close_binding();
        // Relaunched lifetime: the bound generation no longer owns the PID.
        assert_eq!(
            gate_close_retained(
                &binding,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN + 1),
                false
            ),
            Some(VisibilityReason::IdentityStale),
        );
        // Terminated app: stale identity.
        assert_eq!(
            gate_close_retained(
                &binding,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                true
            ),
            Some(VisibilityReason::IdentityStale),
        );
        // Unknown generation (app never observed): stale identity.
        assert_eq!(
            gate_close_retained(&binding, &retained, Some(&ctx), CLOSE_REV, None, false),
            Some(VisibilityReason::IdentityStale),
        );
        // Re-homed target: the frozen domain no longer homes the window.
        let mut moved = close_retained();
        moved.window_domains.insert(
            window_id(CLOSE_PID, CLOSE_WID),
            DomainKey {
                output: OutputId(crate::topology::output_key(2)),
                workspace: WorkspaceId(crate::topology::workspace_key(278)),
            },
        );
        assert_eq!(
            gate_close_retained(
                &binding,
                &moved,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::IdentityStale),
        );
        // Missing roster row: unknown target identity.
        let mut noroster = close_retained();
        noroster
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .clear();
        assert_eq!(
            gate_close_retained(
                &binding,
                &noroster,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::IdentityStale),
        );
        // Unparsable target: stale identity, never a guessed PID.
        let mut bad = binding.clone();
        bad.target = WindowId("nope".to_owned());
        assert_eq!(
            gate_close_retained(
                &bad,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::IdentityStale),
        );
    }

    #[test]
    fn close_retained_visibility_fences() {
        let binding = close_binding();
        let retained = close_retained();
        let current = close_ctx(CLOSE_REV, true, true);
        // Hidden domain (not current): the frozen home is not actuated.
        let hidden = close_ctx(CLOSE_REV, false, true);
        assert_eq!(
            gate_close_retained(
                &binding,
                &retained,
                Some(&hidden),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Unreadable),
        );
        // Homed-but-unreadable domain: retained hidden, never called.
        let gap = NativeContext {
            revision: CLOSE_REV,
            domains: vec![close_domain(true, false)],
            skipped: Vec::new(),
        };
        assert_eq!(
            gate_close_retained(
                &binding,
                &retained,
                Some(&gap),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Unreadable),
        );
        // Minimized lifetime: the host holds the window back.
        let mut minimized = close_retained();
        minimized.minimized.insert(
            window_id(CLOSE_PID, CLOSE_WID),
            MinimizedRecord {
                window: WindowId(window_id(CLOSE_PID, CLOSE_WID)),
                output: close_key().output,
                workspace: close_key().workspace,
                sticky: false,
                minimized: true,
                host_hidden: false,
                fullscreen: false,
                process_id: None,
                role: None,
                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            },
        );
        assert_eq!(
            gate_close_retained(
                &binding,
                &minimized,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::ShowBlocked),
        );
        // Classifier-minimized leg: same show-blocked refusal.
        let mut meta_min = close_retained();
        meta_min
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .get_mut(&CLOSE_WID)
            .expect("row")
            .minimized_meta = true;
        assert_eq!(
            gate_close_retained(
                &binding,
                &meta_min,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::ShowBlocked),
        );
        // Owner-hidden leg: same show-blocked refusal.
        let mut meta_hidden = close_retained();
        meta_hidden
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .get_mut(&CLOSE_WID)
            .expect("row")
            .hidden_meta = true;
        assert_eq!(
            gate_close_retained(
                &binding,
                &meta_hidden,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::ShowBlocked),
        );
        // Non-minimized exclusion (unknown role): unsupported, never tiled.
        let mut excluded = close_retained();
        excluded
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .get_mut(&CLOSE_WID)
            .expect("row")
            .excluded = Some(crate::classifier::ExcludeReason::UnknownRole);
        assert_eq!(
            gate_close_retained(
                &binding,
                &excluded,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Unsupported),
        );
        // Fullscreen target holds the foreground: refuse the steal.
        let mut fs_target = close_retained();
        fs_target
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .windows
            .push(ObservedWindow {
                wid: CLOSE_WID,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: true,
                hints: WindowSizeHints::none(),
            });
        assert_eq!(
            gate_close_retained(
                &binding,
                &fs_target,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Fullscreen),
        );
        // Same-domain fullscreen sibling holds the foreground too.
        let mut fs_sibling = close_retained();
        fs_sibling
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .windows
            .push(ObservedWindow {
                wid: 8,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: true,
                hints: WindowSizeHints::none(),
            });
        fs_sibling
            .window_domains
            .insert(window_id(CLOSE_PID, 8), close_key());
        assert_eq!(
            gate_close_retained(
                &binding,
                &fs_sibling,
                Some(&current),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false
            ),
            Some(VisibilityReason::Fullscreen),
        );
    }

    #[test]
    fn close_owner_retained_first_then_baseline() {
        let retained = close_retained();
        let ctx = close_ctx(CLOSE_REV, true, true);
        let binding = close_binding();
        let pre = WindowId(window_id(CLOSE_PID, 9));
        // Coherent everything: proceed with the proven pre.
        assert_eq!(
            gate_close_owner(
                &binding,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false,
                Some((CLOSE_PID, Some(9))),
            ),
            CloseOwnerGate::Proceed {
                pre: Some(pre.clone()),
            },
        );
        // Retained refusal wins without a fresh read: even a changed
        // foreground still reports the retained cause.
        let mut drifted = binding.clone();
        drifted.topology_revision = CLOSE_REV - 1;
        assert_eq!(
            gate_close_owner(
                &drifted,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false,
                Some((CLOSE_PID, Some(8))),
            ),
            CloseOwnerGate::NoCall {
                reason: VisibilityReason::Superseded,
            },
        );
        // Fresh already on target: already-target without calling.
        assert_eq!(
            gate_close_owner(
                &binding,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false,
                Some((CLOSE_PID, Some(CLOSE_WID))),
            ),
            CloseOwnerGate::AlreadyTarget,
        );
        // Fresh moved on: foreground-changed without calling.
        assert_eq!(
            gate_close_owner(
                &binding,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false,
                Some((CLOSE_PID, Some(8))),
            ),
            CloseOwnerGate::NoCall {
                reason: VisibilityReason::ForegroundChanged,
            },
        );
        // Unknown baseline: focus-unproven without calling.
        let mut unknown = binding.clone();
        unknown.fallback_known = false;
        assert_eq!(
            gate_close_owner(
                &unknown,
                &retained,
                Some(&ctx),
                CLOSE_REV,
                Some(CLOSE_GEN),
                false,
                Some((CLOSE_PID, Some(9))),
            ),
            CloseOwnerGate::NoCall {
                reason: VisibilityReason::FocusUnproven,
            },
        );
    }

    #[test]
    fn close_completion_disposition_matrix() {
        // Only a reported success claims `Accepted` (arrival itself is
        // never claimed here — later observations prove it).
        assert_eq!(
            close_completion_disposition(&AxCompletion::Applied { elapsed_ms: 5 }),
            (VisibilitySetter::Accepted, VisibilityReason::Empty),
        );
        assert_eq!(
            close_completion_disposition(&AxCompletion::Failed {
                reason: "nope".to_owned(),
            }),
            (VisibilitySetter::Refused, VisibilityReason::NativeRefused),
        );
        assert_eq!(
            close_completion_disposition(&AxCompletion::PartialApplied {
                elapsed_ms: 7,
                size_first_applied: true,
                position_applied: false,
                size_applied: true,
                readback_xywh: None,
                readback_unreadable: true,
            }),
            (VisibilitySetter::Threw, VisibilityReason::NativeRefused),
        );
        assert_eq!(
            close_completion_disposition(&AxCompletion::Timeout { timeout_ms: 9 }),
            (VisibilitySetter::Threw, VisibilityReason::NativeRefused),
        );
    }

    #[test]
    fn close_attempt_ms_honest_durations() {
        assert_eq!(
            close_attempt_ms(&AxCompletion::Applied { elapsed_ms: 5 }),
            5,
        );
        assert_eq!(
            close_attempt_ms(&AxCompletion::PartialApplied {
                elapsed_ms: 7,
                size_first_applied: false,
                position_applied: true,
                size_applied: false,
                readback_xywh: Some((1, 2, 3, 4)),
                readback_unreadable: false,
            }),
            7,
        );
        // Failures carry no measured leg: zero, never guessed.
        assert_eq!(
            close_attempt_ms(&AxCompletion::Failed {
                reason: "nope".to_owned(),
            }),
            0,
        );
        assert_eq!(
            close_attempt_ms(&AxCompletion::Timeout { timeout_ms: 9 }),
            9,
        );
    }

    #[test]
    fn close_feedback_row_stamp_passthrough() {
        // The frozen close stamp rides verbatim; `arrival` stays empty (a
        // focus write proves no Space arrival).
        let target = WindowId(window_id(CLOSE_PID, CLOSE_WID));
        let pre = WindowId(window_id(CLOSE_PID, 9));
        let row = close_feedback_row(
            "close-focus-3",
            &target,
            3,
            VisibilitySetter::Accepted,
            VisibilityReason::Empty,
            Some(pre.clone()),
            Some(target.clone()),
            true,
            5,
        );
        assert_eq!(row.id, "close-focus-3");
        assert_eq!(row.target, target);
        assert_eq!(row.revision, 3);
        assert_eq!(row.setter, VisibilitySetter::Accepted);
        assert_eq!(row.reason, VisibilityReason::Empty);
        assert_eq!(row.pre, Some(pre));
        assert_eq!(row.post, Some(target));
        assert!(row.foreground_proven);
        assert!(row.arrival.is_empty());
        assert_eq!(row.attempt_ms, 5);
    }

    fn bound_a() -> RetainedCore {
        let mut retained = RetainedCore::default();
        retained.observed_focus = Some(WindowId("100_7".to_owned()));
        retained.observed_focus_known = true;
        retained
    }

    fn known_empty() -> RetainedCore {
        let mut retained = RetainedCore::default();
        retained.observed_focus = None;
        retained.observed_focus_known = true;
        retained
    }

    #[test]
    fn unpromoted_known_empty_clears_on_fresh_unproven_frontmost() {
        // Retained known-empty (proven windowless, no source PID). A fresh
        // unpromoted read under an affirmative foreign frontmost — unknown
        // oracle, unbound, or foreign — cannot sustain known-none: ownership
        // is unknown, so fail conservative to unknown-empty. The production
        // report then reads empty+unknown, never a false no-client claim.
        for sys in [None, Some((200, Some(99))), Some((300, Some(5)))] {
            let mut retained = known_empty();
            let snapshot = windowless_snapshot(200, sys, Some(300), None);
            assert!(
                !proven_windowless(200, &snapshot),
                "sys {sys:?} is unpromoted, not windowless"
            );
            update_observed_focus(&mut retained, 200, &snapshot, None);
            assert_eq!(retained.observed_focus, None, "sys {sys:?} stays empty");
            assert!(
                !retained.observed_focus_known,
                "sys {sys:?} is unknown-empty, never known-none"
            );
        }
        // Frontmost-self unknown (unsupported oracle under the frontmost
        // owner itself) likewise refuses known-none.
        let mut retained = known_empty();
        let snapshot = windowless_snapshot(300, None, Some(300), None);
        update_observed_focus(&mut retained, 300, &snapshot, None);
        assert_eq!(retained.observed_focus, None);
        assert!(!retained.observed_focus_known);
        // Withheld frontmost (absent/reserved) evidences no movement:
        // known-empty preserves.
        for frontmost in [None, Some(1)] {
            let mut retained = known_empty();
            let snapshot = windowless_snapshot(200, None, frontmost, None);
            update_observed_focus(&mut retained, 200, &snapshot, None);
            assert_eq!(retained.observed_focus, None);
            assert!(
                retained.observed_focus_known,
                "frontmost {frontmost:?} withholds, known-empty preserves"
            );
        }
        // Production report after the foreign-frontmost clear: empty focus
        // rides unknown, never a false no-client claim.
        use std::sync::Arc;

        use crate::world_runtime::{DispatchContext, WorldSession, build_dispatch_report};
        use crate::world_sequencer::ObservationClock;

        let mut retained = known_empty();
        let snapshot = windowless_snapshot(200, None, Some(300), None);
        update_observed_focus(&mut retained, 200, &snapshot, None);
        let mut session = WorldSession::new(
            false,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        );
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Synthetic,
            &[],
            1,
            1,
            None,
            None,
            None,
            "req-known-empty-clear",
        )
        .expect("synthetic report builds");
        assert!(report.focused_global.is_empty());
        assert!(!report.foreground_known);
    }

    #[test]
    fn unpromoted_fresh_frontmost_clears_stale_cross_pid_focus() {
        // Retained bound A (100_7, known). A fresh snapshot from B (pid
        // 200, frontmost 200) with no promotion clears to unknown-empty —
        // never stale A as known, never a known-none claim. Every
        // no-promotion oracle shape arrives here identically: unreadable /
        // unsupported / failure (`None`; the main-fallback route also maps
        // to `None` upstream), unbound, and foreign reads.
        for sys in [None, Some((200, Some(99))), Some((300, Some(5)))] {
            let mut retained = bound_a();
            let snapshot = windowless_snapshot(200, sys, Some(200), None);
            assert!(
                !proven_windowless(200, &snapshot),
                "sys {sys:?} is unpromoted, not windowless"
            );
            update_observed_focus(&mut retained, 200, &snapshot, None);
            assert_eq!(retained.observed_focus, None, "sys {sys:?} clears stale A");
            assert!(
                !retained.observed_focus_known,
                "sys {sys:?} is unknown-empty, never known-none"
            );
        }
    }

    #[test]
    fn unpromoted_background_unknown_preserves_agreeing_focus() {
        // Frontmost still names A (100): a background unknown from pid 200
        // is mere app-local absence, not a contradiction — A stays bound.
        let mut retained = bound_a();
        let snapshot = windowless_snapshot(200, None, Some(100), None);
        update_observed_focus(&mut retained, 200, &snapshot, None);
        assert_eq!(retained.observed_focus, Some(WindowId("100_7".to_owned())));
        assert!(retained.observed_focus_known);
        // A bound oracle read naming A (e.g. delayed owner truth) likewise
        // preserves, even when observed from the background pid.
        let mut retained = bound_a();
        let snapshot = windowless_snapshot(200, Some((100, Some(7))), Some(100), None);
        update_observed_focus(&mut retained, 200, &snapshot, None);
        assert_eq!(retained.observed_focus, Some(WindowId("100_7".to_owned())));
        assert!(retained.observed_focus_known);
    }

    #[test]
    fn unpromoted_owner_unknown_clears_to_unknown_empty() {
        // The owning PID itself reporting unknown (had-inside loss) clears
        // its own focus to unknown-empty.
        let mut retained = bound_a();
        let snapshot = windowless_snapshot(100, None, Some(100), None);
        update_observed_focus(&mut retained, 100, &snapshot, None);
        assert_eq!(retained.observed_focus, None);
        assert!(!retained.observed_focus_known);
    }

    #[test]
    fn unpromoted_withheld_or_reserved_frontmost_preserves() {
        // No affirmative contradiction — absent or reserved frontmost
        // withholds; a transient unreadable must never clobber proven A.
        for frontmost in [None, Some(1)] {
            let mut retained = bound_a();
            let snapshot = windowless_snapshot(200, None, frontmost, None);
            update_observed_focus(&mut retained, 200, &snapshot, None);
            assert_eq!(
                retained.observed_focus,
                Some(WindowId("100_7".to_owned())),
                "frontmost {frontmost:?} preserves A"
            );
            assert!(retained.observed_focus_known);
        }
    }

    #[test]
    fn promoted_candidate_binds_known_and_windowless_binds_known_empty() {
        // Genuine windowless proof (frontmost self, oracle no-client) binds
        // known-empty.
        let mut retained = bound_a();
        let snapshot = windowless_snapshot(100, Some((100, None)), Some(100), None);
        assert!(proven_windowless(100, &snapshot));
        update_observed_focus(&mut retained, 100, &snapshot, None);
        assert_eq!(retained.observed_focus, None);
        assert!(retained.observed_focus_known);
        // A currently bound candidate binds known.
        let mut retained = RetainedCore::default();
        let snapshot = windowless_snapshot(100, Some((100, Some(7))), Some(100), Some(7));
        update_observed_focus(&mut retained, 100, &snapshot, Some(7));
        assert_eq!(retained.observed_focus, Some(WindowId("100_7".to_owned())));
        assert!(retained.observed_focus_known);
    }

    #[test]
    fn dispatch_report_foreground_codec_pairs_truthfully() {
        // Production codec: nonempty focus always rides known (the codec
        // refuses false+nonempty); empty rides known only on genuine
        // windowless proof, else unknown. Never stale-known, never
        // known-none.
        use std::sync::Arc;

        use crate::world_runtime::{DispatchContext, WorldSession, build_dispatch_report};
        use crate::world_sequencer::ObservationClock;

        fn report_for(focus: Option<&str>, known: bool) -> (String, bool) {
            let mut retained = RetainedCore::default();
            retained.observed_focus = focus.map(|f| WindowId(f.to_owned()));
            retained.observed_focus_known = known;
            let mut session = WorldSession::new(
                false,
                FocusRoute::Slps,
                Arc::new(ObservationClock::new()),
                &[],
            );
            let report = build_dispatch_report(
                &mut session,
                &retained,
                &DispatchContext::Synthetic,
                &[],
                1,
                1,
                None,
                None,
                None,
                "req-focus-codec",
            )
            .expect("synthetic report builds");
            (report.focused_global, report.foreground_known)
        }

        assert_eq!(report_for(Some("100_7"), true), ("100_7".to_owned(), true));
        assert_eq!(report_for(None, true), (String::new(), true));
        assert_eq!(report_for(None, false), (String::new(), false));
    }
}
