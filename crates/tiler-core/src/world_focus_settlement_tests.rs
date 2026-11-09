//! Ordinary focus settlement: retained grants bind actually emitted
//! `awaitShown=false` focus effects to the first exact opaque-tuple
//! feedback row with a historical verdict.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_focus::FocusVerdict;
use crate::world_report::{
    FocusFeedback, OneShotEffect, PresentationRowV4, RowDiagnosticKind, VisibilityReason,
    VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4,
    WorldReportV4,
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
    match engine.handle_report(report) {
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

/// Real explicit-command grant for `window` via command `seq`: returns the
/// actually emitted effect (opaque id, target, issuing revision).
fn explicit_grant(engine: &mut WorldEngine, seq: u64, window: &str) -> OneShotEffect {
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
    assert_eq!(reply.effects.len(), 1, "one explicit focus: {reply:?}");
    reply.effects[0].clone()
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

#[test]
fn explicit_grant_settles_proven_without_arrival_or_current_focus() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
    assert!(engine.retained_focus[0].feedback.is_none());
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    // A stale current-foreground fact naming another window never blocks
    // historical proof; the genuine observation legitimately advances
    // memory through the existing observation path (never through
    // settlement).
    let settle = with_feedback(
        report(
            3,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w1",
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
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    assert_eq!(engine.retained_focus[0].feedback, Some(row));
    // Accepted focus memory follows the genuine observation (w1), never a
    // historical-grant rollback to the proven target (w0).
    assert_eq!(
        engine.last_active_window.get(&domain),
        Some(&WindowId("w1".to_owned()))
    );
    // An observation-only followup keeps that memory with no retry.
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
    let effect = explicit_grant(&mut engine, 2, "w0");
    let good = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    let mut forged_id = good.clone();
    forged_id.id = format!("{}-forged", effect.id);
    let mut wrong_rev = good.clone();
    wrong_rev.revision = effect.issued_revision + 1;
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
    assert!(engine.retained_focus[0].feedback.is_none());
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    assert_eq!(engine.retained_focus[0].feedback, Some(good));
}

#[test]
fn proof_holds_despite_nonaccepted_setter() {
    for setter in [
        VisibilitySetter::Accepted,
        VisibilitySetter::Refused,
        VisibilitySetter::Threw,
        VisibilitySetter::NotCalled,
    ] {
        let mut engine = test_engine();
        bootstrap_two(&mut engine);
        let effect = explicit_grant(&mut engine, 2, "w0");
        let row = feedback_row(
            &effect.id,
            "w0",
            effect.issued_revision,
            setter,
            VisibilityReason::NativeRefused,
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
        assert_eq!(
            engine.retained_focus[0].verdict,
            FocusVerdict::Proven,
            "setter {setter:?} never blocks equality proof"
        );
    }
}

#[test]
fn proof_requires_equality_even_when_proven_flag_set() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    // Proven flag with a wrong post is not proof of this grant.
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Unproven);
    // Post equality without the proof flag is not proof either.
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Unproven);
}

/// One verdict-table probe: fresh engine, real explicit grant, one row.
fn settle_once(
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
) -> FocusVerdict {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
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
            vec![feedback_row(
                &effect.id,
                "w0",
                effect.issued_revision,
                setter,
                reason,
                pre,
                post,
                proven,
                9,
            )],
        ),
    );
    engine.retained_focus[0].verdict
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
    let effect = explicit_grant(&mut engine, 2, "w0");
    let first = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        5,
    );
    let mut second = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    assert_eq!(engine.retained_focus[0].feedback, Some(first));
    // A later report carrying the same tuple with different facts cannot
    // replace the latched row.
    let late = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    assert!(
        engine.retained_focus[0]
            .feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 5),
        "original stamp preserved"
    );
}

#[test]
fn later_contradictions_never_upgrade_a_latched_attempt() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    // Latch an unproven no-call first.
    let noproof = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::NotCalled,
        VisibilityReason::Unreadable,
        None,
        None,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Unproven);
    assert_eq!(engine.retained_focus[0].feedback, Some(noproof));
    // Later equality proof for the same historical attempt changes nothing,
    // and fresh foreground facts are not historical verdicts.
    let late_proof = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
                "w0",
                vec![],
                None,
                None,
            ),
            vec![late_proof],
        ),
    );
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Unproven);
    assert!(
        engine.retained_focus[0]
            .feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 5),
        "original no-call row preserved"
    );
}

