//! Focus-feedback ingress: typed schema, global caps, host-arm fencing,
//! and request reuse fencing.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::NativeWorkspaceId;
use crate::world_engine::{KIND_REUSED, KIND_SNAPSHOT, WorldEngine, WorldOutcome};
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, FocusFeedback, MAX_ARRIVAL_BRACKETS_TOTAL,
    MAX_WORLD_FOCUS_FEEDBACK, VisibilityFact, VisibilityReason, VisibilitySample, VisibilitySetter,
    WorkspaceRef, WorldReportV4,
};
use crate::world_report_v4_checks::validate_report_v4_reason;
use crate::world_v4_support_tests::{accepted, fact, rect, report_full, screen};

fn sample(obs: u64) -> VisibilitySample {
    VisibilitySample {
        obs,
        lifetime_matched: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        minimized: Some(false),
        host_hidden: Some(false),
    }
}

fn output_bracket(window: &str, obs: u64, out: Option<&str>) -> ArrivalBracket {
    ArrivalBracket {
        window: WindowId(window.to_owned()),
        obs,
        before: sample(obs),
        location: ArrivalLocation::Output {
            out: out.map(|out| OutputId(out.to_owned())),
        },
        frame: Some(rect(0, 0, 40, 90)),
        after: sample(obs),
    }
}

fn native_home() -> WorkspaceRef {
    WorkspaceRef::Native {
        output: OutputId("out-a".to_owned()),
        workspace: NativeWorkspaceId::parse("ws-1").expect("native workspace"),
    }
}

fn native_bracket(window: &str, obs: u64, home: Option<WorkspaceRef>) -> ArrivalBracket {
    ArrivalBracket {
        window: WindowId(window.to_owned()),
        obs,
        before: sample(obs),
        location: ArrivalLocation::Native { home },
        frame: Some(rect(0, 0, 40, 90)),
        after: sample(obs),
    }
}

fn row(id: &str, target: &str, arrival: Vec<ArrivalBracket>) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision: 7,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-fore".to_owned())),
        post: None,
        foreground_proven: true,
        arrival,
        attempt_ms: 9,
    }
}

fn base(seq: u64) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        vec![fact("win-a", "out-a", 0)],
        vec![],
        "win-a",
        None,
        None,
    )
}

fn with_feedback(mut report: WorldReportV4, rows: Vec<FocusFeedback>) -> WorldReportV4 {
    report.focus_feedback = rows;
    report
}

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn host_engine() -> WorldEngine {
    WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation")
}

#[test]
fn output_arm_with_nulls_accepts_and_preserves() {
    let mut engine = engine();
    let rows = vec![row(
        "fb-1",
        "win-a",
        vec![
            output_bracket("win-a", 1, Some("out-a")),
            output_bracket("win-a", 1, None),
        ],
    )];
    let (outcome, settlements) = engine.handle_report(&with_feedback(base(1), rows.clone()), &[]);
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted report, got {other:?}"),
    };
    assert_eq!(
        engine
            .last_report_v4()
            .as_ref()
            .expect("last report")
            .focus_feedback,
        rows,
    );
    assert!(reply.diagnostics.is_empty());
    assert!(reply.transfers.is_empty());
    assert!(reply.operation_settled.is_empty());
    assert!(settlements.is_empty());
}

#[test]
fn native_arm_with_null_home_accepts_on_host_owned() {
    let mut engine = host_engine();
    let rows = vec![row(
        "fb-1",
        "win-a",
        vec![
            native_bracket("win-a", 1, Some(native_home())),
            native_bracket("win-a", 1, None),
        ],
    )];
    let reply = accepted(&mut engine, &with_feedback(base(1), rows.clone()));
    assert_eq!(
        engine
            .last_report_v4()
            .as_ref()
            .expect("last report")
            .focus_feedback,
        rows,
    );
    assert!(reply.transfers.is_empty());
}

#[test]
fn structural_violations_refuse_with_fixed_tokens() {
    let good = row(
        "fb-1",
        "win-a",
        vec![output_bracket("win-a", 1, Some("out-a"))],
    );
    let mut bad_id = good.clone();
    bad_id.id = String::new();
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_id])),
        Some("focus-id-invalid"),
    );
    let mut bad_target = good.clone();
    bad_target.target = WindowId("has space".to_owned());
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_target])),
        Some("focus-target-invalid"),
    );
    let mut bad_pre = good.clone();
    bad_pre.pre = Some(WindowId(String::new()));
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_pre])),
        Some("focus-foreground-invalid"),
    );
    let mut bad_win = good.clone();
    bad_win.arrival[0].window = WindowId("bad id".to_owned());
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_win])),
        Some("arrival-window-invalid"),
    );
    let mut logical_home = good.clone();
    logical_home.arrival[0].location = ArrivalLocation::Native {
        home: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        }),
    };
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![logical_home])),
        Some("arrival-home-invalid"),
    );
    let mut bad_out = good.clone();
    bad_out.arrival[0].location = ArrivalLocation::Output {
        out: Some(OutputId("bad id".to_owned())),
    };
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_out])),
        Some("arrival-output-invalid"),
    );
    let mut bad_frame = good;
    bad_frame.arrival[0].frame = Some(Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 90,
    });
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), vec![bad_frame])),
        Some("arrival-frame-invalid"),
    );
}

