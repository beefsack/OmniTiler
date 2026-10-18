#![forbid(unsafe_code)]

//! World-report dispatch and emission coherence (Unit B actuation half).
//!
//! Split from [`crate::world_runtime`]: while that module owns the
//! engine-thread [`WorldSession`](crate::world_runtime::WorldSession) and
//! shapes retained truth into complete reports, this module dispatches one
//! report per job and consumes its reply into frame slots, the focus
//! outbox, select claims, and attempt feedback. No AX here, no blocking on
//! workers.
//!
//! Freshness contract (see `emission_coherent`): an accepted reply still
//! must not touch native setters unless its snapshot provably
//! incorporates every submitted input. A sequencer-stale reply actuates
//! nothing; when no honest successor is coming, one deferred
//! observation-only retry recovers instead of stalling. Old report plus
//! newer queued input never yields old frame setters.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::session::DomainKey;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{OneShotEffect, ScopeAuthority, WorldCommand, WorldReply};

use crate::runtime::{
    DesiredFrame, FocusCommand, FocusRequest, FrameCommand, Shared, emit, lock_focus, lock_shared,
    parse_window_id,
};
use crate::runtime_core::RetainedCore;
use crate::runtime_thread::{CoherencePending, FocusSlot, FrameSlot, Inbox};
use crate::runtime_types::{PHASE1_OUTPUT, PHASE1_WORKSPACE, TopologyMode};
use crate::world_runtime::{DispatchContext, WorldSession, build_dispatch_report, track_write};
use crate::world_sequencer::{ReplyVerdict, RequestTicket, command_operation_id};

/// Static reply token for the ring (`emit` needs `&'static str`): every
/// production op/outcome pair is enumerated, so no per-reply allocation or
/// leak carries diagnostics.
fn reply_token(op: &'static str, outcome: &'static str) -> &'static str {
    match (op, outcome) {
        ("reconcile", "accepted") => "reconcile-accepted",
        ("reconcile", "consumed") => "reconcile-consumed",
        ("focus", "accepted") => "focus-accepted",
        ("focus", "consumed") => "focus-consumed",
        ("move", "accepted") => "move-accepted",
        ("move", "consumed") => "move-consumed",
        ("resize", "accepted") => "resize-accepted",
        ("resize", "consumed") => "resize-consumed",
        ("toggle-float", "accepted") => "toggle-float-accepted",
        ("toggle-float", "consumed") => "toggle-float-consumed",
        ("toggle-orientation", "accepted") => "toggle-orientation-accepted",
        ("toggle-orientation", "consumed") => "toggle-orientation-consumed",
        ("drag-drop", "accepted") => "drag-drop-accepted",
        ("drag-drop", "consumed") => "drag-drop-consumed",
        ("select", "accepted") => "select-accepted",
        ("select", "consumed") => "select-consumed",
        ("send", "accepted") => "send-accepted",
        ("send", "consumed") => "send-consumed",
        ("terminate", "accepted") => "terminate-accepted",
        ("terminate", "consumed") => "terminate-consumed",
        // All production (op, outcome) pairs are enumerated above; the
        // fallback is unreachable defense (debug-checked, never formatted
        // per reply, so diagnostics carry no per-reply allocation).
        other => {
            debug_assert!(false, "unenumerated reply token: {other:?}");
            "reconcile-accepted"
        }
    }
}

/// Live per-app emission targets (mirrors the legacy collector).
type LiveSlots = Vec<(u32, u64, Arc<FrameSlot>, Arc<FocusSlot>)>;

/// Whether retained observation truth still shows a native fullscreen
/// overlay for `window`: the adapter-visible hold behind withheld writes
/// (the reserved slot/allocation stays plan truth core-side).
fn is_retained_fullscreen(retained: &RetainedCore, window: &str) -> bool {
    let Some((pid, wid)) = parse_window_id(window) else {
        return false;
    };
    retained
        .apps
        .get(&pid)
        .and_then(|app| app.windows.iter().find(|w| w.wid == wid))
        .is_some_and(|w| w.fullscreen)
}

/// Dispatch outcome for one engine job: what the caller counts and whether
/// a select claim may be stored.
pub(crate) enum DispatchVerdict {
    /// A reply applied (frames/focus/transfers consumed). Carries the
    /// mintable select operation identity when the dispatched command was
    /// a planner-owned select (the caller binds the pending claim under
    /// it); `None` otherwise.
    Applied { select_operation: Option<String> },
    /// Duplicate of the latest reply: no state change, no re-emission.
    Duplicate,
    /// Core or sequencer refusal: counted, no state change — except the
    /// stale-observation refusal of a planner-owned select, which carries
    /// its operation identity: the command already executed core-side
    /// (at-most-once), so the caller still binds the pending claim under
    /// it for drain feedback instead of leaking it unbound.
    Refused {
        token: String,
        select_operation: Option<String>,
    },
    /// Nothing reportable (empty context): counted, no state change.
    NoReport,
}

