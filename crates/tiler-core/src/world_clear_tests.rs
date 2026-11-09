//! Directional maximize-clear lifecycle regressions: publication-first
//! admission/clear/continuation sequences, supersession ordering, focus and
//! view abandonment, held-domain deferral, and terminal observability.
//!
//! Split from [`crate::world_directional`] tests for source size: the
//! directional policy unit tests stay there; every test here drives the
//! retained clear lifecycle through [`crate::world_engine::WorldEngine`].
//! Shared report builders live in [`crate::world_v4_support_tests`].

use crate::directional::{OutputId, WindowId};
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandResult, ScopeAuthority, ScreenReport, WindowFact, WorldCommand, WorldReportV4,
};
use crate::world_v4_support_tests::{
    accepted, desire, fact, float_fact, outcome_str, rect, report, report_full, screen,
};

fn clear_attempt(
    operation_id: &str,
    window: &str,
    setter: crate::world_report::FrameSetter,
    pre: Option<bool>,
    post: Option<bool>,
) -> crate::world_report::ClearAttempt {
    crate::world_report::ClearAttempt {
        operation_id: operation_id.to_owned(),
        window: WindowId(window.to_owned()),
        setter,
        pre_maximized: pre,
        post_maximized: post,
        attempt_ms: 7,
    }
}

#[allow(clippy::too_many_arguments)]
fn report_clears(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    clears: Vec<crate::world_report::ClearAttempt>,
) -> WorldReportV4 {
    let mut report = report_full(
        request_seq,
        observation_seq,
        screens,
        windows,
        Vec::new(),
        focused,
        None,
        None,
    );
    report.clear_attempts = clears;
    report
}

/// Row-tail wire span: `"clear_op":"{op}"` plus the shared canonical lane
/// encoding of the row's correlation tail (`cmdSeq`, visibility, state).
/// The v4 lane has no full-reply canonical encoder in core (wire encoding
/// lives in the protocol crate), but this span is version-independent:
/// the same shared [`WindowDesire`](crate::world_report::WindowDesire)
/// type through the same shared
/// [`encode_window_target_lanes`](crate::world_sizing::encode_window_target_lanes).
fn row_tail_wire(row: &crate::world_report::WindowDesire) -> String {
    let mut out = String::from("\"clear_op\":\"");
    out.push_str(row.clear_operation.as_deref().unwrap_or_default());
    out.push('"');
    crate::world_sizing::encode_window_target_lanes(&mut out, row);
    out
}

/// Frozen target suffix within a row-tail span: from the row's `,"cmdSeq":`
/// through the state-array close (the first `]` after it); no earlier
/// array sits inside the suffix (the single `[` opens the state array).
/// Asserts the slicing assumption instead of trusting it.
fn row_suffix(tail: &str) -> &str {
    let start = tail
        .find(",\"cmdSeq\":")
        .expect("cmdSeq lane after the clear token");
    let after_start = &tail[start..];
    let end = after_start.find(']').expect("state close");
    assert_eq!(
        after_start[..end].matches('[').count(),
        1,
        "no earlier array inside the suffix"
    );
    &after_start[..end + 1]
}

#[test]
fn commandless_clear_continuation_float() {
    // Float route: the maximized move binds under op-13 and publishes
    // the one-shot clear desire; a commandless observation with
    // unmaximized facts plus the correlated accepted attempt resumes
    // the half-snap under the ORIGINAL op id. No second command, no
    // second clear, no replay.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(engine.retained_clears.len(), 1);
    assert_eq!(engine.retained_clears[0].operation_id, "op-13");
    assert_eq!(
        desire(&reply, "win-max").clear_operation.as_deref(),
        Some("op-13"),
        "exactly one clear desire under the original op"
    );
    // Commandless continuation: unmaximized facts plus the correlated
    // accepted attempt. No command rides this report.
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "proof consumes the binding exactly once"
    );
    assert_eq!(engine.retained_floats.len(), 1);
    assert_eq!(engine.retained_floats[0].operation_id, "op-13");
    let row = desire(&reply, "win-max");
    assert_eq!(row.frame_operation.as_deref(), Some("op-13"));
    assert_eq!(row.effective_frame, Some(rect(404, 8, 388, 544)));
    assert_eq!(
        row.clear_operation, None,
        "spent before any second dispatch: no re-actuation"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome == crate::world_report::OperationOutcome::ClearContinuedFloat
        }),
        "continuation observable under original op (not frame proof), got {:?}",
        reply.operation_settled
    );
    // Frame arrival at the half settles the retained float under the
    // same op: the desire withdraws, proven by observation alone.
    // Retirement of the clear above is NOT proof the frame arrived:
    // only this arrival confirms the float.
    let arrived = || WindowFact {
        rect: Some(rect(404, 8, 388, 544)),
        maximized: Some(false),
        ..maximized()
    };
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![arrived(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            Vec::new(),
        ),
    );
    assert!(
        engine.retained_floats.is_empty(),
        "arrival confirms the float"
    );
    assert_eq!(
        desire(&reply, "win-max").frame_operation,
        None,
        "confirmed desire withdraws"
    );
}

#[test]
fn tiled_maximize_publishes_clear_before_attempt() {
    // Production-shaped tiled discriminator: normal tile admission,
    // maximize retaining its slot, one directional intent, actionable
    // clear token under the original op BEFORE any attempt. The
    // production seam cannot stage an attempt from a reply without it.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(
        desire(&reply, "win-max").clear_operation.as_deref(),
        Some("op-13"),
        "tiled rows publish the retained pending-none clear token under the original op"
    );
}

