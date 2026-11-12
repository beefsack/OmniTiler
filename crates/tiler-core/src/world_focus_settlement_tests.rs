//! Ordinary focus settlement: one pending slot binds actually emitted
//! `awaitShown=false` focus requests to the first exact opaque-tuple
//! feedback row, with target-equality satisfaction, terminal
//! refusal/no-call retirement, and inert unproven retention.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_close_focus::PendingFocus;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    FocusFeedback, PresentationRowV4, RowDiagnosticKind, VisibilityReason, VisibilitySetter,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_v4_support_tests::empty_ingress;
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> crate::world_report::ScreenReport {
    crate::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 600)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
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
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
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
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    closed: Vec<&str>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0)],
        windows,
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
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
        presentation: PresentationRowV4::Valid(empty_ingress()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn feedback_row(
    id: &str,
    target: &str,
    revision: u64,
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
    attempt_ms: u64,
) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter,
        reason,
        pre: pre.map(|id| WindowId(id.to_owned())),
        post: post.map(|id| WindowId(id.to_owned())),
        foreground_proven: proven,
        // No arrival brackets anywhere: verdicts never require readiness
        // evidence.
        arrival: Vec::new(),
        attempt_ms,
    }
}

fn with_feedback(mut base: WorldReportV4, rows: Vec<FocusFeedback>) -> WorldReportV4 {
    base.focus_feedback = rows;
    base
}

fn test_engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn focus_command(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: WindowId(window.to_owned()),
        direction: "left".to_owned(),
    }
}

/// Real explicit-command grant for `window` via command `seq`: asserts the
/// wire-retained request, then returns the frozen single-slot pending row
/// (opaque id, target, issuing revision) for exact-tuple feedback.
fn explicit_grant(engine: &mut WorldEngine, seq: u64, window: &str) -> PendingFocus {
    let reply = accepted(
        engine,
        &report(
            seq,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            Some(seq),
            Some(focus_command(window)),
        ),
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, window, "wire target: {reply:?}");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand,
        "wire cause: {reply:?}"
    );
    assert_eq!(request.cmd_seq, Some(seq), "wire cmd_seq: {reply:?}");
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    engine.pending_focus.clone().expect("pending slot")
}

fn bootstrap_two(engine: &mut WorldEngine) {
    accepted(
        engine,
        &report(
            1,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
}

/// Pending-slot read helpers: the single owner retains at most one row.
fn pending(engine: &WorldEngine) -> PendingFocus {
    engine.pending_focus.clone().expect("pending slot")
}

/// Verdict of the latched first row, if any: absence reads `None`.
fn latched_verdict(slot: &PendingFocus) -> Option<FocusVerdict> {
    slot.feedback
        .as_ref()
        .map(|row| classify_focus_feedback(&slot.target, row))
}

fn assert_pending_feedback_none(engine: &WorldEngine, what: &str) {
    assert_eq!(latched_verdict(&pending(engine)), None, "{what}");
}

#[test]
fn explicit_grant_settles_proven_without_arrival_or_current_focus() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    assert_pending_feedback_none(&engine, "fresh grant correlates nothing yet");
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    // A stale current-foreground fact naming another window retires the
    // historical slot (an observed focus change supersedes it) and can
    // never roll memory back to the proven target: the genuine observation
    // legitimately advances memory through the existing observation path
    // (never through settlement).
    let settle = with_feedback(
        report(
            3,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
        vec![row.clone()],
    );
    let memory = engine.last_active_window.clone();
    let domain = memory
        .keys()
        .next()
        .cloned()
        .expect("command records memory");
    assert_eq!(memory.get(&domain), Some(&WindowId("w0".to_owned())));
    let reply = accepted(&mut engine, &settle);
    assert!(
        reply.effects.is_empty(),
        "settlement emits nothing: {reply:?}"
    );
    assert!(reply.transfers.is_empty());
    assert!(reply.operation_settled.is_empty());
    assert_eq!(reply.command_result, None);
    // Accepted focus memory follows the genuine observation (w1) on the
    // followup: observing the target satisfies the slot, and memory never
    // rolls back to the proven target (w0).
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(row));
    assert_eq!(latched_verdict(&slot), Some(FocusVerdict::Proven));
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w1",
            vec![],
            None,
            None,
        ),
    );
    assert!(reply.effects.is_empty());
    assert!(
        engine.pending_focus.is_none(),
        "observing the target satisfies the slot"
    );
    assert_eq!(
        engine.last_active_window.get(&domain),
        Some(&WindowId("w1".to_owned()))
    );
    // A further observation-only followup keeps that memory with no retry.
    let reply = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(reply.effects.is_empty());
    assert_eq!(
        engine.last_active_window.get(&domain),
        Some(&WindowId("w1".to_owned()))
    );
}