/// Dispatch one complete report and consume its reply: the single
/// production path for observations, inputs, terminates, and selects.
/// Effects spend before first dispatch; a sequencer-stale reply actuates
/// nothing (no immediate redispatch from retained truth: the queued
/// successors each dispatch honestly fenced snapshots and converge);
/// core duplicates never re-emit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_report(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    dispatch: DispatchContext,
    closed: BTreeMap<DomainKey, Vec<WindowId>>,
    command: Option<WorldCommand>,
    command_is_select: bool,
    op: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) -> DispatchVerdict {
    // Retained dispatch context (known domains, topology revision)
    // refreshes here, so teardown paths converge without an observation.
    // The watermark is NOT advanced here: only actually-submitted input
    // moves the shared clock (submit lanes and staged completions), so the
    // ticket below stamps submitted input and a mid-flight arrival refuses
    // as stale instead of actuating over it.
    if let DispatchContext::Native(ctx) = &dispatch {
        // The engine adopts a new topology revision through this full
        // report (core-internally retiring vanished domains and seeding
        // new ones — no per-domain release events cross this boundary).
        if crate::runtime_core::note_dispatch_context(retained, ctx) {
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
    }
    let Some(ticket) = session.sequencer().try_begin_request(command.clone()) else {
        // Single-flight, non-queuing: only reachable on re-entrant
        // dispatch (the engine thread never overlaps itself). Drop the
        // stale flight and retry once rather than wedging.
        session.sequencer().clear_flight();
        let Some(retry) = session.sequencer().try_begin_request(command) else {
            count_token(shared, &format!("{op}-refused-no-flight"));
            emit(
                shared,
                pid,
                generation,
                seq,
                "core-reply",
                "refused-no-flight",
                0,
            );
            return DispatchVerdict::Refused {
                token: "refused-no-flight".to_owned(),
                select_operation: None,
            };
        };
        return dispatch_ticket(
            session,
            retained,
            shared,
            inbox,
            focus_queue,
            focus_capacity,
            dispatch,
            closed,
            retry,
            command_is_select,
            true,
            op,
            pid,
            generation,
            seq,
        );
    };
    // Deterministic-test rendezvous (see `MidflightHook`): with the flight
    // open and the ticket stamped, a test can enqueue racing input before
    // the report builds. Cloned out under a short lock, then invoked with
    // no locks held (the scrutinee temporary of an `if let` would hold
    // the shared guard across the call and deadlock submitters);
    // production never sets it. The deferred retry (below) never refires
    // it: the racing input is already incorporated by then.
    let hook = lock_shared(shared)
        .midflight_hook
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
    dispatch_ticket(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        dispatch,
        closed,
        ticket,
        command_is_select,
        true,
        op,
        pid,
        generation,
        seq,
    )
}

#[allow(clippy::too_many_arguments)]
fn dispatch_ticket(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    dispatch: DispatchContext,
    closed: BTreeMap<DomainKey, Vec<WindowId>>,
    ticket: RequestTicket,
    command_is_select: bool,
    retry_allowed: bool,
    op: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) -> DispatchVerdict {
    // The select operation identity binds at ticket time (at-most-once):
    // the caller stores the pending claim under it when the reply applies
    // — and also when a stale-observation refusal proves the command
    // already executed core-side (see below). `None` for unbound selects
    // (no command sequence minted): never a placeholder identity.
    let select_operation: Option<String> = command_is_select
        .then(|| ticket.command_seq.map(command_operation_id))
        .flatten();
    let Some(report) = build_dispatch_report(
        session,
        retained,
        &dispatch,
        closed,
        ticket.observation_seq,
        ticket.request_seq,
        ticket.command_seq,
        ticket.command.clone(),
        &ticket.request_id,
    ) else {
        // Empty context: never actuate (KDE null parity).
        session.sequencer().clear_flight();
        count_token(shared, &format!("{op}-refused-no-context"));
        emit(
            shared,
            pid,
            generation,
            seq,
            "core-reply",
            "refused-no-context",
            0,
        );
        return DispatchVerdict::NoReport;
    };
    // One core transaction per dispatch (bounded 4 MiB request AND reply
    // inside `handle_report`; staged overflow rejects without mutation).
    match session.engine_mut().handle_report(&report) {
        WorldOutcome::Accepted(reply) => {
            match session.sequencer().accept_reply(&reply) {
                ReplyVerdict::Applied(applied) => {
                    let Some(effects) = spend_if_coherent(
                        session, retained, shared, inbox, &dispatch, &applied, op, pid, generation,
                        seq,
                    ) else {
                        // The accepted command (if any) already executed
                        // core-side, so a planner-owned select binds exactly
                        // as on a pre-accept stale refusal; when no honest
                        // successor is coming, one deferred retry recovers.
                        let bind = (command_is_select && ticket.command_seq.is_some())
                            .then(|| select_operation.clone())
                            .flatten();
                        if retry_allowed
                            && let Some(recovered) = maybe_retry(
                                session,
                                retained,
                                shared,
                                inbox,
                                focus_queue,
                                focus_capacity,
                                dispatch,
                                bind.clone(),
                                pid,
                                generation,
                                seq,
                            )
                        {
                            return recovered;
                        }
                        return DispatchVerdict::Refused {
                            token: "stale-observation".to_owned(),
                            select_operation: bind,
                        };
                    };
                    consume_world_reply(
                        session,
                        retained,
                        shared,
                        focus_queue,
                        focus_capacity,
                        &applied,
                        &effects,
                        ticket.command.as_ref(),
                        ticket.command_seq,
                        op,
                        "accepted",
                        pid,
                        generation,
                        seq,
                    );
                    DispatchVerdict::Applied { select_operation }
                }
                ReplyVerdict::Refused { reason } => {
                    // Newer input landed mid-flight (or a fence tripped):
                    // actuate NOTHING from the stale reply. The command (if
                    // any) already executed core-side (at-most-once). When
                    // no honest successor is coming, one deferred
                    // observation-only retry recovers instead of stalling;
                    // otherwise the queued successors each dispatch an
                    // honestly fenced snapshot next.
                    count_token(shared, &format!("{op}-refused-{reason}"));
                    emit(shared, pid, generation, seq, "core-reply", reason, 0);
                    session.sequencer().clear_flight();
                    // A stale-observation refusal proves the core ran the
                    // carried command: a planner-owned select still binds
                    // its operation identity so the retained transfer
                    // settles through the normal drain-feedback loop
                    // instead of leaking unbound. Every other refusal
                    // binds nothing (the core never ran).
                    let stale_select = command_is_select
                        && reason == "stale-observation"
                        && ticket.command_seq.is_some();
                    let select_operation = stale_select.then(|| select_operation.clone()).flatten();
                    if reason == "stale-observation"
                        && retry_allowed
                        && let Some(recovered) = maybe_retry(
                            session,
                            retained,
                            shared,
                            inbox,
                            focus_queue,
                            focus_capacity,
                            dispatch,
                            select_operation.clone(),
                            pid,
                            generation,
                            seq,
                        )
                    {
                        return recovered;
                    }
                    DispatchVerdict::Refused {
                        token: reason.to_owned(),
                        select_operation,
                    }
                }
            }
        }
        WorldOutcome::CommandConsumed(reply) => {
            // Fresh report, already-consumed command: the observation
            // reconciled, the command never re-executed. Consumed exactly
            // like an accept (effects dedupe by id, so replays never
            // reapply).
            match session.sequencer().accept_reply(&reply) {
                ReplyVerdict::Applied(applied) => {
                    let Some(effects) = spend_if_coherent(
                        session, retained, shared, inbox, &dispatch, &applied, op, pid, generation,
                        seq,
                    ) else {
                        // The accepted command (if any) already executed
                        // core-side, so a planner-owned select binds exactly
                        // as on a pre-accept stale refusal; when no honest
                        // successor is coming, one deferred retry recovers.
                        let bind = (command_is_select && ticket.command_seq.is_some())
                            .then(|| select_operation.clone())
                            .flatten();
                        if retry_allowed
                            && let Some(recovered) = maybe_retry(
                                session,
                                retained,
                                shared,
                                inbox,
                                focus_queue,
                                focus_capacity,
                                dispatch,
                                bind.clone(),
                                pid,
                                generation,
                                seq,
                            )
                        {
                            return recovered;
                        }
                        return DispatchVerdict::Refused {
                            token: "stale-observation".to_owned(),
                            select_operation: bind,
                        };
                    };
                    consume_world_reply(
                        session,
                        retained,
                        shared,
                        focus_queue,
                        focus_capacity,
                        &applied,
                        &effects,
                        ticket.command.as_ref(),
                        ticket.command_seq,
                        op,
                        "consumed",
                        pid,
                        generation,
                        seq,
                    );
                    DispatchVerdict::Applied { select_operation }
                }
                ReplyVerdict::Refused { reason } => {
                    count_token(shared, &format!("{op}-refused-{reason}"));
                    emit(shared, pid, generation, seq, "core-reply", reason, 0);
                    session.sequencer().clear_flight();
                    DispatchVerdict::Refused {
                        token: reason.to_owned(),
                        select_operation: None,
                    }
                }
            }
        }
        WorldOutcome::Duplicate(_) => {
            session.sequencer().clear_flight();
            // Identical latest resend: no new revision, no re-emission
            // (latest-wins slots already hold the desired state).
            count_token(shared, &format!("{op}-duplicate"));
            emit(shared, pid, generation, seq, "core-reply", "duplicate", 0);
            DispatchVerdict::Duplicate
        }
        WorldOutcome::Rejected { kind, .. } => {
            session.sequencer().clear_flight();
            count_token(shared, &format!("{op}-rejected-{kind}"));
            emit(shared, pid, generation, seq, "core-reply", kind, 0);
            DispatchVerdict::Refused {
                token: format!("rejected-{kind}"),
                select_operation: None,
            }
        }
    }
}

/// True when a deferred observation-only retry may recover a stale
/// refusal: no still-queued job will incorporate (so no successor is
/// coming to converge instead) and the dispatch topology is still current
/// (so the retry's snapshot is honest). A retry that would refuse again
/// is skipped — under a submit flood every job refuses until the last,
/// which applies. In particular a will-drop mark (stale sequence,
/// dead lifetime, drifted revision) never blocks the retry: its truth is
/// already superseded or invalid, so the retried snapshot is complete
/// without it.
fn retry_worthwhile(
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    dispatch: &DispatchContext,
    retained: &RetainedCore,
) -> bool {
    let pending = inbox.coherence_pending();
    let guard = lock_shared(shared);
    if !topology_current(&guard, dispatch) {
        return false;
    }
    !pending
        .iter()
        .any(|job| will_incorporate(&guard, retained, job))
}

/// Deferred honest catch-up: re-report the refused job's retained truth
/// once, observation-only, through the full honesty path (fresh stamp,
/// acceptance, emission gate). The refused command (if any) already
/// executed core-side, so the retry carries none and at-most-once holds;
/// the recovered verdict keeps the original select binding. A retry that
/// cannot be honest refuses again instead of actuating stale. Depth is
/// one — a refused retry never re-retries.
#[allow(clippy::too_many_arguments)]
fn retry_observation_only(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    dispatch: DispatchContext,
    select_operation: Option<String>,
    pid: u32,
    generation: u64,
    seq: u64,
) -> DispatchVerdict {
    use crate::world_command_map::OP_RECONCILE;
    session.sequencer().clear_flight();
    let Some(retry) = session.sequencer().try_begin_request(None) else {
        return DispatchVerdict::Refused {
            token: "refused-no-flight".to_owned(),
            select_operation,
        };
    };
    match dispatch_ticket(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        dispatch,
        BTreeMap::new(),
        retry,
        false,
        false,
        OP_RECONCILE,
        pid,
        generation,
        seq,
    ) {
        // The refused command already executed core-side: a recovered
        // apply still settles under its original operation identity.
        DispatchVerdict::Applied { .. } => DispatchVerdict::Applied { select_operation },
        DispatchVerdict::Refused { token, .. } => DispatchVerdict::Refused {
            token,
            select_operation,
        },
        // Duplicates and empty contexts carry no command outcome either
        // way; pass through unchanged.
        other => other,
    }
}

/// Attempt the deferred retry when it may recover, else `None` (the
/// caller returns its plain refusal and the queued successors converge).
#[allow(clippy::too_many_arguments)]
fn maybe_retry(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    dispatch: DispatchContext,
    select_operation: Option<String>,
    pid: u32,
    generation: u64,
    seq: u64,
) -> Option<DispatchVerdict> {
    if !retry_worthwhile(shared, inbox, &dispatch, retained) {
        return None;
    }
    Some(retry_observation_only(
        session,
        retained,
        shared,
        inbox,
        focus_queue,
        focus_capacity,
        dispatch,
        select_operation,
        pid,
        generation,
        seq,
    ))
}

/// Post-accept emission coherence: an accepted reply still must not touch
/// native setters unless its snapshot provably incorporates every
/// submitted input. Three legs, all re-read here (after acceptance, before
/// effects spend or any slot/outbox write):
///
/// - watermark equality: the shared clock still equals the reply's stamped
///   observation. Any submit since the ticket fails this, so an old report
///   plus newer submitted input never yields old setters.
/// - topology currency: a native dispatch's snapshot revision still equals
///   the live shared revision with a context present. This catches the
///   mark-without-job update that landed after the dispatch snapshot but
///   before the ticket stamped (its mark is inside the stamp while the
///   report holds retired domains).
/// - no incorporable input still queued: every still-queued observation
///   or terminate is classified against the same fences its processing
///   would apply (lifetime, sequence, context revision): one that will
///   incorporate blocks actuation (its truth is missing from this
///   snapshot), while one that will drop as stale is ignored. Ignoring is
///   load-bearing for liveness: a drop dispatches no successor, so
///   blocking on it would stall convergence with no recovery — while a
///   blocker always dequeues next and incorporates (retained state cannot
///   change under it before then), so convergence is never lost.
///   Queued commands and feedback are not coherence-breaking: they change
///   planned state, not observed truth, and refusing on them would let a
///   gated-away command suppress an honest observation forever; their own
///   dispatches converge next.
///
/// A saturated-queue over-mark (see `invalidate_clock`) is claimed by a
/// coherent dispatch (its truth was never queued; the worker retries it
/// with a fresh mark).
fn emission_coherent(
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    dispatch: &DispatchContext,
    reply: &WorldReply,
    retained: &RetainedCore,
) -> bool {
    // Snapshot the queue first (reads only, lock released before the
    // shared lock below — never nested, so no lock-order risk). Jobs only
    // leave the queue through the engine (busy dispatching here) or
    // shutdown drain (after which no gate runs), so the snapshot is exact;
    // a submit landing after it still moves the clock and fails the
    // equality leg below.
    let pending = inbox.coherence_pending();
    let guard = lock_shared(shared);
    if guard.clock.get() != reply.based_on_observation_seq {
        return false;
    }
    if !topology_current(&guard, dispatch) {
        return false;
    }
    !pending
        .iter()
        .any(|job| will_incorporate(&guard, retained, job))
}

/// Native dispatch topology is still current: the snapshot revision equals
/// the live shared revision with a context present. Synthetic dispatches
/// carry no topology and always pass.
fn topology_current(guard: &Shared, dispatch: &DispatchContext) -> bool {
    match dispatch {
        DispatchContext::Native(ctx) => guard.native_context.as_ref().is_some_and(|live| {
            ctx.revision == live.revision && ctx.revision == guard.topology_revision
        }),
        DispatchContext::Synthetic => true,
    }
}

/// True when one queued coherence-relevant job will incorporate (rather
/// than drop) once dequeued, mirroring the processing fences exactly: the
/// legacy observation arm's lifetime/sequence gates, the scoped arm's
/// lifetime/sequence/context/revision gates, and the terminate arm's
/// generation gate. A blocker carries unincorporated truth, so the current
/// snapshot must not actuate; anything else resolves on dequeue with no
/// state change and no successor dispatch.
fn will_incorporate(guard: &Shared, retained: &RetainedCore, job: &CoherencePending) -> bool {
    // Lifetime gate shared by both observation arms: a dead or relaunched
    // app drops, exactly as processing would.
    let app_valid = |pid: u32, generation: u64| {
        guard
            .apps
            .get(&pid)
            .is_some_and(|r| r.generation == generation && !r.terminated)
    };
    // Per-app sequence fence with generation reset on PID reuse, exactly
    // as processing computes it.
    let fresh = |pid: u32, generation: u64, seq: u64| match retained.apps.get(&pid) {
        Some(app) if app.generation == generation => seq > app.seq_last,
        _ => true,
    };
    match job {
        CoherencePending::Observation {
            pid,
            generation,
            seq,
        } => app_valid(*pid, *generation) && fresh(*pid, *generation, *seq),
        CoherencePending::ScopedObservation {
            pid,
            generation,
            seq,
            revision,
        } => {
            app_valid(*pid, *generation)
                && fresh(*pid, *generation, *seq)
                && guard.native_context.as_ref().is_some_and(|ctx| {
                    *revision == ctx.revision && *revision == guard.topology_revision
                })
        }
        // A stale terminate (PID reused since) drops; a live one forgets
        // members and dispatches, so it blocks.
        CoherencePending::Terminate { pid, generation } => !retained
            .apps
            .get(pid)
            .is_some_and(|app| app.generation != *generation),
    }
}

/// Run the post-accept test rendezvous (if set), then prove emission
/// coherence for one accepted reply. Returns the newly spent one-shot
/// effects when the snapshot is still current; returns `None` (counted as
/// `stale-observation`, spending nothing) when input landed between
/// acceptance and emission — the queued successors converge instead.
#[allow(clippy::too_many_arguments)]
fn spend_if_coherent(
    session: &mut WorldSession,
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    dispatch: &DispatchContext,
    applied: &WorldReply,
    op: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) -> Option<Vec<OneShotEffect>> {
    // Deterministic-test rendezvous (see `PostAcceptHook`): a test parks
    // here to advance the watermark between acceptance and emission.
    // Cloned out under a short lock, invoked with no locks held;
    // production never sets it.
    let post_accept = lock_shared(shared)
        .post_accept_hook
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if let Some(hook) = post_accept {
        hook();
    }
    // Freshness is re-proven AFTER acceptance and BEFORE any native
    // setter: input that landed mid-flight (or a topology that moved
    // under the dispatch snapshot) refuses here even though the sequencer
    // accepted. Effects stay unspent and nothing emits. See
    // `emission_coherent`.
    if !emission_coherent(shared, inbox, dispatch, applied, retained) {
        count_token(shared, &format!("{op}-refused-stale-observation"));
        emit(
            shared,
            pid,
            generation,
            seq,
            "core-reply",
            "stale-observation",
            0,
        );
        return None;
    }
    // Spend one-shot effects BEFORE first native dispatch: duplicates and
    // repeated diffs never reapply.
    Some(session.sequencer().take_unspent_effects(applied))
}

fn count_token(shared: &Arc<Mutex<Shared>>, token: &str) {
    lock_shared(shared).count_reply(token);
}

/// Static scope-hold token for the ring (`emit` needs `&'static str`):
/// every [`HoldReason`](tiler_core::world_report::HoldReason) is
/// enumerated, so a held scope names its reason with no per-reply
/// allocation.
fn hold_token(reason: tiler_core::world_report::HoldReason) -> &'static str {
    use tiler_core::world_report::HoldReason;
    match reason {
        HoldReason::Unreadable => "held-unreadable",
        HoldReason::IncompleteInventory => "held-incomplete-inventory",
        HoldReason::GestureBarrier => "held-gesture-barrier",
        HoldReason::RefusedTarget => "held-refused-target",
        HoldReason::AmbiguousCreation => "held-ambiguous-creation",
        HoldReason::OccupiedRemoval => "held-occupied-removal",
        HoldReason::ConstraintClamp => "held-constraint-clamp",
        HoldReason::Minimized => "held-minimized",
    }
}

