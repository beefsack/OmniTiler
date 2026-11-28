//! Execution-time logical focus cursor (retained-focus Q1): the cursor is
//! separate from native observed foreground and per-workspace history.
//! Accepted focus choices advance it immediately in FIFO order; a live
//! retained request owns the cursor until adjudicated; without one, the
//! eligible observed foreground owns it. Non-focus and refused commands never
//! erase it. Implicit subjects resolve from the cursor before a newer
//! focus decision retires the older authority.

use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, SameAxisMove, WindowId};
use crate::ids::{CorrelationId, OwnerId};
use crate::world_engine::{EventIssue, KIND_GENERATION, WorldEngine};
use crate::world_report::{
    BatchSettlement, CommandRejectKind, CommandResult, FocusFeedback, FocusRequestCause,
    HeldWindowRow, PresentationIngressV4, PresentationRowV4, RowDiagnosticKind, ScopeAuthority,
    SendSource, StateProperty, StateTarget, VisibilityReason, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{fact, screen};

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("valid generation")
}

fn ingress(fg: &str) -> PresentationIngressV4 {
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: None,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: None,
        foreground_out: if fg.is_empty() {
            None
        } else {
            Some(crate::directional::OutputId("out-a".to_owned()))
        },
    }
}

fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
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
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
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
        focus_changes: None,
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(ingress(focused)),
        intent_records: Vec::new(),
        renderer_input: None,
        pointer_activity: None,
        warp_spent: None,
    }
}

fn observe(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report(seq, windows, focused, None, None)
}

fn issue_for(report: &WorldReportV4) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn run_batched(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> (WorldReplyV4, Vec<BatchSettlement>) {
    match engine.handle_report_v4_batched(report, &issue_for(report), queued) {
        (V4RunOutcome::Accepted(reply), settlements) => (reply, settlements),
        (other, _) => panic!("expected accepted batch, got {other:?}"),
    }
}

fn focus_cmd(window: &str) -> WorldCommand {
    WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId(window.to_owned())),
        direction: None,
    }
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: crate::world_report::FocusDirectionalSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction).expect("direction"),
    }
}

fn assert_not_rejected(result: &Option<CommandResult>, what: &str) {
    assert!(
        !matches!(result, Some(CommandResult::Rejected { .. })),
        "{what} must not reject; got {result:?}"
    );
}

/// Target proof for an implicit native maximize: the settlement carries
/// the maximized window identity, not merely a non-rejection.
fn assert_maximize_on(result: &Option<CommandResult>, expected: &str, what: &str) {
    match result {
        Some(CommandResult::Maximize { window, .. }) => {
            assert_eq!(window.0, expected, "{what} must act on {expected}",)
        }
        other => panic!("{what} must settle maximize; got {other:?}"),
    }
}

fn windows() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

fn interval(after: Option<u64>, windows: &[&str]) -> crate::world_report::FocusChanges {
    use crate::world_report::{FocusChange, FocusChanges};

    FocusChanges {
        after,
        events: Some(
            windows
                .iter()
                .map(|win| FocusChange::Foreground {
                    win: Some(WindowId((*win).to_owned())),
                })
                .collect(),
        ),
    }
}

fn toggle_current_float() -> WorldCommand {
    WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::CurrentFocus,
        float_rect: None,
    })
}

#[test]
fn foreground_round_trip_supersedes_even_when_snapshot_matches_previous() {
    let mut engine = engine();
    let roster = || {
        let mut roster = windows();
        roster.push(fact("win-d", "out-a", 200));
        roster
    };
    let (boot, _) = run_batched(&mut engine, &observe(1, roster(), "win-a"), &[]);
    let (issued, _) = run_batched(
        &mut engine,
        &report(2, roster(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    assert!(issued.focus_request.is_some());
    let mut capture = observe(3, roster(), "win-a");
    capture.focus_changes = Some(interval(Some(issued.state_revision), &["win-d", "win-a"]));
    let (reply, _) = run_batched(&mut engine, &capture, &[]);
    assert!(reply.focus_request.is_none());
    assert_ne!(boot.state_revision, issued.state_revision);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-a".to_owned()))
    );
}

#[test]
fn exact_incorporation_revision_anchors_even_when_observation_does_not_advance() {
    let mut engine = engine();
    let mut init = observe(1, windows(), "win-a");
    init.observation_seq = 42;
    init.focus_changes = Some(interval(None, &[]));
    let (boot, _) = run_batched(&mut engine, &init, &[]);
    let mut command = report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b")));
    command.observation_seq = 42;
    command.focus_changes = Some(interval(Some(boot.state_revision), &[]));
    let (issued, _) = run_batched(&mut engine, &command, &[]);
    let mut capture = observe(3, windows(), "win-a");
    capture.observation_seq = 42;
    capture.focus_changes = Some(interval(Some(issued.state_revision), &[]));
    let (held, _) = run_batched(&mut engine, &capture, &[]);
    assert!(held.focus_request.is_some());
    assert_eq!(issued.focus_request, held.focus_request);

    // A previously valid anchor is now stale despite identical obs.
    let mut stale = observe(4, windows(), "win-a");
    stale.observation_seq = 42;
    stale.focus_changes = capture.focus_changes;
    let (retired, _) = run_batched(&mut engine, &stale, &[]);
    assert!(retired.focus_request.is_none());
}

#[test]
fn earlier_destination_intent_then_own_target_uses_observed_target_for_toggle_float() {
    let mut engine = engine();
    let roster = || {
        let mut roster = windows();
        roster.push(fact("win-d", "out-a", 200));
        roster
    };
    run_batched(&mut engine, &observe(1, roster(), "win-a"), &[]);
    let (issued, _) = run_batched(
        &mut engine,
        &report(2, roster(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    let request = issued.focus_request.as_ref().expect("retained focus");
    assert_eq!(request.target.0, "win-b");
    let mut capture = observe(3, roster(), "win-b");
    capture.focus_changes = Some(interval(Some(issued.state_revision), &["win-d"]));
    capture.focus_feedback.push(FocusFeedback {
        id: request.id.clone(),
        target: request.target.clone(),
        revision: request.issued_revision,
        disposition: crate::world_report::FocusDisposition::Called {
            setter: crate::world_report::FrameSetter::Accepted,
            reason: VisibilityReason::Empty,
        },
        pre: Some(WindowId("win-a".to_owned())),
        post: Some(request.target.clone()),
        foreground_proven: true,
        attempt_ms: 0,
        arrival: Vec::new(),
    });
    let trace = crate::world_engine::with_focus_trace_capture(|| {
        let (reply, settled) = run_batched(&mut engine, &capture, &[(2, toggle_current_float())]);
        assert_not_rejected(&settled[0].result, "implicit float");
        assert!(reply.focus_request.is_none());
        assert!(
            reply
                .windows
                .iter()
                .any(|row| row.lifetime.0 == "win-b" && row.floating)
        );
        assert!(
            reply
                .windows
                .iter()
                .any(|row| row.lifetime.0 == "win-d" && !row.floating)
        );
    });
    let evals: Vec<_> = trace
        .iter()
        .filter(|line| line.contains(" ev=focus-eval "))
        .collect();
    assert_eq!(evals.len(), 1);
    assert!(evals[0].ends_with("decision=no-call reason=foreground-intent"));
}

#[test]
fn first_decisive_foreground_event_classifies_retirement() {
    for (events, reason, foreground_index) in [
        (
            ["win-b", "win-a"],
            "decision=equality reason=already-target",
            None,
        ),
        (
            ["win-a", "win-b"],
            "decision=no-call reason=foreground-intent",
            Some(0),
        ),
    ] {
        let mut engine = engine();
        run_batched(&mut engine, &observe(1, windows(), "win-a"), &[]);
        let (issued, _) = run_batched(
            &mut engine,
            &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
            &[],
        );
        assert!(issued.focus_request.is_some());
        let mut capture = observe(3, windows(), events[1]);
        capture.focus_changes = Some(interval(Some(issued.state_revision), &events));
        let trace = crate::world_engine::with_focus_trace_capture(|| {
            let (settled, _) = run_batched(&mut engine, &capture, &[]);
            assert!(settled.focus_request.is_none());
            assert_eq!(
                engine.tx(|tx| tx.execution_subject()),
                Some(WindowId(events[1].to_owned()))
            );
        });
        let evals: Vec<_> = trace
            .iter()
            .filter(|line| line.contains(" ev=focus-eval "))
            .collect();
        assert_eq!(evals.len(), 1);
        assert!(evals[0].ends_with(reason));
        let foreground: Vec<_> = trace
            .iter()
            .filter(|line| line.contains(" ev=focus-foreground-intent "))
            .collect();
        if let Some(index) = foreground_index {
            assert_eq!(foreground.len(), 1);
            assert!(foreground[0].ends_with(&format!(" eventIndex={index} win={}", events[index])));
        } else {
            assert!(foreground.is_empty());
        }
    }
}

#[test]
fn directional_then_toggle_float_uses_new_fifo_target_without_native_activation() {
    let mut engine = engine();
    // The startup layout puts B to the left of A.
    let (boot, _) = run_batched(&mut engine, &observe(1, windows(), "win-a"), &[]);
    let mut capture = observe(2, windows(), "win-a");
    capture.focus_changes = Some(interval(Some(boot.state_revision), &[]));
    let (reply, settled) = run_batched(
        &mut engine,
        &capture,
        &[(1, implicit_focus_dir("left")), (2, toggle_current_float())],
    );
    assert_not_rejected(&settled[0].result, "directional focus");
    assert_not_rejected(&settled[1].result, "implicit float");
    assert_eq!(reply.focus_request.as_ref().unwrap().target.0, "win-b");
    assert!(
        reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-b" && row.floating)
    );
    assert!(
        reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "win-a" && !row.floating)
    );
}

#[test]
fn focus_interval_is_structurally_bounded_and_part_of_exact_replay() {
    use crate::world_report::{FocusChange, FocusChanges, MAX_FOCUS_CHANGES};
    use crate::world_report_v4_checks::{report_digest_v4, validate_report_v4_reason};

    let mut engine = engine();
    let mut capture = observe(1, windows(), "win-a");
    let unknown = FocusChange::Foreground { win: None };
    let known_none = FocusChange::Foreground {
        win: Some(WindowId(String::new())),
    };
    capture.focus_changes = Some(FocusChanges {
        after: None,
        events: Some(vec![unknown.clone(), known_none.clone()]),
    });
    let first = run_batched(&mut engine, &capture, &[]).0;
    let replay = |engine: &mut WorldEngine, capture: &WorldReportV4| {
        engine
            .handle_report_v4_batched(capture, &issue_for(capture), &[])
            .0
    };
    assert!(matches!(
        replay(&mut engine, &capture),
        V4RunOutcome::Duplicate(reply) if reply == first
    ));

    let mut changed = capture.clone();
    let evidence = changed.focus_changes.as_mut().unwrap();
    evidence.events.as_mut().unwrap().reverse();
    assert_ne!(report_digest_v4(&capture), report_digest_v4(&changed));
    assert!(matches!(
        replay(&mut engine, &changed),
        V4RunOutcome::Rejected {
            kind: crate::world_engine::KIND_REUSED,
            ..
        }
    ));

    let variants = [
        None,
        Some(FocusChanges {
            after: None,
            events: None,
        }),
        Some(interval(None, &[])),
        Some(interval(Some(0), &[])),
        Some(FocusChanges {
            after: None,
            events: Some(vec![unknown.clone()]),
        }),
        Some(FocusChanges {
            after: None,
            events: Some(vec![known_none]),
        }),
    ];
    let digests: std::collections::BTreeSet<_> = variants
        .into_iter()
        .map(|proof| {
            changed.focus_changes = proof;
            report_digest_v4(&changed)
        })
        .collect();
    assert_eq!(digests.len(), 6);

    changed.focus_changes = Some(FocusChanges {
        after: None,
        events: Some(vec![unknown; MAX_FOCUS_CHANGES + 1]),
    });
    assert_eq!(
        validate_report_v4_reason(&changed),
        Some("section-over-cap")
    );
    for event in [
        FocusChange::Foreground {
            win: Some(WindowId("x".repeat(crate::bounds::MAX_OPAQUE_ID_LEN + 1))),
        },
        FocusChange::View {
            out: OutputId(String::new()),
            native: crate::world_report::ViewRef::Native {
                output: OutputId(String::new()),
                workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
            },
        },
        FocusChange::View {
            out: OutputId("out-a".to_owned()),
            native: crate::world_report::ViewRef::Native {
                output: OutputId(String::new()),
                workspace: crate::ids::NativeWorkspaceId::parse("n1").expect("native id"),
            },
        },
    ] {
        changed.focus_changes.as_mut().unwrap().events = Some(vec![event]);
        assert_eq!(
            validate_report_v4_reason(&changed),
            Some("focus-change-id-invalid")
        );
    }
}

#[test]
fn complete_focus_interval_does_not_mask_positive_target_loss() {
    let mut engine = engine();
    run_batched(&mut engine, &observe(1, windows(), "win-a"), &[]);
    let (issued, _) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    assert!(issued.focus_request.is_some());
    let mut capture = observe(3, vec![fact("win-a", "out-a", 0)], "win-a");
    capture.closed.push(WindowId("win-b".to_owned()));
    capture.focus_changes = Some(interval(Some(issued.state_revision), &[]));
    let (reply, _) = run_batched(&mut engine, &capture, &[]);
    assert!(reply.focus_request.is_none());
}

/// Focus B, then a non-focus command in the same batch, then a later
/// unchanged-baseline capture: the pending request survives both, and a
/// trailing implicit command acts on the cursor (B), not stale native A.
#[test]
fn cursor_focus_then_nonfocus_survives_and_implicit_acts_on_cursor() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    // Same batch: accepted focus B followed by an accepted non-focus
    // command. The non-focus command must not cancel pending B.
    let (setup, settlements) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[(2, WorldCommand::SelectPrevious { output: None })],
    );
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing non-focus settles");
    assert_not_rejected(&trailing.result, "trailing non-focus");
    // Focus never publishes as one-shot effects on the v4 wire: the
    // retained request is the authority.
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "accepted focus advances the cursor immediately"
    );
    // Later capture on an unchanged baseline (native still A): the live
    // retained target survives with the identical id.
    let (held, _) = run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    let again = held.focus_request.clone().expect("pending B survives");
    assert_eq!(again.target.0, "win-b");
    assert_eq!(again.id, pending.id, "re-emission never changes the id");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "stale observed A must not overwrite pending B"
    );
    // The next implicit subject resolves from the cursor (B): moving
    // right from B reaches A, which stale-A resolution could never do.
    // The new decision supersedes pending B with a new id.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(3, implicit_focus_dir("right"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let superseded = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(
        superseded.target.0, "win-a",
        "implicit subject must be cursor B, not stale native A"
    );
    assert_ne!(superseded.id, pending.id);
}