#[test]
fn commandless_clear_continuation_tiled() {
    // Tiled route: the same commandless proof resumes the original
    // move tiled-as-tiled under the original op id (cross-output
    // transfer), with no directional result polluting the
    // observation-only reply.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert!(!engine.retained_clears[0].float_route);
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![cleared(), fact("win-b", "out-b", 900)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "proof consumes the binding exactly once"
    );
    assert!(
        reply.command_result.is_none(),
        "commandless continuation carries no directional result, got {:?}",
        reply.command_result
    );
    assert!(
        reply.transfers.iter().any(|transfer| {
            transfer.operation_id == "op-13" && transfer.target_output.0 == "out-b"
        }),
        "original intent resumes tiled-as-tiled under op-13, got {:?}",
        reply.transfers
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome == crate::world_report::OperationOutcome::ClearContinuedTiled
        }),
        "tiled continuation observable under original op, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_external_clear_alone_abandons_without_continuation() {
    // An externally cleared window (unmaximized facts, no correlated
    // accepted attempt) abandons the binding: never a continuation.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1);
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            Vec::new(),
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "external clear abandons the binding"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "no continuation without proof, got {:?}",
        engine.retained_floats
    );
    assert_eq!(
        desire(&reply, "win-max").frame_operation,
        None,
        "no half authorized from an external clear"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome
                    == crate::world_report::OperationOutcome::ClearAbandonedExternalClear
        }),
        "abandonment observable with original op/window/reason, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_refusal_retires_and_still_maximized_holds() {
    // Proven native refusal retires the binding with no loop; a still-
    // maximized subject with accepted-but-unproven evidence holds
    // without ever counting as success.
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    // Explicit refusal with the unchanged maximized preimage retires.
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                FrameSetter::Refused,
                Some(true),
                Some(true),
            )],
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "proven refusal retires with no loop"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "refusal authorizes nothing"
    );
    assert_eq!(
        desire(&reply, "win-max").clear_operation,
        None,
        "spent desire never re-actuates"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome == crate::world_report::OperationOutcome::ClearRefusedProven
        }),
        "refusal observable with original op/window/reason, got {:?}",
        reply.operation_settled
    );
    // Rebind, then hold on still-maximized accepted evidence that
    // proves no transition: the binding stays, the desire withholds.
    accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(15),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_clears(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-15",
                "win-max",
                FrameSetter::Accepted,
                Some(true),
                Some(true),
            )],
        ),
    );
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "still-maximized holds with the binding intact"
    );
    assert_eq!(
        desire(&reply, "win-max").clear_operation,
        None,
        "no re-actuation after an attempt"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "unchanged maximized is never success"
    );
}

#[test]
fn clear_ambiguous_throw_holds_without_retry_then_fresh_intent_supersedes() {
    // A throwing attempt latches ambiguity with no retry while still
    // maximized. A later unmaximized observation WITHOUT a new attempt
    // must NOT continue, yet must NOT abandon either: the correlated
    // attempt may be our own clear completing asynchronously, so the
    // binding holds with its staged evidence unchanged, no terminal,
    // no continuation, no second attempt. A fresh same-window intent
    // then supersedes the held binding and works independently.
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                FrameSetter::Threw,
                None,
                None,
            )],
        ),
    );
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "throw latches ambiguity with no retry"
    );
    assert!(engine.retained_clears[0].ambiguous);
    assert_eq!(
        desire(&reply, "win-max").clear_operation,
        None,
        "spent operation never re-actuates"
    );
    assert!(reply.operation_settled.is_empty(), "hold emits no terminal");
    // Later unmaximized facts with NO new attempt: the truthful spent
    // connector emits nothing more for op-13. No continuation, yet no
    // abandonment either: the held binding persists with its staged
    // Threw evidence unchanged.
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            Vec::new(),
        ),
    );
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "ambiguous hold persists with binding and pending unchanged"
    );
    assert!(engine.retained_clears[0].ambiguous);
    assert_eq!(engine.retained_clears[0].operation_id, "op-13");
    assert!(
        engine.retained_floats.is_empty(),
        "no continuation without accepted proof, got {:?}",
        engine.retained_floats
    );
    assert_eq!(
        desire(&reply, "win-max").frame_operation,
        None,
        "no half authorized without proof"
    );
    assert!(
        reply.operation_settled.is_empty(),
        "hold emits no terminal, got {:?}",
        reply.operation_settled
    );
    // A fresh same-window intent supersedes the held binding and works
    // independently: old op-13 retires with one Superseded event and
    // never continues, while the fresh move authorizes its own half
    // under the new op.
    let mut fresh = report_full(
        5,
        5,
        vec![screen("out-a", 0)],
        vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
        Vec::new(),
        "win-max",
        Some(15),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("win-max".to_owned()),
            direction: "right".to_owned(),
            same_axis: crate::directional::SameAxisMove::default(),
        }),
    );
    fresh.clear_attempts = Vec::new();
    let reply = accepted(&mut engine, &fresh);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome == crate::world_report::OperationOutcome::ClearSuperseded
        }),
        "held binding superseded with original op/window/reason, got {:?}",
        reply.operation_settled
    );
    assert!(
        !reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && matches!(
                    event.outcome,
                    crate::world_report::OperationOutcome::ClearContinuedTiled
                        | crate::world_report::OperationOutcome::ClearContinuedFloat
                )
        }),
        "superseded op-13 never continues, got {:?}",
        reply.operation_settled
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-float-applied"),
        "fresh intent works independently under its own op"
    );
    assert_eq!(engine.retained_clears.len(), 0);
    assert_eq!(engine.retained_floats.len(), 1);
    assert_eq!(engine.retained_floats[0].operation_id, "op-15");
}

