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
use tiler_core::gesture::{GestureIngress, GestureSettled};
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{OneShotEffect, ScopeAuthority, WorldCommand, WorldReply};

use crate::focus_route::FocusRoute;
use crate::runtime::{
    DeferredSelectionFocus, DesiredFrame, FocusBaselines, FocusCommand, FocusRequest, FrameCommand,
    Shared, emit, lock_focus, lock_shared, parse_window_id,
};
use crate::runtime_core::RetainedCore;
use crate::runtime_thread::{CoherencePending, FocusSlot, FrameSlot, Inbox};
use crate::runtime_types::TopologyMode;
use crate::world_runtime::{
    DispatchContext, WorldSession, build_dispatch_report, map_gesture_preview, track_write,
};
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
        ("drag-gesture", "accepted") => "drag-gesture-accepted",
        ("drag-gesture", "consumed") => "drag-gesture-consumed",
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
#[derive(Debug)]
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
    closed: Vec<WindowId>,
    command: Option<WorldCommand>,
    gesture: Option<GestureIngress>,
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
    // The startup-resolved presentation style syncs here too, so every
    // report carries the configured outset without the engine thread
    // reading startup config directly.
    session.sync_presentation_style(lock_shared(shared).presentation_style);
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
            gesture,
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
        gesture,
        command_is_select,
        true,
        op,
        pid,
        generation,
        seq,
    )
}