/// Focus B, then a refused non-focus command: the refusal settles only
/// its own sequence while pending B survives with its cursor intact.
#[test]
fn cursor_refused_nonfocus_never_cancels_pending() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let refused_send = WorldCommand::Send {
        window: crate::directional::WindowSubject::Window(WindowId("win-zzz".to_owned())),
        target_output: crate::directional::OutputId("out-a".to_owned()),
        target_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
        follow: false,
    };
    let (reply, settlements) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[(2, refused_send)],
    );
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing send settles");
    assert!(
        matches!(trailing.result, Some(CommandResult::Rejected { .. })),
        "trailing send must refuse; got {:?}",
        trailing.result
    );
    let pending = reply
        .focus_request
        .clone()
        .expect("pending B survives refusal");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "refused non-focus must not erase the cursor"
    );
    // The CoreCommand operation seam stays non-focus vocabulary: an
    // operation refusal is equally powerless over pending focus.
    let refused_op = WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
            "win-zzz".to_owned(),
        )),
        direction: crate::directional::Direction::Left,
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    };
    let (later, _) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(3, refused_op)],
    );
    let still = later
        .focus_request
        .clone()
        .expect("pending B survives op refusal");
    assert_eq!(still.target.0, "win-b");
    assert_eq!(still.id, pending.id);
}

/// Pending B refused by correlated feedback: the cursor resets to the
/// eligible observed focus (A), never the abandoned target, while
/// per-workspace history keeps B. The next implicit subject is A.
#[test]
fn cursor_refused_pending_resets_to_observed_not_abandoned() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    // Correlated refusal for the exact pending tuple retires it.
    let mut refused = observe(3, windows(), "win-a");
    refused.focus_feedback = vec![FocusFeedback {
        id: pending.id.clone(),
        target: WindowId("win-b".to_owned()),
        revision: pending.issued_revision,
        disposition: crate::world_report::FocusDisposition::Called {
            setter: crate::world_report::FrameSetter::Refused,
            reason: VisibilityReason::NativeRefused,
        },
        pre: Some(WindowId("win-a".to_owned())),
        post: Some(WindowId("win-a".to_owned())),
        foreground_proven: false,
        arrival: Vec::new(),
        attempt_ms: 0,
    }];
    let (settled, _) = run_batched(&mut engine, &refused, &[]);
    assert!(
        settled.focus_request.is_none(),
        "correlated refusal must retire pending B; got {:?}",
        settled.focus_request
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-a".to_owned())),
        "retired cursor resets to eligible observed A, not abandoned B"
    );
    assert!(
        engine
            .world
            .focus
            .last_active_window
            .values()
            .any(|w| w.0 == "win-b"),
        "per-workspace history stays on B while the cursor resets to A"
    );
    // The next implicit subject is observed A: moving left from A
    // reaches B, which abandoned-B resolution could never produce.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(2, implicit_focus_dir("left"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(
        next.target.0, "win-b",
        "implicit subject after refusal must be observed A, not abandoned B"
    );
}

/// A restart-shaped first capture (no home for A) establishes no execution
/// subject. Commandless recovery adopts the restored sticky foreground A
/// without minting a request, and implicit sticky-off then acts on A
/// without changing canonical memory or session state.
#[test]
fn implicit_acts_on_restored_sticky_foreground() {
    let mut engine = engine();
    // No home for A yet: the host reports it focused, but nothing is
    // recordable, so no subject is established.
    engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-b", "out-a", 0)], "win-a"))
        .expect("admission observes");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        None,
        "homeless first capture leaves no execution subject"
    );
    let windows = vec![
        WindowFact {
            sticky: Some(true),
            ..fact("win-a", "out-a", 0)
        },
        fact("win-b", "out-a", 100),
    ];
    // Commandless recovery adopts A with identity unchanged. The sticky
    // lane mints nothing; the recovered observation now owns the cursor.
    let (recovered, _) = run_batched(&mut engine, &observe(2, windows.clone(), "win-a"), &[]);
    assert!(
        recovered.focus_request.is_none(),
        "recovery mints no automatic request; got {:?}",
        recovered.focus_request
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned())),
        "recovery seeds the cursor from live A, not a new focus decision"
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-a".to_owned())),
        "live A is the implicit subject"
    );
    let before = engine.world.focus.last_active_window.clone();
    let toggle = WorldCommand::ToggleSticky {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) =
        run_batched(&mut engine, &observe(3, windows, "win-a"), &[(1, toggle)]);
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "implicit sticky-off acts on live A");
    assert!(
        engine
            .world
            .settlement
            .retained_states
            .get(&(WindowId("win-a".to_owned()), StateProperty::Sticky))
            .is_some_and(|binding| !binding.target.value),
        "implicit sticky-off lands on win-a; got {:?}",
        engine
            .world
            .settlement
            .retained_states
            .keys()
            .collect::<Vec<_>>(),
    );
    assert!(
        reply.focus_request.is_none(),
        "fallback mints no focus request; got {:?}",
        reply.focus_request
    );
    assert_eq!(
        engine.world.focus.last_active_window, before,
        "fallback writes no canonical memory"
    );
}

