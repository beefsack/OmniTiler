//! KDE rapid-input regressions (change record
//! `kde-rapid-input-latest-world`): the queue ONLY parks; ONE fresh capture
//! incorporates native observations once, then drains ALL parked commands
//! FIFO against post-previous-command state with ONE final full publication
//! and one per-command settlement. No queue-time execution, no command-only
//! publication lane. The first command keeps the contracted pre-observation
//! SEND-source freeze; later subjects bind post-previous-command state. The
//! explicit by-id subjects keep their meaning; the execution-time
//! (`win:{"currentFocus":true}`) subject resolves from retained core focus; the empty
//! `focus-directional` subject keeps its genuine no-focus meaning; close
//! evidence amid a burst is never lost.

use crate::boundary::CoreCommand;
use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, SameAxisMove, WindowId};
use crate::ids::{CorrelationId, OwnerId};
use crate::intent_data::{IntentData, decode_intent_data, encode_intent_data};
use crate::world_directional::tests::{fact, screen};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    BatchSettlement, CommandResult, EffectKind, IntentWrite, NativeObservation, NativeOccupancy,
    NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4, SendSource, TransferKind,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;

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
            Some(OutputId("out-a".to_owned()))
        },
    }
}

fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    closed: Vec<&str>,
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
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        // Staged native focus stays stale across the burst on purpose: no
        // fresh platform capture arrives between queued commands.
        focused_global: focused.to_owned(),
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
    report(seq, windows, focused, None, None, Vec::new())
}

fn issue_for(report: &WorldReportV4) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

/// Drive one fresh capture plus the parked FIFO batch through the batched
/// lane: one observation ingestion, every command in order, one final full
/// publication. Panics unless the admission outcome is `Accepted`.
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

fn implicit_maximize() -> WorldCommand {
    implicit(WorldCommand::ToggleMaximize {
        window: WindowId(String::new()),
    })
}

fn implicit_resize_dir(direction: &str, mode: &str) -> WorldCommand {
    implicit(WorldCommand::Operation(CoreCommand::Resize {
        window: String::new(),
        direction: direction.to_owned(),
        mode: mode.to_owned(),
        press_index: 1,
    }))
}

fn implicit_move_dir(direction: &str) -> WorldCommand {
    implicit(WorldCommand::MoveDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
        same_axis: SameAxisMove::GroupWithNeighbor,
    })
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    implicit(WorldCommand::FocusDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    })
}

fn has_focus_effect(reply: &WorldReplyV4, window: &str) -> bool {
    reply
        .effects
        .iter()
        .any(|effect| effect.kind == EffectKind::Focus && effect.target.0 == window)
}

fn focus_effects(reply: &WorldReplyV4) -> Vec<&WindowId> {
    reply
        .effects
        .iter()
        .filter(|effect| effect.kind == EffectKind::Focus)
        .map(|effect| &effect.target)
        .collect()
}

/// Admitted core neighbor topology prerequisite: the engine inserts
/// admitted clients into its own tiling order independent of the raw
/// native frame origins, so directional fixtures must prove the engine
/// plan before pressing a direction. Returns (win-a x, win-b x).
/// Panics with the full snapshot when a row has no published geometry.
fn assert_admitted_left_right(reply: &WorldReplyV4, left: &str, right: &str) -> (i32, i32) {
    let x_of = |window: &str| {
        let row = reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| {
                panic!(
                    "topology prerequisite: window {window} missing; rows={:?}",
                    reply
                        .windows
                        .iter()
                        .map(|row| row.lifetime.0.clone())
                        .collect::<Vec<_>>()
                )
            });
        row.allocation
            .or(row.effective_frame)
            .unwrap_or_else(|| {
                panic!(
                    "topology prerequisite: window {window} has no published geometry; row={row:?}"
                )
            })
            .x
    };
    let lx = x_of(left);
    let rx = x_of(right);
    assert!(
        lx < rx,
        "topology prerequisite: {left} must sit left of {right} for left/right fixtures; got {left} x={lx} {right} x={rx}; rows={:?}",
        reply
            .windows
            .iter()
            .map(|row| (row.lifetime.0.clone(), row.allocation, row.effective_frame))
            .collect::<Vec<_>>()
    );
    (lx, rx)
}

fn assert_accepted(reply: &WorldReplyV4, what: &str) {
    assert!(
        !matches!(reply.command_result, Some(CommandResult::Rejected { .. })),
        "{what} must not reject; got {:?}",
        reply.command_result
    );
}

fn assert_ready_settlement(settlement: &BatchSettlement) {
    assert!(
        settlement
            .result
            .as_ref()
            .is_none_or(|result| !matches!(result, CommandResult::Rejected { .. })),
        "seq {} must settle ready; got {:?}",
        settlement.seq,
        settlement.result
    );
}

#[test]
fn rapid_input_implicit_maximize_resolves_from_retained_focus() {
    // After Focus win-b executes, retained core focus is win-b while staged
    // native focus is still win-a. The execution-time subject resolves from
    // the retained focus and bypasses the settled-native gate.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let first = engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (focused, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-b")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&focused, "focus win-b executes");
    assert!(has_focus_effect(&focused, "win-b"));
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(2, implicit_maximize())],
    );
    assert_accepted(&reply, "implicit maximize at retained focus");
    assert_eq!(settlements.len(), 1, "one queued command settles once");
    assert_eq!(settlements[0].seq, 2);
    assert_ready_settlement(&settlements[0]);
    assert_eq!(
        reply.state_revision,
        first.state_revision + 2,
        "two batches advance two revisions"
    );
}

#[test]
fn rapid_input_explicit_maximize_keeps_settled_native_gate() {
    // Explicit by-id subjects keep their meaning: with stale native focus
    // on win-a, an explicit win-b maximize still refuses FocusUnknown.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (focused, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-b")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&focused, "focus win-b executes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(
            2,
            WorldCommand::ToggleMaximize {
                window: WindowId("win-b".to_owned()),
            },
        )],
    );
    assert!(
        matches!(reply.command_result, Some(CommandResult::Rejected { .. })),
        "explicit maximize with stale native focus must keep the gate; got {:?}",
        reply.command_result
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "the per-command settlement carries the refusal: {:?}",
        settlements[0].result
    );
}