#[test]
fn opaque_tuple_matching_ignores_forged_and_mismatched_rows() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let good = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    let mut forged_id = good.clone();
    forged_id.id = format!("{}-forged", grant.id);
    let mut wrong_rev = good.clone();
    wrong_rev.revision = grant.issued_revision + 1;
    let mut wrong_target = good.clone();
    wrong_target.target = WindowId("w1".to_owned());
    // Same spelling family, wrong opaque identity: none may mint or latch.
    let reply = accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![forged_id, wrong_rev, wrong_target],
        ),
    );
    assert!(reply.effects.is_empty());
    assert_pending_feedback_none(&engine, "mismatches latch nothing");
    // The exact tuple latches on a later report.
    let reply = accepted(
        &mut engine,
        &with_feedback(
            report(
                4,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![good.clone()],
        ),
    );
    assert!(reply.effects.is_empty());
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(good));
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
}

#[test]
fn proof_holds_despite_nonaccepted_setter() {
    // Equality proof latches regardless of setter while the row is
    // non-terminal; informative Threw/NotCalled rows retire instead (see
    // terminal_no_call_retires_the_single_slot).
    for (setter, reason) in [
        (VisibilitySetter::Accepted, VisibilityReason::NativeRefused),
        (VisibilitySetter::Refused, VisibilityReason::NativeRefused),
        (VisibilitySetter::Threw, VisibilityReason::Empty),
        (VisibilitySetter::NotCalled, VisibilityReason::Empty),
    ] {
        let mut engine = test_engine();
        bootstrap_two(&mut engine);
        let grant = explicit_grant(&mut engine, 2, "w0");
        let row = feedback_row(
            &grant.id,
            "w0",
            grant.issued_revision,
            setter,
            reason,
            Some("w-fore"),
            Some("w0"),
            true,
            9,
        );
        accepted(
            &mut engine,
            &with_feedback(
                report(
                    3,
                    vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                    "",
                    vec![],
                    None,
                    None,
                ),
                vec![row],
            ),
        );
        let slot = pending(&engine);
        assert_eq!(
            classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
            FocusVerdict::Proven,
            "setter {setter:?} never blocks equality proof"
        );
    }
}

#[test]
fn proof_requires_equality_even_when_proven_flag_set() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    // Proven flag with a wrong post is not proof of this grant.
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w1"),
        true,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Unproven
    );
    // Post equality without the proof flag is not proof either.
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        false,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Unproven
    );
}

/// One verdict-table probe: the pure classifier on one correlated row for
/// the w0 grant. Engine settlement behavior (retain vs retire) is covered
/// by the pending-slot tests below; this table pins the verdict function.
fn settle_once(
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
) -> FocusVerdict {
    classify_focus_feedback(
        &WindowId("w0".to_owned()),
        &feedback_row("fx-test", "w0", 7, setter, reason, pre, post, proven, 9),
    )
}