#[test]
fn observation_revisions_retain_pending_batch_until_delayed_feedback() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
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
        assert_eq!(engine.retained_focus.len(), 1);
        assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
        assert!(engine.retained_focus[0].feedback.is_none());
    }
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Refused);
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
    assert_eq!(reply.effects.len(), 1, "explicit-only grant: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "w0");
    assert_eq!(
        engine.retained_focus.len(),
        1,
        "bound follows the surviving grant"
    );
    // The surviving grant is tracked: the emitted effect retained as issued,
    // nothing dropped or truncated.
    let retained_effects: Vec<OneShotEffect> = engine
        .retained_focus
        .iter()
        .map(|grant| grant.effect.clone())
        .collect();
    assert_eq!(retained_effects, reply.effects);
    assert!(
        engine
            .retained_focus
            .iter()
            .all(|grant| grant.verdict == FocusVerdict::Pending && grant.feedback.is_none())
    );
    let targets: Vec<String> = engine
        .retained_focus
        .iter()
        .map(|grant| grant.effect.target.0.clone())
        .collect();
    assert_eq!(targets, vec!["w0".to_owned()]);
    // The retained grant settles by opaque tuple.
    let rows: Vec<FocusFeedback> = engine
        .retained_focus
        .iter()
        .map(|grant| {
            feedback_row(
                &grant.effect.id,
                &grant.effect.target.0,
                grant.effect.issued_revision,
                VisibilitySetter::Accepted,
                VisibilityReason::Empty,
                Some("w-fore"),
                Some(grant.effect.target.0.clone()).as_deref(),
                true,
                9,
            )
        })
        .collect();
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
            rows,
        ),
    );
    assert!(
        engine
            .retained_focus
            .iter()
            .all(|grant| grant.verdict == FocusVerdict::Proven)
    );
}

#[test]
fn newer_emitted_batch_supersedes_older_and_stale_tuples_ignore() {
    // V4 supersession (docs/decisions/commands.md#kde-rapid-input: the adapter
    // applies only the latest publication, superseded unissued targets drop;
    // world_run_v4.rs batch-focus resolution: the first explicit batch Focus
    // supersedes every unissued prior Focus): the first batch publishes only
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
    assert_eq!(first.effects.len(), 1, "explicit-only grant: {first:?}");
    assert_eq!(first.effects[0].target.0, "w0");
    let stale: Vec<FocusFeedback> = first
        .effects
        .iter()
        .map(|effect| {
            feedback_row(
                &effect.id,
                &effect.target.0,
                effect.issued_revision,
                VisibilitySetter::Accepted,
                VisibilityReason::Empty,
                Some("w-fore"),
                Some(effect.target.0.clone()).as_deref(),
                true,
                9,
            )
        })
        .collect();
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
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].effect.target.0, "w1");
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
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
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].effect.target.0, "w1");
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
    assert!(engine.retained_focus[0].feedback.is_none());
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
    assert_eq!(reply.effects.len(), 1, "hover mints alone: {reply:?}");
    assert_eq!(
        reply.effects[0].id,
        format!("ffc-{}", reply.effects[0].issued_revision),
        "compact hover identity"
    );
    assert_eq!(engine.retained_focus.len(), 1);
    let grant = &engine.retained_focus[0];
    assert_eq!(grant.effect.target.0, "win-b");
    let row = feedback_row(
        &grant.effect.id,
        "win-b",
        grant.effect.issued_revision,
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
    let reply = accepted(&mut engine, &with_feedback(settle, vec![row]));
    assert!(
        reply.effects.is_empty(),
        "hover arrival mints nothing: {reply:?}"
    );
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
}

#[test]
fn settle_under_sweep_hold_and_held_presence_retains() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    // w0 rides a narrowly held row while w1 stays live: the scope holds,
    // but historical feedback still settles and the held grant survives.
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
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    )];
    accepted(&mut engine, &held);
    assert_eq!(engine.retained_focus.len(), 1, "held row keeps the grant");
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    // Complete absence of an engine-known lifetime retains, exactly like
    // the state bindings and intent ledgers: only proven departure (or an
    // explicit close) prunes a known lifetime.
    accepted(
        &mut engine,
        &report(4, vec![fact("w1", "out-a", 100)], "", vec![], None, None),
    );
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
    // Explicit close ends the lifetime and prunes the settled grant.
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
    assert!(engine.retained_focus.is_empty(), "close prunes the grant");
}

