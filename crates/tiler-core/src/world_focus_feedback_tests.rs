//! Focus-feedback ingress: typed schema, global caps, host-arm fencing,
//! request reuse fencing, and estimator exactness.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::NativeWorkspaceId;
use crate::world_directional::tests::{accepted, fact, rect, report_full, screen};
use crate::world_engine::{KIND_REUSED, KIND_SNAPSHOT, LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    ArrivalBracket, ArrivalLocation, FocusFeedback, MAX_ARRIVAL_BRACKETS_TOTAL,
    MAX_WORLD_FOCUS_FEEDBACK, VisibilityFact, VisibilityReason, VisibilitySample, VisibilitySetter,
    WorkspaceRef, WorldReport,
};
use crate::world_sizing::report_estimate;

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

fn null_sample(obs: u64) -> VisibilitySample {
    VisibilitySample {
        obs,
        lifetime_matched: None,
        visibility: VisibilityFact {
            visible: None,
            suppression_owned: None,
        },
        minimized: None,
        host_hidden: None,
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

fn base(seq: u64) -> WorldReport {
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

fn with_feedback(mut report: WorldReport, rows: Vec<FocusFeedback>) -> WorldReport {
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
    let reply = accepted(&mut engine, &with_feedback(base(1), rows.clone()));
    assert_eq!(
        engine
            .last_report
            .as_ref()
            .expect("last report")
            .focus_feedback,
        rows,
    );
    assert!(reply.diagnostics.is_empty());
    assert!(reply.effects.is_empty());
    assert!(reply.transfers.is_empty());
    assert!(reply.operation_settled.is_empty());
    assert!(reply.command_result.is_none());
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
            .last_report
            .as_ref()
            .expect("last report")
            .focus_feedback,
        rows,
    );
    assert!(reply.effects.is_empty());
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
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_id])),
        Some("focus-id-invalid"),
    );
    let mut bad_target = good.clone();
    bad_target.target = WindowId("has space".to_owned());
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_target])),
        Some("focus-target-invalid"),
    );
    let mut bad_pre = good.clone();
    bad_pre.pre = Some(WindowId(String::new()));
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_pre])),
        Some("focus-foreground-invalid"),
    );
    let mut bad_win = good.clone();
    bad_win.arrival[0].window = WindowId("bad id".to_owned());
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_win])),
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
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![logical_home])),
        Some("arrival-home-invalid"),
    );
    let mut bad_out = good.clone();
    bad_out.arrival[0].location = ArrivalLocation::Output {
        out: Some(OutputId("bad id".to_owned())),
    };
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_out])),
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
        crate::world_report::validate_report_reason(&with_feedback(base(1), vec![bad_frame])),
        Some("arrival-frame-invalid"),
    );
}

