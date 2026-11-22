//! Migrated request-envelope regression at the protocol boundary: an
//! oversized typed v4 report is refused by the real
//! [`ProtocolAdmissionGate`](tiler_protocol::world_admission::ProtocolAdmissionGate)
//! through the public engine entry before any state mutation, and the next
//! valid report recovers with exactly one revision. Drives the genuine gate,
//! never a stub or a writer-only count.

use tiler_core::boundary::CoreCommand;
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::intent_data::{IntentData, encode_intent_data};
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{
    AdmissionGateDecision, CoreAdmissionGate, PreflightGateFacts, WorldEngine, WorldOutcome,
};
use tiler_core::world_report::{
    FocusRequestCause, FrameSetter, PresentationIngressV4, PresentationRowV4, ScreenReport,
    StateAttempt, StateProperty, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4,
    WorldReportV4,
};
use tiler_protocol::world_admission::ProtocolAdmissionGate;
use tiler_protocol::world_admission_sizing::{
    MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES, MAX_TARGET_LANES_PER_WINDOW, max_intent_desired_bound,
    max_intent_save_bound_v4, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES};
use tiler_protocol::world_reply_write::count_reply_v4_bytes;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screens() -> Vec<ScreenReport> {
    vec![
        ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: tiler_core::world_report::OutputIdentity::unknown(),
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(800, 0, 800, 600)),
            work_area: Some(rect(800, 0, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: tiler_core::world_report::OutputIdentity::unknown(),
        },
    ]
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
        native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
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

fn report(request_seq: u64, windows: Vec<WindowFact>) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        screens: screens(),
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        foreground_known: false,
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
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    let gate = ProtocolAdmissionGate::new();
    match engine
        .handle_report_with_gate_and_prepare(report, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {})
        .outcome
    {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

#[test]
fn oversized_request_refuses_before_any_mutation() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(&mut engine, &report(1, vec![fact("win-a", "out-a", 0)]));
    let revision = first.state_revision;
    // A validly shaped typed report past the 4 MiB request estimate refuses
    // at the cheap gate: long opaque ids inflate the estimate past the bound
    // while the request stays well formed before any mutation.
    let long = |prefix: &str, n: usize| format!("{prefix}-{n:05}-{}", "w".repeat(100));
    let windows: Vec<WindowFact> = (0..10_000)
        .map(|n| fact(&long("win", n), "out-a", 0))
        .collect();
    let big = report(2, windows);
    assert!(report_estimate_v4(&big) > WORLD_MAX_REQUEST_BYTES);
    let gate = ProtocolAdmissionGate::new();
    let transition = engine
        .handle_report_with_gate_and_prepare(&big, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(transition.dispositions.is_empty());
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "oversized"),
        "oversize gate refuses before mutation"
    );
    assert_eq!(engine.state_revision(), revision);
    assert!(engine.retained_transfers().is_empty());
    let calm = accepted(&mut engine, &report(2, vec![fact("win-a", "out-a", 0)]));
    assert_eq!(calm.state_revision, revision + 1);
}

fn toggle(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: tiler_core::directional::WindowSubject::Window(WindowId(window.to_owned())),
        float_rect: None,
    })
}

fn assert_save_true(reply: &WorldReplyV4, win: &str) {
    let expected = encode_intent_data(&IntentData::new(Some(true), None));
    let found = reply.intent_desired.iter().find(|row| {
        row.win == WindowId(win.to_owned())
            && row.data.as_ref().map(encode_intent_data).as_deref() == Some(expected.as_str())
    });
    assert!(found.is_some(), "missing save {win}: {reply:?}");
}

#[test]
fn oversized_toggle_report_refuses_before_mutation_and_retry_saves() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let revision = engine.state_revision();
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..1500 {
        let mut padded = fact(&format!("win-pad-{i:04}"), "out-a", 0);
        padded.title = Some("t".repeat(3000));
        windows.push(padded);
    }
    let mut big = report(2, windows);
    big.command_seq = Some(2);
    big.command = Some(toggle("win-a"));
    assert!(report_estimate_v4(&big) > WORLD_MAX_REQUEST_BYTES);
    let gate = ProtocolAdmissionGate::new();
    let transition = engine
        .handle_report_with_gate_and_prepare(&big, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(transition.dispositions.is_empty());
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "oversized"),
        "oversize gate refuses before mutation: {:?}",
        transition.outcome
    );
    assert_eq!(engine.state_revision(), revision);

    let mut retry = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    retry.command_seq = Some(2);
    retry.command = Some(toggle("win-a"));
    let reply = accepted(&mut engine, &retry);
    assert_save_true(&reply, "win-a");
}