/// Consume one applied world reply: current-scope controlled frames into
/// per-app latest-wins slots, focus effects into the outbox plus the
/// owning slot, view-transfer hold settling for pending selects, and the
/// desired-geometry snapshot. Hidden/unreadable scopes never emit: only
/// currently-actuated domains with `Controlled` frame authority and an
/// effective frame write.
#[allow(clippy::too_many_arguments)]
fn consume_world_reply(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    reply: &WorldReply,
    effects: &[OneShotEffect],
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
    op: &'static str,
    outcome: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let current: BTreeSet<(String, String)> = {
        let guard = lock_shared(shared);
        match guard.native_context.clone() {
            Some(ctx) => ctx
                .domains
                .iter()
                .filter(|d| d.current)
                .map(|d| (d.output.0.clone(), d.workspace.0.clone()))
                .collect(),
            None => [(PHASE1_OUTPUT.to_owned(), PHASE1_WORKSPACE.to_owned())]
                .into_iter()
                .collect(),
        }
    };
    // Held-scope visibility (diagnostic only): an applied reply whose scope
    // holds authorizes no writes there, yet still counts as accepted with
    // zero frames — without this line the hold is silent in the ring. One
    // bounded line per held scope (holds are exceptional; controlled and
    // untouched scopes log nothing).
    for scope in &reply.scopes {
        if let Some(reason) = scope.authority.hold_reason() {
            emit(shared, pid, generation, seq, "scope", hold_token(reason), 0);
        }
    }
    // Plan-truth snapshot (current scopes only, matching the legacy merged
    // emission): every row carrying geometry — effective frame preferred,
    // allocation fallback (overlays keep plan truth without writes).
    let desired: Vec<DesiredFrame> = reply
        .windows
        .iter()
        .filter(|row| current.contains(&(row.output.0.clone(), row.workspace.clone())))
        .filter_map(|row| {
            let frame = row.effective_frame.or(row.allocation)?;
            Some(DesiredFrame {
                window: row.lifetime.0.clone(),
                x: frame.x,
                y: frame.y,
                w: frame.w,
                h: frame.h,
            })
        })
        .collect();
    // Overlay withhold accounting (legacy `frames_overlay_skipped`
    // parity): current-scope rows withheld from emission while retained
    // truth still shows a native fullscreen overlay. Plan truth is
    // untouched (only actuation is filtered — the core already withholds
    // overlay geometry; this counts the adapter-visible hold).
    let mut overlay_skipped = 0u64;
    for row in &reply.windows {
        if row.frame_authority == ScopeAuthority::Controlled && row.effective_frame.is_some() {
            continue;
        }
        if !current.contains(&(row.output.0.clone(), row.workspace.clone())) {
            continue;
        }
        if row.allocation.is_none() && row.effective_frame.is_none() {
            continue;
        }
        if is_retained_fullscreen(retained, &row.lifetime.0) {
            overlay_skipped += 1;
        }
    }
    // Emission: Controlled frame authority plus an effective frame in a
    // current scope. Explicit placement tokens (`frame_operation`) write
    // exactly once (spent set); ordinary floats/overlays/holds never
    // write — holds preserve native state.
    let mut frames: Vec<(String, Rect)> = Vec::new();
    let mut frame_ops: Vec<(String, String)> = Vec::new();
    for row in &reply.windows {
        if row.frame_authority != ScopeAuthority::Controlled {
            continue;
        }
        let Some(frame) = row.effective_frame else {
            continue;
        };
        if !current.contains(&(row.output.0.clone(), row.workspace.clone())) {
            continue;
        }
        if let Some(operation) = row.frame_operation.clone() {
            if !session.spend_frame_op(&operation) {
                continue;
            }
            frame_ops.push((row.lifetime.0.clone(), operation));
        }
        frames.push((row.lifetime.0.clone(), frame));
    }
    emit_world_frames(
        session,
        retained,
        shared,
        frames,
        frame_ops,
        desired,
        overlay_skipped,
        op,
        outcome,
        pid,
        generation,
        seq,
    );
    dispatch_focus_effects(
        session,
        retained,
        shared,
        focus_queue,
        focus_capacity,
        effects,
        ticket_command,
        ticket_command_seq,
        pid,
        generation,
        seq,
    );
    settle_select_transfers(shared, reply, pid, generation, seq);
}