/// Discriminator 2: windows the automatic lane withholds still serve as
/// the implicit subject once the host focuses them: a newcomer sticky, a
/// float owning the foreground over a withheld tiled newcomer, and a
/// maximized ordinary float.
#[test]
fn empty_cursor_implicit_acts_despite_withheld_automatic_guards() {
    // Newcomer sticky: the automatic target restriction withholds it.
    let mut sticky_engine = engine();
    sticky_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-b", "out-a", 0)], "win-s"))
        .expect("admission observes");
    assert_eq!(sticky_engine.tx(|tx| tx.execution_subject()), None);
    let sticky_windows = vec![
        WindowFact {
            sticky: Some(true),
            ..fact("win-s", "out-a", 0)
        },
        fact("win-b", "out-a", 100),
    ];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) = run_batched(
        &mut sticky_engine,
        &observe(2, sticky_windows, "win-s"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "newcomer sticky");
    assert_maximize_on(&settlements[0].result, "win-s", "newcomer sticky maximize");
    assert!(
        reply.focus_request.is_none(),
        "newcomer sticky: no focus request minted; got {:?}",
        reply.focus_request
    );
    // Floating over the foreground: the tiled newcomer's activation is
    // withheld under the focused float, so no subject is established, and
    // the implicit command then acts on the float itself. This Act 1 is
    // the distinct empty-cursor-F case: the withheld newcomer T is present
    // but unfocused, so the fallback can only see F.
    let mut float_engine = engine();
    float_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-a", "out-a", 0)], "win-f"))
        .expect("admission observes");
    assert_eq!(float_engine.tx(|tx| tx.execution_subject()), None);
    let float_windows = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            transient: Some(true),
            ..fact("win-f", "out-a", 100)
        },
        fact("win-t", "out-a", 200),
    ];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) = run_batched(
        &mut float_engine,
        &observe(2, float_windows.clone(), "win-f"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "floating over foreground");
    assert_maximize_on(&settlements[0].result, "win-f", "empty-cursor fallback");
    assert!(
        reply.focus_request.is_none(),
        "floating over foreground: no focus request minted; got {:?}",
        reply.focus_request
    );
    // Act 2 (withheld-newcomer-later-focused): the host now focuses the
    // previously withheld tiled newcomer T. The implicit command acts on
    // T, with no rearmed automatic focus. No canonical-memory assertion
    // here: the genuine native focus change F -> T records through the
    // pre-existing native path.
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) = run_batched(
        &mut float_engine,
        &observe(3, float_windows, "win-t"),
        &[(2, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "later-focused withheld newcomer");
    assert_maximize_on(
        &settlements[0].result,
        "win-t",
        "later-focused withheld newcomer",
    );
    assert!(
        reply.focus_request.is_none(),
        "withheld newcomer focus mints no automatic request; got {:?}",
        reply.focus_request
    );
    // Maximized ordinary float: the automatic float-overlay veto withholds
    // it, but the implicit command still acts on it once host-focused.
    let mut max_engine = engine();
    max_engine
        .handle_report_v4_for_test(&observe(1, vec![fact("win-a", "out-a", 0)], "win-m"))
        .expect("admission observes");
    assert_eq!(max_engine.tx(|tx| tx.execution_subject()), None);
    let max_windows = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            transient: Some(true),
            maximized: Some(true),
            ..fact("win-m", "out-a", 100)
        },
    ];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) = run_batched(
        &mut max_engine,
        &observe(2, max_windows, "win-m"),
        &[(1, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "maximized ordinary float");
    assert_maximize_on(&settlements[0].result, "win-m", "maximized ordinary float");
    assert!(
        reply.focus_request.is_none(),
        "maximized ordinary float: no focus request minted; got {:?}",
        reply.focus_request
    );
}

/// Discriminator 3: with an empty cursor, an unknown, unbound, or stale
/// (closed) foreground is still refused as focus-unknown.
#[test]
fn empty_cursor_unknown_unbound_stale_still_refused() {
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    // Unknown: names no live fact.
    let mut unknown_engine = engine();
    unknown_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    assert_eq!(unknown_engine.tx(|tx| tx.execution_subject()), None);
    let (_, settlements) = run_batched(
        &mut unknown_engine,
        &observe(2, windows(), "win-ghost"),
        &[(1, toggle())],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    // Unbound: a fact with no logical homing.
    let mut unbound_engine = engine();
    unbound_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let mut unbound = observe(
        2,
        vec![
            fact("win-a", "out-a", 0),
            WindowFact {
                output: None,
                ..fact("win-u", "out-a", 200)
            },
        ],
        "win-u",
    );
    unbound.foreground_known = true;
    let (_, settlements) = run_batched(&mut unbound_engine, &unbound, &[(1, toggle())]);
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unbound foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    // Stale: explicitly closed this report.
    let mut stale_engine = engine();
    stale_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let mut stale = observe(2, vec![fact("win-a", "out-a", 0)], "win-b");
    stale.closed = vec![WindowId("win-b".to_owned())];
    let (_, settlements) = run_batched(&mut stale_engine, &stale, &[(1, toggle())]);
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "stale foreground must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
}

/// Discriminator 3b (USER DECISION 2026-11-12, stale-generation
/// ingress): a wrong-generation report with a queued implicit is refused
/// at the generation gate before any run or mutation -- empty
/// settlements, no subject adopted, no state effects -- and a following
/// current-bound unknown-foreground report then refuses focus-unknown at
/// command scope. The gate refusal must not be conflated with the
/// command FocusUnknown that follows it.
#[test]
fn empty_cursor_stale_generation_ingress_refuses_before_run() {
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let mut gen_engine = engine();
    gen_engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    assert_eq!(gen_engine.tx(|tx| tx.execution_subject()), None);
    let revision = gen_engine.state_revision();
    let mut bad = observe(2, windows(), "win-a");
    bad.core_generation = "gen-999".to_owned();
    let (outcome, settlements) =
        gen_engine.handle_report_v4_batched(&bad, &issue_for(&bad), &[(1, toggle())]);
    match outcome {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(
            kind, KIND_GENERATION,
            "wrong generation must refuse at the generation gate",
        ),
        other => panic!("wrong generation must gate-reject; got {other:?}"),
    }
    assert!(
        settlements.is_empty(),
        "gate refusal settles nothing; got {settlements:?}",
    );
    assert_eq!(
        gen_engine.tx(|tx| tx.execution_subject()),
        None,
        "gate refusal adopts no subject",
    );
    assert_eq!(
        gen_engine.world.focus.logical_focus_cursor, None,
        "gate refusal sets no cursor slot",
    );
    assert!(
        gen_engine.tx(|tx| tx.pending_focus_request()).is_none(),
        "gate refusal mints no request; got {:?}",
        gen_engine.tx(|tx| tx.pending_focus_request()),
    );
    assert_eq!(
        gen_engine.state_revision(),
        revision,
        "gate refusal mutates no state",
    );
    // The next current-bound report is unaffected by the gate refusal:
    // its unknown foreground refuses focus-unknown at command scope.
    let (_, settlements) = run_batched(
        &mut gen_engine,
        &observe(3, windows(), "win-ghost"),
        &[(2, toggle())],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground after gate refusal must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
}

/// Pending-live guard: an explicit focus on a newcomer mints a retained
/// request through the production run, while its narrowly held home
/// scope fail-closes the canonical record so the cursor slot stays
/// empty. The live request then owns the subject -- the empty cursor
/// never falls back to observed foreground -- so a queued implicit
/// refuses focus-unknown while the request stays live.
#[test]
fn pending_live_empty_cursor_guards_implicit_subject() {
    let mut pending_engine = engine();
    pending_engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-h", "out-a", 100)],
            "win-a",
        ))
        .expect("admission observes");
    assert_eq!(pending_engine.tx(|tx| tx.execution_subject()), None);
    // win-h rides a narrowly held row (live rows stay unheld) so its
    // retained scope -- also win-n's home -- fences the canonical
    // record without refusing the explicit effect.
    let mut held_focus = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-a", 100)],
        "win-a",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    held_focus.held_windows = vec![HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let (reply, _) = run_batched(&mut pending_engine, &held_focus, &[]);
    let pending = reply
        .focus_request
        .clone()
        .expect("held-scope explicit focus is retained");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(
        pending_engine.world.focus.logical_focus_cursor, None,
        "fail-closed canonical record leaves the cursor slot empty",
    );
    assert_eq!(
        pending_engine.tx(|tx| tx.execution_subject()),
        None,
        "a live retained request owns the subject; empty cursor never falls back",
    );
    // The next report drops win-n from the staged set on the unchanged
    // baseline: the preserve-slot logic cannot rehome it, so the cursor
    // stays empty while the retained request stays live, and the queued
    // implicit refuses focus-unknown under the guard.
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (_, settlements) = run_batched(
        &mut pending_engine,
        &observe(3, vec![fact("win-b", "out-a", 0)], "win-a"),
        &[(2, toggle)],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(
            &settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "implicit under live pending with empty cursor must refuse focus-unknown; got {:?}",
        settlements[0].result
    );
    let still = pending_engine
        .tx(|tx| tx.pending_focus_request())
        .expect("retained request stays live across the refusal");
    assert_eq!(still.target.0, "win-n");
    assert_eq!(still.id, pending.id);
}

/// Discriminator 4: a live logical cursor still wins over a divergent
/// observed foreground; the empty-cursor fallback never overrides it.
#[test]
fn logical_cursor_wins_over_divergent_observed() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(2, windows(), "win-a", Some(1), Some(focus_cmd("win-b"))),
        &[],
    );
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    let (held, _) = run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    assert!(held.focus_request.is_some(), "pending B survives");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "divergent observed A must not override cursor B"
    );
    // The next implicit subject resolves from the cursor (B): the new
    // rightward decision supersedes pending B with a fresh id.
    let (moved, moved_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(3, implicit_focus_dir("right"))],
    );
    assert_eq!(moved_settlements.len(), 1);
    assert_not_rejected(&moved_settlements[0].result, "implicit directional");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-a");
    assert_ne!(next.id, pending.id, "cursor path supersedes with a new id");
}

fn gap_windows() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ]
}

fn implicit_move_dir(direction: &str) -> WorldCommand {
    WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction).expect("direction"),
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    }
}

fn scope_hold_any(reply: &WorldReplyV4) -> bool {
    reply
        .scopes
        .iter()
        .any(|status| matches!(status.authority, ScopeAuthority::Hold(_)))
}