#[test]
fn rapid_input_implicit_without_canonical_focus_rejects() {
    // No recorded focus anywhere: the execution-time subject has nothing
    // to resolve from and refuses without mutation.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let before = engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), ""),
        &[(1, implicit_maximize())],
    );
    assert!(
        matches!(reply.command_result, Some(CommandResult::Rejected { .. })),
        "implicit subject with no canonical focus must reject; got {:?}",
        reply.command_result
    );
    assert!(
        focus_effects(&reply).is_empty(),
        "a rejected implicit subject must not manufacture a focus effect"
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "the refusal settles per-command: {:?}",
        settlements[0].result
    );
    assert_eq!(
        reply.state_revision,
        before.state_revision + 1,
        "one batch publishes one revision"
    );
}

#[test]
fn rapid_input_burst_applies_fifo_single_revision() {
    // Many actions, one capture: every command executes in arrival order
    // over post-previous-command state and the batch publishes once.
    let mut engine = engine();
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    let before = engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[
            (1, focus_cmd("win-b")),
            (2, focus_cmd("win-c")),
            (3, implicit_maximize()),
        ],
    );
    assert_accepted(&reply, "burst batch accepts");
    assert_eq!(
        settlements.len(),
        3,
        "every queued command settles exactly once"
    );
    for (index, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, index as u64 + 1, "settlements stay FIFO");
        assert_ready_settlement(settlement);
    }
    // Only the latest focus survives: the superseded win-b effect is gone,
    // and the trailing non-focus maximize never clears the win-c focus.
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-c".to_owned())],
        "latest batch focus is the only focus effect: {:?}",
        reply.effects
    );
    assert_accepted(&reply, "implicit maximize resolves at retained win-c");
    assert_eq!(
        reply.state_revision,
        before.state_revision + 1,
        "one capture advances exactly one revision"
    );
    assert_eq!(
        reply.command_result, settlements[2].result,
        "the final publication carries the last command's result"
    );
}

#[test]
fn rapid_input_batch_focus_supersedes_admission_focus() {
    // A newcomer admitted on the batch capture mints automatic focus, but
    // the queued explicit focus supersedes it: the unissued admission
    // effect must not publish.
    let mut engine = engine();
    engine
        .handle_report_v4_for_test(&observe(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
        ))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-d", "out-a", 300),
            ],
            "win-a",
        ),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&reply, "batch with newcomer accepts");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "explicit batch focus replaces the obsolete admission focus: {:?}",
        reply.effects
    );
}

#[test]
fn rapid_input_mixed_100_action_burst_single_capture() {
    // 100 mixed focus/move/resize/tiling actions, no capture between
    // filings: all park, one capture settles every one with one revision.
    // The final explicit focus survives the trailing resize.
    let mut engine = engine();
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    let before = engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let mut queued: Vec<(u64, WorldCommand)> = Vec::new();
    for n in 1..=90u64 {
        let command = match n % 5 {
            1 => focus_cmd("win-a"),
            2 => implicit_maximize(),
            3 => focus_cmd("win-c"),
            4 => WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-c".to_owned(),
                float_rect: None,
            }),
            _ => focus_cmd("win-b"),
        };
        queued.push((n, command));
    }
    // Tail keeps parity even (maximize off, win-c ends the cycles tiled)
    // and re-establishes plain tiled focus before the move/resize finish.
    // The move restacks the row into a full-width triple column where no
    // keyboard resize can plan (no horizontal neighbor, sub-minimum
    // vertical pairs), so win-c floats out first: the surviving tiled
    // pair stacks win-a above win-b, and the trailing resize shrinks
    // win-b upward into it. The current-focus subject still resolves from retained
    // core focus, and the final explicit focus survives the trailing
    // resize.
    for (n, window) in [
        (91, "win-a"),
        (92, "win-b"),
        (93, "win-c"),
        (94, "win-a"),
        (95, "win-b"),
    ] {
        queued.push((n, focus_cmd(window)));
    }
    queued.push((
        96,
        WorldCommand::MoveDirectional {
            window: WindowId("win-b".to_owned()),
            direction: "right".to_owned(),
            same_axis: SameAxisMove::GroupWithNeighbor,
        },
    ));
    queued.push((
        97,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-c".to_owned(),
            float_rect: None,
        }),
    ));
    queued.push((
        98,
        implicit(WorldCommand::Focus {
            window: WindowId(String::new()),
            direction: String::new(),
        }),
    ));
    queued.push((99, focus_cmd("win-b")));
    queued.push((100, implicit_resize_dir("up", "inwards")));
    let (reply, settlements) = run_batched(&mut engine, &observe(2, windows(), "win-a"), &queued);
    assert_eq!(settlements.len(), 100, "every action settles exactly once");
    for (index, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, index as u64 + 1, "settlements stay FIFO");
        assert_ready_settlement(settlement);
    }
    assert_accepted(&reply, "100-action batch accepts");
    assert_eq!(
        reply.state_revision,
        before.state_revision + 1,
        "100 actions share one revision"
    );
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "final focus survives the trailing resize: {:?}",
        reply.effects
    );
    assert_eq!(
        reply.command_result, settlements[99].result,
        "the final publication carries the trailing resize result"
    );
}

#[test]
fn rapid_input_mixed_burst_preserves_explicit_subjects_and_close() {
    // A burst command and an incoming close in the same arrival window
    // both apply; later explicit commands still name their own subjects,
    // and a focus at the retired window refuses.
    let mut engine = engine();
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            None,
            None,
            vec!["win-c"],
        ),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&reply, "burst command with close evidence executes");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "burst focus effect survives the close"
    );
    assert!(
        !engine.members.contains_key(&WindowId("win-c".to_owned())),
        "explicit close amid burst retires the member"
    );
    let (second, more) = run_batched(
        &mut engine,
        &observe(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
        ),
        &[
            (2, focus_cmd("win-a")),
            (3, focus_cmd("win-c")),
            (4, implicit_maximize()),
        ],
    );
    assert_eq!(more.len(), 3, "every follow-up settles");
    assert_ready_settlement(&more[0]);
    assert!(
        matches!(more[1].result, Some(CommandResult::Rejected { .. })),
        "focus at the retired window must settle refused: {:?}",
        more[1].result
    );
    assert_ready_settlement(&more[2]);
    // The refused middle command restores its pre-command image without
    // dropping the surrounding batch effects: only the latest focus wins.
    assert_eq!(
        focus_effects(&second),
        vec![&WindowId("win-a".to_owned())],
        "explicit follow-up focus wins FIFO: {:?}",
        second.effects
    );
    assert_accepted(&second, "implicit maximize at retained win-a");
}

