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

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{WindowId, WindowSubject};
use tiler_core::geometry::Rect;
use tiler_core::gesture::{GestureIngress, GestureSettled};
use tiler_core::world_engine::WorldOutcome;
#[cfg(test)]
use tiler_core::world_report::AttemptLaneV4;
use tiler_core::world_report::{
    FocusNoCallReason, ScopeAuthority, WorldCommand, WorldReplyV4, WorldReportV4,
};
#[cfg(test)]
use tiler_protocol::world_protocol::{
    CreationReceiptWire, CursorHitWire, DecodedReportV4, GestureIngressWire, ReportWireOptionals,
};

use crate::focus_route::FocusRoute;
use crate::runtime::{
    AppliedGeometryKey, DeferredSelectionFocus, DesiredFrame, FocusBaselines, FocusCommand,
    FocusRequest, FrameCommand, Shared, emit, lock_focus, lock_shared, parse_window_id,
};
use crate::runtime_core::RetainedCore;
use crate::runtime_thread::{CoherencePending, FocusSlot, FrameSlot, Inbox};
use crate::runtime_types::{CloseFocusBinding, ProvenArrivalStamp, TopologyMode};
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
        ("cursor", "accepted") => "cursor-accepted",
        ("cursor", "consumed") => "cursor-consumed",
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
        ("toggle-maximize", "accepted") => "toggle-maximize-accepted",
        ("toggle-maximize", "consumed") => "toggle-maximize-consumed",
        ("toggle-workspace-tiling", "accepted") => "toggle-workspace-tiling-accepted",
        ("toggle-workspace-tiling", "consumed") => "toggle-workspace-tiling-consumed",
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

/// Record-only half of the deferred fullscreen-float lane: an applied
/// `ToggleFloat` reply under retained fullscreen stages the pending
/// placement for later reveal. Used by the normal consume path and by the
/// post-accept early returns (`intent-write-held`, stale observation) where
/// core already applied the command but no frame may be emitted.
fn record_deferred_fullscreen_float_placement(
    session: &mut WorldSession,
    retained: &RetainedCore,
    reply: &WorldReplyV4,
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
) {
    // Only a ToggleFloat command applied by this report may create a
    // deferral. A floating row under fullscreen that arrived on an
    // observation-only reply (a normally floated window entering
    // fullscreen) must never be mistaken for an under-overlay toggle, so
    // token presence alone is not enough. Only the production adapter shape
    // (implicit current-focus subject) qualifies.
    let toggled_float = matches!(
        ticket_command,
        Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: WindowSubject::CurrentFocus,
            ..
        }))
    );
    // Correlation: the reply may also carry other floating lifetimes under
    // retained fullscreen with older outstanding placement tokens (e.g. a
    // normally floated window on another fullscreen display). Only the row
    // whose placement token was minted for this exact command may be
    // recorded.
    if toggled_float {
        let expected_operation = ticket_command_seq.map(command_operation_id);
        for row in &reply.windows {
            let placed_by_this_command = ticket_command_seq.is_some_and(|seq| {
                row.command_seq == Some(seq)
                    || expected_operation.as_deref() == row.frame_operation.as_deref()
            });
            if !placed_by_this_command
                || !row.floating
                || !is_retained_fullscreen(retained, &row.lifetime.0)
            {
                continue;
            }
            let (Some(operation), Some(target)) =
                (row.frame_operation.clone(), row.effective_frame)
            else {
                continue;
            };
            let Some((pid, _)) = parse_window_id(&row.lifetime.0) else {
                continue;
            };
            let Some(app) = retained.apps.get(&pid) else {
                continue;
            };
            session.pending_fullscreen_float_placements.insert(
                row.lifetime.0.clone(),
                crate::world_runtime::PendingFullscreenFloatPlacement {
                    operation,
                    target,
                    generation: app.generation,
                },
            );
        }
    }
}

/// Adapter-owned deferred reveal for one explicit float placement withheld
/// behind a retained native fullscreen overlay (D5: no writes, token
/// spends, or native state for a fullscreen subject). Shared core keeps
/// ordinary float rows `Untouched` and never spends their placement token,
/// so the adapter records the durable target on the withheld reply and
/// writes it exactly once on a later applied reply after the overlay
/// clears. The target is always the core-published durable frame, never
/// the fullscreen pose.
#[allow(clippy::too_many_arguments)]
fn stage_deferred_fullscreen_float_reveals(
    session: &mut WorldSession,
    retained: &RetainedCore,
    reply: &WorldReplyV4,
    current: &BTreeSet<(String, String)>,
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
    frames: &mut Vec<(String, Rect)>,
    frame_ops: &mut Vec<(String, String)>,
) {
    record_deferred_fullscreen_float_placement(
        session,
        retained,
        reply,
        ticket_command,
        ticket_command_seq,
    );
    if session.pending_fullscreen_float_placements.is_empty() {
        return;
    }
    // Reveal phase: each remembered placement either stays pending (still
    // withheld) or settles exactly once, never repeating a matching frame.
    let pending = std::mem::take(&mut session.pending_fullscreen_float_placements);
    for (window, stored) in pending {
        let Some((pid, wid)) = parse_window_id(&window) else {
            continue;
        };
        let Some(app) = retained.apps.get(&pid) else {
            continue;
        };
        if app.generation != stored.generation {
            continue;
        }
        // Lifetime clear: the adapter prunes closed lifetimes from the app's
        // observed windows, the minimized lifetimes, and the authoritative
        // roster; a lifetime retained in none of them is gone (the v4 reply
        // carries no close lane, so retained truth is the close signal).
        let lifetime_retained = app.windows.iter().any(|observed| observed.wid == wid)
            || retained.minimized.contains_key(&window)
            || app.roster.contains_key(&wid);
        if !lifetime_retained {
            continue;
        }
        let row = reply.windows.iter().find(|row| row.lifetime.0 == window);
        let Some(row) = row else {
            session
                .pending_fullscreen_float_placements
                .insert(window, stored);
            continue;
        };
        if row
            .frame_operation
            .as_deref()
            .is_some_and(|operation| operation != stored.operation)
        {
            // Another placement token supersedes this one.
            continue;
        }
        // Held/unreadable and minimized/host-hidden rows are republished by
        // core as non-floating `Hold(..)` rows (no effective frame). Those
        // withhold the placement like any veto; they must NOT be mistaken
        // for a real re-tile and clear the record. A scope hold withholds
        // every geometry write in the normal lane for the same reason.
        let row_hold = row.frame_authority.hold_reason().is_some();
        let scope_hold = reply.scopes.iter().any(|scope| {
            scope.authority.hold_reason().is_some()
                && scope.scope.covers_workspace(&row.output, &row.workspace)
        });
        let minimized_veto = retained
            .minimized
            .get(&window)
            .is_some_and(|record| record.minimized || record.host_hidden || record.sticky);
        let roster_veto = app
            .roster
            .get(&wid)
            .is_some_and(|entry| entry.minimized_meta || entry.hidden_meta);
        if row_hold || scope_hold || minimized_veto || roster_veto {
            session
                .pending_fullscreen_float_placements
                .insert(window, stored);
            continue;
        }
        // Only a hold-free non-floating row is a real re-tile that
        // supersedes the deferred placement.
        if !row.floating {
            continue;
        }
        if is_retained_fullscreen(retained, &window)
            || !current.contains(&(row.output.0.clone(), row.workspace.clone()))
        {
            session
                .pending_fullscreen_float_placements
                .insert(window, stored);
            continue;
        }
        let observed = app.windows.iter().find(|observed| observed.wid == wid);
        if observed.is_some_and(|observed| {
            Rect {
                x: observed.x,
                y: observed.y,
                w: observed.w,
                h: observed.h,
            } == stored.target
        }) {
            // Already at the durable placement: never repeat a matching
            // frame.
            continue;
        }
        if frames.iter().any(|(staged, _)| staged == &window) {
            // The normal lane already staged this lifetime (a Controlled
            // composed row): never write twice.
            continue;
        }
        match row.frame_operation.as_deref() {
            Some(operation) => {
                let target = row.effective_frame.unwrap_or(stored.target);
                if session.spend_frame_op(operation) {
                    frame_ops.push((window.clone(), operation.to_owned()));
                    frames.push((window, target));
                }
            }
            // Adapter-owned one-shot reveal: the core no longer carries the
            // token, so write the remembered durable target once.
            None => frames.push((window, stored.target)),
        }
    }
}

/// Canonical property-attempt lane order for one typed report: rows grouped
/// per lane in wire order (`frame`, `member`, `view`, `state`,
/// `visibility`), exactly the sequence the encoder consumes. Production
/// reports carry no held attempt rows, so the length equals the sum of the
/// five lane vecs.
#[cfg(test)]
fn property_attempt_order(report: &WorldReportV4) -> Vec<AttemptLaneV4> {
    let mut order = Vec::with_capacity(
        report.frames.len()
            + report.member_attempts.len()
            + report.view_attempts.len()
            + report.state_attempts.len()
            + report.visibility_attempts.len(),
    );
    let lanes = [
        (AttemptLaneV4::Frame, report.frames.len()),
        (AttemptLaneV4::Member, report.member_attempts.len()),
        (AttemptLaneV4::View, report.view_attempts.len()),
        (AttemptLaneV4::State, report.state_attempts.len()),
        (AttemptLaneV4::Visibility, report.visibility_attempts.len()),
    ];
    for (lane, len) in lanes {
        order.extend(std::iter::repeat_n(lane, len));
    }
    order
}

/// Test-only canonical envelope builder: the decoded-report metadata a wire
/// encode expects, shared with adapter tests so they exercise the genuine
/// codec on the envelope it really consumes instead of a hand-assembled
/// copy. Never a production dispatch fallback: production dispatches the
/// typed report straight into core admission. Wire metadata is inferred
/// canonically: absent cursor-hit/gesture stay absent, `monotonic_ms` rides
/// iff nonzero (attempt stamps preserved), attempt lanes ride iff non-empty,
/// and creation receipts keep `created` null unless the row carries causal
/// proof (`Some`).
#[cfg(test)]
pub(crate) fn decoded_report_for_encode(report: &WorldReportV4) -> DecodedReportV4 {
    DecodedReportV4 {
        report: report.clone(),
        wire: ReportWireOptionals {
            cursor_hit: match &report.cursor_hit {
                None => CursorHitWire::Absent,
                Some(_) => CursorHitWire::Id,
            },
            monotonic_ms_present: report.monotonic_ms != 0,
            creation_attempts_present: !report.creation_attempts.is_empty(),
            gesture_ingress: match &report.gesture {
                None => GestureIngressWire::Absent,
                Some(_) => GestureIngressWire::Present,
            },
        },
        property_attempt_order: property_attempt_order(report),
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
        // Production reports carry no held attempt rows, so no protocol
        // residual evidence exists to re-encode.
        residual: None,
    }
}

/// Production typed transact: the typed report enters core admission
/// directly ([`tiler_protocol::world_admission::handle_report_checked`]);
/// the adapter performs no report or reply JSON encode/decode on the
/// dispatch path. Core owns the request-size and section caps, typed shape
/// semantics, host gates, replay/staleness classification, reply semantic
/// and host-capability gates, and the exact canonical reply byte bound, so
/// every gate applies before sequencer acceptance or any actuation.
pub(crate) fn typed_transact(session: &mut WorldSession, report: &WorldReportV4) -> WorldOutcome {
    tiler_protocol::world_admission::handle_report_checked(session.engine_mut(), report, &[])
        .outcome
}