#[test]
fn mixed_live_and_departed_batch_prunes_only_departed() {
    use crate::world_report::{EffectCause, EffectKind};
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    // Real live grant first: actual explicit-command effect for w0.
    let live = explicit_grant(&mut engine, 2, "w0");
    // Direct ledger-probe scope: no real producer can mint a grant for a
    // never-known lifetime (commands require staged membership), so the
    // proven-departed second grant is seeded to exercise the multigrant
    // prune alignment. The first grant stays genuinely minted.
    engine
        .retained_focus
        .push(crate::world_focus::RetainedFocusGrant {
            effect: OneShotEffect {
                id: "fx-ghost".to_owned(),
                kind: EffectKind::Focus,
                target: WindowId("w-ghost".to_owned()),
                issued_revision: 2,
                cause: EffectCause::ExplicitCommand,
                command_seq: Some(2),
                await_shown: false,
            },
            feedback: None,
            verdict: FocusVerdict::Pending,
        });
    assert_eq!(engine.retained_focus.len(), 2);
    assert!(!engine.engine_knows_window_any(&WindowId("w-ghost".to_owned())));
    // Complete inventory carries the live target but no ghost fact, hold,
    // or exclusion: only the departed ghost must prune.
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
    assert_eq!(engine.retained_focus.len(), 1, "departed ghost must prune");
    assert_eq!(engine.retained_focus[0].effect, live);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
}

#[test]
fn complete_absence_prunes_never_known_targets() {
    use crate::world_report::{EffectCause, EffectKind};
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    // Direct ledger probe for the fail-closed prune arm: no real producer
    // can mint a grant for a never-known lifetime (commands require staged
    // membership), so the never-known target is seeded to prove the proven-
    // departure prune drops exactly it.
    engine
        .retained_focus
        .push(crate::world_focus::RetainedFocusGrant {
            effect: OneShotEffect {
                id: "fx-ghost".to_owned(),
                kind: EffectKind::Focus,
                target: WindowId("w-ghost".to_owned()),
                issued_revision: 2,
                cause: EffectCause::ExplicitCommand,
                command_seq: Some(2),
                await_shown: false,
            },
            feedback: None,
            verdict: FocusVerdict::Pending,
        });
    assert!(!engine.engine_knows_window_any(&WindowId("w-ghost".to_owned())));
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        engine.retained_focus.is_empty(),
        "departure prunes the grant"
    );
}

#[test]
fn explicit_close_prunes_and_reused_id_cannot_match_old_grant() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
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
    assert!(engine.retained_focus.is_empty(), "explicit close prunes");
    // Reused id returns as a newcomer with a fresh opaque batch; the old
    // tuple (old issuing revision) cannot latch the new grant.
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
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    assert!(
        reply.effects.iter().any(|effect| effect.target.0 == "w0"),
        "reused id focuses fresh: {reply:?}"
    );
    assert_eq!(engine.retained_focus.len(), 1);
    assert_ne!(engine.retained_focus[0].effect.id, effect.id);
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Pending);
    assert!(engine.retained_focus[0].feedback.is_none());
}

#[test]
fn accepted_report_rejected_command_retains_feedback_settlement() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    let revision = engine.state_revision;
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w-fore"),
        false,
        9,
    );
    // Rejected focus-unknown command still incorporates the observation and
    // the pre-command feedback settlement survives the command rollback.
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
    assert_eq!(engine.retained_focus.len(), 1);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Refused);
    assert!(
        engine.retained_focus[0]
            .feedback
            .as_ref()
            .is_some_and(|row| row.attempt_ms == 9)
    );
}

#[test]
fn envelope_rejection_adopts_nothing() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let _ = explicit_grant(&mut engine, 2, "w0");
    let revision = engine.state_revision;
    let retained = engine.retained_focus.clone();
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
    match engine.handle_report(&reused) {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REUSED)
        }
        other => panic!("expected reused fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(engine.retained_focus, retained);
}

#[test]
fn consumed_command_report_still_settles_feedback() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("w-fore"),
        Some("w0"),
        true,
        9,
    );
    // The consumed sequence reconciles observation only, but accepted
    // feedback still settles the retained grant.
    let mut consumed = report(
        3,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "",
        vec![],
        Some(2),
        Some(focus_command("w0")),
    );
    consumed.focus_feedback = vec![row];
    match engine.handle_report(&consumed) {
        WorldOutcome::CommandConsumed(reply) => {
            assert_eq!(reply.command_result, None);
            assert!(reply.effects.is_empty());
        }
        other => panic!("expected consumed, got {other:?}"),
    }
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
}

#[test]
fn duplicate_replay_keeps_settlement_unchanged() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
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
            &effect.id,
            "w0",
            effect.issued_revision,
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
    match engine.handle_report(&settled) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, reply),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Proven);
}

#[test]
fn failed_attempt_keeps_accepted_memory_with_no_retry_or_side_effects() {
    let mut engine = test_engine();
    bootstrap_two(&mut engine);
    let effect = explicit_grant(&mut engine, 2, "w0");
    let memory = engine.last_active_window.clone();
    let row = feedback_row(
        &effect.id,
        "w0",
        effect.issued_revision,
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
    assert_eq!(engine.retained_focus[0].verdict, FocusVerdict::Refused);
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