#[test]
fn rapid_input_native_empty_batch_focus_establishes_implicit_subject() {
    // Genuine no-focus native capture across the whole batch: the first
    // explicit Focus still establishes core focus, so the later implicit
    // move in the SAME batch carries that focus in its operation event
    // instead of the empty native lag.
    let mut engine = engine();
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), ""),
        &[(1, focus_cmd("win-b")), (2, implicit_move_dir("right"))],
    );
    assert_accepted(&reply, "batch focus plus implicit move accepts");
    assert_eq!(settlements.len(), 2, "both commands settle");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "batch focus survives the trailing implicit move: {:?}",
        reply.effects
    );
}

#[test]
fn rapid_input_operation_focus_prefers_batch_command_over_empty_native() {
    // Unit contract for the batch focus helper: with staged native focus
    // empty, a session focus established by a prior batch command (and
    // homed in the operation domain) wins over the empty capture while
    // the batch is active. Retained pre-batch state never promotes
    // itself: only an in-batch move counts. The baseline is batch-only:
    // once the run finishes it clears, so the helper falls back to the
    // staged capture instead of leaking batch state into later commands.
    let mut engine = engine();
    let windows = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ]
    };
    engine
        .handle_report_v4_for_test(&observe(1, windows(), ""))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), ""),
        &[(1, focus_cmd("win-b")), (2, implicit_move_dir("right"))],
    );
    assert_accepted(&reply, "batch accepts");
    assert_ready_settlement(&settlements[1]);
    let domain = engine
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .cloned()
        .expect("win-b stays homed");
    assert_eq!(
        engine.effective_operation_focus(&domain, &WindowId(String::new())),
        WindowId(String::new()),
        "baseline clears at finish: no active batch falls back to staged capture"
    );
}

#[test]
fn rapid_input_genuine_focus_loss_clears_implicit_subject() {
    // Native focus genuinely lost (empty) with both windows still present:
    // the execution-time subject must not target the stale prior focus.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (focused, first) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&focused, "focus win-b executes");
    assert_ready_settlement(&first[0]);
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), ""),
        &[(2, implicit_maximize())],
    );
    assert_eq!(settlements.len(), 1, "the command settles once");
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "implicit subject after genuine focus loss must refuse: {:?}",
        settlements[0].result
    );
    assert!(
        focus_effects(&reply).is_empty(),
        "a refused implicit subject manufactures no focus effect"
    );
}

#[test]
fn rapid_input_empty_focus_directional_stays_no_focus() {
    // Unchanged semantics: the explicit empty win on focus-directional
    // names genuinely no focused subject and never resolves to the
    // retained or native focused window.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[
            (
                1,
                WorldCommand::FocusDirectional {
                    window: WindowId(String::new()),
                    direction: "right".to_owned(),
                },
            ),
            (2, focus_cmd("win-b")),
        ],
    );
    assert_accepted(&reply, "batch with empty subject accepts");
    assert_eq!(settlements.len(), 2);
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "empty subject manufactures no focus; explicit focus wins: {:?}",
        reply.effects
    );
}

#[test]
fn rapid_input_batch_refusal_keeps_rollback_and_command_saves() {
    // A mid-burst refusal restores its pre-command image without dropping
    // the pending refusal rollback save or the batch command saves: the
    // single final publication emits each write exactly once.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("valid generation");
    fn cfact(window: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.sticky = None;
        row
    }
    let windows = || vec![cfact("win-a", 0), cfact("win-b", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    // Seed a pending refusal rollback save for an untouched live window.
    engine.sticky_rollback_saves.insert(
        WindowId("win-b".to_owned()),
        IntentWrite {
            win: WindowId("win-b".to_owned()),
            data: Some(encode_intent_data(&IntentData::new(Some(false), None))),
        },
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[
            (
                1,
                WorldCommand::SetSticky {
                    window: WindowId("win-a".to_owned()),
                    value: true,
                },
            ),
            (2, focus_cmd("win-zz")),
            (3, focus_cmd("win-b")),
        ],
    );
    assert_accepted(&reply, "batch with mid refusal accepts");
    assert_eq!(settlements.len(), 3);
    assert!(
        settlements[0]
            .result
            .as_ref()
            .is_none_or(|result| !matches!(result, CommandResult::Rejected { .. })),
        "sticky command settles: {:?}",
        settlements[0].result
    );
    assert!(
        matches!(settlements[1].result, Some(CommandResult::Rejected { .. })),
        "unknown focus settles refused: {:?}",
        settlements[1].result
    );
    assert_ready_settlement(&settlements[2]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "latest focus wins past the refusal: {:?}",
        reply.effects
    );
    let mut wins: Vec<String> = reply
        .intent_writes
        .iter()
        .map(|write| write.win.0.clone())
        .collect();
    wins.sort();
    assert_eq!(
        wins,
        vec!["win-a".to_owned(), "win-b".to_owned()],
        "command save plus untouched rollback save publish exactly once: {:?}",
        reply.intent_writes
    );
}

#[test]
fn rapid_input_pre_ready_batch_reparks_then_drains() {
    // Commands filed before the first usable capture must not silently
    // vanish: the bootstrap adopts nothing, settles nothing, and the same
    // batch drains on the first usable capture.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let mut early = observe(1, windows(), "win-a");
    early.inventory_complete = false;
    let revision_before = engine.state_revision;
    match engine.handle_report_v4_batched(&early, &issue_for(&early), &[(1, focus_cmd("win-b"))]) {
        (V4RunOutcome::Accepted(reply), settlements) => {
            assert!(!reply.ready, "bootstrap carries no authority");
            assert!(
                settlements.is_empty(),
                "bootstrap settles nothing; the batch reparks"
            );
        }
        (other, _) => panic!("expected bootstrap acceptance, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision, revision_before,
        "bootstrap advances no revision"
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&reply, "pre-ready command drains on first usable capture");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
    assert!(has_focus_effect(&reply, "win-b"));
}

#[test]
fn rapid_input_host_native_batch_joins_evidence_once() {
    // Valid host-owned native rows ride the batch capture: the command
    // still settles and the native evidence incorporates without wedging.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            native_sticky: true,
            maximize: true,
            fullscreen: true,
            window_suppression: true,
            ..HostCapabilities::default()
        },
    )
    .expect("valid generation");
    fn native_row() -> NativeObservation {
        NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["n-0".to_owned()],
            current_native: Some("n-0".to_owned()),
            occupancy: vec![NativeOccupancy {
                native: "n-0".to_owned(),
                count: None,
            }],
        }
    }
    fn host_fact(window: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.native_workspace = NativeWorkspaceFact::Value("n-0".to_owned());
        row
    }
    fn host_report(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
        let mut out = report(seq, windows, focused, None, None, Vec::new());
        out.native = vec![native_row()];
        out
    }
    let windows = || vec![host_fact("win-a", 0), host_fact("win-b", 100)];
    engine
        .handle_report_v4_for_test(&host_report(1, windows(), "win-a"))
        .expect("host-native admission accepted");
    let (reply, settlements) = run_batched(
        &mut engine,
        &host_report(2, windows(), "win-a"),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&reply, "command drains on the native-carrying capture");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
    assert!(has_focus_effect(&reply, "win-b"));
}