/// Frame emission from world rows: per-app latest-wins slots, per-app
/// command sequencing, whole-reply empty-set supersede, and the same
/// counters/ring discipline as the legacy path.
#[allow(clippy::too_many_arguments)]
fn emit_world_frames(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    frames: Vec<(String, Rect)>,
    frame_ops: Vec<(String, String)>,
    desired: Vec<DesiredFrame>,
    overlay_skipped: u64,
    op: &'static str,
    outcome: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let frame_op_by_window: BTreeMap<&str, &str> = frame_ops
        .iter()
        .map(|(window, operation)| (window.as_str(), operation.as_str()))
        .collect();
    let (slots, topology_revision): (LiveSlots, u64) = {
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
        )
    };
    let mut by_app: BTreeMap<u32, Vec<FrameCommand>> = BTreeMap::new();
    let mut unparseable = 0u64;
    for (window, rect) in &frames {
        let Some((wpid, wid)) = parse_window_id(window) else {
            unparseable += 1;
            continue;
        };
        let app_generation = slots
            .iter()
            .find(|(p, _, _, _)| *p == wpid)
            .map(|(_, generation, _, _)| *generation)
            .unwrap_or(0);
        let cmd_id = retained
            .apps
            .get_mut(&wpid)
            .map(|app| {
                app.cmd_counter += 1;
                app.cmd_counter
            })
            .unwrap_or(0);
        // Write correlation for explicit placement tokens only
        // (proven-outcome feedback — never false success; ordinary rows
        // stage nothing and occupy no correlation space, see
        // `track_write`).
        track_write(
            session,
            wpid,
            cmd_id,
            window,
            frame_op_by_window.get(window.as_str()).copied(),
        );
        by_app.entry(wpid).or_default().push(FrameCommand {
            pid: wpid,
            generation: app_generation,
            cmd_id,
            window: window.clone(),
            wid,
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
            topology_revision,
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
    // A whole applied reply supersedes pending state: live apps with no
    // windows in this reply converge to empty instead of actuating stale
    // frames. Duplicates and refusals never reach here (no clobber).
    for (wpid, _, slot, _) in &slots {
        if !by_app.contains_key(wpid)
            && let Some(overwritten) = slot.set_latest(Vec::new())
        {
            coalesced += overwritten as u64;
        }
    }
    {
        let mut guard = lock_shared(shared);
        guard.count_reply(&format!("{op}-{outcome}"));
        guard.diagnostics.frames_emitted += emitted;
        guard.diagnostics.frames_coalesced += coalesced;
        guard.diagnostics.frames_overlay_skipped += overlay_skipped;
        guard.diagnostics.terminated_invalidated += dead;
        guard.last_desired = desired;
    }
    let token = reply_token(op, outcome);
    emit(shared, pid, generation, seq, "core-reply", token, emitted);
    if emitted > 0 {
        emit(shared, pid, generation, seq, "frame", "emitted", emitted);
    }
}

/// Focus dispatch from spent one-shot effects: the bounded controller
/// outbox plus the owning app's latest-wins slot (native mode), sharing
/// one `request_id`. Only the explicit focus outcome of the dispatched
/// command dispatches — observations and automatic admission intents never
/// focus (legacy parity: reconcile/admit/move re-plan geometry without
/// moving native focus; observations keep observed-focus authority).
/// Admission intents stay spent (at-most-once) but actuate nothing and
/// move no counter. Raise/Stack effects spend without a native route
/// (counted evidence, never faked as focus).
#[allow(clippy::too_many_arguments)]
fn dispatch_focus_effects(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    effects: &[OneShotEffect],
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    use tiler_core::boundary::CoreCommand;
    use tiler_core::world_report::EffectKind;
    // Structured explicit-focus context (never an id-prefix heuristic):
    // the dispatched command is an explicit focus step, so exactly its
    // correlated `fx-<seq>` outcome may dispatch. Any other ticket —
    // observation-only, move, select, or a consumed replay — dispatches
    // no focus even when the reply carries admission intents.
    let explicit_focus_seq = match (ticket_command, ticket_command_seq) {
        (Some(WorldCommand::Focus { .. }), Some(seq))
        | (Some(WorldCommand::Operation(CoreCommand::Focus { .. })), Some(seq)) => Some(seq),
        _ => None,
    };
    let (slots, topology_revision, native_mode): (LiveSlots, u64, bool) = {
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
            matches!(guard.config.topology_mode, TopologyMode::Native),
        )
    };
    let mut focus_dispatched = 0u64;
    let mut focus_dropped = 0u64;
    let mut unhandled_effects = 0u64;
    for effect in effects {
        match effect.kind {
            EffectKind::Focus => {
                // Legacy observed-focus authority: without an explicit
                // focus ticket this is an automatic admission intent —
                // spent, never actuated. On an explicit ticket only the
                // command's own correlated outcome dispatches (a coincident
                // admission intent on the same reply still must not steal
                // native focus).
                let Some(seq) = explicit_focus_seq else {
                    continue;
                };
                if !effect.is_explicit_focus_for(seq) {
                    continue;
                }
                let Some((fpid, fwid)) = parse_window_id(&effect.target.0) else {
                    focus_dropped += 1;
                    continue;
                };
                let (app_generation, focus_slot) = slots
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
                    generation: app_generation,
                    request_id,
                    wid: fwid,
                    window: effect.target.0.clone(),
                    dispatched: true,
                    verified: false,
                    topology_revision,
                };
                let mut queue = lock_focus(focus_queue);
                if queue.len() < focus_capacity {
                    queue.push(request);
                    focus_dispatched += 1;
                } else {
                    focus_dropped += 1;
                }
                drop(queue);
                // Owner actuation path, native mode only (the synthetic
                // skeleton has no per-app focus dispatch; its workers
                // never read the slot, so emitting there would only
                // perturb the exact outbox counters the legacy tests
                // pin). A stopped slot means the app is terminating: the
                // command dies with its lifetime, counted as dropped.
                if native_mode && let Some(slot) = focus_slot {
                    let cmd = FocusCommand {
                        pid: fpid,
                        generation: app_generation,
                        request_id,
                        wid: fwid,
                        window: effect.target.0.clone(),
                        revision: topology_revision,
                        route: session.focus_route(),
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
            EffectKind::Raise | EffectKind::Stack => {
                // No macOS native route: spent (never reapplied), counted
                // evidence only.
                unhandled_effects += 1;
            }
        }
    }
    {
        let mut guard = lock_shared(shared);
        guard.diagnostics.focus_dispatched += focus_dispatched;
        guard.diagnostics.focus_dropped_full += focus_dropped;
    }
    if focus_dropped > 0 {
        emit(shared, pid, generation, seq, "focus", "dropped-full", 1);
    }
    if unhandled_effects > 0 {
        emit(
            shared,
            pid,
            generation,
            seq,
            "effect",
            "unhandled-without-route",
            unhandled_effects,
        );
    }
}

/// Settle pending select claims against retained view transfers: a held
/// target (possibly-executed) drops the pending claim so the drain never
/// re-actuates it — holds preserve native state. Live targets stay for
/// the main-thread drain with its own fresh fences.
fn settle_select_transfers(
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReply,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let pending_op: Option<String> = lock_shared(shared)
        .select_pending
        .as_ref()
        .and_then(|claim| claim.operation_id.clone());
    let Some(operation_id) = pending_op else {
        return;
    };
    let held = reply
        .transfers
        .iter()
        .find(|target| target.operation_id == operation_id)
        .is_some_and(|target| target.held);
    if held {
        lock_shared(shared).select_pending = None;
        emit(shared, pid, generation, seq, "input", "select-held", 0);
    }
}
