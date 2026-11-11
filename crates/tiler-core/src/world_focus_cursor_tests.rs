//! Execution-time logical focus cursor (retained-focus Q1): the cursor is
//! separate from native observed foreground and per-workspace history.
//! Accepted focus choices advance it immediately in FIFO order; a live
//! retained request preserves its target across unchanged-baseline
//! reports; a newer observed focus supersedes it; refusal resets it to
//! the eligible observed focus; non-focus and refused commands never
//! erase it. Implicit subjects resolve from the cursor before a newer
//! focus decision retires the older authority.

use crate::boundary::CoreCommand;
use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::WindowId;
use crate::ids::{CorrelationId, OwnerId};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    BatchSettlement, CommandResult, FocusFeedback, FocusRequestCause, PresentationIngressV4,
    PresentationRowV4, VisibilityReason, VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowFact,
    WorldCommand, WorldReplyV4, WorldReportV4,
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
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(ingress(focused)),
        intent_records: Vec::new(),
        renderer_input: None,
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
        window: WindowId(window.to_owned()),
        direction: String::new(),
    }
}

fn implicit(command: WorldCommand) -> WorldCommand {
    WorldCommand::Implicit(Box::new(command))
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    implicit(WorldCommand::FocusDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    })
}

fn assert_not_rejected(result: &Option<CommandResult>, what: &str) {
    assert!(
        !matches!(result, Some(CommandResult::Rejected { .. })),
        "{what} must not reject; got {result:?}"
    );
}

fn windows() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
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
    assert!(setup.effects.is_empty());
    let pending = setup.focus_request.clone().expect("focus B retained");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(pending.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(
        engine.execution_subject(),
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
        engine.execution_subject(),
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
        window: WindowId("win-zzz".to_owned()),
        target_output: crate::directional::OutputId("out-a".to_owned()),
        target_workspace: "ws-1".to_owned(),
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
    assert!(reply.effects.is_empty());
    let pending = reply
        .focus_request
        .clone()
        .expect("pending B survives refusal");
    assert_eq!(pending.target.0, "win-b");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-b".to_owned())),
        "refused non-focus must not erase the cursor"
    );
    // The CoreCommand operation seam stays non-focus vocabulary: an
    // operation refusal is equally powerless over pending focus.
    let refused_op = WorldCommand::Operation(CoreCommand::Move {
        window: "win-zzz".to_owned(),
        direction: "left".to_owned(),
        cross_output_transfer: false,
        same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
    });
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
        setter: VisibilitySetter::Refused,
        reason: VisibilityReason::NativeRefused,
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
        engine.execution_subject(),
        Some(WindowId("win-a".to_owned())),
        "retired cursor resets to eligible observed A, not abandoned B"
    );
    assert!(
        engine.last_active_window.values().any(|w| w.0 == "win-b"),
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