#[test]
fn rapid_input_two_implicit_focus_directionals_resolve_sequentially() {
    // Two execution-time focus directionals in ONE capture resolve
    // against post-previous-command core focus. An executed setup focus
    // establishes the slot and the session leaf together (pure
    // observations never move the leaf past a pending plan); a
    // commandless capture lets the native fact catch up to the
    // pre-batch focus, so the batch opens with slot, leaf, and staged
    // native all on win-b. Right moves win-b to win-a, then left must
    // resolve win-a from the first command's focus - not the stale
    // pre-batch native fact - and move back to win-b.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let before = engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-b")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&setup, "setup focus win-b executes");
    assert!(has_focus_effect(&setup, "win-b"));
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (caught, _) = run_batched(&mut engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(&caught, "native fact catches up to the pre-batch focus");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[
            (2, implicit_focus_dir("right")),
            (3, implicit_focus_dir("left")),
        ],
    );
    assert_eq!(
        settlements.len(),
        2,
        "both directionals settle exactly once"
    );
    assert_eq!(settlements[0].seq, 2, "settlements stay FIFO");
    assert_eq!(settlements[1].seq, 3, "settlements stay FIFO");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&reply, "sequential directional batch accepts");
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "latest focus wins: right then left ends on win-b: {:?}",
        reply.effects
    );
    assert_eq!(
        reply.state_revision,
        before.state_revision + 3,
        "setup plus catch-up plus batch advance three revisions"
    );
    assert_eq!(
        reply.command_result, settlements[1].result,
        "the final publication carries the last command's result"
    );
}

#[test]
fn rapid_input_implicit_focus_then_move_uses_post_prior_subject() {
    // Mixed batch: the trailing implicit move resolves its subject from
    // the focus the preceding directional established (win-b), not the
    // stale pre-batch native focus (win-a). The setup focus runs
    // natively unobserved, so slot, leaf, and staged native all open
    // the batch on win-a; the stale win-a move-right would refuse
    // against the post-head leaf while the sequential win-b move
    // succeeds.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-a")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&setup, "setup focus win-a executes");
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[
            (2, implicit_focus_dir("left")),
            (3, implicit_move_dir("right")),
        ],
    );
    assert_eq!(settlements.len(), 2, "both commands settle exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&reply, "focus then move batch accepts");
    assert_eq!(
        reply.command_result, settlements[1].result,
        "the final publication carries the last command's result"
    );
}

#[test]
fn rapid_input_alternating_implicit_directional_burst_resolves_sequentially() {
    // Bounded repeated-action guard: ten alternating execution-time focus
    // directionals in one capture each resolve from the previous
    // command's focus. Every command settles individually (no count
    // cheating): ten settlements, FIFO, final focus win-b.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-b")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&setup, "setup focus win-b executes");
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (caught, _) = run_batched(&mut engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(&caught, "native fact catches up to the pre-batch focus");
    let mut queued: Vec<(u64, WorldCommand)> = Vec::new();
    for n in 2..=11u64 {
        let direction = if n % 2 == 0 { "right" } else { "left" };
        queued.push((n, implicit_focus_dir(direction)));
    }
    let (reply, settlements) = run_batched(&mut engine, &observe(4, windows(), "win-b"), &queued);
    assert_eq!(
        settlements.len(),
        10,
        "every directional settles exactly once"
    );
    for (index, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, index as u64 + 2, "settlements stay FIFO");
        assert_ready_settlement(settlement);
    }
    assert_accepted(&reply, "alternating burst accepts");
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "even alternations end on win-b: {:?}",
        reply.effects
    );
}

fn implicit_toggle_float() -> WorldCommand {
    implicit(WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: String::new(),
        float_rect: None,
    }))
}

fn explicit_toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn implicit_set_sticky(value: bool) -> WorldCommand {
    implicit(WorldCommand::SetSticky {
        window: WindowId(String::new()),
        value,
    })
}

fn float_write_pair(write: &IntentWrite) -> (String, IntentData) {
    (
        write.win.0.clone(),
        decode_intent_data(write.data.as_deref().expect("bytes")).expect("decodes"),
    )
}