#[allow(clippy::too_many_arguments)]
/// Gesture lifecycle dispatch: one complete report carrying `ingress`
/// with no planner command. Mirrors `dispatch_ticket` without retries:
/// a stale reply actuates nothing and never redispatches observation-only
/// (a terminal already consumed its id core-side, so a replay stays dead;
/// queued successors converge next). Returns the verdict plus the
/// requester-correlated transient outputs: the read-only preview outcome
/// and the incorporated terminal, if any. Exactly one engine transaction
/// per call; duplicates never re-emit.
pub(crate) fn dispatch_gesture_report(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    dispatch: DispatchContext,
    ingress: GestureIngress,
    op: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) -> (
    DispatchVerdict,
    crate::drag_drop::PreviewOutcome,
    Option<GestureSettled>,
) {
    use crate::drag_drop::PreviewOutcome;
    session.sync_presentation_style(lock_shared(shared).presentation_style);
    if let DispatchContext::Native(ctx) = &dispatch
        && crate::runtime_core::note_dispatch_context(retained, ctx)
    {
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
    let Some(ticket) = session.sequencer().try_begin_request(None) else {
        session.sequencer().clear_flight();
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
        return (
            DispatchVerdict::Refused {
                token: "refused-no-flight".to_owned(),
                select_operation: None,
            },
            PreviewOutcome::CancelDead {
                reason: "single-flight",
            },
            None,
        );
    };
    let hook = lock_shared(shared)
        .midflight_hook
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
    let Some(report) = build_dispatch_report(
        session,
        retained,
        &dispatch,
        &[],
        ticket.observation_seq,
        ticket.request_seq,
        ticket.command_seq,
        ticket.command.clone(),
        Some(ingress),
        &ticket.request_id,
    ) else {
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
        return (
            DispatchVerdict::NoReport,
            PreviewOutcome::CancelDead {
                reason: "no-context",
            },
            None,
        );
    };
    match session.engine_mut().handle_report(&report) {
        WorldOutcome::Accepted(reply) => match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => {
                let Some(effects) = spend_if_coherent(
                    session, retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
                ) else {
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
                    return (
                        DispatchVerdict::Refused {
                            token: "stale-observation".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                };
                let preview = map_gesture_preview(&applied);
                let settled = applied.gesture_settled;
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
                (
                    DispatchVerdict::Applied {
                        select_operation: None,
                    },
                    preview,
                    settled,
                )
            }
            ReplyVerdict::Refused { reason } => {
                count_token(shared, &format!("{op}-refused-{reason}"));
                emit(shared, pid, generation, seq, "core-reply", reason, 0);
                session.sequencer().clear_flight();
                (
                    DispatchVerdict::Refused {
                        token: reason.to_owned(),
                        select_operation: None,
                    },
                    PreviewOutcome::HideTransient { reason: "held" },
                    None,
                )
            }
        },
        WorldOutcome::CommandConsumed(reply) => match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => {
                let Some(effects) = spend_if_coherent(
                    session, retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
                ) else {
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
                    return (
                        DispatchVerdict::Refused {
                            token: "stale-observation".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                };
                let preview = map_gesture_preview(&applied);
                let settled = applied.gesture_settled;
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
                (
                    DispatchVerdict::Applied {
                        select_operation: None,
                    },
                    preview,
                    settled,
                )
            }
            ReplyVerdict::Refused { reason } => {
                count_token(shared, &format!("{op}-refused-{reason}"));
                emit(shared, pid, generation, seq, "core-reply", reason, 0);
                session.sequencer().clear_flight();
                (
                    DispatchVerdict::Refused {
                        token: reason.to_owned(),
                        select_operation: None,
                    },
                    PreviewOutcome::HideTransient { reason: "held" },
                    None,
                )
            }
        },
        WorldOutcome::Duplicate(reply) => {
            session.sequencer().clear_flight();
            count_token(shared, &format!("{op}-duplicate"));
            emit(shared, pid, generation, seq, "core-reply", "duplicate", 0);
            (
                DispatchVerdict::Duplicate,
                map_gesture_preview(&reply),
                reply.gesture_settled,
            )
        }
        WorldOutcome::Rejected { kind, .. } => {
            session.sequencer().clear_flight();
            count_token(shared, &format!("{op}-rejected-{kind}"));
            emit(shared, pid, generation, seq, "core-reply", kind, 0);
            (
                DispatchVerdict::Refused {
                    token: format!("rejected-{kind}"),
                    select_operation: None,
                },
                crate::drag_drop::map_preview_rejection(kind),
                None,
            )
        }
    }
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
    closed: Vec<WindowId>,
    ticket: RequestTicket,
    gesture: Option<GestureIngress>,
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
        &closed,
        ticket.observation_seq,
        ticket.request_seq,
        ticket.command_seq,
        ticket.command.clone(),
        gesture,
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
                    for id in &report.closed {
                        session.pending_closes.remove(id);
                    }
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
                    for id in &report.closed {
                        session.pending_closes.remove(id);
                    }
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
            // Correlated refusal detail (stderr only, rejected SEND path
            // only): the requested send shape plus the report
            // observation/request ids the rejection consumed and the
            // held-scope detail, so a fenced send names its subject and
            // its fence instead of counting anonymously. Non-send
            // refusals stay ring-only (never a `send refused` line).
            if let Some(command) = ticket.command.as_ref()
                && matches!(
                    command,
                    WorldCommand::SendNumbered { .. }
                        | WorldCommand::SendRelative { .. }
                        | WorldCommand::Send { .. }
                        | WorldCommand::SendDirectional { .. }
                )
            {
                let (subject, index, follow, source) =
                    crate::workspace_send::describe_send_command(command);
                eprintln!(
                    "{}",
                    crate::workspace_send::format_send_refused_line(
                        op,
                        kind,
                        &subject,
                        &index,
                        follow,
                        &source,
                        ticket.observation_seq,
                        ticket.request_seq,
                        &ticket.request_id,
                        ticket.command_seq,
                        &held_detail(&dispatch, retained),
                    )
                );
            }
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
        Vec::new(),
        retry,
        None,
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

/// Held-scope detail for the refusal line: `output/workspace:reason` per
/// unreadable dispatch scope plus `roster:{pid}:{cause}` per incomplete
/// roster (`-` when nothing is held). Metadata only (static tokens plus
/// IDs, never content); computed on the rejected-send path only, so steady
/// state pays nothing.
fn held_detail(dispatch: &DispatchContext, retained: &RetainedCore) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let DispatchContext::Native(ctx) = dispatch {
        for domain in &ctx.domains {
            if !domain.readable {
                parts.push(format!(
                    "{}/{}:unreadable",
                    domain.output.0, domain.workspace.0
                ));
            }
        }
    }
    let mut roster: Vec<String> = retained
        .apps
        .iter()
        .filter(|(_, app)| app.roster_incomplete)
        .map(|(pid, app)| format!("roster:{pid}:{}", app.roster_cause))
        .collect();
    roster.sort();
    parts.extend(roster);
    if parts.is_empty() {
        return "-".to_owned();
    }
    parts.join(",")
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
    // Currently-actuated scopes as (output, LOGICAL workspace) pairs:
    // native current domains translated through the applied reply's
    // `native_binding` rows (a current-reply live lookup, never a durable
    // host table). Desired rows home to core logical workspaces, so the
    // native ids must translate before the current-scope filter below.
    // Unmapped natives fall back to their own id (matches nothing: an
    // unmapped home authorizes no rows).
    let current: BTreeSet<(String, String)> = {
        let guard = lock_shared(shared);
        match guard.native_context.clone() {
            Some(ctx) => ctx
                .domains
                .iter()
                .filter(|d| d.current)
                .map(|d| {
                    let logical = reply
                        .workspaces
                        .iter()
                        .find(|row| {
                            row.output == d.output
                                && row.native_binding.as_deref() == Some(d.workspace.0.as_str())
                        })
                        .map(|row| row.id.clone())
                        .unwrap_or_else(|| d.workspace.0.clone());
                    (d.output.0.clone(), logical)
                })
                .collect(),
            // No native context (synthetic skeleton): the reply's own
            // retained logical views name the actuated scopes.
            None => reply
                .outputs
                .iter()
                .filter_map(|out| {
                    out.logical_view
                        .clone()
                        .map(|view| (out.output.0.clone(), view))
                })
                .collect(),
        }
    };
    // Pager binding rows for the menu-bar indicator: every native domain
    // (current AND hidden) translated through the applied reply's
    // `native_binding` rows — the same live-lookup pattern as `current`
    // above, extended to the hidden Spaces the indicator also shows.
    // Unmapped natives and unbound spares carry no row (their segments
    // render with no miniatures, never a guess). Published next to the
    // presentation below.
    let pager_bindings: Vec<crate::pager::PagerWorkspaceBinding> = {
        let guard = lock_shared(shared);
        match guard.native_context.clone() {
            Some(ctx) => {
                let domains: Vec<(String, String)> = ctx
                    .domains
                    .iter()
                    .map(|d| (d.output.0.clone(), d.workspace.0.clone()))
                    .collect();
                crate::pager::pager_bindings_for(&domains, &reply.workspaces)
            }
            None => Vec::new(),
        }
    };
    // Held-scope visibility (diagnostic only): an applied reply whose scope
    // holds authorizes no writes there, yet still counts as accepted with
    // zero frames — the ring token below keeps the hold visible without a
    // per-scope stderr line (holds persist across observations, so per-held
    // scope stderr would spam steady state; the send-refusal line carries
    // the held detail on the rejected path instead). Controlled and
    // untouched scopes log nothing.
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
    // Applied-reply presentation publish: exactly the applied reply's
    // shapes ride shared state for the tile main thread, which renders
    // these shapes with liveness plus fresh native safety vetoes before
    // painting. Duplicates and refusals never reach here (no clobber).
    {
        let mut guard = lock_shared(shared);
        guard.last_presentation = reply.presentation.clone();
        guard.last_pager_bindings = pager_bindings;
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
        reply,
        effects,
        ticket_command,
        ticket_command_seq,
        pid,
        generation,
        seq,
    );
    settle_select_transfers(session, shared, reply, pid, generation, seq);
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
            *rect,
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

/// Selection tickets whose remembered-leaf focus outcome defers instead
/// of dispatching: the native Dock-swipe selection must prove arrival on
/// the bound Space first. Every other ticket (observation-only, move,
/// send, explicit focus, consumed replay) binds nothing here.
pub(crate) fn selection_ticket_seq(
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
) -> Option<u64> {
    let seq = ticket_command_seq?;
    match ticket_command {
        Some(
            WorldCommand::SelectWorkspace { .. }
            | WorldCommand::SelectNumbered { .. }
            | WorldCommand::SelectRelative { .. }
            | WorldCommand::SelectPrevious { .. }
            | WorldCommand::SelectTrailing { .. }
            | WorldCommand::SwitchView { .. },
        ) => Some(seq),
        _ => None,
    }
}

/// Resolve the deferred selection focus for one replied selection effect:
/// the effect must be the command's own correlated `fx-<seq>` outcome and
/// the caller must supply the reply's live native binding for the retained
/// View target (`None` when unmapped). The binding must parse to a live
/// Space id, and every baseline must be present (observed focus, foreground
/// PID — never guessed). `None` on any gap: refusal direction, never a
/// guessed target or baseline.
pub(crate) fn deferred_selection_focus_for(
    operation_id: &str,
    command_seq: u64,
    effect: &OneShotEffect,
    native_binding: Option<String>,
    route: FocusRoute,
    baselines: FocusBaselines,
) -> Option<DeferredSelectionFocus> {
    if !effect.is_explicit_focus_for(command_seq) {
        return None;
    }
    let expected_sid = crate::workspace_select::parse_native_binding(native_binding?.as_str())?;
    let FocusBaselines {
        app_generation,
        prior_focus,
        prior_frontmost,
    } = baselines;
    let (prior_focus, prior_frontmost) = match (prior_focus, prior_frontmost) {
        (Some(focus), Some(frontmost)) => (focus, frontmost),
        _ => return None,
    };
    Some(DeferredSelectionFocus {
        operation_id: operation_id.to_owned(),
        command_seq,
        window: effect.target.clone(),
        route,
        expected_sid,
        app_generation,
        prior_focus: Some(prior_focus),
        prior_frontmost: Some(prior_frontmost),
    })
}

/// Drop any deferred selection focus without dispatch (supersession or
/// refusal path): counted evidence, never actuated. Locking helper shared
/// by dispatch paths; `token` is a static ring token.
fn drop_selection_focus_locked(shared: &Arc<Mutex<Shared>>, token: &'static str) {
    let mut guard = lock_shared(shared);
    if guard.selection_focus_pending.take().is_some() {
        guard.diagnostics.focus_dropped_full += 1;
        drop(guard);
        emit(shared, 0, 0, 0, "focus", token, 1);
    }
}

/// Bind the correlation-bound deferred selection focus for one replied
/// selection command (engine thread, at dispatch): replaces any older
/// pending intent (latest-wins supersession, counted), or drops the older
/// intent when this command retains no View target or carries no focus
/// outcome (a newer switch still supersedes). Binds nothing when the reply
/// publishes no live native binding for the retained target — the arrival
/// Space is unprovable, so no focus may ever release (refusal direction).
/// Bumps the supersession floor on every selection bind so older
/// completions can never reapply after this newer input.
fn bind_selection_focus(
    session: &mut WorldSession,
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReply,
    effects: &[OneShotEffect],
    command_seq: u64,
    observed_focus: Option<WindowId>,
) {
    let operation_id = command_operation_id(command_seq);
    {
        let mut guard = lock_shared(shared);
        guard.selection_focus_floor_seq = guard.selection_focus_floor_seq.max(command_seq);
    }
    let transfer = session
        .engine_mut()
        .retained_transfers()
        .into_iter()
        .find(|target| {
            target.operation_id == operation_id
                && matches!(target.kind, tiler_core::world_report::TransferKind::View)
        });
    let Some(transfer) = transfer else {
        drop_selection_focus_locked(shared, "superseded-no-transfer");
        return;
    };
    let Some(effect) = effects
        .iter()
        .find(|e| e.is_explicit_focus_for(command_seq))
    else {
        drop_selection_focus_locked(shared, "superseded-no-effect");
        return;
    };
    let (fpid, _fwid) = match parse_window_id(&effect.target.0) {
        Some(pair) => pair,
        None => {
            drop_selection_focus_locked(shared, "unproven-target");
            return;
        }
    };
    // Bind-time baselines, read fresh at dispatch (never guessed): the
    // owning app generation, the engine's worker-gated observed focus, and
    // the main-thread foreground. Any gap refuses the bind — without a
    // baseline the release could never prove non-intervention.
    let baselines = {
        let guard = lock_shared(shared);
        FocusBaselines {
            app_generation: guard.apps.get(&fpid).map(|record| record.generation),
            prior_focus: observed_focus.map(|window| window.0),
            prior_frontmost: guard
                .focus_evidence
                .as_ref()
                .and_then(|evidence| evidence.frontmost_pid),
        }
    };
    let native_binding = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == transfer.target_output.0 && row.id == transfer.target_workspace)
        .and_then(|row| row.native_binding.clone());
    let Some(deferred) = deferred_selection_focus_for(
        &operation_id,
        command_seq,
        effect,
        native_binding,
        session.focus_route(),
        baselines,
    ) else {
        drop_selection_focus_locked(shared, "unproven-baseline");
        return;
    };
    {
        let mut guard = lock_shared(shared);
        if guard
            .selection_focus_pending
            .as_ref()
            .is_some_and(|pending| pending.operation_id != operation_id)
        {
            guard.diagnostics.focus_dropped_full += 1;
            drop(guard);
            emit(shared, 0, 0, 0, "focus", "superseded-deferred", 1);
            let mut guard = lock_shared(shared);
            guard.selection_focus_pending = Some(deferred);
            drop(guard);
        } else {
            guard.selection_focus_pending = Some(deferred);
            drop(guard);
        }
        emit(shared, 0, 0, 0, "focus", "deferred-bound", 1);
    }
}

/// Outcome of one arrival-bound selection-focus release attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionFocusRelease {
    /// Focus enqueued to the outbox plus the owner slot.
    Released,
    /// Deferred intent dropped without dispatch (refusal, supersession,
    /// stale app, user focus move, or unproven arrival).
    Dropped,
    /// No deferred intent names this operation: nothing to do.
    NoPending,
}

/// Release one deferred selection focus after the native selection proved
/// arrival (main thread, on the matching select completion's verified
/// post): the post must name the bound native Space — an unreadable or
/// wrong-Space post drops the intent instead of dispatching early.
/// Freshness fences mirror the dispatch path: a superseded seq, a
/// dead/regenerated app, a user focus move to another app, or a full
/// outbox all drop counted instead of dispatching stale. At-most-once:
/// the slot is taken before any enqueue, so a repeat completion finds
/// nothing (no replay).
pub(crate) fn release_selection_focus_after_arrival(
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    completion_op: Option<&str>,
    post_sid: Option<u64>,
    frontmost_pid: Option<u32>,
) -> SelectionFocusRelease {
    let Some(op) = completion_op else {
        return SelectionFocusRelease::NoPending;
    };
    let deferred = {
        let mut guard = lock_shared(shared);
        match guard.selection_focus_pending.as_ref() {
            Some(pending) if pending.operation_id == op => guard
                .selection_focus_pending
                .take()
                .expect("matched pending"),
            _ => return SelectionFocusRelease::NoPending,
        }
    };
    let drop_stale = |token: &'static str| {
        lock_shared(shared).diagnostics.stale_dropped += 1;
        emit(shared, 0, 0, 0, "focus", token, 1);
        SelectionFocusRelease::Dropped
    };
    if deferred.command_seq < lock_shared(shared).selection_focus_floor_seq {
        return drop_stale("superseded-release");
    }
    if post_sid != Some(deferred.expected_sid) {
        return drop_stale("unproven-arrival");
    }
    let Some((fpid, fwid)) = parse_window_id(&deferred.window.0) else {
        return drop_stale("unproven-target");
    };
    let live = {
        let guard = lock_shared(shared);
        guard.apps.get(&fpid).map(|record| {
            (
                record.generation,
                record.terminated,
                Arc::clone(&record.focus),
            )
        })
    };
    let Some((app_generation, terminated, slot)) = live else {
        return drop_stale("stale-app");
    };
    if terminated || Some(app_generation) != deferred.app_generation {
        return drop_stale("stale-app");
    }
    // Foreground stability since bind: the release-time foreground must
    // equal the bind-time foreground. A move in between proves user input
    // after the select and drops the intent — whether the user left the
    // target app or arrived from elsewhere. An unchanged foreground
    // proceeds even when it names another app (hotkey select from
    // elsewhere is the ordinary restore path, not an intervention).
    // Unknown on either side fails closed: without a stability baseline
    // the release could never prove non-intervention. Same-app window
    // moves under a stable foreground are invisible here; the owner worker
    // re-checks them against the bind-time prior before dispatching.
    match (deferred.prior_frontmost, frontmost_pid) {
        (Some(before), Some(now)) if before == now => {}
        (Some(_), Some(_)) => return drop_stale("user-focus-moved"),
        _ => return drop_stale("unproven-foreground"),
    }
    let (capacity, native_mode, revision) = {
        let guard = lock_shared(shared);
        (
            guard.config.focus_outbox_depth,
            matches!(guard.config.topology_mode, TopologyMode::Native),
            guard.topology_revision,
        )
    };
    let request = FocusRequest {
        pid: fpid,
        generation: app_generation,
        request_id: deferred.command_seq,
        wid: fwid,
        window: deferred.window.0.clone(),
        dispatched: true,
        verified: false,
        topology_revision: revision,
    };
    {
        let mut queue = lock_focus(focus_queue);
        if queue.len() >= capacity {
            drop(queue);
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, 0, 0, 0, "focus", "dropped-full", 1);
            return SelectionFocusRelease::Dropped;
        }
        queue.push(request);
    }
    {
        let mut guard = lock_shared(shared);
        guard.diagnostics.focus_dispatched += 1;
    }
    emit(shared, 0, 0, 0, "focus", "selection-released", 1);
    // Owner actuation path, native mode only (mirrors the dispatch path:
    // the synthetic skeleton has no per-app focus dispatch).
    if native_mode {
        let cmd = FocusCommand {
            pid: fpid,
            generation: app_generation,
            request_id: deferred.command_seq,
            wid: fwid,
            window: deferred.window.0,
            revision,
            route: deferred.route,
            // The bind-time observed focus rides along so the owner worker
            // can refuse a same-app user focus move the main thread cannot
            // see (frontmost is app-level).
            expect_prior: deferred.prior_focus,
        };
        if slot.set_latest(cmd).is_some() {
            lock_shared(shared).diagnostics.focus_dispatched += 1;
        } else {
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, 0, 0, 0, "focus", "dropped-full", 1);
        }
    }
    SelectionFocusRelease::Released
}