#[test]
fn preflight_toggle_save_bound_crossing_refuses_and_retry_saves() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let mut probe = report(2, Vec::new());
    probe.command_seq = Some(2);
    probe.command = Some(toggle("win-a"));
    let save_bound = max_intent_save_bound_v4(&probe);
    assert!(save_bound > 0);

    let bound_of = |windows: &[WindowFact]| {
        let mut r = report(2, windows.to_vec());
        r.command_seq = Some(2);
        r.command = Some(toggle("win-a"));
        reply_upper_bound_v4(&r, 0, 1, 2, 0, 0)
    };
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) - save_bound > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(save_bound.saturating_sub(2))
        .saturating_sub(bound_of(&windows) - save_bound)
        + 2;
    windows[2].title = Some("t".repeat(deficit));

    let mut r = report(2, windows);
    r.command_seq = Some(2);
    r.command = Some(toggle("win-a"));
    let total = reply_upper_bound_v4(&r, 0, 1, 2, 0, 0);
    let without_save = total - save_bound;
    assert!(report_estimate_v4(&r) <= WORLD_MAX_REQUEST_BYTES);
    assert!(without_save <= WORLD_MAX_REPLY_BYTES, "{without_save}");
    assert!(without_save + save_bound > WORLD_MAX_REPLY_BYTES);

    let gate = ProtocolAdmissionGate::new();
    let mut saveless = r.clone();
    saveless.command_seq = None;
    saveless.command = None;
    let saveless_facts = PreflightGateFacts {
        effective_command: None,
        effective_queued: &[],
        retained_settlement_count: 0,
        domain_count: 1,
        screen_count: 2,
        staged_window_count: 0,
        retained_publish_footprint: 0,
        intent_desired_count: 0,
        operation_slot_count: 0,
        switch_output_count: 0,
        switch_failed_count: 0,
        issuing_selection_count: 0,
        policy_terminal_count: 0,
    };
    assert_eq!(
        gate.check_preflight(&saveless, &saveless_facts),
        AdmissionGateDecision::Allow
    );

    let revision = engine.state_revision();
    let transition = engine
        .handle_report_with_gate_and_prepare(&r, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(
        matches!(
            transition.dispositions.as_slice(),
            [tiler_core::world_report::BatchDisposition {
                seq: 2,
                disposition: tiler_core::world_report::CommandDisposition::NotRun,
            }]
        ),
        "expected NotRun disposition, got {:?}",
        transition.dispositions
    );
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "reply-oversize"),
        "expected reply-oversize, got {:?}",
        transition.outcome
    );
    assert_eq!(engine.state_revision(), revision);

    let mut retry = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    retry.command_seq = Some(2);
    retry.command = Some(toggle("win-a"));
    let reply = accepted(&mut engine, &retry);
    assert_save_true(&reply, "win-a");
}

#[test]
fn preflight_target_lanes_bound_crossing_refuses_and_retry_accepts() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let lanes_of = |windows_len: usize| windows_len.saturating_mul(MAX_TARGET_LANES_PER_WINDOW);
    let bound_of = |windows: &[WindowFact]| {
        let r = report(2, windows.to_vec());
        reply_upper_bound_v4(&r, 0, 1, 2, 0, 0)
    };
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) - lanes_of(windows.len()) > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(111)
        .saturating_sub(bound_of(&windows) - lanes_of(windows.len()))
        + 2;
    windows[2].title = Some("t".repeat(deficit));

    let r = report(2, windows);
    let lanes = lanes_of(r.windows.len());
    let without_lanes = reply_upper_bound_v4(&r, 0, 1, 2, 0, 0) - lanes;
    assert!(report_estimate_v4(&r) <= WORLD_MAX_REQUEST_BYTES);
    assert!(without_lanes <= WORLD_MAX_REPLY_BYTES, "{without_lanes}");
    assert!(without_lanes + lanes > WORLD_MAX_REPLY_BYTES);

    let revision = engine.state_revision();
    let gate = ProtocolAdmissionGate::new();
    let transition = engine
        .handle_report_with_gate_and_prepare(&r, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(transition.dispositions.is_empty());
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "reply-oversize"),
        "expected reply-oversize, got {:?}",
        transition.outcome
    );
    assert_eq!(engine.state_revision(), revision);

    let calm = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_eq!(calm.state_revision, revision + 1);
}