#[test]
fn conservative_refused_unproven_table() {
    // Refused: refused setter with a readable unchanged non-target
    // foreground around the attempt.
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::NativeRefused,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Refused
    );
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Refused
    );
    // Proof beats the refusal shape: equality proof is not setter success.
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w0"),
            Some("w0"),
            true
        ),
        FocusVerdict::Proven
    );
    // The proof flag alone never proves: refused setter with an
    // unchanged non-target foreground stays refused even when the
    // foreground flag is set, because post is not the granted target.
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w-fore"),
            true
        ),
        FocusVerdict::Refused
    );
    // Everything else is unproven: changed foreground under refusal.
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w-other"),
            false
        ),
        FocusVerdict::Unproven
    );
    // Refused setter with pre/post equal to the target is not refusal
    // evidence (and without proof, not proof either).
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w0"),
            Some("w0"),
            false
        ),
        FocusVerdict::Unproven
    );
    // Refused setter with unknown/null facts is unproven, never refused.
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            None,
            Some("w-fore"),
            false
        ),
        FocusVerdict::Unproven
    );
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            Some("w-fore"),
            None,
            false
        ),
        FocusVerdict::Unproven
    );
    assert_eq!(
        settle_once(
            VisibilitySetter::Refused,
            VisibilityReason::Empty,
            None,
            None,
            false
        ),
        FocusVerdict::Unproven
    );
    // Reason alone never refuses: accepted setter with a refusal reason and
    // an unchanged non-target foreground stays unproven.
    assert_eq!(
        settle_once(
            VisibilitySetter::Accepted,
            VisibilityReason::NativeRefused,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Unproven
    );
    // Accepted without equality is unproven.
    assert_eq!(
        settle_once(
            VisibilitySetter::Accepted,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Unproven
    );
    // Blocked no-call and threw rows are unproven, even with an unchanged
    // non-target foreground.
    assert_eq!(
        settle_once(
            VisibilitySetter::NotCalled,
            VisibilityReason::Unreadable,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Unproven
    );
    assert_eq!(
        settle_once(
            VisibilitySetter::Threw,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w-fore"),
            false
        ),
        FocusVerdict::Unproven
    );
    // Null facts are unproven.
    assert_eq!(
        settle_once(
            VisibilitySetter::NotCalled,
            VisibilityReason::Unreadable,
            None,
            None,
            false
        ),
        FocusVerdict::Unproven
    );
}

#[test]
fn first_correlated_row_wins_within_and_across_reports() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let first = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        5,
    );
    let mut second = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::NativeRefused,
        Some("w-fore"),
        Some("w-fore"),
        false,
        7,
    );
    second.attempt_ms = 7;
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![first.clone(), second],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    assert_eq!(slot.feedback, Some(first));
    // A later report carrying the same tuple with different facts cannot
    // replace the latched row.
    let late = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        11,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                4,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![late],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    assert!(
        slot.feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 5),
        "original stamp preserved"
    );
}

#[test]
fn later_contradictions_never_upgrade_a_latched_attempt() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    // Latch a non-terminal unproven row first (accepted without equality
    // retains the slot).
    let noproof = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        5,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![noproof.clone()],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Unproven
    );
    assert_eq!(slot.feedback, Some(noproof));
    // Later equality proof for the same historical attempt changes nothing:
    // the observed focus stays elsewhere so target equality cannot satisfy,
    // and the latched row is never replaced.
    let late_proof = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        11,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                4,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![late_proof],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Unproven
    );
    assert!(
        slot.feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 5),
        "original no-proof row preserved"
    );
}

#[test]
fn terminal_no_call_retires_the_single_slot() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    // An informative no-call (terminal reason) spends the id: the slot
    // retires even though the verdict classifies as unproven.
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::NotCalled,
        VisibilityReason::Unreadable,
        Some("w-fore"),
        Some("w-fore"),
        false,
        9,
    );
    assert_eq!(
        classify_focus_feedback(&WindowId("w0".to_owned()), &row),
        FocusVerdict::Unproven
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row],
        ),
    );
    assert!(
        engine.pending_focus.is_none(),
        "terminal no-call retires the slot"
    );
}

#[test]
fn first_tuple_latches_under_hold_incomplete_barrier_and_keeps_stamp() {
    // Pending-owner successor to the retired low-level first-match latch:
    // mismatched tuples latch nothing, the first exact tuple latches under
    // held/incomplete/barrier intake (no readability gate), and a
    // contradictory same-tuple row cannot replace it. Original stamp 17.
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let matched = || {
        feedback_row(
            grant.id.as_str(),
            "w0",
            grant.issued_revision,
            VisibilitySetter::Accepted,
            VisibilityReason::Empty,
            None,
            Some("w0"),
            true,
            17,
        )
    };
    let strained = |engine: &mut WorldEngine, seq: u64, feedback: Vec<FocusFeedback>| {
        // w0 rides a narrowly held row while w1 stays live (held + live
        // facts for one window would fence as duplicate): the strained
        // intake still latches the first exact tuple.
        let mut strained_report =
            report(seq, vec![fact("w1", "out-a", 100)], "", vec![], None, None);
        strained_report.inventory_complete = false;
        strained_report.gesture_barrier = true;
        strained_report.held_windows = vec![crate::world_report::HeldWindowRow {
            window: WindowId("w0".to_owned()),
            ordinal: 0,
            output: None,
            fields: Vec::new(),
            diagnostic: None,
            kind: RowDiagnosticKind::InvalidFact,
        }];
        strained_report.focus_feedback = feedback;
        accepted(engine, &strained_report)
    };
    // Mismatched tuples precede the match and latch nothing.
    let reply = strained(
        &mut engine,
        3,
        vec![
            feedback_row(
                "fx-9",
                "w0",
                grant.issued_revision,
                VisibilitySetter::Accepted,
                VisibilityReason::Empty,
                None,
                Some("w0"),
                true,
                17,
            ),
            feedback_row(
                &grant.id,
                "w0",
                grant.issued_revision + 1,
                VisibilitySetter::Accepted,
                VisibilityReason::Empty,
                None,
                Some("w0"),
                true,
                17,
            ),
        ],
    );
    assert!(reply.effects.is_empty());
    assert_pending_feedback_none(&engine, "mismatch latches nothing");
    // The first exact match arrives under held/incomplete/barrier and
    // still latches: no readability gate.
    let expected = matched();
    strained(&mut engine, 4, vec![expected.clone()]);
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(expected));
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    // A contradictory same-tuple row (refused shape, same opaque identity)
    // cannot overwrite the latched history or its stamp.
    let same_contra = FocusFeedback {
        setter: VisibilitySetter::Refused,
        pre: Some(WindowId("w-fore".to_owned())),
        post: Some(WindowId("w-fore".to_owned())),
        foreground_proven: false,
        ..matched()
    };
    strained(&mut engine, 5, vec![same_contra]);
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    assert!(
        slot.feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 17),
        "original stamp preserved"
    );
}