fn gap_prefix(engine: &mut WorldEngine) -> WorldReplyV4 {
    engine
        .handle_report_v4_for_test(&observe(1, gap_windows(), "win-a"))
        .expect("admission observes");
    let (_, _) = run_batched(engine, &observe(2, gap_windows(), "win-a"), &[]);
    let mut gapped = observe(
        3,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-b",
    );
    gapped.screens[0].complete = false;
    gapped.inventory_complete = false;
    gapped.held_windows = vec![HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let (reply, _) = run_batched(engine, &gapped, &[]);
    assert!(
        scope_hold_any(&reply),
        "gap report holds the source domain; got {:?}",
        reply.scopes
    );
    assert!(
        reply.focus_request.is_none(),
        "gap mints no request; got {:?}",
        reply.focus_request
    );
    let (recovered, _) = run_batched(engine, &observe(4, gap_windows(), "win-b"), &[]);
    assert!(
        !scope_hold_any(&recovered),
        "recovery clears the hold; got {:?}",
        recovered.scopes
    );
    assert!(
        recovered.focus_request.is_none(),
        "recovery mints no request; got {:?}",
        recovered.focus_request
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-b".to_owned())),
        "without a pending focus decision, the recovered observation owns the cursor"
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "recovery subject is observed B"
    );
    recovered
}

/// Gap then focus: incomplete inventory plus a held source domain on a
/// genuine A -> B native change, then complete unchanged B. The observed
/// rightward probe from B enters V[C,A] and selects A (most recently
/// focused descendant).
#[test]
fn gap_held_incomplete_then_focus_acts_on_recovered_foreground() {
    let mut engine = engine();
    gap_prefix(&mut engine);
    let (moved, settlements) = run_batched(
        &mut engine,
        &observe(5, gap_windows(), "win-b"),
        &[(7, implicit_focus_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "gap focus probe");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-a", "observed rightward probe from B");
    assert_eq!(next.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(next.cmd_seq, Some(7));
    assert_eq!(next.id, "focus-5");
}

/// Gap then move: same prefix on a fresh engine. The observed rightward
/// windowed move from B changes the tiling and keeps focus on mover B.
#[test]
fn gap_held_incomplete_then_move_acts_on_recovered_foreground() {
    let mut engine = engine();
    let recovered = gap_prefix(&mut engine);
    let before: Vec<_> = recovered
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(5, gap_windows(), "win-b"),
        &[(7, implicit_move_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        settlements[0].result.is_none(),
        "tiled move success carries no result; got {:?}",
        settlements[0].result
    );
    assert!(
        reply.focus_request.as_ref().is_some_and(|row| {
            row.target.0 == "win-b"
                && row.cause == FocusRequestCause::ExplicitCommand
                && row.cmd_seq == Some(7)
                && row.id == "focus-5"
        }),
        "move keeps focus on mover B; got {:?}",
        reply.focus_request
    );
    let after: Vec<_> = reply
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    assert_ne!(before, after, "move must change the tiling allocation");
    let moved_b = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row");
    assert!(
        before
            .iter()
            .any(|(win, alloc)| win == "win-b" && *alloc != moved_b.allocation),
        "B allocation must change; before {before:?} after {after:?}"
    );
}

/// Send-with-follow settles by target equality on the next unchanged
/// observation (zero-report retention): the follow row lives for the
/// issuing publication only. A later genuine user focus change then runs
/// on the new baseline with no row involved.
#[test]
fn send_follow_satisfies_at_next_observation_and_changed_focus_runs_new_baseline() {
    let mut engine = engine();
    let facts = vec![
        fact("win-a", "out-a", 0),
        fact("win-s", "out-a", 100),
        fact("win-b", "out-a", 200),
    ];
    let first = engine
        .handle_report_v4_for_test(&observe(1, facts.clone(), "win-a"))
        .expect("admission observes");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "same-output send needs a ring: {ring:?}");
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("logical view");
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("second workspace");
    let send = WorldCommand::Send {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        target_output: crate::directional::OutputId("out-a".to_owned()),
        target_workspace: crate::directional::WorkspaceId(other.clone()),
        follow: true,
    };
    let (sent, _) = run_batched(
        &mut engine,
        &report(2, facts.clone(), "win-a", Some(11), Some(send)),
        &[],
    );
    let pending = sent.focus_request.clone().expect("send-follow retains");
    assert_eq!(pending.target.0, "win-a");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(pending.cmd_seq, Some(11));
    assert_eq!(pending.id, "focus-2");
    assert_eq!(pending.issued_revision, 2);
    assert_eq!(pending.after_shown, None);
    let satisfied = run_batched(&mut engine, &observe(3, facts.clone(), "win-a"), &[]).0;
    assert_eq!(
        satisfied.focus_request, None,
        "follow satisfies on next unchanged observation; got {:?}",
        satisfied.focus_request
    );
    // Genuine user focus change to win-s, then the observed implicit
    // right from S reaches B.
    let (moved, settlements) = run_batched(
        &mut engine,
        &observe(4, facts.clone(), "win-s"),
        &[(12, implicit_focus_dir("right"))],
    );
    assert_eq!(settlements.len(), 1);
    assert_not_rejected(&settlements[0].result, "implicit after change");
    let next = moved.focus_request.clone().expect("new decision retained");
    assert_eq!(next.target.0, "win-b", "observed rightward probe from S");
}

/// Held domain with a live bound baseline: B/C live, native moves B -> C
/// while the source screen is held, then an explicit focus on newcomer N
/// in the held domain mints a live row. Repeated held reports keep N
/// with a restored cursor; a held move refuses with the row retained;
/// clearing the hold lets a focus probe supersede to C and a windowed
/// probe act on C.
#[test]
fn held_live_baseline_newcomer_restores_cursor_and_probes_act_from_it() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
            "win-b",
        ))
        .expect("admission observes");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned()))
    );
    let live = vec![
        fact("win-n", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    // Genuine native change B -> C with the source screen held.
    let mut held_change = observe(
        2,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
    );
    held_change.screens[0].complete = false;
    let (changed, _) = run_batched(&mut engine, &held_change, &[]);
    assert!(
        scope_hold_any(&changed),
        "native change under incomplete screen holds; got {:?}",
        changed.scopes
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-c".to_owned())),
        "held scope does not invent focus or mask the live bound observation"
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-c".to_owned())),
        "live C is the subject; individual commands still apply their own hold guards"
    );
    // Explicit focus on the live held-domain newcomer N mints a row.
    let mut held_mint = report(3, live.clone(), "win-c", Some(1), Some(focus_cmd("win-n")));
    held_mint.screens[0].complete = false;
    let (minted, _) = run_batched(&mut engine, &held_mint, &[]);
    let pending = minted.focus_request.clone().expect("held explicit retains");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    // The same held baseline with no command keeps N and restores the
    // cursor to N even while held: the preserve slot has no hold gate.
    let mut held_same = observe(4, live.clone(), "win-c");
    held_same.screens[0].complete = false;
    let (kept, _) = run_batched(&mut engine, &held_same, &[]);
    assert_eq!(
        kept.focus_request.clone().map(|row| (row.target, row.id)),
        Some((WindowId("win-n".to_owned()), pending.id.clone())),
        "held repeat keeps the identical row; got {:?}",
        kept.focus_request
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-n".to_owned())),
        "live N restores the cursor even while held"
    );
    // Held move from N refuses (held source domain fences mutation) and
    // the live row survives the refusal unchanged.
    let mut held_move = observe(5, live.clone(), "win-c");
    held_move.screens[0].complete = false;
    let (after_move, move_settlements) =
        run_batched(&mut engine, &held_move, &[(2, implicit_move_dir("right"))]);
    assert_eq!(move_settlements.len(), 1);
    assert!(
        matches!(
            move_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            })
        ),
        "held move must refuse operation-refused; got {:?}",
        move_settlements[0].result
    );
    assert_eq!(
        after_move
            .focus_request
            .clone()
            .map(|row| (row.target, row.id)),
        Some((WindowId("win-n".to_owned()), pending.id.clone())),
        "held refusal retains the identical row; got {:?}",
        after_move.focus_request
    );
    // Repeated Focus(N) on the still-held domain when the cursor is
    // already Some(N) must preserve the matching cursor so a same-batch
    // implicit probe resolves subject N (refusing OperationRefused on the
    // held scope, never FocusUnknown). The new explicit decision supersedes
    // the earlier request even though its target is unchanged.
    let mut held_repeat_focus = report(6, live.clone(), "win-c", Some(3), Some(focus_cmd("win-n")));
    held_repeat_focus.screens[0].complete = false;
    let (after_repeat, repeat_settlements) = run_batched(
        &mut engine,
        &held_repeat_focus,
        &[(4, implicit_move_dir("right"))],
    );
    assert_eq!(repeat_settlements.len(), 2);
    assert!(
        matches!(
            repeat_settlements[1].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            })
        ),
        "repeated same-target held focus must preserve cursor N so trailing probe refuses operation-refused (not focus-unknown); got {:?}",
        repeat_settlements[1].result
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-n".to_owned())),
        "repeated same-target held focus must keep matching cursor N"
    );
    let repeated = after_repeat.focus_request.expect("new explicit decision");
    assert_eq!(repeated.target.0, "win-n");
    assert_eq!(repeated.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(repeated.cmd_seq, Some(3));
    assert_eq!(repeated.issued_revision, 6);
    assert_ne!(repeated.id, pending.id);
    // Clear the hold: the observed leftward focus from N reaches C and
    // supersedes with a fresh id, then a windowed probe acts on C.
    let (after_focus, focus_settlements) = run_batched(
        &mut engine,
        &observe(7, live.clone(), "win-c"),
        &[(5, implicit_focus_dir("left"))],
    );
    assert_eq!(focus_settlements.len(), 1);
    assert_not_rejected(&focus_settlements[0].result, "cleared focus probe");
    let next = after_focus.focus_request.clone().expect("new decision");
    assert_eq!(next.target.0, "win-c", "observed leftward probe from N");
    assert_eq!(next.cause, FocusRequestCause::ExplicitCommand);
    assert_ne!(next.id, pending.id, "focus probe supersedes with a new id");
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (_, toggle_settlements) = run_batched(
        &mut engine,
        &observe(8, live.clone(), "win-c"),
        &[(6, toggle)],
    );
    assert_eq!(toggle_settlements.len(), 1);
    assert_maximize_on(
        &toggle_settlements[0].result,
        "win-c",
        "cleared windowed probe",
    );
}

/// Held target absent from live windows: explicit N mints while live,
/// then N survives only as a held row while B/C stay live under a known
/// C baseline. Unlike the live-N case above (where the preserve slot
/// restores the cursor because staged homing holds N), here staged
/// homing lacks N, so repeated held reports keep the identical row with
/// an empty cursor and no subject; recovery with N live restores N.
#[test]
fn held_absent_target_retains_row_with_empty_cursor_until_live_recovery() {
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
            "win-b",
        ))
        .expect("admission observes");
    let live = vec![
        fact("win-n", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    let mut held_change = observe(
        2,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
    );
    held_change.screens[0].complete = false;
    let (changed, _) = run_batched(&mut engine, &held_change, &[]);
    assert!(
        scope_hold_any(&changed),
        "native change under incomplete screen holds; got {:?}",
        changed.scopes
    );
    // Explicit focus on newcomer N mints while N is still live.
    let mut held_mint = report(3, live.clone(), "win-c", Some(1), Some(focus_cmd("win-n")));
    held_mint.screens[0].complete = false;
    let (minted, _) = run_batched(&mut engine, &held_mint, &[]);
    let pending = minted.focus_request.clone().expect("held explicit retains");
    assert_eq!(pending.target.0, "win-n");
    let held_n = || HeldWindowRow {
        window: WindowId("win-n".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let absent = |seq: u64| {
        let mut report = observe(
            4 + seq,
            vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
            "win-c",
        );
        report.screens[0].complete = false;
        report.held_windows = vec![held_n()];
        report
    };
    // Same held baseline with no command: the identical row survives,
    // and with staged homing lacking N the preserve slot cannot restore,
    // so the cursor stays empty while this newer report observes live
    // bound C as the execution subject (option 2).
    let (kept, _) = run_batched(&mut engine, &absent(0), &[]);
    assert_eq!(
        kept.focus_request,
        Some(pending.clone()),
        "held repeat keeps the identical row; got {:?}",
        kept.focus_request
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor, None,
        "absent target cannot restore the cursor slot"
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-c".to_owned())),
        "newer report with unavailable pending target acts on live bound observed C"
    );
    // Queued implicit focus under the same held baseline resolves subject C
    // and hits the held scope gate (refusing operation-refused, not
    // focus-unknown) while retaining the identical pending row.
    let (still, focus_settlements) =
        run_batched(&mut engine, &absent(1), &[(2, implicit_focus_dir("left"))]);
    assert_eq!(focus_settlements.len(), 1);
    assert!(
        matches!(
            focus_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::ScopeHeld,
            })
        ),
        "held absent-target focus acts on observed C and hits scope hold; got {:?}",
        focus_settlements[0].result
    );
    assert_eq!(
        still.focus_request,
        Some(pending.clone()),
        "focus refusal retains the identical row; got {:?}",
        still.focus_request
    );
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-c".to_owned()))
    );
    // Queued implicit move likewise resolves subject C and hits the held
    // scope gate with the row retained.
    let (again, move_settlements) =
        run_batched(&mut engine, &absent(2), &[(3, implicit_move_dir("left"))]);
    assert_eq!(move_settlements.len(), 1);
    assert!(
        matches!(
            move_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::ScopeHeld,
            })
        ),
        "held absent-target move acts on observed C and hits scope hold; got {:?}",
        move_settlements[0].result
    );
    assert_eq!(
        again.focus_request,
        Some(pending.clone()),
        "move refusal retains the identical row; got {:?}",
        again.focus_request
    );
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-c".to_owned()))
    );
    // Recovery with N live on the unchanged C baseline: the returning
    // newcomer re-admits, superseding the explicit row with a fresh
    // admission row for the same target, and the subject restores to N.
    let (recovered, _) = run_batched(&mut engine, &observe(7, live.clone(), "win-c"), &[]);
    let readmitted = recovered.focus_request.clone().expect("recovery retains N");
    assert_eq!(readmitted.target.0, "win-n");
    assert_eq!(readmitted.cause, FocusRequestCause::AutomaticAdmission);
    assert_eq!(readmitted.id, "focus-7");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-n".to_owned())),
        "live N restores the subject on recovery"
    );
    // The observed leftward probe from N reaches C and supersedes.
    let (acted, act_settlements) = run_batched(
        &mut engine,
        &observe(8, live.clone(), "win-c"),
        &[(4, implicit_focus_dir("left"))],
    );
    assert_eq!(act_settlements.len(), 1);
    assert_not_rejected(&act_settlements[0].result, "recovery probe");
    let next = acted.focus_request.clone().expect("new decision");
    assert_eq!(next.target.0, "win-c", "observed leftward probe from N");
    // Target equality retires the superseding row.
    let retired = run_batched(&mut engine, &observe(9, live.clone(), "win-c"), &[]).0;
    assert_eq!(
        retired.focus_request, None,
        "target equality retires; got {:?}",
        retired.focus_request
    );
    // Re-mint N, then closing N retires via target loss.
    let (reminted, _) = run_batched(
        &mut engine,
        &report(10, live.clone(), "win-c", Some(5), Some(focus_cmd("win-n"))),
        &[],
    );
    let pending2 = reminted.focus_request.clone().expect("re-mint retains N");
    assert_eq!(pending2.target.0, "win-n");
    let mut close_n = report(
        11,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-a", 200)],
        "win-c",
        None,
        None,
    );
    close_n.closed = vec![WindowId("win-n".to_owned())];
    let closed = run_batched(&mut engine, &close_n, &[]).0;
    assert_eq!(
        closed.focus_request, None,
        "closing the pending target retires; got {:?}",
        closed.focus_request
    );
    // Re-mint N once more, then a genuine user focus change to live B
    // retires via baseline change and B owns the subject.
    let (reminted2, _) = run_batched(
        &mut engine,
        &report(12, live.clone(), "win-c", Some(6), Some(focus_cmd("win-n"))),
        &[],
    );
    assert_eq!(
        reminted2.focus_request.clone().expect("re-mint").target.0,
        "win-n"
    );
    let changed_user = run_batched(&mut engine, &observe(13, live.clone(), "win-b"), &[]).0;
    assert_eq!(
        changed_user.focus_request, None,
        "changed user focus retires; got {:?}",
        changed_user.focus_request
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-b".to_owned())),
        "observed B owns the subject after retirement"
    );
}

