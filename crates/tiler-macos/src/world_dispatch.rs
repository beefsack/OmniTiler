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
use tiler_core::world_report::{
    OneShotEffect, ScopeAuthority, WorldCommand, WorldReplyV4, WorldReportV4,
};
use tiler_protocol::world_protocol::{
    CreationReceiptWire, CursorHitWire, DecodedReplyV4, DecodedReportV4, GestureIngressWire,
    ReplyWireOptionals, ReportWireOptionals, decode_reply_v4_for_host, decode_report_v4,
    encode_reply_v4, encode_report_v4,
};

use crate::focus_route::FocusRoute;
use crate::runtime::{
    AppliedGeometryKey, DeferredSelectionFocus, DesiredFrame, FocusBaselines, FocusCommand,
    FocusRequest, FrameCommand, Shared, emit, lock_focus, lock_shared, parse_window_id,
};
use crate::runtime_core::RetainedCore;
use crate::runtime_thread::{CoherencePending, FocusSlot, FrameSlot, Inbox};
use crate::runtime_types::{CloseFocusBinding, TopologyMode};
use crate::world_runtime::{
    DispatchContext, WorldSession, build_dispatch_report, map_gesture_preview, track_write,
};
use crate::world_sequencer::{CloseEnvelope, ReplyVerdict, RequestTicket, command_operation_id};

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

/// Production byte-codec transact: the typed report encodes through the
/// genuine v4 codec ([`encode_report_v4`]), decodes back to the wire-typed
/// report ([`decode_report_v4`]), runs the real engine, then the typed reply
/// encodes ([`encode_reply_v4`]) and decodes ([`decode_reply_v4_for_host`]
/// against the production [`macos_capabilities`](crate::world_connection::macos_capabilities))
/// before the sequencer ever sees it. Any codec leg failure returns its
/// static kind; callers refuse explicitly and never fall back to a direct
/// typed call. A wrong-capability row (above/warp/sticky, all false on
/// macOS) refuses here as `snapshot-invalid` with its fixed rule token,
/// before sequencer acceptance or any actuation. Wire metadata is inferred canonically: absent cursor-hit/gesture
/// stay absent, `monotonic_ms` rides iff nonzero (attempt stamps preserved),
/// attempt lanes ride iff non-empty, and creation receipts keep
/// `created` null unless the row carries causal proof (`Some`).
fn codec_transact(
    session: &mut WorldSession,
    report: &WorldReportV4,
) -> Result<WorldOutcome, &'static str> {
    let decoded_report = DecodedReportV4 {
        report: report.clone(),
        wire: ReportWireOptionals {
            cursor_hit: match &report.cursor_hit {
                None => CursorHitWire::Absent,
                Some(_) => CursorHitWire::Id,
            },
            monotonic_ms_present: report.monotonic_ms != 0,
            frame_attempts_present: !report.frames.is_empty(),
            member_attempts_present: !report.member_attempts.is_empty(),
            view_attempts_present: !report.view_attempts.is_empty(),
            clear_attempts_present: !report.clear_attempts.is_empty(),
            creation_attempts_present: !report.creation_attempts.is_empty(),
            gesture_ingress: match &report.gesture {
                None => GestureIngressWire::Absent,
                Some(_) => GestureIngressWire::Present,
            },
        },
        creation_receipts: report
            .creation_attempts
            .iter()
            .map(|row| match &row.created_native {
                // No causal proof: `created` stays explicit null (never
                // absent, which would claim no creation was attempted).
                None => CreationReceiptWire::Null,
                Some(_) => CreationReceiptWire::Id,
            })
            .collect(),
    };
    let report_wire = encode_report_v4(&decoded_report).map_err(|_| "codec-report-encode")?;
    let admitted = decode_report_v4(&report_wire).map_err(|_| "codec-report-decode")?;
    let outcome = session.engine_mut().handle_report(&admitted.report);
    // Capability truth at reply decode: the production macOS host declares
    // above/warp/native-sticky false, and the engine gates all three rows
    // on those caps — so an honest reply never carries them and this gate
    // only fires on a dishonest/corrupt row, before sequencer/apply.
    let host = crate::world_connection::macos_capabilities();
    match outcome {
        WorldOutcome::Accepted(reply) => {
            let suspended = reply.presentation.suspended_output.is_some();
            let decoded = DecodedReplyV4 {
                reply,
                wire: ReplyWireOptionals {
                    suspended_output_present: suspended,
                },
            };
            let reply_wire = encode_reply_v4(&decoded).map_err(|_| "codec-reply-encode")?;
            let back =
                decode_reply_v4_for_host(&reply_wire, &host).map_err(|_| "codec-reply-decode")?;
            Ok(WorldOutcome::Accepted(back.reply))
        }
        WorldOutcome::CommandConsumed(reply) => {
            let suspended = reply.presentation.suspended_output.is_some();
            let decoded = DecodedReplyV4 {
                reply,
                wire: ReplyWireOptionals {
                    suspended_output_present: suspended,
                },
            };
            let reply_wire = encode_reply_v4(&decoded).map_err(|_| "codec-reply-encode")?;
            let back =
                decode_reply_v4_for_host(&reply_wire, &host).map_err(|_| "codec-reply-decode")?;
            Ok(WorldOutcome::CommandConsumed(back.reply))
        }
        WorldOutcome::Duplicate(reply) => {
            let suspended = reply.presentation.suspended_output.is_some();
            let decoded = DecodedReplyV4 {
                reply,
                wire: ReplyWireOptionals {
                    suspended_output_present: suspended,
                },
            };
            let reply_wire = encode_reply_v4(&decoded).map_err(|_| "codec-reply-encode")?;
            let back =
                decode_reply_v4_for_host(&reply_wire, &host).map_err(|_| "codec-reply-decode")?;
            Ok(WorldOutcome::Duplicate(back.reply))
        }
        WorldOutcome::Rejected { kind, message } => Ok(WorldOutcome::Rejected { kind, message }),
    }
}

/// Adapter-side fail-closed gate for unpersisted intent writes: the core has
/// no durable backend yet, so any nonempty `intent_writes` refuses before
/// any dependent native apply (no effect spend, no slot/outbox write).
/// Ordinary admission/reflow emits none, so this never fires honestly.
fn intent_writes_hold(reply: &WorldReplyV4) -> bool {
    !reply.intent_writes.is_empty()
}