#[test]
fn observation_revisions_retain_pending_slot_until_delayed_feedback() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    for seq in [3, 4, 5] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
        );
        assert!(
            reply.effects.is_empty(),
            "no retry on observation revisions"
        );
        assert_pending_feedback_none(&engine, "slot waits for feedback");
    }
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                6,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row],
        ),
    );
    assert!(engine.pending_focus.is_none(), "refusal retires the slot");
}

#[test]
fn explicit_batch_focus_supersedes_admission_grant() {
    // V4 supersession (docs/decisions/commands.md#kde-rapid-input: the adapter
    // applies only the latest publication, superseded unissued targets drop;
    // world_run_v4.rs batch-focus resolution: the first explicit batch Focus
    // supersedes every unissued prior Focus): the newcomer admission Focus for
    // w1 drops under the explicit batch Focus for w0, so only the explicit
    // grant publishes.
    // Map: world_rapid_input_tests::rapid_input_batch_focus_supersedes_admission_focus.
    let mut engine = test_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("w0", "out-a", 0)], "", vec![], None, None),
    );
    // Newcomer admission plus an explicit command focus share one reply, but
    // only the explicit grant publishes.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            Some(2),
            Some(focus_command("w0")),
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("explicit-only request");
    assert_eq!(request.target.0, "w0");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(2));
    let slot = pending(&engine);
    assert_eq!(slot.target.0, "w0", "slot follows the surviving grant");
    assert!(slot.feedback.is_none());
    // The retained slot settles by opaque tuple.
    let row = feedback_row(
        &slot.id,
        &slot.target.0,
        slot.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some(slot.target.0.clone()).as_deref(),
        true,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row.clone()],
        ),
    );
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(row));
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
}

#[test]
fn newer_emitted_batch_supersedes_older_and_stale_tuples_ignore() {
    // V4 supersession (docs/decisions/commands.md#kde-rapid-input: the adapter
    // applies only the latest publication, superseded unissued targets drop;
    // world_run_v4.rs batch-focus resolution: the first batch publishes only
    // the explicit grant for w0, and a newer command batch supersedes it with
    // a fresh grant for w1. Stale tuples from the superseded batch cannot
    // settle the newer grant.
    let mut engine = test_engine();
    accepted(
        &mut engine,
        &report(1, vec![fact("w0", "out-a", 0)], "", vec![], None, None),
    );
    let first = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            Some(2),
            Some(focus_command("w0")),
        ),
    );
    assert!(
        first.effects.is_empty(),
        "focus never publishes as wire effects: {first:?}"
    );
    let first_request = first.focus_request.as_ref().expect("explicit-only request");
    assert_eq!(first_request.target.0, "w0");
    let stale = {
        let slot = pending(&engine);
        vec![feedback_row(
            &slot.id,
            &slot.target.0,
            slot.issued_revision,
            VisibilitySetter::Accepted,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some(slot.target.0.clone()).as_deref(),
            true,
            9,
        )]
    };
    // A newer real command batch supersedes: only the fresh grant remains.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            Some(3),
            Some(focus_command("w1")),
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("fresh explicit request");
    assert_eq!(request.target.0, "w1");
    let slot = pending(&engine);
    assert_eq!(slot.target.0, "w1");
    assert!(slot.feedback.is_none());
    // Stale tuples from the superseded batch cannot change the newer grant.
    accepted(
        &mut engine,
        &with_feedback(
            report(
                4,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            stale,
        ),
    );
    let slot = pending(&engine);
    assert_eq!(slot.target.0, "w1");
    assert!(slot.feedback.is_none());
}