#[test]
fn bookkeeping_command_resolves_implicit_subject_once() {
    // The carried bookkeeping identity is the pre-execution explicit
    // subject: implicit maps once, explicit passes through, and an
    // unresolvable implicit falls back to the placeholder (no save arm
    // matches it, preserving mismatch/empty semantics).
    let mut engine = engine();
    engine.implicit_focus = Some(WindowId("win-a".to_owned()));
    assert_eq!(
        engine.bookkeeping_command(&implicit_toggle_float()),
        explicit_toggle_float("win-a")
    );
    assert_eq!(
        engine.bookkeeping_command(&explicit_toggle_float("win-b")),
        explicit_toggle_float("win-b")
    );
    engine.implicit_focus = None;
    assert_eq!(
        engine.bookkeeping_command(&implicit_toggle_float()),
        implicit_toggle_float()
    );
}

fn setup_float_subject(engine: &mut WorldEngine) {
    // Slot, leaf, and staged native all open the batch on win-b: an
    // executed setup focus plus a commandless capture that lets the
    // native fact catch up to the pre-batch focus.
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut *engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-b")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&setup, "setup focus win-b executes");
    assert!(has_focus_effect(&setup, "win-b"));
    let (caught, _) = run_batched(&mut *engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(&caught, "native fact catches up to the pre-batch focus");
}

#[test]
fn rapid_input_implicit_float_intermediate_save_publishes() {
    // The intermediate implicit float resolves win-b at execution: its
    // save must reach the single final publication exactly like the
    // explicit spelling, even though the trailing focus moves on.
    let mut engine = engine();
    setup_float_subject(&mut engine);
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[(2, implicit_toggle_float()), (3, focus_cmd("win-a"))],
    );
    assert_eq!(settlements.len(), 2, "both commands settle exactly once");
    assert_eq!(settlements[0].seq, 2, "settlements stay FIFO");
    assert_eq!(settlements[1].seq, 3, "settlements stay FIFO");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&reply, "float then focus batch accepts");
    assert_eq!(
        reply.command_result, settlements[1].result,
        "the final publication carries the last command's result"
    );
    assert_eq!(reply.intent_writes.len(), 1, "{:?}", reply.intent_writes);
    assert_eq!(
        float_write_pair(&reply.intent_writes[0]),
        ("win-b".to_owned(), IntentData::new(Some(true), None)),
        "intermediate implicit float save publishes resolved"
    );
}

#[test]
fn rapid_input_mixed_implicit_explicit_float_batch_keeps_both_saves() {
    // Latest-wins per window across the batch: the intermediate implicit
    // float on win-b and the final explicit float on win-a each publish
    // their own resolved save exactly once.
    let mut engine = engine();
    setup_float_subject(&mut engine);
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[
            (2, implicit_toggle_float()),
            (3, explicit_toggle_float("win-a")),
        ],
    );
    assert_eq!(settlements.len(), 2, "both commands settle exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&reply, "mixed float batch accepts");
    let mut pairs: Vec<(String, IntentData)> =
        reply.intent_writes.iter().map(float_write_pair).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pairs,
        vec![
            ("win-a".to_owned(), IntentData::new(Some(true), None)),
            ("win-b".to_owned(), IntentData::new(Some(true), None)),
        ],
        "both resolved saves publish: {:?}",
        reply.intent_writes
    );
}

#[test]
fn rapid_input_implicit_sticky_batch_keeps_command_and_rollback_saves() {
    // An implicit sticky command resolves its subject at execution: its
    // staged save joins the single final publication alongside a pending
    // refusal rollback save for an untouched window, with no drop.
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            birth_clear: true,
            native_sticky: false,
            window_suppression: true,
            maximize: true,
            fullscreen: true,
            ..HostCapabilities::default()
        },
        Vec::<(String, RawSettingValue)>::new(),
    )
    .expect("valid generation");
    fn cfact(window: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.sticky = None;
        row
    }
    let windows = || vec![cfact("win-a", 0), cfact("win-b", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    // Seed a pending refusal rollback save for an untouched live window.
    engine.sticky_rollback_saves.insert(
        WindowId("win-b".to_owned()),
        IntentWrite {
            win: WindowId("win-b".to_owned()),
            data: Some(encode_intent_data(&IntentData::new(Some(false), None))),
        },
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[
            (1, implicit_set_sticky(true)),
            (2, focus_cmd("win-zz")),
            (3, focus_cmd("win-b")),
        ],
    );
    assert_accepted(&reply, "batch with mid refusal accepts");
    assert_eq!(settlements.len(), 3);
    assert!(
        settlements[0]
            .result
            .as_ref()
            .is_none_or(|result| !matches!(result, CommandResult::Rejected { .. })),
        "implicit sticky command settles: {:?}",
        settlements[0].result
    );
    assert!(
        matches!(settlements[1].result, Some(CommandResult::Rejected { .. })),
        "unknown focus settles refused: {:?}",
        settlements[1].result
    );
    assert_ready_settlement(&settlements[2]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "latest focus wins past the refusal: {:?}",
        reply.effects
    );
    let mut wins: Vec<String> = reply
        .intent_writes
        .iter()
        .map(|write| write.win.0.clone())
        .collect();
    wins.sort();
    assert_eq!(
        wins,
        vec!["win-a".to_owned(), "win-b".to_owned()],
        "implicit command save plus untouched rollback save publish exactly once: {:?}",
        reply.intent_writes
    );
    let sticky_write = reply
        .intent_writes
        .iter()
        .find(|write| write.win.0 == "win-a")
        .expect("resolved sticky save");
    assert_eq!(
        decode_intent_data(sticky_write.data.as_deref().expect("bytes")).expect("decodes"),
        IntentData::new(Some(true), Some(false)),
        "implicit sticky save carries the resolved subject data"
    );
}

#[test]
fn rapid_input_single_implicit_float_batch_publishes_resolved_save() {
    // A lone implicit float is also the final command: its staged saves
    // stay for the single final publish drain, which must see the carried
    // resolved identity rather than the placeholder.
    let mut engine = engine();
    setup_float_subject(&mut engine);
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[(2, implicit_toggle_float())],
    );
    assert_eq!(
        settlements.len(),
        1,
        "the single command settles exactly once"
    );
    assert_eq!(settlements[0].seq, 2, "settlements stay FIFO");
    assert_ready_settlement(&settlements[0]);
    assert_accepted(&reply, "single implicit float batch accepts");
    assert_eq!(
        reply.command_result, settlements[0].result,
        "the final publication carries the last command's result"
    );
    assert_eq!(reply.intent_writes.len(), 1, "{:?}", reply.intent_writes);
    assert_eq!(
        float_write_pair(&reply.intent_writes[0]),
        ("win-b".to_owned(), IntentData::new(Some(true), None)),
        "final-drain implicit float save publishes resolved"
    );
}