#[test]
fn clear_unknown_current_focus_holds_not_abandons() {
    // Unknown CURRENT focus holds (cannot safely continue); only known
    // changed focus abandons. The binding persists with no terminal.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    // Unknown current focus ("") with unmaximized + accepted proof:
    // holds, never continues, never abandons.
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "unknown current focus holds with identities preserved"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "no continuation on unknown focus"
    );
    assert!(
        reply.operation_settled.is_empty(),
        "hold emits no terminal, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_empty_original_focus_abandons_on_known_other() {
    // Unknown ORIGINAL focus with unknown current holds, but a known
    // current focus naming another window abandons: the native
    // exact-subject proof already ties the request, so no stale intent
    // survives known wrong focus to resume later. The retained baseline
    // is never guessed or latched.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    // Bind with empty original focus.
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears[0].focus, "");
    // Unknown current focus holds first (no contradiction yet).
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1, "unknown holds");
    assert_eq!(
        engine.retained_clears[0].focus, "",
        "retained focus stays immutable, never latched"
    );
    assert!(reply.operation_settled.is_empty());
    // Known current focus on another window abandons promptly with the
    // original op/window/reason; nothing resumes later.
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-target",
            Vec::new(),
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "known wrong focus retires promptly"
    );
    assert!(engine.retained_floats.is_empty(), "nothing resumes later");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome
                    == crate::world_report::OperationOutcome::ClearAbandonedFocusChanged
                && event.cause.as_deref() == Some("non-subject-focus")
        }),
        "abandonment observable with original op/window/reason, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_unknown_baseline_subject_focus_continues() {
    // Unknown baseline plus exact-subject current focus permits
    // continuation with accepted proof; the baseline stays immutable.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(engine.retained_clears.is_empty());
    assert_eq!(engine.retained_floats.len(), 1);
    assert_eq!(engine.retained_floats[0].operation_id, "op-13");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome == crate::world_report::OperationOutcome::ClearContinuedFloat
        }),
        "continuation observable, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_known_non_subject_baseline_retires_promptly() {
    // A KNOWN original focus naming a non-subject window retires
    // promptly with `baseline-non-subject`, even when unchanged: the
    // intent was never its subject's, so it must not linger to resume
    // later.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    // Bind with a known non-subject baseline focus.
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-target",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears[0].focus, "win-target");
    // Unchanged non-subject focus still retires: never its subject's.
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-target",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "non-subject baseline retires promptly, never lingers"
    );
    assert!(engine.retained_floats.is_empty(), "nothing resumes");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome
                    == crate::world_report::OperationOutcome::ClearAbandonedFocusChanged
                && event.cause.as_deref() == Some("baseline-non-subject")
        }),
        "retirement observable with original op/window/reason, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_changed_focus_view_domain_abandons() {
    // Changed lifetime focus, displayed view, or homing abandons the
    // binding: the proof no longer names the bound intent.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    let bind = |engine: &mut WorldEngine, seq: u64, cmd: u64| {
        accepted(
            engine,
            &report(
                seq,
                seq,
                vec![screen("out-a", 0)],
                vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
                None,
                None,
            ),
        );
        accepted(
            engine,
            &report_full(
                seq + 1,
                seq + 1,
                vec![screen("out-a", 0)],
                vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
                Vec::new(),
                "win-max",
                Some(cmd),
                Some(WorldCommand::MoveDirectional {
                    window: WindowId("win-max".to_owned()),
                    direction: "right".to_owned(),
                    same_axis: crate::directional::SameAxisMove::default(),
                }),
            ),
        );
    };
    // Focus change abandons with an observable terminal.
    bind(&mut engine, 1, 13);
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-target",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(engine.retained_clears.is_empty(), "focus change abandons");
    assert!(engine.retained_floats.is_empty(), "no continuation raced");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome
                    == crate::world_report::OperationOutcome::ClearAbandonedFocusChanged
        }),
        "focus abandonment observable, got {:?}",
        reply.operation_settled
    );
    // Displayed-view change abandons. The logical-echo views lane is
    // inert native-free (and has no v4 slot), so the view move rides a
    // real SelectWorkspace command on its own report: settle runs
    // pre-command, so the proof report follows and observes the moved
    // view. The old views-echo setup never moved the view.
    bind(&mut engine, 5, 15);
    let spare = engine
        .mapping
        .get("out-a")
        .expect("ring retained")
        .iter()
        .find(|id| Some(*id) != engine.retained_clears[0].view.as_ref())
        .expect("spare workspace exists")
        .clone();
    let selected = report_full(
        7,
        7,
        vec![screen("out-a", 0)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        vec![],
        "win-max",
        Some(16),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: spare.clone(),
        }),
    );
    let _ = accepted(&mut engine, &selected);
    let reply = accepted(
        &mut engine,
        &report_clears(
            8,
            8,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-15",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(engine.retained_clears.is_empty(), "view change abandons");
    assert!(engine.retained_floats.is_empty(), "no continuation raced");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-15"
                && event.outcome == crate::world_report::OperationOutcome::ClearAbandonedViewChanged
        }),
        "view abandonment observable, got {:?}",
        reply.operation_settled
    );
    // Homing change abandons.
    bind(&mut engine, 9, 17);
    let cleared_b = || WindowFact {
        maximized: Some(false),
        transient: Some(true),
        ..fact("win-max", "out-b", 100)
    };
    let reply = accepted(
        &mut engine,
        &report_clears(
            11,
            11,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![cleared_b(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-17",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(engine.retained_clears.is_empty(), "homing change abandons");
    assert!(engine.retained_floats.is_empty(), "no continuation raced");
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-17"
                && event.outcome
                    == crate::world_report::OperationOutcome::ClearAbandonedHomingChanged
        }),
        "homing abandonment observable, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_close_prunes_and_abandoned_binding_never_reapplies() {
    // Explicit close prunes the binding; an abandoned binding can never
    // affect a later independent intent, including the same direction.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1);
    // Explicit close prunes even with a correlated attempt attached.
    let mut closed = report_clears(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![float_fact("win-target", "out-a", 300, 100)],
        "win-target",
        vec![clear_attempt(
            "op-13",
            "win-max",
            crate::world_report::FrameSetter::Accepted,
            Some(true),
            Some(false),
        )],
    );
    closed.closed = vec![WindowId("win-max".to_owned())];
    accepted(&mut engine, &closed);
    assert!(engine.retained_clears.is_empty(), "close prunes");
    assert!(engine.retained_floats.is_empty(), "close settles");
    // A later independent intent in the same direction binds fresh and
    // proceeds on its own proof, unaffected by the abandoned binding.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(15),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(engine.retained_clears.len(), 1);
    assert_eq!(engine.retained_clears[0].operation_id, "op-15");
}

#[test]
fn clear_same_window_new_intent_supersedes_without_consuming() {
    // No later user command consumes a pending binding: any same-window
    // new intent supersedes the old one and re-evaluates fresh.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears[0].operation_id, "op-13");
    // A same-window move in another direction supersedes: the old op
    // never continues, the fresh intent binds under its own op.
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(14),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "left".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(engine.retained_clears.len(), 1);
    assert_eq!(engine.retained_clears[0].operation_id, "op-14");
    assert_eq!(engine.retained_clears[0].direction, "left");
    // Proof for the SUPERSEDED op-13 can never continue anything. The
    // subject is still maximized (op-14 never dispatched), so op-14
    // holds untouched while the stale op-13 proof reads as absent.
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1, "op-14 survives");
    assert!(
        engine.retained_floats.is_empty(),
        "superseded op-13 proof authorizes nothing, got {:?}",
        engine.retained_floats
    );
    assert_eq!(
        desire(&reply, "win-max").frame_operation,
        None,
        "no continuation from a superseded op"
    );
}