/// Option 2 across genuine reports and commands:
/// (a) same-batch implicit command after a held-scope focus mint refuses
///     focus-unknown (no newer report yet);
/// (b) newer report with unavailable N and live bound C acts on C with
///     an actual maximize effect while retaining the exact original N
///     focus request tuple;
/// (c) when N later returns live, the subject follows N and an implicit
///     command acts on N;
/// (e) when native focus reaches N, target equality satisfies and retires
///     the pending request and N owns the subject.
#[test]
fn unavailable_pending_same_batch_refuses_then_newer_report_acts_on_observed_and_recovers() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let live = vec![
        fact("win-b", "out-a", 0),
        fact("win-c", "out-a", 100),
        fact("win-n", "out-b", 800),
        fact("win-h", "out-b", 900),
    ];
    let mut r1 = observe(1, live.clone(), "win-c");
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    // (a) Report 2: out-b is held via win-h; explicit Focus(win-n) mints
    // pending_focus for win-n with cursor staying empty (held scope).
    // A same-batch implicit command has no newer report yet, so it refuses
    // focus-unknown.
    let held_h = HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 800),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_h.clone()];
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (minted, same_batch_settlements) = run_batched(&mut engine, &r2, &[(2, toggle())]);
    assert_eq!(same_batch_settlements.len(), 2);
    let trailing = same_batch_settlements
        .iter()
        .find(|settlement| settlement.seq == 2)
        .expect("trailing implicit settles");
    assert!(
        matches!(
            trailing.result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "same-batch implicit command after held focus mint must refuse focus-unknown; got {:?}",
        trailing.result
    );
    let pending = minted
        .focus_request
        .clone()
        .expect("held explicit focus retains");
    assert_eq!(pending.target.0, "win-n");
    assert_eq!(pending.issued_revision, 2);
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(pending.cmd_seq, Some(1));
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        None,
        "immediately after minting report with no newer report yet, subject remains None"
    );

    // (b) Report 3 (newer than request revision 2): win-n is now held-only
    // and unavailable on out-b, while live bound win-c is focused on
    // complete unheld out-a. Implicit ToggleMaximize acts on win-c AND
    // retains the exact original pending tuple for win-n.
    let held_n = HeldWindowRow {
        window: WindowId("win-n".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let held_h2 = HeldWindowRow {
        ordinal: 1,
        ..held_h.clone()
    };
    let mut r3 = observe(
        3,
        vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
        "win-c",
    );
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_n, held_h2];
    let expected_c_maximized = StateTarget {
        property: StateProperty::Maximized,
        value: true,
        authority: ScopeAuthority::Controlled,
        command_seq: Some(3),
        automatic_clear: false,
    };
    let (acted_c, c_settlements) = run_batched(&mut engine, &r3, &[(3, toggle())]);
    assert_eq!(c_settlements.len(), 1);
    assert_maximize_on(
        &c_settlements[0].result,
        "win-c",
        "newer report with unavailable pending N must maximize observed live bound C",
    );
    assert_eq!(
        acted_c.focus_request,
        Some(pending.clone()),
        "non-focus command on observed C must keep exact original pending tuple; got {:?}",
        acted_c.focus_request
    );
    assert!(
        acted_c
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "acted_c must publish win-c Maximized controlled state with command_seq Some(3); got {:?}",
        acted_c.windows
    );
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-c".to_owned()))
    );

    // (c) Report 4: N recovers live on out-b (still with native focus on
    // C). The subject follows N and an implicit maximize acts on N.
    let mut r4 = observe(4, live.clone(), "win-c");
    r4.screens = two_screens();
    let (recovered_n, n_settlements) = run_batched(&mut engine, &r4, &[(4, toggle())]);
    assert_eq!(n_settlements.len(), 1);
    assert_maximize_on(
        &n_settlements[0].result,
        "win-n",
        "when pending N returns live, implicit command must act on N",
    );
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-n".to_owned()))
    );
    assert_eq!(
        recovered_n.focus_request,
        Some(pending.clone()),
        "pending N remains active until focus arrives; got {:?}",
        recovered_n.focus_request
    );
    assert!(
        recovered_n
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "recovered_n must preserve win-c Maximized controlled state with command_seq Some(3); got {:?}",
        recovered_n.windows
    );

    // (e) Report 5: native focus arrives on N -> target equality
    // satisfies and retires pending_focus, and N owns the subject.
    let mut r5 = observe(5, live, "win-n");
    r5.screens = two_screens();
    r5.presentation = PresentationRowV4::Valid(PresentationIngressV4 {
        foreground_out: Some(crate::directional::OutputId("out-b".to_owned())),
        ..ingress("win-n")
    });
    let (completed, final_settlements) = run_batched(&mut engine, &r5, &[(5, toggle())]);
    assert_eq!(completed.focus_request, None, "target equality retires");
    assert_eq!(
        engine.tx(|tx| tx.execution_subject()),
        Some(WindowId("win-n".to_owned()))
    );
    assert_eq!(final_settlements.len(), 1);
    assert_maximize_on(
        &final_settlements[0].result,
        "win-n",
        "after normal completion on N, commands continue acting on N",
    );
    assert!(
        completed
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&expected_c_maximized)),
        "completed must preserve win-c Maximized controlled state with command_seq Some(3); got {:?}",
        completed.windows
    );
}

/// Newer accepted report with equal `observation_seq` and strictly higher
/// `request_seq` still counts as strictly newer than the minting report
/// (freshness follows accepted report/state revision order, not
/// `observation_seq` alone).
#[test]
fn unavailable_pending_newer_accepted_report_with_equal_observation_seq_acts_on_observed() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let live = vec![
        fact("win-b", "out-a", 0),
        fact("win-c", "out-a", 100),
        fact("win-n", "out-b", 800),
        fact("win-h", "out-b", 900),
    ];
    let mut r1 = observe(1, live, "win-c");
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 800),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_row("win-h", 0)];
    let (minted, _) = run_batched(&mut engine, &r2, &[]);
    let pending = minted.focus_request.clone().expect("pending N retained");
    assert_eq!(pending.issued_revision, 2);

    // Report 3 keeps observation_seq == 2 (same as r2) while advancing
    // request_seq to 3 -> accepted at revision 3 > pending.issued_revision (2).
    let mut r3 = observe(
        3,
        vec![fact("win-b", "out-a", 0), fact("win-c", "out-a", 100)],
        "win-c",
    );
    r3.observation_seq = 2;
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (acted_c, c_settlements) = run_batched(&mut engine, &r3, &[(2, toggle)]);
    assert_eq!(c_settlements.len(), 1);
    assert_maximize_on(
        &c_settlements[0].result,
        "win-c",
        "equal observation_seq with higher request_seq is newer report and must act on C",
    );
    assert_eq!(acted_c.focus_request, Some(pending));
    assert!(
        acted_c
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-c")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(2),
                automatic_clear: false,
            })),
        "acted_c must publish win-c Maximized controlled state with command_seq Some(2); got {:?}",
        acted_c.windows
    );
}

/// (a) Same-batch refusal also holds when an older unavailable pending
/// request exists from a prior report and a fresh Focus command in the
/// current batch leaves the cursor empty: the same-batch implicit command
/// must refuse focus-unknown rather than falling back via the older row.
#[test]
fn same_batch_focus_decision_blocks_fallback_even_with_older_unavailable_pending() {
    let mut engine = engine();
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let mut r1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-x", "out-b", 800),
            fact("win-n", "out-b", 900),
            fact("win-h", "out-b", 950),
        ],
        "win-c",
    );
    r1.screens = two_screens();
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    // Report 2: mint pending_focus on win-x while out-b is held.
    let mut r2 = report(
        2,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-x", "out-b", 800),
            fact("win-n", "out-b", 900),
        ],
        "win-c",
        Some(1),
        Some(focus_cmd("win-x")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![held_row("win-h", 0)];
    let (minted_x, _) = run_batched(&mut engine, &r2, &[]);
    assert_eq!(
        minted_x.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-x")
    );

    // Report 3 (newer than win-x): win-x is unavailable, out-b is still
    // held, and Command 1 explicitly focuses live win-n (leaving cursor
    // empty because out-b is held). Command 2 (same batch) is Implicit:
    // must refuse FocusUnknown, never fall back to observed win-c.
    let mut r3 = report(
        3,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-n", "out-b", 900),
        ],
        "win-c",
        Some(2),
        Some(focus_cmd("win-n")),
    );
    r3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r3.held_windows = vec![held_row("win-x", 0), held_row("win-h", 1)];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (minted_n, settlements) = run_batched(&mut engine, &r3, &[(3, toggle)]);
    assert_eq!(settlements.len(), 2);
    let trailing = settlements
        .iter()
        .find(|settlement| settlement.seq == 3)
        .expect("trailing implicit settles");
    assert!(
        matches!(
            trailing.result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "same-batch implicit after Focus(win-n) must refuse focus-unknown; got {:?}",
        trailing.result
    );
    assert_eq!(
        minted_n.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-n")
    );
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(engine.tx(|tx| tx.execution_subject()), None);
}