/// Session-lifetime fail-closed latch over [`intent_writes_hold`]: the
/// first applied reply carrying nonempty `intent_writes` latches
/// [`WorldSession`] for its whole lifetime, so a later reply without
/// writes stays held (a bare per-reply check would wrongly release it).
/// Never clears: no filesystem storage/backend or recovery exists, so no
/// later empty reply can prove persistence, and nothing here claims
/// durability. Call at every real apply route after acceptance and before
/// any effect spend, slot/outbox write, or presentation publish.
fn unpersisted_hold(session: &mut WorldSession, reply: &WorldReplyV4) -> bool {
    if session.unpersisted_intent_hold() {
        return true;
    }
    if intent_writes_hold(reply) {
        session.mark_unpersisted_intent_writes();
        return true;
    }
    false
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
    // Genuine-connection gate: reports never run before the one-time
    // handshake minted a generation and bound the sequencer to it. No
    // flight opens, no engine call, no writes.
    if !session.connection_bound() {
        count_token(shared, &format!("{op}-refused-no-connection"));
        emit(
            shared,
            pid,
            generation,
            seq,
            "core-reply",
            "refused-no-connection",
            0,
        );
        return DispatchVerdict::Refused {
            token: "refused-no-connection".to_owned(),
            select_operation: None,
        };
    }
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
    if !session.connection_bound() {
        count_token(shared, &format!("{op}-refused-no-connection"));
        emit(
            shared,
            pid,
            generation,
            seq,
            "core-reply",
            "refused-no-connection",
            0,
        );
        return (
            DispatchVerdict::Refused {
                token: "refused-no-connection".to_owned(),
                select_operation: None,
            },
            PreviewOutcome::CancelDead {
                reason: "no-connection",
            },
            None,
        );
    }
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
    let outcome = match codec_transact(session, &report) {
        Ok(outcome) => outcome,
        Err(kind) => {
            session.sequencer().clear_flight();
            count_token(shared, &format!("{op}-rejected-{kind}"));
            emit(shared, pid, generation, seq, "core-reply", kind, 0);
            return (
                DispatchVerdict::Refused {
                    token: format!("rejected-{kind}"),
                    select_operation: None,
                },
                PreviewOutcome::HideTransient { reason: "held" },
                None,
            );
        }
    };
    match outcome {
        WorldOutcome::Accepted(reply) => match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => {
                if unpersisted_hold(session, &applied) {
                    count_token(shared, &format!("{op}-refused-intent-write-unpersisted"));
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "core-reply",
                        "intent-write-unpersisted",
                        0,
                    );
                    return (
                        DispatchVerdict::Refused {
                            token: "intent-write-unpersisted".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                }
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
                if unpersisted_hold(session, &applied) {
                    count_token(shared, &format!("{op}-refused-intent-write-unpersisted"));
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "core-reply",
                        "intent-write-unpersisted",
                        0,
                    );
                    return (
                        DispatchVerdict::Refused {
                            token: "intent-write-unpersisted".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                }
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
    // One core transaction per dispatch through the genuine v4 byte codec
    // (bounded 4 MiB request AND reply inside the codec; staged overflow
    // rejects without mutation; codec failure refuses explicitly, never a
    // direct typed fallback).
    let outcome = match codec_transact(session, &report) {
        Ok(outcome) => outcome,
        Err(kind) => {
            session.sequencer().clear_flight();
            count_token(shared, &format!("{op}-rejected-{kind}"));
            emit(shared, pid, generation, seq, "core-reply", kind, 0);
            return DispatchVerdict::Refused {
                token: format!("rejected-{kind}"),
                select_operation: None,
            };
        }
    };
    match outcome {
        WorldOutcome::Accepted(reply) => {
            match session.sequencer().accept_reply(&reply) {
                ReplyVerdict::Applied(applied) => {
                    if unpersisted_hold(session, &applied) {
                        count_token(shared, &format!("{op}-refused-intent-write-unpersisted"));
                        emit(
                            shared,
                            pid,
                            generation,
                            seq,
                            "core-reply",
                            "intent-write-unpersisted",
                            0,
                        );
                        return DispatchVerdict::Refused {
                            token: "intent-write-unpersisted".to_owned(),
                            select_operation: None,
                        };
                    }
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
                    if unpersisted_hold(session, &applied) {
                        count_token(shared, &format!("{op}-refused-intent-write-unpersisted"));
                        emit(
                            shared,
                            pid,
                            generation,
                            seq,
                            "core-reply",
                            "intent-write-unpersisted",
                            0,
                        );
                        return DispatchVerdict::Refused {
                            token: "intent-write-unpersisted".to_owned(),
                            select_operation: None,
                        };
                    }
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
    reply: &WorldReplyV4,
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
    applied: &WorldReplyV4,
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

/// Held-reply stable-geometry retention (pure, testable): a same-state
/// held reply preserves the last applied authorized desired geometry
/// instead of publishing an empty plan truth, then refreshes once on
/// observed recovery. The state key (current scopes, workspace bindings,
/// window homing, current native workareas/bounds) grounds the retention:
/// any view, binding, lifetime, or workarea change moves the key and the
/// fresh reply publishes as-is (clear, never a stale overlay). Carried rows
/// are exactly the last applied rows for windows the fresh reply still
/// lists as held (same current scope, non-minimized — a minimized lifetime
/// has no managed geometry, so its absence stands); fresh rows always win,
/// and nothing is ever carried into a scope that is not current (no
/// off-current write). The presentation needs no cache: the engine computes
/// it read-only over retained state, so a same-view held reply carries
/// identical shapes (proven by
/// `held_same_view_reply_preserves_stable_presentation_and_desired`).
/// No second policy owner: only last applied authorized same-state
/// geometry is preserved, never fabricated. Renderers still revalidate
/// every shape against live native legs (foreground token, current-space
/// membership, fresh minimized/fullscreen/session vetoes) before painting.
fn retain_stable_geometry(
    prior_desired: &[DesiredFrame],
    prior_key: Option<&AppliedGeometryKey>,
    current: &BTreeSet<(String, String)>,
    workareas: &BTreeMap<(String, String), (Rect, Rect)>,
    reply: &WorldReplyV4,
    fresh_desired: Vec<DesiredFrame>,
) -> (Vec<DesiredFrame>, AppliedGeometryKey) {
    use tiler_core::world_report::HoldReason;
    let mut bindings: Vec<(String, String, Option<String>)> = reply
        .workspaces
        .iter()
        .map(|row| {
            (
                row.output.0.clone(),
                row.id.clone(),
                row.native_binding.clone(),
            )
        })
        .collect();
    bindings.sort();
    let fresh_key = AppliedGeometryKey {
        current: current.clone(),
        bindings,
        homing: reply
            .windows
            .iter()
            .map(|row| {
                (
                    row.lifetime.0.clone(),
                    (row.output.0.clone(), row.workspace.clone()),
                )
            })
            .collect(),
        workareas: workareas.clone(),
    };
    let same_state = prior_key.is_some_and(|key| *key == fresh_key);
    let held_current = reply.windows.iter().any(|row| {
        row.frame_authority.hold_reason().is_some()
            && current.contains(&(row.output.0.clone(), row.workspace.clone()))
    });
    if !same_state || !held_current {
        return (fresh_desired, fresh_key);
    }
    let fresh_windows: BTreeSet<String> =
        fresh_desired.iter().map(|row| row.window.clone()).collect();
    let mut merged = fresh_desired;
    for prior in prior_desired {
        if fresh_windows.contains(&prior.window) {
            continue;
        }
        let Some(row) = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == prior.window)
        else {
            continue;
        };
        let held = match row.frame_authority.hold_reason() {
            Some(HoldReason::Minimized) | None => false,
            Some(_) => true,
        };
        if !held {
            continue;
        }
        if !current.contains(&(row.output.0.clone(), row.workspace.clone())) {
            continue;
        }
        merged.push(prior.clone());
    }
    (merged, fresh_key)
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
    reply: &WorldReplyV4,
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
        if let Some(operation) = row.frame_operation.clone().as_deref() {
            if !session.spend_frame_op(&operation) {
                continue;
            }
            frame_ops.push((row.lifetime.0.clone(), operation.to_string()));
        }
        frames.push((row.lifetime.0.clone(), frame));
    }
    // Applied-reply presentation publish: exactly the applied reply's
    // shapes ride shared state for the tile main thread, which renders
    // these shapes with liveness plus fresh native safety vetoes before
    // painting — except a same-state hold keeps the last stable desired
    // geometry instead of publishing an empty plan truth (see
    // `retain_stable_geometry`; the presentation itself is stable across
    // holds, so it publishes fresh). Duplicates and refusals never reach
    // here (no clobber).
    // Retained current native geometry grounding the retention key
    // above: per-current-scope (visible work area, full bounds) from the
    // live native domains, translated to logical scopes through this
    // reply's native bindings (same live lookup as `current`). A
    // workarea-only topology move during a hold changes this map and
    // clears the retention; own-append growth leaves every current
    // domain's rects untouched, so retention (and the global revision,
    // deliberately excluded) never churns on it.
    let workareas: BTreeMap<(String, String), (Rect, Rect)> = {
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
                    ((d.output.0.clone(), logical), (d.visible, d.bounds))
                })
                .collect(),
            None => BTreeMap::new(),
        }
    };
    let (desired, geometry_key) = {
        let guard = lock_shared(shared);
        retain_stable_geometry(
            &guard.last_desired,
            guard.last_geometry_key.as_ref(),
            &current,
            &workareas,
            reply,
            desired,
        )
    };
    {
        let mut guard = lock_shared(shared);
        guard.last_presentation = reply.presentation.clone();
        guard.last_pager_bindings = pager_bindings;
        guard.last_workspace_desires = reply.workspaces.clone();
        guard.last_core_generation = reply.core_generation.clone();
        guard.last_geometry_key = Some(geometry_key);
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
    dispatch_close_focus(
        session,
        retained,
        shared,
        focus_queue,
        focus_capacity,
        reply,
        &current,
        ticket_command,
        pid,
        generation,
        seq,
    );
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
        // Wake test-only condition waits: the desired plan, emission
        // counts, and reply outcome all commit under this same lock.
        guard.notify_test_waiters();
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
    reply: &WorldReplyV4,
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
            close: None,
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
    reply: &WorldReplyV4,
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
        // Directional focus steps ride the execution-time implicit subject:
        // the core still mints the correlated explicit-command focus
        // effect, so it dispatches exactly like a by-id focus step.
        (Some(WorldCommand::Implicit(inner)), Some(seq))
            if matches!(**inner, WorldCommand::FocusDirectional { .. }) =>
        {
            Some(seq)
        }
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
                        close: None,
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

/// Close-focus dispatch on the coherent LATEST reply lane, independent of
/// the command ticket: observes the nullable close envelope, publishes the
/// taken-but-unstarted epoch, and either attempts once or spends into a
/// safety no-call. Never fabricates `Accepted` on queueing: the attempt
/// stages nothing; the owner result path stages the raw row. Null or a
/// user/explicit command ticket retires and publishes `None`, cancelling
/// old taken-unstarted commands. Stale replays and spent re-emissions keep
/// the current epoch (a spent stable reemit never cancels the in-flight
/// same id). Admission/hover and the explicit/deferred paths are untouched.
#[allow(clippy::too_many_arguments)]
fn dispatch_close_focus(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    reply: &WorldReplyV4,
    current: &BTreeSet<(String, String)>,
    ticket_command: Option<&WorldCommand>,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    use crate::runtime_shared::CloseFocusEpoch;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::session::DomainKey;
    use tiler_core::world_report::{FocusFeedback, VisibilityReason, VisibilitySetter};

    let incarnation = reply.adapter_incarnation.clone();
    let core_gen = reply.core_generation.clone();
    // Any command ticket supersedes close intent before the latest apply:
    // retire the sequencer record and publish null so old taken-unstarted
    // commands never dispatch.
    if ticket_command.is_some() {
        session
            .sequencer()
            .observe_close_envelope(None, &incarnation, &core_gen);
        crate::runtime_shared::publish_close_focus_current(shared, None);
        emit(shared, pid, generation, seq, "close-focus", "superseded", 1);
        return;
    }
    let envelope = session.sequencer().observe_close_envelope(
        reply.close_focus.as_ref(),
        &incarnation,
        &core_gen,
    );
    match envelope {
        CloseEnvelope::Absent => {
            crate::runtime_shared::publish_close_focus_current(shared, None);
            emit(shared, pid, generation, seq, "close-focus", "absent", 0);
            return;
        }
        CloseEnvelope::Stale => {
            emit(shared, pid, generation, seq, "close-focus", "stale", 0);
            return;
        }
        CloseEnvelope::DuplicateSpent => {
            emit(shared, pid, generation, seq, "close-focus", "duplicate", 0);
            return;
        }
        CloseEnvelope::Fresh => {}
    }
    let req = reply.close_focus.as_ref().expect("Fresh implies Some");
    // Publish the latest epoch first: older taken commands mismatch and
    // cancel; the in-flight same id still matches.
    crate::runtime_shared::publish_close_focus_current(
        shared,
        Some(CloseFocusEpoch {
            id: req.id.clone(),
            revision: req.issued_revision,
            adapter_incarnation: incarnation.clone(),
            core_generation: core_gen.clone(),
        }),
    );
    let target_str = req.target.0.clone();
    let fallback_pre = retained.observed_focus.clone();
    let fallback_known = retained.observed_focus_known;
    // Zero-setter satisfied: focus already names the target.
    if fallback_pre.as_ref() == Some(&req.target) {
        session.sequencer().spend_close(&req.id);
        session.stage_close_feedback(FocusFeedback {
            id: req.id.clone(),
            target: req.target.clone(),
            revision: req.issued_revision,
            setter: VisibilitySetter::NotCalled,
            reason: VisibilityReason::Empty,
            pre: Some(req.target.clone()),
            post: Some(req.target.clone()),
            foreground_proven: fallback_known,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        emit(shared, pid, generation, seq, "close-focus", "satisfied", 0);
        return;
    }
    // No-call helper: spends once, stages one raw row, counts the drop.
    let no_call =
        |session: &mut WorldSession, shared: &Arc<Mutex<Shared>>, reason: VisibilityReason| {
            session.sequencer().spend_close(&req.id);
            session.stage_close_feedback(FocusFeedback {
                id: req.id.clone(),
                target: req.target.clone(),
                revision: req.issued_revision,
                setter: VisibilitySetter::NotCalled,
                reason,
                pre: fallback_pre.clone(),
                post: None,
                foreground_proven: false,
                arrival: Vec::new(),
                attempt_ms: 0,
            });
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
        };
    // Frozen-target fences over retained truth plus the live native plan.
    let domain = DomainKey {
        output: req.output.clone(),
        workspace: WorkspaceId(req.workspace.clone()),
    };
    let Some((fpid, fwid)) = parse_window_id(&target_str) else {
        no_call(session, shared, VisibilityReason::IdentityStale);
        return;
    };
    let (app_generation, focus_slot, native_mode): (Option<u64>, Option<Arc<FocusSlot>>, bool) = {
        let guard = lock_shared(shared);
        let row = guard.apps.get(&fpid).filter(|r| !r.terminated);
        (
            row.map(|r| r.generation),
            row.map(|r| Arc::clone(&r.focus)),
            matches!(
                guard.config.topology_mode,
                crate::runtime_types::TopologyMode::Native
            ),
        )
    };
    let Some(app_generation) = app_generation else {
        no_call(session, shared, VisibilityReason::IdentityStale);
        return;
    };
    if retained.window_domains.get(&target_str) != Some(&domain) {
        no_call(session, shared, VisibilityReason::IdentityStale);
        return;
    }
    if !current.contains(&(req.output.0.clone(), req.workspace.clone())) {
        no_call(session, shared, VisibilityReason::Unreadable);
        return;
    }
    // Shown/current native leg when a live context exists (synthetic
    // skeletons skip this: `current` above already gates them).
    {
        let guard = lock_shared(shared);
        if let Some(ctx) = guard.native_context.clone() {
            let topology_revision = guard.topology_revision;
            let row = ctx
                .domains
                .iter()
                .find(|d| d.output == req.output && d.workspace.0 == req.workspace);
            match row {
                Some(d) if d.current && d.readable && ctx.revision == topology_revision => {}
                Some(_) => {
                    drop(guard);
                    no_call(session, shared, VisibilityReason::Unreadable);
                    return;
                }
                None => {
                    drop(guard);
                    no_call(session, shared, VisibilityReason::IdentityStale);
                    return;
                }
            }
        }
    }
    if retained.minimized.contains_key(&target_str) {
        no_call(session, shared, VisibilityReason::ShowBlocked);
        return;
    }
    let roster_row = retained
        .apps
        .get(&fpid)
        .and_then(|app| app.roster.get(&fwid));
    let Some(entry) = roster_row else {
        no_call(session, shared, VisibilityReason::IdentityStale);
        return;
    };
    if entry.minimized_meta || entry.hidden_meta {
        no_call(session, shared, VisibilityReason::ShowBlocked);
        return;
    }
    match entry.excluded {
        Some(crate::classifier::ExcludeReason::Minimized)
        | Some(crate::classifier::ExcludeReason::HiddenApp) => {
            no_call(session, shared, VisibilityReason::ShowBlocked);
            return;
        }
        Some(_) => {
            no_call(session, shared, VisibilityReason::Unsupported);
            return;
        }
        None => {}
    }
    // Fullscreen holds the foreground: the target itself or any same-domain
    // sibling holding a retained fullscreen window refuses the steal.
    let sibling_fullscreen = retained.apps.iter().any(|(apid, app)| {
        app.windows.iter().any(|w| {
            w.fullscreen
                && retained
                    .window_domains
                    .get(&crate::runtime_types::window_id(*apid, w.wid))
                    .is_some_and(|home| home == &domain)
        })
    });
    if sibling_fullscreen {
        no_call(session, shared, VisibilityReason::Fullscreen);
        return;
    }
    // Game/overlay defers to the core pre-publish gate (no adapter-side
    // game fact exists); unknown foreground never dispatches blind.
    if retained.observed_focus.is_none() && !retained.observed_focus_known {
        no_call(session, shared, VisibilityReason::FocusUnproven);
        return;
    }
    // Outbox full and ownership-lost legs spend into no-call, never retry.
    if lock_focus(focus_queue).len() >= focus_capacity {
        no_call(session, shared, VisibilityReason::Superseded);
        return;
    }
    let topology_revision = lock_shared(shared).topology_revision;
    let binding = CloseFocusBinding {
        close_id: req.id.clone(),
        target: req.target.clone(),
        issued_revision: req.issued_revision,
        adapter_incarnation: incarnation.clone(),
        core_generation: core_gen.clone(),
        output: req.output.clone(),
        workspace: WorkspaceId(req.workspace.clone()),
        domain,
        generation: app_generation,
        topology_revision,
        fallback_pre,
        fallback_known,
    };
    // Spend before the attempt-or-no-call fork: exactly once per envelope.
    session.sequencer().spend_close(&req.id);
    let fallback_for_slot = binding.fallback_pre.clone();
    let request_id = {
        retained.focus_seq += 1;
        retained.focus_seq
    };
    let request = FocusRequest {
        pid: fpid,
        generation: app_generation,
        request_id,
        wid: fwid,
        window: target_str.clone(),
        dispatched: true,
        verified: false,
        topology_revision,
    };
    // Owner actuation path, native mode only (synthetic skeletons never
    // read the slot). The owner arm precedes the queue record: a refused
    // command never leaves a queued `dispatched` row for the drain lane
    // to deliver or report. A stopped/missing slot is ownership lost:
    // no-call, spent, never retried, and never queued. Never stage
    // `Accepted` here: the owner result path stages the honest mechanical
    // row.
    if native_mode {
        // Re-check capacity before arming: never arm a worker command the
        // outbox cannot record. (The pre-dispatch fence above still
        // bounds the queue; the drain lane only shrinks it.)
        if lock_focus(focus_queue).len() >= focus_capacity {
            session.stage_close_feedback(FocusFeedback {
                id: req.id.clone(),
                target: req.target.clone(),
                revision: req.issued_revision,
                setter: VisibilitySetter::NotCalled,
                reason: VisibilityReason::Superseded,
                pre: binding.fallback_pre.clone(),
                post: None,
                foreground_proven: false,
                arrival: Vec::new(),
                attempt_ms: 0,
            });
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
            return;
        }
        match focus_slot {
            Some(slot) => {
                let cmd = FocusCommand {
                    pid: fpid,
                    generation: app_generation,
                    request_id,
                    wid: fwid,
                    window: target_str,
                    revision: topology_revision,
                    route: session.focus_route(),
                    expect_prior: None,
                    close: Some(binding),
                };
                if slot.set_latest(cmd).is_some() {
                    lock_focus(focus_queue).push(request);
                    lock_shared(shared).diagnostics.focus_dispatched += 1;
                    emit(shared, pid, generation, seq, "close-focus", "dispatched", 1);
                } else {
                    session.stage_close_feedback(FocusFeedback {
                        id: req.id.clone(),
                        target: req.target.clone(),
                        revision: req.issued_revision,
                        setter: VisibilitySetter::NotCalled,
                        reason: VisibilityReason::IdentityStale,
                        pre: fallback_for_slot.clone(),
                        post: None,
                        foreground_proven: false,
                        arrival: Vec::new(),
                        attempt_ms: 0,
                    });
                    lock_shared(shared).diagnostics.focus_dropped_full += 1;
                    emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
                }
            }
            None => {
                session.stage_close_feedback(FocusFeedback {
                    id: req.id.clone(),
                    target: req.target.clone(),
                    revision: req.issued_revision,
                    setter: VisibilitySetter::NotCalled,
                    reason: VisibilityReason::IdentityStale,
                    pre: fallback_for_slot.clone(),
                    post: None,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    attempt_ms: 0,
                });
                lock_shared(shared).diagnostics.focus_dropped_full += 1;
                emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
            }
        }
    } else {
        {
            let mut queue = lock_focus(focus_queue);
            if queue.len() >= focus_capacity {
                drop(queue);
                session.stage_close_feedback(FocusFeedback {
                    id: req.id.clone(),
                    target: req.target.clone(),
                    revision: req.issued_revision,
                    setter: VisibilitySetter::NotCalled,
                    reason: VisibilityReason::Superseded,
                    pre: binding.fallback_pre.clone(),
                    post: None,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    attempt_ms: 0,
                });
                lock_shared(shared).diagnostics.focus_dropped_full += 1;
                emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
                return;
            }
            queue.push(request);
        }
        lock_shared(shared).diagnostics.focus_dispatched += 1;
        emit(shared, pid, generation, seq, "close-focus", "dispatched", 1);
    }
}

fn settle_select_transfers(
    session: &mut WorldSession,
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReplyV4,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    let pending_ops: Vec<Option<String>> = lock_shared(shared)
        .select_pending
        .iter()
        .map(|claim| claim.operation_id.clone())
        .collect();
    if pending_ops.iter().all(Option::is_none) {
        return;
    }
    let mut held_ops: Vec<String> = Vec::new();
    let mut rebind_ops: Vec<String> = Vec::new();
    for operation_id in pending_ops.into_iter().flatten() {
        let held = reply
            .transfers
            .iter()
            .find(|target| target.operation_id == operation_id)
            .is_some_and(|target| target.held);
        if held {
            held_ops.push(operation_id);
            continue;
        }
        let needs_plan = lock_shared(shared).select_pending.iter().any(|claim| {
            claim.operation_id.as_deref() == Some(operation_id.as_str())
                && claim.select_target_output.is_none()
        });
        if needs_plan {
            rebind_ops.push(operation_id);
        }
    }
    if !held_ops.is_empty() {
        {
            let mut guard = lock_shared(shared);
            guard.select_pending.retain(|claim| {
                !claim
                    .operation_id
                    .as_deref()
                    .is_some_and(|op| held_ops.iter().any(|held| held == op))
            });
            if guard.select_pending.is_empty() {
                // The dropped claims will never be polled: clear their
                // publish wake so no spurious early body fires for them.
                // A later store re-arms through the same flag.
                guard.select_claim_pending = false;
            }
        }
        for _ in &held_ops {
            emit(shared, pid, generation, seq, "input", "select-held", 0);
        }
        rebind_ops.retain(|op| !held_ops.contains(op));
    }
    for operation_id in rebind_ops {
        // Rebind a plan-less claim (stale-refusal store) from this applied
        // reply: resolve the retained View transfer by operation identity,
        // then the live native binding (or explicit creation desire) plus
        // the observation-time baseline from the current native context.
        // Never invents a binding; missing targets leave the claim unbound
        // so the drain refuses without actuation. Claims already taken for
        // drain are popped from the queue, so the drain in progress is
        // never touched: only queued entries rebind.
        let transfer = session
            .engine_mut()
            .retained_transfers()
            .into_iter()
            .find(|target| {
                target.operation_id == operation_id
                    && matches!(target.kind, tiler_core::world_report::TransferKind::View)
            });
        let Some(transfer) = transfer else {
            continue;
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
        let Some(claim) = guard.select_pending.iter_mut().find(|claim| {
            claim.operation_id.as_deref() == Some(operation_id.as_str())
                && claim.select_target_output.is_none()
        }) else {
            continue;
        };
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::directional::OutputId;

    fn output() -> OutputId {
        OutputId("macos-display-1".to_owned())
    }

    fn empty_reply() -> tiler_core::world_report::WorldReplyV4 {
        tiler_core::world_report::WorldReplyV4 {
            contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
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
            close_focus: None,
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: crate::runtime::empty_world_presentation_v4(),
            intent_writes: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn unpersisted_writes_latch_holds_later_empty_reply() {
        use tiler_core::directional::WindowId;
        // Fail-closed latch through the production gate (the same
        // `unpersisted_hold` the four real apply sites call, after
        // acceptance and before any effect spend or slot/outbox write):
        // the first applied reply carries an opaque save, the next
        // carries none, yet both hold.
        let mut session = focus_session();
        let shared = focus_shared();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut first = empty_reply();
        first
            .intent_writes
            .push(tiler_core::world_report::IntentWrite {
                win: WindowId("4242_7".to_owned()),
                data: Some("opaque-save".to_owned()),
            });
        first.effects.push(focus_effect(5));
        assert!(
            unpersisted_hold(&mut session, &first),
            "write reply must hold"
        );
        assert!(
            session.unpersisted_intent_hold(),
            "latch sets on the write reply"
        );
        // No dependent effect spent at gate time: the held reply's effect
        // is still unspent (production never reached `spend_if_coherent`).
        assert_eq!(
            session.sequencer().take_unspent_effects(&first).len(),
            1,
            "held reply spends no effects"
        );
        // Later reply without writes stays held: never clears simply
        // because the later reply has no writes.
        let mut second = empty_reply();
        second.effects.push(focus_effect(6));
        assert!(
            unpersisted_hold(&mut session, &second),
            "latch must hold the later empty reply"
        );
        assert!(
            session.unpersisted_intent_hold(),
            "latch persists across dispatches"
        );
        assert_eq!(
            session.sequencer().take_unspent_effects(&second).len(),
            1,
            "latched hold still spends no effects"
        );
        // No dependent native write/presentation queued either route:
        // neither gate call touched slots, outbox, or retained presentation.
        assert!(lock_shared(&shared).last_desired.is_empty());
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn ordinary_save_free_reply_not_held_and_dispatches() {
        // Ordinary admission/reflow emits no saves, so the production gate
        // stays open — and the real production dispatch still applies.
        let mut session = focus_session();
        assert!(
            !unpersisted_hold(&mut session, &empty_reply()),
            "save-free reply must not hold"
        );
        assert!(!session.unpersisted_intent_hold());
        let mut retained = crate::runtime_core::RetainedCore::default();
        let shared = focus_shared();
        let inbox = Arc::new(Inbox::new(16));
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        match dispatch_report(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Synthetic,
            Vec::new(),
            None,
            None,
            false,
            crate::world_command_map::OP_RECONCILE,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("save-free synthetic dispatch must apply, got {other:?}"),
        }
        assert!(
            !session.unpersisted_intent_hold(),
            "save-free dispatch must not latch"
        );
    }

    #[test]
    fn capability_truth_at_reply_decode_refuses_host_unsupported_rows() {
        use tiler_core::world_report::{ScopeAuthority, StateProperty, StateTarget};
        use tiler_protocol::world_protocol::{
            DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, encode_reply_v4,
        };
        // Capability truth at reply decode: production `codec_transact`
        // decodes through `decode_reply_v4_for_host` against the actual
        // production `macos_capabilities` (above/warp/native-sticky all
        // false, engine-gated so honest replies never carry them). Each
        // host-unsupported row is shape-valid wire but refuses here as
        // `snapshot-invalid` with its fixed rule token — before any
        // sequencer acceptance or actuation.
        let host = crate::world_connection::macos_capabilities();
        assert!(!host.above && !host.warp && !host.native_sticky);
        let mut tainted = empty_reply();
        tainted
            .windows
            .push(tiler_core::world_report::WindowDesire {
                lifetime: tiler_core::directional::WindowId("4242_7".to_owned()),
                output: output(),
                workspace: "ws-a".to_owned(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: ScopeAuthority::Untouched,
                floating: false,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: vec![StateTarget {
                    property: StateProperty::Above,
                    value: true,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                }],
            });
        tainted.cursor_warp = Some((4, 4));
        let wire = encode_reply_v4(&DecodedReplyV4 {
            reply: tainted,
            wire: ReplyWireOptionals {
                suspended_output_present: false,
            },
        })
        .expect("tainted reply encodes");
        // Shape decode accepts the rows; the host-gated decode refuses on
        // the first gate (above runs before warp/sticky).
        decode_reply_v4(&wire).expect("shape decode accepts host-unsupported rows");
        let error = super::decode_reply_v4_for_host(&wire, &host)
            .expect_err("macOS host must refuse above/warp rows");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "above-capability-false");
        // Each gate fires independently: warp-only and sticky-only rows
        // refuse with their own fixed tokens.
        let mut warp_only = empty_reply();
        warp_only.cursor_warp = Some((4, 4));
        let warp_wire = encode_reply_v4(&DecodedReplyV4 {
            reply: warp_only,
            wire: ReplyWireOptionals {
                suspended_output_present: false,
            },
        })
        .expect("warp-only reply encodes");
        let error = super::decode_reply_v4_for_host(&warp_wire, &host)
            .expect_err("macOS host must refuse warp point");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "warp-capability-false");
        let mut sticky_only = empty_reply();
        sticky_only
            .windows
            .push(tiler_core::world_report::WindowDesire {
                lifetime: tiler_core::directional::WindowId("4242_7".to_owned()),
                output: output(),
                workspace: "ws-a".to_owned(),
                allocation: None,
                effective_frame: None,
                frame_authority: ScopeAuthority::Untouched,
                membership_authority: ScopeAuthority::Untouched,
                floating: false,
                float_origin: None,
                frame_operation: None,
                clear_operation: None,
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: vec![StateTarget {
                    property: StateProperty::Sticky,
                    value: true,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                }],
            });
        let sticky_wire = encode_reply_v4(&DecodedReplyV4 {
            reply: sticky_only,
            wire: ReplyWireOptionals {
                suspended_output_present: false,
            },
        })
        .expect("sticky-only reply encodes");
        let error = super::decode_reply_v4_for_host(&sticky_wire, &host)
            .expect_err("macOS host must refuse sticky target");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "sticky-capability-false");
        // Ordinary save-free replies stay accepted under the same gate
        // (success leg pinned end-to-end by
        // `ordinary_save_free_reply_not_held_and_dispatches` dispatching
        // through the real `codec_transact`).
        let clean_wire = encode_reply_v4(&DecodedReplyV4 {
            reply: empty_reply(),
            wire: ReplyWireOptionals {
                suspended_output_present: false,
            },
        })
        .expect("ordinary reply encodes");
        super::decode_reply_v4_for_host(&clean_wire, &host)
            .expect("ordinary reply must decode under macOS caps");
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
            main_wake: None,
            topology_revision: 9,
            native_context: None,
            native_cell: crate::runtime_shared::NativeContextCell::new(),
            select_pending: std::collections::VecDeque::new(),
            select_busy: false,
            select_claim_pending: false,
            selection_focus_pending: None,
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
            log: std::collections::VecDeque::new(),
            last_desired: Vec::new(),
            test_wake: None,
            test_seq: 0,
            last_geometry_key: None,
            last_presentation: crate::runtime::empty_world_presentation_v4(),
            last_pager_bindings: Vec::new(),
            last_workspace_desires: Vec::new(),
            last_core_generation: String::new(),
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
            true,
            crate::focus_route::FocusRoute::Slps,
            Arc::new(crate::world_sequencer::ObservationClock::new()),
            &[],
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

    #[test]
    fn held_same_view_reply_preserves_stable_presentation_and_desired() {
        fn unmarked_presentation(
            presentation: &tiler_core::world_report::WorldPresentationV4,
        ) -> tiler_core::world_report::WorldPresentationV4 {
            // The explicit trailing marker is report-dependent (incomplete
            // inventory clears it fail-closed); every other shape is
            // retained-state stable across the hold.
            let mut unmarked = presentation.clone();
            for screen in &mut unmarked.pager {
                for workspace in &mut screen.workspaces {
                    workspace.trailing = false;
                }
            }
            unmarked
        }
        // Offline held-presentation verification through the REAL engine
        // plus the REAL `consume_world_reply` (not a bare helper): a
        // controlled reply followed by a same-view incomplete-inventory
        // hold with unchanged native frames. The hold authorizes no
        // writes, but the stable plan truth (desired geometry) and the
        // stable shapes (presentation/pager) must survive it and refresh
        // once on recovery.
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::geometry::Rect;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            HoldReason, NativeWorkspaceFact, PresentationIngressV4, PresentationPagerFrameV4,
            PresentationRowV4, ScopeAuthority, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact,
            WorldReportV4,
        };

        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        fn screen(complete: bool) -> ScreenReport {
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(0, 0, 800, 600)),
                work_area: Some(rect(0, 0, 800, 560)),
                complete,
                scale: 1.0,
                identity: tiler_core::world_report::OutputIdentity::unknown(),
            }
        }
        fn fact() -> WindowFact {
            WindowFact {
                window: WindowId("4242_7".to_owned()),
                output: Some(OutputId("out-a".to_owned())),
                rect: Some(rect(0, 0, 400, 300)),
                fullscreen: Some(false),
                maximized: Some(false),
                sticky: Some(false),
                hints: Some(tiler_core::size_hints::WindowSizeHints::none()),
                minimized: Some(false),
                native_workspace: NativeWorkspaceFact::Omitted,
                host_hidden: Some(false),
                privileged: Some(false),
                managed: Some(true),
                accepts_input: Some(true),
                activation_inhibited: Some(false),
                tool_window: Some(false),
                owned: Some(false),
                transient: Some(false),
                modal: Some(false),
                resizable: Some(true),
                above: Some(false),
                below: Some(false),
                owner: None,
                parent: None,
                role: None,
                app_id: None,
                executable: Some("synthetic.exe".to_owned()),
                process_id: Some("foreign-pid".to_owned()),
                class: Some("Synthetic".to_owned()),
                title: None,
                window_type: Some(tiler_core::world_report::WindowType::Normal),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                layer: Some(0),
                fullscreen_owner: None,
                fullscreen_exitable: None,
                suppression_available: None,
                visibility: tiler_core::world_report::VisibilityFact {
                    visible: Some(true),
                    suppression_owned: None,
                },
                last_write: None,
            }
        }
        fn ingress() -> PresentationIngressV4 {
            PresentationIngressV4 {
                foreground: "4242_7".to_owned(),
                foreground_minimized: Some(false),
                foreground_maximized: Some(false),
                foreground_fullscreen: Some(false),
                foreground_host_hidden: Some(false),
                foreground_hidden_workspace: Some(false),
                foreground_frame: Some(rect(0, 0, 400, 300)),
                chord_held: false,
                move_window: String::new(),
                pager_order: Vec::new(),
                pager_frames: Vec::new(),
                witness: None,
                foreground_out: None,
            }
        }
        fn report(seq: u64, complete: bool) -> WorldReportV4 {
            let windows = vec![fact()];
            let mut presentation = ingress();
            presentation.pager_frames = vec![PresentationPagerFrameV4 {
                window: WindowId("4242_7".to_owned()),
                live: Some(rect(0, 0, 400, 300)),
            }];
            presentation.pager_order = vec![WindowId("4242_7".to_owned())];
            WorldReportV4 {
                contract_version: WORLD_CONTRACT_VERSION_V4,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
                observation_seq: seq,
                request_seq: seq,
                request_id: format!("req-{seq}"),
                command_seq: None,
                command: None,
                screens: vec![screen(complete)],
                held_screens: Vec::new(),
                windows,
                held_windows: Vec::new(),
                closed: vec![],
                native: Vec::new(),
                held_native: Vec::new(),
                cursor: None,
                cursor_hit: None,
                focused_global: "4242_7".to_owned(),
                foreground_known: true,
                action_foreground: None,
                action_cached: None,
                action_outputs: Vec::new(),
                primary_output: None,
                inventory_complete: true,
                gesture_barrier: false,
                gesture: None,
                monotonic_ms: seq * 1000,
                frames: Vec::new(),
                member_attempts: Vec::new(),
                view_attempts: Vec::new(),
                clear_attempts: Vec::new(),
                creation_attempts: Vec::new(),
                held_attempts: Vec::new(),
                state_attempts: Vec::new(),
                focus_feedback: Vec::new(),
                presentation: PresentationRowV4::Valid(presentation),
                intent_records: Vec::new(),
                renderer_input: None,
            }
        }

        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let controlled = match engine.handle_report(&report(1, true)) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("controlled report must accept, got {other:?}"),
        };
        assert!(
            controlled
                .scopes
                .iter()
                .all(|scope| scope.authority == ScopeAuthority::Controlled),
            "first reply actuates: {controlled:?}"
        );
        assert!(
            controlled
                .windows
                .iter()
                .any(|row| row.lifetime.0 == "4242_7" && row.effective_frame.is_some()),
            "first reply carries geometry: {controlled:?}"
        );
        let held = match engine.handle_report(&report(2, false)) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("incomplete-inventory report must accept held, got {other:?}"),
        };
        assert!(
            held.scopes
                .iter()
                .any(|scope| scope.authority
                    == ScopeAuthority::Hold(HoldReason::IncompleteInventory)),
            "second reply holds the scope: {held:?}"
        );
        assert!(
            held.windows
                .iter()
                .all(|row| row.effective_frame.is_none() && row.allocation.is_none()),
            "held rows carry no geometry (core publish): {held:?}"
        );
        // Same view (same homing, same bindings): only the hold differs.
        let homing = |reply: &tiler_core::world_report::WorldReplyV4| {
            reply
                .windows
                .iter()
                .map(|row| {
                    (
                        row.lifetime.0.clone(),
                        row.output.0.clone(),
                        row.workspace.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            homing(&controlled),
            homing(&held),
            "same view across the hold"
        );
        // Engine characterization: a same-view hold carries identical
        // shapes (presentation is read-only over retained state), so the
        // presentation overwrite below is already a no-op and needs no
        // cache - only the desired plan truth needs retention. The
        // explicit trailing marker is the one report-dependent exception:
        // incomplete inventory clears it fail-closed.
        let mut expected = unmarked_presentation(&controlled.presentation);
        expected.rev = held.presentation.rev;
        assert_eq!(
            held.presentation, expected,
            "same-view hold carries stable presentation with the marker cleared"
        );

        let shared = focus_shared();
        live_focus_app(&shared, 4242, 1);
        let mut session = focus_session();
        let mut retained = RetainedCore::default();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                None,
                None,
                "select",
                "accepted",
                0,
                0,
                0,
            );
        };
        dispatch(&mut session, &mut retained, &controlled);
        let desired_stable = lock_shared(&shared).last_desired.clone();
        assert!(
            !desired_stable.is_empty(),
            "controlled reply publishes plan truth"
        );
        let presentation_stable = lock_shared(&shared).last_presentation.clone();
        assert!(
            !presentation_stable.pager.is_empty(),
            "controlled reply publishes pager content"
        );
        let emitted_after_controlled = lock_shared(&shared).diagnostics.frames_emitted;

        dispatch(&mut session, &mut retained, &held);
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            emitted_after_controlled,
            "held reply authorizes no writes"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_stable,
            "same-view hold keeps stable plan truth"
        );
        assert_eq!(
            lock_shared(&shared).last_presentation,
            held.presentation,
            "same-view hold overwrites the presentation fail-closed"
        );

        // Recovery refreshes once: a controlled reply for the same view
        // publishes fresh geometry again.
        let recovered = match engine.handle_report(&report(3, true)) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("recovery report must accept, got {other:?}"),
        };
        assert!(
            recovered
                .scopes
                .iter()
                .all(|scope| scope.authority == ScopeAuthority::Controlled),
            "recovery reply actuates: {recovered:?}"
        );
        dispatch(&mut session, &mut retained, &recovered);
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_stable,
            "recovery republishes the same stable plan truth"
        );
        assert!(
            lock_shared(&shared).diagnostics.frames_emitted > emitted_after_controlled,
            "recovery authorizes writes again"
        );
    }

    fn stable_held_row(
        window: &str,
        output: &str,
        workspace: &str,
        reason: tiler_core::world_report::HoldReason,
    ) -> tiler_core::world_report::WindowDesire {
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::world_report::ScopeAuthority;
        tiler_core::world_report::WindowDesire {
            lifetime: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            workspace: workspace.to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Hold(reason),
            membership_authority: ScopeAuthority::Hold(reason),
            frame_operation: None,
            clear_operation: None,
            floating: false,
            float_origin: None,
            command_seq: None,
            visibility: None,
            visibility_authority: ScopeAuthority::Hold(reason),
            state: Vec::new(),
        }
    }

    fn stable_current() -> BTreeSet<(String, String)> {
        [("out-a".to_owned(), "ws-a".to_owned())]
            .into_iter()
            .collect()
    }

    fn stable_workareas() -> BTreeMap<(String, String), (Rect, Rect)> {
        [(
            ("out-a".to_owned(), "ws-a".to_owned()),
            (
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 560,
                },
                Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
            ),
        )]
        .into_iter()
        .collect()
    }

    fn stable_prior_row() -> DesiredFrame {
        DesiredFrame {
            window: "4242_7".to_owned(),
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        }
    }

    /// Shared fixture for the retention gate tests: the minted key, the
    /// same-state held reply, its current scopes, and its workareas.
    type StableHoldFixture = (
        AppliedGeometryKey,
        WorldReplyV4,
        BTreeSet<(String, String)>,
        BTreeMap<(String, String), (Rect, Rect)>,
    );

    fn stable_keyed_hold() -> StableHoldFixture {
        // First apply (no prior key) publishes fresh and mints the key;
        // the held reply below shares its exact state.
        let current = stable_current();
        let workareas = stable_workareas();
        let prior = stable_prior_row();
        let mut controlled = empty_reply();
        controlled.windows = vec![tiler_core::world_report::WindowDesire {
            lifetime: tiler_core::directional::WindowId("4242_7".to_owned()),
            output: tiler_core::directional::OutputId("out-a".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: Some(tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            }),
            effective_frame: Some(tiler_core::geometry::Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            }),
            frame_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            membership_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            frame_operation: None,
            clear_operation: None,
            floating: false,
            float_origin: None,
            command_seq: None,
            visibility: None,
            visibility_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            state: Vec::new(),
        }];
        let (desired, key) = retain_stable_geometry(
            &[],
            None,
            &current,
            &workareas,
            &controlled,
            vec![prior.clone()],
        );
        assert_eq!(desired, vec![prior]);
        let mut held = empty_reply();
        held.windows = vec![stable_held_row(
            "4242_7",
            "out-a",
            "ws-a",
            tiler_core::world_report::HoldReason::IncompleteInventory,
        )];
        (key, held, current, workareas)
    }

    #[test]
    fn stable_geometry_same_state_hold_carries_and_fresh_wins() {
        let prior = stable_prior_row();
        let (key, held, current, workareas) = stable_keyed_hold();
        // Same-state hold carries the last applied row; the key is stable.
        let (desired, next_key) = retain_stable_geometry(
            std::slice::from_ref(&prior),
            Some(&key),
            &current,
            &workareas,
            &held,
            Vec::new(),
        );
        assert_eq!(desired, vec![prior.clone()]);
        assert_eq!(next_key, key);
        // Fresh rows always win: no duplication when geometry is present.
        let (desired, _) = retain_stable_geometry(
            std::slice::from_ref(&prior),
            Some(&key),
            &current,
            &workareas,
            &held,
            vec![prior.clone()],
        );
        assert_eq!(desired.len(), 1);
    }

    #[test]
    fn stable_geometry_minimized_absence_stands() {
        let prior = stable_prior_row();
        let (key, _, current, workareas) = stable_keyed_hold();
        // A minimized lifetime has no managed geometry: the same-state
        // hold drops the prior row instead of resurrecting it.
        let mut minimized = empty_reply();
        minimized.windows = vec![stable_held_row(
            "4242_7",
            "out-a",
            "ws-a",
            tiler_core::world_report::HoldReason::Minimized,
        )];
        let (desired, next_key) = retain_stable_geometry(
            &[prior],
            Some(&key),
            &current,
            &workareas,
            &minimized,
            Vec::new(),
        );
        assert!(desired.is_empty());
        assert_eq!(next_key, key);
    }

    #[test]
    fn stable_geometry_clears_on_changed_lifetime() {
        let prior = stable_prior_row();
        let (key, _, current, workareas) = stable_keyed_hold();
        // An added lifetime moves the key: the fresh reply publishes
        // as-is (clear, never a stale carry).
        let mut added = empty_reply();
        added.windows = vec![
            stable_held_row(
                "4242_7",
                "out-a",
                "ws-a",
                tiler_core::world_report::HoldReason::IncompleteInventory,
            ),
            stable_held_row(
                "4242_8",
                "out-a",
                "ws-a",
                tiler_core::world_report::HoldReason::IncompleteInventory,
            ),
        ];
        let (desired, next_key) = retain_stable_geometry(
            &[prior],
            Some(&key),
            &current,
            &workareas,
            &added,
            Vec::new(),
        );
        assert!(desired.is_empty());
        assert_ne!(next_key, key);
    }

    #[test]
    fn stable_geometry_clears_on_workarea_only_change() {
        // Same view, same bindings, same lifetimes — but the retained
        // native work area moved (dock/resolution change mid-hold): the
        // fresh reply publishes as-is instead of keeping stale geometry.
        let prior = stable_prior_row();
        let (key, held, current, _) = stable_keyed_hold();
        let mut moved = stable_workareas();
        let entry = moved
            .get_mut(&("out-a".to_owned(), "ws-a".to_owned()))
            .expect("stable scope present");
        entry.0.h = 500;
        let (desired, next_key) =
            retain_stable_geometry(&[prior], Some(&key), &current, &moved, &held, Vec::new());
        assert!(desired.is_empty());
        assert_ne!(next_key, key);
    }

    #[test]
    fn stable_geometry_never_writes_off_current() {
        let prior = stable_prior_row();
        let workareas = stable_workareas();
        // Same homing as the held reply below, but the row's scope is
        // not current: nothing is carried there.
        let key = AppliedGeometryKey {
            current: [("out-a".to_owned(), "ws-b".to_owned())]
                .into_iter()
                .collect(),
            bindings: Vec::new(),
            homing: [("4242_7".to_owned(), ("out-a".to_owned(), "ws-a".to_owned()))]
                .into_iter()
                .collect(),
            workareas: workareas.clone(),
        };
        let mut held = empty_reply();
        held.windows = vec![stable_held_row(
            "4242_7",
            "out-a",
            "ws-a",
            tiler_core::world_report::HoldReason::IncompleteInventory,
        )];
        let off_current: BTreeSet<(String, String)> = [("out-a".to_owned(), "ws-b".to_owned())]
            .into_iter()
            .collect();
        let (desired, _) = retain_stable_geometry(
            &[prior],
            Some(&key),
            &off_current,
            &workareas,
            &held,
            Vec::new(),
        );
        assert!(desired.is_empty());
    }

    #[test]
    fn stable_geometry_needs_a_hold() {
        let prior = stable_prior_row();
        let (key, _, current, workareas) = stable_keyed_hold();
        // Same state but no hold anywhere: a geometry loss publishes
        // as-is instead of being papered over.
        let mut unheld = empty_reply();
        unheld.windows = vec![tiler_core::world_report::WindowDesire {
            lifetime: tiler_core::directional::WindowId("4242_7".to_owned()),
            output: tiler_core::directional::OutputId("out-a".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            membership_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            frame_operation: None,
            clear_operation: None,
            floating: false,
            float_origin: None,
            command_seq: None,
            visibility: None,
            visibility_authority: tiler_core::world_report::ScopeAuthority::Controlled,
            state: Vec::new(),
        }];
        let (desired, _) = retain_stable_geometry(
            &[prior],
            Some(&key),
            &current,
            &workareas,
            &unheld,
            Vec::new(),
        );
        assert!(desired.is_empty());
    }

    fn workarea_ctx(visible_h: i32) -> Arc<crate::runtime_types::NativeContext> {
        use tiler_core::directional::{OutputId, WorkspaceId};
        Arc::new(crate::runtime_types::NativeContext {
            revision: 9,
            domains: vec![crate::runtime_types::NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("space-7".to_owned()),
                space: 7,
                current: true,
                readable: true,
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: 600,
                },
                visible: Rect {
                    x: 0,
                    y: 0,
                    w: 800,
                    h: visible_h,
                },
            }],
            skipped: Vec::new(),
        })
    }

    fn workarea_reply(controlled: bool) -> tiler_core::world_report::WorldReplyV4 {
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::world_report::{ScopeAuthority, TilingMode};
        let mut reply = empty_reply();
        reply.workspaces = vec![tiler_core::world_report::WorkspaceDesire {
            id: "ws-a".to_owned(),
            output: OutputId("out-a".to_owned()),
            native_binding: Some("space-7".to_owned()),
            scope: tiler_core::world_report::AuthorityScope::Output(OutputId("out-a".to_owned())),
            order: 0,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: false,
        }];
        let authority = if controlled {
            ScopeAuthority::Controlled
        } else {
            ScopeAuthority::Hold(tiler_core::world_report::HoldReason::IncompleteInventory)
        };
        let frame = controlled.then_some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        });
        reply.windows = vec![tiler_core::world_report::WindowDesire {
            lifetime: WindowId("4242_7".to_owned()),
            output: OutputId("out-a".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: frame,
            effective_frame: frame,
            frame_authority: authority,
            membership_authority: authority,
            frame_operation: None,
            clear_operation: None,
            floating: false,
            float_origin: None,
            command_seq: None,
            visibility: None,
            visibility_authority: authority,
            state: Vec::new(),
        }];
        reply
    }

    #[test]
    fn consume_hold_clears_retention_on_workarea_only_change() {
        // End-to-end through the REAL `consume_world_reply` with a live
        // native context: an unchanged-workarea hold preserves stable plan
        // truth, a workarea-only topology move during the hold clears it,
        // and recovery republishes. No native writes ride the retention
        // (frames only move on controlled recovery).
        let shared = focus_shared();
        live_focus_app(&shared, 4242, 1);
        let mut session = focus_session();
        let mut retained = RetainedCore::default();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        lock_shared(&shared).native_context = Some(workarea_ctx(560));
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                None,
                None,
                "select",
                "accepted",
                0,
                0,
                0,
            );
        };
        let controlled = workarea_reply(true);
        let held = workarea_reply(false);
        dispatch(&mut session, &mut retained, &controlled);
        let desired_stable = lock_shared(&shared).last_desired.clone();
        assert_eq!(
            desired_stable.len(),
            1,
            "controlled reply publishes plan truth"
        );
        let key_stable = lock_shared(&shared)
            .last_geometry_key
            .clone()
            .expect("controlled reply mints the key");
        let emitted_after_controlled = lock_shared(&shared).diagnostics.frames_emitted;

        dispatch(&mut session, &mut retained, &held);
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            emitted_after_controlled,
            "held reply authorizes no writes"
        );
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_stable,
            "unchanged-workarea hold preserves stable plan truth"
        );
        assert_eq!(
            lock_shared(&shared).last_geometry_key,
            Some(key_stable.clone()),
            "unchanged workarea keeps the key"
        );

        // Workarea-only move (same view, same bindings, same lifetimes,
        // same revision): the held reply must clear, never keep stale
        // geometry sized for the old work area.
        lock_shared(&shared).native_context = Some(workarea_ctx(500));
        dispatch(&mut session, &mut retained, &held);
        assert!(
            lock_shared(&shared).last_desired.is_empty(),
            "fresh workarea clears the held retention"
        );
        assert_ne!(
            lock_shared(&shared).last_geometry_key,
            Some(key_stable),
            "workarea move rotates the key"
        );
        assert_eq!(
            lock_shared(&shared).diagnostics.frames_emitted,
            emitted_after_controlled,
            "clearing retention authorizes no native writes"
        );

        // Recovery refreshes: a controlled reply republishes fresh geometry
        // and writes again.
        dispatch(&mut session, &mut retained, &controlled);
        assert_eq!(
            lock_shared(&shared).last_desired,
            desired_stable,
            "recovery republishes plan truth"
        );
        assert!(
            lock_shared(&shared).diagnostics.frames_emitted > emitted_after_controlled,
            "recovery authorizes writes again"
        );
    }

    #[test]
    fn settle_select_transfers_drops_held_before_actuation_and_keeps_live_queued() {
        // T3 stale-topology supersession of ACTUATION only: a held
        // (possibly-executed) transfer drops its matching queued claims so
        // the drain never re-actuates them — holds preserve native state —
        // while live queued claims stay queued in order. The intent already
        // executed core-side at-most-once; the drop is drain-side only,
        // counted, with no native touch and no pre-core refusal.
        let shared = focus_shared();
        let mut session = focus_session();
        let mut held_claim = crate::runtime_types::ClaimedSelect::tap(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 2 },
        );
        held_claim.operation_id = Some("op-1".to_owned());
        let mut live_claim = crate::runtime_types::ClaimedSelect::tap(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 1 },
        );
        live_claim.operation_id = Some("op-2".to_owned());
        live_claim.select_target_output = Some(output().0.clone());
        {
            let mut guard = lock_shared(&shared);
            guard.select_pending.push_back(held_claim);
            guard.select_pending.push_back(live_claim);
            guard.select_claim_pending = true;
        }
        let mut reply = empty_reply();
        reply
            .transfers
            .push(tiler_core::world_report::TransferTargetV4 {
                operation_id: "op-1".to_owned(),
                kind: tiler_core::world_report::TransferKind::View,
                target_output: output(),
                target_workspace: "ws".to_owned(),
                follow: false,
                held: true,
                command_seq: None,
            });
        settle_select_transfers(&mut session, &shared, &reply, 0, 0, 0);
        let guard = lock_shared(&shared);
        let ops: Vec<Option<String>> = guard
            .select_pending
            .iter()
            .map(|claim| claim.operation_id.clone())
            .collect();
        assert_eq!(
            ops,
            vec![Some("op-2".to_owned())],
            "held claim drops before actuation, live claim stays queued"
        );
        assert!(
            guard
                .log
                .iter()
                .any(|entry| entry.kind == "input" && entry.outcome == "select-held"),
            "held drop counted on the ring"
        );
    }

    // -- Close-focus dispatch (production consume/spend/outbox + mock slot) --

    const CLOSE_PID: u32 = 4242;
    const CLOSE_GEN: u64 = 1;

    fn close_domain() -> tiler_core::session::DomainKey {
        tiler_core::session::DomainKey {
            output: output(),
            workspace: tiler_core::directional::WorkspaceId("ws-a".to_owned()),
        }
    }

    fn close_target(wid: u32) -> tiler_core::directional::WindowId {
        tiler_core::directional::WindowId(format!("{CLOSE_PID}_{wid}"))
    }

    fn close_roster_entry(wid: u32) -> crate::runtime_core::RosterEntry {
        crate::runtime_core::RosterEntry {
            observed: Some(crate::runtime::ObservedWindow {
                wid,
                x: 0,
                y: 0,
                w: 100,
                h: 100,
                floating: false,
                fullscreen: false,
                hints: crate::runtime::WindowSizeHints::none(),
            }),
            spaces: vec![7],
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
            activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        }
    }

    fn close_retained(target_wid: u32, baseline: Option<WindowId>, known: bool) -> RetainedCore {
        let mut retained = RetainedCore::default();
        let mut app = crate::runtime_core::RetainedApp {
            generation: CLOSE_GEN,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        };
        app.roster
            .insert(target_wid, close_roster_entry(target_wid));
        retained.apps.insert(CLOSE_PID, app);
        retained
            .window_domains
            .insert(format!("{CLOSE_PID}_{target_wid}"), close_domain());
        retained.observed_focus = baseline;
        retained.observed_focus_known = known;
        retained
    }

    fn close_reply_for(
        session: &WorldSession,
        id: &str,
        rev: u64,
        target_wid: u32,
    ) -> tiler_core::world_report::WorldReplyV4 {
        let mut reply = empty_reply();
        reply.adapter_incarnation = session.incarnation().to_owned();
        reply.core_generation = session.sequencer().bound_generation();
        reply.outputs = vec![tiler_core::world_report::OutputDesire {
            output: output(),
            logical_view: Some("ws-a".to_owned()),
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            hold: None,
        }];
        reply.close_focus = Some(tiler_core::world_report::CloseFocusRequest {
            id: id.to_owned(),
            target: close_target(target_wid),
            issued_revision: rev,
            cause: tiler_core::world_report::CloseFocusCause::PositiveClose,
            output: output(),
            workspace: "ws-a".to_owned(),
        });
        reply
    }

    fn consume_close(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        reply: &tiler_core::world_report::WorldReplyV4,
        ticket: Option<&WorldCommand>,
    ) {
        consume_world_reply(
            session,
            retained,
            shared,
            focus_queue,
            8,
            reply,
            &[],
            ticket,
            None,
            "reconcile",
            "accepted",
            CLOSE_PID,
            CLOSE_GEN,
            1,
        );
    }

    #[test]
    fn close_latest_only_skips_issuing_and_dispatches_once() {
        // Focus A(7),C(9),B(8) then close B baseline A => target C. The
        // issuing/intermediate publications are skipped: only the last
        // snapshot (rev 7) applies, yielding exactly one outbox attempt and
        // one mock slot command with the observed fallback.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "close-focus-7", 7, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1, "exactly one attempt");
        assert_eq!(lock_focus(&focus_queue)[0].window, format!("{CLOSE_PID}_9"));
        let cmd = slot.take().expect("mock owner slot carries the attempt");
        assert_eq!(cmd.window, format!("{CLOSE_PID}_9"));
        assert_eq!(
            cmd.close.as_ref().expect("typed close").close_id,
            "close-focus-7"
        );
        assert_eq!(
            cmd.close.as_ref().expect("typed close").fallback_pre,
            Some(close_target(7))
        );
        assert!(crate::runtime_shared::close_focus_is_current(
            &shared,
            session.incarnation(),
            &session.sequencer().bound_generation(),
            "close-focus-7",
            7
        ));
        assert!(
            session.peek_close_feedback().is_none(),
            "attempt stages nothing"
        );
        assert_eq!(lock_shared(&shared).diagnostics.focus_dispatched, 1);
    }

    #[test]
    fn close_duplicate_latest_no_replay() {
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "close-focus-7", 7, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "duplicate replays nothing"
        );
        assert!(slot.take().is_none(), "duplicate sets no slot");
        assert!(session.peek_close_feedback().is_none());
    }

    #[test]
    fn close_current_then_null_then_old_cached_rejected() {
        // Coherent-latest precondition: the current publication (rev 7)
        // applies first, then a null retires it into the fence. A delayed
        // older cached request (rev 4) then refuses as Stale: no new
        // outbox row, no slot command, no staging, epoch stays null.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let current = close_reply_for(&session, "close-focus-7", 7, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &current,
            None,
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        let armed = slot.take().expect("current arms the owner slot");
        assert_eq!(
            armed.close.as_ref().expect("typed close").close_id,
            "close-focus-7"
        );
        let mut null_reply = empty_reply();
        null_reply.adapter_incarnation = session.incarnation().to_owned();
        null_reply.core_generation = session.sequencer().bound_generation();
        null_reply.outputs = vec![tiler_core::world_report::OutputDesire {
            output: output(),
            logical_view: Some("ws-a".to_owned()),
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            hold: None,
        }];
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &null_reply,
            None,
        );
        assert!(lock_shared(&shared).close_focus_current.is_none());
        let old = close_reply_for(&session, "close-focus-4", 4, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &old,
            None,
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "old cached replays nothing"
        );
        assert!(slot.take().is_none(), "old cached arms no slot");
        assert!(session.peek_close_feedback().is_none());
        assert!(lock_shared(&shared).close_focus_current.is_none());
    }

    #[test]
    fn close_public_gate_null_applied_then_old_cached_refused_before_consume() {
        // Public-gate regression through `accept_reply`: the first applied
        // publication is null (the issuing request was skipped/retired),
        // then a delayed older cached request refuses at the gate as
        // stale-revision and never reaches consume: zero slot calls, zero
        // outbox rows, zero staged feedback, epoch stays null.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let incarnation = session.incarnation().to_owned();
        let generation = session.sequencer().bound_generation();
        // First flight: null publication at a newer revision.
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("first flight");
        let mut null_reply = empty_reply();
        null_reply.adapter_incarnation = incarnation.clone();
        null_reply.core_generation = generation.clone();
        null_reply.request_id = ticket.request_id.clone();
        null_reply.based_on_observation_seq = ticket.observation_seq;
        null_reply.state_revision = 7;
        null_reply.ready = true;
        null_reply.outputs = vec![tiler_core::world_report::OutputDesire {
            output: output(),
            logical_view: Some("ws-a".to_owned()),
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            hold: None,
        }];
        let applied = match session.sequencer().accept_reply(&null_reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => panic!("null must apply, got {reason}"),
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        assert!(lock_shared(&shared).close_focus_current.is_none());
        // Delayed older cached request on a new flight: older revision.
        let ticket2 = session
            .sequencer()
            .try_begin_request(None)
            .expect("second flight");
        let mut old = close_reply_for(&session, "close-focus-4", 4, 9);
        old.request_id = ticket2.request_id.clone();
        old.based_on_observation_seq = ticket2.observation_seq;
        old.state_revision = 4;
        old.ready = true;
        match session.sequencer().accept_reply(&old) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "stale-revision"),
            ReplyVerdict::Applied(_) => panic!("old cached must refuse at the gate"),
        }
        // Never consumed: zero slot calls, zero outbox rows, zero staging.
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        assert!(lock_shared(&shared).close_focus_current.is_none());
    }

    #[test]
    fn close_safety_refusal_spent_and_silent_reemit() {
        // Unknown PID target: spends once into NotCalled IdentityStale with
        // first-wins feedback; a retained re-emit is DuplicateSpent silent.
        let mut session = focus_session();
        let shared = focus_shared();
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        // Unknown target lives outside every fence.
        let mut reply = close_reply_for(&session, "close-focus-5", 5, 9);
        reply.close_focus.as_mut().expect("req").target =
            tiler_core::directional::WindowId("9999_1".to_owned());
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        let staged = session
            .peek_close_feedback()
            .expect("refusal stages one row");
        assert_eq!(staged.id, "close-focus-5");
        assert_eq!(
            staged.setter,
            tiler_core::world_report::VisibilitySetter::NotCalled
        );
        assert_eq!(
            staged.reason,
            tiler_core::world_report::VisibilityReason::IdentityStale
        );
        assert_eq!(lock_shared(&shared).diagnostics.focus_dropped_full, 1);
        let dropped_before = lock_shared(&shared).diagnostics.focus_dropped_full;
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dropped_full,
            dropped_before,
            "DuplicateSpent re-emit stays silent"
        );
        assert_eq!(
            session.peek_close_feedback().expect("first wins").id,
            "close-focus-5"
        );
    }

    #[test]
    fn close_already_target_zero_setter() {
        // Successful OWN attempt then next observation names C: satisfied
        // with zero setter, pre=post=target, no outbox or slot write.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(9)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "close-focus-6", 6, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_focus(&focus_queue).is_empty(),
            "satisfied writes no outbox"
        );
        assert!(slot.take().is_none(), "satisfied sets no slot");
        let staged = session.peek_close_feedback().expect("satisfied stages");
        assert_eq!(
            staged.setter,
            tiler_core::world_report::VisibilitySetter::NotCalled
        );
        assert_eq!(
            staged.reason,
            tiler_core::world_report::VisibilityReason::Empty
        );
        assert_eq!(staged.pre, Some(close_target(9)));
        assert_eq!(staged.post, Some(close_target(9)));
    }

    #[test]
    fn close_ticket_and_null_supersede_to_zero() {
        // A later focus/command/view ticket before apply supersedes: the
        // dispatch publishes null, stages nothing, queues nothing. A last
        // null then yields zero setters.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "close-focus-7", 7, 9);
        let ticket = WorldCommand::Focus {
            window: close_target(8),
            direction: "left".to_owned(),
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            Some(&ticket),
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        assert!(lock_shared(&shared).close_focus_current.is_none());
        // Last null yields zero setters.
        let mut null_reply = empty_reply();
        null_reply.adapter_incarnation = session.incarnation().to_owned();
        null_reply.core_generation = session.sequencer().bound_generation();
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &null_reply,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
    }

    #[test]
    fn close_generation_alias_rejected() {
        // Same id/revision on a foreign session never adopts: an old cached
        // generation alias and a foreign incarnation both refuse with no
        // outbox, no slot, no staging.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut alias = close_reply_for(&session, "close-focus-7", 7, 9);
        alias.core_generation = "gen-alias".to_owned();
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &alias,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        let mut foreign = close_reply_for(&session, "close-focus-7", 7, 9);
        foreign.adapter_incarnation = "adapter-foreign".to_owned();
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &foreign,
            None,
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
    }

    #[test]
    fn close_outbox_full_and_slot_stopped_no_call_spent() {
        // Outbox saturation spends into Superseded no-call; a stopped slot
        // spends into IdentityStale no-call. Both stage once and never retry.
        let mut session = focus_session();
        let shared = focus_shared();
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        for i in 0..8u32 {
            lock_focus(&focus_queue).push(FocusRequest {
                pid: CLOSE_PID,
                generation: CLOSE_GEN,
                request_id: u64::from(i),
                wid: 100 + i,
                window: format!("{CLOSE_PID}_{}", 100 + i),
                dispatched: true,
                verified: false,
                topology_revision: 9,
            });
        }
        let reply = close_reply_for(&session, "close-focus-9", 9, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            8,
            "full outbox queues nothing"
        );
        let staged = session.peek_close_feedback().expect("saturation stages");
        assert_eq!(
            staged.reason,
            tiler_core::world_report::VisibilityReason::Superseded
        );

        // Stopped slot: ownership lost spends into IdentityStale no-call.
        // The owner arm precedes the queue record, so a stopped slot
        // leaves no queued `dispatched` row for the drain lane to
        // deliver: only the no-call evidence stages, exactly once, with
        // no slot command and no dispatch counter.
        let mut session2 = focus_session();
        let shared2 = focus_shared();
        let slot2 = live_focus_app(&shared2, CLOSE_PID, CLOSE_GEN);
        slot2.stop();
        let mut retained2 = close_retained(9, Some(close_target(7)), true);
        let focus_queue2 = Arc::new(Mutex::new(Vec::new()));
        let reply2 = close_reply_for(&session2, "close-focus-10", 10, 9);
        consume_close(
            &mut session2,
            &mut retained2,
            &shared2,
            &focus_queue2,
            &reply2,
            None,
        );
        assert!(
            lock_focus(&focus_queue2).is_empty(),
            "stopped slot queues nothing"
        );
        assert!(slot2.take().is_none(), "stopped slot carries no command");
        let staged2 = session2.peek_close_feedback().expect("slot loss stages");
        assert_eq!(
            staged2.reason,
            tiler_core::world_report::VisibilityReason::IdentityStale
        );
        assert_eq!(
            lock_shared(&shared2).diagnostics.focus_dispatched,
            0,
            "no-call moves no dispatch counter"
        );
        assert_eq!(
            lock_shared(&shared2).diagnostics.focus_dropped_full,
            1,
            "slot loss counts exactly one no-call"
        );
        // A spent re-emit stays silent: no retry, no second row.
        let dropped_before = lock_shared(&shared2).diagnostics.focus_dropped_full;
        consume_close(
            &mut session2,
            &mut retained2,
            &shared2,
            &focus_queue2,
            &reply2,
            None,
        );
        assert!(lock_focus(&focus_queue2).is_empty());
        assert!(slot2.take().is_none());
        assert_eq!(
            lock_shared(&shared2).diagnostics.focus_dropped_full,
            dropped_before,
            "DuplicateSpent re-emit stays silent"
        );
        assert_eq!(
            session2.peek_close_feedback().expect("first wins").id,
            "close-focus-10"
        );
    }

    #[test]
    fn close_stale_result_first_wins_keeps_first_feedback() {
        // Pending old result correlation: first-wins staging never lets a
        // later (even older-id) result overwrite the fresh request's first
        // feedback row.
        let mut session = focus_session();
        let first = tiler_core::world_report::FocusFeedback {
            id: "close-focus-7".to_owned(),
            target: close_target(9),
            revision: 7,
            setter: tiler_core::world_report::VisibilitySetter::NotCalled,
            reason: tiler_core::world_report::VisibilityReason::IdentityStale,
            pre: Some(close_target(7)),
            post: None,
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        };
        session.stage_close_feedback(first.clone());
        session.stage_close_feedback(tiler_core::world_report::FocusFeedback {
            id: "close-focus-6".to_owned(),
            target: close_target(9),
            revision: 6,
            setter: tiler_core::world_report::VisibilitySetter::NotCalled,
            reason: tiler_core::world_report::VisibilityReason::Superseded,
            pre: Some(close_target(7)),
            post: None,
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        assert_eq!(session.peek_close_feedback().expect("first wins"), first);
    }

    #[test]
    fn close_core_producer_codec_latest_reply_mock_actuator() {
        // Genuine producer -> codec -> core -> latest-reply -> MOCK actuator:
        // focus A,C,B then close B baseline A mints target C through the
        // real engine; the reply round-trips the real v4 byte codec; only
        // the last snapshot applies to the mock outbox/slot.
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            CloseFocusCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
            ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReportV4,
        };
        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        fn screen(output: &str, x: i32) -> ScreenReport {
            ScreenReport {
                output: OutputId(output.to_owned()),
                full_rect: Some(rect(x, 0, 800, 600)),
                work_area: Some(rect(x, 0, 800, 560)),
                complete: true,
                scale: 1.0,
                identity: tiler_core::world_report::OutputIdentity::unknown(),
            }
        }
        fn fact(window: &str, output: &str, x: i32) -> WindowFact {
            WindowFact {
                window: WindowId(window.to_owned()),
                output: Some(OutputId(output.to_owned())),
                rect: Some(rect(x, 0, 40, 90)),
                fullscreen: Some(false),
                maximized: Some(false),
                sticky: Some(false),
                hints: Some(WindowSizeHints::none()),
                minimized: Some(false),
                native_workspace: NativeWorkspaceFact::Omitted,
                host_hidden: Some(false),
                privileged: Some(false),
                managed: Some(true),
                accepts_input: Some(true),
                activation_inhibited: Some(false),
                tool_window: Some(false),
                owned: Some(false),
                transient: Some(false),
                modal: Some(false),
                resizable: Some(true),
                above: Some(false),
                below: Some(false),
                owner: None,
                parent: None,
                role: None,
                app_id: None,
                executable: Some("synthetic.exe".to_owned()),
                process_id: Some("foreign-pid".to_owned()),
                class: Some("Synthetic".to_owned()),
                title: None,
                window_type: Some(tiler_core::world_report::WindowType::Normal),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                layer: Some(0),
                fullscreen_owner: None,
                fullscreen_exitable: None,
                suppression_available: None,
                visibility: tiler_core::world_report::VisibilityFact {
                    visible: Some(true),
                    suppression_owned: None,
                },
                last_write: None,
            }
        }
        fn report(
            seq: u64,
            windows: Vec<WindowFact>,
            closed: Vec<&str>,
            focused: &str,
        ) -> WorldReportV4 {
            WorldReportV4 {
                contract_version: WORLD_CONTRACT_VERSION_V4,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
                observation_seq: seq,
                request_seq: seq,
                request_id: format!("req-{seq}"),
                command_seq: None,
                command: None,
                screens: vec![screen("out-a", 0)],
                held_screens: Vec::new(),
                windows,
                held_windows: Vec::new(),
                held_native: Vec::new(),
                native: Vec::new(),
                closed: closed
                    .into_iter()
                    .map(|id| WindowId(id.to_owned()))
                    .collect(),
                cursor: None,
                cursor_hit: None,
                focused_global: focused.to_owned(),
                foreground_known: true,
                action_foreground: None,
                action_cached: None,
                action_outputs: Vec::new(),
                primary_output: None,
                inventory_complete: true,
                gesture_barrier: false,
                gesture: None,
                monotonic_ms: 0,
                frames: Vec::new(),
                member_attempts: Vec::new(),
                view_attempts: Vec::new(),
                clear_attempts: Vec::new(),
                creation_attempts: Vec::new(),
                held_attempts: Vec::new(),
                state_attempts: Vec::new(),
                focus_feedback: Vec::new(),
                presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
                intent_records: Vec::new(),
                renderer_input: None,
            }
        }
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let accepted =
            |engine: &mut WorldEngine, report: &WorldReportV4| match engine.handle_report(report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
        accepted(
            &mut engine,
            &report(
                1,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                "win-a",
            ),
        );
        accepted(
            &mut engine,
            &report(
                2,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                "win-c",
            ),
        );
        accepted(
            &mut engine,
            &report(
                5,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                "win-b",
            ),
        );
        // SKIP issuing/intermediate publications: apply ONLY the last
        // snapshot (close B baseline A) and prove target C exactly once.
        let closed = accepted(
            &mut engine,
            &report(
                7,
                vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
                vec!["win-b"],
                "win-a",
            ),
        );
        let req = closed.close_focus.clone().expect("close mints");
        assert_eq!(req.target.0, "win-c");
        assert_eq!(req.cause, CloseFocusCause::PositiveClose);
        // Real v4 byte-codec round-trip of the latest reply.
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                reply: closed.clone(),
                wire: tiler_protocol::world_protocol::ReplyWireOptionals {
                    suspended_output_present: false,
                },
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.close_focus, Some(req.clone()));
        // Duplicate latest replays no new close spend core-side is covered
        // by the sequencer tests; here the mock actuator proves one attempt
        // total for the skipped sequence.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        // Map the genuine core target onto the adapter mock target: the
        // production consume path below is the same mock-actuator leg the
        // skipped sequence would have driven (one attempt, no replay).
        let mut mock_reply = close_reply_for(&session, &req.id, req.issued_revision, 9);
        mock_reply.adapter_incarnation = session.incarnation().to_owned();
        mock_reply.core_generation = session.sequencer().bound_generation();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &mock_reply,
            None,
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &mock_reply,
            None,
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "duplicate latest no replay"
        );
    }
    struct FakeCloseBackend {
        fresh: Option<(u32, Option<u32>)>,
        post: Option<(u32, Option<u32>)>,
        reads: usize,
        dispatches: usize,
        completion: crate::runtime_types::AxCompletion,
    }

    impl crate::runtime_backend::AxBackend for FakeCloseBackend {
        fn apply_frame(
            &mut self,
            _cmd: &crate::runtime::FrameCommand,
        ) -> crate::runtime_types::AxCompletion {
            crate::runtime_types::AxCompletion::Failed {
                reason: "fake no frames".to_owned(),
            }
        }
        fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
            Ok(Vec::new())
        }
        fn selected_focus_evidence(&mut self) -> Option<(u32, Option<u32>)> {
            let value = if self.reads == 0 {
                self.fresh
            } else {
                self.post
            };
            self.reads += 1;
            value
        }
        fn dispatch_focus(
            &mut self,
            _cmd: &crate::runtime_types::FocusCommand,
        ) -> crate::runtime_types::AxCompletion {
            self.dispatches += 1;
            self.completion.clone()
        }
    }

    fn close_owner_cmd(
        session: &mut WorldSession,
        shared: &std::sync::Arc<std::sync::Mutex<Shared>>,
        slot: &std::sync::Arc<FocusSlot>,
        retained: &mut RetainedCore,
        focus_queue: &std::sync::Arc<std::sync::Mutex<Vec<FocusRequest>>>,
        reply: &WorldReplyV4,
    ) -> crate::runtime_types::FocusCommand {
        consume_close(session, retained, shared, focus_queue, reply, None);
        slot.take().expect("mock owner slot carries the attempt")
    }

    #[test]
    fn close_owner_mock_dispatch_accepted_stages_truthful() {
        use tiler_core::directional::WindowId as CoreWindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            CloseFocusCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
            ScreenReport, WindowFact, WorldReportV4,
        };
        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        fn screen(output: &str, x: i32) -> ScreenReport {
            ScreenReport {
                output: tiler_core::directional::OutputId(output.to_owned()),
                full_rect: Some(rect(x, 0, 800, 600)),
                work_area: Some(rect(x, 0, 800, 560)),
                complete: true,
                scale: 1.0,
                identity: tiler_core::world_report::OutputIdentity::unknown(),
            }
        }
        fn fact(window: &str, output: &str, x: i32) -> WindowFact {
            WindowFact {
                window: CoreWindowId(window.to_owned()),
                output: Some(tiler_core::directional::OutputId(output.to_owned())),
                rect: Some(rect(x, 0, 40, 90)),
                fullscreen: Some(false),
                maximized: None,
                sticky: None,
                hints: Some(WindowSizeHints::none()),
                minimized: Some(false),
                // Host-owned Space id, required for view membership.
                native_workspace: NativeWorkspaceFact::Value("space-11".to_owned()),
                host_hidden: Some(false),
                // Null per macOS caps: listed-unavailable facts plus
                // maximize/sticky (unsupported capabilities grade any
                // carried value ConflictingFact).
                privileged: None,
                managed: None,
                accepts_input: None,
                activation_inhibited: None,
                tool_window: None,
                owned: None,
                transient: None,
                modal: Some(false),
                resizable: Some(true),
                above: None,
                below: None,
                owner: None,
                parent: None,
                role: None,
                app_id: None,
                executable: None,
                process_id: Some("foreign-pid".to_owned()),
                class: None,
                title: None,
                window_type: Some(tiler_core::world_report::WindowType::Normal),
                activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                layer: Some(0),
                fullscreen_owner: None,
                fullscreen_exitable: None,
                suppression_available: None,
                visibility: tiler_core::world_report::VisibilityFact {
                    visible: Some(true),
                    suppression_owned: None,
                },
                last_write: None,
            }
        }
        fn report(
            incarnation: &str,
            generation: &str,
            seq: u64,
            windows: Vec<WindowFact>,
            closed: Vec<&str>,
            focused: &str,
        ) -> WorldReportV4 {
            WorldReportV4 {
                contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
                adapter_incarnation: incarnation.to_owned(),
                core_generation: generation.to_owned(),
                observation_seq: seq,
                request_seq: seq,
                request_id: format!("req-{seq}"),
                command_seq: None,
                command: None,
                screens: vec![screen("macos-display-1", 0)],
                held_screens: Vec::new(),
                windows,
                held_windows: Vec::new(),
                held_native: Vec::new(),
                // Lone display's current Space, mirroring production.
                native: vec![tiler_core::world_report::NativeObservation {
                    output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
                    native_order: vec!["space-11".to_owned()],
                    current_native: Some("space-11".to_owned()),
                    occupancy: vec![tiler_core::world_report::NativeOccupancy {
                        native: "space-11".to_owned(),
                        count: None,
                    }],
                }],
                closed: closed
                    .into_iter()
                    .map(|id| CoreWindowId(id.to_owned()))
                    .collect(),
                cursor: None,
                cursor_hit: None,
                focused_global: focused.to_owned(),
                foreground_known: true,
                action_foreground: None,
                action_cached: None,
                action_outputs: Vec::new(),
                primary_output: None,
                inventory_complete: true,
                gesture_barrier: false,
                gesture: None,
                monotonic_ms: 0,
                frames: Vec::new(),
                member_attempts: Vec::new(),
                view_attempts: Vec::new(),
                clear_attempts: Vec::new(),
                creation_attempts: Vec::new(),
                held_attempts: Vec::new(),
                state_attempts: Vec::new(),
                focus_feedback: Vec::new(),
                presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
                intent_records: Vec::new(),
                renderer_input: None,
            }
        }
        let mut session = focus_session();
        let incarnation = session.incarnation().to_owned();
        let generation = session.sequencer().bound_generation();
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            &generation,
            &incarnation,
            crate::world_connection::macos_capabilities(),
        )
        .expect("producer binds the consumer handshake");
        let accepted =
            |engine: &mut WorldEngine, report: &WorldReportV4| match engine.handle_report(report) {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            };
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                1,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                    fact("4242_9", "macos-display-1", 200),
                ],
                vec![],
                "4242_7",
            ),
        );
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                2,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                    fact("4242_9", "macos-display-1", 200),
                ],
                vec![],
                "4242_9",
            ),
        );
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                5,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                    fact("4242_9", "macos-display-1", 200),
                ],
                vec![],
                "4242_8",
            ),
        );
        let closed = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                7,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_9", "macos-display-1", 200),
                ],
                vec!["4242_8"],
                "4242_7",
            ),
        );
        let req = closed.close_focus.clone().expect("close mints");
        assert_eq!(req.target.0, "4242_9");
        assert_eq!(req.cause, CloseFocusCause::PositiveClose);
        assert_eq!(req.issued_revision, closed.state_revision);
        // Skipped-issuing proof: two further unchanged baseline reports
        // (same survivors, same host fallback) re-emit the identical
        // retained request core-side without ever touching the adapter.
        // Only the final retained publication encodes and applies below.
        let survivors = vec![
            fact("4242_7", "macos-display-1", 0),
            fact("4242_9", "macos-display-1", 200),
        ];
        for seq in [8u64, 9u64] {
            let retained = accepted(
                &mut engine,
                &report(
                    &incarnation,
                    &generation,
                    seq,
                    survivors.clone(),
                    vec![],
                    "4242_7",
                ),
            );
            assert_eq!(
                retained.close_focus,
                Some(req.clone()),
                "unchanged baseline retains the identical request"
            );
        }
        let latest = accepted(
            &mut engine,
            &report(&incarnation, &generation, 10, survivors, vec![], "4242_7"),
        );
        assert_eq!(
            latest.close_focus,
            Some(req.clone()),
            "final publication still carries the issuing request"
        );
        assert!(
            latest.state_revision > closed.state_revision,
            "applied publication is newer than the issuing publication"
        );
        assert_eq!(latest.based_on_observation_seq, 10);
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                reply: latest.clone(),
                wire: tiler_protocol::world_protocol::ReplyWireOptionals {
                    suspended_output_present: false,
                },
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.close_focus, Some(req.clone()));
        // Adapter leg for the genuine close through the matching bound
        // producer/consumer: the session above minted the request, so its
        // incarnation/generation already name the handshake; the final
        // retained publication above is the unchanged genuine core row
        // (codec round-tripped), applied through the production consume
        // path. The issuing (seq 7) and intermediate (seq 8, 9)
        // publications never touch the adapter sequencer, so exactly one
        // outbox row and one mock slot command prove latest-only.
        assert_eq!(back.reply.adapter_incarnation, incarnation);
        assert_eq!(back.reply.core_generation, generation);
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        retained.window_domains.insert(
            req.target.0.clone(),
            tiler_core::session::DomainKey {
                output: req.output.clone(),
                workspace: tiler_core::directional::WorkspaceId(req.workspace.clone()),
            },
        );
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &back.reply,
            None,
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1, "latest applies once");
        let cmd = slot.take().expect("mock owner slot carries the attempt");
        assert_eq!(cmd.window, format!("{CLOSE_PID}_9"));
        let close = cmd.close.clone().expect("typed close");
        assert_eq!(close.fallback_pre, Some(close_target(7)));
        let revision = lock_shared(&shared).topology_revision;
        // Production owner route with a scripted fake: fresh names the
        // bind-time baseline, post names the target, backend reports
        // Applied. The counted mock call proves the dispatch ran.
        let mut fake = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 3 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut fake, &cmd, &close, CLOSE_PID, CLOSE_GEN, revision, &shared,
        );
        assert_eq!(
            fake.dispatches, 1,
            "production owner must call dispatch_focus once"
        );
        assert_eq!(fake.reads, 2, "fresh baseline plus post read");
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                setter,
                reason,
                pre,
                post,
                foreground_proven,
                attempt_ms,
                close_id,
                target,
                issued_rev,
                ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::Accepted
                );
                assert_eq!(*reason, tiler_core::world_report::VisibilityReason::Empty);
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(*post, Some(close_target(9)));
                assert!(*foreground_proven);
                assert_eq!(*attempt_ms, 3);
                assert_eq!(close_id, &req.id);
                assert_eq!(*issued_rev, req.issued_revision);
                assert_eq!(*target, close_target(9));
            }
            other => panic!("expected CloseFocusResult"),
        }
        let inbox = Arc::new(Inbox::new(16));
        crate::runtime_thread::process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let staged = session.peek_close_feedback().expect("accepted stages");
        assert_eq!(staged.id, req.id);
        assert_eq!(
            staged.setter,
            tiler_core::world_report::VisibilitySetter::Accepted
        );
        assert_eq!(
            staged.reason,
            tiler_core::world_report::VisibilityReason::Empty
        );
        assert_eq!(staged.pre, Some(close_target(7)));
        assert_eq!(staged.post, Some(close_target(9)));
        // Duplicate latest replays nothing new: the production dispatcher
        // spends the envelope once (see `dispatch_close_focus`
        // `DuplicateSpent`), so no second slot command, no second owner
        // dispatch, and the first staged row keeps its stamp.
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &back.reply,
            None,
        );
        assert!(
            slot.take().is_none(),
            "duplicate latest carries no new command"
        );
        assert_eq!(
            fake.dispatches, 1,
            "no second dispatch without a new command"
        );
        assert_eq!(
            session.peek_close_feedback().expect("first wins").id,
            req.id
        );
        // Own-success next observation: the target reports as the truthful
        // global focus next, so the core retires the lane (no new request)
        // and the production consume of that null publication retires the
        // adapter epoch too.
        let satisfied = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                11,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_9", "macos-display-1", 200),
                ],
                vec![],
                "4242_9",
            ),
        );
        assert_eq!(
            satisfied.close_focus, None,
            "target equality retires the lane"
        );
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &satisfied,
            None,
        );
        assert!(
            lock_shared(&shared).close_focus_current.is_none(),
            "null publication retires the adapter epoch"
        );
        // FAILED-to-focus honesty through the matching bound session: the
        // session's own engine mints the genuine request, the unchanged
        // reply applies through the production consume path, and the backend
        // reports Applied yet the post read still names the baseline
        // (nothing moved). The raw row still stages Accepted with the
        // unchanged post; the unchanged observation plus that truthful row
        // retains the same id core-side, and the latest reemission through
        // the production dispatcher queues nothing and sets nothing twice.
        let mut session2 = focus_session();
        let incarnation2 = session2.incarnation().to_owned();
        let generation2 = session2.sequencer().bound_generation();
        let windows_full = vec![
            fact("4242_7", "macos-display-1", 0),
            fact("4242_8", "macos-display-1", 100),
            fact("4242_9", "macos-display-1", 200),
        ];
        let survivors2 = vec![
            fact("4242_7", "macos-display-1", 0),
            fact("4242_9", "macos-display-1", 200),
        ];
        for (seq, windows, closed_ids, focused) in [
            (1u64, windows_full.clone(), Vec::<&str>::new(), "4242_7"),
            (2u64, windows_full.clone(), Vec::<&str>::new(), "4242_9"),
            (5u64, windows_full, Vec::<&str>::new(), "4242_8"),
        ] {
            let rep = report(
                &incarnation2,
                &generation2,
                seq,
                windows,
                closed_ids,
                focused,
            );
            match session2.engine_mut().handle_report(&rep) {
                WorldOutcome::Accepted(_) => {}
                other => panic!("expected accepted, got {other:?}"),
            }
        }
        let close_rep = report(
            &incarnation2,
            &generation2,
            7,
            survivors2.clone(),
            vec!["4242_8"],
            "4242_7",
        );
        let closed2 = match session2.engine_mut().handle_report(&close_rep) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let req2 = closed2.close_focus.clone().expect("close mints");
        assert_eq!(req2.target.0, "4242_9");
        let wire2 = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                reply: closed2.clone(),
                wire: tiler_protocol::world_protocol::ReplyWireOptionals {
                    suspended_output_present: false,
                },
            },
        )
        .expect("latest reply encodes");
        let back2 =
            tiler_protocol::world_protocol::decode_reply_v4(&wire2).expect("latest decodes");
        assert_eq!(back2.reply.close_focus, Some(req2.clone()));
        let shared2 = focus_shared();
        let slot2 = live_focus_app(&shared2, CLOSE_PID, CLOSE_GEN);
        let mut retained2 = close_retained(9, Some(close_target(7)), true);
        retained2.window_domains.insert(
            req2.target.0.clone(),
            tiler_core::session::DomainKey {
                output: req2.output.clone(),
                workspace: tiler_core::directional::WorkspaceId(req2.workspace.clone()),
            },
        );
        let focus_queue2 = Arc::new(Mutex::new(Vec::new()));
        consume_close(
            &mut session2,
            &mut retained2,
            &shared2,
            &focus_queue2,
            &back2.reply,
            None,
        );
        assert_eq!(lock_focus(&focus_queue2).len(), 1);
        let cmd2 = slot2.take().expect("mock owner slot carries the attempt");
        let close2 = cmd2.close.clone().expect("typed close");
        let revision2 = lock_shared(&shared2).topology_revision;
        let mut stuck = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(7))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 3 },
        };
        let job2 = crate::runtime_worker::handle_close_focus_command(
            &mut stuck, &cmd2, &close2, CLOSE_PID, CLOSE_GEN, revision2, &shared2,
        );
        assert_eq!(stuck.dispatches, 1);
        match &job2 {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                setter, pre, post, ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::Accepted
                );
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(
                    *post,
                    Some(close_target(7)),
                    "post unchanged baseline stays honest"
                );
            }
            other => panic!("expected CloseFocusResult"),
        }
        let inbox2 = Arc::new(Inbox::new(16));
        crate::runtime_thread::process_job(
            job2,
            &mut session2,
            &mut retained2,
            &shared2,
            &inbox2,
            &focus_queue2,
            8,
        );
        let first = session2
            .peek_close_feedback()
            .expect("stuck applied still stages")
            .clone();
        assert_eq!(first.post, Some(close_target(7)));
        // Unchanged observation plus the truthful failed row retains the
        // same id core-side (Accepted disagreement stays unproven, never a
        // terminal refusal).
        let mut retain_rep = report(
            &incarnation2,
            &generation2,
            8,
            survivors2,
            Vec::new(),
            "4242_7",
        );
        retain_rep.focus_feedback = vec![first.clone()];
        let retained_outcome = match session2.engine_mut().handle_report(&retain_rep) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        assert_eq!(
            retained_outcome.close_focus,
            Some(req2.clone()),
            "truthful failed row retains the same id"
        );
        // Latest reemission through the production dispatcher spends
        // nothing new (`DuplicateSpent` in `dispatch_close_focus`): no new
        // slot command, no queued row, no second backend call, and the
        // first staged row keeps its stamp (no second setter).
        let queued_before = lock_focus(&focus_queue2).len();
        consume_close(
            &mut session2,
            &mut retained2,
            &shared2,
            &focus_queue2,
            &retained_outcome,
            None,
        );
        assert!(slot2.take().is_none(), "reemission carries no new command");
        assert_eq!(
            lock_focus(&focus_queue2).len(),
            queued_before,
            "reemission queues nothing new"
        );
        assert_eq!(
            stuck.dispatches, 1,
            "no second dispatch without a new command"
        );
        assert_eq!(
            session2.peek_close_feedback().expect("first wins").id,
            first.id,
            "same-id re-feed keeps the first row, no second setter"
        );
    }

    #[test]
    fn close_owner_mock_no_calls_cover_unproven_changed_superseded() {
        // Unknown baseline performs no call: fresh unreadable refuses as
        // FocusUnproven with zero dispatches.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut reply = close_reply_for(&session, "close-focus-7", 7, 9);
        reply.adapter_incarnation = session.incarnation().to_owned();
        reply.core_generation = session.sequencer().bound_generation();
        let cmd = close_owner_cmd(
            &mut session,
            &shared,
            &slot,
            &mut retained,
            &focus_queue,
            &reply,
        );
        let close = cmd.close.clone().expect("typed close");
        let revision = lock_shared(&shared).topology_revision;
        let mut unknown = FakeCloseBackend {
            fresh: None,
            post: None,
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut unknown,
            &cmd,
            &close,
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(unknown.dispatches, 0, "unproven performs no call");
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                setter,
                reason,
                pre,
                post,
                ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::NotCalled
                );
                assert_eq!(
                    *reason,
                    tiler_core::world_report::VisibilityReason::FocusUnproven
                );
                assert_eq!(*pre, Some(close_target(7)));
                assert!(post.is_none());
            }
            other => panic!("expected CloseFocusResult"),
        }
        // Changed baseline performs no call: a third window names
        // ForegroundChanged with zero dispatches.
        let mut session_c = focus_session();
        let shared_c = focus_shared();
        let slot_c = live_focus_app(&shared_c, CLOSE_PID, CLOSE_GEN);
        let mut retained_c = close_retained(9, Some(close_target(7)), true);
        let focus_queue_c = Arc::new(Mutex::new(Vec::new()));
        let mut reply_c = close_reply_for(&session_c, "close-focus-7", 7, 9);
        reply_c.adapter_incarnation = session_c.incarnation().to_owned();
        reply_c.core_generation = session_c.sequencer().bound_generation();
        let cmd_c = close_owner_cmd(
            &mut session_c,
            &shared_c,
            &slot_c,
            &mut retained_c,
            &focus_queue_c,
            &reply_c,
        );
        let close_c = cmd_c.close.clone().expect("typed close");
        let revision_c = lock_shared(&shared_c).topology_revision;
        let mut moved = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(5))),
            post: Some((CLOSE_PID, Some(5))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job_c = crate::runtime_worker::handle_close_focus_command(
            &mut moved, &cmd_c, &close_c, CLOSE_PID, CLOSE_GEN, revision_c, &shared_c,
        );
        assert_eq!(moved.dispatches, 0, "changed baseline performs no call");
        match &job_c {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                setter,
                reason,
                pre,
                post,
                ..
            } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::NotCalled
                );
                assert_eq!(
                    *reason,
                    tiler_core::world_report::VisibilityReason::ForegroundChanged
                );
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(*post, Some(close_target(5)));
            }
            other => panic!("expected CloseFocusResult"),
        }
        // Null epoch while taken cancels with zero dispatches.
        let mut session_n = focus_session();
        let shared_n = focus_shared();
        let slot_n = live_focus_app(&shared_n, CLOSE_PID, CLOSE_GEN);
        let mut retained_n = close_retained(9, Some(close_target(7)), true);
        let focus_queue_n = Arc::new(Mutex::new(Vec::new()));
        let mut reply_n = close_reply_for(&session_n, "close-focus-7", 7, 9);
        reply_n.adapter_incarnation = session_n.incarnation().to_owned();
        reply_n.core_generation = session_n.sequencer().bound_generation();
        let cmd_n = close_owner_cmd(
            &mut session_n,
            &shared_n,
            &slot_n,
            &mut retained_n,
            &focus_queue_n,
            &reply_n,
        );
        let close_n = cmd_n.close.clone().expect("typed close");
        let revision_n = lock_shared(&shared_n).topology_revision;
        crate::runtime_shared::publish_close_focus_current(&shared_n, None);
        let mut nulled = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job_n = crate::runtime_worker::handle_close_focus_command(
            &mut nulled,
            &cmd_n,
            &close_n,
            CLOSE_PID,
            CLOSE_GEN,
            revision_n,
            &shared_n,
        );
        assert_eq!(nulled.dispatches, 0, "null epoch cancels with no call");
        match &job_n {
            crate::runtime_thread::EngineJob::CloseFocusResult { setter, reason, .. } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::NotCalled
                );
                assert_eq!(
                    *reason,
                    tiler_core::world_report::VisibilityReason::Superseded
                );
            }
            other => panic!("expected CloseFocusResult"),
        }
        // Newer epoch while taken cancels with zero dispatches.
        let mut session_w = focus_session();
        let shared_w = focus_shared();
        let slot_w = live_focus_app(&shared_w, CLOSE_PID, CLOSE_GEN);
        let mut retained_w = close_retained(9, Some(close_target(7)), true);
        let focus_queue_w = Arc::new(Mutex::new(Vec::new()));
        let mut reply_w = close_reply_for(&session_w, "close-focus-7", 7, 9);
        reply_w.adapter_incarnation = session_w.incarnation().to_owned();
        reply_w.core_generation = session_w.sequencer().bound_generation();
        let cmd_w = close_owner_cmd(
            &mut session_w,
            &shared_w,
            &slot_w,
            &mut retained_w,
            &focus_queue_w,
            &reply_w,
        );
        let close_w = cmd_w.close.clone().expect("typed close");
        let revision_w = lock_shared(&shared_w).topology_revision;
        crate::runtime_shared::publish_close_focus_current(
            &shared_w,
            Some(crate::runtime_shared::CloseFocusEpoch {
                id: "close-focus-8".to_owned(),
                revision: 8,
                adapter_incarnation: session_w.incarnation().to_owned(),
                core_generation: session_w.sequencer().bound_generation(),
            }),
        );
        let mut newer = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job_w = crate::runtime_worker::handle_close_focus_command(
            &mut newer, &cmd_w, &close_w, CLOSE_PID, CLOSE_GEN, revision_w, &shared_w,
        );
        assert_eq!(newer.dispatches, 0, "newer epoch cancels with no call");
        match &job_w {
            crate::runtime_thread::EngineJob::CloseFocusResult { setter, reason, .. } => {
                assert_eq!(
                    *setter,
                    tiler_core::world_report::VisibilitySetter::NotCalled
                );
                assert_eq!(
                    *reason,
                    tiler_core::world_report::VisibilityReason::Superseded
                );
            }
            other => panic!("expected CloseFocusResult"),
        }
    }
}