#[test]
fn focus_follows_cursor_grant_settles_alone() {
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "focusFollowsCursor".to_owned(),
            crate::connection::RawSettingValue::Bool(true),
        )],
    )
    .expect("valid generation");
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let mut base = report(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        vec![],
        None,
        None,
    );
    base.screens = screens.clone();
    base.cursor = Some((10, 10));
    accepted(&mut engine, &base);
    let mut hover = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        vec![],
        None,
        None,
    );
    hover.screens = screens;
    hover.cursor = Some((110, 10));
    hover.cursor_hit = Some(WindowId("win-b".to_owned()));
    let reply = accepted(&mut engine, &hover);
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("hover request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::FocusFollowsCursor
    );
    assert_eq!(request.id, format!("focus-{}", request.issued_revision));
    let slot = pending(&engine);
    assert_eq!(slot.target.0, "win-b");
    let row = feedback_row(
        &slot.id,
        "win-b",
        slot.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-a"),
        Some("win-b"),
        true,
        9,
    );
    let mut settle = report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        vec![],
        None,
        None,
    );
    settle.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    settle.cursor = Some((110, 10));
    settle.cursor_hit = Some(WindowId("win-b".to_owned()));
    let reply = accepted(&mut engine, &with_feedback(settle, vec![row.clone()]));
    assert!(
        reply.effects.is_empty(),
        "hover arrival mints nothing: {reply:?}"
    );
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(row));
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
}

#[test]
fn settle_under_sweep_hold_and_held_presence_retains() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    // w0 rides a narrowly held row while w1 stays live: the scope holds,
    // but historical feedback still latches and the held slot survives.
    let mut held = report(3, vec![fact("w1", "out-a", 100)], "", vec![], None, None);
    held.held_windows = vec![crate::world_report::HeldWindowRow {
        window: WindowId("w0".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    held.focus_feedback = vec![feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    )];
    accepted(&mut engine, &held);
    let slot = pending(&engine);
    assert!(
        slot.feedback.is_some(),
        "held row keeps the slot and latches"
    );
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    // Complete absence of an engine-known lifetime retains, exactly like
    // the state bindings and intent ledgers: only proven departure (or an
    // explicit close) prunes a known lifetime.
    accepted(
        &mut engine,
        &report(4, vec![fact("w1", "out-a", 100)], "", vec![], None, None),
    );
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
    // Explicit close ends the lifetime and prunes the settled slot.
    accepted(
        &mut engine,
        &report(
            5,
            vec![fact("w1", "out-a", 100)],
            "",
            vec!["w0"],
            None,
            None,
        ),
    );
    assert!(engine.pending_focus.is_none(), "close prunes the slot");
}

#[test]
fn never_known_target_prunes_single_slot() {
    // Single-slot successor to the retired multigrant ledger probes
    // (`mixed_live_and_departed_batch_prunes_only_departed`,
    // `complete_absence_prunes_never_known_targets`): no real producer can
    // mint a request for a never-known lifetime (commands require staged
    // membership), so the departed-target prune is exercised by seating a
    // ghost row directly. Complete inventory carrying no ghost fact, hold,
    // or exclusion prunes exactly it; the live half of the old mixed test
    // is covered by every retention test above.
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let mut ghost = grant.clone();
    ghost.target = WindowId("w-ghost".to_owned());
    ghost.id = "fx-ghost".to_owned();
    engine.pending_focus = Some(ghost);
    assert!(!engine.engine_knows_window_any(&WindowId("w-ghost".to_owned())));
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        engine.pending_focus.is_none(),
        "departure prunes the ghost slot"
    );
}