/// (d) Newer report with unavailable pending target and unknown or
/// unbound/foreign foreground must refuse focus-unknown while keeping
/// the pending request retained.
#[test]
fn unavailable_pending_newer_unknown_or_unbound_foreground_refuses() {
    let held_row = |id: &str, ordinal: usize| HeldWindowRow {
        window: WindowId(id.to_owned()),
        ordinal,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };

    // Case 1: unknown empty foreground (foreground_known = false).
    let mut engine_unknown = engine();
    let mut u1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-b",
    );
    u1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_unknown
        .handle_report_v4_for_test(&u1)
        .expect("admission observes");
    let mut u2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    u2.foreground_known = false;
    u2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    u2.held_windows = vec![held_row("win-h", 0)];
    let (u_minted, _) = run_batched(&mut engine_unknown, &u2, &[]);
    let u_pending = u_minted.focus_request.clone().expect("pending N retained");
    let mut u3 = observe(3, vec![fact("win-b", "out-a", 0)], "");
    u3.foreground_known = false;
    u3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    u3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let (u_after, u_settlements) = run_batched(&mut engine_unknown, &u3, &[(2, toggle())]);
    assert_eq!(u_settlements.len(), 1);
    assert!(
        matches!(
            u_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unknown foreground must refuse focus-unknown; got {:?}",
        u_settlements[0].result
    );
    assert_eq!(u_after.focus_request, Some(u_pending));
    assert_eq!(engine_unknown.tx(|tx| tx.execution_subject()), None);

    // Case 2: unbound/foreign foreground (not present in windows).
    let mut engine_foreign = engine();
    let mut f1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-foreign",
    );
    f1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_foreign
        .handle_report_v4_for_test(&f1)
        .expect("admission observes");
    let mut f2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "win-foreign",
        Some(1),
        Some(focus_cmd("win-n")),
    );
    f2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    f2.held_windows = vec![held_row("win-h", 0)];
    let (f_minted, _) = run_batched(&mut engine_foreign, &f2, &[]);
    let f_pending = f_minted.focus_request.clone().expect("pending N retained");
    let mut f3 = observe(3, vec![fact("win-b", "out-a", 0)], "win-foreign");
    f3.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    f3.held_windows = vec![held_row("win-n", 0), held_row("win-h", 1)];
    let (f_after, f_settlements) = run_batched(&mut engine_foreign, &f3, &[(2, toggle())]);
    assert_eq!(f_settlements.len(), 1);
    assert!(
        matches!(
            f_settlements[0].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "unbound/foreign foreground must refuse focus-unknown; got {:?}",
        f_settlements[0].result
    );
    assert_eq!(f_after.focus_request, Some(f_pending));
    assert_eq!(engine_foreign.tx(|tx| tx.execution_subject()), None);
}

/// Non-`record_command_focus` focus-effect carriers (`settle_send` follow/stay
/// and `move_directional`) must not wipe a valid prior `logical_focus_cursor`
/// when returning an `Effect(Focus)` with a different target.
#[test]
fn send_follow_effect_preserves_command_focus_cursor() {
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let mut engine_follow = engine();
    let mut r1 = observe(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
    );
    r1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_follow
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");
    let mut r2 = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let send_follow = WorldCommand::SendRelative {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        source: SendSource::Explicit(OutputId("out-a".to_owned())),
        delta: 1,
        follow: true,
    };
    let (reply_follow, follow_settlements) =
        run_batched(&mut engine_follow, &r2, &[(2, send_follow), (3, toggle())]);
    assert_eq!(follow_settlements.len(), 3);
    assert_not_rejected(&follow_settlements[1].result, "SendRelative(follow=true)");
    assert_maximize_on(
        &follow_settlements[2].result,
        "win-b",
        "SendRelative(follow=true) effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_follow
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
                automatic_clear: false,
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_follow.windows
    );
    assert_eq!(
        reply_follow
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-a"),
        "post-batch drain_focus_candidates retains SendRelative's explicit focus winner"
    );
    assert_eq!(
        engine_follow.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned()))
    );
}

#[test]
fn send_stay_effect_preserves_command_focus_cursor() {
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let mut engine_stay = engine();
    let mut r1_stay = observe(
        1,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
    );
    r1_stay.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_stay
        .handle_report_v4_for_test(&r1_stay)
        .expect("admission observes");
    let mut r2_stay = report(
        2,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2_stay.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let send_stay = WorldCommand::SendRelative {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        source: SendSource::Explicit(OutputId("out-a".to_owned())),
        delta: 1,
        follow: false,
    };
    let (reply_stay, stay_settlements) =
        run_batched(&mut engine_stay, &r2_stay, &[(2, send_stay), (3, toggle())]);
    assert_eq!(stay_settlements.len(), 3);
    assert_not_rejected(&stay_settlements[1].result, "SendRelative(follow=false)");
    assert_maximize_on(
        &stay_settlements[2].result,
        "win-b",
        "SendRelative(follow=false) effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_stay
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
                automatic_clear: false,
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_stay.windows
    );
    assert_eq!(
        reply_stay
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-c"),
        "post-batch drain_focus_candidates retains SendRelative's source-survivor winner"
    );
    assert_eq!(
        engine_stay.world.focus.logical_focus_cursor,
        Some(WindowId("win-c".to_owned()))
    );
}

#[test]
fn move_directional_effect_preserves_command_focus_cursor() {
    let toggle = || WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let mut engine_move = engine();
    let mut m1 = observe(
        1,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
    );
    m1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine_move
        .handle_report_v4_for_test(&m1)
        .expect("admission observes");
    let mut m2 = report(
        2,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-c", "out-a", 100),
            fact("win-b", "out-b", 800),
        ],
        "win-a",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    m2.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let move_cmd = WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: crate::directional::Direction::Right,
        same_axis: SameAxisMove::SwapWithNeighbor,
    };
    let (reply_move, move_settlements) =
        run_batched(&mut engine_move, &m2, &[(2, move_cmd), (3, toggle())]);
    assert_eq!(move_settlements.len(), 3);
    assert_not_rejected(&move_settlements[1].result, "MoveDirectional");
    assert_maximize_on(
        &move_settlements[2].result,
        "win-b",
        "MoveDirectional effect must not clear prior Focus(win-b) cursor",
    );
    assert!(
        reply_move
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-b")
            .is_some_and(|row| row.state.contains(&StateTarget {
                property: StateProperty::Maximized,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: Some(3),
                automatic_clear: false,
            })),
        "trailing implicit maximize must publish win-b Maximized; got {:?}",
        reply_move.windows
    );
    assert_eq!(
        reply_move
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-a"),
        "post-batch drain_focus_candidates retains MoveDirectional's mover focus winner"
    );
    assert_eq!(
        engine_move.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned()))
    );
}

/// Failed canonical command focus (`record_command_focus` returning false
/// because the target scope is held) still clears a valid prior
/// `logical_focus_cursor`, so a trailing same-batch implicit command refuses
/// `FocusUnknown` instead of acting on the stale cursor.
#[test]
fn failed_record_command_focus_clears_prior_cursor() {
    let mut engine = engine();
    let mut r1 = observe(
        1,
        vec![
            fact("win-b", "out-a", 0),
            fact("win-n", "out-b", 800),
            fact("win-h", "out-b", 900),
        ],
        "win-b",
    );
    r1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&r1)
        .expect("admission observes");

    let mut r2 = report(
        2,
        vec![fact("win-b", "out-a", 0), fact("win-n", "out-b", 800)],
        "win-b",
        Some(1),
        Some(focus_cmd("win-b")),
    );
    r2.screens = vec![
        screen("out-a", 0),
        crate::world_report::ScreenReport {
            complete: false,
            ..screen("out-b", 800)
        },
    ];
    r2.held_windows = vec![HeldWindowRow {
        window: WindowId("win-h".to_owned()),
        ordinal: 0,
        output: None,
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let toggle = WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    };
    let (reply, settlements) =
        run_batched(&mut engine, &r2, &[(2, focus_cmd("win-n")), (3, toggle)]);
    assert_eq!(settlements.len(), 3);
    assert!(
        matches!(
            settlements[2].result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown,
            })
        ),
        "failed record_command_focus must clear prior cursor so trailing implicit refuses; got {:?}",
        settlements[2].result
    );
    assert_eq!(
        reply
            .focus_request
            .as_ref()
            .map(|req| req.target.0.as_str()),
        Some("win-n")
    );
    assert_eq!(engine.world.focus.logical_focus_cursor, None);
    assert_eq!(engine.tx(|tx| tx.execution_subject()), None);
}

fn engine_with_hover() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
        vec![("focusFollowsCursor".to_owned(), RawSettingValue::Bool(true))],
    )
    .expect("valid generation")
}

/// Admit `win-a`, `win-c`, then focus `win-a` and admit `win-b` on 800x560
/// so the tiled tree is `H[V[win-a, win-b], win-c]`.
fn admit_h_vab_c(engine: &mut WorldEngine) -> u64 {
    let wa = fact("win-a", "out-a", 0);
    let wb = fact("win-b", "out-a", 100);
    let wc = fact("win-c", "out-a", 200);
    engine
        .handle_report_v4_for_test(&observe(1, vec![wa.clone()], "win-a"))
        .expect("admit A");
    engine
        .handle_report_v4_for_test(&observe(2, vec![wa.clone(), wc.clone()], "win-a"))
        .expect("admit C");
    engine
        .handle_report_v4_for_test(&observe(3, vec![wa, wb, wc], "win-a"))
        .expect("admit B under A");
    let key = engine.world.layout.domain_keys()[0].clone();
    let tree = engine.world.layout.tree(&key).expect("tree");
    match tree {
        crate::directional::Node::Group { axis, children, .. } => {
            assert_eq!(*axis, crate::directional::Axis::Horizontal);
            assert_eq!(children.len(), 2);
            match (&children[0], &children[1]) {
                (
                    crate::directional::Node::Group {
                        axis: inner_axis,
                        children: inner_children,
                        ..
                    },
                    crate::directional::Node::Leaf { id: c_id },
                ) => {
                    assert_eq!(*inner_axis, crate::directional::Axis::Vertical);
                    assert_eq!(inner_children.len(), 2);
                    let leaf_c = engine
                        .world
                        .layout
                        .leaves(&key)
                        .and_then(|leaves| leaves.get(&WindowId("win-c".to_owned())))
                        .expect("win-c leaf");
                    assert_eq!(c_id, leaf_c);
                }
                other => panic!("expected H[V[A,B],C], got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
    3
}

#[test]
fn perpendicular_descent_both_histories_abc_vs_bac() {
    let all = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    // History A, B, C -> Left from C picks B.
    let mut eng_abc = engine();
    let mut seq = admit_h_vab_c(&mut eng_abc);
    for fg in ["win-a", "win-b", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng_abc, &observe(seq, all(), fg), &[]);
    }
    seq += 1;
    let (reply_abc, settlements) = run_batched(
        &mut eng_abc,
        &observe(seq, all(), "win-c"),
        &[(
            1,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-c".to_owned(),
                )),
                direction: crate::directional::Direction::Left,
            },
        )],
    );
    assert_not_rejected(&settlements[0].result, "focus left abc");
    assert_eq!(
        reply_abc
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-b"),
        "H[V[A,B],C*] with A,B,C history must descend left to B"
    );

    // Fresh history B, A, C -> Left from C picks A.
    let mut eng_bac = engine();
    let mut seq = admit_h_vab_c(&mut eng_bac);
    for fg in ["win-b", "win-a", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng_bac, &observe(seq, all(), fg), &[]);
    }
    seq += 1;
    let (reply_bac, settlements) = run_batched(
        &mut eng_bac,
        &observe(seq, all(), "win-c"),
        &[(
            1,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-c".to_owned(),
                )),
                direction: crate::directional::Direction::Left,
            },
        )],
    );
    assert_not_rejected(&settlements[0].result, "focus left bac");
    assert_eq!(
        reply_bac
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-a"),
        "H[V[A,B],C*] with B,A,C history must descend left to A"
    );
}