#[test]
fn clear_same_report_proof_and_new_intent_supersedes_before_continuation() {
    // Ordering boundary: a single report carrying old accepted proof
    // (unmaximized + Accepted for op-13) AND a fresh same-window intent
    // must supersede the old binding BEFORE old continuation. No
    // unwanted old move; the new intent binds/proceeds on its own.
    // Covers WorldCommand intents; CoreCommand covered below.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1);
    // Same report: old proof + fresh same-window move right (op-14).
    // Facts unmaximized, so the fresh intent needs no clear and runs
    // tiled; the old op-13 must never resume.
    let mut both = report_full(
        3,
        3,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![cleared(), fact("win-b", "out-b", 900)],
        Vec::new(),
        "win-max",
        Some(14),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("win-max".to_owned()),
            direction: "right".to_owned(),
            same_axis: crate::directional::SameAxisMove::default(),
        }),
    );
    both.clear_attempts = vec![clear_attempt(
        "op-13",
        "win-max",
        crate::world_report::FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &both);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome == crate::world_report::OperationOutcome::ClearSuperseded
        }),
        "old binding superseded with original op/window/reason, got {:?}",
        reply.operation_settled
    );
    assert!(
        !reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && matches!(
                    event.outcome,
                    crate::world_report::OperationOutcome::ClearContinuedTiled
                        | crate::world_report::OperationOutcome::ClearContinuedFloat
                )
        }),
        "superseded op-13 never continues, got {:?}",
        reply.operation_settled
    );
    assert!(
        !reply.transfers.iter().any(|t| t.operation_id == "op-13"),
        "no unwanted old move, got {:?}",
        reply.transfers
    );
    assert!(
        engine.retained_clears.is_empty(),
        "fresh unmaximized intent binds no clear, old superseded"
    );
}

#[test]
fn clear_same_report_core_command_supersedes_before_continuation() {
    // Same ordering boundary through the CoreCommand route
    // (`WorldCommand::Operation`): an old proof plus a fresh
    // same-window CoreCommand move supersedes before continuation.
    use crate::boundary::CoreCommand;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let mut both = report_full(
        3,
        3,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![cleared(), fact("win-b", "out-b", 900)],
        Vec::new(),
        "win-max",
        Some(14),
        Some(WorldCommand::Operation(CoreCommand::Move {
            window: "win-max".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: crate::directional::SameAxisMove::default(),
        })),
    );
    both.clear_attempts = vec![clear_attempt(
        "op-13",
        "win-max",
        crate::world_report::FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &both);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome == crate::world_report::OperationOutcome::ClearSuperseded
        }),
        "CoreCommand supersedes too, got {:?}",
        reply.operation_settled
    );
    assert!(
        !reply.transfers.iter().any(|t| t.operation_id == "op-13"),
        "no unwanted old move, got {:?}",
        reply.transfers
    );
}

#[test]
fn clear_consumed_replay_does_not_supersede() {
    // A consumed/replayed command (seq at or below the high-watermark)
    // must NOT supersede: the old proof still continues under op-13.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    // Replay op-13 (consumed) with old proof: must still continue.
    let mut replay = report_full(
        3,
        3,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![cleared(), fact("win-b", "out-b", 900)],
        Vec::new(),
        "win-max",
        Some(13),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("win-max".to_owned()),
            direction: "right".to_owned(),
            same_axis: crate::directional::SameAxisMove::default(),
        }),
    );
    replay.clear_attempts = vec![clear_attempt(
        "op-13",
        "win-max",
        crate::world_report::FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let outcome = engine.handle_report(&replay);
    match outcome {
        WorldOutcome::CommandConsumed(reply) => {
            assert!(
                reply.transfers.iter().any(|t| t.operation_id == "op-13"),
                "consumed replay still continues old op-13, got {:?}",
                reply.transfers
            );
            assert!(
                !reply.operation_settled.iter().any(|event| {
                    event.operation_id == "op-13"
                        && event.outcome == crate::world_report::OperationOutcome::ClearSuperseded
                }),
                "consumed replay never supersedes, got {:?}",
                reply.operation_settled
            );
        }
        other => panic!("expected command-consumed, got {other:?}"),
    }
}

#[test]
fn clear_send_supersedes_without_consuming() {
    // Settlement is the sole supersession choke point for every
    // window-carrying command: an accepted same-window send supersedes
    // the pending clear binding before any continuation. A structurally
    // rejected report changes nothing transactionally (settlement is
    // discarded with the staged candidate), so only fresh accepted
    // reports supersede; consumed replays never do (see
    // `clear_consumed_replay_does_not_supersede`).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-target", "out-b", 900)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-target", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1);
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-target", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(14),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert!(
        engine.retained_clears.is_empty(),
        "same-window send supersedes the binding"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "supersession authorizes no clear continuation"
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|transfer| transfer.operation_id == "op-14"),
        "the send itself retains under its own op, got {:?}",
        reply.transfers
    );
}