/// Drop any deferred selection focus naming one operation (refused select
/// completion): arrival never proved, so the intent must never fire.
/// Returns true when an intent was dropped.
pub(crate) fn drop_selection_focus_for_operation(
    shared: &Arc<Mutex<Shared>>,
    operation_id: Option<&str>,
) -> bool {
    let Some(op) = operation_id else {
        return false;
    };
    let mut guard = lock_shared(shared);
    if guard
        .selection_focus_pending
        .as_ref()
        .is_some_and(|pending| pending.operation_id == op)
    {
        guard.selection_focus_pending = None;
        guard.diagnostics.stale_dropped += 1;
        drop(guard);
        emit(shared, 0, 0, 0, "focus", "cancelled-with-select", 1);
        return true;
    }
    false
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
    reply: &WorldReply,
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
    // no focus even when the reply carries admission intents. Newer core
    // selection effects (remembered-leaf focus on actual view changes)
    // never dispatch here either: they bind a correlation-bound deferred
    // (see `bind_selection_focus`) released only after the native
    // selection proves arrival.
    let explicit_focus_seq = match (ticket_command, ticket_command_seq) {
        (Some(WorldCommand::Focus { .. }), Some(seq))
        | (Some(WorldCommand::Operation(CoreCommand::Focus { .. })), Some(seq)) => Some(seq),
        _ => None,
    };
    if let Some(seq) = explicit_focus_seq {
        // Newer user focus intent supersedes any deferred selection
        // focus: never reapply a stale selection focus after it.
        drop_selection_focus_locked(shared, "explicit-focus-superseded");
        let mut guard = lock_shared(shared);
        guard.selection_focus_floor_seq = guard.selection_focus_floor_seq.max(seq);
    } else if let Some(seq) = selection_ticket_seq(ticket_command, ticket_command_seq) {
        bind_selection_focus(
            session,
            shared,
            reply,
            effects,
            seq,
            retained.observed_focus.clone(),
        );
    }
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
                        // Explicit focus dispatches legacy-unchanged: no
                        // prior check (the user intent IS the focus move).
                        expect_prior: None,
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
/// the main-thread drain with its own fresh fences. A pending claim that
/// carries an operation identity but no plan (stored from a
/// sequencer-stale refusal, so no stale binding was bound) rebinds its
/// plan here from the currently applied reply — never from a previous
/// one — while the lane is idle. A claim already taken for drain
/// (`select_busy`) is never touched.
fn settle_select_transfers(
    session: &mut WorldSession,
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
        return;
    }
    // Rebind a plan-less claim (stale-refusal store) from this applied
    // reply: resolve the retained View transfer by operation identity,
    // then the live native binding (or explicit creation desire) plus the
    // observation-time baseline from the current native context. Never
    // invents a binding; missing targets leave the claim unbound so the
    // drain refuses without actuation.
    let needs_plan = lock_shared(shared)
        .select_pending
        .as_ref()
        .is_some_and(|claim| {
            claim.operation_id.as_deref() == Some(operation_id.as_str())
                && claim.select_target_output.is_none()
        });
    if !needs_plan {
        return;
    }
    let busy = lock_shared(shared).select_busy;
    if busy {
        return;
    }
    let transfer = session
        .engine_mut()
        .retained_transfers()
        .into_iter()
        .find(|target| {
            target.operation_id == operation_id
                && matches!(target.kind, tiler_core::world_report::TransferKind::View)
        });
    let Some(transfer) = transfer else {
        return;
    };
    let target_output = transfer.target_output.0.clone();
    let target_workspace = transfer.target_workspace.clone();
    let native_binding = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == target_output && row.id == target_workspace)
        .and_then(|row| row.native_binding.clone());
    let creation = reply.native_creations.iter().find(|desire| {
        desire.target_output.0 == target_output && desire.logical_workspace == target_workspace
    });
    let baseline_sid = lock_shared(shared).native_context.clone().and_then(|ctx| {
        ctx.domains
            .iter()
            .find(|domain| domain.output.0 == target_output && domain.current)
            .map(|domain| domain.space)
    });
    let mut guard = lock_shared(shared);
    let Some(claim) = guard.select_pending.as_mut() else {
        return;
    };
    if claim.operation_id.as_deref() != Some(operation_id.as_str())
        || claim.select_target_output.is_some()
    {
        return;
    }
    claim.select_target_output = Some(target_output.clone());
    claim.select_target_workspace = Some(target_workspace.clone());
    claim.select_native_binding = native_binding;
    if let Some(desire) = creation {
        claim.select_creation_operation = Some(desire.operation_id.clone());
        claim.select_creation_logical = Some(desire.logical_workspace.clone());
        claim.select_creation_output = Some(desire.target_output.0.clone());
    }
    claim.select_baseline_sid = baseline_sid;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::OutputId;

    fn output() -> OutputId {
        OutputId("macos-display-1".to_owned())
    }

    fn empty_reply() -> WorldReply {
        WorldReply {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            request_id: "req-0".to_owned(),
            based_on_observation_seq: 0,
            state_revision: 0,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: tiler_core::world_presentation::WorldPresentation::default(),
        }
    }

    #[test]
    fn v2_command_shapes_carry_no_native_workspace() {
        use tiler_core::directional::WindowId;
        use tiler_core::world_report::{SendSource, WorldCommand};
        // Numbered selects are positional over the adopted ring with no
        // adapter output (core resolves cursor/focus); trailing resolves
        // core-side to reuse-or-create. No arm pins a native workspace id
        // into the command.
        assert!(matches!(
            crate::world_command_map::select_command(
                &crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 3 },
                None,
            ),
            Some(WorldCommand::SelectNumbered {
                output: None,
                index: 3
            })
        ));
        assert!(matches!(
            crate::world_command_map::select_command(
                &crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 0 },
                None,
            ),
            Some(WorldCommand::SelectTrailing { output: None })
        ));
        // Numbered sends ride the explicit admission-resolved source with a
        // positional index; the pinned native target stays drain-side for
        // revalidation only.
        match crate::world_command_map::send_command(
            &crate::runtime_types::InputSemanticAction::WorkspaceSend {
                index: 2,
                follow: true,
            },
            "4242_7",
            output(),
        ) {
            Some(WorldCommand::SendNumbered {
                window,
                source: SendSource::Explicit(_),
                index: 2,
                follow: true,
            }) => assert_eq!(window, WindowId("4242_7".to_owned())),
            other => panic!("send-2 must map to positional SendNumbered, got {other:?}"),
        }
        // The empty reply carries no transfers: no pending claim settles.
        assert!(empty_reply().transfers.is_empty());
    }

    // -- Arrival-bound deferred selection focus ----------------------------

    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    fn focus_shared() -> Arc<Mutex<Shared>> {
        let config = crate::runtime_types::RuntimeConfig {
            topology_mode: crate::runtime_types::TopologyMode::Native,
            ..Default::default()
        };
        Arc::new(Mutex::new(Shared {
            config,
            handles_live: 0,
            generation_counter: 0,
            log_counter: 0,
            start: Instant::now(),
            apps: BTreeMap::new(),
            clock: Arc::new(crate::world_sequencer::ObservationClock::new()),
            midflight_hook: Mutex::new(None),
            post_accept_hook: Mutex::new(None),
            topology: None,
            display_inputs: None,
            select_wake: None,
            topology_revision: 9,
            native_context: None,
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: None,
            select_busy: false,
            selection_focus_pending: None,
            selection_focus_floor_seq: 0,
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
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            last_presentation: tiler_core::world_presentation::WorldPresentation::default(),
            last_pager_bindings: Vec::new(),
            presentation_style: crate::active_border::PresentationStylePx::default(),
        }))
    }

    fn live_focus_app(shared: &Arc<Mutex<Shared>>, pid: u32, generation: u64) -> Arc<FocusSlot> {
        let slot = Arc::new(FocusSlot::new());
        lock_shared(shared).apps.insert(
            pid,
            crate::runtime_shared::AppRecord {
                generation,
                terminated: false,
                stop: Arc::new(AtomicBool::new(false)),
                frames: Arc::new(FrameSlot::new()),
                focus: Arc::clone(&slot),
                ready: Arc::new(Mutex::new(Some(Ok::<(), String>(())))),
                worker: None,
                worker_name: "test".to_owned(),
            },
        );
        slot
    }

    fn focus_session() -> WorldSession {
        WorldSession::new(
            tiler_core::size_hints::FixedSizePredicate::BothAxes,
            true,
            crate::focus_route::FocusRoute::Slps,
            Arc::new(crate::world_sequencer::ObservationClock::new()),
        )
    }

    fn focus_effect(seq: u64) -> OneShotEffect {
        use tiler_core::directional::WindowId;
        OneShotEffect::explicit_focus(seq, WindowId("4242_7".to_owned()), seq + 100)
    }

    fn pending_op3() -> DeferredSelectionFocus {
        use tiler_core::directional::WindowId;
        DeferredSelectionFocus {
            operation_id: "op-3".to_owned(),
            command_seq: 3,
            window: WindowId("4242_7".to_owned()),
            route: crate::focus_route::FocusRoute::Slps,
            expected_sid: 3,
            app_generation: Some(1),
            prior_focus: Some("4242_7".to_owned()),
            prior_frontmost: Some(4242),
        }
    }

    #[test]
    fn selection_tickets_defer_while_other_tickets_bind_nothing() {
        use tiler_core::directional::OutputId;
        use tiler_core::world_report::WorldCommand;
        let out = Some(OutputId("macos-display-1".to_owned()));
        let tickets = [
            WorldCommand::SelectWorkspace {
                output: out.clone().expect("output"),
                workspace: "ws-a".to_owned(),
            },
            WorldCommand::SwitchView {
                output: out.clone().expect("output"),
                workspace: "ws-a".to_owned(),
            },
            WorldCommand::SelectNumbered {
                output: out.clone(),
                index: 2,
            },
            WorldCommand::SelectRelative {
                output: out.clone(),
                delta: 1,
            },
            WorldCommand::SelectPrevious {
                output: out.clone(),
            },
            WorldCommand::SelectTrailing { output: out },
        ];
        for ticket in &tickets {
            assert_eq!(selection_ticket_seq(Some(ticket), Some(9)), Some(9));
        }
        // Explicit focus, missing seq, missing ticket, and unrelated
        // commands bind nothing here.
        assert_eq!(
            selection_ticket_seq(
                Some(&WorldCommand::Focus {
                    window: tiler_core::directional::WindowId("4242_7".to_owned()),
                    direction: "left".to_owned(),
                }),
                Some(9),
            ),
            None,
        );
        assert_eq!(selection_ticket_seq(Some(&tickets[0]), None), None);
        assert_eq!(selection_ticket_seq(None, Some(9)), None);
    }

    #[test]
    fn deferred_resolver_requires_correlated_effect_and_live_binding() {
        use tiler_core::directional::WindowId;
        let target = WindowId("4242_7".to_owned());
        let route = crate::focus_route::FocusRoute::Slps;
        let baselines = || FocusBaselines {
            app_generation: Some(1),
            prior_focus: Some("4242_7".to_owned()),
            prior_frontmost: Some(4242),
        };
        let effect = OneShotEffect::explicit_focus(4, target.clone(), 104);
        let resolved = deferred_selection_focus_for(
            "op-4",
            4,
            &effect,
            Some("space-3".to_owned()),
            route,
            baselines(),
        )
        .expect("correlated effect with a live binding resolves");
        assert_eq!(resolved.operation_id, "op-4");
        assert_eq!(resolved.command_seq, 4);
        assert_eq!(resolved.window, target);
        assert_eq!(resolved.expected_sid, 3);
        assert_eq!(resolved.app_generation, Some(1));
        assert_eq!(resolved.prior_focus.as_deref(), Some("4242_7"));
        assert_eq!(resolved.prior_frontmost, Some(4242));
        // Uncorrelated effect id never resolves.
        let other = OneShotEffect::explicit_focus(5, target.clone(), 105);
        assert!(
            deferred_selection_focus_for(
                "op-4",
                4,
                &other,
                Some("space-3".to_owned()),
                route,
                baselines(),
            )
            .is_none(),
            "fx-5 is not the outcome of command 4"
        );
        // Missing or unparsable bindings never resolve (no guessed Space).
        assert!(
            deferred_selection_focus_for("op-4", 4, &effect, None, route, baselines(),).is_none()
        );
        assert!(
            deferred_selection_focus_for(
                "op-4",
                4,
                &effect,
                Some("desktop-3".to_owned()),
                route,
                baselines(),
            )
            .is_none()
        );
        assert!(
            deferred_selection_focus_for(
                "op-4",
                4,
                &effect,
                Some("space-0".to_owned()),
                route,
                baselines(),
            )
            .is_none(),
            "zero SID never resolves"
        );
        // Missing baselines never resolve: without an observed-focus or
        // foreground baseline, non-intervention is unprovable.
        assert!(
            deferred_selection_focus_for(
                "op-4",
                4,
                &effect,
                Some("space-3".to_owned()),
                route,
                FocusBaselines {
                    prior_focus: None,
                    ..baselines()
                },
            )
            .is_none()
        );
        assert!(
            deferred_selection_focus_for(
                "op-4",
                4,
                &effect,
                Some("space-3".to_owned()),
                route,
                FocusBaselines {
                    prior_frontmost: None,
                    ..baselines()
                },
            )
            .is_none()
        );
    }

    #[test]
    fn selection_dispatch_without_retained_target_binds_nothing() {
        // Refusal direction through the real dispatch path: a selection
        // reply with no retained View transfer (fresh engine, nothing
        // retained) binds no deferred focus and queues nothing — even
        // when the reply carries a focus effect.
        use tiler_core::world_report::WorldCommand;
        let mut session = focus_session();
        let mut retained = crate::runtime_core::RetainedCore::default();
        let shared = focus_shared();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = empty_reply();
        dispatch_focus_effects(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            8,
            &reply,
            &[focus_effect(5)],
            Some(&WorldCommand::SelectNumbered {
                output: None,
                index: 2,
            }),
            Some(5),
            0,
            0,
            0,
        );
        let guard = lock_shared(&shared);
        assert!(guard.selection_focus_pending.is_none());
        assert_eq!(guard.selection_focus_floor_seq, 5);
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
        assert_eq!(guard.diagnostics.focus_dropped_full, 0);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn newer_switch_supersedes_pending_selection_focus() {
        // Supersession through the real dispatch path: a newer selection
        // command (seq 5) with no retained target drops the older pending
        // intent (op-3) counted — it can never fire after this input.
        use tiler_core::world_report::WorldCommand;
        let mut session = focus_session();
        let mut retained = crate::runtime_core::RetainedCore::default();
        let shared = focus_shared();
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = empty_reply();
        dispatch_focus_effects(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            8,
            &reply,
            &[],
            Some(&WorldCommand::SelectNumbered {
                output: None,
                index: 2,
            }),
            Some(5),
            0,
            0,
            0,
        );
        let guard = lock_shared(&shared);
        assert!(guard.selection_focus_pending.is_none());
        assert_eq!(guard.selection_focus_floor_seq, 5);
        assert_eq!(guard.diagnostics.focus_dropped_full, 1);
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
    }

    #[test]
    fn explicit_focus_supersedes_and_dispatches_immediately() {
        // Explicit focus still dispatches at once (unchanged path) and
        // supersedes any deferred selection intent plus the floor.
        use tiler_core::directional::WindowId;
        use tiler_core::world_report::WorldCommand;
        let mut session = focus_session();
        let mut retained = crate::runtime_core::RetainedCore::default();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, 4242, 1);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = empty_reply();
        dispatch_focus_effects(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            8,
            &reply,
            &[focus_effect(7)],
            Some(&WorldCommand::Focus {
                window: WindowId("4242_7".to_owned()),
                direction: "left".to_owned(),
            }),
            Some(7),
            4242,
            1,
            7,
        );
        let guard = lock_shared(&shared);
        assert!(guard.selection_focus_pending.is_none());
        assert_eq!(guard.selection_focus_floor_seq, 7);
        // Immediate dispatch is unchanged: queue plus owner slot.
        assert_eq!(guard.diagnostics.focus_dispatched, 2);
        drop(guard);
        let queued = lock_focus(&focus_queue);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].pid, 4242);
        assert_eq!(queued[0].wid, 7);
        drop(queued);
        assert!(slot.take().is_some(), "owner slot dispatches at once");
    }

    #[test]
    fn release_fires_only_on_verified_arrival_exactly_once() {
        // Async arrival: nothing fires before the verified post; the first
        // matching completion releases to the queue plus the owner slot,
        // and a repeat completion finds nothing (no replay).
        let shared = focus_shared();
        let slot = live_focus_app(&shared, 4242, 1);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-9"),
                Some(3),
                None
            ),
            SelectionFocusRelease::NoPending,
            "unrelated operations never touch the intent"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "unrelated release keeps the intent"
        );
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                Some(3),
                Some(4242)
            ),
            SelectionFocusRelease::Released,
        );
        {
            let queued = lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1);
            assert_eq!((queued[0].pid, queued[0].wid), (4242, 7));
            assert_eq!(queued[0].generation, 1);
            assert_eq!(queued[0].topology_revision, 9);
        }
        let cmd = slot.take().expect("owner slot carries the release");
        assert_eq!(cmd.window, "4242_7");
        assert_eq!(cmd.revision, 9);
        assert_eq!(
            cmd.expect_prior.as_deref(),
            Some("4242_7"),
            "bind-time observed focus rides for the worker check"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dispatched,
            2,
            "queue plus slot count"
        );
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                Some(3),
                None
            ),
            SelectionFocusRelease::NoPending,
            "at-most-once: no replay"
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn release_drops_unproven_wrong_space_and_stale_states() {
        // No focus before verified arrival: an unreadable post drops. A
        // wrong-Space post drops. Stale apps, regenerated lifetimes, a user
        // foreground move, unknown foreground on either side, a superseding
        // floor, and a full outbox all drop counted — never dispatch stale.
        // An unchanged foreground releases even when it names another app.
        let release_with = |pending: DeferredSelectionFocus,
                            post: Option<u64>,
                            frontmost: Option<u32>,
                            floor: u64,
                            terminate: bool,
                            configure: &dyn Fn(&Arc<Mutex<Shared>>)|
         -> (
            SelectionFocusRelease,
            Arc<Mutex<Shared>>,
            Arc<Mutex<Vec<FocusRequest>>>,
            Arc<FocusSlot>,
        ) {
            let shared = focus_shared();
            let slot = live_focus_app(&shared, 4242, 1);
            if terminate {
                lock_shared(&shared)
                    .apps
                    .get_mut(&4242)
                    .expect("app")
                    .terminated = true;
            }
            lock_shared(&shared).selection_focus_pending = Some(pending);
            lock_shared(&shared).selection_focus_floor_seq = floor;
            configure(&shared);
            let focus_queue = Arc::new(Mutex::new(Vec::new()));
            let outcome = release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                post,
                frontmost,
            );
            (outcome, shared, focus_queue, slot)
        };
        let no_configure = |_: &Arc<Mutex<Shared>>| {};
        // Unreadable post: arrival unproven.
        let (outcome, shared, queue, slot) =
            release_with(pending_op3(), None, None, 0, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        assert!(slot.take().is_none());
        assert_eq!(lock_shared(&shared).diagnostics.stale_dropped, 1);
        // Wrong Space post.
        let (outcome, _, queue, _) =
            release_with(pending_op3(), Some(9), None, 0, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        // Terminated app.
        let (outcome, _, queue, _) =
            release_with(pending_op3(), Some(3), None, 0, true, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        // Regenerated lifetime (bound gen 1, live gen 2).
        let (outcome, _, queue, _) = release_with(
            DeferredSelectionFocus {
                app_generation: Some(1),
                ..pending_op3()
            },
            Some(3),
            None,
            0,
            false,
            &|shared: &Arc<Mutex<Shared>>| {
                lock_shared(shared)
                    .apps
                    .get_mut(&4242)
                    .expect("app")
                    .generation = 2;
            },
        );
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        // User moved foreground to another app after the select.
        let (outcome, _, queue, _) =
            release_with(pending_op3(), Some(3), Some(9999), 0, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        // Unknown foreground fails closed: without fresh evidence the
        // target still owns the foreground, nothing dispatches.
        let (outcome, shared, queue, slot) =
            release_with(pending_op3(), Some(3), None, 0, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        assert!(slot.take().is_none());
        assert_eq!(lock_shared(&shared).diagnostics.stale_dropped, 1);
        // Superseded by newer input (floor past the bound seq).
        let (outcome, _, queue, _) =
            release_with(pending_op3(), Some(3), None, 4, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        // Full outbox.
        let (outcome, shared, queue, slot) = release_with(
            pending_op3(),
            Some(3),
            Some(4242),
            0,
            false,
            &|shared: &Arc<Mutex<Shared>>| {
                lock_shared(shared).config.focus_outbox_depth = 0;
            },
        );
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        assert!(slot.take().is_none());
        assert_eq!(lock_shared(&shared).diagnostics.focus_dropped_full, 1);
        // Foreground already on the target app still releases.
        let (outcome, _, queue, slot) =
            release_with(pending_op3(), Some(3), Some(4242), 0, false, &no_configure);
        assert_eq!(outcome, SelectionFocusRelease::Released);
        assert_eq!(lock_focus(&queue).len(), 1);
        assert!(slot.take().is_some());
        // Unchanged foreground elsewhere releases too: the user sat in
        // another app at bind and never moved — hotkey select from
        // elsewhere is the ordinary restore path, not an intervention.
        let (outcome, _, queue, slot) = release_with(
            DeferredSelectionFocus {
                prior_frontmost: Some(9999),
                ..pending_op3()
            },
            Some(3),
            Some(9999),
            0,
            false,
            &no_configure,
        );
        assert_eq!(outcome, SelectionFocusRelease::Released);
        assert_eq!(lock_focus(&queue).len(), 1);
        assert!(slot.take().is_some());
        // Unknown bind-time foreground drops even with a fresh release
        // read: stability needs both baselines.
        let (outcome, shared, queue, slot) = release_with(
            DeferredSelectionFocus {
                prior_frontmost: None,
                ..pending_op3()
            },
            Some(3),
            Some(4242),
            0,
            false,
            &no_configure,
        );
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        assert!(slot.take().is_none());
        assert_eq!(lock_shared(&shared).diagnostics.stale_dropped, 1);
    }

    #[test]
    fn refused_completion_drops_the_matching_intent_only() {
        let shared = focus_shared();
        live_focus_app(&shared, 4242, 1);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        assert!(!drop_selection_focus_for_operation(&shared, Some("op-9")));
        assert!(lock_shared(&shared).selection_focus_pending.is_some());
        assert!(!drop_selection_focus_for_operation(&shared, None));
        assert!(drop_selection_focus_for_operation(&shared, Some("op-3")));
        assert!(lock_shared(&shared).selection_focus_pending.is_none());
        assert_eq!(lock_shared(&shared).diagnostics.stale_dropped, 1);
    }
}