#[test]
fn perpendicular_descent_no_history_fallback_and_nested() {
    use crate::directional::{Axis, Direction, FocusPlan, Node, NodeId};

    // No-history fallback in pure plan_focus:
    // H[V[A, B], C*] with no history -> Left from C falls back to stable first child A.
    let tree = Node::Group {
        id: NodeId::from("root"),
        axis: Axis::Horizontal,
        shares: vec![1, 1],
        children: vec![
            Node::Group {
                id: NodeId::from("inner"),
                axis: Axis::Vertical,
                shares: vec![1, 1],
                children: vec![
                    Node::Leaf {
                        id: NodeId::from("leaf-a"),
                    },
                    Node::Leaf {
                        id: NodeId::from("leaf-b"),
                    },
                ],
            },
            Node::Leaf {
                id: NodeId::from("leaf-c"),
            },
        ],
    };
    assert_eq!(
        crate::directional::plan_focus(&tree, &NodeId::from("leaf-c"), Direction::Left),
        Some(FocusPlan::Focused {
            leaf: NodeId::from("leaf-a"),
            route: vec![NodeId::from("inner"), NodeId::from("leaf-a")],
        })
    );

    // Nested WorldEngine scenario: H[V[H2[win-a, win-d], win-b], win-c]
    // (win-a is 400x280, so admitting win-d under win-a splits horizontally).
    let mut eng = engine();
    let mut seq = admit_h_vab_c(&mut eng);
    let all_4 = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
        ]
    };
    seq += 1;
    let _ = run_batched(&mut eng, &observe(seq, all_4(), "win-a"), &[]);
    // History: win-b, win-a, win-c -> Left from win-c enters V (perpendicular,
    // picks H2[win-a, win-d] because win-a is more recent than win-b), then
    // H2 is parallel Left so it stays on bordering edge win-d!
    for fg in ["win-b", "win-a", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all_4(), fg), &[]);
    }
    seq += 1;
    let (reply, settlements) = run_batched(
        &mut eng,
        &observe(seq, all_4(), "win-c"),
        &[(
            1,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-c".to_owned(),
                )),
                direction: crate::directional::Direction::Left,
            },
        )],
    );
    assert_not_rejected(&settlements[0].result, "nested focus left");
    assert_eq!(
        reply.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-d"),
        "perpendicular V picks H2 (MRU win-a), parallel H2 Left stays on bordering edge win-d"
    );

    // Flip MRU inside V to win-b -> Left from win-c now descends to win-b.
    for fg in ["win-a", "win-b", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all_4(), fg), &[]);
    }
    seq += 1;
    let (reply_b, settlements_b) = run_batched(
        &mut eng,
        &observe(seq, all_4(), "win-c"),
        &[(
            2,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-c".to_owned(),
                )),
                direction: crate::directional::Direction::Left,
            },
        )],
    );
    assert_not_rejected(&settlements_b[0].result, "nested focus left flip");
    assert_eq!(
        reply_b.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-b"),
        "flipping MRU in V to win-b must descend left to win-b"
    );

    // Perpendicular-into-perpendicular in WorldEngine:
    // Admitting win-e under win-b (400x280) splits horizontally to H3[win-b, win-e].
    // Down from win-d (in H2) enters V's next child H3[win-b, win-e] (perpendicular
    // to Down) and selects MRU between win-b and win-e.
    let all_5 = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
            fact("win-e", "out-a", 400),
        ]
    };
    seq += 1;
    let _ = run_batched(&mut eng, &observe(seq, all_5(), "win-b"), &[]);
    // History: win-b then win-e then win-d -> Down from win-d enters H3[win-b, win-e]
    // (perpendicular to Down) and must pick MRU win-e (not first child win-b).
    for fg in ["win-b", "win-e", "win-d"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all_5(), fg), &[]);
    }
    seq += 1;
    let (reply_perp, settlements_perp) = run_batched(
        &mut eng,
        &observe(seq, all_5(), "win-d"),
        &[(
            3,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-d".to_owned(),
                )),
                direction: crate::directional::Direction::Down,
            },
        )],
    );
    assert_not_rejected(&settlements_perp[0].result, "nested perp-into-perp down");
    assert_eq!(
        reply_perp
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-e"),
        "perpendicular-into-perpendicular Down from win-d into H3[win-b, win-e] must pick MRU win-e"
    );
}

#[test]
fn perpendicular_descent_explicit_focus_command_and_rejected_observation() {
    let all = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    let mut eng = engine();
    let mut seq = admit_h_vab_c(&mut eng);
    for fg in ["win-a", "win-b", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all(), fg), &[]);
    }

    // Rejected raw observation claiming win-a focused must NOT update history.
    let mut bad = observe(seq + 1, all(), "win-a");
    bad.core_generation = "stale-gen".to_owned();
    assert!(
        eng.handle_report_v4_for_test(&bad).is_err(),
        "stale generation raw observation must reject"
    );

    // Explicit Focus(win-a) followed by Focus(win-c) updates history so Left
    // from win-c now picks win-a.
    seq += 1;
    let (reply, settlements) = run_batched(
        &mut eng,
        &observe(seq, all(), "win-c"),
        &[
            (1, focus_cmd("win-a")),
            (2, focus_cmd("win-c")),
            (
                3,
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                        "win-c".to_owned(),
                    )),
                    direction: crate::directional::Direction::Left,
                },
            ),
        ],
    );
    assert_eq!(settlements.len(), 3);
    assert_not_rejected(&settlements[2].result, "focus left after explicit Focus");
    assert_eq!(
        reply.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-a"),
        "explicit Focus(win-a) must update perpendicular descent history"
    );
}

#[test]
fn perpendicular_descent_accepted_hover_focus_updates_history() {
    let all = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    let mut eng = engine_with_hover();
    let mut seq = admit_h_vab_c(&mut eng);
    for fg in ["win-a", "win-b", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all(), fg), &[]);
    }

    // Hover over win-a while win-c is still focused -> mints FocusFollowsCursor.
    seq += 1;
    let mut hover_report = observe(seq, all(), "win-c");
    hover_report.cursor = Some((10, 10));
    hover_report.cursor_hit = Some(WindowId("win-a".to_owned()));
    let (hover_reply, _) = run_batched(&mut eng, &hover_report, &[]);
    let hover_req = hover_reply.focus_request.expect("hover focus request");
    assert_eq!(hover_req.target.0, "win-a");
    assert_eq!(hover_req.cause, FocusRequestCause::FocusFollowsCursor);
    assert_eq!(
        eng.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned())),
        "accepted logical hover must advance logical_focus_cursor before host actuation"
    );

    // BEFORE host confirms hover (focused_global is STILL "win-c"), refocus
    // win-c and descend Left -> must already select win-a (not win-b)!
    seq += 1;
    let (pre_confirm_reply, pre_confirm_settlements) = run_batched(
        &mut eng,
        &observe(seq, all(), "win-c"),
        &[
            (1, focus_cmd("win-c")),
            (
                2,
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                        "win-c".to_owned(),
                    )),
                    direction: crate::directional::Direction::Left,
                },
            ),
        ],
    );
    assert_eq!(pre_confirm_settlements.len(), 2);
    assert_not_rejected(
        &pre_confirm_settlements[1].result,
        "focus left after accepted hover before host confirmation",
    );
    assert_eq!(
        pre_confirm_reply
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-a"),
        "accepted hover focus must update perpendicular descent history BEFORE host confirms hover"
    );
}

#[test]
fn perpendicular_descent_accepted_hover_not_overwritten_by_lagging_foreground() {
    let all_4 = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
        ]
    };
    let mut eng = engine_with_hover();
    let mut seq = admit_h_vab_c(&mut eng);
    // Admit win-d under win-c -> H[V[win-a, win-b], V2[win-c, win-d]].
    seq += 1;
    let _ = run_batched(&mut eng, &observe(seq, all_4(), "win-c"), &[]);
    // History: win-a, win-b, win-d, win-c (win-c is newest in V2, native fg = win-c).
    for fg in ["win-a", "win-b", "win-d", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all_4(), fg), &[]);
    }

    // Hover over win-d while native foreground is still win-c -> mints FocusFollowsCursor.
    seq += 1;
    let mut hover_report = observe(seq, all_4(), "win-c");
    hover_report.cursor = Some((500, 400));
    hover_report.cursor_hit = Some(WindowId("win-d".to_owned()));
    let (hover_reply, _) = run_batched(&mut eng, &hover_report, &[]);
    let hover_req = hover_reply.focus_request.expect("hover focus request");
    assert_eq!(hover_req.target.0, "win-d");
    assert_eq!(hover_req.cause, FocusRequestCause::FocusFollowsCursor);

    // Next report STILL carries lagging native focused_global = "win-c" (hover
    // not yet confirmed by host): Focus(win-a) + FocusDirectional(win-a, right)
    // enters V2[win-c, win-d] and must select win-d (not stale lagging win-c).
    seq += 1;
    let (reply, settlements) = run_batched(
        &mut eng,
        &observe(seq, all_4(), "win-c"),
        &[
            (1, focus_cmd("win-a")),
            (
                2,
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    direction: crate::directional::Direction::Right,
                },
            ),
        ],
    );
    assert_eq!(settlements.len(), 2);
    assert_not_rejected(
        &settlements[1].result,
        "focus right after unconfirmed hover",
    );
    assert_eq!(
        reply.focus_request.as_ref().map(|r| r.target.0.as_str()),
        Some("win-d"),
        "lagging native foreground must not overwrite accepted hover in perpendicular descent history"
    );
}