#[test]
fn clear_same_report_refused_intent_still_supersedes() {
    // Supersession precedes continuation for ANY fresh intent on a
    // structurally accepted report, even when the new command itself
    // refuses: one old Superseded event, no old continuation, and the
    // new refusal stays truthful. An unwanted old move is worse than a
    // refused new intent.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1);
    // Same report: old accepted proof plus a fresh same-window move on
    // a now-fullscreen subject. The new command refuses
    // (`move-refused-fullscreen`), yet the old binding still
    // supersedes first and never continues.
    let fullscreened = || WindowFact {
        fullscreen: Some(true),
        maximized: Some(false),
        ..maximized()
    };
    let mut both = report_full(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![fullscreened(), float_fact("win-target", "out-a", 300, 100)],
        Vec::new(),
        "win-max",
        Some(14),
        Some(WorldCommand::MoveDirectional {
            window: WindowId("win-max".to_owned()),
            direction: "right".to_owned(),
            same_axis: crate::directional::SameAxisMove::default(),
        }),
    );
    both.clear_attempts = vec![clear_attempt(
        "op-13",
        "win-max",
        crate::world_report::FrameSetter::Accepted,
        Some(true),
        Some(false),
    )];
    let reply = accepted(&mut engine, &both);
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-refused-fullscreen"),
        "new refusal stays truthful"
    );
    assert_eq!(
        reply.operation_settled.len(),
        1,
        "exactly one old event, got {:?}",
        reply.operation_settled
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome == crate::world_report::OperationOutcome::ClearSuperseded
        }),
        "old binding superseded with original op/window/reason, got {:?}",
        reply.operation_settled
    );
    assert!(
        !reply.transfers.iter().any(|t| t.operation_id == "op-13"),
        "no unwanted old move, got {:?}",
        reply.transfers
    );
    assert!(
        engine.retained_floats.is_empty(),
        "refused continuation authorizes nothing"
    );
    assert!(
        engine.retained_clears.is_empty(),
        "refused fresh intent binds no clear and old is gone"
    );
}

#[test]
fn clear_held_and_unreadable_withhold_without_weakening() {
    // Held scopes (gap inventory, gesture barrier) and unreadable
    // facts withhold the desire and hold the binding with identities
    // preserved; authority never weakens and nothing re-actuates.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        desire(&reply, "win-max").clear_operation.as_deref(),
        Some("op-13")
    );
    // Gap inventory holds with the desire withheld.
    let mut gapped = report_clears(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        "win-max",
        vec![clear_attempt(
            "op-13",
            "win-max",
            crate::world_report::FrameSetter::Accepted,
            Some(true),
            Some(false),
        )],
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "gap holds with identities preserved"
    );
    assert_eq!(
        desire(&reply, "win-max").clear_operation,
        None,
        "gap withholds the desire"
    );
    // Gesture barrier holds the same way.
    let mut gestured = report_clears(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        "win-max",
        Vec::new(),
    );
    gestured.gesture_barrier = true;
    let reply = accepted(&mut engine, &gestured);
    assert_eq!(engine.retained_clears.len(), 1);
    assert_eq!(
        desire(&reply, "win-max").clear_operation,
        None,
        "gesture withholds the desire"
    );
}

#[test]
fn clear_held_domain_defers_continuation_until_authority_returns() {
    // Held-domain defers both float and tiled continuation until
    // authority returns (never silently drops). An incomplete home
    // screen holds the proven clear; a later complete observation
    // resumes on the staged proof with no second attempt.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    // Incomplete home screen with unmaximized + accepted proof: holds,
    // never continues, never retires.
    let mut held_screen = screen("out-a", 0);
    held_screen.complete = false;
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![held_screen],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert_eq!(engine.retained_clears.len(), 1, "held defers, never drops");
    assert!(
        engine.retained_floats.is_empty(),
        "no continuation while held"
    );
    assert!(
        reply.operation_settled.is_empty(),
        "hold emits no terminal, got {:?}",
        reply.operation_settled
    );
    // Authority returns: the staged proof resumes with no second
    // attempt (production once-only lane cannot restage it).
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            Vec::new(),
        ),
    );
    assert!(engine.retained_clears.is_empty());
    assert_eq!(engine.retained_floats.len(), 1);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.outcome == crate::world_report::OperationOutcome::ClearContinuedFloat
        }),
        "deferred continuation resumes, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn clear_continuation_refusal_observable_with_original_op() {
    // Post-clear continuation refusal must be observable with the
    // original op id/window/reason, not silently disappearing. A proven
    // clear (unmaximized + Accepted) whose half violates declared
    // constraints retires with `continuation-refused` and authorizes
    // nothing.
    use crate::world_report::FrameSetter;
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut maximized = WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    // Declared minimum wider than any half: the half can never satisfy it.
    maximized.hints.as_mut().expect("hints").min_w = Some(10_000);
    let mut cleared = maximized.clone();
    cleared.maximized = Some(false);
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![
                maximized.clone(),
                float_fact("win-target", "out-a", 300, 100),
            ],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![
                maximized.clone(),
                float_fact("win-target", "out-a", 300, 100),
            ],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared, float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    assert!(engine.retained_clears.is_empty(), "refusal retires");
    assert!(
        engine.retained_floats.is_empty(),
        "refused continuation authorizes nothing"
    );
    assert_eq!(
        desire(&reply, "win-max").frame_operation,
        None,
        "no half authorized"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == "op-13"
                && event.window.0 == "win-max"
                && event.outcome == crate::world_report::OperationOutcome::ClearContinuationRefused
                && event.cause.as_deref() == Some("constraints")
        }),
        "continuation refusal observable, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn maximized_clear_continuation_binds_lifetime_domain_view_focus() {
    // Binding shape: the fresh maximized move binds window, direction,
    // home domain, displayed view, observed focus, and the original
    // observation sequence under the originating op. Focus fences
    // without binding a clear.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    let focus = || WorldCommand::FocusDirectional {
        window: WindowId("win-max".to_owned()),
        direction: "right".to_owned(),
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Some(12),
            Some(focus()),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("focus-refused-maximize")
    );
    assert!(
        engine.retained_clears.is_empty(),
        "focus fences without binding a clear"
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(engine.retained_clears.len(), 1);
    let retained = &engine.retained_clears[0];
    assert_eq!(retained.window, WindowId("win-max".to_owned()));
    assert_eq!(retained.direction, "right");
    assert_eq!(retained.operation_id, "op-13");
    // Unknown windows refuse stale with the preserved cause instead of
    // a bare rejection: focus and move share one vocabulary.
    let unknown = [
        (
            WorldCommand::FocusDirectional {
                window: WindowId("win-gone".to_owned()),
                direction: "right".to_owned(),
            },
            "focus-float-refused-stale",
        ),
        (
            WorldCommand::MoveDirectional {
                window: WindowId("win-gone".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            },
            "move-float-refused-stale",
        ),
    ];
    for (index, (command, outcome)) in unknown.into_iter().enumerate() {
        let seq = 20 + index as u64;
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                seq,
                vec![screen("out-a", 0)],
                vec![
                    WindowFact {
                        maximized: Some(false),
                        ..maximized()
                    },
                    float_fact("win-target", "out-a", 300, 100),
                ],
                Some(40 + seq),
                Some(command),
            ),
        );
        assert_eq!(outcome_str(&reply.command_result), Some(outcome));
        assert_eq!(
            reply
                .command_result
                .as_ref()
                .and_then(|result| match result {
                    CommandResult::Directional { cause, .. } => cause.clone(),
                    _ => None,
                }),
            Some("unknown-window".to_owned())
        );
        assert!(
            reply.effects.is_empty(),
            "stale refusals stay atomic, got {:?}",
            reply.effects
        );
    }
}