#[test]
fn rapid_input_implicit_fixed_tile_unfloat_records_tile_save() {
    // The adapter fixed-tile case: an implicit unfloat of an automatic
    // fixed float resolves the fixed window, tiles the row, and records
    // the exact tile save the explicit spelling emits.
    let mut engine = engine();
    fn fixed(window: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.hints = Some(crate::size_hints::WindowSizeHints {
            min_w: Some(40),
            min_h: Some(90),
            max_w: Some(40),
            max_h: Some(90),
        });
        row
    }
    let windows = || vec![fixed("win-a", 0), fact("win-b", "out-a", 400)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, _) = run_batched(
        &mut engine,
        &report(
            2,
            windows(),
            "win-a",
            Some(1),
            Some(focus_cmd("win-a")),
            Vec::new(),
        ),
        &[],
    );
    assert_accepted(&setup, "setup focus win-a executes");
    assert!(has_focus_effect(&setup, "win-a"));
    let (caught, _) = run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    assert_accepted(&caught, "native fact catches up to the pre-batch focus");
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-a"),
        &[(2, implicit_toggle_float())],
    );
    assert_eq!(
        settlements.len(),
        1,
        "the single command settles exactly once"
    );
    assert_ready_settlement(&settlements[0]);
    assert_accepted(&reply, "implicit fixed unfloat accepts");
    match &reply.command_result {
        Some(CommandResult::Float { window, .. }) => assert_eq!(
            window.0, "win-a",
            "tile result targets the resolved window: {:?}",
            reply.command_result
        ),
        other => panic!("expected float result, got {other:?}"),
    }
    assert_eq!(reply.intent_writes.len(), 1, "{:?}", reply.intent_writes);
    assert_eq!(
        float_write_pair(&reply.intent_writes[0]),
        ("win-a".to_owned(), IntentData::new(Some(false), None)),
        "implicit fixed unfloat records the exact tile save"
    );
}

fn implicit_send_numbered_home() -> WorldCommand {
    WorldCommand::Implicit(Box::new(WorldCommand::SendNumbered {
        window: WindowId(String::new()),
        source: SendSource::HomeThenAction,
        index: 0,
        follow: false,
    }))
}

fn two_screen_capture(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    foreground: Option<&str>,
    cursor: Option<(i32, i32)>,
    registry: Vec<&str>,
) -> WorldReportV4 {
    let mut out = observe(seq, windows, focused);
    out.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    out.action_foreground = foreground.map(|name| OutputId(name.to_owned()));
    out.cursor = cursor;
    out.action_cached = None;
    out.action_outputs = registry
        .into_iter()
        .map(|name| OutputId(name.to_owned()))
        .collect();
    out
}

#[test]
fn rapid_input_implicit_send_freezes_pre_observation_retained_home() {
    // First-command vintage: the retained home (out-a) beats a disagreeing
    // action context (out-b). The fresh capture keeps win-a observed on
    // out-a with stale native focus on win-a, so only the action legs point
    // at out-b. The buggy empty-placeholder lookup falls back to the action
    // context out-b and refuses CrossDomainMismatch; the fix pins out-a.
    let mut engine = engine();
    let mut seed = observe(1, vec![fact("win-a", "out-a", 0)], "win-a");
    seed.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&seed)
        .expect("admission observes");
    assert_eq!(
        engine.implicit_focus,
        Some(WindowId("win-a".to_owned())),
        "precondition: retained subject is win-a"
    );
    let capture = two_screen_capture(
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some("out-b"),
        Some((850, 10)),
        vec!["out-b", "out-a"],
    );
    let (reply, settlements) =
        run_batched(&mut engine, &capture, &[(2, implicit_send_numbered_home())]);
    assert_eq!(settlements.len(), 1, "the send settles exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_accepted(&reply, "implicit send at retained home accepts");
    let transfer = reply
        .transfers
        .iter()
        .find(|row| matches!(&row.kind, TransferKind::Window(win) if win.0 == "win-a"))
        .expect("implicit send retains one win-a transfer");
    assert_eq!(
        transfer.target_output,
        OutputId("out-a".to_owned()),
        "frozen source pins the retained home, never the action context"
    );
}

#[test]
fn rapid_input_implicit_send_later_command_uses_post_prior_home() {
    // Later-command vintage: the batch loop refreezes every post-first
    // command from staged post-previous-command state
    // (`RawRouting::from_staged` plus the live slot). An executed Focus
    // moves core focus to win-b (homed out-b) while staged native/action
    // legs stay on out-a, so refreezing the trailing implicit send must
    // pin win-b's post-prior home out-b: not the pre-observation subject
    // win-a (out-a), not the action context out-a. The buggy
    // empty-placeholder lookup falls back to the action context out-a.
    let mut engine = engine();
    let mut seed = observe(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
    );
    seed.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&seed)
        .expect("admission observes");
    assert_eq!(
        engine.implicit_focus,
        Some(WindowId("win-a".to_owned())),
        "precondition: batch opens on win-a"
    );
    let capture = two_screen_capture(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        Some("out-a"),
        Some((10, 10)),
        vec!["out-a", "out-b"],
    );
    let (focused, first_settlements) =
        run_batched(&mut engine, &capture, &[(2, focus_cmd("win-b"))]);
    assert_accepted(&focused, "setup focus win-b executes");
    assert_eq!(first_settlements.len(), 1);
    assert_ready_settlement(&first_settlements[0]);
    assert_eq!(
        engine.implicit_focus,
        Some(WindowId("win-b".to_owned())),
        "post-prior core subject is win-b while staged legs stay out-a"
    );
    // The exact later-command refreeze call from the batch loop.
    let routing = crate::world_engine::RawRouting::from_staged(&engine.staged);
    let (frozen, _) = engine.freeze_command_sources(Some(&implicit_send_numbered_home()), &routing);
    assert_eq!(
        frozen,
        Some(OutputId("out-b".to_owned())),
        "later refreeze pins the post-prior home, never action nor pre-observation subject"
    );
}