#[test]
fn preflight_state_attempt_diagnostic_bound_crossing_refuses_and_retry_accepts() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let attempts = 400usize;
    let make_attempts = || {
        (0..attempts)
            .map(|i| StateAttempt {
                window: WindowId(format!("win-pad-{i:05}")),
                property: StateProperty::Above,
                target: true,
                revision: 9,
                disposition: FrameSetter::Accepted,
                pre: Some(false),
                post: Some(true),
                attempt_ms: 1,
            })
            .collect::<Vec<_>>()
    };
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    let mut r = report(2, windows.clone());
    r.state_attempts = make_attempts();
    while reply_upper_bound_v4(&r, 0, 1, 2, 0, 0) <= WORLD_MAX_REPLY_BYTES {
        let i = windows.len();
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
        assert!(windows.len() < tiler_core::world_report::MAX_WORLD_FACTS);
        r = report(2, windows.clone());
        r.state_attempts = make_attempts();
        assert!(report_estimate_v4(&r) <= WORLD_MAX_REQUEST_BYTES);
    }
    let state_term = attempts * MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES;
    let without_state = reply_upper_bound_v4(&r, 0, 1, 2, 0, 0) - state_term;
    assert!(without_state <= WORLD_MAX_REPLY_BYTES, "{without_state}");
    assert!(without_state + state_term > WORLD_MAX_REPLY_BYTES);

    let gate = ProtocolAdmissionGate::new();
    let mut attemptless = r.clone();
    attemptless.state_attempts.clear();
    let attemptless_facts = PreflightGateFacts {
        effective_command: None,
        effective_queued: &[],
        retained_settlement_count: 0,
        domain_count: 1,
        screen_count: 2,
        staged_window_count: 0,
        retained_publish_footprint: 0,
        intent_desired_count: 0,
        operation_slot_count: 0,
        switch_output_count: 0,
        switch_failed_count: 0,
        issuing_selection_count: 0,
        policy_terminal_count: 0,
    };
    assert_eq!(
        gate.check_preflight(&attemptless, &attemptless_facts),
        AdmissionGateDecision::Allow
    );

    let revision = engine.state_revision();
    let transition = engine
        .handle_report_with_gate_and_prepare(&r, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(transition.dispositions.is_empty());
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "reply-oversize"),
        "expected reply-oversize, got {:?}",
        transition.outcome
    );
    assert_eq!(engine.state_revision(), revision);

    let calm = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_eq!(calm.state_revision, revision + 1);
}

#[test]
fn preflight_intent_desired_reserve_crossing_refuses_and_retry_accepts() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    let mut seed_toggle = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
    );
    seed_toggle.command_seq = Some(1);
    seed_toggle.command = Some(toggle("win-a"));
    let seeded = accepted(&mut engine, &seed_toggle);
    assert_save_true(&seeded, "win-a");

    let reserve = max_intent_desired_bound(1);
    assert!(reserve > 0);
    let bound_of = |windows: &[WindowFact]| {
        let r = report(3, windows.to_vec());
        reply_upper_bound_v4(&r, 0, 1, 2, 0, 0)
    };
    let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
    for i in 0..2200 {
        windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
    }
    while bound_of(&windows) > WORLD_MAX_REPLY_BYTES {
        windows.pop();
    }
    let deficit = WORLD_MAX_REPLY_BYTES
        .saturating_sub(reserve.saturating_sub(1))
        .saturating_sub(bound_of(&windows))
        + 2;
    windows[2].title = Some("t".repeat(deficit));

    let r = report(3, windows);
    let without_reserve = reply_upper_bound_v4(&r, 0, 1, 2, 0, 0);
    assert!(report_estimate_v4(&r) <= WORLD_MAX_REQUEST_BYTES);
    assert!(
        without_reserve <= WORLD_MAX_REPLY_BYTES,
        "{without_reserve}"
    );
    assert!(without_reserve + reserve > WORLD_MAX_REPLY_BYTES);

    let gate = ProtocolAdmissionGate::new();
    let zero_intent_facts = PreflightGateFacts {
        effective_command: None,
        effective_queued: &[],
        retained_settlement_count: 0,
        domain_count: 1,
        screen_count: 2,
        staged_window_count: 0,
        retained_publish_footprint: 0,
        intent_desired_count: 0,
        operation_slot_count: 0,
        switch_output_count: 0,
        switch_failed_count: 0,
        issuing_selection_count: 0,
        policy_terminal_count: 0,
    };
    assert_eq!(
        gate.check_preflight(&r, &zero_intent_facts),
        AdmissionGateDecision::Allow
    );

    let revision = engine.state_revision();
    let transition = engine
        .handle_report_with_gate_and_prepare(&r, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(transition.dispositions.is_empty());
    assert!(
        matches!(transition.outcome, WorldOutcome::Rejected { kind, .. } if kind == "reply-oversize"),
        "expected reply-oversize, got {:?}",
        transition.outcome
    );
    assert_eq!(engine.state_revision(), revision);

    let calm = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
        ),
    );
    assert_eq!(calm.state_revision, revision + 1);
    assert_save_true(&calm, "win-a");
}

#[test]
fn explicit_focus_with_newcomer_fits_preflight_reserve_and_canonical_count() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(&mut engine, &report(1, vec![fact("win-a", "out-a", 0)]));

    let mut input = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
    );
    input.command_seq = Some(2);
    input.command = Some(WorldCommand::Focus {
        window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: None,
    });
    let pre_bound = reply_upper_bound_v4(&input, 0, 1, 2, 0, 1);
    let reply = accepted(&mut engine, &input);
    let explicit = reply.focus_request.as_ref().expect("explicit request");
    assert_eq!(explicit.target.0, "win-a");
    assert_eq!(explicit.id, format!("focus-{}", reply.state_revision));
    assert_eq!(explicit.issued_revision, reply.state_revision);
    assert_eq!(explicit.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(explicit.cmd_seq, Some(2));
    assert_eq!(explicit.after_shown, None);

    let actual = count_reply_v4_bytes(&reply);
    assert!(
        actual <= pre_bound,
        "canonical reply {actual} fits pre-mutation bound {pre_bound}"
    );
}