#[test]
fn global_caps_refuse_and_boundaries_hold() {
    let over_rows: Vec<FocusFeedback> = (0..MAX_WORLD_FOCUS_FEEDBACK + 1)
        .map(|i| row(&format!("fb-{i}"), "win-a", Vec::new()))
        .collect();
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), over_rows)),
        Some("section-over-cap"),
    );
    let over_brackets: Vec<ArrivalBracket> = (0..MAX_ARRIVAL_BRACKETS_TOTAL + 1)
        .map(|i| output_bracket(&format!("win-{i}"), 1, Some("out-a")))
        .collect();
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(
            base(1),
            vec![row("fb-1", "win-a", over_brackets)],
        )),
        Some("section-over-cap"),
    );
    let at_rows: Vec<FocusFeedback> = (0..MAX_WORLD_FOCUS_FEEDBACK)
        .map(|i| row(&format!("fb-{i}"), "win-a", Vec::new()))
        .collect();
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(base(1), at_rows)),
        None,
    );
    let at_brackets: Vec<ArrivalBracket> = (0..MAX_ARRIVAL_BRACKETS_TOTAL)
        .map(|i| output_bracket(&format!("win-{i}"), 1, Some("out-a")))
        .collect();
    assert_eq!(
        crate::world_report::validate_report_reason(&with_feedback(
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
    let revision = engine.state_revision;
    let last = engine.last_report.clone().expect("last report");
    match engine.handle_report_legacy(&with_feedback(
        base(2),
        vec![native_bracket("win-a", 2, None)]
            .into_iter()
            .map(|b| row("fb-2", "win-a", vec![b]))
            .collect::<Vec<_>>(),
    )) {
        LegacyWorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "arrival-host-mismatch");
        }
        other => panic!("expected host mismatch, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(engine.last_report, Some(last));
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
    let revision = engine.state_revision;
    let last = engine.last_report.clone().expect("last report");
    match engine.handle_report_legacy(&with_feedback(
        base(2),
        vec![row(
            "fb-2",
            "win-a",
            vec![output_bracket("win-a", 2, Some("out-a"))],
        )],
    )) {
        LegacyWorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, KIND_SNAPSHOT);
            assert_eq!(message, "arrival-host-mismatch");
        }
        other => panic!("expected host mismatch, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
    assert_eq!(engine.last_report, Some(last));
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
    let revision = engine.state_revision;
    match engine.handle_report_legacy(&first) {
        LegacyWorldOutcome::Duplicate(cached) => {
            assert_eq!(cached, reply);
            assert_eq!(engine.state_revision, revision);
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    let mut changed = first.clone();
    changed.focus_feedback[0].attempt_ms = 10;
    match engine.handle_report_legacy(&changed) {
        LegacyWorldOutcome::Rejected { kind, .. } => assert_eq!(kind, KIND_REUSED),
        other => panic!("expected reused fence, got {other:?}"),
    }
    assert_eq!(engine.state_revision, revision);
}

fn b(value: Option<bool>) -> &'static str {
    match value {
        None => "null",
        Some(true) => "true",
        Some(false) => "false",
    }
}

fn sample_literal(sample: &VisibilitySample) -> String {
    format!(
        "{{\"obs\":\"{}\",\"lifetimeMatched\":{},\"visibility\":{{\"visible\":{},\"suppressionOwned\":{}}},\"minimized\":{},\"hostHidden\":{}}}",
        sample.obs,
        b(sample.lifetime_matched),
        b(sample.visibility.visible),
        b(sample.visibility.suppression_owned),
        b(sample.minimized),
        b(sample.host_hidden),
    )
}

fn bracket_literal(bracket: &ArrivalBracket) -> String {
    let location = match &bracket.location {
        ArrivalLocation::Native { home } => {
            let home_text = home.as_ref().map_or("null".to_owned(), |home| match home {
                WorkspaceRef::Native { output, workspace } => format!(
                    "{{\"kind\":\"native\",\"out\":\"{}\",\"ws\":\"{}\"}}",
                    output.0,
                    workspace.as_str(),
                ),
                WorkspaceRef::Logical { output, workspace } => format!(
                    "{{\"kind\":\"logical\",\"out\":\"{}\",\"ws\":\"{}\"}}",
                    output.0, workspace.0,
                ),
            });
            format!("\"kind\":\"native\",\"home\":{home_text}")
        }
        ArrivalLocation::Output { out } => {
            let out_text = out
                .as_ref()
                .map_or("null".to_owned(), |out| format!("\"{}\"", out.0));
            format!("\"kind\":\"output\",\"out\":{out_text}")
        }
    };
    let frame_text = bracket.frame.map_or("null".to_owned(), |frame| {
        format!("[{},{},{},{}]", frame.x, frame.y, frame.w, frame.h)
    });
    format!(
        "{{\"win\":\"{}\",\"obs\":\"{}\",\"before\":{},{},\"frame\":{},\"after\":{}}}",
        bracket.window.0,
        bracket.obs,
        sample_literal(&bracket.before),
        location,
        frame_text,
        sample_literal(&bracket.after),
    )
}

fn row_literal(row: &FocusFeedback) -> String {
    let brackets: Vec<String> = row.arrival.iter().map(bracket_literal).collect();
    let pre_text = row
        .pre
        .as_ref()
        .map_or("null".to_owned(), |id| format!("\"{}\"", id.0));
    let post_text = row
        .post
        .as_ref()
        .map_or("null".to_owned(), |id| format!("\"{}\"", id.0));
    format!(
        "{{\"id\":\"{}\",\"target\":\"{}\",\"rev\":\"{}\",\"setter\":\"{}\",\"reason\":\"{}\",\"pre\":{},\"post\":{},\"foregroundProven\":{},\"arrival\":[{}],\"attemptMs\":\"{}\"}}",
        row.id,
        row.target.0,
        row.revision,
        row.setter.as_str(),
        row.reason.as_str(),
        pre_text,
        post_text,
        b(Some(row.foreground_proven)),
        brackets.join(","),
        row.attempt_ms,
    )
}

#[test]
fn estimate_delta_matches_hand_built_canonical_bytes() {
    let bracket_out = ArrivalBracket {
        window: WindowId("win-a".to_owned()),
        obs: 3,
        before: VisibilitySample {
            obs: 3,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(true),
                suppression_owned: None,
            },
            minimized: Some(false),
            host_hidden: None,
        },
        location: ArrivalLocation::Output {
            out: Some(OutputId("out-a".to_owned())),
        },
        frame: Some(rect(0, 0, 40, 90)),
        after: VisibilitySample {
            obs: 3,
            lifetime_matched: None,
            visibility: VisibilityFact {
                visible: None,
                suppression_owned: None,
            },
            minimized: None,
            host_hidden: Some(true),
        },
    };
    let row_out = FocusFeedback {
        id: "fb-1".to_owned(),
        target: WindowId("win-a".to_owned()),
        revision: 7,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-f".to_owned())),
        post: None,
        foreground_proven: true,
        arrival: vec![bracket_out],
        attempt_ms: 9,
    };
    let bracket_native_home = ArrivalBracket {
        window: WindowId("win-b".to_owned()),
        obs: 44,
        before: null_sample(44),
        location: ArrivalLocation::Native {
            home: Some(native_home()),
        },
        frame: None,
        after: null_sample(44),
    };
    let mut bracket_native_null = bracket_native_home.clone();
    bracket_native_null.window = WindowId("win-c".to_owned());
    bracket_native_null.obs = 5;
    bracket_native_null.location = ArrivalLocation::Native { home: None };
    let row_native = FocusFeedback {
        id: "fb-2".to_owned(),
        target: WindowId("win-b".to_owned()),
        revision: 12345,
        setter: VisibilitySetter::NotCalled,
        reason: VisibilityReason::Unreadable,
        pre: None,
        post: Some(WindowId("win-g".to_owned())),
        foreground_proven: false,
        arrival: vec![bracket_native_home, bracket_native_null],
        attempt_ms: 100,
    };
    // The assumed canonical shapes pin every carried key, quote, comma, and
    // arm token the estimator counts; any drift fails the delta below.
    let literal_out = row_literal(&row_out);
    assert_eq!(
        literal_out,
        "{\"id\":\"fb-1\",\"target\":\"win-a\",\"rev\":\"7\",\"setter\":\"accepted\",\"reason\":\"\",\"pre\":\"win-f\",\"post\":null,\"foregroundProven\":true,\"arrival\":[{\"win\":\"win-a\",\"obs\":\"3\",\"before\":{\"obs\":\"3\",\"lifetimeMatched\":true,\"visibility\":{\"visible\":true,\"suppressionOwned\":null},\"minimized\":false,\"hostHidden\":null},\"kind\":\"output\",\"out\":\"out-a\",\"frame\":[0,0,40,90],\"after\":{\"obs\":\"3\",\"lifetimeMatched\":null,\"visibility\":{\"visible\":null,\"suppressionOwned\":null},\"minimized\":null,\"hostHidden\":true}}],\"attemptMs\":\"9\"}",
    );
    let literal_native = row_literal(&row_native);
    assert!(literal_native.contains(
        "\"kind\":\"native\",\"home\":{\"kind\":\"native\",\"out\":\"out-a\",\"ws\":\"ws-1\"}"
    ));
    assert!(literal_native.contains("\"kind\":\"native\",\"home\":null"));
    let before = report_estimate(&base(1));
    let with_out = with_feedback(base(1), vec![row_out.clone()]);
    assert_eq!(report_estimate(&with_out) - before, literal_out.len());
    let with_both = with_feedback(base(1), vec![row_out, row_native]);
    assert_eq!(
        report_estimate(&with_both) - report_estimate(&with_out),
        literal_native.len() + 1,
    );
}