#[test]
fn rapid_input_implicit_send_first_command_uses_post_native_subject() {
    // Fresh native focus moves the execution subject after the
    // pre-observation freeze: the retained snapshot home of the UPDATED
    // subject (win-b/out-b) beats the disagreeing action context (out-a).
    // Seeded win-a/out-a + win-b/out-b focused win-a; the fresh capture
    // reports native focus win-b while every action leg points at out-a.
    // The stale-slot freeze pins out-a and refuses; the fix pins out-b.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)];
    let mut seed = observe(1, windows(), "win-a");
    seed.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&seed)
        .expect("admission observes");
    let capture = two_screen_capture(
        2,
        windows(),
        "win-b",
        Some("out-a"),
        Some((10, 10)),
        vec!["out-a", "out-b"],
    );
    let (reply, settlements) =
        run_batched(&mut engine, &capture, &[(2, implicit_send_numbered_home())]);
    assert_eq!(settlements.len(), 1, "the send settles exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_accepted(&reply, "implicit send at the post-native subject accepts");
    let transfer = reply
        .transfers
        .iter()
        .find(|row| matches!(&row.kind, TransferKind::Window(win) if win.0 == "win-b"))
        .expect("implicit send retains one win-b transfer");
    assert_eq!(
        transfer.target_output,
        OutputId("out-b".to_owned()),
        "frozen source pins the post-native subject snapshot home, never the action context"
    );
}

#[test]
fn rapid_input_implicit_send_same_report_rehome_refuses() {
    // Same-report rehoming must preserve the pre-observation home of the
    // EXECUTION subject: win-b moves out-a to out-b in the fresh capture,
    // the post-native subject is win-b, but its snapshot home is out-a
    // while its staged home is out-b. The correct outcome refuses (prehome
    // mismatches staged home) instead of retargeting to either the staged
    // home or the action context. Lock-in oracle: passes before and after
    // the vintage fix; guards against staged-home/action regressions.
    let mut engine = engine();
    let mut seed = observe(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
    );
    seed.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&seed)
        .expect("admission observes");
    let capture = two_screen_capture(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-b",
        Some("out-b"),
        Some((850, 10)),
        vec!["out-b", "out-a"],
    );
    let (reply, settlements) =
        run_batched(&mut engine, &capture, &[(2, implicit_send_numbered_home())]);
    assert_eq!(settlements.len(), 1, "the send settles exactly once");
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "rehomed mover must refuse, never retarget: {:?}",
        settlements[0].result
    );
    assert!(
        !reply.transfers.iter().any(|row| matches!(&row.kind,
            TransferKind::Window(win) if win.0 == "win-b")),
        "refusal retains no win-b transfer: {:?}",
        reply.transfers
    );
}

#[test]
fn rapid_input_implicit_send_after_same_focus_batch_command_commits() {
    // In-batch focus provenance: the out-b session already rests on win-b
    // (sole member), so an explicit batch Focus win-b changes no session
    // value versus the baseline. The following implicit SEND must still
    // see post-prior core state (win-b), not the empty staged native lag
    // (native focus stays win-a on out-a). Stale-slot reasoning refuses
    // absent-focus; actual command focus commits with a valid target.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)];
    let mut seed = observe(1, windows(), "win-a");
    seed.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine
        .handle_report_v4_for_test(&seed)
        .expect("admission observes");
    let capture = two_screen_capture(
        2,
        windows(),
        "win-a",
        Some("out-a"),
        Some((10, 10)),
        vec!["out-a", "out-b"],
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &capture,
        &[(2, focus_cmd("win-b")), (3, implicit_send_numbered_home())],
    );
    assert_eq!(settlements.len(), 2, "both commands settle exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&reply, "focus plus following implicit send accepts");
    assert!(
        engine.batch_command_focus.is_empty(),
        "accepted finish clears batch-only provenance, never retained: {:?}",
        engine.batch_command_focus
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|row| matches!(&row.kind, TransferKind::Window(win) if win.0 == "win-b"))
        .expect("implicit send retains one win-b transfer");
    assert_eq!(
        transfer.target_output,
        OutputId("out-b".to_owned()),
        "following send commits against post-prior home with a valid target"
    );
}

#[test]
fn rapid_input_save_reserve_covers_inline_implicit_max_id() {
    // Pre-mutation intent-save reserve must unwrap the execution-time
    // subject: an inline implicit float stages a resolved save up to the
    // longest opaque id, never the empty placeholder width.
    use crate::world_sizing_v4::max_intent_save_bound_v4;
    let implicit = WorldCommand::Implicit(Box::new(WorldCommand::Operation(
        CoreCommand::ToggleFloat {
            window: String::new(),
            float_rect: None,
        },
    )));
    let probe = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(1),
        Some(implicit),
        vec![],
    );
    assert_eq!(
        max_intent_save_bound_v4(&probe),
        16 + 130 + crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON,
        "implicit inline reserves the max resolved id, not the placeholder"
    );
}

#[test]
fn rapid_input_batch_save_bound_covers_distinct_saves() {
    // The drained FIFO batch stages real intentWrites bytes beyond
    // report.command: the combined inline-plus-batch reserve must cover the
    // exact published section.
    use crate::world_sizing::intent_writes_estimate;
    use crate::world_sizing_v4::{max_intent_save_batch_bound_v4, max_intent_save_bound_v4};
    let mut eng = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    eng.handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let report = observe(2, windows(), "win-a");
    let queued = vec![
        (
            1,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-a".to_owned(),
                float_rect: None,
            }),
        ),
        (
            2,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "win-b".to_owned(),
                float_rect: None,
            }),
        ),
    ];
    let (reply, settlements) = run_batched(&mut eng, &report, &queued);
    assert_eq!(settlements.len(), 2);
    assert!(!reply.intent_writes.is_empty(), "batch publishes saves");
    let exact_section = intent_writes_estimate(&reply.intent_writes);
    let bound =
        max_intent_save_bound_v4(&report).saturating_add(max_intent_save_batch_bound_v4(&queued));
    // Structural estimator only (same sizing family, not the canonical
    // encoder): the empty 18-byte section rides the 4096 envelope slack
    // already in the reply bound; the batch reserve covers rows only. Real
    // canonical bytes are proven service-side in
    // `rapid_input_burst::rapid_input_batch_saves_canonical_bytes_fit_reserve`.
    assert!(
        bound.saturating_add(18) >= exact_section,
        "combined bound must cover the batch section: bound={bound} exact={exact_section} writes={:?}",
        reply.intent_writes
    );
}