#[test]
fn sticky_maximize_publishes_clear_before_attempt() {
    // Sticky publication-first: a maximized sticky binds NeedClear and
    // publishes the one-shot token under the original op before any
    // attempt, through the same retained lane as floats.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized_sticky = || WindowFact {
        sticky: Some(true),
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-sticky", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![
                maximized_sticky(),
                float_fact("win-target", "out-a", 300, 100),
            ],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![
                maximized_sticky(),
                float_fact("win-target", "out-a", 300, 100),
            ],
            Vec::new(),
            "win-sticky",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-sticky".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert_eq!(
        desire(&reply, "win-sticky").clear_operation.as_deref(),
        Some("op-13"),
        "sticky rows publish the retained pending-none clear token"
    );
}

#[test]
fn float_settled_events_are_one_shot_and_replay_cached() {
    // Float verdicts ride the same one-shot `operation_settled`
    // carrier: native observation at the retained target confirms with
    // a `float-frame-proven` event (never setter success alone); the
    // next unrelated report publishes empty; a duplicate delivery
    // repeats the cached reply with its events.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![
                float_fact("win-subject", "out-a", 100, 100),
                float_fact("win-target", "out-a", 300, 100),
            ],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![
                float_fact("win-subject", "out-a", 100, 100),
                float_fact("win-target", "out-a", 300, 100),
            ],
            Some(11),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-subject".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-float-applied")
    );
    assert!(reply.operation_settled.is_empty(), "bind emits nothing");
    // Native observation arrives at the retained half with no frame
    // attempts at all: observation alone proves the frame.
    let half = rect(404, 8, 388, 544);
    let arrived = || WindowFact {
        rect: Some(half),
        ..float_fact("win-subject", "out-a", 100, 100)
    };
    let confirming = report(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![arrived(), float_fact("win-target", "out-a", 300, 100)],
        None,
        None,
    );
    let reply = accepted(&mut engine, &confirming);
    assert!(
        engine.retained_floats.is_empty(),
        "arrival confirms the float"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.window.0 == "win-subject"
                && event.outcome == crate::world_report::OperationOutcome::FloatFrameProven
        }),
        "frame-proven event carries op/window/outcome, got {:?}",
        reply.operation_settled
    );
    // Replay of the identical report repeats the cached reply with its
    // events instead of settling twice.
    match engine.handle_report(&confirming) {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.operation_settled, reply.operation_settled);
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    // The next unrelated observation publishes empty: one-shot, never
    // retained across reports.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![arrived(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    assert!(
        reply.operation_settled.is_empty(),
        "one-shot events never leak, got {:?}",
        reply.operation_settled
    );
}