#[test]
fn explicit_close_prunes_and_reused_id_cannot_match_old_grant() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("w1", "out-a", 100)],
            "",
            vec!["w0"],
            None,
            None,
        ),
    );
    assert!(engine.pending_focus.is_none(), "explicit close prunes");
    // Reused id returns as a newcomer with a fresh opaque batch; the old
    // tuple (old issuing revision) cannot latch the new slot.
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w1", "out-a", 100), fact("w0", "out-a", 200)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let stale = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("reused newcomer request");
    assert_eq!(request.target.0, "w0", "reused id focuses fresh: {reply:?}");
    let slot = pending(&engine);
    assert_ne!(slot.id, grant.id);
    accepted(
        &mut engine,
        &with_feedback(
            report(
                5,
                vec![fact("w1", "out-a", 100), fact("w0", "out-a", 200)],
                "",
                vec![],
                None,
                None,
            ),
            vec![stale],
        ),
    );
    assert_pending_feedback_none(&engine, "stale tuple latches nothing on the new slot");
}

#[test]
fn accepted_report_rejected_command_retains_feedback_settlement() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let revision = engine.state_revision;
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        9,
    );
    // Rejected focus-unknown command still incorporates the observation and
    // the pre-command feedback retirement survives the command rollback.
    let mut rejected = report(
        3,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "",
        vec![],
        Some(3),
        Some(focus_command("w-ghost")),
    );
    rejected.focus_feedback = vec![row];
    let reply = accepted(&mut engine, &rejected);
    assert_eq!(
        reply.command_result,
        Some(crate::world_report::CommandResult::Rejected {
            kind: crate::world_report::CommandRejectKind::FocusUnknown
        })
    );
    assert!(engine.state_revision > revision);
    assert!(
        engine.pending_focus.is_none(),
        "refusal retirement survives the command rollback"
    );
}

#[test]
fn envelope_rejection_adopts_nothing() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let _ = explicit_grant(&mut engine, 2, "w0");
    let revision = engine.state_revision;
    let retained = engine.pending_focus.clone();
    // Same request sequence with changed content fences as reused: the
    // staged candidate (and its settlement) drops without adoption.
    let mut reused = report(2, vec![fact("w0", "out-a", 0)], "", vec![], None, None);
    reused.focus_feedback = vec![feedback_row(
        "forged",
        "w0",
        0,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    )];
    match engine.handle_report(&reused, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REUSED)
        }
        other => panic!("expected reused fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(engine.pending_focus, retained);
}

#[test]
fn consumed_command_report_still_settles_feedback() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    // The consumed sequence reconciles observation only, but accepted
    // feedback still latches on the retained slot.
    let mut consumed = report(
        3,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "",
        vec![],
        Some(2),
        Some(focus_command("w0")),
    );
    consumed.focus_feedback = vec![row.clone()];
    match engine.handle_report(&consumed, &[]).0 {
        WorldOutcome::CommandConsumed(reply) => {
            assert_eq!(reply.command_result, None);
            assert!(reply.effects.is_empty());
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    let slot = pending(&engine);
    assert_eq!(slot.feedback, Some(row));
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
}

#[test]
fn duplicate_replay_keeps_settlement_unchanged() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let settled = with_feedback(
        report(
            3,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
        vec![feedback_row(
            &grant.id,
            "w0",
            grant.issued_revision,
            VisibilitySetter::Accepted,
            VisibilityReason::Empty,
            Some("w-fore"),
            Some("w0"),
            true,
            9,
        )],
    );
    let reply = accepted(&mut engine, &settled);
    let revision = engine.state_revision;
    match engine.handle_report(&settled, &[]).0 {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, reply),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    let slot = pending(&engine);
    assert_eq!(
        classify_focus_feedback(&slot.target, slot.feedback.as_ref().expect("feedback")),
        FocusVerdict::Proven
    );
}

#[test]
fn failed_attempt_keeps_accepted_memory_with_no_retry_or_side_effects() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let grant = explicit_grant(&mut engine, 2, "w0");
    let memory = engine.last_active_window.clone();
    let row = feedback_row(
        &grant.id,
        "w0",
        grant.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        9,
    );
    accepted(
        &mut engine,
        &with_feedback(
            report(
                3,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
            vec![row],
        ),
    );
    assert!(engine.pending_focus.is_none(), "refusal retires the slot");
    // No accepted-choice rollback, no retry, no transfers or operation
    // events on the followup observation.
    assert_eq!(engine.last_active_window, memory);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(engine.last_active_window, memory);
    assert!(reply.effects.is_empty(), "no focus retry: {reply:?}");
    assert!(reply.transfers.is_empty());
    assert!(reply.operation_settled.is_empty());
    assert_eq!(reply.command_result, None);
}