#[test]
fn rapid_input_batch_save_bound_max_widths_and_distinct() {
    // Exact canonical encoding stays under the bound at maximum opaque id
    // widths; repeated explicit subjects count once while each implicit
    // save counts a max row.
    use crate::world_sizing::{MAX_INTENT_SAVE_DATA_JSON, json_string_len};
    use crate::world_sizing_v4::{
        is_intent_save_command_v4, max_intent_save_batch_bound_v4, max_intent_save_batch_rows_v4,
    };
    let max_id = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    assert_eq!(json_string_len(&max_id), 130);
    // Synthetic bound proof only (no 128-window execution): 100 distinct
    // max-width explicit plus 28 implicit is 128 rows, each a max row
    // (explicit exact equals max at this width). Estimator bytes here, not
    // canonical encoder bytes; canonical proof lives service-side.
    let mut queued: Vec<(u64, WorldCommand)> = Vec::new();
    for n in 1..=100u64 {
        queued.push((
            n,
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: format!("w{:0127}", n),
                float_rect: None,
            }),
        ));
    }
    for n in 101..=128u64 {
        queued.push((
            n,
            WorldCommand::Implicit(Box::new(WorldCommand::Operation(
                CoreCommand::ToggleFloat {
                    window: String::new(),
                    float_rect: None,
                },
            ))),
        ));
    }
    assert!(queued.iter().all(|(_, c)| is_intent_save_command_v4(c)));
    assert_eq!(max_intent_save_batch_rows_v4(&queued), 128);
    let bound = max_intent_save_batch_bound_v4(&queued);
    assert_eq!(bound, 128 * (16 + 130 + MAX_INTENT_SAVE_DATA_JSON + 1));
    // Manual exact section with 128 max-id rows fits under the bound.
    let writes: Vec<crate::world_report::IntentWrite> = queued
        .iter()
        .enumerate()
        .map(|(i, _)| crate::world_report::IntentWrite {
            win: WindowId(format!("w{:0127}", i)),
            data: Some(crate::intent_data::encode_intent_data(
                &crate::intent_data::IntentData::new(Some(true), None),
            )),
        })
        .collect();
    let exact = crate::world_sizing::intent_writes_estimate(&writes);
    assert!(
        bound >= exact,
        "max-width bound covers exact: {bound} vs {exact}"
    );
    // One repeated explicit subject counts once, not 100 times.
    let repeated: Vec<(u64, WorldCommand)> = (1..=100u64)
        .map(|n| {
            (
                n,
                WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: None,
                }),
            )
        })
        .collect();
    assert_eq!(max_intent_save_batch_rows_v4(&repeated), 1);
    assert_eq!(
        max_intent_save_batch_bound_v4(&repeated),
        16 + json_string_len("win-a") + MAX_INTENT_SAVE_DATA_JSON + 1
    );
    // Non-save commands reserve nothing.
    let unrelated = vec![(1, focus_cmd("win-a")), (2, focus_cmd("win-b"))];
    assert_eq!(max_intent_save_batch_rows_v4(&unrelated), 0);
    assert_eq!(max_intent_save_batch_bound_v4(&unrelated), 0);
}

#[test]
fn rapid_input_batch_rowcap_leaves_batch_unsettled() {
    // Fixed rowcap: rollback plus inline plus distinct batch saves must fit
    // MAX_INTENT_WRITES. Seed the rollback map full, then one queued save
    // must refuse before mutation with empty settlements, leaving the batch
    // unsettled for the caller (core itself holds no broker queue).
    use crate::world_report::MAX_INTENT_WRITES;
    let mut eng = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    eng.handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    for i in 0..MAX_INTENT_WRITES {
        eng.sticky_rollback_saves.insert(
            WindowId(format!("win-rb-{i:05}")),
            IntentWrite {
                win: WindowId(format!("win-rb-{i:05}")),
                data: Some(encode_intent_data(&IntentData::new(Some(false), None))),
            },
        );
    }
    assert_eq!(eng.sticky_rollback_saves.len(), MAX_INTENT_WRITES);
    let queued = vec![(
        1,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: "win-a".to_owned(),
            float_rect: None,
        }),
    )];
    let revision_before = eng.state_revision;
    let highwater_before = eng.command_highwater;
    let cached_reply_before = eng.last_reply_v4.clone();
    let capture = observe(2, windows(), "win-a");
    match eng.handle_report_v4_batched(&capture, &issue_for(&capture), &queued) {
        (V4RunOutcome::Rejected { kind, message }, settlements) => {
            assert_eq!(kind, crate::world_engine::KIND_SNAPSHOT);
            assert_eq!(message, "section-over-cap");
            assert!(
                settlements.is_empty(),
                "refusal settles nothing; caller leaves the batch unsettled"
            );
        }
        (other, _) => panic!("expected rowcap refusal, got {other:?}"),
    }
    assert_eq!(
        eng.state_revision, revision_before,
        "rowcap refusal advances no revision"
    );
    assert_eq!(
        eng.command_highwater, highwater_before,
        "rowcap refusal consumes no sequence"
    );
    assert_eq!(
        eng.sticky_rollback_saves.len(),
        MAX_INTENT_WRITES,
        "rowcap refusal mutates no ledger"
    );
    assert_eq!(
        eng.last_reply_v4, cached_reply_before,
        "rowcap refusal preserves the exact cached reply"
    );
}