#[test]
fn desire_cmdseq_explicit_clear_retained_across_observation_only_replies() {
    // Typed correlation: the published explicit clear token carries the
    // originating command sequence, retained across observation-only
    // replies. Never a parsed id, never the latest report sequence:
    // no published token means no sequence.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    let row = desire(&reply, "win-max");
    assert_eq!(row.clear_operation.as_deref(), Some("op-13"));
    assert_eq!(row.command_seq, Some(13), "explicit token carries origin");
    // Wire bytes: the frozen target suffix rides directly after the
    // clear token with the quoted original sequence.
    let tail = row_tail_wire(row);
    assert!(tail.contains(
        "\"clear_op\":\"op-13\",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    ));
    assert_eq!(
        row_suffix(&tail),
        ",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    );
    // No new visibility or state authority on any branch.
    for row in &reply.windows {
        assert_eq!(
            row.visibility, None,
            "no visibility target: {}",
            row.lifetime.0
        );
        assert_eq!(
            row.visibility_authority,
            ScopeAuthority::Untouched,
            "no visibility authority: {}",
            row.lifetime.0
        );
        assert!(row.state.is_empty(), "no state targets: {}", row.lifetime.0);
    }
    // Observation-only reply: same facts, no command. The token and its
    // typed sequence persist; nothing re-mints from the report sequence.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![maximized(), fact("win-b", "out-b", 900)],
            None,
            None,
        ),
    );
    let row = desire(&reply, "win-max");
    assert_eq!(row.clear_operation.as_deref(), Some("op-13"));
    assert_eq!(
        row.command_seq,
        Some(13),
        "retained across observation-only"
    );
    let tail = row_tail_wire(row);
    assert!(tail.contains(
        "\"clear_op\":\"op-13\",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    ));
    assert_eq!(
        row_suffix(&tail),
        ",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    );
}

#[test]
fn desire_cmdseq_cleared_with_withheld_or_spent_token() {
    // Withheld (gap, gesture) and spent (dispatched, proof pending)
    // tokens publish no sequence while the binding survives.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(desire(&reply, "win-max").command_seq, Some(13));
    // Gapped inventory stages the dispatched attempt as pending before
    // any hold and withholds the spent desire with identities preserved.
    let mut gapped = report_clears(
        3,
        3,
        vec![screen("out-a", 0)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        "win-max",
        vec![clear_attempt(
            "op-13",
            "win-max",
            crate::world_report::FrameSetter::Accepted,
            Some(true),
            Some(false),
        )],
    );
    gapped.inventory_complete = false;
    let reply = accepted(&mut engine, &gapped);
    assert_eq!(engine.retained_clears.len(), 1, "gap keeps the binding");
    let row = desire(&reply, "win-max");
    assert_eq!(row.clear_operation, None);
    assert_eq!(row.command_seq, None, "withheld token carries no sequence");
    // Gesture barrier withholds the same way.
    let mut gestured = report_clears(
        4,
        4,
        vec![screen("out-a", 0)],
        vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
        "win-max",
        Vec::new(),
    );
    gestured.gesture_barrier = true;
    let reply = accepted(&mut engine, &gestured);
    assert_eq!(engine.retained_clears.len(), 1);
    let row = desire(&reply, "win-max");
    assert_eq!(row.clear_operation, None);
    assert_eq!(row.command_seq, None, "withheld token carries no sequence");
    // Dispatched attempt, still maximized (no proof): the token is spent
    // while the binding holds ambiguous, so no sequence publishes.
    let reply = accepted(
        &mut engine,
        &report_clears(
            5,
            5,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(true),
            )],
        ),
    );
    assert_eq!(
        engine.retained_clears.len(),
        1,
        "unproven holds, never retires"
    );
    let row = desire(&reply, "win-max");
    assert_eq!(row.clear_operation, None, "spent token never re-actuates");
    assert_eq!(row.command_seq, None, "spent token carries no sequence");
}

#[test]
fn desire_cmdseq_float_continuation_preserves_original_seq() {
    // Float-route continuation resumes under the ORIGINAL operation id
    // and its original command sequence: no second command, no re-mint.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let maximized = || WindowFact {
        maximized: Some(true),
        transient: Some(true),
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: Some(false),
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![maximized(), float_fact("win-target", "out-a", 300, 100)],
            Vec::new(),
            "win-max",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-max".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(desire(&reply, "win-max").command_seq, Some(13));
    // Commandless proof resumes the half-snap: the spent clear withdraws
    // while the continued float keeps the original sequence.
    let reply = accepted(
        &mut engine,
        &report_clears(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            vec![clear_attempt(
                "op-13",
                "win-max",
                crate::world_report::FrameSetter::Accepted,
                Some(true),
                Some(false),
            )],
        ),
    );
    let row = desire(&reply, "win-max");
    assert_eq!(row.frame_operation.as_deref(), Some("op-13"));
    assert_eq!(row.command_seq, Some(13), "continuation keeps original seq");
    assert_eq!(row.clear_operation, None, "spent clear never re-actuates");
    assert!(
        row.floating,
        "continued float still publishes floating: {row:?}"
    );
    assert_eq!(
        row.float_origin, None,
        "ordinary float carries no origin: {row:?}"
    );
    // Tail bytes: the spent clear publishes nothing while the continued
    // float keeps the original quoted sequence after the frame token.
    let tail = row_tail_wire(row);
    assert!(tail.contains(
        "\"clear_op\":\"\",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    ));
    assert_eq!(
        row_suffix(&tail),
        ",\"cmdSeq\":\"13\",\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    );
    // Frame arrival confirms the float: both tokens withdraw, no sequence.
    let arrived = || WindowFact {
        rect: Some(rect(404, 8, 388, 544)),
        maximized: Some(false),
        ..maximized()
    };
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![arrived(), float_fact("win-target", "out-a", 300, 100)],
            "win-max",
            Vec::new(),
        ),
    );
    let row = desire(&reply, "win-max");
    assert_eq!(row.frame_operation, None);
    assert_eq!(row.command_seq, None, "confirmed desire withdraws sequence");
    assert!(
        row.floating,
        "confirmed float still publishes floating: {row:?}"
    );
    assert_eq!(
        row.float_origin, None,
        "ordinary float carries no origin: {row:?}"
    );
    // Tail bytes: both tokens spent means a null correlation suffix.
    let tail = row_tail_wire(row);
    assert!(tail.contains(
        "\"clear_op\":\"\",\"cmdSeq\":null,\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    ));
    assert_eq!(
        row_suffix(&tail),
        ",\"cmdSeq\":null,\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    );
}

#[test]
fn desire_cmdseq_explicit_float_retained_then_withdrawn() {
    // Explicit float placement publishes its command sequence until the
    // placement settles; ordinary floats and withdrawals carry none.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let tiled = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    accepted(
        &mut engine,
        &report(1, 1, vec![screen("out-a", 0)], tiled(), None, None),
    );
    let target = rect(10, 10, 200, 150);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            tiled(),
            Vec::new(),
            "",
            Some(9),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: Some(target),
                },
            )),
        ),
    );
    let row = desire(&reply, "win-a");
    assert!(row.floating, "placement floats: {row:?}");
    assert_eq!(row.frame_operation.as_deref(), Some("op-9"));
    assert_eq!(row.command_seq, Some(9), "explicit token carries origin");
    assert_eq!(row.visibility, None);
    assert_eq!(row.visibility_authority, ScopeAuthority::Untouched);
    assert!(row.state.is_empty());
    // Observation-only reply retains the token and its sequence.
    let reply = accepted(
        &mut engine,
        &report(3, 3, vec![screen("out-a", 0)], tiled(), None, None),
    );
    let row = desire(&reply, "win-a");
    assert_eq!(row.frame_operation.as_deref(), Some("op-9"));
    assert_eq!(row.command_seq, Some(9), "retained across observation-only");
    // Unfloat withdraws the placement: no token, no sequence.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            tiled(),
            Vec::new(),
            "",
            Some(10),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: None,
                },
            )),
        ),
    );
    let row = desire(&reply, "win-a");
    assert_eq!(row.frame_operation, None, "withdrawn: {row:?}");
    assert_eq!(row.command_seq, None, "withdrawn carries no sequence");
}