/// Adapter-side durable-intent gate: reconciliation persists any changed
/// `intent_desired` row before dependent native apply (see
/// [`WorldSession::reconcile_intent`]). `true` means the whole reply must be
/// held because the durable store could not be updated or the target process
/// is gone with no verifiable lifetime; the next accepted reply retries while
/// unequal. A live process with an unverifiable lifetime is skipped, not held.
/// There is no session-lifetime latch and no timer: a transient failure
/// recovers on the next accepted reply.
fn intent_write_hold(session: &mut WorldSession, reply: &WorldReplyV4) -> bool {
    session.reconcile_intent(reply)
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
/// and the incorporated terminal, if any. When an issuing reply is skipped,
/// the terminal remains available to the requester on a later applied reply.
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
    let gesture_id = ingress.id;
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
    let core_clock = crate::stage_timing::PhaseClock::start();
    let outcome = typed_transact(session, &report);
    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
        crate::stage_timing::record(
            shared,
            span.id,
            crate::stage_timing::stage::CORE,
            span.action,
            core_clock.elapsed(),
        );
    }
    match outcome {
        WorldOutcome::Accepted(reply) => match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => {
                // Core already processed `report.closed`: drain accepted
                // closes before any early return, so they never re-send.
                for id in &report.closed {
                    session.pending_closes.remove(id);
                }
                session.mark_intent_admitted(&report.intent_records);
                if intent_write_hold(session, &applied) {
                    count_token(shared, &format!("{op}-refused-intent-write-held"));
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "core-reply",
                        "intent-write-held",
                        0,
                    );
                    return (
                        DispatchVerdict::Refused {
                            token: "intent-write-held".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                }
                let Some(()) = prove_coherent(
                    retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
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
                let preview = map_gesture_preview(&applied, gesture_id);
                // A terminal belongs to the gesture this report carries:
                // a reply that still settles an older id is a late
                // drain, not this report's terminal.
                let settled = applied
                    .gesture_settled
                    .filter(|settled| settled.id == gesture_id);
                consume_world_reply(
                    session,
                    retained,
                    shared,
                    focus_queue,
                    focus_capacity,
                    &applied,
                    &report.native,
                    ticket.command.as_ref(),
                    ticket.command_seq,
                    op,
                    "accepted",
                    pid,
                    generation,
                    seq,
                    ticket.request_seq,
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
                // Core already processed `report.closed`: drain accepted
                // closes before any early return, so they never re-send.
                for id in &report.closed {
                    session.pending_closes.remove(id);
                }
                session.mark_intent_admitted(&report.intent_records);
                if intent_write_hold(session, &applied) {
                    count_token(shared, &format!("{op}-refused-intent-write-held"));
                    emit(
                        shared,
                        pid,
                        generation,
                        seq,
                        "core-reply",
                        "intent-write-held",
                        0,
                    );
                    return (
                        DispatchVerdict::Refused {
                            token: "intent-write-held".to_owned(),
                            select_operation: None,
                        },
                        PreviewOutcome::HideTransient { reason: "held" },
                        None,
                    );
                }
                let Some(()) = prove_coherent(
                    retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
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
                let preview = map_gesture_preview(&applied, gesture_id);
                // A terminal belongs to the gesture this report carries:
                // a reply that still settles an older id is a late
                // drain, not this report's terminal.
                let settled = applied
                    .gesture_settled
                    .filter(|settled| settled.id == gesture_id);
                consume_world_reply(
                    session,
                    retained,
                    shared,
                    focus_queue,
                    focus_capacity,
                    &applied,
                    &report.native,
                    ticket.command.as_ref(),
                    ticket.command_seq,
                    op,
                    "consumed",
                    pid,
                    generation,
                    seq,
                    ticket.request_seq,
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
                map_gesture_preview(&reply, gesture_id),
                reply
                    .gesture_settled
                    .filter(|settled| settled.id == gesture_id),
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

/// Stage-timing wrapper around [`dispatch_ticket_inner`]: reuses the
/// input-installed thread span or mints an engine-side one naming `op`,
/// then emits one whole-dispatch record plus, for planner-owned
/// select/send binds, one operation-bind record mapping the numeric
/// `op-<seq>` back to the trace id for drain-side joins. Disabled costs
/// flag loads only; behavior is unchanged (the verdict passes through).
#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_ticket(
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
    let traced = crate::stage_timing::ensure_span(op);
    let span = traced.as_ref().map(|held| held.0);
    let clock = crate::stage_timing::PhaseClock::start();
    let verdict = dispatch_ticket_inner(
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
        retry_allowed,
        op,
        pid,
        generation,
        seq,
    );
    if let Some(span) = span {
        let (outcome, bind) = match &verdict {
            DispatchVerdict::Applied { select_operation } => (
                crate::stage_timing::outcome::APPLIED,
                select_operation.as_deref(),
            ),
            DispatchVerdict::Duplicate => (crate::stage_timing::outcome::DUPLICATE, None),
            DispatchVerdict::Refused {
                select_operation, ..
            } => (
                crate::stage_timing::outcome::REFUSED,
                select_operation.as_deref(),
            ),
            DispatchVerdict::NoReport => (crate::stage_timing::outcome::NO_REPORT, None),
        };
        crate::stage_timing::record(
            shared,
            span.id,
            crate::stage_timing::stage::DISPATCH,
            outcome,
            clock.elapsed(),
        );
        if let Some(op_seq) = bind.and_then(crate::stage_timing::parse_op_seq) {
            crate::stage_timing::record(
                shared,
                span.id,
                crate::stage_timing::stage::OP_BIND,
                span.action,
                op_seq,
            );
            crate::stage_timing::note_op(op_seq, span.id, span.t0_us);
        }
    }
    verdict
}

#[allow(clippy::too_many_arguments)]
fn dispatch_ticket_inner(
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
    let report_clock = crate::stage_timing::PhaseClock::start();
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
    if crate::diag_cause::enabled() {
        let (unreadable_domains, skipped_outputs) = match &dispatch {
            DispatchContext::Native(ctx) => (
                ctx.domains.iter().filter(|d| !d.readable).count() as u64,
                ctx.skipped.len() as u64,
            ),
            DispatchContext::Synthetic => (0, 0),
        };
        let apps: Vec<(u32, bool, &str, u64)> = retained
            .apps
            .iter()
            .map(|(app_pid, app)| {
                (
                    *app_pid,
                    app.roster_incomplete,
                    app.roster_cause.as_str(),
                    app.roster.len() as u64,
                )
            })
            .collect();
        crate::diag_cause::emit_report_diag(
            shared,
            pid,
            generation,
            ticket.observation_seq,
            report.focused_global.is_empty(),
            report.foreground_known,
            report.inventory_complete,
            unreadable_domains,
            skipped_outputs,
            &apps,
        );
        crate::diag_cause::emit_close(
            shared,
            pid,
            generation,
            ticket.observation_seq,
            crate::diag_cause::close_stage::REPORT_SENT,
        );
        crate::diag_cause::emit_close_report(
            seq,
            ticket.observation_seq,
            ticket.request_seq,
            crate::stage_timing::current_id().unwrap_or(0),
            pid,
            generation,
        );
        eprintln!(
            "tiler-diag stage=report stamp_us={} dispatch={} obs={} req={} known={} foreground={} inventory_complete={} gap={}",
            crate::stage_timing::now_us(),
            seq,
            ticket.observation_seq,
            ticket.request_seq,
            u8::from(report.foreground_known),
            if report.focused_global.is_empty() {
                "-"
            } else {
                crate::diag_cause::diag_id(&report.focused_global)
            },
            u8::from(report.inventory_complete),
            u8::from(session.coverage_gap()),
        );
    }
    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
        crate::stage_timing::record(
            shared,
            span.id,
            crate::stage_timing::stage::REPORT,
            span.action,
            report_clock.elapsed(),
        );
    }
    // One core transaction per dispatch through typed ingress: the typed
    // report enters core admission directly (no adapter-side JSON
    // round-trip), where the core applies request/section caps, typed shape
    // semantics, host gates, replay/staleness, reply semantics/capabilities,
    // and the exact reply byte bound before sequencer acceptance or
    // actuation.
    let core_clock = crate::stage_timing::PhaseClock::start();
    let outcome = typed_transact(session, &report);
    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
        crate::stage_timing::record(
            shared,
            span.id,
            crate::stage_timing::stage::CORE,
            span.action,
            core_clock.elapsed(),
        );
    }
    match outcome {
        WorldOutcome::Accepted(reply) => {
            let accept_clock = crate::stage_timing::PhaseClock::start();
            match session.sequencer().accept_reply(&reply) {
                ReplyVerdict::Applied(applied) => {
                    // Core already processed `report.closed`: drain the
                    // accepted closes first, so every early return below
                    // (held intent write, emission refusal) never re-sends
                    // them on the next report.
                    for id in &report.closed {
                        session.pending_closes.remove(id);
                    }
                    session.mark_intent_admitted(&report.intent_records);
                    if intent_write_hold(session, &applied) {
                        count_token(shared, &format!("{op}-refused-intent-write-held"));
                        emit(
                            shared,
                            pid,
                            generation,
                            seq,
                            "core-reply",
                            "intent-write-held",
                            0,
                        );
                        // Core already applied the command; record a
                        // deferred fullscreen float placement even though
                        // this reply is emission-refused.
                        record_deferred_fullscreen_float_placement(
                            session,
                            retained,
                            &applied,
                            ticket.command.as_ref(),
                            ticket.command_seq,
                        );
                        return DispatchVerdict::Refused {
                            token: "intent-write-held".to_owned(),
                            select_operation: select_operation.clone(),
                        };
                    }
                    let Some(()) = prove_coherent(
                        retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
                    ) else {
                        // Core already applied the command; record a
                        // deferred fullscreen float placement even though
                        // emission is refused as stale.
                        record_deferred_fullscreen_float_placement(
                            session,
                            retained,
                            &applied,
                            ticket.command.as_ref(),
                            ticket.command_seq,
                        );
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
                    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
                        crate::stage_timing::record(
                            shared,
                            span.id,
                            crate::stage_timing::stage::ACCEPT,
                            span.action,
                            accept_clock.elapsed(),
                        );
                    }
                    let publish_clock = crate::stage_timing::PhaseClock::start();
                    consume_world_reply(
                        session,
                        retained,
                        shared,
                        focus_queue,
                        focus_capacity,
                        &applied,
                        &report.native,
                        ticket.command.as_ref(),
                        ticket.command_seq,
                        op,
                        "accepted",
                        pid,
                        generation,
                        seq,
                        ticket.request_seq,
                    );
                    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
                        crate::stage_timing::record(
                            shared,
                            span.id,
                            crate::stage_timing::stage::PUBLISH,
                            span.action,
                            publish_clock.elapsed(),
                        );
                        // Latest-publication identity for coalesced submit
                        // attribution (border/indicator submits below join
                        // through this, never through a per-action render
                        // claim). Coalesced: a later publication overwrites
                        // an earlier one before any submit; an obsolete
                        // publication may be skipped entirely.
                        crate::stage_timing::note_publish(span.id);
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
            // like an accept (the sequencer applies it as observation).
            match session.sequencer().accept_reply(&reply) {
                ReplyVerdict::Applied(applied) => {
                    // Core already processed `report.closed`: drain the
                    // accepted closes first, so every early return below
                    // (held intent write, emission refusal) never re-sends
                    // them on the next report.
                    for id in &report.closed {
                        session.pending_closes.remove(id);
                    }
                    session.mark_intent_admitted(&report.intent_records);
                    if intent_write_hold(session, &applied) {
                        count_token(shared, &format!("{op}-refused-intent-write-held"));
                        emit(
                            shared,
                            pid,
                            generation,
                            seq,
                            "core-reply",
                            "intent-write-held",
                            0,
                        );
                        // Core already applied the command; record a
                        // deferred fullscreen float placement even though
                        // this reply is emission-refused.
                        record_deferred_fullscreen_float_placement(
                            session,
                            retained,
                            &applied,
                            ticket.command.as_ref(),
                            ticket.command_seq,
                        );
                        return DispatchVerdict::Refused {
                            token: "intent-write-held".to_owned(),
                            select_operation: select_operation.clone(),
                        };
                    }
                    let Some(()) = prove_coherent(
                        retained, shared, inbox, &dispatch, &applied, op, pid, generation, seq,
                    ) else {
                        // Core already applied the command; record a
                        // deferred fullscreen float placement even though
                        // emission is refused as stale.
                        record_deferred_fullscreen_float_placement(
                            session,
                            retained,
                            &applied,
                            ticket.command.as_ref(),
                            ticket.command_seq,
                        );
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
                        &report.native,
                        ticket.command.as_ref(),
                        ticket.command_seq,
                        op,
                        "consumed",
                        pid,
                        generation,
                        seq,
                        ticket.request_seq,
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
/// any slot/outbox write):
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
/// coherence for one accepted reply. Returns `Some(())` when the snapshot is
/// still current; returns `None` (counted as `stale-observation`, emitting
/// nothing) when input landed between acceptance and emission — the queued
/// successors converge instead.
#[allow(clippy::too_many_arguments)]
fn prove_coherent(
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    inbox: &Arc<Inbox>,
    dispatch: &DispatchContext,
    applied: &WorldReplyV4,
    op: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
) -> Option<()> {
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
    // accepted. Nothing emits. See `emission_coherent`.
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
    Some(())
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

/// Publish the applied reply's outstanding native creation authorizations
/// for the adapter-facing read surface. `native` is the report's own native
/// order — the order the core minted a new desire against — and a desire
/// keeps the baseline of its first publication while it stays outstanding,
/// so a topology move after the mint never rewrites the authorization's
/// mint-time baseline. Desires absent from the reply (settled, pending, or
/// ambiguous) drop from the surface. Read-only: nothing here executes a
/// creation or changes settlement.
fn publish_native_creations(
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReplyV4,
    native: &[tiler_core::world_report::NativeObservation],
) {
    let mut guard = lock_shared(shared);
    let previous = std::mem::take(&mut guard.last_native_creations);
    let sid_order = |output: &tiler_core::directional::OutputId| -> Vec<u64> {
        native
            .iter()
            .find(|row| row.output == *output)
            .map(|row| {
                row.native_order
                    .iter()
                    .filter_map(|id| id.strip_prefix("space-")?.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    guard.last_native_creations = reply
        .native_creations
        .iter()
        .map(|desire| crate::runtime_types::NativeCreationRequest {
            operation_id: desire.operation_id.clone(),
            logical_workspace: desire.logical_workspace.clone(),
            target_output: desire.target_output.0.clone(),
            order_before: previous
                .iter()
                .find(|request| {
                    request.operation_id == desire.operation_id
                        && request.logical_workspace == desire.logical_workspace
                        && request.target_output == desire.target_output.0
                })
                .map(|request| request.order_before.clone())
                .unwrap_or_else(|| sid_order(&desire.target_output)),
        })
        .collect();
}

/// Apply the applied reply's retained switch-settlement projection and
/// publish one lifecycle receipt per newly applied terminal.
///
/// [`WorldSession::apply_switch_settlements`] owns the once-per-identity
/// rule: the projection is complete current state re-published on every
/// applied reply, so an exact replay is inert and a differing row for the
/// same output replaces the earlier applied snapshot (the retained
/// projection is authoritative). This wrapper only turns each genuine
/// application into receipts: `switch` carries the closed terminal
/// outcome, `switch-focus` the core-derived focus verdict, both counting
/// the frozen failed members and correlating to the settled parent
/// operation. Receipts are diagnostic: a settled switch never authorizes a
/// setter, unspends a dispatch, or releases an arrival barrier.
fn consume_switch_settlements(
    session: &mut WorldSession,
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReplyV4,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    session.apply_switch_settlements(&reply.switch_settled, |settlement| {
        let failed = settlement.failed.len() as u64;
        // Correlate the receipt to the settled parent operation; a parent
        // token with no numeric op falls back to this dispatch's sequence
        // (never a guessed identity).
        let parent = crate::stage_timing::parse_op_seq(&settlement.operation_id).unwrap_or(seq);
        emit(
            shared,
            pid,
            generation,
            parent,
            "switch",
            settlement.outcome.as_str(),
            failed,
        );
        emit(
            shared,
            pid,
            generation,
            parent,
            "switch-focus",
            settlement.focus.as_str(),
            failed,
        );
    });
}

/// Apply the applied reply's retained gesture terminal, publish one
/// lifecycle receipt, and publish the terminal for the adapter-local
/// requester.
///
/// [`WorldSession::apply_gesture_settlement`] owns the one-slot rule: the
/// field re-publishes on every applied reply, a same-id replay is inert
/// (never regressing identity or changing the original verdict), and only
/// a newly incorporated terminal applies. A terminal whose issuing reply
/// was skipped first lands on a later accepted reply, which is the only
/// publisher for the adapter-local requester. The shared publication lets a
/// main-thread drop whose issuing callback was skipped or dropped settle
/// from the exact terminal instead of a fabricated verdict. Receipts are
/// diagnostic: a settled gesture never authorizes a setter, unspends a
/// dispatch, or releases an arrival barrier. The ring's reference id
/// carries the settled gesture identity for correlation.
fn consume_gesture_settlement(
    session: &mut WorldSession,
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReplyV4,
    pid: u32,
    generation: u64,
) {
    let Some(applied) = session.apply_gesture_settlement(reply.gesture_settled.as_ref()) else {
        return;
    };
    let outcome = if applied.committed {
        "committed"
    } else {
        "uncommitted"
    };
    emit(shared, pid, generation, applied.id, "gesture", outcome, 0);
    crate::runtime_shared::publish_gesture_settlement(
        shared,
        crate::runtime_shared::PublishedGestureSettlement {
            id: applied.id,
            committed: applied.committed,
        },
    );
}

/// Apply the applied reply's retained operation-settlement projection and
/// publish one lifecycle receipt per newly applied terminal.
///
/// [`WorldSession::apply_operation_settlements`] owns the once-per-identity
/// rule: the projection is complete current state re-published on every
/// applied reply, so an exact replay is inert and a differing row for the
/// same `(window, kind)` replaces the earlier applied snapshot. This
/// wrapper only turns each genuine application into a receipt: the ring
/// kind names the phase (`operation-clear`/`operation-float`), the outcome
/// carries the closed terminal token, and the reference id is this
/// dispatch's sequence (no operation-id parsing, no adopted identity). The
/// narrow `cause` stays typed in the retained row (the ring carries static
/// tokens only, never eagerly formatted text). Receipts are diagnostic: a
/// settled operation never authorizes a setter, unspends a dispatch, or
/// releases an arrival barrier.
fn consume_operation_settlements(
    session: &mut WorldSession,
    shared: &Arc<Mutex<Shared>>,
    reply: &WorldReplyV4,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    session.apply_operation_settlements(&reply.operation_settled, |settlement| {
        let kind = match settlement.kind {
            tiler_core::world_report::OperationKind::Clear => "operation-clear",
            tiler_core::world_report::OperationKind::Float => "operation-float",
        };
        // Diagnostic reference id is the dispatch sequence: the settlement
        // note forbids parsing/adopting operation-id authority here, and a
        // receipt never needs the parent token to name its verdict.
        emit(
            shared,
            pid,
            generation,
            seq,
            kind,
            settlement.outcome.as_str(),
            0,
        );
    });
}

/// Consume one applied world reply: current-scope controlled frames into
/// per-app latest-wins slots, focus requests into the outbox plus the
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
    native: &[tiler_core::world_report::NativeObservation],
    ticket_command: Option<&WorldCommand>,
    ticket_command_seq: Option<u64>,
    op: &'static str,
    outcome: &'static str,
    pid: u32,
    generation: u64,
    seq: u64,
    ticket_request_seq: u64,
) {
    publish_native_creations(shared, reply, native);
    // Retained settlement projections: a fresh generation clears every
    // retained outcome before any row applies.
    if session.sync_settlement_generation(&reply.core_generation) {
        lock_shared(shared).gesture_settlement = None;
    }
    // Retained gesture terminal: applied once per exact identity across
    // every applied reply, so a terminal whose issuing reply was skipped
    // still lands (see `consume_gesture_settlement`).
    consume_gesture_settlement(session, shared, reply, pid, generation);
    // Retained operation settlement projection: applied once per exact
    // `(window, kind)` terminal (see `consume_operation_settlements`).
    consume_operation_settlements(session, shared, reply, pid, generation, seq);
    // Retained switch settlement projection: applied once per exact
    // terminal across every applied reply, so a verdict whose issuing
    // reply was skipped still lands (see `consume_switch_settlements`).
    consume_switch_settlements(session, shared, reply, pid, generation, seq);
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
    // Diagnostic-only hold identity: the ring scope index alone cannot
    // identify the domain, so held scopes also log explicit
    // output/workspace (native binding as bound, logical id as planned)
    // plus one cover line with the coverage that held them. All from
    // existing retained/native facts; unmapped reads `unavailable`, never
    // a guess.
    if crate::diag_cause::enabled() {
        let guard = lock_shared(shared);
        let native_ctx = guard.native_context.clone();
        drop(guard);
        let mut unreadable: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        if let Some(ctx) = native_ctx.as_ref() {
            for d in ctx.domains.iter().filter(|d| !d.readable) {
                unreadable.push(format!("{}:{}", d.output.0, d.workspace.0));
            }
            for s in &ctx.skipped {
                skipped.push(format!(
                    "{}:{}",
                    s.cg_id,
                    crate::diag_cause::diag_cause(&s.reason)
                ));
            }
        }
        let unreadable_s = if native_ctx.is_none() {
            "unavailable".to_owned()
        } else if unreadable.is_empty() {
            "none".to_owned()
        } else {
            unreadable.join(",")
        };
        let skipped_s = if native_ctx.is_none() {
            "unavailable".to_owned()
        } else if skipped.is_empty() {
            "none".to_owned()
        } else {
            skipped.join(",")
        };
        let mut roster: Vec<String> = Vec::new();
        let mut members_readable = 0u64;
        let mut members_unreadable = 0u64;
        for (app_pid, app) in retained.apps.iter() {
            if app.roster_incomplete {
                roster.push(format!(
                    "{app_pid}:{}:{}",
                    app.generation,
                    crate::diag_cause::roster_cause_token(&app.roster_cause)
                ));
            }
            for (wid, entry) in app.roster.iter().filter(|(_, e)| e.observed.is_none()) {
                members_unreadable += 1;
                let id = crate::runtime_types::window_id(*app_pid, *wid);
                let why = entry
                    .excluded
                    .as_ref()
                    .map(|r| r.log_token())
                    .unwrap_or("unreadable");
                let home = retained
                    .window_domains
                    .get(&id)
                    .map(|key| format!("{}:{}", key.output.0, key.workspace.0))
                    .unwrap_or_else(|| "unavailable".to_owned());
                roster.push(format!("{id}:{why} gen={} home={home}", app.generation));
            }
            members_readable += app
                .roster
                .iter()
                .filter(|(_, e)| e.observed.is_some())
                .count() as u64;
        }
        if roster.len() > 24 {
            let extra = roster.len() - 24;
            roster.truncate(24);
            roster.push(format!("+{extra}-more"));
        }
        let roster_s = if roster.is_empty() {
            "complete".to_owned()
        } else {
            roster.join(",")
        };
        let foreground = retained
            .observed_focus
            .as_ref()
            .map(|w| w.0.as_str())
            .unwrap_or("unavailable");
        crate::diag_cause::emit_hold_cover(
            &unreadable_s,
            &skipped_s,
            &roster_s,
            reply.based_on_observation_seq,
            seq,
            ticket_request_seq,
            session.coverage_gap(),
            foreground,
            members_readable,
            members_unreadable,
        );
    }
    for (index, scope) in reply.scopes.iter().enumerate() {
        if let Some(reason) = scope.authority.hold_reason() {
            emit(shared, pid, generation, seq, "scope", hold_token(reason), 0);
        }
        let authority = match scope.authority.hold_reason() {
            Some(reason) => hold_token(reason),
            None => match scope.authority {
                ScopeAuthority::Controlled => "controlled",
                ScopeAuthority::Untouched => "untouched",
                ScopeAuthority::Hold(_) => "held-unknown",
            },
        };
        crate::diag_cause::emit_reply_scope(shared, pid, generation, seq, authority, index as u64);
        if crate::diag_cause::enabled() && scope.authority.hold_reason().is_some() {
            let (out, ws) = match &scope.scope {
                tiler_core::world_report::AuthorityScope::Output(output) => {
                    (output.0.clone(), "unavailable".to_owned())
                }
                tiler_core::world_report::AuthorityScope::Workspace { output, workspace } => {
                    (output.0.clone(), workspace.0.clone())
                }
            };
            let (native, logical) = match &scope.scope {
                tiler_core::world_report::AuthorityScope::Output(output) => (
                    "unavailable".to_owned(),
                    reply
                        .outputs
                        .iter()
                        .find(|row| row.output == *output)
                        .and_then(|row| row.logical_view.clone())
                        .unwrap_or_else(|| "unavailable".to_owned()),
                ),
                tiler_core::world_report::AuthorityScope::Workspace { output, workspace } => (
                    reply
                        .workspaces
                        .iter()
                        .find(|row| row.output == *output && row.id == workspace.0)
                        .and_then(|row| row.native_binding.clone())
                        .unwrap_or_else(|| "unavailable".to_owned()),
                    workspace.0.clone(),
                ),
            };
            crate::diag_cause::emit_hold_scope(
                index as u64,
                &out,
                &ws,
                &native,
                &logical,
                authority,
                reply.based_on_observation_seq,
                seq,
            );
        }
    }
    crate::diag_cause::emit_close(
        shared,
        pid,
        generation,
        seq,
        crate::diag_cause::close_stage::REPLY_APPLIED,
    );
    crate::diag_cause::emit_close_reply(seq, reply.based_on_observation_seq, ticket_request_seq);
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
            if !session.spend_frame_op(operation) {
                continue;
            }
            frame_ops.push((row.lifetime.0.clone(), operation.to_string()));
        }
        frames.push((row.lifetime.0.clone(), frame));
    }
    // Deferred adapter reveal for explicit float placements withheld behind
    // a retained fullscreen overlay (shared core keeps those rows
    // `Untouched`, so the normal lane above cannot stage them).
    stage_deferred_fullscreen_float_reveals(
        session,
        retained,
        reply,
        &current,
        ticket_command,
        ticket_command_seq,
        &mut frames,
        &mut frame_ops,
    );
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
        // Compute the change facts BEFORE overwriting: a toggle reply
        // carries a changed current-workspace tiling mode (a desire row),
        // and a pager move changes the presentation. Either advances the
        // monotonic indicator revision so the menu-bar checkmark repaints
        // on this reply alone (an identical duplicate leaves it steady).
        let desires_changed = guard.last_workspace_desires != reply.workspaces;
        let pager_changed = guard.last_presentation.pager != reply.presentation.pager;
        guard.last_presentation = reply.presentation.clone();
        guard.last_pager_bindings = pager_bindings;
        guard.last_workspace_desires = reply.workspaces.clone();
        guard.last_core_generation = reply.core_generation.clone();
        guard.last_geometry_key = Some(geometry_key);
        if desires_changed || pager_changed {
            guard.indicator_revision = guard.indicator_revision.wrapping_add(1);
        }
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
    settle_select_transfers(session, shared, reply, pid, generation, seq);
    dispatch_close_focus(
        session,
        retained,
        shared,
        focus_queue,
        focus_capacity,
        reply,
        &current,
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
    crate::diag_cause::emit_batch_queued(
        frames.len() as u64,
        frame_ops.len() as u64,
        overlay_skipped,
        seq,
    );
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
        // Stage timing binds this worker `cmd_id` (per-app counter, with
        // the owning `pid`) back to the dispatch trace id so worker apply
        // records join without new plumbing.
        if let Some(span) = crate::stage_timing::with_current(|held| *held) {
            crate::stage_timing::record_for(
                shared,
                wpid,
                app_generation,
                cmd_id,
                crate::stage_timing::stage::FRAME_BIND,
                "frame",
                span.id,
            );
            crate::stage_timing::note_cmd(wpid, app_generation, cmd_id, span.id, span.t0_us);
        }
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

/// Retained requests whose focus outcome defers instead of dispatching:
/// exactly the selection-backed explicit-command cause carrying frozen
/// `afterShown` dependencies — the native selection must prove arrival on
/// the bound Space first. Every other cause (automatic admission,
/// focus-follows-cursor, positive close) actuates immediately, and a bare
/// explicit command without dependencies dispatches at once while
/// superseding any pending intent. Never the ticket: the frozen request
/// projection is the whole dependency.
#[cfg(test)]
pub(crate) fn request_binds_selection(req: &tiler_core::world_report::FocusRequest) -> bool {
    use tiler_core::world_report::FocusRequestCause;
    req.cause == FocusRequestCause::ExplicitCommand && req.after_shown.is_some()
}

/// Resolve the deferred selection focus for one selection-backed explicit
/// focus request: the request must carry its originating command sequence
/// plus frozen `afterShown` dependencies (the existing parent operation id,
/// the quoted issuing observation, members, views and selected/departing
/// scopes — a projection of existing selection state, never a new work id).
/// The arrival scope is exactly the request's frozen activation scope
/// (`req.output`/`req.workspace`, never `selected.first`): the reply's live
/// native binding for that scope must parse to a live Space id. The full
/// frozen projection rides into the deferred intent (never reconstructed
/// from later rows), alongside the immutable request tuple (focus id,
/// issuing revision, adapter incarnation, core generation). Every baseline
/// must be present (observed focus, foreground PID — never guessed).
/// `None` on any gap: refusal direction, never a guessed target or
/// baseline. The parent transfer may already have settled: nothing here
/// consults retained transfers, the current ticket, or latest window rows.
pub(crate) fn deferred_selection_focus_for_request(
    req: &tiler_core::world_report::FocusRequest,
    reply: &WorldReplyV4,
    adapter_incarnation: &str,
    core_generation: &str,
    route: FocusRoute,
    baselines: FocusBaselines,
) -> Option<DeferredSelectionFocus> {
    use tiler_core::world_report::FocusRequestCause;
    if req.cause != FocusRequestCause::ExplicitCommand {
        return None;
    }
    let command_seq = req.cmd_seq?;
    let after = req.after_shown.as_ref()?.clone();
    let native_binding = reply
        .workspaces
        .iter()
        .find(|row| row.output == req.output && row.id == req.workspace)
        .and_then(|row| row.native_binding.clone())?;
    let expected_sid = crate::workspace_select::parse_native_binding(native_binding.as_str())?;
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
        operation_id: after.op.clone(),
        command_seq,
        focus_id: req.id.clone(),
        issued_revision: req.issued_revision,
        adapter_incarnation: adapter_incarnation.to_owned(),
        core_generation: core_generation.to_owned(),
        window: req.target.clone(),
        output: req.output.clone(),
        workspace: req.workspace.clone(),
        after_shown: after,
        route,
        expected_sid,
        app_generation,
        prior_focus: Some(prior_focus),
        prior_frontmost: Some(prior_frontmost),
        proven_stamp: None,
        owner_queued: false,
    })
}

/// Outcome of one frozen arrival-set proof evaluation: `Proven` authorizes
/// the bound intent (plus an immediate release when the parent already
/// settled and the destination is currently arrived); `NotReady` binds the
/// intent and defers — absence is not proof of terminal ineligibility, so a
/// later same-id publication reevaluates; `Refused` spends into a safety
/// no-call under a pre-existing terminal rule on retained truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrozenProof {
    Proven,
    NotReady,
    Refused,
}

/// Sample the bounded proven-arrival stamp from live state: the issuing
/// observation watermark, topology revision, and native-context generation.
/// Sampled BEFORE the full frozen-member proof; carried only when still live
/// after the proof plus the live native mapping (`NotReady` binds stay
/// unstamped, and a moved epoch binds unstamped too — never a newer sample).
fn proven_stamp_now(shared: &Arc<Mutex<Shared>>) -> ProvenArrivalStamp {
    let guard = lock_shared(shared);
    ProvenArrivalStamp {
        obs: guard.clock.get(),
        revision: guard.topology_revision,
        context_generation: guard.native_cell.generation(),
    }
}

/// Recheck one proven-arrival stamp against live state. Shared lock held
/// by the caller.
pub(crate) fn proven_stamp_live(guard: &Shared, stamp: &ProvenArrivalStamp) -> bool {
    guard.clock.get() == stamp.obs
        && guard.topology_revision == stamp.revision
        && guard.native_cell.generation() == stamp.context_generation
}

/// Frozen arrival-set proof for one bound selection intent: every frozen
/// member must still home to its frozen scope, stay live/rostered/
/// unhidden/unexcluded, sit under a non-held workspace row, and — when the
/// frozen frame is non-null — match the latest roster-observed live frame
/// exactly (a null frozen frame waives equality, never readable geometry:
/// no observed rect at all still defers). The roster observed rect is the
/// identity-bound live frame source — never the reply's desired rows (no
/// `WindowDesire` output/workspace/frame reads: later rows are never
/// validity requirements) — but equality alone never proves arrival: it
/// only refuses nothing and defers on mismatch (staleness may resolve on a
/// later publication). Arrival itself is proven only by the select
/// completion's verified post plus the live native current/readable
/// recheck at release, or by a same-id latest reevaluation under those
/// same live legs. Frozen member scopes are core logical `(out, ws)`;
/// retained homing is native `(output, space-<sid>)`, so the home leg
/// translates each frozen scope through the reply's live workspace row
/// native binding (a current-reply live lookup, never a durable table): a
/// missing member binding defers (a later publication may carry it), never
/// refuses. The full frozen projection (members/views/selected/departing)
/// plus the issuing watermark rides into the intent; the proof consumes it.
/// Terminal (`Refused`) legs are pre-existing retained-truth rules only:
/// unparsable identity, retained home mismatch, retained minimization, or
/// roster minimized/hidden/excluded flags. Everything merely absent —
/// missing roster rows, missing bindings, missing/unreadable native
/// domains, missing native context, held scopes, departing homes, or a
/// reply watermark older than the issuing observation — defers (`NotReady`).
fn frozen_members_proof(
    shared: &Arc<Mutex<Shared>>,
    retained: &RetainedCore,
    reply: &WorldReplyV4,
    after: &tiler_core::world_report::AfterShown,
) -> FrozenProof {
    use tiler_core::directional::WorkspaceId;
    use tiler_core::session::DomainKey;
    // Ordering gate (necessary, never sufficient): the reply must
    // incorporate at least the issuing observation. An older base proves
    // nothing about the issue — defer for a newer publication.
    if reply.based_on_observation_seq < after.obs {
        return FrozenProof::NotReady;
    }
    let mut not_ready = false;
    for member in &after.members {
        let win_str = member.win.0.as_str();
        let Some((fpid, fwid)) = parse_window_id(win_str) else {
            return FrozenProof::Refused;
        };
        let Some(native_binding) = reply
            .workspaces
            .iter()
            .find(|ws| ws.output == member.out && ws.id == member.ws)
            .and_then(|ws| ws.native_binding.clone())
        else {
            not_ready = true;
            continue;
        };
        let frozen_home = DomainKey {
            output: member.out.clone(),
            workspace: WorkspaceId(native_binding.clone()),
        };
        if retained.window_domains.get(win_str) != Some(&frozen_home) {
            return FrozenProof::Refused;
        }
        if retained.minimized.contains_key(win_str) {
            return FrozenProof::Refused;
        }
        // A departing home never arrives: the member is leaving, not
        // showing — defer (a later publication settles the scope).
        if after
            .departing
            .iter()
            .any(|scope| scope.out == member.out && scope.ws == member.ws)
        {
            not_ready = true;
        }
        let Some(entry) = retained
            .apps
            .get(&fpid)
            .and_then(|app| app.roster.get(&fwid))
        else {
            // Absent roster proves nothing either way: the window may
            // admit on a later observation — defer, never refuse.
            not_ready = true;
            continue;
        };
        if entry.minimized_meta || entry.hidden_meta || entry.excluded.is_some() {
            return FrozenProof::Refused;
        }
        match (member.frame, entry.observed) {
            // A null frozen frame waives equality (readability is
            // satisfied by the observed `Some` here); an exact geometry
            // match proves the live window still carries the frozen frame.
            (None, Some(_)) => {}
            (Some(frozen), Some(live))
                if live.x == frozen.x
                    && live.y == frozen.y
                    && live.w == frozen.w
                    && live.h == frozen.h => {}
            // A mismatch or a missing live rect only proves not-ready (a
            // stale capture may resolve later), never refusal.
            _ => {
                not_ready = true;
            }
        }
        let held = reply
            .workspaces
            .iter()
            .find(|ws| ws.output == member.out && ws.id == member.ws)
            .is_some_and(|ws| ws.hold.is_some());
        if held {
            not_ready = true;
        }
        // Owner-captured arrival bracket gate: every frozen member needs a
        // current capture (exact tuple, watermark, revision, lifetime, no
        // invalidation) with shown before/after legs, a singleton home
        // matching this member's live binding SID, and a live frame (null
        // frozen frame still needs readable live geometry; exact frames
        // must match). One missing or stale capture defers, never refuses.
        {
            let guard = lock_shared(shared);
            let tuple = reply.focus_request.as_ref().map(|req| {
                (
                    req.id.clone(),
                    req.issued_revision,
                    reply.adapter_incarnation.clone(),
                    reply.core_generation.clone(),
                )
            });
            let Some((fid, frev, ainc, cg)) = tuple else {
                not_ready = true;
                continue;
            };
            let live_sid = crate::workspace_select::parse_native_binding(native_binding.as_str());
            let Some(cap) = guard.arrival_captures.get(win_str) else {
                not_ready = true;
                continue;
            };
            if !arrival_capture_current(&guard, cap, &fid, frev, &ainc, &cg) {
                not_ready = true;
                continue;
            }
            if cap.obs < after.obs {
                not_ready = true;
                continue;
            }
            let shown = |s: &tiler_core::world_report::VisibilitySample| {
                s.visibility.visible == Some(true) && s.lifetime_matched == Some(true)
            };
            if !shown(&cap.before) || !shown(&cap.after) {
                not_ready = true;
                continue;
            }
            if live_sid.is_none_or(|sid| cap.home_sid != Some(sid)) {
                not_ready = true;
                continue;
            }
            // Live domain output must still name this member's frozen
            // output under the live revision: a Space that moved display
            // since the bind proves nothing about this scope — defer.
            // (The capture home alone never proves the display leg.)
            if live_sid.is_none_or(|sid| {
                guard.native_context.as_ref().is_none_or(|live| {
                    live.revision != guard.topology_revision
                        || !live
                            .domains
                            .iter()
                            .any(|d| d.space == sid && d.output == member.out && d.readable)
                })
            }) {
                not_ready = true;
                continue;
            }
            match (member.frame, cap.frame) {
                (None, Some(_)) => {}
                (Some(frozen), Some(live))
                    if live.x == frozen.x
                        && live.y == frozen.y
                        && live.w == frozen.w
                        && live.h == frozen.h => {}
                _ => {
                    not_ready = true;
                }
            }
        }
    }
    // Binding-time native-view legs (existence only, never currency):
    // every frozen views/selected scope must resolve through the reply's
    // live native binding to a readable native domain under the live
    // topology revision. Anything missing or unreadable — including an
    // absent native context — defers, never refuses.
    {
        let guard = lock_shared(shared);
        let ctx = guard.native_context.clone();
        let revision = guard.topology_revision;
        let scope_ready = |scope: &tiler_core::world_report::AfterShownScope| -> bool {
            let Some(binding) = reply
                .workspaces
                .iter()
                .find(|ws| ws.output == scope.out && ws.id == scope.ws)
                .and_then(|ws| ws.native_binding.clone())
            else {
                return false;
            };
            let Some(sid) = crate::workspace_select::parse_native_binding(binding.as_str()) else {
                return false;
            };
            ctx.as_ref().is_some_and(|live| {
                live.revision == revision
                    && live
                        .domains
                        .iter()
                        .any(|d| d.space == sid && d.readable && d.output == scope.out)
            })
        };
        if !after.views.iter().all(&scope_ready) || !after.selected.iter().all(&scope_ready) {
            not_ready = true;
        }
    }
    if not_ready {
        FrozenProof::NotReady
    } else {
        FrozenProof::Proven
    }
}

/// Live native-view arrival proof for one expected Space: when a native
/// context exists, the expected Space must name a current, readable domain
/// under the live topology revision. An absent context proves nothing
/// (never a waiver): the bind defers and the release drops counted instead
/// of dispatching on unproven arrival.
fn native_arrival_ready(shared: &Arc<Mutex<Shared>>, expected_sid: u64) -> bool {
    let guard = lock_shared(shared);
    let (Some(ctx), revision) = (guard.native_context.clone(), guard.topology_revision) else {
        return false;
    };
    ctx.revision == revision
        && ctx
            .domains
            .iter()
            .any(|d| d.space == expected_sid && d.current && d.readable)
}

/// Bind-time native mapping proof for one expected Space: when a native
/// context exists, the expected Space must name a readable domain under
/// the live topology revision — existence only, never currency. The
/// selection has not arrived yet at dispatch time (that is the deferred
/// wait); currency is proven later at release. An absent context proves no
/// mapping (never a waiver): the bind defers instead of refusing or
/// releasing.
fn native_binding_ready(shared: &Arc<Mutex<Shared>>, expected_sid: u64) -> bool {
    let guard = lock_shared(shared);
    let (Some(ctx), revision) = (guard.native_context.clone(), guard.topology_revision) else {
        return false;
    };
    ctx.revision == revision
        && ctx
            .domains
            .iter()
            .any(|d| d.space == expected_sid && d.readable)
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

/// Bind the correlation-bound deferred selection focus for one retained
/// selection-backed explicit request (engine thread, at dispatch):
/// replaces any older pending intent (latest-wins supersession, counted).
/// Returns `(refusal, immediate_ready)`: `None` refusal on a bound intent
/// (proven or not-ready — both bind and defer), `Some(reason)` on a refusal
/// no-call. `immediate_ready` names a proven intent whose parent has
/// already settled while the frozen arrival set proves ready from this
/// latest reply — the caller releases it from the latest without waiting
/// for the old select callback/ticket. A merely not-ready proof still binds
/// the intent and defers (absence is never terminal): a later same-id
/// publication reevaluates and releases. Binds nothing when the reply
/// publishes no live native binding for the request's frozen activation
/// scope — without an arrival scope no intent can exist (refusal
/// direction). Bumps the supersession floor to the request's command
/// sequence so older completions can never reapply after this newer input.
fn bind_selection_focus_from_request(
    shared: &Arc<Mutex<Shared>>,
    session: &mut WorldSession,
    retained: &RetainedCore,
    reply: &WorldReplyV4,
    req: &tiler_core::world_report::FocusRequest,
    route: FocusRoute,
    observed_focus: Option<WindowId>,
) -> (Option<FocusNoCallReason>, bool) {
    if let Some(seq) = req.cmd_seq {
        let mut guard = lock_shared(shared);
        guard.selection_focus_floor_seq = guard.selection_focus_floor_seq.max(seq);
    }
    let Some(after) = req.after_shown.as_ref() else {
        drop_selection_focus_locked(shared, "superseded-no-after-shown");
        return (Some(FocusNoCallReason::ForegroundUnproven), false);
    };
    let Some((fpid, _fwid)) = parse_window_id(&req.target.0) else {
        drop_selection_focus_locked(shared, "unproven-target");
        return (Some(FocusNoCallReason::IdentityStale), false);
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
    // Frozen arrival scope is exactly the request scope (never
    // `selected.first`, so cross-output migrations resolve their own
    // destination): translated through the reply's live native bindings (a
    // current-reply live lookup, never a durable host table, never the
    // retained transfer rows: the parent transfer may already have
    // settled). The full frozen projection (members/obs/views/selected/
    // departing) rides into the intent; the proof below consumes it.
    let incarnation = reply.adapter_incarnation.clone();
    let core_gen = reply.core_generation.clone();
    let Some(mut deferred) =
        deferred_selection_focus_for_request(req, reply, &incarnation, &core_gen, route, baselines)
    else {
        drop_selection_focus_locked(shared, "unproven-baseline");
        return (Some(FocusNoCallReason::ForegroundUnproven), false);
    };
    // Epoch bracket: sample the original stamp BEFORE the whole proof and
    // native mapping evaluation; a moved epoch after the full proof binds
    // unstamped (never a newer sample).
    let original_stamp = proven_stamp_now(shared);
    match frozen_members_proof(shared, retained, reply, after) {
        FrozenProof::Refused => {
            drop_selection_focus_locked(shared, "unproven-baseline");
            return (Some(FocusNoCallReason::ForegroundUnproven), false);
        }
        FrozenProof::NotReady => {
            bind_deferred_selection_focus(shared, &deferred);
            lock_shared(shared).native_cell.request_reread();
            return (None, false);
        }
        FrozenProof::Proven => {}
    }
    if !native_binding_ready(shared, deferred.expected_sid) {
        // Missing mapping or context defers a proven intent: the arrival
        // Space is unproven from this reply, so no immediate release — but
        // the bind stands unstamped for a later same-id reevaluation.
        bind_deferred_selection_focus(shared, &deferred);
        return (None, false);
    }
    {
        let guard = lock_shared(shared);
        if !proven_stamp_live(&guard, &original_stamp) {
            drop(guard);
            bind_deferred_selection_focus(shared, &deferred);
            lock_shared(shared).native_cell.request_reread();
            return (None, false);
        }
    }
    deferred.proven_stamp = Some(original_stamp);
    bind_deferred_selection_focus(shared, &deferred);
    // Immediate release from the latest when the parent has already settled
    // and this latest reply proves the frozen arrival set: no retained View
    // transfer for the parent op and no transfer row for it on this reply,
    // plus live native arrival (the destination already current). The bind
    // above already proved the frozen members and the native mapping, so
    // settlement plus arrival is the only remaining gate — the
    // skipped-issuing selection never deadlocks waiting for a callback
    // that already ran.
    let parent_settled = selection_parent_settled(session, reply, &deferred.operation_id);
    let arrived = native_arrival_ready(shared, deferred.expected_sid);
    (None, parent_settled && arrived)
}

/// Diagnostic outcome token for a deferred-selection bind refusal. The
/// binder yields no other no-call reasons, so these two tokens cover it.
fn deferred_refusal_token(reason: FocusNoCallReason) -> &'static str {
    match reason {
        FocusNoCallReason::IdentityStale => "deferred-refused-identity-stale",
        _ => "deferred-refused-focus-unproven",
    }
}

/// Latest-wins bind of one deferred selection intent (counted
/// supersession): replaces any older pending intent naming another
/// operation.
fn bind_deferred_selection_focus(shared: &Arc<Mutex<Shared>>, deferred: &DeferredSelectionFocus) {
    {
        let mut guard = lock_shared(shared);
        if guard
            .selection_focus_pending
            .as_ref()
            .is_some_and(|pending| pending.operation_id != deferred.operation_id)
        {
            guard.diagnostics.focus_dropped_full += 1;
            drop(guard);
            emit(shared, 0, 0, 0, "focus", "superseded-deferred", 1);
            let mut guard = lock_shared(shared);
            guard.selection_focus_pending = Some(deferred.clone());
            drop(guard);
        } else {
            guard.selection_focus_pending = Some(deferred.clone());
            drop(guard);
        }
        emit(shared, 0, 0, 0, "focus", "deferred-bound", 1);
    }
}

/// Whether one retained arrival capture is current for the named request
/// tuple: exact tuple match, live watermark and revision, live native-context
/// generation, live app lifetime, and no invalidation. Visibility, home, and
/// frame legs belong to the proof itself, not to currency. Shared lock held
/// by the caller.
pub(crate) fn arrival_capture_current(
    guard: &Shared,
    cap: &crate::runtime_types::ArrivalCapture,
    focus_id: &str,
    issued_revision: u64,
    adapter_incarnation: &str,
    core_generation: &str,
) -> bool {
    if cap.invalidated {
        return false;
    }
    if cap.focus_id != focus_id
        || cap.issued_revision != issued_revision
        || cap.adapter_incarnation != adapter_incarnation
        || cap.core_generation != core_generation
    {
        return false;
    }
    if cap.obs != guard.clock.get() || cap.revision != guard.topology_revision {
        return false;
    }
    if cap.context_generation != guard.native_cell.generation() {
        return false;
    }
    guard
        .apps
        .get(&cap.pid)
        .is_some_and(|record| record.generation == cap.generation && !record.terminated)
}

/// Engine-side completion for one owner arrival-capture notification:
/// validate the exact current request tuple, app lifetime, watermark, and
/// revision, retain the bounded per-member rows, then reevaluate the latest
/// same-id deferred intent from the sequencer's current reply. Never bumps
/// the clock, never reissues a report, never warms or skips publishing: the
/// reevaluation releases only through the existing live legs. Stale tuples,
/// dead lifetimes, moved watermarks, or drifted revisions retain nothing and
/// release nothing (safe defer); a missing current reply reevaluates
/// nothing. Bounded: at most the pending frozen members are retained; rows
/// for windows outside the pending set are dropped.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_arrival_capture(
    session: &mut WorldSession,
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    pid: u32,
    generation: u64,
    focus_id: &str,
    issued_revision: u64,
    adapter_incarnation: &str,
    core_generation: &str,
    obs: u64,
    revision: u64,
    context_generation: u64,
    captures: Vec<crate::runtime_types::ArrivalCapture>,
) {
    let live = {
        let guard = lock_shared(shared);
        (
            guard.clock.get(),
            guard.topology_revision,
            guard
                .apps
                .get(&pid)
                .map(|record| (record.generation, record.terminated)),
            guard.native_cell.generation(),
        )
    };
    let Some((live_generation, terminated)) = live.2 else {
        return;
    };
    if terminated || live_generation != generation {
        return;
    }
    if live.0 != obs || live.1 != revision {
        return;
    }
    if live.3 != context_generation {
        return;
    }
    // The pending intent must still name exactly this tuple; otherwise the
    // rows belong to superseded work and retain nothing.
    let member_set: std::collections::BTreeSet<String> = {
        let guard = lock_shared(shared);
        match guard.selection_focus_pending.as_ref() {
            Some(pending)
                if pending.focus_id == focus_id
                    && pending.issued_revision == issued_revision
                    && pending.adapter_incarnation == adapter_incarnation
                    && pending.core_generation == core_generation =>
            {
                pending
                    .after_shown
                    .members
                    .iter()
                    .map(|m| m.win.0.clone())
                    .collect()
            }
            _ => return,
        }
    };
    {
        let mut guard = lock_shared(shared);
        for cap in captures {
            if cap.pid != pid || cap.generation != generation {
                continue;
            }
            if cap.focus_id != focus_id
                || cap.issued_revision != issued_revision
                || cap.adapter_incarnation != adapter_incarnation
                || cap.core_generation != core_generation
            {
                continue;
            }
            if cap.obs != obs
                || cap.revision != revision
                || cap.context_generation != context_generation
                || cap.invalidated
            {
                continue;
            }
            if !member_set.contains(&cap.win) {
                continue;
            }
            guard.arrival_captures.insert(cap.win.clone(), cap);
        }
        // Bound the map to the pending frozen set: rows outside it drop.
        guard
            .arrival_captures
            .retain(|key, _| member_set.contains(key));
        guard.notify_test_waiters();
    }
    // Same-id latest reevaluation from the sequencer's current reply (never
    // a reissued report, never warmed or skipped publishing).
    let reply = session.sequencer().last_reply();
    if let Some(reply) = reply {
        reevaluate_selection_release(session, retained, shared, focus_queue, &reply);
    }
}

/// Same-id latest DEFER reevaluation: a spent selection id re-published
/// on a later publication re-proves the frozen arrival set against the
/// latest reply and the live native view, releasing only the still-pending
/// current deferred work — exactly once (the release takes the slot, so a
/// further re-publication finds nothing). A newer or null publication
/// cancels through the Fresh/Absent supersession legs, never here: a spent
/// publication naming another request is stale evidence about old work and
/// neither releases nor cancels the current intent. Every release fence
/// (epoch, supersession floor, live app generation, foreground stability,
/// live native arrival, outbox capacity) re-applies inside the release.
pub(crate) fn reevaluate_selection_release(
    session: &mut WorldSession,
    retained: &RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    reply: &WorldReplyV4,
) {
    let Some(req) = reply.focus_request.as_ref() else {
        return;
    };
    let (operation_id, expected_sid, after_shown) = {
        let guard = lock_shared(shared);
        let Some(pending) = guard.selection_focus_pending.as_ref() else {
            return;
        };
        if pending.focus_id != req.id {
            return;
        }
        (
            pending.operation_id.clone(),
            pending.expected_sid,
            pending.after_shown.clone(),
        )
    };
    // Epoch bracket: sample the original stamp BEFORE the whole proof and
    // native mapping evaluation; a moved epoch after the full proof leaves
    // the pending intent unstamped (never a newer sample).
    let original_stamp = proven_stamp_now(shared);
    if frozen_members_proof(shared, retained, reply, &after_shown) != FrozenProof::Proven {
        return;
    }
    if !native_binding_ready(shared, expected_sid) {
        return;
    }
    // Stamp the proven intent so a later select callback can release: the
    // stamp carries the original proof liveness (never reissued at release).
    // Stays even when the parent/arrival gates below withhold — those gate
    // only this latest-driven release, not a later callback. Requires the
    // original epoch still live after the full proof and mapping check.
    {
        let mut guard = lock_shared(shared);
        if !proven_stamp_live(&guard, &original_stamp) {
            return;
        }
        let Some(pending) = guard.selection_focus_pending.as_mut() else {
            return;
        };
        if pending.focus_id != req.id || pending.operation_id != operation_id {
            return;
        }
        pending.proven_stamp = Some(original_stamp);
    }
    if !selection_parent_settled(session, reply, &operation_id) {
        return;
    }
    if !native_arrival_ready(shared, expected_sid) {
        return;
    }
    let frontmost = lock_shared(shared)
        .focus_evidence
        .as_ref()
        .and_then(|evidence| evidence.frontmost_pid);
    release_selection_focus_after_arrival(
        shared,
        focus_queue,
        Some(&operation_id),
        Some(expected_sid),
        frontmost,
    );
}

/// Parent-settlement leg shared by the bind-time immediate release and the
/// same-id latest reevaluation: no retained View transfer for the parent op
/// and no transfer row for it on the latest reply.
fn selection_parent_settled(
    session: &mut WorldSession,
    reply: &WorldReplyV4,
    operation_id: &str,
) -> bool {
    !session
        .engine_mut()
        .retained_transfers()
        .into_iter()
        .any(|target| {
            target.operation_id == operation_id
                && matches!(target.kind, tiler_core::world_report::TransferKind::View)
        })
        && !reply
            .transfers
            .iter()
            .any(|target| target.operation_id == operation_id)
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
    /// Deferred intent kept without dispatch: the frozen proof is not yet
    /// proven (missing/stale stamp) — a later capture-driven reevaluation
    /// may still stamp and release. Never a terminal drop.
    Deferred,
}

/// Retire one deferred selection intent only when it still names exactly
/// the retired tuple: terminal pre-queue fences and post-queue ownership
/// loss clear through here, so a raced supersession naming newer work is
/// never clobbered and a cleared intent can never resume authority.
fn retire_selection_focus_exact(shared: &Arc<Mutex<Shared>>, retired: &DeferredSelectionFocus) {
    let mut guard = lock_shared(shared);
    if guard
        .selection_focus_pending
        .as_ref()
        .is_some_and(|pending| {
            pending.operation_id == retired.operation_id
                && pending.focus_id == retired.focus_id
                && pending.issued_revision == retired.issued_revision
                && pending.adapter_incarnation == retired.adapter_incarnation
                && pending.core_generation == retired.core_generation
                && pending.command_seq == retired.command_seq
                && pending.window == retired.window
        })
    {
        guard.selection_focus_pending = None;
    }
}

/// Remove exactly one outbox row by queued command identity: a stopped
/// owner unwinds only the row it just pushed, never another operation's
/// row via a blind pop. The match is the full triple
/// (`request_id`, `window`, `close_id`): the numeric id alone collides
/// across the selection and close lanes, so the core close id names the
/// exact row.
fn remove_focus_request_exact(
    queue: &mut Vec<FocusRequest>,
    request_id: u64,
    window: &str,
    close_id: &str,
) -> bool {
    if let Some(pos) = queue.iter().position(|row| {
        row.request_id == request_id && row.window == window && row.close_id == close_id
    }) {
        queue.remove(pos);
        true
    } else {
        false
    }
}

/// Retire every outbox row for a terminal owner completion: the terminal
/// `CloseFocusResult` (staged or stale-rejected alike) ends the command, so
/// no row naming its queued identity may keep occupying the bounded outbox.
/// The match is the full triple (`request_id`, `window`, `close_id`): the
/// numeric id alone collides across the selection lane (core command seq)
/// and the close lane (local focus seq), so a purged row's late terminal
/// result must never retire a live replacement row that reused the number.
/// Remove-all (not first-only): deferral recovery can re-queue the same
/// command identity twice, and both rows die with the single terminal
/// result. Feedback itself never rides the queue (it stages through
/// `pending_close_feedback`), so a taken-but-still-pending row removed here
/// loses nothing the core still needs; the result-handler removal for an
/// already-purged row is simply a no-op.
pub(crate) fn retire_focus_request_exact(
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    request_id: u64,
    window: &str,
    close_id: &str,
) {
    lock_focus(focus_queue).retain(|row| {
        !(row.request_id == request_id && row.window == window && row.close_id == close_id)
    });
}

/// Release one deferred selection focus after the native selection proved
/// arrival (main thread, on the matching select completion's verified
/// post, or synthesized from the latest reply when the parent already
/// settled): the post must name the bound native Space — an unreadable or
/// wrong-Space post drops the intent instead of dispatching early.
/// Freshness fences mirror the dispatch path: a moved epoch (null/newer
/// publication), a superseded seq, a dead/regenerated app, a user focus
/// move to another app, a stale live native view, a full outbox, or a
/// stopped owner slot all retire the exact still-current tuple counted
/// instead of dispatching stale — terminal failures never become retry, so
/// a cleared intent can never resume authority later. A missing/stale proof
/// stamp or an already-queued owner command defers without dispatch
/// (`Deferred` / `NoPending`): never a terminal no-call, so a later
/// capture-driven reevaluation can still stamp and release. The queue mark
/// is reserved under the exact tuple BEFORE pushing the outbox row, so a
/// raced duplicate or supersession between the fences and the mark releases
/// nothing and leaks no stale row; a stopped slot unwinds exactly the row
/// just pushed (never a blind pop). The release dispatches through
/// the unified epoch/accounting/raw-feedback route (`close: Some` built from
/// the immutable deferred tuple): the owner worker rechecks the epoch plus
/// the fallback baseline and the full frozen proof rows, then reports one
/// typed `CloseFocusResult` — or one internal `SelectionFocusDeferred`
/// when the owner pre-setter gate withholds (no feedback staged, never a
/// terminal no-call). At-most-once per queueing: the original pending stays
/// bound with `owner_queued` set once the owner command enqueues, so a
/// repeat completion reports `NoPending` instead of duplicating the command;
/// terminal owner completions clear only the matching pending, and deferral
/// reports reset the queue mark plus the stamp (deps and baselines retained)
/// so evidence refresh can queue the same unstarted work again.
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
    // Frozen-proof gate before queueing the owner command: the callback
    // never bypasses the proof. A missing/stale stamp keeps the intent for
    // capture-driven reevaluation (never a terminal drop); an already-queued
    // owner command reports no duplicate. Only a fresh proven stamp on
    // unqueued work may release once the unchanged fences below hold. The
    // original pending stays bound through the release (never taken here):
    // terminal owner completions clear it, deferral reports reset it.
    {
        let guard = lock_shared(shared);
        match guard.selection_focus_pending.as_ref() {
            Some(pending) if pending.operation_id == op => {
                let live = pending
                    .proven_stamp
                    .as_ref()
                    .is_some_and(|stamp| proven_stamp_live(&guard, stamp));
                if !live {
                    return SelectionFocusRelease::Deferred;
                }
                if pending.owner_queued {
                    return SelectionFocusRelease::NoPending;
                }
            }
            _ => return SelectionFocusRelease::NoPending,
        }
    }
    let deferred = {
        let guard = lock_shared(shared);
        match guard.selection_focus_pending.as_ref() {
            Some(pending) if pending.operation_id == op => pending.clone(),
            _ => return SelectionFocusRelease::NoPending,
        }
    };
    let drop_stale_exact = |token: &'static str, retired: &DeferredSelectionFocus| {
        // Tuple-exact terminal retire: only the still-current exact tuple is
        // cleared; a raced supersession naming newer work is preserved.
        let mut guard = lock_shared(shared);
        if guard
            .selection_focus_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.operation_id == retired.operation_id
                    && pending.focus_id == retired.focus_id
                    && pending.issued_revision == retired.issued_revision
                    && pending.adapter_incarnation == retired.adapter_incarnation
                    && pending.core_generation == retired.core_generation
                    && pending.command_seq == retired.command_seq
                    && pending.window == retired.window
            })
        {
            guard.selection_focus_pending = None;
        }
        guard.diagnostics.stale_dropped += 1;
        drop(guard);
        emit(shared, 0, 0, 0, "focus", token, 1);
        SelectionFocusRelease::Dropped
    };
    if deferred.command_seq < lock_shared(shared).selection_focus_floor_seq {
        return drop_stale_exact("superseded-release", &deferred);
    }
    if !crate::runtime_shared::close_focus_is_current(
        shared,
        &deferred.adapter_incarnation,
        &deferred.core_generation,
        &deferred.focus_id,
        deferred.issued_revision,
    ) {
        return drop_stale_exact("superseded-release", &deferred);
    }
    if post_sid != Some(deferred.expected_sid) {
        return drop_stale_exact("unproven-arrival", &deferred);
    }
    let Some((fpid, fwid)) = parse_window_id(&deferred.window.0) else {
        return drop_stale_exact("unproven-target", &deferred);
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
        return drop_stale_exact("stale-app", &deferred);
    };
    if terminated || Some(app_generation) != deferred.app_generation {
        return drop_stale_exact("stale-app", &deferred);
    }
    // Foreground stability since bind: the release-time foreground must
    // equal the bind-time foreground. A move in between proves user input
    // after the select and drops the intent — whether the user left the
    // target app or arrived from elsewhere. An unchanged foreground
    // proceeds even when it names another app (hotkey select from
    // elsewhere is the ordinary restore path, not an intervention).
    // Unknown on either side fails closed: without a stability baseline
    // the release could never prove non-intervention. Same-app window
    // moves under a stable foreground are invisible here; the unified
    // owner re-checks the fallback baseline fresh before dispatching.
    match (deferred.prior_frontmost, frontmost_pid) {
        (Some(before), Some(now)) if before == now => {}
        (Some(_), Some(_)) => return drop_stale_exact("user-focus-moved", &deferred),
        _ => return drop_stale_exact("unproven-foreground", &deferred),
    }
    // Live native-view recheck from the frozen projection: the bound
    // Space must still name a current, readable domain under the live
    // revision — an absent context proves nothing and drops counted. A
    // native move since the bind proves the arrival stale and drops
    // instead of dispatching to the wrong Space.
    if !native_arrival_ready(shared, deferred.expected_sid) {
        return drop_stale_exact("unproven-arrival", &deferred);
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
        close_id: deferred.focus_id.clone(),
        wid: fwid,
        window: deferred.window.0.clone(),
        dispatched: true,
        verified: false,
        topology_revision: revision,
    };
    // Reserve the queue mark under the exact tuple BEFORE pushing: only one
    // winner reserves, so a concurrent repeat completion reports NoPending
    // without duplicating the outbox row, and a raced supersession between
    // the fences above and this mark releases nothing and leaks no stale
    // row. The mark rides the still-bound pending (never taken here):
    // terminal owner completions clear it, deferral reports reset it.
    {
        let mut guard = lock_shared(shared);
        let current = guard
            .selection_focus_pending
            .as_ref()
            .is_some_and(|pending| {
                !pending.owner_queued
                    && pending.operation_id == op
                    && pending.focus_id == deferred.focus_id
                    && pending.issued_revision == deferred.issued_revision
                    && pending.adapter_incarnation == deferred.adapter_incarnation
                    && pending.core_generation == deferred.core_generation
                    && pending.command_seq == deferred.command_seq
                    && pending.window == deferred.window
            });
        if !current {
            return SelectionFocusRelease::NoPending;
        }
        if let Some(pending) = guard.selection_focus_pending.as_mut() {
            pending.owner_queued = true;
        }
    }
    {
        let mut queue = lock_focus(focus_queue);
        if queue.len() >= capacity {
            drop(queue);
            // Terminal backpressure drop: unwind the reserve and retire the
            // exact tuple so the wedged intent can never resume authority.
            retire_selection_focus_exact(shared, &deferred);
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, 0, 0, 0, "focus", "dropped-full", 1);
            return SelectionFocusRelease::Dropped;
        }
        // Latest-only per app: the slot holds one command per app, so an
        // older same-pid row is dead (its untaken command was overwritten by
        // this release) or dying (taken, its feedback rides the staged row,
        // never the queue, so the result-handler retirement for a purged row
        // is a harmless no-op). Purge after the strict capacity fence, so a
        // genuinely full outbox still spends into backpressure: only the
        // admitted replacement compacts its own superseded rows.
        queue.retain(|row| row.pid != fpid);
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
        use tiler_core::directional::WorkspaceId;
        use tiler_core::session::DomainKey;
        use tiler_core::world_report::FocusRequestCause;
        let binding = CloseFocusBinding {
            close_id: deferred.focus_id.clone(),
            target: deferred.window.clone(),
            issued_revision: deferred.issued_revision,
            cause: FocusRequestCause::ExplicitCommand,
            cmd_seq: Some(deferred.command_seq),
            after_shown: Some(deferred.after_shown.clone()),
            adapter_incarnation: deferred.adapter_incarnation.clone(),
            core_generation: deferred.core_generation.clone(),
            output: deferred.output.clone(),
            workspace: WorkspaceId(deferred.workspace.clone()),
            domain: DomainKey {
                output: deferred.output.clone(),
                workspace: WorkspaceId(deferred.workspace.clone()),
            },
            generation: app_generation,
            topology_revision: revision,
            fallback_pre: deferred
                .prior_focus
                .clone()
                .map(tiler_core::directional::WindowId),
            fallback_known: true,
            expected_sid: Some(deferred.expected_sid),
            proven_stamp: deferred.proven_stamp,
        };
        let cmd = FocusCommand {
            pid: fpid,
            generation: app_generation,
            request_id: deferred.command_seq,
            wid: fwid,
            window: deferred.window.0.clone(),
            revision,
            route: deferred.route,
            // Unified route: the fallback baseline above carries the
            // bind-time observed focus for the owner recheck; no legacy
            // prior gate.
            expect_prior: None,
            close: Some(binding),
        };
        if slot.set_latest(cmd).is_some() {
            lock_shared(shared).diagnostics.focus_dispatched += 1;
            // Join through the shared op map under the completion identity:
            // this release runs on the main thread (tile settle poll, or the
            // engine immediate-release path), so the engine dispatch span is
            // never reliably installed here - the op map carries the original
            // intake origin (trace plus `t0_us`), never a follow-up span. A
            // current-span fallback covers only the map-miss path.
            let joined = crate::stage_timing::parse_op_seq(op)
                .and_then(crate::stage_timing::trace_for_op)
                .map(|(trace, t0_us, _)| (trace, t0_us))
                .or_else(|| crate::stage_timing::with_current(|held| (held.id, held.t0_us)));
            if let Some((trace, t0_us)) = joined {
                crate::stage_timing::record_for(
                    shared,
                    fpid,
                    app_generation,
                    deferred.command_seq,
                    crate::stage_timing::stage::FOCUS_BIND,
                    "focus",
                    trace,
                );
                crate::stage_timing::note_focus(
                    fpid,
                    app_generation,
                    deferred.command_seq,
                    trace,
                    t0_us,
                );
            }
        } else {
            // Ownership lost after queueing: the worker is gone, so the
            // intent is terminal — unwind exactly the row just pushed
            // (never a blind pop that could steal another operation's row)
            // and retire the exact tuple so it can never resume authority
            // or retry behind a dead owner.
            remove_focus_request_exact(
                &mut lock_focus(focus_queue),
                deferred.command_seq,
                &deferred.window.0,
                &deferred.focus_id,
            );
            retire_selection_focus_exact(shared, &deferred);
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, 0, 0, 0, "focus", "dropped-full", 1);
            return SelectionFocusRelease::Dropped;
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

/// Retained-focus dispatch on the coherent LATEST reply lane for every
/// cause (explicit command, automatic admission, focus-follows-cursor,
/// positive close): observes the nullable retained envelope, publishes the
/// taken-but-unstarted epoch, and either attempts once or spends into a
/// safety no-call. Never fabricates `Accepted` on queueing: the attempt
/// stages nothing; the owner result path stages the raw row. Latest/null
/// epoch only authorizes: a null envelope retires and publishes `None`,
/// cancelling old taken-unstarted commands — command tickets never cancel
/// (admission actuates). Stale replays and spent re-emissions keep the
/// current epoch (a spent stable reemit never cancels the in-flight same
/// id). Selection-backed explicit requests (frozen `afterShown`) bind a
/// correlation-bound deferred intent released only after the native
/// selection proves arrival — never dispatched early, never reconstructed
/// from latest window rows. A bare explicit command dispatches at once
/// when safe and supersedes any pending deferred intent.
#[allow(clippy::too_many_arguments)]
fn dispatch_close_focus(
    session: &mut WorldSession,
    retained: &mut RetainedCore,
    shared: &Arc<Mutex<Shared>>,
    focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
    focus_capacity: usize,
    reply: &WorldReplyV4,
    current: &BTreeSet<(String, String)>,
    pid: u32,
    generation: u64,
    seq: u64,
) {
    use crate::runtime_shared::CloseFocusEpoch;
    use tiler_core::directional::WorkspaceId;
    use tiler_core::session::DomainKey;
    use tiler_core::world_report::{FocusDisposition, FocusFeedback, FocusRequestCause};

    let incarnation = reply.adapter_incarnation.clone();
    let core_gen = reply.core_generation.clone();
    let envelope = session.sequencer().observe_close_envelope(
        reply.focus_request.as_ref(),
        &incarnation,
        &core_gen,
    );
    match envelope {
        CloseEnvelope::Absent => {
            crate::runtime_shared::publish_close_focus_current(shared, None);
            // Latest/null cancels every cause's unstarted deferred
            // authority — not only bare explicit — so a later legacy
            // completion can never actuate old work.
            drop_selection_focus_locked(shared, "null-superseded");
            emit(shared, pid, generation, seq, "close-focus", "absent", 0);
            crate::diag_cause::emit_focus_diag(
                shared,
                0,
                0,
                seq,
                crate::diag_cause::kind::FOCUS_CLOSE,
                "envelope-absent",
                false,
                false,
                false,
            );
            crate::diag_cause::emit_focus_line(
                "unavailable",
                FocusRequestCause::PositiveClose,
                "unavailable",
                None,
                false,
                seq,
                "envelope-absent",
            );
            return;
        }
        CloseEnvelope::Stale => {
            emit(shared, pid, generation, seq, "close-focus", "stale", 0);
            let (id, cause, target) = match reply.focus_request.as_ref() {
                Some(req) => (req.id.as_str(), req.cause, req.target.0.as_str()),
                None => (
                    "unavailable",
                    FocusRequestCause::PositiveClose,
                    "unavailable",
                ),
            };
            crate::diag_cause::emit_focus_diag(
                shared,
                0,
                0,
                seq,
                crate::diag_cause::kind::FOCUS_CLOSE,
                "envelope-stale",
                false,
                false,
                false,
            );
            crate::diag_cause::emit_focus_line(
                id,
                cause,
                target,
                retained.observed_focus.as_ref().map(|w| w.0.as_str()),
                retained.observed_focus_known,
                seq,
                "envelope-stale",
            );
            return;
        }
        CloseEnvelope::DuplicateSpent => {
            // Same-id latest reevaluation: the spent selection id
            // re-published on a later publication re-proves the frozen
            // arrival set against the latest reply and the live native
            // view — a deferred intent whose parent settled and whose
            // destination arrived while its callback was skipped still
            // releases exactly once. Anything else stays deferred.
            reevaluate_selection_release(session, retained, shared, focus_queue, reply);
            emit(shared, pid, generation, seq, "close-focus", "duplicate", 0);
            let (id, cause, target) = match reply.focus_request.as_ref() {
                Some(req) => (req.id.as_str(), req.cause, req.target.0.as_str()),
                None => (
                    "unavailable",
                    FocusRequestCause::PositiveClose,
                    "unavailable",
                ),
            };
            crate::diag_cause::emit_focus_diag(
                shared,
                0,
                0,
                seq,
                crate::diag_cause::kind::FOCUS_CLOSE,
                "envelope-duplicate-spent",
                retained.observed_focus.is_some(),
                retained.observed_focus_known,
                false,
            );
            crate::diag_cause::emit_focus_line(
                id,
                cause,
                target,
                retained.observed_focus.as_ref().map(|w| w.0.as_str()),
                retained.observed_focus_known,
                seq,
                "envelope-duplicate-spent",
            );
            return;
        }
        CloseEnvelope::Fresh => {}
    }
    let req = reply.focus_request.as_ref().expect("Fresh implies Some");
    let target_str = req.target.0.clone();
    // Selection-backed explicit requests defer: bind the arrival-bound
    // intent from the frozen `afterShown` projection (the parent transfer
    // may already have settled), spend the envelope once, publish the
    // epoch, and return without dispatching — focus releases only after
    // the native selection proves arrival (or immediately from this latest
    // reply when the parent already settled). A refused bind spends into a
    // no-call row, never a retry.
    if req.cause == FocusRequestCause::ExplicitCommand && req.after_shown.is_some() {
        crate::runtime_shared::publish_close_focus_current(
            shared,
            Some(CloseFocusEpoch {
                id: req.id.clone(),
                revision: req.issued_revision,
                adapter_incarnation: incarnation.clone(),
                core_generation: core_gen.clone(),
            }),
        );
        let (refusal, immediate_ready) = bind_selection_focus_from_request(
            shared,
            session,
            retained,
            reply,
            req,
            session.focus_route(),
            retained.observed_focus.clone(),
        );
        session.sequencer().spend_close(&req.id);
        match refusal {
            None => {
                emit(shared, pid, generation, seq, "close-focus", "deferred", 1);
                {
                    let (tpid, _) = parse_window_id(&target_str).unwrap_or((0, 0));
                    let observed = retained.observed_focus.clone();
                    let is_target = observed.as_ref() == Some(&req.target);
                    crate::diag_cause::emit_focus_diag(
                        shared,
                        tpid,
                        0,
                        seq,
                        crate::diag_cause::focus_cause_kind(req.cause),
                        if immediate_ready {
                            "deferred-selection-ready"
                        } else {
                            "deferred-selection-pending"
                        },
                        observed.is_some(),
                        retained.observed_focus_known,
                        is_target,
                    );
                    crate::diag_cause::emit_focus_line(
                        &req.id,
                        req.cause,
                        &target_str,
                        observed.as_ref().map(|w| w.0.as_str()),
                        retained.observed_focus_known,
                        seq,
                        if immediate_ready {
                            "deferred-selection-ready"
                        } else {
                            "deferred-selection-pending"
                        },
                    );
                }
                if immediate_ready {
                    let (expected_sid, frontmost) = {
                        let guard = lock_shared(shared);
                        guard
                            .selection_focus_pending
                            .as_ref()
                            .map_or((None, None), |pending| {
                                (Some(pending.expected_sid), pending.prior_frontmost)
                            })
                    };
                    if let Some(expected_sid) = expected_sid {
                        let op = req.after_shown.as_ref().map(|after| after.op.clone());
                        release_selection_focus_after_arrival(
                            shared,
                            focus_queue,
                            op.as_deref(),
                            Some(expected_sid),
                            frontmost,
                        );
                    }
                }
            }
            Some(reason) => {
                {
                    let (tpid, _) = parse_window_id(&target_str).unwrap_or((0, 0));
                    let observed = retained.observed_focus.clone();
                    crate::diag_cause::emit_focus_diag(
                        shared,
                        tpid,
                        0,
                        seq,
                        crate::diag_cause::focus_cause_kind(req.cause),
                        deferred_refusal_token(reason),
                        observed.is_some(),
                        retained.observed_focus_known,
                        observed.as_ref() == Some(&req.target),
                    );
                    crate::diag_cause::emit_focus_line(
                        &req.id,
                        req.cause,
                        &target_str,
                        observed.as_ref().map(|w| w.0.as_str()),
                        retained.observed_focus_known,
                        seq,
                        deferred_refusal_token(reason),
                    );
                }
                session.stage_close_feedback(FocusFeedback {
                    id: req.id.clone(),
                    target: req.target.clone(),
                    revision: req.issued_revision,
                    disposition: FocusDisposition::NotCalled { reason },
                    pre: retained.observed_focus.clone(),
                    post: None,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    attempt_ms: 0,
                });
                lock_shared(shared).diagnostics.focus_dropped_full += 1;
                emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
            }
        }
        return;
    }
    // A bare explicit command supersedes any deferred selection focus:
    // never reapply a stale selection focus after newer user intent.
    // A newer non-explicit publication likewise cancels every cause's
    // unstarted deferred authority when it names a different request.
    if req.cause == FocusRequestCause::ExplicitCommand {
        drop_selection_focus_locked(shared, "explicit-focus-superseded");
        if let Some(cmd_seq) = req.cmd_seq {
            let mut guard = lock_shared(shared);
            guard.selection_focus_floor_seq = guard.selection_focus_floor_seq.max(cmd_seq);
        }
    } else {
        let stale = lock_shared(shared)
            .selection_focus_pending
            .as_ref()
            .is_some_and(|pending| pending.focus_id != req.id);
        if stale {
            drop_selection_focus_locked(shared, "explicit-focus-superseded");
        }
    }
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
    let fallback_pre = retained.observed_focus.clone();
    let fallback_known = retained.observed_focus_known;
    // Diagnostic-only route record: one ring line per exit below with the
    // exact predicate token, the request cause, the target app identity (0
    // when unparsed/unknown), and the retained observed-focus evidence
    // bits (present/known/equals-target). No fresh reads; the exact
    // (pid, wid) target rides the staged feedback row each exit stages.
    let diag =
        |shared: &Arc<Mutex<Shared>>, outcome: &'static str, target_pid: u32, app_gen: u64| {
            crate::diag_cause::emit_focus_diag(
                shared,
                target_pid,
                app_gen,
                seq,
                crate::diag_cause::focus_cause_kind(req.cause),
                outcome,
                fallback_pre.is_some(),
                fallback_known,
                fallback_pre.as_ref() == Some(&req.target),
            );
            crate::diag_cause::emit_focus_line(
                &req.id,
                req.cause,
                &target_str,
                fallback_pre.as_ref().map(|w| w.0.as_str()),
                fallback_known,
                seq,
                outcome,
            );
        };
    // Zero-setter satisfied: focus already names the target.
    if fallback_pre.as_ref() == Some(&req.target) {
        session.sequencer().spend_close(&req.id);
        session.stage_close_feedback(FocusFeedback {
            id: req.id.clone(),
            target: req.target.clone(),
            revision: req.issued_revision,
            disposition: FocusDisposition::NotCalled {
                reason: FocusNoCallReason::NoReason,
            },
            pre: Some(req.target.clone()),
            post: Some(req.target.clone()),
            foreground_proven: fallback_known,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        emit(shared, pid, generation, seq, "close-focus", "satisfied", 0);
        {
            let (tpid, _) = parse_window_id(&target_str).unwrap_or((0, 0));
            diag(shared, "already-satisfied", tpid, 0);
        }
        return;
    }
    // No-call helper: spends once, stages one raw row, counts the drop.
    let no_call = |session: &mut WorldSession,
                   shared: &Arc<Mutex<Shared>>,
                   reason: FocusNoCallReason,
                   outcome: &'static str,
                   target_pid: u32,
                   app_gen: u64| {
        session.sequencer().spend_close(&req.id);
        session.stage_close_feedback(FocusFeedback {
            id: req.id.clone(),
            target: req.target.clone(),
            revision: req.issued_revision,
            disposition: FocusDisposition::NotCalled { reason },
            pre: fallback_pre.clone(),
            post: None,
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        lock_shared(shared).diagnostics.focus_dropped_full += 1;
        emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
        diag(shared, outcome, target_pid, app_gen);
    };
    // Frozen-target fences over retained truth plus the live native plan.
    // The request names core-logical workspaces while native retained
    // domains and the native context are native-keyed, so resolve through
    // this reply's live `native_binding` row (the same live-lookup pattern
    // as `current` in `consume_world_reply`). Synthetic skeletons have no
    // native context, so their logical keys compare verbatim; an unmapped
    // native request keeps the logical id and refuses below: no durable
    // host table, never a guess.
    let domain = DomainKey {
        output: req.output.clone(),
        workspace: WorkspaceId(
            lock_shared(shared)
                .native_context
                .is_some()
                .then(|| {
                    reply
                        .workspaces
                        .iter()
                        .find(|row| row.output == req.output && row.id == req.workspace)
                        .and_then(|row| row.native_binding.clone())
                })
                .flatten()
                .unwrap_or_else(|| req.workspace.clone()),
        ),
    };
    let Some((fpid, fwid)) = parse_window_id(&target_str) else {
        no_call(
            session,
            shared,
            FocusNoCallReason::IdentityStale,
            "no-call-parse-identity-stale",
            0,
            0,
        );
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
        no_call(
            session,
            shared,
            FocusNoCallReason::IdentityStale,
            "no-call-app-identity-stale",
            fpid,
            0,
        );
        return;
    };
    if retained.window_domains.get(&target_str) != Some(&domain) {
        no_call(
            session,
            shared,
            FocusNoCallReason::IdentityStale,
            "no-call-home-identity-stale",
            fpid,
            app_generation,
        );
        return;
    }
    if !current.contains(&(req.output.0.clone(), req.workspace.clone())) {
        no_call(
            session,
            shared,
            FocusNoCallReason::Unreadable,
            "no-call-scope-unreadable",
            fpid,
            app_generation,
        );
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
                .find(|d| d.output == req.output && d.workspace == domain.workspace);
            match row {
                Some(d) if d.current && d.readable && ctx.revision == topology_revision => {}
                Some(_) => {
                    drop(guard);
                    no_call(
                        session,
                        shared,
                        FocusNoCallReason::Unreadable,
                        "no-call-native-unreadable",
                        fpid,
                        app_generation,
                    );
                    return;
                }
                None => {
                    drop(guard);
                    no_call(
                        session,
                        shared,
                        FocusNoCallReason::IdentityStale,
                        "no-call-native-missing-identity-stale",
                        fpid,
                        app_generation,
                    );
                    return;
                }
            }
        }
    }
    if retained.minimized.contains_key(&target_str) {
        no_call(
            session,
            shared,
            FocusNoCallReason::ShowBlocked,
            "no-call-minimized-show-blocked",
            fpid,
            app_generation,
        );
        return;
    }
    let roster_row = retained
        .apps
        .get(&fpid)
        .and_then(|app| app.roster.get(&fwid));
    let Some(entry) = roster_row else {
        no_call(
            session,
            shared,
            FocusNoCallReason::IdentityStale,
            "no-call-roster-missing-identity-stale",
            fpid,
            app_generation,
        );
        return;
    };
    if entry.minimized_meta || entry.hidden_meta {
        no_call(
            session,
            shared,
            FocusNoCallReason::ShowBlocked,
            "no-call-roster-meta-show-blocked",
            fpid,
            app_generation,
        );
        return;
    }
    match entry.excluded {
        Some(crate::classifier::ExcludeReason::Minimized)
        | Some(crate::classifier::ExcludeReason::HiddenApp) => {
            no_call(
                session,
                shared,
                FocusNoCallReason::ShowBlocked,
                "no-call-excluded-show-blocked",
                fpid,
                app_generation,
            );
            return;
        }
        Some(_) => {
            no_call(
                session,
                shared,
                FocusNoCallReason::Unsupported,
                "no-call-excluded-unsupported",
                fpid,
                app_generation,
            );
            return;
        }
        None => {}
    }
    // Core resolves automatic fullscreen/game policy before publishing a
    // request. The owner still rechecks the target's live native safety.
    if retained.observed_focus.is_none() && !retained.observed_focus_known {
        no_call(
            session,
            shared,
            FocusNoCallReason::ForegroundUnproven,
            "no-call-focus-unproven",
            fpid,
            app_generation,
        );
        return;
    }
    // Outbox full and ownership-lost legs spend into no-call, never retry.
    if lock_focus(focus_queue).len() >= focus_capacity {
        no_call(
            session,
            shared,
            FocusNoCallReason::Superseded,
            "no-call-outbox-full-superseded",
            fpid,
            app_generation,
        );
        return;
    }
    let topology_revision = lock_shared(shared).topology_revision;
    let binding = CloseFocusBinding {
        close_id: req.id.clone(),
        target: req.target.clone(),
        issued_revision: req.issued_revision,
        cause: req.cause,
        cmd_seq: req.cmd_seq,
        after_shown: req.after_shown.clone(),
        adapter_incarnation: incarnation.clone(),
        core_generation: core_gen.clone(),
        output: req.output.clone(),
        workspace: WorkspaceId(req.workspace.clone()),
        domain,
        generation: app_generation,
        topology_revision,
        fallback_pre: fallback_pre.clone(),
        fallback_known,
        expected_sid: None,
        proven_stamp: None,
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
        close_id: req.id.clone(),
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
                disposition: FocusDisposition::NotCalled {
                    reason: FocusNoCallReason::Superseded,
                },
                pre: binding.fallback_pre.clone(),
                post: None,
                foreground_proven: false,
                arrival: Vec::new(),
                attempt_ms: 0,
            });
            lock_shared(shared).diagnostics.focus_dropped_full += 1;
            emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
            diag(
                shared,
                "no-call-outbox-full-reserved-superseded",
                fpid,
                app_generation,
            );
            return;
        }
        match focus_slot {
            Some(slot) => {
                let cmd = FocusCommand {
                    pid: fpid,
                    generation: app_generation,
                    request_id,
                    wid: fwid,
                    window: target_str.clone(),
                    revision: topology_revision,
                    route: session.focus_route(),
                    expect_prior: None,
                    close: Some(binding),
                };
                if slot.set_latest(cmd).is_some() {
                    // Latest-only per app: purge superseded same-pid rows
                    // before recording the replacement (same rationale as the
                    // selection-release lane: taken rows lose nothing, their
                    // feedback rides the staged row; the terminal retirement
                    // for a purged row is a no-op). Runs after both capacity
                    // fences, so a genuinely full outbox still spends into
                    // backpressure above.
                    lock_focus(focus_queue).retain(|row| row.pid != fpid);
                    lock_focus(focus_queue).push(request);
                    lock_shared(shared).diagnostics.focus_dispatched += 1;
                    emit(shared, pid, generation, seq, "close-focus", "dispatched", 1);
                    diag(shared, "dispatched-native", fpid, app_generation);
                    if let Some(span) = crate::stage_timing::with_current(|held| *held) {
                        crate::stage_timing::record_for(
                            shared,
                            fpid,
                            app_generation,
                            request_id,
                            crate::stage_timing::stage::FOCUS_BIND,
                            "focus",
                            span.id,
                        );
                        crate::stage_timing::note_focus(
                            fpid,
                            app_generation,
                            request_id,
                            span.id,
                            span.t0_us,
                        );
                    }
                } else {
                    session.stage_close_feedback(FocusFeedback {
                        id: req.id.clone(),
                        target: req.target.clone(),
                        revision: req.issued_revision,
                        disposition: FocusDisposition::NotCalled {
                            reason: FocusNoCallReason::IdentityStale,
                        },
                        pre: fallback_for_slot.clone(),
                        post: None,
                        foreground_proven: false,
                        arrival: Vec::new(),
                        attempt_ms: 0,
                    });
                    lock_shared(shared).diagnostics.focus_dropped_full += 1;
                    emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
                    diag(shared, "slot-stopped-identity-stale", fpid, app_generation);
                }
            }
            None => {
                session.stage_close_feedback(FocusFeedback {
                    id: req.id.clone(),
                    target: req.target.clone(),
                    revision: req.issued_revision,
                    disposition: FocusDisposition::NotCalled {
                        reason: FocusNoCallReason::IdentityStale,
                    },
                    pre: fallback_for_slot.clone(),
                    post: None,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    attempt_ms: 0,
                });
                lock_shared(shared).diagnostics.focus_dropped_full += 1;
                emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
                diag(shared, "slot-missing-identity-stale", fpid, app_generation);
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
                    disposition: FocusDisposition::NotCalled {
                        reason: FocusNoCallReason::Superseded,
                    },
                    pre: binding.fallback_pre.clone(),
                    post: None,
                    foreground_proven: false,
                    arrival: Vec::new(),
                    attempt_ms: 0,
                });
                lock_shared(shared).diagnostics.focus_dropped_full += 1;
                emit(shared, pid, generation, seq, "close-focus", "no-call", 1);
                diag(
                    shared,
                    "no-call-outbox-full-synthetic-superseded",
                    fpid,
                    app_generation,
                );
                return;
            }
            queue.push(request);
        }
        lock_shared(shared).diagnostics.focus_dispatched += 1;
        emit(shared, pid, generation, seq, "close-focus", "dispatched", 1);
        diag(shared, "dispatched-synthetic", fpid, app_generation);
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

    /// Immutable request tuple plus sampled stamps that dedup one capture.
    type ArrivalCaptureAttemptKey = (String, u64, String, String, u64, u64, u64, u64);
    /// Strict bind harness legs: session, retained core, shared state,
    /// focus queue, and the synthetic reply.
    type StrictBindParts = (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Mutex<Vec<FocusRequest>>>,
        tiler_core::world_report::WorldReplyV4,
    );
    /// Focus-3 arrival bind harness: strict legs plus the live focus slot.
    type Focus3BindParts = (
        WorldSession,
        RetainedCore,
        Arc<Mutex<Shared>>,
        Arc<Mutex<Vec<FocusRequest>>>,
        Arc<FocusSlot>,
        tiler_core::world_report::WorldReplyV4,
    );
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::{
        OperationEvent, OperationKind, OperationOutcome, SwitchEvent, SwitchFocus, SwitchOutcome,
    };

    fn output() -> OutputId {
        OutputId("macos-display-1".to_owned())
    }

    /// The disposition every accepted native focus write reports: one setter
    /// call that natively reported success with no mechanical reason.
    fn accepted_disposition() -> tiler_core::world_report::FocusDisposition {
        tiler_core::world_report::FocusDisposition::Called {
            setter: tiler_core::world_report::FrameSetter::Accepted,
            reason: tiler_core::world_report::VisibilityReason::Empty,
        }
    }

    /// A focus no-call with its bounded mechanical reason.
    fn not_called_disposition(
        reason: tiler_core::world_report::FocusNoCallReason,
    ) -> tiler_core::world_report::FocusDisposition {
        tiler_core::world_report::FocusDisposition::NotCalled { reason }
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
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: crate::runtime::empty_world_presentation_v4(),
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Deterministic live-lifetime bindings for adapter intent tests.
    #[derive(Clone)]
    struct IdentityCell(Arc<Mutex<BTreeMap<u32, crate::intent_store::LiveBinding>>>);

    impl IdentityCell {
        fn new(pid: u32, launch_date_ns: u64, bundle_id: &str) -> Self {
            let cell = Self(Arc::new(Mutex::new(BTreeMap::new())));
            cell.set(pid, launch_date_ns, bundle_id);
            cell
        }

        fn set(&self, pid: u32, launch_date_ns: u64, bundle_id: &str) {
            self.0.lock().expect("identity cell").insert(
                pid,
                crate::intent_store::LiveBinding {
                    launch_date_ns,
                    bundle_id: bundle_id.to_owned(),
                },
            );
        }

        fn probe(&self) -> crate::intent_store::IdentityProbe {
            let cell = self.clone();
            Box::new(move |pid| cell.0.lock().expect("identity cell").get(&pid).cloned())
        }
    }

    fn intent_reply(
        win: &str,
        data: Option<tiler_core::intent_data::IntentData>,
    ) -> tiler_core::world_report::WorldReplyV4 {
        let mut reply = empty_reply();
        reply
            .intent_desired
            .push(tiler_core::world_report::IntentDesire {
                win: tiler_core::directional::WindowId(win.to_owned()),
                data,
            });
        reply
    }

    fn intent_session(
        store: crate::intent_store::IntentStore,
        identity: &IdentityCell,
    ) -> WorldSession {
        let mut session = focus_session();
        session.attach_intent_store(store);
        session.set_intent_identity(identity.probe());
        // Existing intent tests always supply a live binding, so liveness is
        // never consulted; pin it deterministically anyway rather than read
        // the host's real process table.
        session.set_intent_process_liveness(Box::new(|_| true));
        session
    }

    fn store_snapshot(path: &std::path::Path) -> (Vec<u8>, std::time::SystemTime) {
        (
            std::fs::read(path).expect("read intent store"),
            std::fs::metadata(path)
                .expect("intent store metadata")
                .modified()
                .expect("intent store mtime"),
        )
    }

    #[test]
    fn intent_reconcile_writes_once_then_zero_on_equality_and_clears() {
        use std::os::unix::fs::PermissionsExt;
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        assert!(
            !session.reconcile_intent(&save),
            "first save persists and proceeds"
        );
        let path = store_dir.join(format!(
            "intent-v{}-{}-{}.json",
            crate::intent_store::STORE_VERSION,
            crate::ffi::audit_self().map(|(uid, _)| uid).unwrap_or(0),
            crate::ffi::audit_self().map(|(_, asid)| asid).unwrap_or(0),
        ));
        assert!(path.exists(), "durable entry written: {path:?}");
        let (bytes, mtime) = store_snapshot(&path);
        // A later accepted reply carries the same retained projection:
        // exact equality costs zero writes.
        assert!(
            !session.reconcile_intent(&save),
            "equal verified desire never holds"
        );
        let (again, again_mtime) = store_snapshot(&path);
        assert_eq!(bytes, again, "equal desire writes zero bytes");
        assert_eq!(mtime, again_mtime, "equal desire touches no mtime");
        // Explicit null clears the persisted entry and is remembered.
        let clear = intent_reply("4242_7", None);
        assert!(!session.reconcile_intent(&clear));
        let cleared = std::fs::read(&path).expect("read cleared store");
        assert!(
            !String::from_utf8_lossy(&cleared).contains("4242_7"),
            "clear removes the row"
        );
        let (_, cleared_mtime) = store_snapshot(&path);
        assert!(
            !session.reconcile_intent(&clear),
            "repeated clear carries no authority"
        );
        assert_eq!(
            store_snapshot(&path).1,
            cleared_mtime,
            "null equality costs zero writes"
        );
        // A later changed save persists again (repeatable storage).
        let tile = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(false), None)),
        );
        assert!(!session.reconcile_intent(&tile));
        let stored = std::fs::read(&path).expect("read store");
        assert!(String::from_utf8_lossy(&stored).contains("4242_7"));
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn intent_failed_write_holds_then_recovers_on_later_accept() {
        use std::os::unix::fs::PermissionsExt;
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        std::fs::create_dir_all(&store_dir).expect("store dir");
        std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o500))
            .expect("read-only store dir");
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        assert!(
            session.reconcile_intent(&save),
            "unwritable store holds the whole reply"
        );
        let path = store_dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        assert!(!path.exists(), "failed write persists nothing");
        // Restore storage; the NEXT accepted reply retries the still-unequal
        // desire and recovers with no timer or latch.
        std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o700))
            .expect("restore store dir");
        assert!(
            !session.reconcile_intent(&save),
            "later accepted reply recovers"
        );
        assert!(path.exists(), "recovered reply persists");
        assert!(
            !session.reconcile_intent(&save),
            "recovered equality is open"
        );
    }

    #[test]
    fn intent_skipped_issuing_publication_persists_on_later_reply() {
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        // Issuing publication skipped entirely (never reconciled): nothing
        // is written from it, and no continuation is retained.
        let path = store_dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        assert!(!path.exists());
        // A later accepted commandless publication carries the retained
        // target: exactly one write, then native apply is free.
        assert!(!session.reconcile_intent(&save));
        assert!(path.exists());
        let (bytes, mtime) = store_snapshot(&path);
        assert!(!session.reconcile_intent(&save));
        assert_eq!(bytes, store_snapshot(&path).0);
        assert_eq!(mtime, store_snapshot(&path).1);
    }

    #[test]
    fn intent_store_mode_attaches_only_the_configured_store() {
        use crate::runtime_types::IntentStoreMode;
        let mut session = focus_session();
        assert!(
            session
                .open_intent_store(&IntentStoreMode::Disabled)
                .is_none(),
            "disabled mode opens nothing"
        );
        assert!(!session.intent_store_attached(), "no implicit store");

        let (root, _unused) = crate::intent_store::test_store_parts();
        let dir = root.dir();
        if crate::ffi::audit_self().is_err() {
            assert!(
                session
                    .open_intent_store(&IntentStoreMode::Isolated(dir))
                    .is_none(),
                "no live audit namespace: the store refuses closed"
            );
            return;
        }
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        session.set_intent_identity(identity.probe());
        session.set_intent_process_liveness(Box::new(|_| true));
        let opened = session.open_intent_store(&IntentStoreMode::Isolated(dir.clone()));
        assert_eq!(
            opened.as_deref(),
            Some(dir.as_path()),
            "exact configured dir"
        );
        assert!(session.intent_store_attached());
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        assert!(!session.reconcile_intent(&save));
        let path = dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        assert!(path.exists(), "reconcile writes inside the isolated store");
    }

    #[test]
    fn intent_alive_unverifiable_row_is_skipped_once_and_never_freezes() {
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        // No live binding for the target pid: the identity cell holds a
        // different process only.
        let identity = IdentityCell::new(9999, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        session.set_intent_process_liveness(Box::new(|pid| pid == 4242));
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        let path = store_dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        assert!(
            !session.reconcile_intent(&save),
            "alive-but-unverifiable row never freezes the reply"
        );
        assert!(
            !path.exists(),
            "an unverifiable lifetime binding is never persisted"
        );
        assert!(
            session.intent_unverifiable_windows().contains("4242_7"),
            "one bounded diagnostic is recorded for the window"
        );
        // Later accepted replies stay open and never repeat the diagnostic.
        assert!(!session.reconcile_intent(&save));
        assert!(!session.reconcile_intent(&save));
        assert!(!path.exists());
        assert_eq!(session.intent_unverifiable_windows().len(), 1);
    }

    #[test]
    fn intent_gone_process_without_binding_still_holds() {
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let identity = IdentityCell::new(9999, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        session.set_intent_process_liveness(Box::new(|_| false));
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        let path = store_dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        assert!(
            session.reconcile_intent(&save),
            "gone process keeps the recoverable whole-reply hold"
        );
        assert!(!path.exists(), "nothing persisted while unverifiable");
        assert!(
            session.intent_unverifiable_windows().is_empty(),
            "a gone process is a hold, not a skip diagnostic"
        );
        // The process returns with still no readable lifetime binding: the
        // row is skipped (never persisted) and the reply opens.
        session.set_intent_process_liveness(Box::new(|_| true));
        assert!(!session.reconcile_intent(&save));
        assert!(!path.exists());
        assert!(session.intent_unverifiable_windows().contains("4242_7"));
    }

    #[test]
    fn intent_lifetime_change_fences_equality_and_hydration() {
        let (root, store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        let mut session = intent_session(store, &identity);
        let save = intent_reply(
            "4242_7",
            Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        );
        assert!(!session.reconcile_intent(&save));
        let path = store_dir.join({
            let (uid, asid) = crate::ffi::audit_self().unwrap_or((0, 0));
            format!("intent-v1-{uid}-{asid}.json")
        });
        let original = std::fs::read(&path).expect("read store");
        // PID reuse with a different numeric launch date is a different
        // lifetime: the same data is NOT equal, so it rewrites the binding.
        identity.set(4242, 778, "com.example.app");
        assert!(!session.reconcile_intent(&save));
        let rewritten = std::fs::read(&path).expect("read store");
        assert_ne!(original, rewritten, "new lifetime rewrites the binding");

        // Hydration against the old binding must refuse; the live binding
        // imports. Late import after admission is ignored.
        let (root, mut store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let seed = crate::intent_store::StoredEntry {
            win: "4242_7".to_owned(),
            wid: 7,
            pid: 4242,
            launch_date_ns: 777,
            bundle_id: "com.example.app".to_owned(),
            data: tiler_core::intent_data::encode_intent_data(
                &tiler_core::intent_data::IntentData::new(Some(true), None),
            ),
        };
        let mut changes = BTreeMap::new();
        changes.insert("4242_7".to_owned(), Some(seed));
        store.apply_changes(changes).expect("seed store");
        let identity = IdentityCell::new(4242, 778, "com.example.app");
        let mut session = focus_session();
        session.attach_intent_store(store);
        session.set_intent_identity(identity.probe());
        assert!(
            session.intent_records_for_admission(&["4242_7"]).is_empty(),
            "ID reuse with a new launch date imports nothing"
        );
        assert!(
            session.intent_records_for_admission(&["4242_8"]).is_empty(),
            "store rows only import for a window present in the report"
        );
        identity.set(4242, 777, "com.example.app");
        let imported = session.intent_records_for_admission(&["4242_7"]);
        assert_eq!(
            imported,
            vec![tiler_core::world_report::IntentRecord {
                win: tiler_core::directional::WindowId("4242_7".to_owned()),
                data: tiler_core::intent_data::encode_intent_data(
                    &tiler_core::intent_data::IntentData::new(Some(true), None)
                ),
            }]
        );
        session.mark_intent_admitted(&imported);
        assert!(
            session.intent_records_for_admission(&["4242_7"]).is_empty(),
            "an imported window never rides a later report"
        );
        let _ = store_dir;
    }

    #[test]
    fn intent_hydration_is_per_window_and_admits_late_apps_once() {
        let (root, mut store) = crate::intent_store::test_store_parts();
        let store_dir = root.dir();
        let data_a = tiler_core::intent_data::encode_intent_data(
            &tiler_core::intent_data::IntentData::new(Some(true), None),
        );
        let data_b = tiler_core::intent_data::encode_intent_data(
            &tiler_core::intent_data::IntentData::new(Some(false), None),
        );
        let mut changes = BTreeMap::new();
        for (win, wid, data) in [("4242_7", 7, &data_a), ("4242_9", 9, &data_b)] {
            changes.insert(
                win.to_owned(),
                Some(crate::intent_store::StoredEntry {
                    win: win.to_owned(),
                    wid,
                    pid: 4242,
                    launch_date_ns: 777,
                    bundle_id: "com.example.app".to_owned(),
                    data: data.clone(),
                }),
            );
        }
        store.apply_changes(changes).expect("seed two windows");
        let identity = IdentityCell::new(4242, 777, "com.example.app");
        let mut session = focus_session();
        session.attach_intent_store(store);
        session.set_intent_identity(identity.probe());

        // Report #1 sees only A: only A hydrates and is marked imported.
        let first = session.intent_records_for_admission(&["4242_7"]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].win.0, "4242_7");
        session.mark_intent_admitted(&first);

        // Report #2 sees A and B: A never rides again, while late-arriving
        // B still hydrates exactly once.
        let second = session.intent_records_for_admission(&["4242_7", "4242_9"]);
        assert_eq!(second.len(), 1, "only the newly present window imports");
        assert_eq!(second[0].win.0, "4242_9");
        assert_eq!(second[0].data, data_b);
        session.mark_intent_admitted(&second);

        // Report #3: every stored window already rode, so nothing imports.
        assert!(
            session
                .intent_records_for_admission(&["4242_7", "4242_9"])
                .is_empty(),
            "each stored window imports at most once"
        );
        // A present-but-lifetime-mismatched window still refuses, and the
        // imported set is per window: a different pid never suppresses it.
        identity.set(4242, 778, "com.example.app");
        assert!(
            session
                .intent_records_for_admission(&["4242_9", "4242_11"])
                .is_empty(),
            "ID reuse with a new launch date imports nothing"
        );
        let _ = store_dir;
    }

    // -- Retained switch settlement projection (once per identity) --

    /// One retained switch terminal row.
    fn switch_event(
        output: &OutputId,
        operation: &str,
        outcome: SwitchOutcome,
        failed: &[&str],
        focus: SwitchFocus,
    ) -> SwitchEvent {
        use tiler_core::directional::WindowId;
        SwitchEvent {
            output: output.clone(),
            operation_id: operation.to_owned(),
            outcome,
            failed: failed
                .iter()
                .map(|window| WindowId((*window).to_owned()))
                .collect(),
            focus,
            fallback: false,
        }
    }

    /// A reply bound to the session's current incarnation/generation.
    fn switch_reply(
        session: &WorldSession,
        request_id: &str,
        observation_seq: u64,
        revision: u64,
        event: SwitchEvent,
    ) -> WorldReplyV4 {
        let mut reply = empty_reply();
        reply.adapter_incarnation = session.incarnation().to_owned();
        reply.core_generation = session.sequencer().bound_generation();
        reply.request_id = request_id.to_owned();
        reply.based_on_observation_seq = observation_seq;
        reply.state_revision = revision;
        reply.ready = true;
        reply.switch_settled.push(event);
        reply
    }

    /// `(outcome, count)` receipts of one ring kind, in emission order.
    fn receipts(shared: &Arc<Mutex<Shared>>, kind: &str) -> Vec<(&'static str, u64)> {
        lock_shared(shared)
            .log
            .iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| (entry.outcome, entry.count))
            .collect()
    }

    #[test]
    fn switch_settlement_survives_a_skipped_reply_via_the_retained_projection() {
        // The required `switchSettled` field is complete current state,
        // not a take-once queue: a terminal whose issuing reply was skipped
        // (newer input moved the watermark mid-flight, so the sequencer
        // refuses it) must still be applied from the later applied reply
        // that re-publishes the same retained row.
        let mut session = focus_session();
        let shared = focus_shared();
        let terminal = switch_event(
            &output(),
            "op-5",
            SwitchOutcome::Complete,
            &[],
            SwitchFocus::Proven,
        );

        let issued = session
            .sequencer()
            .try_begin_request(None)
            .expect("issuing flight opens");
        session.sequencer().invalidate();
        let issuing = switch_reply(
            &session,
            &issued.request_id,
            issued.observation_seq,
            1,
            terminal.clone(),
        );
        match session.sequencer().accept_reply(&issuing) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "stale-observation"),
            ReplyVerdict::Applied(_) => panic!("stale issuing reply must be skipped"),
        }
        session.sequencer().clear_flight();
        assert!(
            session.switch_settlements.is_empty(),
            "a skipped reply applies no switch terminal"
        );
        assert!(receipts(&shared, "switch").is_empty());

        // The catch-up reply carries the retained projection: the verdict
        // lands here, exactly once, even though the issuing reply never
        // applied.
        let catchup = session
            .sequencer()
            .try_begin_request(None)
            .expect("catch-up flight opens");
        let later = switch_reply(
            &session,
            &catchup.request_id,
            catchup.observation_seq,
            2,
            terminal,
        );
        let ReplyVerdict::Applied(applied) = session.sequencer().accept_reply(&later) else {
            panic!("fresh catch-up reply must apply")
        };
        consume_switch_settlements(&mut session, &shared, &applied, 0, 0, 0);
        assert_eq!(session.switch_settlements.len(), 1);
        assert_eq!(session.switch_settlements[0].output, output());
        assert_eq!(session.switch_settlements[0].operation_id, "op-5");
        assert_eq!(
            receipts(&shared, "switch"),
            vec![("complete", 0)],
            "one receipt for the terminal the skipped reply lost"
        );
        assert_eq!(receipts(&shared, "switch-focus"), vec![("proven", 0)]);
        // A later applied reply re-publishes the same row: inert.
        consume_switch_settlements(&mut session, &shared, &applied, 0, 0, 0);
        assert_eq!(
            receipts(&shared, "switch").len(),
            1,
            "retained re-projection never double-applies"
        );
    }

    #[test]
    fn retained_operation_settlements_distinguish_phases_and_prune_departed_owner() {
        let mut session = focus_session();
        let shared = focus_shared();
        let window = WindowId("4242_7".to_owned());
        let mut reply = empty_reply();
        reply.operation_settled = vec![
            OperationEvent {
                window: window.clone(),
                kind: OperationKind::Clear,
                operation_id: "same-operation".to_owned(),
                outcome: OperationOutcome::ClearContinuedFloat,
                cause: None,
            },
            OperationEvent {
                window,
                kind: OperationKind::Float,
                operation_id: "same-operation".to_owned(),
                outcome: OperationOutcome::FloatFrameProven,
                cause: None,
            },
        ];
        for _ in 0..2 {
            consume_operation_settlements(&mut session, &shared, &reply, 0, 0, 0);
        }
        let old_rows = std::mem::take(&mut reply.operation_settled);
        consume_operation_settlements(&mut session, &shared, &reply, 0, 0, 0);
        assert_eq!(
            receipts(&shared, "operation-clear"),
            [("continued-float", 0)]
        );
        assert_eq!(
            receipts(&shared, "operation-float"),
            [("float-frame-proven", 0)]
        );
        // An owner seen after departure does not inherit the old dedup slot.
        reply.operation_settled = old_rows;
        consume_operation_settlements(&mut session, &shared, &reply, 0, 0, 0);
        assert_eq!(receipts(&shared, "operation-clear").len(), 2);
        assert_eq!(receipts(&shared, "operation-float").len(), 2);
    }

    #[test]
    fn replayed_switch_settlement_applies_once() {
        let mut session = focus_session();
        let shared = focus_shared();
        let terminal = switch_event(
            &output(),
            "op-5",
            SwitchOutcome::CompletedWithFailures,
            &["4242_7", "4242_8"],
            SwitchFocus::Refused,
        );
        let mut reply = empty_reply();
        reply.switch_settled.push(terminal);
        // Duplicate delivery (redelivered reply, re-projected row) is
        // exact-identity inert: one application, one receipt pair.
        for _ in 0..3 {
            consume_switch_settlements(&mut session, &shared, &reply, 0, 0, 0);
        }
        assert_eq!(session.switch_settlements.len(), 1);
        let slot = &session.switch_settlements[0];
        assert_eq!(slot.outcome, SwitchOutcome::CompletedWithFailures);
        assert_eq!(slot.focus, SwitchFocus::Refused);
        assert_eq!(slot.failed.len(), 2, "frozen failed members preserved");
        assert_eq!(
            receipts(&shared, "switch"),
            vec![("completed-with-failures", 2)]
        );
        assert_eq!(receipts(&shared, "switch-focus"), vec![("refused", 2)]);
    }

    #[test]
    fn issuing_reply_and_retained_projection_agree_apply_once() {
        // The issuing reply carries the terminal and the retained
        // projection re-publishes the identical row on the next applied
        // reply: both channels agree, so the terminal applies once through
        // the production consumption site.
        let mut session = focus_session();
        let mut retained = crate::runtime_core::RetainedCore::default();
        let shared = focus_shared();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let terminal = switch_event(
            &output(),
            "op-5",
            SwitchOutcome::Complete,
            &[],
            SwitchFocus::Proven,
        );

        let mut issuing = empty_reply();
        issuing.switch_settled.push(terminal.clone());
        consume_world_reply(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            16,
            &issuing,
            &[],
            None,
            None,
            "select",
            "accepted",
            0,
            0,
            0,
            0,
        );
        assert_eq!(session.switch_settlements.len(), 1);

        // The retained projection re-publishes the same row.
        let mut reprojected = empty_reply();
        reprojected.switch_settled.push(terminal);
        consume_world_reply(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            16,
            &reprojected,
            &[],
            None,
            None,
            "select",
            "accepted",
            0,
            0,
            0,
            0,
        );
        assert_eq!(session.switch_settlements.len(), 1);
        assert_eq!(session.switch_settlements[0].operation_id, "op-5");
        assert_eq!(
            receipts(&shared, "switch"),
            vec![("complete", 0)],
            "agreement between the two channels applies the terminal once"
        );
    }

    #[test]
    fn retained_switch_projection_is_authoritative_over_an_earlier_reply() {
        // The adapter applied the predecessor operation's terminal from its
        // issuing reply. A later applied reply's retained projection names
        // the successor operation's terminal for the same owning output (the
        // contract's Q1 A replacement) and a second output's terminal. The
        // retained projection is complete current state, so it is
        // authoritative: the differing row replaces the earlier applied
        // snapshot instead of being ignored in its favour, while a
        // different output never erases the first.
        let mut session = focus_session();
        let shared = focus_shared();
        let other = OutputId("macos-display-2".to_owned());
        let predecessor = switch_event(
            &output(),
            "op-5",
            SwitchOutcome::Complete,
            &[],
            SwitchFocus::Proven,
        );
        let successor = switch_event(
            &output(),
            "op-6",
            SwitchOutcome::CompletedWithFailures,
            &["4242_7"],
            SwitchFocus::Unproven,
        );
        let elsewhere = switch_event(
            &other,
            "op-7",
            SwitchOutcome::Complete,
            &[],
            SwitchFocus::Unproven,
        );

        let mut issuing = empty_reply();
        issuing.switch_settled.push(predecessor);
        consume_switch_settlements(&mut session, &shared, &issuing, 0, 0, 0);
        assert_eq!(session.switch_settlements[0].operation_id, "op-5");

        let mut later = empty_reply();
        later.switch_settled.push(successor);
        later.switch_settled.push(elsewhere);
        consume_switch_settlements(&mut session, &shared, &later, 0, 0, 0);

        assert_eq!(
            session.switch_settlements.len(),
            2,
            "keys never erase each other"
        );
        assert_eq!(session.switch_settlements[0].output, output());
        assert_eq!(
            session.switch_settlements[0].operation_id, "op-6",
            "the retained projection replaced the reply-scoped predecessor"
        );
        assert_eq!(
            session.switch_settlements[0].outcome,
            SwitchOutcome::CompletedWithFailures
        );
        assert_eq!(session.switch_settlements[0].focus, SwitchFocus::Unproven);
        assert_eq!(session.switch_settlements[1].output, other);
        assert_eq!(session.switch_settlements[1].operation_id, "op-7");
        assert_eq!(
            receipts(&shared, "switch"),
            vec![
                ("complete", 0),
                ("completed-with-failures", 1),
                ("complete", 0)
            ],
            "each distinct terminal applies once; the corrected one too"
        );

        // A corrected verdict for the same parent operation is a distinct
        // terminal identity: the projection wins and the slot is replaced.
        let mut corrected = later;
        corrected.switch_settled[1] = switch_event(
            &other,
            "op-7",
            SwitchOutcome::Complete,
            &[],
            SwitchFocus::Proven,
        );
        consume_switch_settlements(&mut session, &shared, &corrected, 0, 0, 0);
        assert_eq!(session.switch_settlements[1].focus, SwitchFocus::Proven);
        assert_eq!(
            receipts(&shared, "switch").len(),
            4,
            "the corrected verdict is applied, not dropped in favour of the first"
        );
    }

    #[test]
    fn ordinary_save_free_reply_not_held_and_dispatches() {
        // Ordinary admission/reflow emits no intent desires, so the durable
        // gate stays open — and the real production dispatch still applies.
        let mut session = focus_session();
        assert!(
            !intent_write_hold(&mut session, &empty_reply()),
            "save-free reply must not hold"
        );
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
    }

    #[test]
    fn capability_truth_at_typed_reply_gate_refuses_host_unsupported_rows() {
        use tiler_core::world_report::{ScopeAuthority, StateProperty, StateTarget};
        // Typed capability gate: the production `WorldSession` handshake
        // binds exactly the `macos_capabilities` truth (above/warp/
        // native-sticky all false) that core reply validation reads. Each
        // host-unsupported row refuses as `snapshot-invalid` with its fixed
        // rule token — before any sequencer acceptance or actuation, with
        // no codec decode on the production path.
        fn state_window(property: StateProperty) -> tiler_core::world_report::WindowDesire {
            tiler_core::world_report::WindowDesire {
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
                command_seq: None,
                visibility: None,
                visibility_authority: ScopeAuthority::Untouched,
                state: vec![StateTarget {
                    property,
                    value: true,
                    authority: ScopeAuthority::Untouched,
                    command_seq: None,
                    automatic_clear: false,
                }],
            }
        }
        let mut session = focus_session();
        let host = crate::world_connection::macos_capabilities();
        assert!(!host.above && !host.warp && !host.native_sticky);
        assert_eq!(
            session.engine_mut().capabilities(),
            Some(&host),
            "the production handshake binds macOS capability truth"
        );
        let mut above_only = empty_reply();
        above_only.windows.push(state_window(StateProperty::Above));
        let error = tiler_core::world_reply_validation::validate_reply_v4(
            &above_only,
            session.engine_mut().capabilities(),
        )
        .expect_err("macOS host must refuse above rows");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "above-capability-false");
        // Each gate fires independently: warp-only and sticky-only rows
        // refuse with their own fixed tokens.
        let mut warp_only = empty_reply();
        warp_only.cursor_warp = Some((4, 4));
        let error = tiler_core::world_reply_validation::validate_reply_v4(
            &warp_only,
            session.engine_mut().capabilities(),
        )
        .expect_err("macOS host must refuse warp point");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "warp-capability-false");
        let mut sticky_only = empty_reply();
        sticky_only
            .windows
            .push(state_window(StateProperty::Sticky));
        let error = tiler_core::world_reply_validation::validate_reply_v4(
            &sticky_only,
            session.engine_mut().capabilities(),
        )
        .expect_err("macOS host must refuse sticky target");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "sticky-capability-false");
        // Ordinary save-free replies stay accepted under the same gate
        // (success leg pinned end-to-end by
        // `ordinary_save_free_reply_not_held_and_dispatches` dispatching
        // through the production typed ingress).
        tiler_core::world_reply_validation::validate_reply_v4(
            &empty_reply(),
            session.engine_mut().capabilities(),
        )
        .expect("ordinary reply must validate under macOS caps");
    }

    #[test]
    fn typed_ingress_refuses_invalid_and_over_cap_typed_reports() {
        // The typed twins of the old decoder refusals come from core
        // admission, not codec tokens: duplicate action-output ids fail the
        // typed shape check, an over-cap section fails the request-size
        // gate, and neither refusal mutates engine state.
        let mut session = focus_session();
        let retained = crate::runtime_core::RetainedCore::default();
        let mut report = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Synthetic,
            &[],
            0,
            1,
            None,
            None,
            None,
            "req-1",
        )
        .expect("synthetic report builds");
        let revision = session.engine_mut().state_revision();
        assert!(
            matches!(
                typed_transact(&mut session, &report),
                WorldOutcome::Accepted(_)
            ),
            "a valid typed report is accepted"
        );
        assert!(
            session.engine_mut().state_revision() > revision,
            "acceptance advances engine state"
        );
        let accepted_revision = session.engine_mut().state_revision();

        let mut duplicate = report.clone();
        duplicate.action_outputs = vec![output(), output()];
        match typed_transact(&mut session, &duplicate) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "snapshot-invalid");
                assert_eq!(message, "duplicate-action-output");
            }
            other => panic!("duplicate action outputs must be rejected, got {other:?}"),
        }
        assert_eq!(
            session.engine_mut().state_revision(),
            accepted_revision,
            "a typed shape refusal never mutates engine state"
        );

        report.action_outputs = (0..=tiler_core::world_report::MAX_WORLD_SCREENS)
            .map(|index| tiler_core::directional::OutputId(format!("out-{index}")))
            .collect();
        match typed_transact(&mut session, &report) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "snapshot-invalid");
                assert_eq!(message, "section-over-cap");
            }
            other => panic!("over-cap action outputs must be rejected, got {other:?}"),
        }
        assert_eq!(
            session.engine_mut().state_revision(),
            accepted_revision,
            "an over-cap request refusal never mutates engine state"
        );

        // The canonical request-byte gate refuses an over-bound typed
        // request with the same frozen size token the wire decode used,
        // before any typed shape check and without state mutation.
        report.request_id = "req-over".to_owned();
        report.action_outputs = vec![tiler_core::directional::OutputId(
            "o".repeat(tiler_protocol::world_protocol::WORLD_MAX_REQUEST_BYTES + 1),
        )];
        match typed_transact(&mut session, &report) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "oversized");
                assert_eq!(message, "request exceeds size bound");
            }
            other => panic!("an over-bound typed request must refuse, got {other:?}"),
        }
        assert_eq!(
            session.engine_mut().state_revision(),
            accepted_revision,
            "an over-bound request refusal never mutates engine state"
        );
    }

    #[test]
    fn typed_ingress_classifies_duplicate_reused_and_superseded() {
        // Core replay classification over typed ingress: an identical
        // resend duplicates, a reused request sequence with changed input
        // refuses as reused, and an older sequence refuses as superseded.
        let mut session = focus_session();
        let retained = crate::runtime_core::RetainedCore::default();
        let report = build_dispatch_report(
            &mut session,
            &retained,
            &DispatchContext::Synthetic,
            &[],
            0,
            5,
            None,
            None,
            None,
            "req-5",
        )
        .expect("synthetic report builds");
        assert!(
            matches!(
                typed_transact(&mut session, &report),
                WorldOutcome::Accepted(_)
            ),
            "the first typed report is accepted"
        );
        assert!(
            matches!(
                typed_transact(&mut session, &report),
                WorldOutcome::Duplicate(_)
            ),
            "an identical resend duplicates"
        );
        let mut reused = report.clone();
        reused.monotonic_ms += 1;
        match typed_transact(&mut session, &reused) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "request-reused");
                assert_eq!(message, "request sequence reused with different input");
            }
            other => panic!("changed content on the same sequence must refuse, got {other:?}"),
        }
        let mut superseded = report.clone();
        superseded.request_seq = 4;
        match typed_transact(&mut session, &superseded) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "request-superseded");
                assert_eq!(
                    message,
                    "request sequence is older than the accepted request"
                );
            }
            other => panic!("an older request sequence must refuse, got {other:?}"),
        }
    }

    /// P6-2 (CommandConsumed + emission refusal): a replayed command whose
    /// sequence is at/below core's command high-watermark reconciles as
    /// `CommandConsumed`. Core already processed the report's closes when the
    /// reply is accepted, so the drain must run before the post-accept
    /// emission gate refuses the stale reply — otherwise the next report
    /// re-sends a close core already processed.
    #[test]
    fn consumed_stale_reply_drains_accepted_closes_before_refusal() {
        use tiler_core::world_report::WorldCommand;

        let mut session = focus_session();
        let shared = focus_shared();
        let ctx = mock_space_context(false);
        lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
        lock_shared(&shared)
            .native_cell
            .store(Some(Arc::clone(&ctx)));
        // Focus stays on a surviving window; a second roster window is the
        // accepted close target, so the replay report stays shape-valid with
        // both windows homed to the live current Space.
        let mut retained = close_retained(9, Some(close_target(9)), true);
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("retained app")
            .roster
            .insert(7, close_roster_entry(7));
        for wid in [9u32, 7] {
            retained.window_domains.insert(
                format!("{CLOSE_PID}_{wid}"),
                tiler_core::session::DomainKey {
                    output: output(),
                    workspace: tiler_core::directional::WorkspaceId("space-7".to_owned()),
                },
            );
        }
        let inbox = Arc::new(Inbox::new(16));
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        // Save-free core-resolved selection: the point is only the command
        // high-watermark and the close drain ordering, never an intent save.
        let command = WorldCommand::SelectNumbered {
            output: Some(output()),
            index: 1,
        };
        // First real dispatch executes the command core-side and records its
        // sequence as the command high-watermark.
        let first = session
            .sequencer()
            .try_begin_request(Some(command.clone()))
            .expect("first flight opens");
        let consumed_seq = first
            .command_seq
            .expect("a fresh command claims its sequence");
        match dispatch_ticket(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            first,
            None,
            false,
            false,
            crate::world_command_map::OP_SELECT,
            0,
            0,
            0,
        ) {
            DispatchVerdict::Applied { .. } => {}
            other => panic!("the first selection dispatch must apply, got {other:?}"),
        }
        assert!(
            !session
                .sequencer()
                .last_reply()
                .expect("applied reply")
                .command_consumed,
            "the first dispatch really executed the command"
        );

        // Fresh flight identity carrying the earlier consumed sequence:
        // core classifies the inline command as already consumed.
        crate::runtime::lock_shared(&shared).clock.invalidate();
        let mut replay = session
            .sequencer()
            .try_begin_request(Some(command))
            .expect("replay flight opens");
        replay.command_seq = Some(consumed_seq);
        // Core processes this close on the replay; the post-accept watermark
        // move then refuses the reply at the emission gate.
        session.pending_closes.insert(close_target(7));
        let hook: crate::runtime_shared::PostAcceptHook = Arc::new({
            let shared = Arc::clone(&shared);
            move || {
                crate::runtime::lock_shared(&shared).clock.invalidate();
            }
        });
        lock_shared(&shared)
            .post_accept_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .replace(hook);
        let verdict = dispatch_ticket(
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
            DispatchContext::Native(Arc::clone(&ctx)),
            Vec::new(),
            replay,
            None,
            false,
            false,
            crate::world_command_map::OP_SELECT,
            0,
            0,
            0,
        );
        lock_shared(&shared)
            .post_accept_hook
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
        match verdict {
            DispatchVerdict::Refused {
                token,
                select_operation,
            } => {
                assert_eq!(token, "stale-observation");
                assert!(
                    select_operation.is_none(),
                    "a non-select lane binds no operation identity"
                );
            }
            other => panic!("the consumed stale reply must refuse at emission, got {other:?}"),
        }
        assert!(
            session
                .sequencer()
                .last_reply()
                .expect("the applied consumed reply is retained")
                .command_consumed,
            "the replay really was WorldOutcome::CommandConsumed"
        );
        assert!(
            session.pending_closes.is_empty(),
            "the accepted close drains before the emission refusal"
        );
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
            }) => assert_eq!(
                window,
                tiler_core::directional::WindowSubject::Window(WindowId("4242_7".to_owned()))
            ),
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
            cursor_ack_seq: 0,
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
            arrival_captures: std::collections::BTreeMap::new(),
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
            last_native_creations: Vec::new(),
            last_core_generation: String::new(),
            indicator_revision: 0,
            presentation_style: crate::active_border::PresentationStylePx::default(),
            gesture_settlement: None,
            gesture_settlement_seq: 0,
        }))
    }

    /// Mocked native context for selection tests: revision 9 with space-3
    /// (readable, currency as flagged) plus space-7 (current, readable).
    /// Genuine domain rows — never a context waiver.
    fn mock_space_context(space3_current: bool) -> Arc<crate::runtime_types::NativeContext> {
        let domain = |sid: u64, current: bool| crate::runtime_types::NativeDomain {
            cg_id: 1,
            uuid: None,
            output: output(),
            workspace: tiler_core::directional::WorkspaceId(format!("space-{sid}")),
            space: sid,
            current,
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
                h: 560,
            },
        };
        Arc::new(crate::runtime_types::NativeContext {
            revision: 9,
            domains: vec![domain(3, space3_current), domain(7, true)],
            skipped: Vec::new(),
        })
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

    fn pending_op3() -> DeferredSelectionFocus {
        use tiler_core::directional::WindowId;
        DeferredSelectionFocus {
            operation_id: "op-3".to_owned(),
            command_seq: 3,
            focus_id: "focus-3".to_owned(),
            issued_revision: 3,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            window: WindowId("4242_7".to_owned()),
            output: output(),
            workspace: "ws-a".to_owned(),
            after_shown: after_shown_for("op-3", 3),
            route: crate::focus_route::FocusRoute::Slps,
            expected_sid: 3,
            app_generation: Some(1),
            prior_focus: Some("4242_7".to_owned()),
            prior_frontmost: Some(4242),
            proven_stamp: None,
            owner_queued: false,
        }
    }

    fn publish_pending_epoch(shared: &Arc<Mutex<Shared>>, pending: &DeferredSelectionFocus) {
        crate::runtime_shared::publish_close_focus_current(
            shared,
            Some(crate::runtime_shared::CloseFocusEpoch {
                id: pending.focus_id.clone(),
                revision: pending.issued_revision,
                adapter_incarnation: pending.adapter_incarnation.clone(),
                core_generation: pending.core_generation.clone(),
            }),
        );
    }

    /// Stamp the bound pending intent with the currently live
    /// proven-arrival stamp: direct `release_selection_focus_after_arrival`
    /// fence-isolation tests carry a live stamp the way the bind/reevaluate
    /// legs issue one — an unstamped intent correctly defers. This is a
    /// unit-level stand-in for a capture-proven stamp, never arrival proof:
    /// it isolates the completion-side fences (epoch, floor, app, foreground,
    /// native, outbox, slot) from the capture pipeline, whose provenance the
    /// gate/frozen/duplicate/`release_fires` tests prove through the real
    /// owner capture round below.
    fn stamp_pending_live(shared: &Arc<Mutex<Shared>>) {
        let stamp = {
            let guard = lock_shared(shared);
            crate::runtime_types::ProvenArrivalStamp {
                obs: guard.clock.get(),
                revision: guard.topology_revision,
                context_generation: guard.native_cell.generation(),
            }
        };
        if let Some(pending) = lock_shared(shared).selection_focus_pending.as_mut() {
            pending.proven_stamp = Some(stamp);
        }
    }

    /// Scripted arrival-capture backend for selection-focus tests: serves one
    /// ordered owner bracket batch for the frozen member. The batch below
    /// names the member live (shown both legs, singleton home, live frame),
    /// so the frozen proof can pass on genuine rows — never a seeded arrival
    /// map or a hand-written stamp. Runtime-thread tests can reuse this exact
    /// shape (backend plus [`drive_arrival_capture_once`]) for their own
    /// capture legs: it depends only on the shared [`Shared`] cell, the
    /// pending intent, and the [`Inbox`].
    struct CaptureBackend {
        reads: Vec<crate::runtime_types::BracketRead>,
    }
    impl crate::runtime_backend::AxBackend for CaptureBackend {
        fn apply_frame(
            &mut self,
            _cmd: &crate::runtime::FrameCommand,
        ) -> crate::runtime_types::AxCompletion {
            crate::runtime_types::AxCompletion::Failed {
                reason: "capture gate never actuates".to_owned(),
            }
        }
        fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
            Ok(Vec::new())
        }
        fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
            self.reads
                .iter()
                .filter(|read| wids.contains(&read.wid))
                .cloned()
                .collect()
        }
    }

    /// Fresh bracket batch for the frozen `4242_7` member: shown both legs,
    /// singleton home in Space 3, live frame matching the roster row.
    fn capture_reads_for_frozen_member() -> Vec<crate::runtime_types::BracketRead> {
        vec![crate::runtime_types::BracketRead {
            wid: 7,
            owner_pid: Some(CLOSE_PID),
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces: vec![3],
            frame: Some((0, 0, 100, 100)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }]
    }

    /// Drive one genuine owner arrival-capture round for the currently bound
    /// deferred intent through the real worker/engine path
    /// (`maybe_send_arrival_capture` over a real inbox, `Inbox::recv`, real
    /// `process_job` completion): validates the exact current tuple, retains
    /// the fresh bracket, and reevaluates the latest same-id intent from the
    /// sequencer's current reply. Panics when the worker owes no capture —
    /// the caller must have bound an intent whose frozen member `pid` owns.
    /// Clock precondition: the shared clock must already sit at or past the
    /// frozen `after.obs` — capture rows carry the live watermark verbatim,
    /// and the proof withholds rows older than the issue. The caller's
    /// `last_attempt` threads across rounds so a duplicate capture never
    /// re-reads. Returns the inbox for later duplicate-send or owner-result
    /// legs.
    #[allow(clippy::too_many_arguments)]
    fn drive_arrival_capture_once(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        pid: u32,
        generation: u64,
        reads: Vec<crate::runtime_types::BracketRead>,
        last_attempt: &mut Option<ArrivalCaptureAttemptKey>,
    ) -> Arc<Inbox> {
        let inbox = Arc::new(Inbox::new(16));
        let mut backend = CaptureBackend { reads };
        let cell = lock_shared(shared).native_cell.clone();
        assert!(
            crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                pid,
                generation,
                shared,
                &cell,
                &inbox,
                last_attempt,
            ),
            "fresh bracket batch notifies once"
        );
        let stop = AtomicBool::new(false);
        let job = inbox.recv(&stop).expect("capture notification queued");
        crate::runtime_thread::process_job(job, session, retained, shared, &inbox, focus_queue, 8);
        inbox
    }

    /// Retire the released queued intent through the genuine owner-result
    /// engine leg: a `CloseFocusResult` naming the exact still-current tuple
    /// through the real `process_job` completion. The result job stands in
    /// for the owner dispatch outcome (the full owner actuation producing
    /// such a result is a separate producer journey); the tuple retirement
    /// plus feedback staging below run the production code. Asserts nothing —
    /// callers assert the post-state they need.
    fn complete_owner_focus_result(
        session: &mut WorldSession,
        retained: &mut RetainedCore,
        shared: &Arc<Mutex<Shared>>,
        focus_queue: &Arc<Mutex<Vec<FocusRequest>>>,
        inbox: &Arc<Inbox>,
        pending: &DeferredSelectionFocus,
    ) {
        let job = crate::runtime_thread::EngineJob::CloseFocusResult {
            pid: CLOSE_PID,
            generation: CLOSE_GEN,
            cmd_id: pending.command_seq,
            close_id: pending.focus_id.clone(),
            target: pending.window.clone(),
            issued_rev: pending.issued_revision,
            adapter_incarnation: pending.adapter_incarnation.clone(),
            core_generation: pending.core_generation.clone(),
            disposition: tiler_core::world_report::FocusDisposition::Called {
                setter: tiler_core::world_report::FrameSetter::Accepted,
                reason: tiler_core::world_report::VisibilityReason::Empty,
            },
            pre: pending
                .prior_focus
                .clone()
                .map(tiler_core::directional::WindowId),
            post: Some(pending.window.clone()),
            foreground_proven: true,
            arrival: Vec::new(),
            attempt_ms: 0,
        };
        crate::runtime_thread::process_job(job, session, retained, shared, inbox, focus_queue, 8);
    }

    #[test]
    fn selection_binds_only_explicit_after_shown() {
        // Cause-gated deferral (never the ticket): exactly the
        // selection-backed explicit-command cause carrying frozen
        // `afterShown` binds a deferred intent. Every other cause — and a
        // bare explicit command without dependencies — binds nothing here
        // (they actuate immediately on the request lane).
        use tiler_core::world_report::{FocusRequest, FocusRequestCause};
        let mut req = FocusRequest {
            id: "focus-9".to_owned(),
            target: tiler_core::directional::WindowId("4242_7".to_owned()),
            issued_revision: 9,
            cause: FocusRequestCause::ExplicitCommand,
            output: output(),
            workspace: "ws-a".to_owned(),
            cmd_seq: Some(9),
            after_shown: Some(after_shown_for("op-9", 9)),
        };
        assert!(request_binds_selection(&req));
        req.after_shown = None;
        assert!(!request_binds_selection(&req));
        for cause in [
            FocusRequestCause::AutomaticAdmission,
            FocusRequestCause::FocusFollowsCursor,
            FocusRequestCause::PositiveClose,
        ] {
            req.cause = cause;
            req.after_shown = Some(after_shown_for("op-9", 9));
            assert!(
                !request_binds_selection(&req),
                "cause {cause:?} never defers"
            );
        }
    }

    #[test]
    fn deferred_resolver_requires_cmd_seq_after_shown_and_live_binding() {
        use tiler_core::directional::WindowId;
        use tiler_core::world_report::{FocusRequest, FocusRequestCause};
        let target = WindowId("4242_7".to_owned());
        let route = crate::focus_route::FocusRoute::Slps;
        let baselines = || FocusBaselines {
            app_generation: Some(1),
            prior_focus: Some("4242_7".to_owned()),
            prior_frontmost: Some(4242),
        };
        let req = || FocusRequest {
            id: "focus-4".to_owned(),
            target: target.clone(),
            issued_revision: 104,
            cause: FocusRequestCause::ExplicitCommand,
            output: output(),
            workspace: "ws-a".to_owned(),
            cmd_seq: Some(4),
            after_shown: Some(after_shown_for("op-4", 4)),
        };
        let reply_with = |binding: Option<&str>| {
            let mut reply = empty_reply();
            reply.adapter_incarnation = "adapter-1".to_owned();
            reply.core_generation = "gen-1".to_owned();
            if let Some(binding) = binding {
                reply.workspaces = vec![tiler_core::world_report::WorkspaceDesire {
                    id: "ws-a".to_owned(),
                    output: output(),
                    native_binding: Some(binding.to_owned()),
                    scope: tiler_core::world_report::AuthorityScope::Output(output()),
                    order: 0,
                    tiling_mode: tiler_core::world_report::TilingMode::Tiled,
                    existence: true,
                    hold: None,
                    trailing: false,
                }];
            }
            reply
        };
        let resolve = |req: &FocusRequest, reply: &WorldReplyV4, baselines: FocusBaselines| {
            deferred_selection_focus_for_request(req, reply, "adapter-1", "gen-1", route, baselines)
        };
        let resolved = resolve(&req(), &reply_with(Some("space-3")), baselines())
            .expect("correlated request with a live binding resolves");
        assert_eq!(resolved.operation_id, "op-4");
        assert_eq!(resolved.command_seq, 4);
        assert_eq!(resolved.focus_id, "focus-4");
        assert_eq!(resolved.issued_revision, 104);
        assert_eq!(resolved.adapter_incarnation, "adapter-1");
        assert_eq!(resolved.core_generation, "gen-1");
        assert_eq!(resolved.window, target);
        assert_eq!(resolved.output, output());
        assert_eq!(resolved.workspace, "ws-a");
        assert_eq!(resolved.after_shown.op, "op-4");
        assert_eq!(resolved.expected_sid, 3);
        assert_eq!(resolved.app_generation, Some(1));
        assert_eq!(resolved.prior_focus.as_deref(), Some("4242_7"));
        assert_eq!(resolved.prior_frontmost, Some(4242));
        // A non-explicit cause never resolves, even with dependencies.
        let mut other_cause = req();
        other_cause.cause = FocusRequestCause::PositiveClose;
        other_cause.cmd_seq = None;
        assert!(
            resolve(&other_cause, &reply_with(Some("space-3")), baselines()).is_none(),
            "close cause never defers"
        );
        // A missing command sequence never resolves (no guessed correlation).
        let mut no_seq = req();
        no_seq.cmd_seq = None;
        assert!(
            resolve(&no_seq, &reply_with(Some("space-3")), baselines()).is_none(),
            "uncorrelated request never resolves"
        );
        // A missing dependency projection never resolves.
        let mut no_after = req();
        no_after.after_shown = None;
        assert!(resolve(&no_after, &reply_with(Some("space-3")), baselines()).is_none());
        // Missing or unparsable bindings never resolve (no guessed Space).
        // The arrival scope is exactly the request scope: a reply without
        // the request-scope row, with a wrong vocabulary, or with a zero
        // SID never resolves — never `selected.first` from another scope.
        assert!(resolve(&req(), &reply_with(None), baselines()).is_none());
        assert!(resolve(&req(), &reply_with(Some("desktop-3")), baselines()).is_none());
        assert!(
            resolve(&req(), &reply_with(Some("space-0")), baselines()).is_none(),
            "zero SID never resolves"
        );
        // Missing baselines never resolve: without an observed-focus or
        // foreground baseline, non-intervention is unprovable.
        assert!(
            resolve(
                &req(),
                &reply_with(Some("space-3")),
                FocusBaselines {
                    prior_focus: None,
                    ..baselines()
                },
            )
            .is_none()
        );
        assert!(
            resolve(
                &req(),
                &reply_with(Some("space-3")),
                FocusBaselines {
                    prior_frontmost: None,
                    ..baselines()
                },
            )
            .is_none()
        );
    }

    #[test]
    fn non_explicit_request_leaves_deferred_intent_untouched() {
        // Latest/null cancels every cause's unstarted deferred authority:
        // a newer PositiveClose dispatches immediately on the request lane
        // while the pending op-3 intent (a different request id) cancels
        // counted instead of lingering for a later legacy completion.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "focus-5", 5, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert!(
            guard.selection_focus_pending.is_none(),
            "newer non-explicit publication cancels the old deferred intent"
        );
        assert_eq!(guard.selection_focus_floor_seq, 0);
        assert_eq!(guard.diagnostics.focus_dropped_full, 1);
        drop(guard);
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
    }

    #[test]
    fn newer_explicit_after_shown_supersedes_pending_selection_focus() {
        // Supersession through the unified request lane: a newer explicit
        // command with frozen afterShown replaces the older pending intent
        // (op-3) with the bound op-5 intent, counted, without dispatching.
        let mut session = focus_session();
        let shared = focus_shared();
        // Dispatch-time arrival unproven: the mapping for space-3 exists
        // (readable) but currency waits elsewhere, so the bind defers.
        // The mocked context is genuine domain rows at the live revision —
        // never a waiver — and the mocked roster/homing below lets the
        // frozen proof run its strict legs (proven members, proven
        // mapping, unproven currency).
        lock_shared(&shared).native_context = Some(mock_space_context(false));
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        let mut retained = close_retained(9, Some(close_target(7)), true);
        // Strict-leg roster/homing for the frozen member 4242_7 (bound to
        // the live space-3 binding): observed live, homed, unhidden.
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .insert(7, close_roster_entry(7));
        retained.window_domains.insert(
            format!("{CLOSE_PID}_7"),
            tiler_core::session::DomainKey {
                output: output(),
                workspace: tiler_core::directional::WorkspaceId("space-3".to_owned()),
            },
        );
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut reply = focus_reply_for(
            &session,
            "focus-5",
            5,
            7,
            tiler_core::world_report::FocusRequestCause::ExplicitCommand,
            Some(5),
            Some(after_shown_for("op-5", 5)),
        );
        // The issuing observation is incorporated: the watermark gate passes
        // so the strict member/scope legs decide (proven here — only
        // currency defers).
        reply.based_on_observation_seq = 5;
        reply.workspaces = vec![tiler_core::world_report::WorkspaceDesire {
            id: "ws-a".to_owned(),
            output: output(),
            native_binding: Some("space-3".to_owned()),
            scope: tiler_core::world_report::AuthorityScope::Output(output()),
            order: 0,
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: false,
        }];
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert_eq!(
            guard
                .selection_focus_pending
                .as_ref()
                .expect("new intent bound")
                .operation_id,
            "op-5"
        );
        assert_eq!(guard.selection_focus_floor_seq, 5);
        assert_eq!(guard.diagnostics.focus_dropped_full, 1);
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none(), "deferred binds dispatch nothing");
    }

    // Strict frozen-proof harness: genuine roster/homing/context rows at
    // the live revision (never a waiver). Defaults prove everything except
    // currency (space-3 readable but not current): bind defers.
    fn strict_bind_parts() -> StrictBindParts {
        strict_bind_parts_for("focus-9", 9, 9, "op-9")
    }

    /// Same strict harness for an arbitrary selection identity: the frozen
    /// `afterShown` op/obs, request id/revision/seq, and reply watermark all
    /// ride the given values. Lets arrival-proof tests bind the exact tuple
    /// their release legs assert (e.g. op-3) instead of hand-placing it.
    fn strict_bind_parts_for(id: &str, rev: u64, seq: u64, op: &str) -> StrictBindParts {
        let session = focus_session();
        let shared = focus_shared();
        lock_shared(&shared).native_context = Some(mock_space_context(false));
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        let mut retained = close_retained(9, Some(close_target(7)), true);
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .insert(7, close_roster_entry(7));
        retained.window_domains.insert(
            format!("{CLOSE_PID}_7"),
            tiler_core::session::DomainKey {
                output: output(),
                workspace: tiler_core::directional::WorkspaceId("space-3".to_owned()),
            },
        );
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut reply = focus_reply_for(
            &session,
            id,
            rev,
            7,
            tiler_core::world_report::FocusRequestCause::ExplicitCommand,
            Some(seq),
            Some(after_shown_for(op, rev)),
        );
        reply.based_on_observation_seq = rev;
        reply.workspaces = vec![tiler_core::world_report::WorkspaceDesire {
            id: "ws-a".to_owned(),
            output: output(),
            native_binding: Some("space-3".to_owned()),
            scope: tiler_core::world_report::AuthorityScope::Output(output()),
            order: 0,
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: false,
        }];
        (session, retained, shared, focus_queue, reply)
    }

    #[test]
    fn frozen_proof_proven_defers_without_currency() {
        // Strict legs all pass (roster observed, homing, watermark,
        // readable views) except native currency: the intent binds and
        // defers — nothing dispatches, nothing refuses.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert_eq!(
            guard
                .selection_focus_pending
                .as_ref()
                .expect("proven intent bound")
                .operation_id,
            "op-9"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_proof_proven_releases_when_arrived_and_settled() {
        // Same strict bind with space-3 currently arrived, through the
        // genuine owner capture round: the bind defers with no captures yet,
        // one real bracket batch retained plus reevaluation releases exactly
        // once (exactly one queue row plus one slot command, intent retained
        // queued), and the genuine owner completion retires the tuple —
        // never the old select callback.
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let ctx = mock_space_context(true);
        lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
        lock_shared(&shared).native_cell.store(Some(ctx));
        // Open the report flight and accept the reply through the real
        // sequencer path first (the current reply the capture reevaluation
        // reads back): the strict bind below then defers with no captures
        // yet, and the owner round releases from this latest.
        for _ in 0..9 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture round flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture round reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "bind defers without captures"
        );
        assert!(lock_focus(&focus_queue).is_empty(), "defer queues nothing");
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        assert!(
            lock_shared(&shared).arrival_captures.contains_key("4242_7"),
            "completion retains the fresh bracket"
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("release keeps the queued intent");
        assert!(pending.owner_queued, "release marks the intent queued");
        assert!(
            pending.proven_stamp.is_some(),
            "release carries the capture-proven stamp"
        );
        {
            let queued = lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "release queues exactly one row");
            assert_eq!((queued[0].pid, queued[0].wid), (CLOSE_PID, 7));
        }
        assert!(slot.take().is_some(), "release sets one slot command");
        assert_eq!(
            lock_shared(&shared).diagnostics.focus_dispatched,
            2,
            "queue plus slot count"
        );
        complete_owner_focus_result(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &inbox,
            &pending,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "owner completion retires the tuple"
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            0,
            "terminal completion retires its outbox row, queues nothing more"
        );
        assert!(slot.take().is_none(), "completion sets no new slot command");
    }

    #[test]
    fn arrival_capture_gate_releases_once_with_unchanged_facts() {
        // Capture-gate leg through the real arrival path: the strict bind
        // defers with no captures yet, then one scripted owner bracket
        // batch sent as `ArrivalCaptureReady` over a real inbox and run
        // through the real engine completion retains the fresh bracket and
        // releases exactly once with unchanged native facts and no new
        // report. A duplicate capture plus a duplicate consume queue
        // nothing more.
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let ctx = mock_space_context(true);
        lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
        lock_shared(&shared).native_cell.store(Some(ctx));
        for _ in 0..9 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture gate flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture gate reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "strict bind defers without captures"
        );
        assert!(lock_focus(&focus_queue).is_empty(), "defer queues nothing");
        assert!(slot.take().is_none(), "defer sets no slot command");
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        assert!(
            lock_shared(&shared).arrival_captures.contains_key("4242_7"),
            "completion retains the fresh bracket"
        );
        assert_eq!(
            lock_shared(&shared).clock.get(),
            9,
            "capture never bumps the clock"
        );
        assert_eq!(
            lock_shared(&shared).topology_revision,
            9,
            "no new native facts"
        );
        assert_eq!(
            session
                .sequencer()
                .last_reply()
                .expect("accepted reply retained")
                .request_id,
            ticket.request_id,
            "no new report"
        );
        let queued = lock_focus(&focus_queue);
        assert_eq!(queued.len(), 1, "release queues exactly one row");
        assert_eq!((queued[0].pid, queued[0].wid), (CLOSE_PID, 7));
        drop(queued);
        assert!(slot.take().is_some(), "release sets one slot command");
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("release keeps the queued intent");
        assert!(pending.owner_queued, "release marks the intent queued");
        {
            let cell = lock_shared(&shared).native_cell.clone();
            let mut backend = CaptureBackend {
                reads: capture_reads_for_frozen_member(),
            };
            assert!(
                !crate::runtime_worker::maybe_send_arrival_capture(
                    &mut backend,
                    CLOSE_PID,
                    CLOSE_GEN,
                    &shared,
                    &cell,
                    &inbox,
                    &mut last_attempt,
                ),
                "duplicate capture never re-reads"
            );
        }
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "duplicate consume queues nothing"
        );
        assert!(slot.take().is_none(), "duplicate consume sets no slot");
    }

    #[test]
    fn two_owner_captures_converge_once_with_full_peer_setter_feedback() {
        // Two frozen members owned by different PIDs through the real owner
        // capture rounds: the first owner's bracket alone retains but proves
        // nothing (peer missing) — unstamped, no queue, no slot, no setter,
        // no feedback. The second owner's bracket at the SAME live watermark
        // completes the set and releases exactly once. Each owner threads its
        // own `last_attempt` dedupe state — never shared across pids. The
        // dispatching owner's setter then emits the full peer set as truthful
        // feedback (homes, visibility, lifetime, watermark).
        const PEER_PID: u32 = 5000;
        const PEER_GEN: u64 = 1;
        const PEER_WID: u32 = 8;
        let session = focus_session();
        let shared = focus_shared();
        let ctx = mock_space_context(true);
        lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
        lock_shared(&shared).native_cell.store(Some(ctx));
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        live_focus_app(&shared, PEER_PID, PEER_GEN);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        let mut retained = close_retained(9, Some(close_target(9)), true);
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .insert(7, close_roster_entry(7));
        retained.window_domains.insert(
            format!("{CLOSE_PID}_7"),
            tiler_core::session::DomainKey {
                output: output(),
                workspace: tiler_core::directional::WorkspaceId("space-3".to_owned()),
            },
        );
        let mut peer_app = crate::runtime_core::RetainedApp {
            generation: PEER_GEN,
            seq_last: 1,
            cmd_counter: 0,
            windows: Vec::new(),
            focused_wid: None,
            roster: std::collections::BTreeMap::new(),
            roster_incomplete: false,
            roster_cause: String::new(),
        };
        peer_app
            .roster
            .insert(PEER_WID, close_roster_entry(PEER_WID));
        retained.apps.insert(PEER_PID, peer_app);
        retained.window_domains.insert(
            format!("{PEER_PID}_{PEER_WID}"),
            tiler_core::session::DomainKey {
                output: output(),
                workspace: tiler_core::directional::WorkspaceId("space-3".to_owned()),
            },
        );
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let after = tiler_core::world_report::AfterShown {
            op: "op-2pid".to_owned(),
            obs: 3,
            members: vec![
                tiler_core::world_report::AfterShownMember {
                    win: close_target(7),
                    out: output(),
                    ws: "ws-a".to_owned(),
                    frame: None,
                },
                tiler_core::world_report::AfterShownMember {
                    win: tiler_core::directional::WindowId(format!("{PEER_PID}_{PEER_WID}")),
                    out: output(),
                    ws: "ws-a".to_owned(),
                    frame: None,
                },
            ],
            views: vec![tiler_core::world_report::AfterShownScope {
                out: output(),
                ws: "ws-a".to_owned(),
            }],
            selected: vec![tiler_core::world_report::AfterShownScope {
                out: output(),
                ws: "ws-a".to_owned(),
            }],
            departing: Vec::new(),
        };
        let mut session = session;
        let mut reply = focus_reply_for(
            &session,
            "focus-2pid",
            3,
            7,
            tiler_core::world_report::FocusRequestCause::ExplicitCommand,
            Some(3),
            Some(after),
        );
        reply.workspaces = vec![tiler_core::world_report::WorkspaceDesire {
            id: "ws-a".to_owned(),
            output: output(),
            native_binding: Some("space-3".to_owned()),
            scope: tiler_core::world_report::AuthorityScope::Output(output()),
            order: 0,
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: false,
        }];
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("two-owner flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("two-owner reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "two-member intent binds and defers"
        );
        assert!(lock_focus(&focus_queue).is_empty());
        let live_obs = lock_shared(&shared).clock.get();
        assert_eq!(live_obs, 3);
        let reads_owner = vec![crate::runtime_types::BracketRead {
            wid: 7,
            owner_pid: Some(CLOSE_PID),
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces: vec![3],
            frame: Some((0, 0, 100, 100)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }];
        let reads_peer = vec![crate::runtime_types::BracketRead {
            wid: PEER_WID,
            owner_pid: Some(PEER_PID),
            before_minimized: Some(false),
            before_hidden: Some(false),
            spaces: vec![3],
            frame: Some((0, 0, 100, 100)),
            after_minimized: Some(false),
            after_hidden: Some(false),
        }];
        let mut last_owner: Option<ArrivalCaptureAttemptKey> = None;
        let mut last_peer: Option<ArrivalCaptureAttemptKey> = None;
        let inbox_owner = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            reads_owner,
            &mut last_owner,
        );
        assert!(
            lock_shared(&shared).arrival_captures.contains_key("4242_7"),
            "first owner bracket retained"
        );
        assert!(
            !lock_shared(&shared)
                .arrival_captures
                .contains_key(&format!("{PEER_PID}_{PEER_WID}")),
            "peer row still missing"
        );
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("partial set stays bound");
            assert!(
                pending.proven_stamp.is_none(),
                "partial set stays unstamped"
            );
            assert!(!pending.owner_queued);
        }
        assert!(
            lock_focus(&focus_queue).is_empty(),
            "partial set queues nothing"
        );
        assert!(
            session.peek_close_feedback().is_none(),
            "missing peer stages no fabricated feedback"
        );
        drop(inbox_owner);
        let inbox_peer = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            PEER_PID,
            PEER_GEN,
            reads_peer,
            &mut last_peer,
        );
        assert_eq!(
            lock_shared(&shared).clock.get(),
            live_obs,
            "converging capture never bumps the watermark"
        );
        assert!(
            lock_shared(&shared).arrival_captures.contains_key("4242_7"),
            "owner row kept"
        );
        assert!(
            lock_shared(&shared)
                .arrival_captures
                .contains_key(&format!("{PEER_PID}_{PEER_WID}")),
            "peer row retained"
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("full set keeps the queued intent");
        assert!(
            pending.proven_stamp.is_some(),
            "full set carries the capture-proven stamp"
        );
        assert!(pending.owner_queued, "full set marks the intent queued");
        assert_eq!(pending.focus_id, "focus-2pid");
        assert_eq!(pending.operation_id, "op-2pid");
        {
            let queued = lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "converged release queues exactly one row");
            assert_eq!((queued[0].pid, queued[0].wid), (CLOSE_PID, 7));
        }
        // The dispatching owner re-proves the full peer set before the
        // setter: bound pre is the 4242_9 observed focus, so a fresh read of
        // (4242, 9) proceeds toward the 4242_7 target with a post of
        // (4242, 7).
        let released_cmd = {
            let guard = lock_shared(&shared);
            guard
                .apps
                .get(&CLOSE_PID)
                .expect("app")
                .focus
                .take()
                .expect("owner slot carries the release")
        };
        assert_eq!(released_cmd.window, "4242_7");
        let released_close = released_cmd.close.clone().expect("unified close route");
        assert_eq!(released_close.expected_sid, Some(3));
        let revision = lock_shared(&shared).topology_revision;
        let mut genuine = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(9))),
            post: Some((CLOSE_PID, Some(7))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut genuine,
            &released_cmd,
            &released_close,
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(genuine.dispatches, 1, "full set dispatches once");
        let arrival = match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                arrival,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                arrival.clone()
            }
            _other => panic!("expected CloseFocusResult"),
        };
        assert_eq!(arrival.len(), 2, "setter emits the full peer set");
        for row in &arrival {
            assert!(
                row.window.0 == "4242_7" || row.window.0 == format!("{PEER_PID}_{PEER_WID}"),
                "row names a frozen member"
            );
            assert_eq!(row.obs, live_obs, "row carries the live watermark");
            for sample in [&row.before, &row.after] {
                assert_eq!(sample.visibility.visible, Some(true));
                assert_eq!(sample.lifetime_matched, Some(true));
            }
            assert_eq!(
                row.frame,
                Some(tiler_core::geometry::Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100
                }),
                "row carries the readable live frame"
            );
            match &row.location {
                tiler_core::world_report::ArrivalLocation::Native { home } => match home {
                    Some(tiler_core::world_report::WorkspaceRef::Native {
                        output: home_output,
                        ..
                    }) => assert_eq!(*home_output, output()),
                    other => panic!("expected native home, got {other:?}"),
                },
                other => panic!("expected native arm, got {other:?}"),
            }
        }
        crate::runtime_thread::process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox_peer,
            &focus_queue,
            8,
        );
        let staged = session
            .peek_close_feedback()
            .expect("owner setter stages truthful feedback");
        assert_eq!(staged.id, "focus-2pid");
        assert_eq!(staged.disposition, accepted_disposition());
        assert_eq!(
            staged.arrival.len(),
            2,
            "feedback carries the full peer set"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "owner completion retires the tuple"
        );
        assert_eq!(
            lock_focus(&focus_queue).len(),
            0,
            "terminal completion retires its outbox row, queues nothing more"
        );
    }

    #[test]
    fn null_frozen_frame_unreadable_defers_then_readable_recovers_same_request() {
        // Null frozen frame waives equality but never readability: a null
        // live frame defers with no setter and no feedback. A newer
        // observation then permits one fresh readable capture under the same
        // immutable request tuple, which recovers and releases exactly once.
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("null-frame flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("null-frame reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(lock_shared(&shared).selection_focus_pending.is_some());
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let mut null_reads = capture_reads_for_frozen_member();
        null_reads[0].frame = None;
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            null_reads,
            &mut last_attempt,
        );
        let null_cap = lock_shared(&shared)
            .arrival_captures
            .get("4242_7")
            .cloned()
            .expect("null-frame bracket retained");
        assert!(null_cap.frame.is_none(), "unreadable live frame recorded");
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("null frame stays bound");
            assert!(pending.proven_stamp.is_none(), "null frame stays unstamped");
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none(), "null frame sets no slot command");
        assert!(
            session.peek_close_feedback().is_none(),
            "unreadable frame stages no feedback"
        );
        drop(inbox);
        let before = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("intent");
        lock_shared(&shared).clock.invalidate();
        let fresh_obs = lock_shared(&shared).clock.get();
        assert_eq!(fresh_obs, 4);
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("readable capture keeps the queued intent");
        assert_eq!(pending.focus_id, before.focus_id);
        assert_eq!(pending.issued_revision, before.issued_revision);
        assert_eq!(pending.adapter_incarnation, before.adapter_incarnation);
        assert_eq!(pending.core_generation, before.core_generation);
        assert_eq!(pending.command_seq, before.command_seq);
        assert_eq!(pending.window, before.window);
        assert_eq!(pending.operation_id, before.operation_id);
        assert_eq!(pending.after_shown, before.after_shown);
        let stamp = pending.proven_stamp.expect("readable capture stamps");
        assert_eq!(stamp.obs, fresh_obs);
        assert!(pending.owner_queued);
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        drop(inbox);
    }

    #[test]
    fn exact_frozen_frame_mismatch_capture_stays_deferred() {
        // The frozen frame is exact but the live frame disagrees: the
        // readable capture is retained yet the proof still defers — mismatch
        // alone never spends into a refusal or a release.
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        let req = reply.focus_request.as_mut().expect("request");
        let after = req.after_shown.as_mut().expect("after");
        after.members[0].frame = Some(tiler_core::geometry::Rect {
            x: 10,
            y: 10,
            w: 50,
            h: 50,
        });
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("mismatch flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("mismatch reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(lock_shared(&shared).selection_focus_pending.is_some());
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let _inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        assert!(
            lock_shared(&shared).arrival_captures.contains_key("4242_7"),
            "mismatched capture retained"
        );
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("mismatch stays bound");
            assert!(pending.proven_stamp.is_none(), "mismatch stays unstamped");
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(
            session.peek_close_feedback().is_none(),
            "mismatch stages no feedback"
        );
    }

    #[test]
    fn frozen_frame_mismatch_defers_never_refuses() {
        // The roster live frame disagrees with the frozen desire frame: the
        // capture may be stale, so the intent binds and defers — mismatch
        // alone never spends into a refusal no-call.
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        let req = reply.focus_request.as_mut().expect("request");
        let after = req.after_shown.as_mut().expect("after");
        after.members[0].frame = Some(Rect {
            x: 10,
            y: 10,
            w: 50,
            h: 50,
        });
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert!(
            guard.selection_focus_pending.is_some(),
            "mismatch defers, never refuses"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
        assert_eq!(guard.diagnostics.focus_dropped_full, 0);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_unobserved_member_defers() {
        // No readable live rect for the frozen member: null waives equality
        // but never readability — defer, never refuse.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .get_mut(&7)
            .expect("row")
            .observed = None;
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "unobserved member defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_missing_roster_defers_never_refuses() {
        // Absence proves nothing either way (late admission may still
        // arrive): an empty-windows skeleton runs the same retained-truth
        // legs, and the missing row defers — never a waiver, never a
        // refusal.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        assert!(reply.windows.is_empty(), "skeleton carries no rows");
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .expect("app")
            .roster
            .remove(&7);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "missing roster defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_home_mismatch_refuses_terminal() {
        // Pre-existing retained-truth rule: the retained home disagrees
        // with the frozen home — terminal refusal, no intent bound.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        retained.window_domains.insert(
            format!("{CLOSE_PID}_7"),
            tiler_core::session::DomainKey {
                output: output(),
                workspace: tiler_core::directional::WorkspaceId("space-7".to_owned()),
            },
        );
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert!(
            guard.selection_focus_pending.is_none(),
            "home mismatch refuses"
        );
        assert_eq!(guard.diagnostics.focus_dropped_full, 1);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty());
        let feedback = session.peek_close_feedback().expect("refusal row staged");
        assert_eq!(
            feedback.disposition,
            not_called_disposition(tiler_core::world_report::FocusNoCallReason::ForegroundUnproven)
        );
    }

    #[test]
    fn frozen_hold_defers() {
        // A held frozen scope withholds: defer for a later publication,
        // never refuse.
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        reply.workspaces[0].hold = Some(tiler_core::world_report::HoldReason::Unreadable);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "held scope defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_stale_watermark_defers() {
        // The reply base predates the issuing observation: this publication
        // cannot prove the issue — defer for a newer one.
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        reply.based_on_observation_seq = 8;
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "stale base defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_unreadable_view_defers() {
        // The frozen selected scope resolves to an unreadable native
        // domain: binding-time existence fails — defer, never refuse.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        {
            let mut guard = lock_shared(&shared);
            let ctx = guard.native_context.as_mut().expect("mocked context");
            let ctx_mut = Arc::get_mut(ctx).expect("sole owner");
            ctx_mut.domains[0].readable = false;
        }
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "unreadable view defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn frozen_absent_context_defers_never_waives() {
        // No native context proves no mapping and no arrival: the proven
        // members still bind, but the intent only defers — release stays
        // impossible until a context arrives.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        lock_shared(&shared).native_context = None;
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "absent context defers"
        );
        assert!(session.peek_close_feedback().is_none(), "no refusal row");
        assert!(lock_focus(&focus_queue).is_empty());
    }

    #[test]
    fn duplicate_spent_same_id_reevaluates_and_releases_once() {
        // Same-id latest reevaluation through the genuine owner capture
        // round: the issuing publication binds and defers (no currency, no
        // captures); one real bracket batch retained plus reevaluation
        // releases exactly once (intent retained queued); the genuine owner
        // completion retires the tuple; a third identical consume finds
        // nothing (the spent envelope only reevaluates, never re-binds).
        let (mut session, mut retained, shared, focus_queue, mut reply) = strict_bind_parts();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let ctx = mock_space_context(true);
        lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
        lock_shared(&shared).native_cell.store(Some(ctx));
        // Open the report flight and accept the reply through the real
        // sequencer path first (the current reply the capture reevaluation
        // reads back): the issuing publication below then binds and defers
        // with no captures yet.
        for _ in 0..9 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture round flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture round reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "issuing publication defers"
        );
        assert!(lock_focus(&focus_queue).is_empty());
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("release keeps the queued intent");
        assert!(pending.owner_queued, "release marks the intent queued");
        assert_eq!(lock_focus(&focus_queue).len(), 1, "exactly one release");
        assert!(slot.take().is_some(), "release sets one slot command");
        complete_owner_focus_result(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &inbox,
            &pending,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "owner completion retires the tuple"
        );
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
            0,
            "no replay: the retired row stays retired across duplicate consume"
        );
        assert!(
            slot.take().is_none(),
            "duplicate consume sets no slot command"
        );
    }

    #[test]
    fn explicit_focus_supersedes_and_dispatches_immediately() {
        // A bare explicit command supersedes any deferred selection intent
        // plus the floor, then dispatches at once on the request lane.
        let mut session = focus_session();
        let mut retained = close_retained(7, Some(close_target(8)), true);
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        lock_shared(&shared).selection_focus_pending = Some(pending_op3());
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = focus_reply_for(
            &session,
            "focus-7",
            7,
            7,
            tiler_core::world_report::FocusRequestCause::ExplicitCommand,
            Some(7),
            None,
        );
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        assert!(guard.selection_focus_pending.is_none());
        assert_eq!(guard.selection_focus_floor_seq, 7);
        // Immediate dispatch on the unified lane: exactly one queue row.
        assert_eq!(guard.diagnostics.focus_dispatched, 1);
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
        // Async arrival through the genuine owner capture round: the op-3
        // intent binds and defers (currency unproven); one real bracket
        // batch retained stamps the proof but withholds the release while
        // the destination has not arrived. Nothing fires before the verified
        // post; the first matching completion then releases to the queue
        // plus the owner slot, and a repeat completion finds nothing.
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = lock_shared(&shared)
                .native_context
                .clone()
                .expect("strict harness publishes a context");
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        // Accept through the real sequencer path first (the current reply
        // the capture reevaluation reads back), at the frozen `after.obs`.
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture round flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture round reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "bind defers without captures"
        );
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let _inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        // Stamped but withheld: space-3 is readable, not current.
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("capture stamps the deferred intent");
        assert!(
            pending.proven_stamp.is_some(),
            "capture-proven stamp rides the intent"
        );
        assert!(
            !pending.owner_queued,
            "unarrived destination queues nothing yet"
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        // The destination arrives: swap the live native view to current
        // without touching the clock, revision, or cell generation, so the
        // capture-proven stamp stays live under the production liveness
        // rule — the release below proves against genuine arrival rows.
        lock_shared(&shared).native_context = Some(mock_space_context(true));
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
            cmd.expect_prior, None,
            "unified route carries no legacy prior gate"
        );
        let close = cmd
            .close
            .expect("release dispatches through the unified route");
        assert_eq!(close.close_id, "focus-3");
        assert_eq!(
            close.target,
            tiler_core::directional::WindowId("4242_7".to_owned())
        );
        assert_eq!(close.issued_revision, 3);
        assert_eq!(close.cmd_seq, Some(3));
        assert_eq!(
            close.after_shown.as_ref().map(|after| after.op.as_str()),
            Some("op-3")
        );
        assert_eq!(
            close.fallback_pre,
            Some(tiler_core::directional::WindowId("4242_7".to_owned())),
            "bind-time observed focus rides as the owner fallback"
        );
        assert!(close.fallback_known);
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
    #[ignore]
    fn deferred_release_binds_focus_join_through_shared_op_map() {
        // Enabled-only (`TILER_MACOS_STAGE_TIMING=1`): the release runs on
        // the main thread where no engine dispatch span is ever installed,
        // so the FOCUS_BIND plus `note_focus` must join through the shared
        // op map under the completion identity - carrying the original
        // intake origin (`t0_us`), never a follow-up span. The `note_op`
        // below stands in for the engine dispatch bind; the release then
        // proves the FocusResult join (`trace_for_focus` hits with the
        // dispatch origin).
        if !crate::stage_timing::enabled() {
            eprintln!("skipped without TILER_MACOS_STAGE_TIMING=1");
            return;
        }
        let shared = focus_shared();
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        lock_shared(&shared).focus_evidence = Some(crate::runtime_types::FocusEvidence {
            frontmost_pid: Some(4242),
            frontmost_bundle: None,
            ax_focus: None,
        });
        let pending = pending_op3();
        // Simulate the engine dispatch bind: op seq 3 carries the trace id
        // with its intake origin (the release must reuse both, never a
        // follow-up span - none is installed on this thread).
        crate::stage_timing::note_op(3, 777_001, 1_000_000);
        lock_shared(&shared).selection_focus_pending = Some(pending.clone());
        publish_pending_epoch(&shared, &pending);
        stamp_pending_live(&shared);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                Some(3),
                Some(4242)
            ),
            SelectionFocusRelease::Released,
            "verified arrival releases"
        );
        let (trace, t0_us, _) = crate::stage_timing::trace_for_focus(CLOSE_PID, CLOSE_GEN, 3)
            .expect("focus join bound");
        assert_eq!(trace, 777_001, "bind carries the dispatch trace");
        assert_eq!(t0_us, 1_000_000, "bind reuses the intake origin");
    }

    #[test]
    fn owner_rechecks_stamped_expected_home_before_setter() {
        // Replacement-capture guard through the real capture pipeline: a
        // genuine owner round releases with the stamped expected Space, then
        // a replacement capture naming another Space under the same obs/cell
        // stamp — mapped by `capture_member_brackets` from a real
        // `BracketRead`, never hand-built — withholds the owner row. The
        // count gate then fails and the owner defers internally with no
        // setter and no feedback, never a cross-Space dispatch. The genuine
        // row first proves the control dispatches with the exact home.
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture round flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture round reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let _inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let released_cmd = slot.take().expect("owner slot carries the release");
        let released_close = released_cmd.close.clone().expect("unified close route");
        assert_eq!(
            released_close.expected_sid,
            Some(3),
            "release stamps the expected Space into the owner binding"
        );
        // Retarget to a fresh window so the baseline gate proceeds instead
        // of shorting as already-satisfied; the arrival rows still prove the
        // frozen member.
        let owner_cmd = |binding: crate::runtime_types::CloseFocusBinding| {
            let mut binding = binding;
            binding.target = close_target(9);
            crate::runtime_types::FocusCommand {
                pid: CLOSE_PID,
                generation: CLOSE_GEN,
                request_id: 3,
                wid: 9,
                window: format!("{CLOSE_PID}_9"),
                revision: 9,
                route: crate::focus_route::FocusRoute::Slps,
                expect_prior: None,
                close: Some(binding),
            }
        };
        let revision = lock_shared(&shared).topology_revision;
        let cmd = owner_cmd(released_close.clone());
        let mut genuine = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut genuine,
            &cmd,
            cmd.close.as_ref().expect("close"),
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(genuine.dispatches, 1, "genuine row dispatches once");
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                arrival,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(arrival.len(), 1, "exactly the stamped home row");
                match &arrival[0].location {
                    tiler_core::world_report::ArrivalLocation::Native { home } => match home {
                        Some(tiler_core::world_report::WorkspaceRef::Native {
                            output: home_output,
                            ..
                        }) => {
                            assert_eq!(*home_output, output(), "home output names the frozen scope")
                        }
                        other => panic!("expected native home, got {other:?}"),
                    },
                    other => panic!("expected native arm, got {other:?}"),
                }
            }
            _other => panic!("expected CloseFocusResult"),
        }
        // Replacement capture through the real bracket mapper: same tuple,
        // same obs/revision/cell stamp, singleton home on Space 7 instead of
        // the stamped Space 3. The engine retains it (exact tuple, live
        // watermark); the owner must still withhold the row.
        let genuine_cap = lock_shared(&shared)
            .arrival_captures
            .get("4242_7")
            .cloned()
            .expect("genuine bracket retained");
        let members = released_close
            .after_shown
            .as_ref()
            .expect("afterShown")
            .members
            .clone();
        let mut wrong_reads = capture_reads_for_frozen_member();
        wrong_reads[0].spaces = vec![7];
        let replacement = crate::runtime_worker::capture_member_brackets(
            &wrong_reads,
            &members,
            CLOSE_PID,
            CLOSE_GEN,
            genuine_cap.revision,
            genuine_cap.obs,
            genuine_cap.obs,
            genuine_cap.context_generation,
            genuine_cap.context_generation,
            &released_close.close_id,
            released_close.issued_revision,
            &released_close.adapter_incarnation,
            &released_close.core_generation,
        );
        assert_eq!(
            replacement.len(),
            1,
            "real mapper keeps the wrong-Space row"
        );
        assert_eq!(
            replacement[0].home_sid,
            Some(7),
            "replacement names another Space"
        );
        lock_shared(&shared).arrival_captures.insert(
            "4242_7".to_owned(),
            replacement.into_iter().next().expect("row"),
        );
        let cmd = owner_cmd(released_close.clone());
        let mut replaced = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut replaced,
            &cmd,
            cmd.close.as_ref().expect("close"),
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(
            replaced.dispatches, 0,
            "replacement home performs no setter call"
        );
        match &job {
            crate::runtime_thread::EngineJob::SelectionFocusDeferred { .. } => {}
            _ => panic!("expected SelectionFocusDeferred"),
        }
        assert!(
            session.peek_close_feedback().is_none(),
            "deferral stages no feedback"
        );
    }

    #[test]
    fn owner_rechecks_live_domain_output_before_setter() {
        // Moved-display guard: the genuine capture still names the stamped
        // Space, but the live domain for that Space moved to another output
        // (same revision, same cell generation, so the proof stamp stays
        // live). The owner withholds the row and defers internally — never a
        // setter on the wrong display.
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("capture round flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("capture round reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let _inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let released_cmd = slot.take().expect("owner slot carries the release");
        let released_close = released_cmd.close.clone().expect("unified close route");
        // Move Space 3 to another display without touching the revision, so
        // the store keeps the cell generation and the proof stamp stays live:
        // only the output leg changes.
        let moved = {
            let ctx = lock_shared(&shared)
                .native_context
                .clone()
                .expect("context");
            let mut domains = ctx.domains.clone();
            for domain in domains.iter_mut() {
                if domain.space == 3 {
                    domain.output = tiler_core::directional::OutputId("macos-display-2".to_owned());
                }
            }
            Arc::new(crate::runtime_types::NativeContext {
                revision: ctx.revision,
                domains,
                skipped: Vec::new(),
            })
        };
        lock_shared(&shared).native_context = Some(Arc::clone(&moved));
        lock_shared(&shared).native_cell.store(Some(moved));
        let mut binding = released_close.clone();
        binding.target = close_target(9);
        let cmd = crate::runtime_types::FocusCommand {
            pid: CLOSE_PID,
            generation: CLOSE_GEN,
            request_id: 3,
            wid: 9,
            window: format!("{CLOSE_PID}_9"),
            revision: 9,
            route: crate::focus_route::FocusRoute::Slps,
            expect_prior: None,
            close: Some(binding),
        };
        let revision = lock_shared(&shared).topology_revision;
        let mut moved_backend = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut moved_backend,
            &cmd,
            cmd.close.as_ref().expect("close"),
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(
            moved_backend.dispatches, 0,
            "moved display performs no setter call"
        );
        match &job {
            crate::runtime_thread::EngineJob::SelectionFocusDeferred { .. } => {}
            _ => panic!("expected SelectionFocusDeferred"),
        }
        assert!(
            session.peek_close_feedback().is_none(),
            "deferral stages no feedback"
        );
    }

    #[test]
    fn frozen_proof_defers_when_live_domain_output_moves() {
        // Engine-side output leg: the reply still binds `ws-a` to Space 3,
        // but the live domain for Space 3 names another display under the
        // live revision. The frozen proof defers (binds unstamped, never
        // refuses) instead of proving arrival on the wrong display.
        let (mut session, mut retained, shared, focus_queue, reply) = strict_bind_parts();
        let moved = {
            let ctx = lock_shared(&shared)
                .native_context
                .clone()
                .expect("strict harness publishes a context");
            let mut domains = ctx.domains.clone();
            for domain in domains.iter_mut() {
                domain.current = true;
                if domain.space == 3 {
                    domain.output = tiler_core::directional::OutputId("macos-display-2".to_owned());
                }
            }
            Arc::new(crate::runtime_types::NativeContext {
                revision: ctx.revision,
                domains,
                skipped: Vec::new(),
            })
        };
        lock_shared(&shared).native_context = Some(moved);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        let guard = lock_shared(&shared);
        let pending = guard
            .selection_focus_pending
            .as_ref()
            .expect("moved display still binds the intent");
        assert_eq!(pending.operation_id, "op-9");
        assert!(
            pending.proven_stamp.is_none(),
            "output move leaves the intent unstamped"
        );
        assert_eq!(guard.diagnostics.focus_dispatched, 0);
        drop(guard);
        assert!(lock_focus(&focus_queue).is_empty(), "defer queues nothing");
        assert!(
            session.peek_close_feedback().is_none(),
            "deferral is not a refusal"
        );
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
            // Genuine arrival rows for the cases that reach the arrival
            // recheck (outbox-full, releases): space-3 current at the live
            // revision. Earlier fences drop before the recheck, unaffected.
            lock_shared(&shared).native_context = Some(mock_space_context(true));
            let slot = live_focus_app(&shared, 4242, 1);
            if terminate {
                lock_shared(&shared)
                    .apps
                    .get_mut(&4242)
                    .expect("app")
                    .terminated = true;
            }
            publish_pending_epoch(&shared, &pending);
            lock_shared(&shared).selection_focus_pending = Some(pending);
            lock_shared(&shared).selection_focus_floor_seq = floor;
            configure(&shared);
            // Direct completions carry the bind-issued live stamp the way
            // the bind/reevaluate legs issue one; cases that reconfigure
            // the epoch/floor/native state afterwards still exercise their
            // own terminal fence on a live stamp.
            stamp_pending_live(&shared);
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
        // Moved epoch (null/newer publication) cancels the taken intent:
        // a newer request published after the bind invalidates the old
        // tuple, so the late completion drops instead of actuating old
        // work through the legacy path.
        let (outcome, shared, queue, slot) = release_with(
            pending_op3(),
            Some(3),
            Some(4242),
            0,
            false,
            &|shared: &Arc<Mutex<Shared>>| {
                crate::runtime_shared::publish_close_focus_current(
                    shared,
                    Some(crate::runtime_shared::CloseFocusEpoch {
                        id: "focus-9".to_owned(),
                        revision: 9,
                        adapter_incarnation: "adapter-1".to_owned(),
                        core_generation: "gen-1".to_owned(),
                    }),
                );
            },
        );
        assert_eq!(outcome, SelectionFocusRelease::Dropped);
        assert!(lock_focus(&queue).is_empty());
        assert!(slot.take().is_none());
        assert_eq!(lock_shared(&shared).diagnostics.stale_dropped, 1);
    }

    #[test]
    fn terminal_release_retires_exact_tuple_without_retry() {
        // Terminal pre-queue fences retire the exact still-current tuple:
        // the intent is gone (never a retry), a repeat completion reports
        // NoPending, and nothing queues. Covers wrong-Space, stale app, and
        // outbox backpressure drops.
        for (post, frontmost, _floor, terminate, cap) in [
            (Some(9), Some(4242), 0u64, false, usize::MAX),
            (Some(3), Some(4242), 0u64, true, usize::MAX),
            (Some(3), Some(4242), 0u64, false, 0usize),
        ] {
            let shared = focus_shared();
            lock_shared(&shared).native_context = Some(mock_space_context(true));
            let slot = live_focus_app(&shared, 4242, 1);
            if terminate {
                lock_shared(&shared)
                    .apps
                    .get_mut(&4242)
                    .expect("app")
                    .terminated = true;
            }
            if cap == 0 {
                lock_shared(&shared).config.focus_outbox_depth = 0;
            }
            let pending = pending_op3();
            publish_pending_epoch(&shared, &pending);
            lock_shared(&shared).selection_focus_pending = Some(pending);
            stamp_pending_live(&shared);
            let focus_queue = Arc::new(Mutex::new(Vec::new()));
            assert_eq!(
                release_selection_focus_after_arrival(
                    &shared,
                    &focus_queue,
                    Some("op-3"),
                    post,
                    frontmost,
                ),
                SelectionFocusRelease::Dropped,
            );
            assert!(
                lock_shared(&shared).selection_focus_pending.is_none(),
                "terminal drop retires the tuple"
            );
            assert!(lock_focus(&focus_queue).is_empty());
            assert!(slot.take().is_none());
            // No retry: a repeat completion finds nothing and queues nothing.
            assert_eq!(
                release_selection_focus_after_arrival(
                    &shared,
                    &focus_queue,
                    Some("op-3"),
                    post,
                    frontmost,
                ),
                SelectionFocusRelease::NoPending,
            );
            assert!(lock_focus(&focus_queue).is_empty());
        }
    }

    #[test]
    fn stopped_slot_unwind_removes_only_its_row_and_retires() {
        // A stopped owner slot is terminal: the release unwinds exactly its
        // own outbox row (a blind pop could steal another operation's row),
        // retires the tuple, and never retries.
        let shared = focus_shared();
        lock_shared(&shared).native_context = Some(mock_space_context(true));
        let slot = live_focus_app(&shared, 4242, 1);
        let pending = pending_op3();
        publish_pending_epoch(&shared, &pending);
        lock_shared(&shared).selection_focus_pending = Some(pending);
        stamp_pending_live(&shared);
        let focus_queue = Arc::new(Mutex::new(vec![FocusRequest {
            pid: 9,
            generation: 9,
            request_id: 99,
            close_id: "focus-99".to_owned(),
            wid: 9,
            window: "9_9".to_owned(),
            dispatched: true,
            verified: false,
            topology_revision: 9,
        }]));
        slot.stop();
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                Some(3),
                Some(4242),
            ),
            SelectionFocusRelease::Dropped,
        );
        {
            let queued = lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "only the foreign row survives");
            assert_eq!(queued[0].request_id, 99);
            assert_eq!(queued[0].window, "9_9");
        }
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "stopped slot retires the tuple"
        );
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &focus_queue,
                Some("op-3"),
                Some(3),
                Some(4242),
            ),
            SelectionFocusRelease::NoPending,
            "no retry behind a dead owner"
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
    }

    #[test]
    fn stale_completion_preserves_newer_intent() {
        // A late completion for superseded work reports NoPending and never
        // touches the newer bound intent.
        let shared = focus_shared();
        live_focus_app(&shared, 4242, 1);
        let mut newer = pending_op3();
        newer.operation_id = "op-9".to_owned();
        newer.focus_id = "focus-9".to_owned();
        newer.issued_revision = 9;
        newer.command_seq = 9;
        publish_pending_epoch(&shared, &newer);
        lock_shared(&shared).selection_focus_pending = Some(newer);
        assert_eq!(
            release_selection_focus_after_arrival(
                &shared,
                &Arc::new(Mutex::new(Vec::new())),
                Some("op-3"),
                Some(3),
                Some(4242)
            ),
            SelectionFocusRelease::NoPending,
        );
        let guard = lock_shared(&shared);
        assert_eq!(
            guard
                .selection_focus_pending
                .as_ref()
                .map(|pending| pending.operation_id.clone()),
            Some("op-9".to_owned()),
            "newer intent preserved"
        );
    }

    #[test]
    fn owner_deferral_resets_mark_without_feedback_and_stays_retryable() {
        // Internal unstarted deferral (owner pre-setter gate withholds):
        // the engine resets the queue mark plus the stamp, retains the deps
        // and baselines, and stages no FocusFeedback — evidence refresh can
        // queue the same unstarted work again.
        let mut session = focus_session();
        let mut retained = close_retained(7, Some(close_target(8)), true);
        let shared = focus_shared();
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut pending = pending_op3();
        pending.operation_id = "op-7".to_owned();
        pending.command_seq = 7;
        pending.focus_id = "close-focus-7".to_owned();
        pending.issued_revision = 7;
        pending.window = close_target(7);
        pending.after_shown = after_shown_for("op-7", 7);
        pending.prior_focus = Some(format!("{CLOSE_PID}_8"));
        pending.owner_queued = true;
        publish_pending_epoch(&shared, &pending);
        lock_shared(&shared).selection_focus_pending = Some(pending);
        stamp_pending_live(&shared);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let inbox = Arc::new(Inbox::new(16));
        crate::runtime_thread::process_job(
            crate::runtime_thread::EngineJob::SelectionFocusDeferred {
                pid: CLOSE_PID,
                generation: CLOSE_GEN,
                focus_id: "close-focus-7".to_owned(),
                issued_revision: 7,
                adapter_incarnation: "adapter-1".to_owned(),
                core_generation: "gen-1".to_owned(),
                cmd_id: 7,
                target: close_target(7),
            },
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        let guard = lock_shared(&shared);
        let pending = guard
            .selection_focus_pending
            .as_ref()
            .expect("deferral retains the intent for retry");
        assert!(!pending.owner_queued, "queue mark reset");
        assert!(pending.proven_stamp.is_none(), "stamp reset");
        assert_eq!(pending.operation_id, "op-7", "deps retained");
        drop(guard);
        assert!(
            session.peek_close_feedback().is_none(),
            "no false feedback for unstarted deferral"
        );
        assert!(lock_focus(&focus_queue).is_empty());
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

    /// Bind one strict focus-3 round at the live watermark through the real
    /// sequencer path: the caller gets the bound pending intent with no
    /// captures yet, plus the slot and a fresh dedupe key. The reply is the
    /// synthetic strict fixture, but every proof below runs through the real
    /// owner capture path (`maybe_send_arrival_capture` over a real inbox,
    /// `Inbox::recv`, real `process_job`).
    fn bind_focus3_arrival_ready() -> Focus3BindParts {
        let (mut session, mut retained, shared, focus_queue, mut reply) =
            strict_bind_parts_for("focus-3", 3, 3, "op-3");
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        {
            let ctx = mock_space_context(true);
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            lock_shared(&shared).native_cell.store(Some(ctx));
        }
        for _ in 0..3 {
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        let ticket = session
            .sequencer()
            .try_begin_request(None)
            .expect("arrival flight");
        reply.request_id = ticket.request_id.clone();
        reply.based_on_observation_seq = ticket.observation_seq;
        reply.ready = true;
        let applied = match session.sequencer().accept_reply(&reply) {
            ReplyVerdict::Applied(applied) => applied,
            ReplyVerdict::Refused { reason } => {
                panic!("arrival reply must apply, got {reason}")
            }
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &applied,
            None,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_some(),
            "strict bind defers without captures"
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        (session, retained, shared, focus_queue, slot, *applied)
    }

    #[test]
    fn fresh_capture_mid_read_observation_move_withholds_then_recovers() {
        use crate::runtime_backend::AxBackend;
        struct MovingClockBackend {
            shared: Arc<Mutex<Shared>>,
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl AxBackend for MovingClockBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                lock_shared(&self.shared).clock.invalidate();
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let (mut session, mut retained, shared, focus_queue, slot, _) = bind_focus3_arrival_ready();
        let inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let mut backend = MovingClockBackend {
            shared: Arc::clone(&shared),
            reads: capture_reads_for_frozen_member(),
        };
        assert!(
            !crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                CLOSE_PID,
                CLOSE_GEN,
                &shared,
                &cell,
                &inbox,
                &mut last_attempt,
            ),
            "moved watermark around the read sends nothing"
        );
        assert!(
            last_attempt.is_some(),
            "moved attempt still records the key"
        );
        let stop = AtomicBool::new(true);
        assert!(
            inbox.recv(&stop).is_none(),
            "no notification queued for the moved read"
        );
        assert!(
            lock_shared(&shared).arrival_captures.is_empty(),
            "moved read retains nothing"
        );
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("moved read keeps the intent bound");
            assert!(pending.proven_stamp.is_none());
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(
            session.peek_close_feedback().is_none(),
            "moved read stages no feedback"
        );
        // Genuine fresh bracket at the new watermark recovers the same
        // immutable tuple exactly once.
        let before = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("intent");
        let fresh_obs = lock_shared(&shared).clock.get();
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("fresh capture keeps the queued intent");
        assert_eq!(pending.focus_id, before.focus_id);
        assert_eq!(pending.issued_revision, before.issued_revision);
        assert_eq!(pending.adapter_incarnation, before.adapter_incarnation);
        assert_eq!(pending.core_generation, before.core_generation);
        assert_eq!(pending.command_seq, before.command_seq);
        assert_eq!(pending.window, before.window);
        assert_eq!(pending.operation_id, before.operation_id);
        assert_eq!(pending.after_shown, before.after_shown);
        let stamp = pending.proven_stamp.expect("fresh capture stamps");
        assert_eq!(stamp.obs, fresh_obs);
        assert!(pending.owner_queued);
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        drop(inbox);
    }

    #[test]
    fn fresh_capture_mid_read_cell_republish_same_revision_withholds_then_recovers() {
        use crate::runtime_backend::AxBackend;
        struct RepublishCellBackend {
            shared: Arc<Mutex<Shared>>,
            ctx: Arc<crate::runtime_types::NativeContext>,
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl AxBackend for RepublishCellBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                // Clear plus same-revision republish: the revision is
                // unchanged but the cell generation moves, so the bracket is
                // evidence about a different native epoch.
                let cell = lock_shared(&self.shared).native_cell.clone();
                cell.store(None);
                cell.store(Some(Arc::clone(&self.ctx)));
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let (mut session, mut retained, shared, focus_queue, slot, _) = bind_focus3_arrival_ready();
        let live_ctx = lock_shared(&shared)
            .native_cell
            .load_versioned()
            .0
            .expect("bound cell holds the live context");
        assert_eq!(live_ctx.revision, 9);
        let inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let gen_before = cell.generation();
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let mut backend = RepublishCellBackend {
            shared: Arc::clone(&shared),
            ctx: live_ctx,
            reads: capture_reads_for_frozen_member(),
        };
        assert!(
            !crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                CLOSE_PID,
                CLOSE_GEN,
                &shared,
                &cell,
                &inbox,
                &mut last_attempt,
            ),
            "clear+republish around the read sends nothing"
        );
        assert!(
            cell.generation() > gen_before,
            "same-revision clear+republish still bumps the cell generation"
        );
        assert_eq!(
            lock_shared(&shared).topology_revision,
            9,
            "revision itself never moved"
        );
        let stop = AtomicBool::new(true);
        assert!(inbox.recv(&stop).is_none(), "no notification queued");
        assert!(lock_shared(&shared).arrival_captures.is_empty());
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("cell move keeps the intent bound");
            assert!(pending.proven_stamp.is_none());
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        let before = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("intent");
        let fresh_gen = cell.generation();
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("fresh capture keeps the queued intent");
        assert_eq!(pending.focus_id, before.focus_id);
        assert_eq!(pending.after_shown, before.after_shown);
        let stamp = pending.proven_stamp.expect("fresh capture stamps");
        assert_eq!(stamp.context_generation, fresh_gen);
        assert!(pending.owner_queued);
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        drop(inbox);
    }

    #[test]
    fn fresh_capture_attestation_mismatch_withholds_then_recovers() {
        use crate::runtime_backend::AxBackend;
        struct ReadsBackend {
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl AxBackend for ReadsBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let (mut session, mut retained, shared, focus_queue, slot, _) = bind_focus3_arrival_ready();
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        // Wrong-owner attestation: the backend attests another PID's window.
        let mut wrong_owner = capture_reads_for_frozen_member();
        wrong_owner[0].owner_pid = Some(9999);
        // Unattested legs: no owner ever claimed the window around the read.
        let mut unattested = capture_reads_for_frozen_member();
        unattested[0].owner_pid = None;
        // Reused-WID shape: the batch names a window the frozen member never
        // owned, so the member has no read at all.
        let mut wrong_wid = capture_reads_for_frozen_member();
        wrong_wid[0].wid = 8;
        for (label, reads) in [
            ("wrong owner", wrong_owner),
            ("unattested", unattested),
            ("reused wid", wrong_wid),
        ] {
            let inbox = Arc::new(Inbox::new(16));
            let mut backend = ReadsBackend { reads };
            assert!(
                !crate::runtime_worker::maybe_send_arrival_capture(
                    &mut backend,
                    CLOSE_PID,
                    CLOSE_GEN,
                    &shared,
                    &cell,
                    &inbox,
                    &mut last_attempt,
                ),
                "{label} sends no notification"
            );
            let stop = AtomicBool::new(true);
            assert!(
                inbox.recv(&stop).is_none(),
                "{label} queues no notification"
            );
            // Each withheld shape records its key, so the next shape needs a
            // new watermark to attempt again.
            lock_shared(&shared).clock.invalidate();
            session.sequencer().invalidate();
        }
        assert!(lock_shared(&shared).arrival_captures.is_empty());
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("mismatches keep the intent bound");
            assert!(pending.proven_stamp.is_none());
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        let before = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("intent");
        let fresh_obs = lock_shared(&shared).clock.get();
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("good bracket recovers the same tuple");
        assert_eq!(pending.focus_id, before.focus_id);
        assert_eq!(pending.after_shown, before.after_shown);
        assert_eq!(
            pending.proven_stamp.expect("recovery stamps").obs,
            fresh_obs
        );
        assert!(pending.owner_queued);
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        drop(inbox);
    }

    #[test]
    fn fresh_capture_mid_read_lifetime_replacement_retires_without_rebind() {
        use crate::runtime_backend::AxBackend;
        struct ReplaceAppBackend {
            shared: Arc<Mutex<Shared>>,
            reads: Vec<crate::runtime_types::BracketRead>,
        }
        impl AxBackend for ReplaceAppBackend {
            fn apply_frame(
                &mut self,
                _cmd: &crate::runtime::FrameCommand,
            ) -> crate::runtime_types::AxCompletion {
                crate::runtime_types::AxCompletion::Failed {
                    reason: "capture never actuates".to_owned(),
                }
            }
            fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
                Ok(Vec::new())
            }
            fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
                // Lifetime replacement racing the native read: the PID now
                // names a new generation before the bracket is mapped.
                if let Some(record) = lock_shared(&self.shared).apps.get_mut(&CLOSE_PID) {
                    record.generation = 2;
                }
                self.reads
                    .iter()
                    .filter(|read| wids.contains(&read.wid))
                    .cloned()
                    .collect()
            }
        }
        let (mut session, mut retained, shared, focus_queue, slot, _) = bind_focus3_arrival_ready();
        let inbox = Arc::new(Inbox::new(16));
        let cell = lock_shared(&shared).native_cell.clone();
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let mut backend = ReplaceAppBackend {
            shared: Arc::clone(&shared),
            reads: capture_reads_for_frozen_member(),
        };
        // The watermark never moved, so the worker still notifies; the
        // engine completion must drop the old-lifetime rows.
        assert!(
            crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                CLOSE_PID,
                CLOSE_GEN,
                &shared,
                &cell,
                &inbox,
                &mut last_attempt,
            ),
            "unchanged watermark still notifies"
        );
        assert_eq!(
            lock_shared(&shared)
                .apps
                .get(&CLOSE_PID)
                .expect("app")
                .generation,
            2,
            "replacement raced the read"
        );
        let stop = AtomicBool::new(false);
        let job = inbox.recv(&stop).expect("old-lifetime notification queued");
        crate::runtime_thread::process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            lock_shared(&shared).arrival_captures.is_empty(),
            "old-lifetime rows retain nothing"
        );
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("replacement keeps the intent until release drops it");
            assert!(pending.proven_stamp.is_none());
            assert!(!pending.owner_queued);
        }
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(session.peek_close_feedback().is_none());
        // The same immutable tuple under the new lifetime is not a recovery:
        // the release gate retires the original instead of rebinding it.
        let inbox = Arc::new(Inbox::new(16));
        let mut backend = ReadsForRecovery {
            reads: capture_reads_for_frozen_member(),
        };
        assert!(
            crate::runtime_worker::maybe_send_arrival_capture(
                &mut backend,
                CLOSE_PID,
                2,
                &shared,
                &cell,
                &inbox,
                &mut last_attempt,
            ),
            "new lifetime notifies once under its own key"
        );
        let stop = AtomicBool::new(false);
        let job = inbox.recv(&stop).expect("new-lifetime notification queued");
        crate::runtime_thread::process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "lifetime replacement retires the original without rebinding"
        );
        assert!(lock_focus(&focus_queue).is_empty());
        assert!(slot.take().is_none());
        assert!(
            session.peek_close_feedback().is_none(),
            "retirement stages no feedback"
        );
    }

    #[test]
    fn stale_queued_stamp_defers_on_setter_invalidation_then_recovers_with_one_setter() {
        // Real capture stamps and queues the intent; a clock plus cell move
        // immediately before the setter makes the owner defer internally.
        // Processing that deferral resets the mark and stamp while retaining
        // the immutable deps, with no feedback; one fresh bracket at the new
        // watermark then recovers with exactly one setter dispatch.
        let (mut session, mut retained, shared, focus_queue, slot, _) = bind_focus3_arrival_ready();
        // Steer the fallback baseline off the target so the owner setter
        // reaches its proof gate: fallback 4242_8 with target 4242_7 proceeds
        // on a fresh 4242_8 read instead of shorting as already-satisfied.
        lock_shared(&shared)
            .selection_focus_pending
            .as_mut()
            .expect("bound intent")
            .prior_focus = Some(format!("{CLOSE_PID}_8"));
        let mut last_attempt: Option<ArrivalCaptureAttemptKey> = None;
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let queued = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("real capture queues the intent");
        assert!(queued.owner_queued);
        assert!(queued.proven_stamp.is_some());
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        let released_cmd = slot.take().expect("owner slot carries the release");
        let released_close = released_cmd.close.clone().expect("unified close route");
        // Setter-time invalidation: the stamp the release just proved no
        // longer names live state.
        lock_shared(&shared).clock.invalidate();
        lock_shared(&shared).native_cell.request_reread();
        let mut no_setter = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(8))),
            post: Some((CLOSE_PID, Some(7))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let revision = lock_shared(&shared).topology_revision;
        let deferred = crate::runtime_worker::handle_close_focus_command(
            &mut no_setter,
            &released_cmd,
            &released_close,
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(
            no_setter.dispatches, 0,
            "stale stamp performs no setter call"
        );
        match &deferred {
            crate::runtime_thread::EngineJob::SelectionFocusDeferred {
                pid,
                generation,
                focus_id,
                issued_revision,
                cmd_id,
                target,
                ..
            } => {
                assert_eq!(*pid, CLOSE_PID);
                assert_eq!(*generation, CLOSE_GEN);
                assert_eq!(focus_id, &queued.focus_id);
                assert_eq!(*issued_revision, queued.issued_revision);
                assert_eq!(*cmd_id, queued.command_seq);
                assert_eq!(target, &queued.window);
            }
            _other => panic!("expected SelectionFocusDeferred"),
        }
        assert!(session.peek_close_feedback().is_none());
        // Engine deferral: the exact still-current queued tuple resets its
        // mark and stamp but keeps every immutable dep for retry.
        crate::runtime_thread::process_job(
            deferred,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        {
            let guard = lock_shared(&shared);
            let pending = guard
                .selection_focus_pending
                .as_ref()
                .expect("deferral retains the intent for retry");
            assert!(!pending.owner_queued, "queue mark reset");
            assert!(pending.proven_stamp.is_none(), "stamp reset");
            assert_eq!(pending.operation_id, queued.operation_id);
            assert_eq!(pending.focus_id, queued.focus_id);
            assert_eq!(pending.issued_revision, queued.issued_revision);
            assert_eq!(pending.adapter_incarnation, queued.adapter_incarnation);
            assert_eq!(pending.core_generation, queued.core_generation);
            assert_eq!(pending.command_seq, queued.command_seq);
            assert_eq!(pending.window, queued.window);
            assert_eq!(pending.after_shown, queued.after_shown);
            assert_eq!(pending.app_generation, queued.app_generation);
            assert_eq!(pending.prior_focus, queued.prior_focus);
            assert_eq!(pending.prior_frontmost, queued.prior_frontmost);
        }
        assert!(session.peek_close_feedback().is_none());
        // Fresh evidence at the new watermark recovers the same tuple with
        // exactly one setter dispatch and one truthful completion.
        let fresh_obs = lock_shared(&shared).clock.get();
        let fresh_gen = lock_shared(&shared).native_cell.generation();
        let queue_before = lock_focus(&focus_queue).len();
        let inbox = drive_arrival_capture_once(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            CLOSE_PID,
            CLOSE_GEN,
            capture_reads_for_frozen_member(),
            &mut last_attempt,
        );
        let pending = lock_shared(&shared)
            .selection_focus_pending
            .clone()
            .expect("fresh evidence re-queues the same intent");
        assert_eq!(pending.focus_id, queued.focus_id);
        assert_eq!(pending.after_shown, queued.after_shown);
        let stamp = pending.proven_stamp.expect("recovery re-stamps");
        assert_eq!(stamp.obs, fresh_obs);
        assert_eq!(stamp.context_generation, fresh_gen);
        assert!(pending.owner_queued);
        // Latest-only recovery replaces the deferred row instead of
        // duplicating it: the re-release purges the same-pid superseded row
        // before recording the replacement, so the count is unchanged and
        // the single terminal completion retires exactly that row.
        assert_eq!(lock_focus(&focus_queue).len(), queue_before);
        let recovered_cmd = slot.take().expect("recovery sets one slot command");
        let recovered_close = recovered_cmd.close.clone().expect("unified route");
        let mut genuine = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(8))),
            post: Some((CLOSE_PID, Some(7))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job = crate::runtime_worker::handle_close_focus_command(
            &mut genuine,
            &recovered_cmd,
            &recovered_close,
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(genuine.dispatches, 1, "new evidence performs one setter");
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                arrival,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(arrival.len(), 1);
            }
            _other => panic!("expected CloseFocusResult"),
        }
        crate::runtime_thread::process_job(
            job,
            &mut session,
            &mut retained,
            &shared,
            &inbox,
            &focus_queue,
            8,
        );
        assert!(
            session.peek_close_feedback().is_some(),
            "truthful setter stages feedback"
        );
        assert!(
            lock_shared(&shared).selection_focus_pending.is_none(),
            "setter completion retires the tuple"
        );
    }

    struct ReadsForRecovery {
        reads: Vec<crate::runtime_types::BracketRead>,
    }

    impl crate::runtime_backend::AxBackend for ReadsForRecovery {
        fn apply_frame(
            &mut self,
            _cmd: &crate::runtime::FrameCommand,
        ) -> crate::runtime_types::AxCompletion {
            crate::runtime_types::AxCompletion::Failed {
                reason: "capture never actuates".to_owned(),
            }
        }
        fn snapshot(&mut self) -> Result<Vec<crate::runtime::ObservedWindow>, String> {
            Ok(Vec::new())
        }
        fn capture_brackets(&mut self, wids: &[u32]) -> Vec<crate::runtime_types::BracketRead> {
            self.reads
                .iter()
                .filter(|read| wids.contains(&read.wid))
                .cloned()
                .collect()
        }
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
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let checked = tiler_protocol::world_admission::handle_report_checked(
            &mut engine,
            &report(1, true),
            &[],
        );
        let controlled = match checked.outcome {
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
        let checked = tiler_protocol::world_admission::handle_report_checked(
            &mut engine,
            &report(2, false),
            &[],
        );
        let held = match checked.outcome {
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
        let checked = tiler_protocol::world_admission::handle_report_checked(
            &mut engine,
            &report(3, true),
            &[],
        );
        let recovered = match checked.outcome {
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

    #[test]
    fn indicator_revision_advances_on_changed_desires_and_holds_on_duplicates() {
        use tiler_core::world_report::{TilingMode, WorkspaceDesire, workspace_authority_scope};
        let shared = focus_shared();
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
                0,
            );
        };
        let reply = |tiling: TilingMode| {
            let out = output();
            let mut reply = empty_reply();
            reply.workspaces = vec![WorkspaceDesire {
                id: "ws-a".to_owned(),
                output: out.clone(),
                native_binding: Some("space-3".to_owned()),
                scope: workspace_authority_scope(&out, "ws-a"),
                order: 0,
                tiling_mode: tiling,
                existence: true,
                hold: None,
                trailing: false,
            }];
            reply
        };
        assert_eq!(
            lock_shared(&shared).indicator_revision,
            0,
            "no applied reply has published yet"
        );
        // First applied reply publishes desires: advances once.
        dispatch(&mut session, &mut retained, &reply(TilingMode::Tiled));
        assert_eq!(lock_shared(&shared).indicator_revision, 1);
        // Identical duplicate (same desires, same pager): steady.
        dispatch(&mut session, &mut retained, &reply(TilingMode::Tiled));
        assert_eq!(
            lock_shared(&shared).indicator_revision,
            1,
            "identical applied replies never rebump the indicator revision"
        );
        // A toggle reply flips the retained planner mode: advances again.
        dispatch(&mut session, &mut retained, &reply(TilingMode::Floating));
        assert_eq!(lock_shared(&shared).indicator_revision, 2);
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
                parent: None,
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
        focus_reply_for(
            session,
            id,
            rev,
            target_wid,
            tiler_core::world_report::FocusRequestCause::PositiveClose,
            None,
            None,
        )
    }

    fn focus_reply_for(
        session: &WorldSession,
        id: &str,
        rev: u64,
        target_wid: u32,
        cause: tiler_core::world_report::FocusRequestCause,
        cmd_seq: Option<u64>,
        after_shown: Option<tiler_core::world_report::AfterShown>,
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
        reply.focus_request = Some(tiler_core::world_report::FocusRequest {
            id: id.to_owned(),
            target: close_target(target_wid),
            issued_revision: rev,
            cause,
            output: output(),
            workspace: "ws-a".to_owned(),
            cmd_seq,
            after_shown,
        });
        reply
    }

    fn after_shown_for(op: &str, obs: u64) -> tiler_core::world_report::AfterShown {
        tiler_core::world_report::AfterShown {
            op: op.to_owned(),
            obs,
            members: vec![tiler_core::world_report::AfterShownMember {
                win: close_target(7),
                out: output(),
                ws: "ws-a".to_owned(),
                frame: None,
            }],
            views: vec![tiler_core::world_report::AfterShownScope {
                out: output(),
                ws: "ws-a".to_owned(),
            }],
            selected: vec![tiler_core::world_report::AfterShownScope {
                out: output(),
                ws: "ws-a".to_owned(),
            }],
            departing: Vec::new(),
        }
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
            0,
        );
    }

    #[test]
    fn retained_focus_does_not_reapply_automatic_fullscreen_policy() {
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        // Model adapter-only retained inventory: the accepted core reply owns
        // policy, so a sibling from that inventory cannot veto it again.
        let mut sibling = close_roster_entry(8).observed.expect("sibling frame");
        sibling.fullscreen = true;
        retained
            .apps
            .get_mut(&CLOSE_PID)
            .unwrap()
            .windows
            .push(sibling);
        retained
            .window_domains
            .insert(close_target(8).0, close_domain());
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
        assert_eq!(slot.take().expect("approved request reaches owner").wid, 9);
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
        reply.focus_request.as_mut().expect("req").target =
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
            staged.disposition,
            not_called_disposition(tiler_core::world_report::FocusNoCallReason::IdentityStale)
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
            staged.disposition,
            not_called_disposition(tiler_core::world_report::FocusNoCallReason::NoReason)
        );
        assert_eq!(staged.pre, Some(close_target(9)));
        assert_eq!(staged.post, Some(close_target(9)));
    }

    #[test]
    fn command_ticket_no_longer_cancels_null_retires() {
        // Command tickets never cancel: a focus ticket alongside the reply
        // still dispatches the retained request (queue plus owner slot, epoch
        // published). A later null publication retires the epoch and queues
        // nothing new.
        let mut session = focus_session();
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(9, Some(close_target(7)), true);
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let reply = close_reply_for(&session, "close-focus-7", 7, 9);
        let ticket = tiler_core::world_report::WorldCommand::Focus {
            window: tiler_core::directional::WindowSubject::Window(close_target(8)),
            direction: Some(tiler_core::directional::Direction::Left),
        };
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            Some(&ticket),
        );
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(slot.take().is_some());
        assert!(lock_shared(&shared).close_focus_current.is_some());
        // Null retires the epoch.
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
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "null retires without queueing"
        );
        assert!(slot.take().is_none());
        assert!(lock_shared(&shared).close_focus_current.is_none());
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
                close_id: format!("close-focus-{i}"),
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
            staged.disposition,
            not_called_disposition(tiler_core::world_report::FocusNoCallReason::Superseded)
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
            staged2.disposition,
            not_called_disposition(tiler_core::world_report::FocusNoCallReason::IdentityStale)
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
    fn close_same_pid_purge_keeps_replacement_and_stale_retire_is_noop() {
        // Cross-lane numeric collision: the selection lane mints the queued
        // numeric id from the core command seq while the close lane mints it
        // from a local focus seq, so both lanes count 1,2,3... on the same
        // window. The same-pid purge must keep the replacement row, and a
        // late terminal result for the purged row must not retire the live
        // replacement that reused the number. The full triple
        // (request_id, window, close_id) names the exact row.
        use crate::world_dispatch::retire_focus_request_exact;
        let shared = focus_shared();
        live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut session = focus_session();
        let mut retained = close_retained(9, Some(close_target(7)), true);
        retained.focus_seq = 4;
        let focus_queue = Arc::new(Mutex::new(vec![FocusRequest {
            pid: CLOSE_PID,
            generation: CLOSE_GEN,
            request_id: 5,
            close_id: "focus-5".to_owned(),
            wid: 9,
            window: format!("{CLOSE_PID}_9"),
            dispatched: true,
            verified: false,
            topology_revision: 9,
        }]));
        let reply = close_reply_for(&session, "close-focus-9", 7, 9);
        consume_close(
            &mut session,
            &mut retained,
            &shared,
            &focus_queue,
            &reply,
            None,
        );
        {
            let queued = lock_focus(&focus_queue);
            assert_eq!(queued.len(), 1, "purge keeps only the replacement");
            assert_eq!(queued[0].request_id, 5);
            assert_eq!(queued[0].close_id, "close-focus-9");
        }
        // Stale retire for the purged foreign-namespace row is a no-op.
        retire_focus_request_exact(&focus_queue, 5, &format!("{CLOSE_PID}_9"), "focus-5");
        assert_eq!(
            lock_focus(&focus_queue).len(),
            1,
            "stale close id must not retire the replacement"
        );
        // Exact retire for the replacement removes it.
        retire_focus_request_exact(&focus_queue, 5, &format!("{CLOSE_PID}_9"), "close-focus-9");
        assert!(lock_focus(&focus_queue).is_empty());
        // Direct single-remove collision check: a wrong close id survives.
        lock_focus(&focus_queue).push(FocusRequest {
            pid: CLOSE_PID,
            generation: CLOSE_GEN,
            request_id: 5,
            close_id: "focus-X".to_owned(),
            wid: 9,
            window: format!("{CLOSE_PID}_9"),
            dispatched: true,
            verified: false,
            topology_revision: 9,
        });
        assert!(!remove_focus_request_exact(
            &mut lock_focus(&focus_queue),
            5,
            &format!("{CLOSE_PID}_9"),
            "other-close",
        ));
        assert_eq!(lock_focus(&focus_queue).len(), 1);
        assert!(remove_focus_request_exact(
            &mut lock_focus(&focus_queue),
            5,
            &format!("{CLOSE_PID}_9"),
            "focus-X",
        ));
        assert!(lock_focus(&focus_queue).is_empty());
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
            disposition: tiler_core::world_report::FocusDisposition::NotCalled {
                reason: tiler_core::world_report::FocusNoCallReason::IdentityStale,
            },
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
            disposition: tiler_core::world_report::FocusDisposition::NotCalled {
                reason: tiler_core::world_report::FocusNoCallReason::Superseded,
            },
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
            FocusRequestCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
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
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let accepted = |engine: &mut WorldEngine, report: &WorldReportV4| {
            match tiler_protocol::world_admission::handle_report_checked(engine, report, &[])
                .outcome
            {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            }
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
        let req = closed.focus_request.clone().expect("close mints");
        assert_eq!(req.target.0, "win-c");
        assert_eq!(req.cause, FocusRequestCause::PositiveClose);
        // Real v4 byte-codec round-trip of the latest reply.
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                roster_plan: None,
                reply: closed.clone(),
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.focus_request, Some(req.clone()));
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
            FocusRequestCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
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
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let accepted = |engine: &mut WorldEngine, report: &WorldReportV4| {
            match tiler_protocol::world_admission::handle_report_checked(engine, report, &[])
                .outcome
            {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            }
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
        let req = closed.focus_request.clone().expect("close mints");
        assert_eq!(req.target.0, "4242_9");
        assert_eq!(req.cause, FocusRequestCause::PositiveClose);
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
                retained.focus_request,
                Some(req.clone()),
                "unchanged baseline retains the identical request"
            );
        }
        let latest = accepted(
            &mut engine,
            &report(&incarnation, &generation, 10, survivors, vec![], "4242_7"),
        );
        assert_eq!(
            latest.focus_request,
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
                roster_plan: None,
                reply: latest.clone(),
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.focus_request, Some(req.clone()));
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
                disposition,
                pre,
                post,
                foreground_proven,
                attempt_ms,
                close_id,
                target,
                issued_rev,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(*post, Some(close_target(9)));
                assert!(*foreground_proven);
                assert_eq!(*attempt_ms, 3);
                assert_eq!(close_id, &req.id);
                assert_eq!(*issued_rev, req.issued_revision);
                assert_eq!(*target, close_target(9));
            }
            _other => panic!("expected CloseFocusResult"),
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
        assert_eq!(staged.disposition, accepted_disposition());
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
            satisfied.focus_request, None,
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
            let checked = tiler_protocol::world_admission::handle_report_checked(
                session2.engine_mut(),
                &rep,
                &[],
            );
            match checked.outcome {
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
        let checked = tiler_protocol::world_admission::handle_report_checked(
            session2.engine_mut(),
            &close_rep,
            &[],
        );
        let closed2 = match checked.outcome {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let req2 = closed2.focus_request.clone().expect("close mints");
        assert_eq!(req2.target.0, "4242_9");
        let wire2 = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                roster_plan: None,
                reply: closed2.clone(),
            },
        )
        .expect("latest reply encodes");
        let back2 =
            tiler_protocol::world_protocol::decode_reply_v4(&wire2).expect("latest decodes");
        assert_eq!(back2.reply.focus_request, Some(req2.clone()));
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
                disposition,
                pre,
                post,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(
                    *post,
                    Some(close_target(7)),
                    "post unchanged baseline stays honest"
                );
            }
            _other => panic!("expected CloseFocusResult"),
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
        let checked = tiler_protocol::world_admission::handle_report_checked(
            session2.engine_mut(),
            &retain_rep,
            &[],
        );
        let retained_outcome = match checked.outcome {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        assert_eq!(
            retained_outcome.focus_request,
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
                disposition,
                pre,
                post,
                ..
            } => {
                assert_eq!(
                    *disposition,
                    not_called_disposition(
                        tiler_core::world_report::FocusNoCallReason::ForegroundUnproven
                    )
                );
                assert_eq!(*pre, Some(close_target(7)));
                assert!(post.is_none());
            }
            _other => panic!("expected CloseFocusResult"),
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
                disposition,
                pre,
                post,
                ..
            } => {
                assert_eq!(
                    *disposition,
                    not_called_disposition(
                        tiler_core::world_report::FocusNoCallReason::ForegroundChanged
                    )
                );
                assert_eq!(*pre, Some(close_target(7)));
                assert_eq!(*post, Some(close_target(5)));
            }
            _other => panic!("expected CloseFocusResult"),
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
            crate::runtime_thread::EngineJob::CloseFocusResult { disposition, .. } => {
                assert_eq!(
                    *disposition,
                    not_called_disposition(tiler_core::world_report::FocusNoCallReason::Superseded)
                );
            }
            _other => panic!("expected CloseFocusResult"),
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
            crate::runtime_thread::EngineJob::CloseFocusResult { disposition, .. } => {
                assert_eq!(
                    *disposition,
                    not_called_disposition(tiler_core::world_report::FocusNoCallReason::Superseded)
                );
            }
            _other => panic!("expected CloseFocusResult"),
        }
    }

    #[test]
    fn close_owner_unstamped_after_shown_defers_bare_dispatches() {
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
        let revision = lock_shared(&shared).topology_revision;
        let mut unstamped = cmd.close.clone().expect("typed close");
        unstamped.after_shown = Some(after_shown_for("op-3", 3));
        unstamped.proven_stamp = None;
        let mut deferred = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job_d = crate::runtime_worker::handle_close_focus_command(
            &mut deferred,
            &cmd,
            &unstamped,
            CLOSE_PID,
            CLOSE_GEN,
            revision,
            &shared,
        );
        assert_eq!(deferred.dispatches, 0, "unstamped afterShown defers");
        match &job_d {
            crate::runtime_thread::EngineJob::SelectionFocusDeferred { .. } => {}
            _ => panic!("expected SelectionFocusDeferred"),
        }
        let close = cmd.close.clone().expect("typed close");
        assert!(
            close.after_shown.is_none(),
            "bare close carries no afterShown"
        );
        let mut bare = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(9))),
            reads: 0,
            dispatches: 0,
            completion: crate::runtime_types::AxCompletion::Applied { elapsed_ms: 1 },
        };
        let job_b = crate::runtime_worker::handle_close_focus_command(
            &mut bare, &cmd, &close, CLOSE_PID, CLOSE_GEN, revision, &shared,
        );
        assert_eq!(bare.dispatches, 1, "bare close dispatches once");
        match &job_b {
            crate::runtime_thread::EngineJob::CloseFocusResult { .. } => {}
            _ => panic!("expected CloseFocusResult"),
        }
    }

    #[test]
    fn explicit_direct_core_producer_codec_latest_mock_actuator() {
        use tiler_core::directional::WindowId as CoreWindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            FocusRequestCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
            ScreenReport, WindowFact, WorldCommand, WorldReportV4,
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
                native_workspace: NativeWorkspaceFact::Value("space-11".to_owned()),
                host_hidden: Some(false),
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
            }
        }
        fn report(
            incarnation: &str,
            generation: &str,
            seq: u64,
            windows: Vec<WindowFact>,
            focused: &str,
            command_seq: Option<u64>,
            command: Option<WorldCommand>,
        ) -> WorldReportV4 {
            WorldReportV4 {
                contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
                adapter_incarnation: incarnation.to_owned(),
                core_generation: generation.to_owned(),
                observation_seq: seq,
                request_seq: seq,
                request_id: format!("req-{seq}"),
                command_seq,
                command,
                screens: vec![screen("macos-display-1", 0)],
                held_screens: Vec::new(),
                windows,
                held_windows: Vec::new(),
                held_native: Vec::new(),
                native: vec![tiler_core::world_report::NativeObservation {
                    output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
                    native_order: vec!["space-11".to_owned()],
                    current_native: Some("space-11".to_owned()),
                    occupancy: vec![tiler_core::world_report::NativeOccupancy {
                        native: "space-11".to_owned(),
                        count: None,
                    }],
                }],
                closed: Vec::new(),
                cursor: None,
                cursor_hit: None,
                focused_global: focused.to_owned(),
                foreground_known: true,
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let accepted = |engine: &mut WorldEngine, report: &WorldReportV4| {
            match tiler_protocol::world_admission::handle_report_checked(engine, report, &[])
                .outcome
            {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            }
        };
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                1,
                vec![fact("4242_7", "macos-display-1", 0)],
                "4242_7",
                None,
                None,
            ),
        );
        // Genuine explicit direct focus producer: a Focus command for 4242_8
        // mints an ExplicitCommand request through the real engine. The
        // same-report newcomer loses under explicit > admission precedence.
        let issuing = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                2,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                ],
                "4242_7",
                Some(2),
                Some(WorldCommand::Focus {
                    window: tiler_core::directional::WindowSubject::Window(CoreWindowId(
                        "4242_8".to_owned(),
                    )),
                    direction: None,
                }),
            ),
        );
        let req = issuing.focus_request.clone().expect("explicit mints");
        assert_eq!(req.target.0, "4242_8");
        assert_eq!(req.cause, FocusRequestCause::ExplicitCommand);
        assert_eq!(req.cmd_seq, Some(2));
        assert_eq!(req.issued_revision, issuing.state_revision);
        // SKIP issuing plus two unchanged-baseline publications: every one
        // re-emits the identical retained request core-side.
        let survivors = vec![
            fact("4242_7", "macos-display-1", 0),
            fact("4242_8", "macos-display-1", 100),
        ];
        for seq in [3u64, 4u64] {
            let retained = accepted(
                &mut engine,
                &report(
                    &incarnation,
                    &generation,
                    seq,
                    survivors.clone(),
                    "4242_7",
                    None,
                    None,
                ),
            );
            assert_eq!(
                retained.focus_request,
                Some(req.clone()),
                "unchanged baseline retains the identical request"
            );
        }
        // APPLY ONLY the final retained publication.
        let latest = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                5,
                survivors,
                "4242_7",
                None,
                None,
            ),
        );
        assert_eq!(
            latest.focus_request,
            Some(req.clone()),
            "final publication still carries the issuing request"
        );
        assert!(
            latest.state_revision > issuing.state_revision,
            "applied publication is newer than the issuing publication"
        );
        assert_eq!(latest.based_on_observation_seq, 5);
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                roster_plan: None,
                reply: latest.clone(),
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.focus_request, Some(req.clone()));
        assert_eq!(back.reply.adapter_incarnation, incarnation);
        assert_eq!(back.reply.core_generation, generation);
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(8, Some(close_target(7)), true);
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
        assert_eq!(cmd.window, format!("{CLOSE_PID}_8"));
        let revision = lock_shared(&shared).topology_revision;
        let close = cmd.close.clone().expect("typed close");
        let mut fake = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(8))),
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
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                target,
                issued_rev,
                close_id,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(close_id, &req.id);
                assert_eq!(*issued_rev, req.issued_revision);
                assert_eq!(*target, close_target(8));
            }
            _other => panic!("expected CloseFocusResult"),
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
        assert_eq!(staged.disposition, accepted_disposition());
        // Duplicate latest replays nothing new: no second slot command, no
        // second owner dispatch, first staged row keeps its stamp.
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
        // Truthful target observation satisfies: the target reports as the
        // global focus next, so the core retires the lane and the production
        // consume of that null publication retires the adapter epoch too.
        // (explicit leg)
        let satisfied = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                6,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                ],
                "4242_8",
                None,
                None,
            ),
        );
        assert_eq!(
            satisfied.focus_request, None,
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
    }

    #[test]
    fn admission_core_producer_codec_latest_mock_actuator() {
        use tiler_core::directional::WindowId as CoreWindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            FocusRequestCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
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
                native_workspace: NativeWorkspaceFact::Value("space-11".to_owned()),
                host_hidden: Some(false),
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
            }
        }
        fn report(
            incarnation: &str,
            generation: &str,
            seq: u64,
            windows: Vec<WindowFact>,
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
                native: vec![tiler_core::world_report::NativeObservation {
                    output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
                    native_order: vec!["space-11".to_owned()],
                    current_native: Some("space-11".to_owned()),
                    occupancy: vec![tiler_core::world_report::NativeOccupancy {
                        native: "space-11".to_owned(),
                        count: None,
                    }],
                }],
                closed: Vec::new(),
                cursor: None,
                cursor_hit: None,
                focused_global: focused.to_owned(),
                foreground_known: true,
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let accepted = |engine: &mut WorldEngine, report: &WorldReportV4| {
            match tiler_protocol::world_admission::handle_report_checked(engine, report, &[])
                .outcome
            {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            }
        };
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                1,
                vec![fact("4242_7", "macos-display-1", 0)],
                "4242_7",
            ),
        );
        // Genuine automatic-admission producer: newcomer 4242_8 arrives
        // while native stays on 4242_7, minting through the real engine.
        let issuing = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                2,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                ],
                "4242_7",
            ),
        );
        let req = issuing.focus_request.clone().expect("admission mints");
        assert_eq!(req.target.0, "4242_8");
        assert_eq!(req.cause, FocusRequestCause::AutomaticAdmission);
        assert_eq!(req.issued_revision, issuing.state_revision);
        // SKIP issuing plus two unchanged-baseline publications: every one
        // re-emits the identical retained request core-side.
        let survivors = vec![
            fact("4242_7", "macos-display-1", 0),
            fact("4242_8", "macos-display-1", 100),
        ];
        for seq in [3u64, 4u64] {
            let retained = accepted(
                &mut engine,
                &report(&incarnation, &generation, seq, survivors.clone(), "4242_7"),
            );
            assert_eq!(
                retained.focus_request,
                Some(req.clone()),
                "unchanged baseline retains the identical request"
            );
        }
        // APPLY ONLY the final retained publication.
        let latest = accepted(
            &mut engine,
            &report(&incarnation, &generation, 5, survivors, "4242_7"),
        );
        assert_eq!(
            latest.focus_request,
            Some(req.clone()),
            "final publication still carries the issuing request"
        );
        assert!(
            latest.state_revision > issuing.state_revision,
            "applied publication is newer than the issuing publication"
        );
        assert_eq!(latest.based_on_observation_seq, 5);
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                roster_plan: None,
                reply: latest.clone(),
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.focus_request, Some(req.clone()));
        assert_eq!(back.reply.adapter_incarnation, incarnation);
        assert_eq!(back.reply.core_generation, generation);
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(8, Some(close_target(7)), true);
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
        assert_eq!(cmd.window, format!("{CLOSE_PID}_8"));
        let revision = lock_shared(&shared).topology_revision;
        let close = cmd.close.clone().expect("typed close");
        let mut fake = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(8))),
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
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                target,
                issued_rev,
                close_id,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(close_id, &req.id);
                assert_eq!(*issued_rev, req.issued_revision);
                assert_eq!(*target, close_target(8));
            }
            _other => panic!("expected CloseFocusResult"),
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
        assert_eq!(staged.disposition, accepted_disposition());
        // Duplicate latest replays nothing new: no second slot command, no
        // second owner dispatch, first staged row keeps its stamp.
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
        // Truthful target observation satisfies: the target reports as the
        // global focus next, so the core retires the lane and the production
        // consume of that null publication retires the adapter epoch too.
        let satisfied = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                6,
                vec![
                    fact("4242_7", "macos-display-1", 0),
                    fact("4242_8", "macos-display-1", 100),
                ],
                "4242_8",
            ),
        );
        assert_eq!(
            satisfied.focus_request, None,
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
    }

    #[test]
    fn hover_core_producer_codec_latest_mock_actuator() {
        use tiler_core::directional::WindowId as CoreWindowId;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            FocusRequestCause, NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4,
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
                native_workspace: NativeWorkspaceFact::Value("space-11".to_owned()),
                host_hidden: Some(false),
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
            }
        }
        fn report(
            incarnation: &str,
            generation: &str,
            seq: u64,
            windows: Vec<WindowFact>,
            focused: &str,
            cursor: Option<(i32, i32)>,
            cursor_hit: Option<&str>,
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
                native: vec![tiler_core::world_report::NativeObservation {
                    output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
                    native_order: vec!["space-11".to_owned()],
                    current_native: Some("space-11".to_owned()),
                    occupancy: vec![tiler_core::world_report::NativeOccupancy {
                        native: "space-11".to_owned(),
                        count: None,
                    }],
                }],
                closed: Vec::new(),
                cursor,
                cursor_hit: cursor_hit.map(|hit| CoreWindowId(hit.to_owned())),
                focused_global: focused.to_owned(),
                foreground_known: true,
                focus_changes: None,
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
                visibility_attempts: Vec::new(),
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
        let mut engine = WorldEngine::new_test_bound_with_initial_settings(
            &generation,
            &incarnation,
            crate::world_connection::macos_capabilities(),
            vec![(
                "focusFollowsCursor".to_owned(),
                tiler_core::connection::RawSettingValue::Bool(true),
            )],
        )
        .expect("producer binds the consumer handshake");
        let accepted = |engine: &mut WorldEngine, report: &WorldReportV4| {
            match tiler_protocol::world_admission::handle_report_checked(engine, report, &[])
                .outcome
            {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("expected accepted, got {other:?}"),
            }
        };
        let pair = || {
            vec![
                fact("4242_7", "macos-display-1", 0),
                fact("4242_8", "macos-display-1", 100),
            ]
        };
        accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                1,
                pair(),
                "4242_7",
                Some((10, 10)),
                None,
            ),
        );
        // Genuine focus-follows-cursor producer: the cursor moves onto
        // 4242_8 while native stays on 4242_7, minting through the real
        // engine.
        let issuing = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                2,
                pair(),
                "4242_7",
                Some((110, 10)),
                Some("4242_8"),
            ),
        );
        let req = issuing.focus_request.clone().expect("hover mints");
        assert_eq!(req.target.0, "4242_8");
        assert_eq!(req.cause, FocusRequestCause::FocusFollowsCursor);
        assert_eq!(req.issued_revision, issuing.state_revision);
        // SKIP issuing plus two unchanged-baseline publications: pointer
        // rest over the pending target keeps the identical id core-side.
        for seq in [3u64, 4u64] {
            let retained = accepted(
                &mut engine,
                &report(
                    &incarnation,
                    &generation,
                    seq,
                    pair(),
                    "4242_7",
                    Some((110, 10)),
                    Some("4242_8"),
                ),
            );
            assert_eq!(
                retained.focus_request,
                Some(req.clone()),
                "unchanged baseline retains the identical request"
            );
        }
        // APPLY ONLY the final retained publication.
        let latest = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                5,
                pair(),
                "4242_7",
                Some((110, 10)),
                Some("4242_8"),
            ),
        );
        assert_eq!(
            latest.focus_request,
            Some(req.clone()),
            "final publication still carries the issuing request"
        );
        assert!(
            latest.state_revision > issuing.state_revision,
            "applied publication is newer than the issuing publication"
        );
        assert_eq!(latest.based_on_observation_seq, 5);
        let wire = tiler_protocol::world_protocol::encode_reply_v4(
            &tiler_protocol::world_protocol::DecodedReplyV4 {
                roster_plan: None,
                reply: latest.clone(),
            },
        )
        .expect("latest reply encodes");
        let back = tiler_protocol::world_protocol::decode_reply_v4(&wire).expect("latest decodes");
        assert_eq!(back.reply.focus_request, Some(req.clone()));
        assert_eq!(back.reply.adapter_incarnation, incarnation);
        assert_eq!(back.reply.core_generation, generation);
        let shared = focus_shared();
        let slot = live_focus_app(&shared, CLOSE_PID, CLOSE_GEN);
        let mut retained = close_retained(8, Some(close_target(7)), true);
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
        assert_eq!(cmd.window, format!("{CLOSE_PID}_8"));
        let revision = lock_shared(&shared).topology_revision;
        let close = cmd.close.clone().expect("typed close");
        let mut fake = FakeCloseBackend {
            fresh: Some((CLOSE_PID, Some(7))),
            post: Some((CLOSE_PID, Some(8))),
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
        match &job {
            crate::runtime_thread::EngineJob::CloseFocusResult {
                disposition,
                target,
                issued_rev,
                close_id,
                ..
            } => {
                assert_eq!(*disposition, accepted_disposition());
                assert_eq!(close_id, &req.id);
                assert_eq!(*issued_rev, req.issued_revision);
                assert_eq!(*target, close_target(8));
            }
            _other => panic!("expected CloseFocusResult"),
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
        assert_eq!(staged.disposition, accepted_disposition());
        // Duplicate latest replays nothing new: no second slot command, no
        // second owner dispatch, first staged row keeps its stamp.
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
        // Truthful target observation satisfies: the target reports as the
        // global focus next, so the core retires the lane and the production
        // consume of that null publication retires the adapter epoch too.
        let satisfied = accepted(
            &mut engine,
            &report(
                &incarnation,
                &generation,
                6,
                pair(),
                "4242_8",
                Some((110, 10)),
                Some("4242_8"),
            ),
        );
        assert_eq!(
            satisfied.focus_request, None,
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
    }

    // -- Fullscreen float toggle on the v4 world path (offline) ------------

    /// The core regression's fixture shape: an 800x600 work area and two
    /// windows under one live PID. Window ids parse as `<pid>_<wid>` so the
    /// adapter's frame staging resolves the live app slot.
    const FF_PID: u32 = 4242;
    const FF_SIBLING: &str = "4242_7";
    const FF_SUBJECT: &str = "4242_8";

    fn ff_rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn ff_screen() -> tiler_core::world_report::ScreenReport {
        use tiler_core::directional::OutputId;
        tiler_core::world_report::ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(ff_rect(0, 0, 800, 600)),
            work_area: Some(ff_rect(0, 0, 800, 600)),
            complete: true,
            scale: 1.0,
            identity: tiler_core::world_report::OutputIdentity::unknown(),
        }
    }

    fn ff_fact(window: &str, x: i32) -> tiler_core::world_report::WindowFact {
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::world_report::{
            ActivationPolicy, NativeWorkspaceFact, VisibilityFact, WindowType,
        };
        tiler_core::world_report::WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId("out-a".to_owned())),
            rect: Some(ff_rect(x, 0, 40, 90)),
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
            window_type: Some(WindowType::Normal),
            activation_policy: Some(ActivationPolicy::Regular),
            layer: Some(0),
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
        }
    }

    fn ff_report(
        seq: u64,
        windows: Vec<tiler_core::world_report::WindowFact>,
        focused: &str,
        command_seq: Option<u64>,
        command: Option<tiler_core::world_report::WorldCommand>,
    ) -> tiler_core::world_report::WorldReportV4 {
        use tiler_core::world_report::{
            PresentationIngressV4, PresentationRowV4, WORLD_CONTRACT_VERSION_V4, WorldReportV4,
        };
        WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq: seq,
            request_seq: seq,
            request_id: format!("req-{seq}"),
            command_seq,
            command,
            screens: vec![ff_screen()],
            held_screens: Vec::new(),
            windows,
            held_windows: Vec::new(),
            closed: Vec::new(),
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: focused.to_owned(),
            foreground_known: true,
            focus_changes: None,
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
            visibility_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            focus_feedback: Vec::new(),
            presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
            intent_records: Vec::new(),
            renderer_input: None,
        }
    }

    fn ff_engine() -> tiler_core::world_engine::WorldEngine {
        use tiler_core::connection::{HostCapabilities, RawSettingValue};
        tiler_core::world_engine::WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            HostCapabilities {
                // Fixtures carry native sticky and maximize facts; the lane
                // under test is the native route.
                native_sticky: true,
                maximize: true,
                ..Default::default()
            },
            vec![("outer".to_owned(), RawSettingValue::Number(0.0))],
        )
        .expect("generation")
    }

    /// Engine bound to the real macOS handshake: every fact the adapter
    /// declares unavailable stays null, native sticky/maximize are absent,
    /// and host-owned native domains need a roster row.
    fn mf_engine() -> tiler_core::world_engine::WorldEngine {
        use tiler_core::connection::RawSettingValue;
        tiler_core::world_engine::WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            crate::world_connection::macos_capabilities(),
            vec![("outer".to_owned(), RawSettingValue::Number(0.0))],
        )
        .expect("generation")
    }

    /// One macOS-shaped fact: truthful readable geometry/role/process/layer
    /// ride, every capability-unavailable fact rides null.
    fn mf_fact(window: &str, x: i32, fullscreen: bool) -> tiler_core::world_report::WindowFact {
        use tiler_core::directional::{OutputId, WindowId};
        use tiler_core::world_report::{
            ActivationPolicy, NativeWorkspaceFact, VisibilityFact, WindowType,
        };
        let (w, h) = if fullscreen { (800, 600) } else { (40, 90) };
        tiler_core::world_report::WindowFact {
            window: WindowId(window.to_owned()),
            output: Some(OutputId("out-a".to_owned())),
            rect: Some(ff_rect(x, 0, w, h)),
            fullscreen: Some(fullscreen),
            maximized: None,
            sticky: None,
            hints: Some(tiler_core::size_hints::WindowSizeHints::none()),
            minimized: Some(false),
            native_workspace: NativeWorkspaceFact::Value("native-a-0".to_owned()),
            host_hidden: Some(false),
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
            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
            app_id: Some("com.example.app".to_owned()),
            executable: None,
            process_id: Some(FF_PID.to_string()),
            class: None,
            title: None,
            window_type: Some(WindowType::Normal),
            activation_policy: Some(ActivationPolicy::Regular),
            layer: Some(0),
            fullscreen_owner: None,
            fullscreen_exitable: None,
            suppression_available: None,
            visibility: VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
        }
    }

    /// A macOS report: the ff fixture plus the host-owned native roster row
    /// the capability-gated domain needs to admit its windows.
    fn mf_report(
        seq: u64,
        windows: Vec<tiler_core::world_report::WindowFact>,
        focused: &str,
        command_seq: Option<u64>,
        command: Option<tiler_core::world_report::WorldCommand>,
    ) -> tiler_core::world_report::WorldReportV4 {
        use tiler_core::directional::OutputId;
        use tiler_core::world_report::{NativeObservation, NativeOccupancy};
        let mut report = ff_report(seq, windows, focused, command_seq, command);
        report.native = vec![NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["native-a-0".to_owned()],
            current_native: Some("native-a-0".to_owned()),
            occupancy: vec![NativeOccupancy {
                native: "native-a-0".to_owned(),
                count: None,
            }],
        }];
        report
    }

    fn ff_accepted(
        engine: &mut tiler_core::world_engine::WorldEngine,
        report: &tiler_core::world_report::WorldReportV4,
    ) -> tiler_core::world_report::WorldReplyV4 {
        match tiler_protocol::world_admission::handle_report_checked(engine, report, &[]).outcome {
            tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    /// The ToggleFloat command exactly as production sources it: the
    /// adapter's semantic-action map (implicit `CurrentFocus` subject, no
    /// explicit float rect).
    fn ff_toggle_float_command() -> (tiler_core::world_report::WorldCommand, &'static str) {
        let mut retained = RetainedCore {
            observed_focus: Some(tiler_core::directional::WindowId(FF_SUBJECT.to_owned())),
            ..Default::default()
        };
        crate::world_command_map::operation_command(
            &crate::runtime_types::InputSemanticAction::ToggleFloat,
            &mut retained,
            None,
            &[],
        )
        .expect("ToggleFloat maps to a world command")
    }

    /// One real reply row by lifetime.
    fn ff_row<'a>(
        reply: &'a tiler_core::world_report::WorldReplyV4,
        window: &str,
    ) -> &'a tiler_core::world_report::WindowDesire {
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| panic!("missing row {window}"))
    }

    /// Adapter-observed native truth for the fixture PID: both windows
    /// homed on out-a, tiled, subject not yet fullscreen.
    fn ff_retained() -> RetainedCore {
        use crate::runtime_core::RetainedApp;
        use tiler_core::directional::WindowId;
        let observed = |wid: u32, x: i32, fullscreen: bool| {
            let (w, h) = if fullscreen { (800, 600) } else { (40, 90) };
            crate::runtime_types::ObservedWindow {
                wid,
                x,
                y: 0,
                w,
                h,
                floating: false,
                fullscreen,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            }
        };
        RetainedCore {
            apps: BTreeMap::from([(
                FF_PID,
                RetainedApp {
                    generation: 1,
                    seq_last: 0,
                    cmd_counter: 0,
                    windows: vec![observed(7, 0, false), observed(8, 100, false)],
                    focused_wid: Some(7),
                    roster: BTreeMap::new(),
                    roster_incomplete: false,
                    roster_cause: String::new(),
                },
            )]),
            observed_focus: Some(WindowId(FF_SIBLING.to_owned())),
            observed_focus_known: true,
            ..Default::default()
        }
    }

    /// One live readable user Space matching the fixture screen (`out-a`).
    fn ff_native_context() -> Arc<crate::runtime_types::NativeContext> {
        use tiler_core::directional::{OutputId, WorkspaceId};
        Arc::new(crate::runtime_types::NativeContext {
            revision: 9,
            domains: vec![crate::runtime_types::NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("ws-a".to_owned()),
                space: 1,
                current: true,
                readable: true,
                bounds: ff_rect(0, 0, 800, 600),
                visible: ff_rect(0, 0, 800, 600),
            }],
            skipped: Vec::new(),
        })
    }

    /// Home the fixture windows on the context Space and give both the
    /// realistic ordinary macOS raw facts (role/subrole/layer/...) the core
    /// needs to classify them as managed tiles rather than held unknowns.
    fn ff_home_windows(retained: &mut RetainedCore, ctx: &crate::runtime_types::NativeContext) {
        use crate::runtime_core::RosterEntry;
        use tiler_core::session::DomainKey;
        let home = DomainKey {
            output: ctx.domains[0].output.clone(),
            workspace: ctx.domains[0].workspace.clone(),
        };
        for window in [FF_SIBLING, FF_SUBJECT] {
            retained
                .window_domains
                .insert(window.to_owned(), home.clone());
        }
        let app = retained.apps.get_mut(&FF_PID).expect("fixture app");
        for wid in [7u32, 8] {
            app.roster.insert(
                wid,
                RosterEntry {
                    observed: None,
                    spaces: Vec::new(),
                    fullscreen_meta: false,
                    minimized_meta: false,
                    hidden_meta: false,
                    excluded: None,
                    role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                    subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                    modal: Some(false),
                    resizable: Some(true),
                    layer: Some(0),
                    app_id: Some("com.example.app".to_owned()),
                    activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
                },
            );
        }
    }

    /// Flip the adapter-observed native fullscreen truth for the subject
    /// (truth only: the core owns all fullscreen policy).
    fn ff_set_subject_fullscreen(retained: &mut RetainedCore, fullscreen: bool) {
        let Some(app) = retained.apps.get_mut(&FF_PID) else {
            panic!("fixture app");
        };
        let subject = app
            .windows
            .iter_mut()
            .find(|window| window.wid == 8)
            .expect("fixture subject");
        subject.fullscreen = fullscreen;
        (subject.x, subject.y, subject.w, subject.h) = if fullscreen {
            (0, 0, 800, 600)
        } else {
            (100, 0, 40, 90)
        };
    }

    /// Set the adapter-observed native frame for the subject.
    fn ff_set_subject_observed(retained: &mut RetainedCore, frame: Rect) {
        let Some(app) = retained.apps.get_mut(&FF_PID) else {
            panic!("fixture app");
        };
        let subject = app
            .windows
            .iter_mut()
            .find(|window| window.wid == 8)
            .expect("fixture subject");
        (subject.x, subject.y, subject.w, subject.h) = (frame.x, frame.y, frame.w, frame.h);
    }

    /// Install/clear one authoritative roster row for the subject so the
    /// reveal veto can be exercised without inventing native state.
    fn ff_set_roster_minimized(retained: &mut RetainedCore, minimized: bool) {
        use crate::runtime_core::RosterEntry;
        let Some(app) = retained.apps.get_mut(&FF_PID) else {
            panic!("fixture app");
        };
        app.roster.insert(
            8,
            RosterEntry {
                observed: None,
                spaces: Vec::new(),
                fullscreen_meta: false,
                minimized_meta: minimized,
                hidden_meta: false,
                excluded: None,
                role: None,
                subrole: None,
                modal: None,
                resizable: None,
                layer: None,
                app_id: None,
                activation_policy: None,
            },
        );
    }

    /// Prune the subject lifetime from adapter retained truth (explicit
    /// close: gone from observed windows, minimized records, and roster).
    fn ff_forget_subject(retained: &mut RetainedCore) {
        let Some(app) = retained.apps.get_mut(&FF_PID) else {
            panic!("fixture app");
        };
        app.windows.retain(|window| window.wid != 8);
        app.roster.remove(&8);
        retained.minimized.remove(FF_SUBJECT);
    }

    /// Adapter observables: frames staged, overlay-withheld rows, focus
    /// dispatches, and published native-creation desires.
    fn ff_counters(shared: &Arc<Mutex<Shared>>) -> (u64, u64, u64, usize) {
        let guard = lock_shared(shared);
        (
            guard.diagnostics.frames_emitted,
            guard.diagnostics.frames_overlay_skipped,
            guard.diagnostics.focus_dispatched,
            guard.last_native_creations.len(),
        )
    }

    /// Rows the adapter's emission gate would stage for one reply: current
    /// scope, `Controlled` frame authority, and an effective frame (the
    /// frame-operation token is spent only after this filter).
    fn ff_gated_rows(
        reply: &tiler_core::world_report::WorldReplyV4,
    ) -> Vec<&tiler_core::world_report::WindowDesire> {
        let current: Vec<(String, String)> = reply
            .outputs
            .iter()
            .filter_map(|out| {
                out.logical_view
                    .as_ref()
                    .map(|view| (out.output.0.clone(), view.clone()))
            })
            .collect();
        reply
            .windows
            .iter()
            .filter(|row| {
                row.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled
                    && row.effective_frame.is_some()
                    && current.contains(&(row.output.0.clone(), row.workspace.clone()))
            })
            .collect()
    }

    #[test]
    fn fullscreen_toggle_float_under_fullscreen_stages_no_subject_frames() {
        // Real ENGINE (handle_report_checked, tiler-core) plus the real
        // `consume_world_reply`, with the ToggleFloat command sourced from
        // the production adapter map (implicit CurrentFocus subject). On a
        // focused fullscreen subject the first toggle floats it beneath the
        // retained overlay: no tile allocation, sibling reflow, and the
        // adapter stages nothing for the subject (no frame write, no focus,
        // no native creation/state). The second toggle re-tiles invisibly.
        let (command, token) = ff_toggle_float_command();
        assert_eq!(token, "toggle-float", "production op token");
        let shared = focus_shared();
        live_focus_app(&shared, FF_PID, 1);
        let mut session = focus_session();
        let mut retained = ff_retained();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4,
                        ticket: Option<(&WorldCommand, u64)>| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                ticket.map(|(command, _)| command),
                ticket.map(|(_, seq)| seq),
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        let area = |r: Rect| i64::from(r.w) * i64::from(r.h);
        let fallback = ff_rect(160, 120, 480, 360);
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        let mut engine = ff_engine();
        let seed = ff_accepted(
            &mut engine,
            &ff_report(
                1,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SIBLING,
                None,
                None,
            ),
        );
        assert_eq!(
            ff_row(&seed, FF_SUBJECT).frame_authority,
            tiler_core::world_report::ScopeAuthority::Controlled,
            "seed tiles the subject: {:?}",
            seed.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &seed, None);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "seed stages both tiles: {after:?} vs {before:?}"
        );
        assert_eq!(after.1, before.1, "no retained overlay at seed time");
        assert_eq!(after.2, before.2, "seed dispatches no focus");
        assert_eq!(after.3, 0, "seed publishes no native creation");
        let sibling_seed = ff_row(&seed, FF_SIBLING)
            .allocation
            .expect("seed sibling tile");

        // Native truth catches up: the subject is fullscreen and focused.
        ff_set_subject_fullscreen(&mut retained, true);
        retained.observed_focus = Some(WindowId(FF_SUBJECT.to_owned()));
        retained.observed_focus_known = true;

        let mut subject_full = ff_fact(FF_SUBJECT, 100);
        subject_full.fullscreen = Some(true);
        subject_full.rect = Some(fullscreen_frame);
        let mut first = ff_accepted(
            &mut engine,
            &ff_report(
                2,
                vec![ff_fact(FF_SIBLING, 0), subject_full],
                FF_SUBJECT,
                Some(2),
                Some(command.clone()),
            ),
        );
        let subject = ff_row(&first, FF_SUBJECT);
        assert!(subject.floating, "subject floats: {subject:?}");
        assert_eq!(
            subject.allocation, None,
            "floated subject holds no tile allocation: {subject:?}"
        );
        assert_eq!(
            subject.effective_frame,
            Some(fallback),
            "durable centered-60% fallback, never the fullscreen frame: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "no float frame write to a fullscreen subject: {subject:?}"
        );
        assert_eq!(
            subject.membership_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "no membership write to the fullscreen subject: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation.as_deref(),
            Some("op-2"),
            "one-shot placement token rides the row without write authority: {subject:?}"
        );
        assert!(
            subject.state.is_empty(),
            "no native state desire for the subject: {subject:?}"
        );
        assert_eq!(
            subject.visibility, None,
            "no visibility write for the subject: {subject:?}"
        );
        let sibling = ff_row(&first, FF_SIBLING);
        assert_eq!(
            sibling.frame_authority,
            tiler_core::world_report::ScopeAuthority::Controlled,
            "sibling reflow is Controlled: {sibling:?}"
        );
        let sibling_alloc = sibling.allocation.expect("sibling tile");
        assert!(
            area(sibling_alloc) > area(sibling_seed),
            "sibling expands into the freed tile: {sibling_alloc:?} vs {sibling_seed:?}"
        );
        assert_eq!(
            (sibling_alloc.w, sibling_alloc.h),
            (800, 600),
            "sibling takes the full domain: {sibling_alloc:?}"
        );
        assert!(
            first.native_creations.is_empty(),
            "no native creation for the subject: {:?}",
            first.native_creations
        );
        assert!(
            first.focus_request.is_none(),
            "float toggle mints no focus request: {:?}",
            first.focus_request
        );
        // Correlation regression: the same reply also carries an unrelated
        // floating lifetime under retained fullscreen with an older
        // placement token. Only the row minted for this command (op-2 /
        // command_seq 2) may be recorded; the older token must stay unspent
        // and schedule no reveal.
        let unrelated_fallback = ff_rect(200, 150, 300, 200);
        let mut unrelated = ff_row(&first, FF_SUBJECT).clone();
        unrelated.lifetime = WindowId("4242_9".to_owned());
        unrelated.frame_operation = Some("op-1".to_owned());
        unrelated.command_seq = Some(1);
        unrelated.effective_frame = Some(unrelated_fallback);
        first.windows.push(unrelated);
        {
            let app = retained.apps.get_mut(&FF_PID).expect("fixture app");
            app.windows.push(crate::runtime_types::ObservedWindow {
                wid: 9,
                x: 0,
                y: 0,
                w: 800,
                h: 600,
                floating: true,
                fullscreen: true,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            });
        }
        let gated = ff_gated_rows(&first);
        assert_eq!(
            gated
                .iter()
                .map(|row| row.lifetime.0.as_str())
                .collect::<Vec<_>>(),
            vec![FF_SIBLING],
            "only the sibling row is emission-eligible: {gated:?}"
        );

        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &first, Some((&command, 2)));
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "only the sibling's Controlled frame stages: {after:?} vs {before:?}"
        );
        assert_eq!(
            after.1 - before.1,
            2,
            "the subject and the unrelated floating fullscreen row are both \
             overlay-withheld: {after:?} vs {before:?}"
        );
        assert_eq!(after.2, before.2, "no focus dispatched for the subject");
        assert_eq!(after.3, 0, "no native creation published");
        assert!(
            lock_focus(&focus_queue).is_empty(),
            "focus outbox stays empty"
        );
        assert_eq!(
            session.pending_fullscreen_float_placements.get(FF_SUBJECT),
            Some(&crate::world_runtime::PendingFullscreenFloatPlacement {
                operation: "op-2".to_owned(),
                target: fallback,
                generation: 1,
            }),
            "the withheld placement is recorded (token unspent) with the durable target: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert_eq!(
            session.pending_fullscreen_float_placements.len(),
            1,
            "only the toggled subject gets a deferral: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert!(
            !session
                .pending_fullscreen_float_placements
                .contains_key("4242_9"),
            "the unrelated floating fullscreen row is never captured: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert!(
            session.spend_frame_op("op-1"),
            "the unrelated row's older token stays unspent (no reveal scheduled)"
        );

        // Second toggle: re-tile beneath the still-retained fullscreen
        // overlay. Allocation returns, no effective frame, no token.
        let mut subject_float_full = ff_fact(FF_SUBJECT, 100);
        subject_float_full.transient = Some(true);
        subject_float_full.fullscreen = Some(true);
        subject_float_full.rect = Some(fullscreen_frame);
        let second = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), subject_float_full],
                FF_SUBJECT,
                Some(3),
                Some(command.clone()),
            ),
        );
        let subject = ff_row(&second, FF_SUBJECT);
        assert!(!subject.floating, "subject re-tiles: {subject:?}");
        let retiled = subject
            .allocation
            .expect("subject holds a tile again invisibly");
        assert_eq!(
            (retiled.x, retiled.y, retiled.w, retiled.h),
            (404, 0, 396, 600),
            "the invisible re-tile reserves the ordinary tile: {subject:?}"
        );
        assert_eq!(
            subject.effective_frame, None,
            "invisible re-tile under the overlay: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "no frame write to the retained fullscreen subject: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation, None,
            "no placement token for a tiled row: {subject:?}"
        );
        let sibling = ff_row(&second, FF_SIBLING);
        let sibling_shrunk = sibling.allocation.expect("sibling tile");
        assert!(
            area(sibling_shrunk) < area(sibling_alloc),
            "sibling shrinks once the subject re-tiles: {sibling_shrunk:?} vs {sibling_alloc:?}"
        );
        assert_eq!(
            gated_names(&second),
            vec![FF_SIBLING],
            "subject re-tile has no frame authority: {:?}",
            second.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &second, Some((&command, 3)));
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "invisible re-tile stages only the sibling: {after:?} vs {before:?}"
        );
        assert_eq!(
            after.1 - before.1,
            1,
            "the invisible subject row is overlay-withheld: {after:?} vs {before:?}"
        );
        assert_eq!(after.2, before.2, "no focus dispatched for the subject");
        assert_eq!(after.3, 0, "no native creation published");
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the re-tile supersedes the deferred placement: {:?}",
            session.pending_fullscreen_float_placements
        );
    }

    fn gated_names(reply: &tiler_core::world_report::WorldReplyV4) -> Vec<&str> {
        ff_gated_rows(reply)
            .iter()
            .map(|row| row.lifetime.0.as_str())
            .collect()
    }

    #[test]
    fn fullscreen_toggle_float_exit_without_command_reveals_base_state() {
        // Both actual native exits of the fullscreen float toggle, with no
        // command: the shared core reveals the base state. Float base: the
        // row keeps the durable fallback with `Untouched` authority, so the
        // adapter's deferred lane writes the withheld placement exactly once
        // after the overlay clears (never from the fullscreen pose); tile
        // base: the row is a Controlled tiled allocation and the normal lane
        // stages it. No fullscreen frame is ever adopted as float placement.
        let (command, token) = ff_toggle_float_command();
        let fallback = ff_rect(160, 120, 480, 360);
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        let setup = |full_toggles: u64| {
            // Fresh engine, live adapter app, retained observation truth.
            let shared = focus_shared();
            live_focus_app(&shared, FF_PID, 1);
            let mut session = focus_session();
            let mut retained = ff_retained();
            let focus_queue = Arc::new(Mutex::new(Vec::new()));
            let mut engine = ff_engine();
            let seed = ff_accepted(
                &mut engine,
                &ff_report(
                    1,
                    vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                    FF_SIBLING,
                    None,
                    None,
                ),
            );
            let dispatch = |session: &mut WorldSession,
                            retained: &mut RetainedCore,
                            reply: &tiler_core::world_report::WorldReplyV4,
                            ticket: Option<(&WorldCommand, u64)>| {
                consume_world_reply(
                    session,
                    retained,
                    &shared,
                    &focus_queue,
                    16,
                    reply,
                    &[],
                    ticket.map(|(command, _)| command),
                    ticket.map(|(_, seq)| seq),
                    token,
                    "accepted",
                    0,
                    0,
                    0,
                    0,
                );
            };
            dispatch(&mut session, &mut retained, &seed, None);
            ff_set_subject_fullscreen(&mut retained, true);
            retained.observed_focus = Some(WindowId(FF_SUBJECT.to_owned()));
            retained.observed_focus_known = true;
            let mut subject_full = ff_fact(FF_SUBJECT, 100);
            subject_full.fullscreen = Some(true);
            subject_full.rect = Some(fullscreen_frame);
            let first = ff_accepted(
                &mut engine,
                &ff_report(
                    2,
                    vec![ff_fact(FF_SIBLING, 0), subject_full],
                    FF_SUBJECT,
                    Some(2),
                    Some(command.clone()),
                ),
            );
            dispatch(&mut session, &mut retained, &first, Some((&command, 2)));
            for seq in 3..(3 + full_toggles.saturating_sub(1)) {
                let mut float_full = ff_fact(FF_SUBJECT, 100);
                float_full.transient = Some(true);
                float_full.fullscreen = Some(true);
                float_full.rect = Some(fullscreen_frame);
                let reply = ff_accepted(
                    &mut engine,
                    &ff_report(
                        seq,
                        vec![ff_fact(FF_SIBLING, 0), float_full],
                        FF_SUBJECT,
                        Some(seq),
                        Some(command.clone()),
                    ),
                );
                dispatch(&mut session, &mut retained, &reply, Some((&command, seq)));
            }
            (shared, session, retained, focus_queue, engine)
        };

        // Leg 1: single toggle, then the overlay clears with no command and
        // the native frame still reports the full-screen pose (the pose the
        // deferred reveal has to replace).
        let (shared, mut session, mut retained, focus_queue, mut engine) = setup(1);
        assert_eq!(
            session
                .pending_fullscreen_float_placements
                .get(FF_SUBJECT)
                .map(|pending| (pending.operation.as_str(), pending.target)),
            Some(("op-2", fallback)),
            "the toggle records the withheld placement: {:?}",
            session.pending_fullscreen_float_placements
        );
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_set_subject_fullscreen(&mut retained, false);
        ff_set_subject_observed(&mut retained, fullscreen_frame);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let reply = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let subject = ff_row(&reply, FF_SUBJECT);
        assert!(subject.floating, "exit keeps the float: {subject:?}");
        assert_eq!(subject.allocation, None, "still no tile: {subject:?}");
        assert_eq!(
            subject.effective_frame,
            Some(fallback),
            "exit keeps the durable fallback: {subject:?}"
        );
        assert_ne!(
            subject.effective_frame,
            Some(fullscreen_frame),
            "the fullscreen frame is never adopted as float placement: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "the float reveal row carries no frame authority: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation.as_deref(),
            Some("op-2"),
            "the needed placement token stays retained: {subject:?}"
        );
        assert_eq!(
            gated_names(&reply),
            vec![FF_SIBLING],
            "only the sibling row is emission-eligible on float exit: {:?}",
            reply.windows
        );
        assert!(
            reply.focus_request.is_none() && reply.native_creations.is_empty(),
            "float exit mints no focus/native creation: {reply:?}"
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &reply);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "float exit stages the sibling reflow plus the deferred subject reveal: \
             {after:?} vs {before:?}"
        );
        assert_eq!(
            after.1, before.1,
            "no retained overlay after the exit: {after:?} vs {before:?}"
        );
        assert_eq!(after.2, before.2, "float exit dispatches no focus");
        assert_eq!(after.3, 0, "float exit publishes no native creation");
        assert!(
            !session.spend_frame_op("op-2"),
            "the deferred reveal spent the core placement token"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the reveal settles the pending record: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert!(
            lock_focus(&focus_queue).is_empty(),
            "focus outbox stays empty"
        );
        // The adapter publishes the same reveal as plan truth (what the
        // presentation renders).
        let desired = lock_shared(&shared).last_desired.clone();
        assert!(
            desired.iter().any(|frame| frame.window == FF_SUBJECT
                && (frame.x, frame.y, frame.w, frame.h)
                    == (fallback.x, fallback.y, fallback.w, fallback.h)),
            "plan truth reveals the float at the fallback: {desired:?}"
        );

        // Leg 2: double toggle, then the overlay clears with no command and
        // a plain tiled observation: both rows re-tile and the adapter
        // stages both reveal writes.
        let (shared, mut session, mut retained, focus_queue, mut engine) = setup(2);
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the re-tile left no deferred placement: {:?}",
            session.pending_fullscreen_float_placements
        );
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_set_subject_fullscreen(&mut retained, false);
        let reply = ff_accepted(
            &mut engine,
            &ff_report(
                4,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let subject = ff_row(&reply, FF_SUBJECT);
        assert!(!subject.floating, "exit stays tiled: {subject:?}");
        let subject_alloc = subject.allocation.expect("tiled after exit");
        assert_eq!(
            subject.effective_frame,
            Some(subject_alloc),
            "tiled frame rides the allocation: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Controlled,
            "tile reveal carries frame authority: {subject:?}"
        );
        assert_ne!(
            subject_alloc, fullscreen_frame,
            "exit never keeps the fullscreen frame: {subject:?}"
        );
        let sibling_alloc = ff_row(&reply, FF_SIBLING)
            .allocation
            .expect("sibling tile after exit");
        assert_eq!(
            (
                sibling_alloc.x,
                sibling_alloc.y,
                sibling_alloc.w,
                sibling_alloc.h
            ),
            (0, 0, 396, 600),
            "sibling reveal tile: {sibling_alloc:?}"
        );
        assert!(
            sibling_alloc.x + sibling_alloc.w <= subject_alloc.x,
            "both tiles share the domain without overlap: {sibling_alloc:?} {subject_alloc:?}"
        );
        assert_eq!(
            (
                subject_alloc.x,
                subject_alloc.y,
                subject_alloc.w,
                subject_alloc.h
            ),
            (404, 0, 396, 600),
            "tile reveal is the tiled allocation, not the float placement: {subject:?}"
        );
        assert_ne!(
            subject_alloc, fallback,
            "tile reveal never reuses the float placement: {subject:?}"
        );
        assert_eq!(
            gated_names(&reply),
            vec![FF_SIBLING, FF_SUBJECT],
            "both tiled rows are emission-eligible on tile exit: {:?}",
            reply.windows
        );
        assert!(
            reply.focus_request.is_none() && reply.native_creations.is_empty(),
            "tile exit mints no focus/native creation: {reply:?}"
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &reply);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "tile exit stages both reveal writes: {after:?} vs {before:?}"
        );
        assert_eq!(
            after.1, before.1,
            "no retained overlay after the exit: {after:?} vs {before:?}"
        );
        assert_eq!(after.2, before.2, "tile exit dispatches no focus");
        assert_eq!(after.3, 0, "tile exit publishes no native creation");
        assert!(
            lock_focus(&focus_queue).is_empty(),
            "focus outbox stays empty"
        );
    }

    /// One fresh single-toggle fixture handle bundle for the deferred-reveal
    /// legs (seed + first fullscreen float toggle already dispatched).
    struct FfFixture {
        shared: Arc<Mutex<Shared>>,
        session: WorldSession,
        retained: RetainedCore,
        focus_queue: Arc<Mutex<Vec<super::FocusRequest>>>,
        engine: tiler_core::world_engine::WorldEngine,
    }

    /// Fresh single-toggle fixture: seed, first fullscreen float toggle, and
    /// the adapter dispatch closure shared by the deferred-reveal legs.
    fn ff_fullscreen_float_fixture() -> FfFixture {
        let (command, token) = ff_toggle_float_command();
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        let shared = focus_shared();
        live_focus_app(&shared, FF_PID, 1);
        let mut session = focus_session();
        let mut retained = ff_retained();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut engine = ff_engine();
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4,
                        ticket: Option<(&WorldCommand, u64)>| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                ticket.map(|(command, _)| command),
                ticket.map(|(_, seq)| seq),
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        let seed = ff_accepted(
            &mut engine,
            &ff_report(
                1,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SIBLING,
                None,
                None,
            ),
        );
        dispatch(&mut session, &mut retained, &seed, None);
        ff_set_subject_fullscreen(&mut retained, true);
        retained.observed_focus = Some(WindowId(FF_SUBJECT.to_owned()));
        retained.observed_focus_known = true;
        let mut subject_full = ff_fact(FF_SUBJECT, 100);
        subject_full.fullscreen = Some(true);
        subject_full.rect = Some(fullscreen_frame);
        let first = ff_accepted(
            &mut engine,
            &ff_report(
                2,
                vec![ff_fact(FF_SIBLING, 0), subject_full],
                FF_SUBJECT,
                Some(2),
                Some(command.clone()),
            ),
        );
        dispatch(&mut session, &mut retained, &first, Some((&command, 2)));
        FfFixture {
            shared,
            session,
            retained,
            focus_queue,
            engine,
        }
    }

    #[test]
    fn fullscreen_toggle_float_deferred_reveal_echo_veto_and_close() {
        // The deferred placement writes once, never repeats a matching
        // frame, is vetoed by retained native minimized truth, and clears
        // when the lifetime leaves adapter truth.
        let fallback = ff_rect(160, 120, 480, 360);
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        let (_, token) = ff_toggle_float_command();

        // Leg A: after the reveal, a settled echo at the placement stages no
        // repeat subject write.
        let FfFixture {
            shared,
            mut session,
            mut retained,
            focus_queue,
            mut engine,
        } = ff_fullscreen_float_fixture();
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_set_subject_fullscreen(&mut retained, false);
        ff_set_subject_observed(&mut retained, fullscreen_frame);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let reveal = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &reveal);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "the reveal stages the sibling plus the deferred subject:              {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the reveal clears the record: {:?}",
            session.pending_fullscreen_float_placements
        );
        // Echo: the subject now observes the placement, no command.
        ff_set_subject_observed(&mut retained, fallback);
        let mut echo = ff_fact(FF_SUBJECT, 100);
        echo.transient = Some(true);
        echo.rect = Some(fallback);
        let echo_reply = ff_accepted(
            &mut engine,
            &ff_report(
                4,
                vec![ff_fact(FF_SIBLING, 0), echo],
                FF_SIBLING,
                None,
                None,
            ),
        );
        assert!(
            ff_row(&echo_reply, FF_SUBJECT).floating,
            "echo keeps the float: {:?}",
            echo_reply.windows
        );
        let echo_subject = ff_row(&echo_reply, FF_SUBJECT);
        assert_eq!(
            echo_subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "echo row carries no frame authority: {echo_subject:?}"
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &echo_reply);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "a matching placement never repeats a subject write: {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "echo leaves no pending record: {:?}",
            session.pending_fullscreen_float_placements
        );

        // Leg B: retained native minimized truth vetoes the reveal until it
        // clears; then exactly one write lands.
        let FfFixture {
            shared,
            mut session,
            mut retained,
            focus_queue,
            mut engine,
        } = ff_fullscreen_float_fixture();
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_set_subject_fullscreen(&mut retained, false);
        ff_set_subject_observed(&mut retained, fullscreen_frame);
        ff_set_roster_minimized(&mut retained, true);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let mut vetoed = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        // Production minimized/host-hidden republish: the float row is no
        // longer floating, has no geometry/token, and carries a Hold.
        for row in &mut vetoed.windows {
            if row.lifetime.0 == FF_SUBJECT {
                row.floating = false;
                row.allocation = None;
                row.effective_frame = None;
                row.frame_operation = None;
                row.command_seq = None;
                row.frame_authority = tiler_core::world_report::ScopeAuthority::Hold(
                    tiler_core::world_report::HoldReason::Minimized,
                );
                row.membership_authority = tiler_core::world_report::ScopeAuthority::Hold(
                    tiler_core::world_report::HoldReason::Minimized,
                );
            }
        }
        assert!(
            !ff_row(&vetoed, FF_SUBJECT).floating
                && ff_row(&vetoed, FF_SUBJECT)
                    .frame_authority
                    .hold_reason()
                    .is_some(),
            "the minimized held shape is in place: {:?}",
            vetoed.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &vetoed);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "the minimized veto withholds the subject write: {after:?} vs {before:?}"
        );
        assert_eq!(
            session
                .pending_fullscreen_float_placements
                .get(FF_SUBJECT)
                .map(|pending| pending.target),
            Some(fallback),
            "the veto keeps the placement pending: {:?}",
            session.pending_fullscreen_float_placements
        );
        // Minimized truth clears: exactly one write lands.
        ff_set_roster_minimized(&mut retained, false);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let revealed = ff_accepted(
            &mut engine,
            &ff_report(
                4,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &revealed);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "the cleared veto releases the deferred write once: {after:?} vs {before:?}"
        );
        assert!(
            !session.spend_frame_op("op-2"),
            "the released reveal spent the placement token"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the released reveal settles the record: {:?}",
            session.pending_fullscreen_float_placements
        );

        // Leg C: a row-level hold (constraint clamp) defers the reveal
        // until an authoritative reply, like the other vetoes.
        let FfFixture {
            shared,
            mut session,
            mut retained,
            focus_queue,
            mut engine,
        } = ff_fullscreen_float_fixture();
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_set_subject_fullscreen(&mut retained, false);
        ff_set_subject_observed(&mut retained, fullscreen_frame);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let mut held = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        for row in &mut held.windows {
            if row.lifetime.0 == FF_SUBJECT {
                row.floating = false;
                row.allocation = None;
                row.effective_frame = None;
                row.frame_operation = None;
                row.command_seq = None;
                row.frame_authority = tiler_core::world_report::ScopeAuthority::Hold(
                    tiler_core::world_report::HoldReason::ConstraintClamp,
                );
                row.membership_authority = tiler_core::world_report::ScopeAuthority::Hold(
                    tiler_core::world_report::HoldReason::ConstraintClamp,
                );
            }
        }
        assert_eq!(
            ff_row(&held, FF_SUBJECT).frame_authority,
            tiler_core::world_report::ScopeAuthority::Hold(
                tiler_core::world_report::HoldReason::ConstraintClamp
            ),
            "the row-level hold shape is in place: {:?}",
            held.windows
        );
        assert!(
            !ff_row(&held, FF_SUBJECT).floating,
            "the held row is republished non-floating: {:?}",
            held.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &held);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "a row-level hold withholds the reveal, staging only the sibling: \
             {after:?} vs {before:?}"
        );
        assert_eq!(
            session
                .pending_fullscreen_float_placements
                .get(FF_SUBJECT)
                .map(|pending| pending.target),
            Some(fallback),
            "a row-level hold keeps the placement pending: {:?}",
            session.pending_fullscreen_float_placements
        );
        // The next authoritative reply releases the one write.
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let released = ff_accepted(
            &mut engine,
            &ff_report(
                4,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &released);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "the authoritative reply releases the deferred write once: \
             {after:?} vs {before:?}"
        );
        assert!(
            !session.spend_frame_op("op-2"),
            "the released reveal spent the placement token"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the authoritative reveal settles the record: {:?}",
            session.pending_fullscreen_float_placements
        );

        // Leg D: a lifetime leaving adapter truth clears the deferral
        // without inventing a write.
        let FfFixture {
            shared,
            mut session,
            mut retained,
            focus_queue,
            mut engine,
        } = ff_fullscreen_float_fixture();
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
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        ff_forget_subject(&mut retained);
        let mut close_report = ff_report(3, vec![ff_fact(FF_SIBLING, 0)], FF_SIBLING, None, None);
        close_report.closed = vec![WindowId(FF_SUBJECT.to_owned())];
        let closed = ff_accepted(&mut engine, &close_report);
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &closed);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "a closed lifetime stages only the surviving sibling: {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the closed lifetime clears its deferral: {:?}",
            session.pending_fullscreen_float_placements
        );
    }

    #[test]
    fn fullscreen_toggle_float_ordinary_first_float_stays_write_free() {
        // Ordinary (not fullscreen) first float: core publishes the
        // execution-time placement (the subject's current frame) with
        // `Untouched` authority and keeps the token; the adapter must not
        // stage a subject write or spend the token.
        let (command, token) = ff_toggle_float_command();
        let shared = focus_shared();
        live_focus_app(&shared, FF_PID, 1);
        let mut session = focus_session();
        let mut retained = ff_retained();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut engine = ff_engine();
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4,
                        ticket: Option<(&WorldCommand, u64)>| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                ticket.map(|(command, _)| command),
                ticket.map(|(_, seq)| seq),
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        let seed = ff_accepted(
            &mut engine,
            &ff_report(
                1,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SIBLING,
                None,
                None,
            ),
        );
        dispatch(&mut session, &mut retained, &seed, None);
        let first = ff_accepted(
            &mut engine,
            &ff_report(
                2,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SUBJECT,
                Some(2),
                Some(command.clone()),
            ),
        );
        let subject = ff_row(&first, FF_SUBJECT);
        assert!(subject.floating, "subject floats: {subject:?}");
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "ordinary float placement stays adapter-withheld: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation.as_deref(),
            Some("op-2"),
            "the placement token rides the row unspent: {subject:?}"
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &first, Some((&command, 2)));
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "only the sibling's Controlled frame stages: {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "ordinary floats never enter the fullscreen deferred lane: {:?}",
            session.pending_fullscreen_float_placements
        );

        // The same ordinary float enters fullscreen on an
        // observation-only reply: it now sits under retained fullscreen
        // with its placement token, but no ToggleFloat command was
        // applied, so the adapter must not record a deferral. The token
        // presence alone is not the discriminator.
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        ff_set_subject_fullscreen(&mut retained, true);
        let mut subject_full = ff_fact(FF_SUBJECT, 100);
        subject_full.transient = Some(true);
        subject_full.fullscreen = Some(true);
        subject_full.rect = Some(fullscreen_frame);
        let mut entered = ff_accepted(
            &mut engine,
            &ff_report(
                3,
                vec![ff_fact(FF_SIBLING, 0), subject_full],
                FF_SIBLING,
                None,
                None,
            ),
        );
        let subject = ff_row(&entered, FF_SUBJECT);
        assert!(
            subject.floating,
            "observation keeps the ordinary float: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation, None,
            "the observation reply itself drops the spent token: {subject:?}"
        );
        // Counterfactual probe: a normally floated lifetime can still carry
        // a placement token when it enters fullscreen (a stale/unsettled
        // placement). Token presence must not create a deferral without a
        // ToggleFloat command.
        for row in &mut entered.windows {
            if row.lifetime.0 == FF_SUBJECT {
                row.frame_operation = Some("op-2".to_owned());
            }
        }
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &entered, None);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "observation-only fullscreen entry stages only the sibling: \
             {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "no command means no deferral, even with a floating token row \
             under fullscreen: {:?}",
            session.pending_fullscreen_float_placements
        );

        // Fullscreen exits with the native frame still at the fullscreen
        // pose; with no recorded deferral there is no placement write, so
        // the ordinary float stays in place.
        ff_set_subject_fullscreen(&mut retained, false);
        ff_set_subject_observed(&mut retained, fullscreen_frame);
        let mut exited = ff_fact(FF_SUBJECT, 100);
        exited.transient = Some(true);
        exited.rect = Some(fullscreen_frame);
        let exited = ff_accepted(
            &mut engine,
            &ff_report(
                4,
                vec![ff_fact(FF_SIBLING, 0), exited],
                FF_SIBLING,
                None,
                None,
            ),
        );
        assert!(
            ff_row(&exited, FF_SUBJECT).floating,
            "exit keeps the ordinary float: {:?}",
            exited.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &exited, None);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "no deferred placement is invented on exit: {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "the observation-only lifetime never enters the lane: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert!(
            session.spend_frame_op("op-2"),
            "ordinary float token stays unspent (no write)"
        );
    }

    #[test]
    fn macos_first_float_lands_at_current_tile_frame() {
        use tiler_core::world_report::{CommandDisposition, CommandResult, ScopeAuthority};
        // The production intent carries no rectangle: core owns placement.
        let (command, token) = ff_toggle_float_command();
        assert!(
            matches!(
                &command,
                tiler_core::world_report::WorldCommand::Operation(
                    tiler_core::boundary::CoreCommand::ToggleFloat {
                        float_rect: None,
                        ..
                    }
                )
            ),
            "production ToggleFloat must not preselect a rectangle: {command:?}"
        );
        let shared = focus_shared();
        live_focus_app(&shared, FF_PID, 1);
        let mut session = focus_session();
        let mut retained = ff_retained();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let mut engine = mf_engine();
        let dispatch =
            |session: &mut WorldSession,
             retained: &mut RetainedCore,
             reply: &tiler_core::world_report::WorldReplyV4,
             ticket: Option<(&tiler_core::world_report::WorldCommand, u64)>| {
                consume_world_reply(
                    session,
                    retained,
                    &shared,
                    &focus_queue,
                    16,
                    reply,
                    &[],
                    ticket.map(|(command, _)| command),
                    ticket.map(|(_, seq)| seq),
                    token,
                    "accepted",
                    0,
                    0,
                    0,
                    0,
                );
            };
        let seed = ff_accepted(
            &mut engine,
            &mf_report(
                1,
                vec![
                    mf_fact(FF_SIBLING, 0, false),
                    mf_fact(FF_SUBJECT, 100, false),
                ],
                FF_SIBLING,
                None,
                None,
            ),
        );
        dispatch(&mut session, &mut retained, &seed, None);
        let observed_subject = mf_fact(FF_SUBJECT, 100, false);
        let observed_tile_frame = observed_subject.rect.expect("readable tile frame");
        let allocation = ff_row(&seed, FF_SUBJECT)
            .allocation
            .expect("core allocated a tile");
        assert_ne!(observed_tile_frame, allocation);
        let floated = tiler_protocol::world_admission::handle_report_checked(
            &mut engine,
            &mf_report(
                2,
                vec![mf_fact(FF_SIBLING, 0, false), observed_subject],
                FF_SUBJECT,
                Some(2),
                Some(command.clone()),
            ),
            &[],
        );
        let tiler_core::world_engine::WorldOutcome::Accepted(ref first) = floated.outcome else {
            panic!("expected accepted float, got {:?}", floated.outcome);
        };
        // First float uses the subject's observed frame, not the layout
        // allocation or the centered 60% fallback.
        let placed = floated
            .dispositions
            .iter()
            .find_map(|entry| match &entry.disposition {
                CommandDisposition::Executed(Some(CommandResult::Float { rect, .. })) => *rect,
                _ => None,
            })
            .expect("float applied");
        assert_eq!(placed, observed_tile_frame, "{floated:?}");
        assert_ne!(placed, ff_rect(160, 120, 480, 360), "not centered 60%");
        let subject = ff_row(first, FF_SUBJECT);
        assert!(subject.floating, "subject floats: {subject:?}");
        assert_eq!(
            subject.effective_frame,
            Some(observed_tile_frame),
            "the durable placement is the current frame: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            ScopeAuthority::Untouched,
            "an in-place placement needs no adapter write: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation.as_deref(),
            Some("op-2"),
            "the placement token rides the row unspent: {subject:?}"
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, first, Some((&command, 2)));
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "only the sibling's reflow frame stages: {after:?} vs {before:?}"
        );
        assert!(
            session.pending_fullscreen_float_placements.is_empty(),
            "an ordinary first float never enters the deferred lane: {:?}",
            session.pending_fullscreen_float_placements
        );
        assert!(
            session.spend_frame_op("op-2"),
            "the placement token stays unspent (no subject write)"
        );
    }

    #[test]
    fn fullscreen_toggle_float_refused_after_accept_still_records_for_reveal() {
        // Real native-route dispatch (production report builder over a live
        // Space, unit-test roster raw facts) with the production ToggleFloat
        // command: core applies the float under the retained fullscreen
        // overlay, then either post-accept early return refuses the reply
        // before emission:
        //   - `intent-write-held`: no durable intent store, so the applied
        //     reply's `intent_desired` row cannot be persisted;
        //   - `stale-observation`: an attached store persists the row, then
        //     the post-accept hook invalidates the clock before emission.
        // Both must still run the record-only reconciliation and stage the
        // pending placement. A later observation-only exit reveals it exactly
        // once at the durable fallback.
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        for (expected, stale) in [("intent-write-held", false), ("stale-observation", true)] {
            let (command, token) = ff_toggle_float_command();
            let shared = focus_shared();
            live_focus_app(&shared, FF_PID, 1);
            let clock = Arc::clone(&lock_shared(&shared).clock);
            let ctx = ff_native_context();
            lock_shared(&shared).native_context = Some(Arc::clone(&ctx));
            let mut session = WorldSession::new(
                true,
                crate::focus_route::FocusRoute::Slps,
                Arc::clone(&clock),
                &[],
            );
            // The durable store only exists for the stale leg: with one, the
            // applied intent row persists and the reply proceeds to the
            // emission-coherence gate; without one, the intent lane holds it.
            let _store_root = stale.then(|| {
                let (root, store) = crate::intent_store::test_store_parts();
                session.attach_intent_store(store);
                session.set_intent_identity(Box::new(|pid| {
                    (pid == FF_PID).then_some(crate::intent_store::LiveBinding {
                        launch_date_ns: 1,
                        bundle_id: "com.example.app".to_owned(),
                    })
                }));
                session.set_intent_process_liveness(Box::new(|pid| pid == FF_PID));
                root
            });
            let mut retained = ff_retained();
            ff_home_windows(&mut retained, &ctx);
            let focus_queue = Arc::new(Mutex::new(Vec::new()));
            let inbox = Arc::new(Inbox::new(16));

            // Seed through the same native route so core stages the exact
            // domain, roster facts, and tiled layout the toggle observes.
            let seed = session
                .sequencer()
                .try_begin_request(None)
                .expect("seed flight opens");
            match dispatch_ticket(
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                16,
                DispatchContext::Native(Arc::clone(&ctx)),
                Vec::new(),
                seed,
                None,
                false,
                false,
                "reconcile",
                FF_PID,
                1,
                1,
            ) {
                DispatchVerdict::Applied { .. } => {}
                other => panic!("the seed observation must apply, got {other:?}"),
            }
            assert!(
                session.pending_fullscreen_float_placements.is_empty(),
                "the seed stages no deferral"
            );

            // Native truth catches up: the subject is fullscreen and focused.
            ff_set_subject_fullscreen(&mut retained, true);
            retained.observed_focus = Some(WindowId(FF_SUBJECT.to_owned()));
            retained.observed_focus_known = true;
            clock.invalidate();
            if stale {
                let hook: crate::runtime_shared::PostAcceptHook = Arc::new({
                    let clock = Arc::clone(&clock);
                    move || {
                        clock.invalidate();
                    }
                });
                lock_shared(&shared)
                    .post_accept_hook
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .replace(hook);
            }
            let ticket = session
                .sequencer()
                .try_begin_request(Some(command))
                .expect("toggle flight opens");
            let command_seq = ticket.command_seq.expect("command sequence minted");
            let verdict = dispatch_ticket(
                &mut session,
                &mut retained,
                &shared,
                &inbox,
                &focus_queue,
                16,
                DispatchContext::Native(Arc::clone(&ctx)),
                Vec::new(),
                ticket,
                None,
                false,
                false,
                token,
                FF_PID,
                1,
                2,
            );
            lock_shared(&shared)
                .post_accept_hook
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .take();
            match verdict {
                DispatchVerdict::Refused { token, .. } => {
                    assert_eq!(token, expected, "the applied reply refuses here");
                }
                other => panic!("expected a refusal at {expected}, got {other:?}"),
            }
            let operation = format!("op-{command_seq}");
            // The durable target is whatever the real applied reply published
            // for the withheld placement (the settings-resolved centered
            // float), never a hand-picked rect.
            let applied = session
                .sequencer()
                .last_reply()
                .expect("the applied reply stays retained");
            let subject = ff_row(&applied, FF_SUBJECT);
            assert!(
                subject.floating,
                "{expected}: core floated the subject: {subject:?}"
            );
            let durable = subject
                .effective_frame
                .expect("the withheld placement carries a durable target");
            assert_eq!(
                subject.frame_operation.as_deref(),
                Some(operation.as_str()),
                "{expected}: the placement token is minted for this command"
            );
            assert_eq!(
                session
                    .pending_fullscreen_float_placements
                    .get(FF_SUBJECT)
                    .map(|pending| (
                        pending.operation.as_str(),
                        pending.target,
                        pending.generation
                    )),
                Some((operation.as_str(), durable, 1)),
                "the refused-but-applied reply records the deferral: {:?}",
                session.pending_fullscreen_float_placements
            );

            // Observation-only exit: no ticket, but the pending record is
            // revealed exactly once at the durable fallback.
            ff_set_subject_fullscreen(&mut retained, false);
            ff_set_subject_observed(&mut retained, fullscreen_frame);
            clock.invalidate();
            // Production native builder over the same live Space: the exit
            // observation carries the truthful fullscreen=false bit and the
            // stale native frame, while core keeps the retained float target
            // (the record) because the observation does not echo it.
            let exit_report = build_dispatch_report(
                &mut session,
                &retained,
                &DispatchContext::Native(Arc::clone(&ctx)),
                &[],
                4,
                3,
                None,
                None,
                None,
                "req-exit",
            )
            .expect("exit report context");
            let exit_reply = ff_accepted(session.engine_mut(), &exit_report);
            let before = ff_counters(&shared);
            consume_world_reply(
                &mut session,
                &mut retained,
                &shared,
                &focus_queue,
                16,
                &exit_reply,
                &[],
                None,
                None,
                token,
                "accepted",
                FF_PID,
                1,
                3,
                0,
            );
            let after = ff_counters(&shared);
            assert_eq!(
                after.0 - before.0,
                2,
                "{expected}: the exit reveals the sibling plus the deferred subject: \
                 {after:?} vs {before:?}"
            );
            assert!(
                session.pending_fullscreen_float_placements.is_empty(),
                "{expected}: the reveal settles the record: {:?}",
                session.pending_fullscreen_float_placements
            );
            assert!(
                !session.spend_frame_op(&operation),
                "{expected}: the reveal spent the placement token"
            );
        }
    }

    #[test]
    fn fullscreen_subject_emission_gate_requires_controlled_authority() {
        // Emission-gate characterization on the REAL applied first-toggle
        // reply: the retained-fullscreen subject row carries geometry AND
        // the placement token with `Untouched` authority, so it stages
        // nothing. The identical row with `frame_authority` flipped to
        // `Controlled` stages, demonstrating that the existing authority
        // filter (not a fullscreen special case) is the discriminator and
        // that the real reply never spent the placement token. The flipped
        // row is a test-only counterfactual probe, never a shape production
        // core emits.
        let (command, token) = ff_toggle_float_command();
        let fallback = ff_rect(160, 120, 480, 360);
        let fullscreen_frame = ff_rect(0, 0, 800, 600);
        let shared = focus_shared();
        live_focus_app(&shared, FF_PID, 1);
        let mut session = focus_session();
        let mut retained = ff_retained();
        let focus_queue = Arc::new(Mutex::new(Vec::new()));
        let dispatch = |session: &mut WorldSession,
                        retained: &mut RetainedCore,
                        reply: &tiler_core::world_report::WorldReplyV4,
                        ticket: Option<(&WorldCommand, u64)>| {
            consume_world_reply(
                session,
                retained,
                &shared,
                &focus_queue,
                16,
                reply,
                &[],
                ticket.map(|(command, _)| command),
                ticket.map(|(_, seq)| seq),
                token,
                "accepted",
                0,
                0,
                0,
                0,
            );
        };
        let mut engine = ff_engine();
        let seed = ff_accepted(
            &mut engine,
            &ff_report(
                1,
                vec![ff_fact(FF_SIBLING, 0), ff_fact(FF_SUBJECT, 100)],
                FF_SIBLING,
                None,
                None,
            ),
        );
        dispatch(&mut session, &mut retained, &seed, None);
        ff_set_subject_fullscreen(&mut retained, true);
        let mut subject_full = ff_fact(FF_SUBJECT, 100);
        subject_full.fullscreen = Some(true);
        subject_full.rect = Some(fullscreen_frame);
        let real = ff_accepted(
            &mut engine,
            &ff_report(
                2,
                vec![ff_fact(FF_SIBLING, 0), subject_full],
                FF_SUBJECT,
                Some(2),
                Some(command.clone()),
            ),
        );
        let subject = ff_row(&real, FF_SUBJECT);
        assert_eq!(
            subject.effective_frame,
            Some(fallback),
            "real subject row carries geometry: {subject:?}"
        );
        assert_eq!(
            subject.frame_authority,
            tiler_core::world_report::ScopeAuthority::Untouched,
            "real subject row is not Controlled: {subject:?}"
        );
        assert_eq!(
            subject.frame_operation.as_deref(),
            Some("op-2"),
            "real subject row carries the token: {subject:?}"
        );
        assert_eq!(
            gated_names(&real),
            vec![FF_SIBLING],
            "the retained-fullscreen subject row fails the authority gate: {:?}",
            real.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &real, Some((&command, 2)));
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            1,
            "authority-gated real reply stages only the sibling: {after:?} vs {before:?}"
        );

        // Differential probe: same row, flipped authority only.
        let mut flipped = real.clone();
        let row = flipped
            .windows
            .iter_mut()
            .find(|row| row.lifetime.0 == FF_SUBJECT)
            .expect("subject row");
        row.frame_authority = tiler_core::world_report::ScopeAuthority::Controlled;
        assert_eq!(
            gated_names(&flipped),
            vec![FF_SIBLING, FF_SUBJECT],
            "the flipped identical row becomes emission-eligible: {:?}",
            flipped.windows
        );
        let before = ff_counters(&shared);
        dispatch(&mut session, &mut retained, &flipped, None);
        let after = ff_counters(&shared);
        assert_eq!(
            after.0 - before.0,
            2,
            "the flipped row stages, proving the authority check is the \
             discriminator and the real reply left the placement token unspent: \
             {after:?} vs {before:?}"
        );
    }
}
