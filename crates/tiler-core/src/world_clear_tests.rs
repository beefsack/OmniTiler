//! Directional maximize-clear lifecycle regressions: publication-first
//! admission/clear/continuation sequences, supersession ordering, focus and
//! view abandonment, held-domain deferral, and terminal observability.
//!
//! Split from [`crate::world_directional`] tests for source size: the
//! directional policy unit tests stay there; every test here drives the
//! retained clear lifecycle through [`crate::world_engine::WorldEngine`].
//! Shared report builders live in [`crate::world_directional::tests`].

use crate::directional::{OutputId, WindowId};
use crate::world_directional::tests::{
    accepted, fact, float_fact, outcome_str, rect, report, report_full, screen,
};
use crate::world_engine::WorldEngine;
use crate::world_report::{CommandResult, ScreenReport, WindowFact, WorldCommand, WorldReport};

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
) -> WorldReport {
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

pub(crate) fn desire<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .expect("window keeps a row")
}

#[test]
fn commandless_clear_continuation_float() {
    // Float route: the maximized move binds under op-13 and publishes
    // the one-shot clear desire; a commandless observation with
    // unmaximized facts plus the correlated accepted attempt resumes
    // the half-snap under the ORIGINAL op id. No second command, no
    // second clear, no replay.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        maximized: false,
        ..maximized()
    };
    let reply = accepted(
        &mut engine,
        &report_clears(
            4,
            4,
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    let bind = |engine: &mut WorldEngine, seq: u64, cmd: u64| {
        accepted(
            engine,
            &report(
                seq,
                seq,
                vec![screen("out-a", 0, true)],
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
                vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    // Displayed-view change abandons.
    bind(&mut engine, 5, 15);
    let spare = engine
        .mapping
        .get("out-a")
        .expect("ring retained")
        .iter()
        .find(|id| Some(*id) != engine.retained_clears[0].view.as_ref())
        .expect("spare workspace exists")
        .clone();
    let mut viewed = report_clears(
        7,
        7,
        vec![screen("out-a", 0, true)],
        vec![cleared(), float_fact("win-target", "out-a", 300, 100)],
        "win-max",
        vec![clear_attempt(
            "op-15",
            "win-max",
            crate::world_report::FrameSetter::Accepted,
            Some(true),
            Some(false),
        )],
    );
    viewed.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-a".to_owned()),
        view: spare,
    }];
    let reply = accepted(&mut engine, &viewed);
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
        maximized: false,
        floating: true,
        ..fact("win-max", "out-b", 100)
    };
    let reply = accepted(
        &mut engine,
        &report_clears(
            11,
            11,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
        vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
        vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
        vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
        crate::world_engine::WorldOutcome::CommandConsumed(reply) => {
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 0)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        fullscreen: true,
        maximized: false,
        ..maximized()
    };
    let mut both = report_full(
        3,
        3,
        vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        vec![screen("out-a", 0, true)],
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
        vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    let cleared = || WindowFact {
        maximized: false,
        ..maximized()
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut held_screen = screen("out-a", 0, true);
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut maximized = WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    // Declared minimum wider than any half: the half can never satisfy it.
    maximized.hints.min_w = Some(10_000);
    let mut cleared = maximized.clone();
    cleared.maximized = false;
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized = || WindowFact {
        floating: true,
        maximized: true,
        ..fact("win-max", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
                vec![screen("out-a", 0, true)],
                vec![
                    WindowFact {
                        maximized: false,
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let maximized_sticky = || WindowFact {
        floating: true,
        sticky: true,
        maximized: true,
        ..fact("win-sticky", "out-a", 100)
    };
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![screen("out-a", 0, true)],
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
            vec![screen("out-a", 0, true)],
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
        vec![screen("out-a", 0, true)],
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
        crate::world_engine::WorldOutcome::Duplicate(cached) => {
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
            vec![screen("out-a", 0, true)],
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