#[test]
fn desire_cmdseq_born_clear_is_commandless() {
    // Born commandless clears ride the token lane with no command
    // correlation: the binding id comes from the core born serial.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            birth_clear: true,
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0)],
            vec![WindowFact {
                maximized: Some(true),
                ..fact("win-born", "out-a", 100)
            }],
            None,
            None,
        ),
    );
    let row = desire(&reply, "win-born");
    let op = row
        .clear_operation
        .clone()
        .expect("born binds a clear token");
    assert!(op.starts_with("op-born-"), "commandless id, got {op}");
    assert_eq!(row.command_seq, None, "born clear carries no sequence");
    // Tail bytes: commandless born token rides with a null correlation.
    let tail = row_tail_wire(row);
    assert!(tail.contains(&format!(
        "\"clear_op\":\"{op}\",\"cmdSeq\":null,\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    )));
    assert_eq!(
        row_suffix(&tail),
        ",\"cmdSeq\":null,\"visibility\":null,\
         \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
         \"state\":[]"
    );
    assert_eq!(row.visibility, None);
    assert_eq!(row.visibility_authority, ScopeAuthority::Untouched);
    assert!(row.state.is_empty());
    // Observation-only reply retains the token, still commandless.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![WindowFact {
                maximized: Some(true),
                ..fact("win-born", "out-a", 100)
            }],
            None,
            None,
        ),
    );
    let row = desire(&reply, "win-born");
    assert_eq!(row.clear_operation.as_deref(), Some(op.as_str()));
    assert_eq!(row.command_seq, None, "retained born stays commandless");
}

#[test]
fn state_property_wire_tokens_are_closed() {
    // Closed typed vocabulary for the outbound state lane, consistent
    // with the existing as_str conventions.
    use crate::world_report::StateProperty;
    assert_eq!(StateProperty::Fullscreen.as_str(), "fullscreen");
    assert_eq!(StateProperty::Maximized.as_str(), "maximized");
    assert_eq!(StateProperty::Sticky.as_str(), "sticky");
    assert_eq!(StateProperty::Above.as_str(), "above");
}

#[test]
fn newer_clear_intent_supersedes_retained_float_placement() {
    // Normative newer-intent supersession: both tokens clear on newer
    // intent, so a NeedClear for a window with an outstanding float
    // placement drops the stale placement at mint. The reply publishes
    // only the clear token under the new sequence, never a stale
    // placement alongside it.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let tiled = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    // R1: admit tiles.
    accepted(
        &mut engine,
        &report(1, 1, vec![screen("out-a", 0)], tiled(), None, None),
    );
    // R2: cmd9 floats win-a with an explicit placement.
    let target = rect(10, 10, 200, 150);
    let reply = accepted(
        &mut engine,
        &report_full(
            2,
            2,
            vec![screen("out-a", 0)],
            tiled(),
            Vec::new(),
            "",
            Some(9),
            Some(WorldCommand::Operation(
                crate::boundary::CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: Some(target),
                },
            )),
        ),
    );
    assert_eq!(
        desire(&reply, "win-a").frame_operation.as_deref(),
        Some("op-9")
    );
    assert_eq!(engine.retained_floats.len(), 1);
    // R3: observation-only maximized report; the placement is still
    // outstanding with its frame unchanged.
    let maximized_a = || WindowFact {
        maximized: Some(true),
        ..fact("win-a", "out-a", 0)
    };
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0)],
            vec![maximized_a(), fact("win-b", "out-a", 100)],
            None,
            None,
        ),
    );
    assert_eq!(
        desire(&reply, "win-a").frame_operation.as_deref(),
        Some("op-9"),
        "placement survives observation-only"
    );
    // R4: cmd13 directional move needs a clear; the newer intent
    // supersedes the stale placement.
    let reply = accepted(
        &mut engine,
        &report_full(
            4,
            4,
            vec![screen("out-a", 0)],
            vec![maximized_a(), fact("win-b", "out-a", 100)],
            Vec::new(),
            "win-a",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::default(),
            }),
        ),
    );
    assert_eq!(
        outcome_str(&reply.command_result),
        Some("move-maximize-clear")
    );
    assert!(
        engine.retained_floats.is_empty(),
        "newer clear intent drops the stale placement"
    );
    assert_eq!(engine.retained_clears.len(), 1);
    let row = desire(&reply, "win-a");
    assert_eq!(
        row.frame_operation, None,
        "no stale placement alongside the new clear: {row:?}"
    );
    assert_eq!(
        row.clear_operation.as_deref(),
        Some("op-13"),
        "only the new clear token publishes"
    );
    assert_eq!(
        row.command_seq,
        Some(13),
        "slot carries the surviving token, got {row:?}"
    );
}