#[test]
fn perpendicular_descent_closed_mru_falls_through_to_next_candidate() {
    use crate::directional::{Direction, WorkspaceId};
    use crate::geometry::Rect;
    use crate::seed::EngineWindow;
    use crate::session::{ObservedWindow, OutputDomain};
    use crate::world_layout::WorldLayout;

    // H[V[A, H2[B, D]], C*]: focus A, B, D, C, then close D -> tree collapses
    // to H[V[A, B], C*], MRU in V is B (not first child A).
    let domain = OutputDomain {
        id: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        gap: 0,
        adjacent: std::collections::BTreeMap::new(),
    };
    let key = domain.key();
    let mut layout = WorldLayout::new();
    let make_engine_obs = |wins: &[&str]| -> Vec<EngineWindow> {
        wins.iter()
            .enumerate()
            .map(|(i, id)| EngineWindow {
                window: WindowId((*id).to_owned()),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                rect: Rect {
                    x: (i as i32) * 100,
                    y: 0,
                    w: 100,
                    h: 560,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect()
    };
    let make_obs = |wins: &[&str]| -> Vec<ObservedWindow> {
        wins.iter()
            .map(|id| ObservedWindow {
                window: WindowId((*id).to_owned()),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect()
    };
    layout
        .reconcile_fresh(
            &domain,
            0,
            &WindowId("win-a".to_owned()),
            &make_engine_obs(&["win-a", "win-c"]),
        )
        .unwrap();
    layout
        .converge_domain(
            &key,
            &make_obs(&["win-a", "win-b", "win-c"]),
            Some(&WindowId("win-b".to_owned())),
            &std::collections::BTreeSet::new(),
        )
        .unwrap();
    layout
        .converge_domain(
            &key,
            &make_obs(&["win-a", "win-b", "win-c", "win-d"]),
            Some(&WindowId("win-d".to_owned())),
            &std::collections::BTreeSet::new(),
        )
        .unwrap();
    for w in ["win-a", "win-b", "win-d", "win-c"] {
        assert!(layout.remember_focus(&key, &WindowId(w.to_owned())));
    }
    let obs_after_close = make_obs(&["win-a", "win-b", "win-c"]);
    layout
        .converge_domain(
            &key,
            &obs_after_close,
            Some(&WindowId("win-c".to_owned())),
            &std::collections::BTreeSet::new(),
        )
        .unwrap();
    let outcome = layout
        .local_focus(
            &key,
            &WindowId("win-c".to_owned()),
            Direction::Left,
            &WindowId("win-c".to_owned()),
            &obs_after_close,
        )
        .expect("local_focus");
    let b_leaf = layout
        .bindings(&key)
        .find(|(w, _)| w.0 == "win-b")
        .unwrap()
        .1
        .clone();
    assert_eq!(
        outcome.to_leaf, b_leaf,
        "closed MRU D must fall through to next MRU candidate B, not first child A"
    );

    // WorldEngine closed/floated/moved MRU pruning at selection:
    // H[V[win-a, H2[win-b, win-d]], win-c]
    let all_4 = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
        ]
    };
    let mut eng = engine();
    let mut seq = admit_h_vab_c(&mut eng);
    // Focus win-b before admitting win-d so H2[win-b, win-d] is under V's 2nd child.
    seq += 1;
    let _ = run_batched(
        &mut eng,
        &observe(
            seq,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
        ),
        &[],
    );
    seq += 1;
    let _ = run_batched(&mut eng, &observe(seq, all_4(), "win-d"), &[]);
    // Float win-d -> pruned from tiled history, Left from win-c falls through to win-b (not win-a).
    for fg in ["win-a", "win-b", "win-d", "win-c"] {
        seq += 1;
        let _ = run_batched(&mut eng, &observe(seq, all_4(), fg), &[]);
    }
    seq += 1;
    let (float_reply, float_settlements) = run_batched(
        &mut eng,
        &observe(seq, all_4(), "win-c"),
        &[
            (
                1,
                WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                    window: crate::directional::WindowSubject::Window(
                        crate::directional::WindowId("win-d".to_owned()),
                    ),
                    float_rect: None,
                }),
            ),
            (
                2,
                WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                        "win-c".to_owned(),
                    )),
                    direction: crate::directional::Direction::Left,
                },
            ),
        ],
    );
    assert_not_rejected(&float_settlements[1].result, "focus left after float MRU");
    assert_eq!(
        float_reply
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-b"),
        "floated MRU win-d must be pruned so Left falls through to next MRU win-b"
    );

    // Send win-b to out-b (left of out-a) -> moved MRU win-b is pruned and Left
    // from win-c falls through to win-a.
    seq += 1;
    let mut two_out = observe(
        seq,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
            fact("win-e", "out-b", -800),
        ],
        "win-c",
    );
    two_out.screens = vec![screen("out-a", 0), screen("out-b", -800)];
    let _ = run_batched(&mut eng, &two_out, &[]);
    seq += 1;
    let mut send_rep = observe(
        seq,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
            fact("win-e", "out-b", -800),
        ],
        "win-b",
    );
    send_rep.screens = vec![screen("out-a", 0), screen("out-b", -800)];
    let (_, send_settlements) = run_batched(
        &mut eng,
        &send_rep,
        &[(
            3,
            WorldCommand::SendDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
                direction: crate::directional::Direction::Left,
                follow: false,
            },
        )],
    );
    assert_not_rejected(&send_settlements[0].result, "send win-b to out-b");
    seq += 1;
    let mut after_send = observe(
        seq,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-b", -700),
            fact("win-c", "out-a", 200),
            fact("win-d", "out-a", 300),
            fact("win-e", "out-b", -800),
        ],
        "win-c",
    );
    after_send.screens = vec![screen("out-a", 0), screen("out-b", -800)];
    let (move_reply, move_settlements) = run_batched(
        &mut eng,
        &after_send,
        &[(
            4,
            WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-c".to_owned(),
                )),
                direction: crate::directional::Direction::Left,
            },
        )],
    );
    assert_not_rejected(&move_settlements[0].result, "focus left after moved MRU");
    assert_eq!(
        move_reply
            .focus_request
            .as_ref()
            .map(|r| r.target.0.as_str()),
        Some("win-a"),
        "moved MRU win-b must be pruned so Left falls through to remaining MRU win-a"
    );
}

#[test]
fn perpendicular_descent_leaves_move_and_send_unchanged() {
    let all = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    let run_move = |order: [&str; 3]| {
        let mut eng = engine();
        let mut seq = admit_h_vab_c(&mut eng);
        for fg in order {
            seq += 1;
            let _ = run_batched(&mut eng, &observe(seq, all(), fg), &[]);
        }
        seq += 1;
        let (reply, settlements) = run_batched(
            &mut eng,
            &observe(seq, all(), "win-c"),
            &[(
                1,
                WorldCommand::MoveDirectional {
                    window: crate::directional::WindowSubject::Window(WindowId("win-c".to_owned())),
                    direction: crate::directional::Direction::Left,
                    same_axis: SameAxisMove::GroupWithNeighbor,
                },
            )],
        );
        assert_not_rejected(&settlements[0].result, "move left");
        reply
            .windows
            .iter()
            .map(|w| (w.lifetime.0.clone(), w.allocation))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        run_move(["win-a", "win-b", "win-c"]),
        run_move(["win-b", "win-a", "win-c"]),
        "MoveDirectional(GroupWithNeighbor) must be completely unaffected by perpendicular focus history"
    );

    let run_move_swap = |order: [&str; 3]| {
        let mut eng = engine();
        let mut seq = admit_h_vab_c(&mut eng);
        for fg in order {
            seq += 1;
            let _ = run_batched(&mut eng, &observe(seq, all(), fg), &[]);
        }
        seq += 1;
        let (reply, settlements) = run_batched(
            &mut eng,
            &observe(seq, all(), "win-c"),
            &[(
                1,
                WorldCommand::MoveDirectional {
                    window: crate::directional::WindowSubject::Window(WindowId("win-c".to_owned())),
                    direction: crate::directional::Direction::Left,
                    same_axis: SameAxisMove::SwapWithNeighbor,
                },
            )],
        );
        assert_not_rejected(&settlements[0].result, "move swap left");
        (
            reply
                .windows
                .iter()
                .map(|w| (w.lifetime.0.clone(), w.allocation))
                .collect::<Vec<_>>(),
            reply.focus_request.map(|r| r.target),
        )
    };
    assert_eq!(
        run_move_swap(["win-a", "win-b", "win-c"]),
        run_move_swap(["win-b", "win-a", "win-c"]),
        "MoveDirectional(SwapWithNeighbor) must be completely unaffected by perpendicular focus history"
    );

    let run_send = |order: [&str; 3]| {
        let mut eng = engine();
        let mut seq = admit_h_vab_c(&mut eng);
        let mut four = all();
        four.push(fact("win-d", "out-b", 800));
        let mut rep = observe(seq + 1, four.clone(), "win-c");
        rep.screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let _ = run_batched(&mut eng, &rep, &[]);
        seq += 1;
        for fg in order {
            seq += 1;
            let mut r = observe(seq, four.clone(), fg);
            r.screens = vec![screen("out-a", 0), screen("out-b", 800)];
            let _ = run_batched(&mut eng, &r, &[]);
        }
        seq += 1;
        let mut send_rep = observe(seq, four, "win-d");
        send_rep.screens = vec![screen("out-a", 0), screen("out-b", 800)];
        let (reply, settlements) = run_batched(
            &mut eng,
            &send_rep,
            &[(
                1,
                WorldCommand::SendDirectional {
                    window: crate::directional::WindowSubject::Window(WindowId("win-d".to_owned())),
                    direction: crate::directional::Direction::Left,
                    follow: false,
                },
            )],
        );
        assert_not_rejected(&settlements[0].result, "send left");
        reply
            .windows
            .iter()
            .map(|w| (w.lifetime.0.clone(), w.allocation))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run_send(["win-a", "win-b", "win-c"]),
        run_send(["win-b", "win-a", "win-c"]),
        "SendRelative anchoring must be completely unaffected by perpendicular focus history"
    );
}

#[test]
fn detached_cursor_guard_is_output_scoped_and_uses_fresh_foreground() {
    use crate::world_report::{NativeObservation, NativeOccupancy, NativeWorkspaceFact, ViewRef};

    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            detached_fullscreen_view: true,
            ..HostCapabilities::default()
        },
        Vec::new(),
    )
    .expect("valid host capability");
    let a = OutputId("out-a".into());
    let b = OutputId("out-b".into());
    let native = |output: &OutputId, id: &str| NativeObservation {
        output: output.clone(),
        native_order: vec![id.into()],
        current_native: Some(id.into()),
        occupancy: vec![NativeOccupancy { native: id.into(), count: None }],
        current_known: true,
        detached_current: None,
    };
    let a_fact = WindowFact {
        native_workspace: NativeWorkspaceFact::Value("desktop-a".into()),
        ..fact("win-a", "out-a", 0)
    };
    let b_fact = WindowFact {
        native_workspace: NativeWorkspaceFact::Value("desktop-b".into()),
        ..fact("win-b", "out-b", 800)
    };
    let capture = |seq, focused: &str| {
        let mut report = observe(seq, vec![a_fact.clone(), b_fact.clone()], focused);
        report.screens.push(screen("out-b", 800));
        report.native = vec![native(&a, "desktop-a"), native(&b, "desktop-b")];
        report
    };
    let accepted = crate::world_v4_support_tests::accepted;
    accepted(&mut engine, &capture(1, "win-a"));
    accepted(&mut engine, &capture(2, "win-a"));
    engine.tx(|tx| {
        assert_eq!(tx.execution_subject(), Some(a_fact.window.clone()));
        tx.replace_detached_cursor_guards(std::slice::from_ref(&a), true);
        tx.refresh_detached_cursor_guards(true);
        assert_eq!(tx.execution_subject(), None, "grant capture cannot seed the cursor");
    });

    accepted(&mut engine, &capture(3, "win-b"));
    engine.tx(|tx| {
        assert_eq!(tx.execution_subject(), Some(b_fact.window.clone()));
        assert!(tx.world.focus.detached_cursor_guards.contains_key(&a));
        tx.replace_detached_cursor_guards(std::slice::from_ref(&b), true);
        tx.note_detached_cursor_refusal(&b, &ViewRef::Native {
            output: b.clone(),
            workspace: crate::ids::NativeWorkspaceId::parse("desktop-b").unwrap(),
        });
    });
    let mut gap = capture(4, "");
    gap.foreground_known = true;
    gap.native[1].current_known = false;
    gap.native[1].current_native = None;
    accepted(&mut engine, &gap);
    engine.tx(|tx| {
        assert!(tx.world.focus.detached_cursor_guards.contains_key(&a));
        assert!(tx.world.focus.detached_cursor_guards.contains_key(&b));
        assert_eq!(tx.execution_subject(), None);
    });

    let mut empty = capture(5, "");
    empty.foreground_known = true;
    accepted(&mut engine, &empty);
    engine.tx(|tx| {
        assert!(tx.world.focus.detached_cursor_guards.contains_key(&a));
        assert!(!tx.world.focus.detached_cursor_guards.contains_key(&b));
        assert_eq!(tx.execution_subject(), None);
    });
    accepted(&mut engine, &capture(6, "win-a"));
    engine.tx(|tx| {
        assert!(!tx.world.focus.detached_cursor_guards.contains_key(&a));
        assert_eq!(tx.execution_subject(), Some(a_fact.window));
    });
}