#[test]
fn global_caps_refuse_and_boundaries_hold() {
    let over_rows: Vec<FocusFeedback> = (0..MAX_WORLD_FOCUS_FEEDBACK + 1)
        .map(|i| row(&format!("fb-{i}"), "win-a", Vec::new()))
        .collect();
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), over_rows)),
        Some("section-over-cap"),
    );
    let over_brackets: Vec<ArrivalBracket> = (0..MAX_ARRIVAL_BRACKETS_TOTAL + 1)
        .map(|i| output_bracket(&format!("win-{i}"), 1, Some("out-a")))
        .collect();
    assert_eq!(
        validate_report_v4_reason(&with_feedback(
            base(1),
            vec![row("fb-1", "win-a", over_brackets)],
        )),
        Some("section-over-cap"),
    );
    let at_rows: Vec<FocusFeedback> = (0..MAX_WORLD_FOCUS_FEEDBACK)
        .map(|i| row(&format!("fb-{i}"), "win-a", Vec::new()))
        .collect();
    assert_eq!(
        validate_report_v4_reason(&with_feedback(base(1), at_rows)),
        None,
    );
    let at_brackets: Vec<ArrivalBracket> = (0..MAX_ARRIVAL_BRACKETS_TOTAL)
        .map(|i| output_bracket(&format!("win-{i}"), 1, Some("out-a")))
        .collect();
    assert_eq!(
        validate_report_v4_reason(&with_feedback(
            base(1),
            vec![row("fb-1", "win-a", at_brackets)],
        )),
        None,
    );
}

#[test]
fn host_arm_mismatch_rejects_without_mutation() {
    let mut engine = engine();
    accepted(&mut engine, &base(1));
    let revision = engine.state_revision();
    let last = engine.last_report_v4().clone().expect("last report");
    match engine
        .handle_report(
            &with_feedback(
                base(2),
                vec![native_bracket("win-a", 2, None)]
                    .into_iter()
                    .map(|b| row("fb-2", "win-a", vec![b]))
                    .collect::<Vec<_>>(),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "arrival-host-mismatch");
        }
        other => panic!("expected host mismatch, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision);
    assert_eq!(engine.last_report_v4(), Some(last));
    accepted(
        &mut engine,
        &with_feedback(
            base(2),
            vec![row(
                "fb-2",
                "win-a",
                vec![output_bracket("win-a", 2, Some("out-a"))],
            )],
        ),
    );
}

#[test]
fn host_owned_mirrors_arm_gate() {
    let mut engine = host_engine();
    accepted(&mut engine, &base(1));
    let revision = engine.state_revision();
    let last = engine.last_report_v4().clone().expect("last report");
    match engine
        .handle_report(
            &with_feedback(
                base(2),
                vec![row(
                    "fb-2",
                    "win-a",
                    vec![output_bracket("win-a", 2, Some("out-a"))],
                )],
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "arrival-host-mismatch");
        }
        other => panic!("expected host mismatch, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision);
    assert_eq!(engine.last_report_v4(), Some(last));
    accepted(
        &mut engine,
        &with_feedback(
            base(2),
            vec![row(
                "fb-2",
                "win-a",
                vec![native_bracket("win-a", 2, Some(native_home()))],
            )],
        ),
    );
}

#[test]
fn changed_feedback_reuse_fences_while_exact_resend_duplicates() {
    let mut engine = engine();
    let first = with_feedback(
        base(1),
        vec![row(
            "fb-1",
            "win-a",
            vec![output_bracket("win-a", 1, Some("out-a"))],
        )],
    );
    let reply = accepted(&mut engine, &first);
    let revision = engine.state_revision();
    match engine.handle_report(&first, &[]).0 {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached, reply);
            assert_eq!(engine.state_revision(), revision);
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    let mut changed = first.clone();
    changed.focus_feedback[0].attempt_ms = 10;
    match engine.handle_report(&changed, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision);
}
