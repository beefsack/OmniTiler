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
use crate::intent_data::IntentData;
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    BatchSettlement, CommandResult, IntentDesire, NativeObservation, NativeOccupancy,
    NativeWorkspaceFact, PresentationIngressV4, PresentationRowV4, SendSource, TransferKind,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReplyV4, WorldReportV4,
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
        window: crate::directional::WindowSubject::Window(WindowId(window.to_owned())),
        direction: None,
    }
}

fn implicit_maximize() -> WorldCommand {
    WorldCommand::ToggleMaximize {
        window: crate::directional::WindowSubject::CurrentFocus,
    }
}

fn implicit_resize_dir(direction: &str, mode: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::Resize {
        window: crate::directional::WindowSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction)
            .expect("valid test direction"),
        mode: crate::directional::ResizeSelector::parse_wire(mode).expect("valid test mode"),
        press_index: 1,
    })
}

fn implicit_move_dir(direction: &str) -> WorldCommand {
    WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction).expect("direction"),
        same_axis: SameAxisMove::GroupWithNeighbor,
    }
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: crate::world_report::FocusDirectionalSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction).expect("direction"),
    }
}

fn has_focus_effect(reply: &WorldReplyV4, window: &str) -> bool {
    match reply.focus_request.as_ref() {
        Some(request) => {
            assert_eq!(request.id, format!("focus-{}", reply.state_revision));
            assert_eq!(request.issued_revision, reply.state_revision);
            request.target.0 == window
        }
        None => false,
    }
}

fn focus_effects(reply: &WorldReplyV4) -> Vec<&WindowId> {
    reply
        .focus_request
        .iter()
        .map(|r| {
            assert_eq!(r.id, format!("focus-{}", reply.state_revision));
            assert_eq!(r.issued_revision, reply.state_revision);
            assert_eq!(
                r.cause,
                crate::world_report::FocusRequestCause::ExplicitCommand
            );
            assert!(
                r.cmd_seq.is_some(),
                "explicit batch focus carries cmd_seq: {r:?}"
            );
            &r.target
        })
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

fn assert_accepted(settlements: &[BatchSettlement], what: &str) {
    assert!(
        !settlements
            .iter()
            .any(|s| matches!(s.result, Some(CommandResult::Rejected { .. }))),
        "{what} must not reject; got {settlements:?}"
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
fn rapid_input_implicit_maximize_resolves_from_logical_cursor() {
    // After Focus win-b executes, retained core focus is win-b while staged
    // native focus is still win-a. The execution-time subject resolves from
    // the retained focus and bypasses the settled-native gate.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let first = engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (focused, focused_settlements) = run_batched(
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
    assert_accepted(&focused_settlements, "focus win-b executes");
    assert!(has_focus_effect(&focused, "win-b"));
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(2, implicit_maximize())],
    );
    assert_accepted(&settlements, "implicit maximize at retained focus");
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
    let (_focused, focused_settlements) = run_batched(
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
    assert_accepted(&focused_settlements, "focus win-b executes");
    let (_reply, settlements) = run_batched(
        &mut engine,
        &observe(3, windows(), "win-a"),
        &[(
            2,
            WorldCommand::ToggleMaximize {
                window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
            },
        )],
    );
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "explicit maximize with stale native focus must keep the gate; got {:?}",
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
    assert_accepted(&settlements, "burst batch accepts");
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
    assert_accepted(&settlements, "implicit maximize resolves at retained win-c");
    assert_eq!(
        reply.state_revision,
        before.state_revision + 1,
        "one capture advances exactly one revision"
    );
    assert_eq!(settlements.len(), 3);
    assert_ready_settlement(&settlements[2]);
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
    assert!(reply.focus_request.is_some());
    assert_accepted(&settlements, "batch with newcomer accepts");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
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
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "win-c".to_owned(),
                )),
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
            window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
            direction: crate::directional::Direction::Right,
            same_axis: SameAxisMove::GroupWithNeighbor,
        },
    ));
    queued.push((
        97,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: crate::directional::WindowSubject::Window(WindowId("win-c".to_owned())),
            float_rect: None,
        }),
    ));
    queued.push((
        98,
        WorldCommand::Focus {
            window: crate::directional::WindowSubject::CurrentFocus,
            direction: None,
        },
    ));
    queued.push((99, focus_cmd("win-b")));
    queued.push((100, implicit_resize_dir("up", "inwards")));
    let (reply, settlements) = run_batched(&mut engine, &observe(2, windows(), "win-a"), &queued);
    assert_eq!(settlements.len(), 100, "every action settles exactly once");
    for (index, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, index as u64 + 1, "settlements stay FIFO");
        assert_ready_settlement(settlement);
    }
    assert_accepted(&settlements, "100-action batch accepts");
    assert_eq!(
        reply.state_revision,
        before.state_revision + 1,
        "100 actions share one revision"
    );
    assert_eq!(settlements.len(), 100);
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
    assert_accepted(&settlements, "burst command with close evidence executes");
    assert_eq!(settlements.len(), 1);
    assert_ready_settlement(&settlements[0]);
    assert_eq!(
        focus_effects(&reply),
        vec![&WindowId("win-b".to_owned())],
        "burst focus effect survives the close"
    );
    assert!(
        !engine
            .world
            .workspace
            .members
            .contains_key(&WindowId("win-c".to_owned())),
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
    assert!(second.focus_request.is_some());
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
    assert!(reply.focus_request.is_some());
    assert_accepted(&settlements, "batch focus plus implicit move accepts");
    assert_eq!(settlements.len(), 2, "both commands settle");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
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
    let (_reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), ""),
        &[(1, focus_cmd("win-b")), (2, implicit_move_dir("right"))],
    );
    assert_accepted(&settlements, "batch accepts");
    assert_ready_settlement(&settlements[1]);
    let domain = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .cloned()
        .expect("win-b stays homed");
    assert_eq!(
        engine.tx(|tx| tx.effective_operation_focus(&domain, &WindowId(String::new()))),
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
    let (_focused, first) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(&first, "focus win-b executes");
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
                    window: crate::world_report::FocusDirectionalSubject::NoFocus,
                    direction: crate::directional::Direction::Right,
                },
            ),
            (2, focus_cmd("win-b")),
        ],
    );
    assert!(reply.focus_request.is_some());
    assert_accepted(&settlements, "batch with empty subject accepts");
    assert_eq!(settlements.len(), 2);
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
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
    // Seed a retained desire for an untouched live window (as a prior
    // proven-refusal rollback would have left it).
    engine.world.intent.intent_desired.insert(
        WindowId("win-b".to_owned()),
        Some(IntentData::new(Some(false), None)),
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[
            (
                1,
                WorldCommand::SetSticky {
                    window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                    value: true,
                },
            ),
            (2, focus_cmd("win-zz")),
            (3, focus_cmd("win-b")),
        ],
    );
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
    let mut wins: Vec<String> = reply
        .intent_desired
        .iter()
        .map(|write| write.win.0.clone())
        .collect();
    wins.sort();
    assert_eq!(
        wins,
        vec!["win-a".to_owned(), "win-b".to_owned()],
        "command save plus untouched rollback save publish exactly once: {:?}",
        reply.intent_desired
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
    let revision_before = engine.state_revision();
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
        engine.state_revision(),
        revision_before,
        "bootstrap advances no revision"
    );
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(2, windows(), "win-a"),
        &[(1, focus_cmd("win-b"))],
    );
    assert_accepted(
        &settlements,
        "pre-ready command drains on first usable capture",
    );
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
    assert_accepted(
        &settlements,
        "command drains on the native-carrying capture",
    );
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
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-b executes");
    assert!(has_focus_effect(&setup, "win-b"));
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (_caught, caught_settlements) =
        run_batched(&mut engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(
        &caught_settlements,
        "native fact catches up to the pre-batch focus",
    );
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
    assert_accepted(&settlements, "sequential directional batch accepts");
    assert_eq!(
        reply.state_revision,
        before.state_revision + 3,
        "setup plus catch-up plus batch advance three revisions"
    );
    assert_eq!(settlements.len(), 2);
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
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-a executes");
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (_reply, settlements) = run_batched(
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
    assert_accepted(&settlements, "focus then move batch accepts");
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
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-b executes");
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (_caught, caught_settlements) =
        run_batched(&mut engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(
        &caught_settlements,
        "native fact catches up to the pre-batch focus",
    );
    let mut queued: Vec<(u64, WorldCommand)> = Vec::new();
    for n in 2..=11u64 {
        let direction = if n % 2 == 0 { "right" } else { "left" };
        queued.push((n, implicit_focus_dir(direction)));
    }
    let (reply, settlements) = run_batched(&mut engine, &observe(4, windows(), "win-b"), &queued);
    assert!(reply.focus_request.is_some());
    assert_eq!(
        settlements.len(),
        10,
        "every directional settles exactly once"
    );
    for (index, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, index as u64 + 2, "settlements stay FIFO");
        assert_ready_settlement(settlement);
    }
    assert_accepted(&settlements, "alternating burst accepts");
}

fn implicit_toggle_float() -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::CurrentFocus,
        float_rect: None,
    })
}

fn explicit_toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::Window(WindowId(window.to_owned())),
        float_rect: None,
    })
}

fn implicit_set_sticky(value: bool) -> WorldCommand {
    WorldCommand::SetSticky {
        window: crate::directional::WindowSubject::CurrentFocus,
        value,
    }
}

fn float_write_pair(write: &IntentDesire) -> (String, IntentData) {
    (write.win.0.clone(), (*write.data.as_ref().expect("bytes")))
}

#[test]
fn bookkeeping_command_resolves_implicit_subject_once() {
    // The carried bookkeeping identity is the pre-execution explicit
    // subject: implicit maps once, explicit passes through, and an
    // unresolvable implicit falls back to the placeholder (no save arm
    // matches it, preserving mismatch/empty semantics).
    let mut engine = engine();
    engine.world.focus.logical_focus_cursor = Some(WindowId("win-a".to_owned()));
    assert_eq!(
        engine.tx(|tx| tx.bookkeeping_command(&implicit_toggle_float())),
        explicit_toggle_float("win-a")
    );
    assert_eq!(
        engine.tx(|tx| tx.bookkeeping_command(&explicit_toggle_float("win-b"))),
        explicit_toggle_float("win-b")
    );
    engine.world.focus.logical_focus_cursor = None;
    assert_eq!(
        engine.tx(|tx| tx.bookkeeping_command(&implicit_toggle_float())),
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
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-b executes");
    assert!(has_focus_effect(&setup, "win-b"));
    let (_caught, caught_settlements) =
        run_batched(&mut *engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(
        &caught_settlements,
        "native fact catches up to the pre-batch focus",
    );
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
    assert_accepted(&settlements, "float then focus batch accepts");
    assert_eq!(reply.intent_desired.len(), 1, "{:?}", reply.intent_desired);
    assert_eq!(
        float_write_pair(&reply.intent_desired[0]),
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
    assert_accepted(&settlements, "mixed float batch accepts");
    let mut pairs: Vec<(String, IntentData)> =
        reply.intent_desired.iter().map(float_write_pair).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pairs,
        vec![
            ("win-a".to_owned(), IntentData::new(Some(true), None)),
            ("win-b".to_owned(), IntentData::new(Some(true), None)),
        ],
        "both resolved saves publish: {:?}",
        reply.intent_desired
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
    // Seed a retained desire for an untouched live window (as a prior
    // proven-refusal rollback would have left it).
    engine.world.intent.intent_desired.insert(
        WindowId("win-b".to_owned()),
        Some(IntentData::new(Some(false), None)),
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
    let mut wins: Vec<String> = reply
        .intent_desired
        .iter()
        .map(|write| write.win.0.clone())
        .collect();
    wins.sort();
    assert_eq!(
        wins,
        vec!["win-a".to_owned(), "win-b".to_owned()],
        "implicit command save plus untouched rollback save publish exactly once: {:?}",
        reply.intent_desired
    );
    let sticky_write = reply
        .intent_desired
        .iter()
        .find(|write| write.win.0 == "win-a")
        .expect("resolved sticky save");
    assert_eq!(
        (*sticky_write.data.as_ref().expect("bytes")),
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
    assert_accepted(&settlements, "single implicit float batch accepts");
    assert_eq!(reply.intent_desired.len(), 1, "{:?}", reply.intent_desired);
    assert_eq!(
        float_write_pair(&reply.intent_desired[0]),
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
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-a executes");
    assert!(has_focus_effect(&setup, "win-a"));
    let (_caught, caught_settlements) =
        run_batched(&mut engine, &observe(3, windows(), "win-a"), &[]);
    assert_accepted(
        &caught_settlements,
        "native fact catches up to the pre-batch focus",
    );
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
    assert_accepted(&settlements, "implicit fixed unfloat accepts");
    match &settlements[0].result {
        Some(CommandResult::Float { window, .. }) => assert_eq!(
            window.0, "win-a",
            "tile result targets the resolved window: {:?}",
            settlements[0].result
        ),
        other => panic!("expected float result, got {other:?}"),
    }
    assert_eq!(reply.intent_desired.len(), 1, "{:?}", reply.intent_desired);
    assert_eq!(
        float_write_pair(&reply.intent_desired[0]),
        ("win-a".to_owned(), IntentData::new(Some(false), None)),
        "implicit fixed unfloat records the exact tile save"
    );
}

fn implicit_send_numbered_home() -> WorldCommand {
    WorldCommand::SendNumbered {
        window: crate::directional::WindowSubject::CurrentFocus,
        source: SendSource::HomeThenAction,
        // Use an explicit ordinal so sole-member Append policy cannot
        // mask the implicit subject/source routing under test.
        index: 2,
        follow: false,
    }
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
        engine.world.focus.logical_focus_cursor,
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
    assert_accepted(&settlements, "implicit send at retained home accepts");
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
        engine.world.focus.logical_focus_cursor,
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
    let (_focused, first_settlements) =
        run_batched(&mut engine, &capture, &[(2, focus_cmd("win-b"))]);
    assert_accepted(&first_settlements, "setup focus win-b executes");
    assert_eq!(first_settlements.len(), 1);
    assert_ready_settlement(&first_settlements[0]);
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-b".to_owned())),
        "post-prior core subject is win-b while staged legs stay out-a"
    );
    // The exact later-command refreeze call from the batch loop.
    let routing = crate::world_engine::RawRouting::from_staged(&engine.world.staged);
    let (frozen, _) = engine
        .tx_ro(|tx| tx.freeze_command_sources(Some(&implicit_send_numbered_home()), &routing));
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
    assert_accepted(
        &settlements,
        "implicit send at the post-native subject accepts",
    );
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
    assert_accepted(&settlements, "focus plus following implicit send accepts");
    assert!(
        engine.world.focus.batch_command_focus.is_empty(),
        "accepted finish clears batch-only provenance, never retained: {:?}",
        engine.world.focus.batch_command_focus
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
fn rapid_input_batched_preflight_sees_effective_queued_and_refuses_before_mutation() {
    // Format-neutral preflight gate proves `PreflightGateFacts.effective_queued`
    // receives the exact parked FIFO batch commands, preflight refusal leaves
    // revision/highwater/cached-reply/intent untouched with empty settlements,
    // and retry publishes the exact distinct saves. Protocol sizing bounds
    // live in `tiler-protocol/src/world_admission.rs`.
    use crate::world_engine::{
        AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
        FutureReserveGateFacts, PreflightGateFacts,
    };
    use std::cell::{Cell, RefCell};

    struct LocalBatchedPreflightGate {
        refuse_preflight: Cell<bool>,
        observed_queued: RefCell<Vec<(u64, WorldCommand)>>,
    }

    impl CoreAdmissionGate for LocalBatchedPreflightGate {
        fn check_request_size(&self, _report: &WorldReportV4) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }

        fn check_preflight(
            &self,
            _report: &WorldReportV4,
            facts: &PreflightGateFacts<'_>,
        ) -> AdmissionGateDecision {
            *self.observed_queued.borrow_mut() = facts.effective_queued.to_vec();
            if self.refuse_preflight.get() {
                AdmissionGateDecision::Refuse
            } else {
                AdmissionGateDecision::Allow
            }
        }

        fn check_exact_inner(&self, _reply: &WorldReplyV4) -> ExactInnerDecision {
            ExactInnerDecision::Allow(ExactInnerCheck::new(0))
        }

        fn check_future_reserve(
            &self,
            _reply: &WorldReplyV4,
            _facts: &FutureReserveGateFacts,
            _exact: ExactInnerCheck,
        ) -> AdmissionGateDecision {
            AdmissionGateDecision::Allow
        }
    }

    let mut eng = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    eng.handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let capture = observe(2, windows(), "win-a");
    let queued = vec![
        (1, explicit_toggle_float("win-a")),
        (2, explicit_toggle_float("win-b")),
    ];
    let revision_before = eng.state_revision();
    let highwater_before = eng.world.command_highwater;
    let cached_reply_before = eng.last_reply_v4().clone();
    let gate = LocalBatchedPreflightGate {
        refuse_preflight: Cell::new(true),
        observed_queued: RefCell::new(Vec::new()),
    };
    let res = eng
        .handle_report_v4_batched_with_gate_and_prepare(
            &capture,
            &issue_for(&capture),
            &queued,
            &gate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    match res.outcome {
        V4RunOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, crate::world_engine::KIND_REPLY_SIZE);
            assert!(
                res.dispositions
                    .iter()
                    .all(|d| d.disposition == crate::world_report::CommandDisposition::NotRun),
                "preflight refusal settles nothing; batch stays NotRun"
            );
        }
        other => panic!("expected reply-oversize refusal, got {other:?}"),
    }
    assert_eq!(*gate.observed_queued.borrow(), queued);
    assert_eq!(eng.state_revision(), revision_before);
    assert_eq!(eng.world.command_highwater, highwater_before);
    assert_eq!(eng.last_reply_v4(), cached_reply_before);
    assert!(eng.world.intent.intent_desired.is_empty());

    gate.refuse_preflight.set(false);
    let retry_res = eng
        .handle_report_v4_batched_with_gate_and_prepare(
            &capture,
            &issue_for(&capture),
            &queued,
            &gate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    let reply = match retry_res.outcome {
        V4RunOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted retry, got {other:?}"),
    };
    assert_eq!(retry_res.dispositions.len(), 2);
    assert!(matches!(
        retry_res.dispositions[0].disposition,
        crate::world_report::CommandDisposition::Executed(_)
    ));
    assert!(matches!(
        retry_res.dispositions[1].disposition,
        crate::world_report::CommandDisposition::Executed(_)
    ));
    let mut pairs: Vec<(String, IntentData)> =
        reply.intent_desired.iter().map(float_write_pair).collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        pairs,
        vec![
            ("win-a".to_owned(), IntentData::new(Some(true), None)),
            ("win-b".to_owned(), IntentData::new(Some(true), None)),
        ]
    );
}

#[test]
fn rapid_input_batch_rowcap_leaves_batch_unsettled() {
    // Fixed rowcap: the whole retained map plus genuinely new rows from the
    // inline command and the parked batch must fit MAX_INTENT_DESIRED.
    // Seed the retained map full with unrelated windows, then one queued
    // save for a new window must refuse before mutation with empty
    // settlements, leaving the batch unsettled for the caller (core
    // itself holds no broker queue).
    use crate::world_report::MAX_INTENT_DESIRED;
    let mut eng = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    eng.handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    for i in 0..MAX_INTENT_DESIRED {
        eng.world.intent.intent_desired.insert(
            WindowId(format!("win-rb-{i:05}")),
            Some(IntentData::new(Some(false), None)),
        );
    }
    assert_eq!(eng.world.intent.intent_desired.len(), MAX_INTENT_DESIRED);
    let queued = vec![(
        1,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                "win-a".to_owned(),
            )),
            float_rect: None,
        }),
    )];
    let revision_before = eng.state_revision();
    let highwater_before = eng.world.command_highwater;
    let cached_reply_before = eng.last_reply_v4().clone();
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
        eng.state_revision(),
        revision_before,
        "rowcap refusal advances no revision"
    );
    assert_eq!(
        eng.world.command_highwater, highwater_before,
        "rowcap refusal consumes no sequence"
    );
    assert_eq!(
        eng.world.intent.intent_desired.len(),
        MAX_INTENT_DESIRED,
        "rowcap refusal mutates no ledger"
    );
    assert_eq!(
        eng.last_reply_v4(),
        cached_reply_before,
        "rowcap refusal preserves the exact cached reply"
    );
}

fn implicit_resize_press0(direction: &str, mode: &str) -> WorldCommand {
    // Live-faithful press index: every resize in the 60-command burst carries
    // `press:0` (out/in alternate on the same key and focus changes reset the
    // shared key-repeat counter).
    WorldCommand::Operation(CoreCommand::Resize {
        window: crate::directional::WindowSubject::CurrentFocus,
        direction: crate::directional::Direction::parse_wire(direction)
            .expect("valid test direction"),
        mode: crate::directional::ResizeSelector::parse_wire(mode).expect("valid test mode"),
        press_index: 0,
    })
}

#[test]
fn rapid_input_capture_boundary_stale_native_keeps_implicit_subject() {
    // Capture-boundary invariant for the live 10..60 tail: after an in-batch
    // focus reversal lands core focus on the right window while the queued
    // native re-report stays on the left, the FIRST implicit command of the
    // NEXT capture still executes against the preserved unlanded core choice
    // (win-a). The execution-time subject is core state for every queued
    // command irrespective of capture boundary; an unchanged lagging staged
    // native never defeats it. A CHANGED (caught-up) native observation on
    // the same slot succeeds, and an explicit by-id focus on the same stale
    // native succeeds: the explicit seam is unaffected. Genuine no-focus and
    // edge refusals stay intact elsewhere.
    let mut engine = engine();
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, windows(), "win-a"))
        .expect("admission observes");
    let (setup, setup_settlements) = run_batched(
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
    assert_accepted(&setup_settlements, "setup focus win-b executes");
    assert!(has_focus_effect(&setup, "win-b"));
    // Named left/right prerequisite before command 1: the engine inserts
    // admitted clients into its own tiling order, so prove the plan (left
    // is win-b, right is win-a) rather than assuming raw rect order.
    assert_admitted_left_right(&setup, "win-b", "win-a");
    let (_caught, caught_settlements) =
        run_batched(&mut engine, &observe(3, windows(), "win-b"), &[]);
    assert_accepted(
        &caught_settlements,
        "native fact catches up to the pre-batch focus",
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-b".to_owned())),
        "precondition: slot, leaf and staged native all open on left win-b"
    );
    assert_eq!(
        engine.world.staged.observation.focused_global, "win-b",
        "precondition: staged native focus is left win-b"
    );
    // In-batch reversal: implicit focus-right moves left win-b to right
    // win-a against the matching staged native. The unlanded choice (slot
    // and session win-a) now leads the lagging staged native win-b.
    let (reversal, reversal_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[(4, implicit_focus_dir("right"))],
    );
    assert!(reversal.focus_request.is_some());
    assert_eq!(
        reversal_settlements.len(),
        1,
        "reversal settles exactly once"
    );
    assert_ready_settlement(&reversal_settlements[0]);
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned())),
        "reversal leaves the unlanded core choice on win-a while staged native lags win-b"
    );
    assert!(
        engine.world.focus.batch_command_focus.is_empty(),
        "accepted finish clears batch-only provenance: {:?}",
        engine.world.focus.batch_command_focus
    );
    // STALE leg: the next capture re-reports the unchanged native focus
    // win-b (no new information, so world_run_v4 keeps the slot win-a per
    // the unchanged-re-report rule). The first implicit command resolves
    // its subject from the slot (win-a) and the operation event carries the
    // slot as its focus; the engine syncs to win-a and `propose_resize`
    // plans. The resize grows leftwards into the neighbor: the right
    // window's outer right edge has no planning pair in this two-window
    // fixture, so left/outwards is the direction that plans (see the fresh
    // leg below).
    let (_stale_reply, stale_settlements) = run_batched(
        &mut engine,
        &observe(5, windows(), "win-b"),
        &[(5, implicit_resize_press0("left", "outwards"))],
    );
    assert_eq!(stale_settlements.len(), 1, "stale leg settles exactly once");
    assert_ready_settlement(&stale_settlements[0]);
    assert_accepted(
        &stale_settlements,
        "unchanged lagging native keeps the implicit subject",
    );
    assert_eq!(
        engine.world.focus.logical_focus_cursor,
        Some(WindowId("win-a".to_owned())),
        "unchanged re-report preserves the unlanded core choice win-a"
    );
    // EXPLICIT control on the same stale native: a by-id focus names its
    // own subject and never routes through the implicit slot/event-focus
    // match, so it still succeeds. The explicit seam is unaffected.
    let (explicit_reply, explicit_settlements) = run_batched(
        &mut engine,
        &observe(6, windows(), "win-b"),
        &[(6, focus_cmd("win-a"))],
    );
    assert!(explicit_reply.focus_request.is_some());
    assert_eq!(
        explicit_settlements.len(),
        1,
        "explicit control settles exactly once"
    );
    assert_ready_settlement(&explicit_settlements[0]);
    // CHANGED leg: the native fact catches up to the slot (genuine new
    // focus event for win-a). Same slot, same command shape as the stale
    // leg; now the event focus matches the subject and the resize plans.
    let (_fresh_reply, fresh_settlements) = run_batched(
        &mut engine,
        &observe(7, windows(), "win-a"),
        &[(7, implicit_resize_press0("left", "outwards"))],
    );
    assert_eq!(fresh_settlements.len(), 1, "fresh leg settles exactly once");
    assert_ready_settlement(&fresh_settlements[0]);
    assert_accepted(
        &fresh_settlements,
        "changed (caught-up) native lets the same implicit resize plan",
    );
}

#[test]
fn rapid_input_start_floating_unfloat_then_refloat_retains_moved_frame() {
    let mut engine = engine();
    let moved_rect = crate::geometry::Rect {
        x: 140,
        y: 120,
        w: 620,
        h: 420,
    };
    let initial = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    engine
        .handle_report_v4_for_test(&observe(1, initial(), "win-a"))
        .expect("admission observes");
    let (float_reply, float_settlements) = run_batched(
        &mut engine,
        &observe(2, initial(), "win-a"),
        &[(1, implicit_toggle_float())],
    );
    assert_accepted(&float_settlements, "initial float accepts");
    let centered_rect = float_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .and_then(|row| row.effective_frame)
        .expect("initial float target rect");

    let mut settled_a = fact("win-a", "out-a", 0);
    settled_a.rect = Some(centered_rect);
    let (_arrived_reply, arrived_settlements) = run_batched(
        &mut engine,
        &observe(3, vec![settled_a, fact("win-b", "out-a", 100)], "win-a"),
        &[],
    );
    assert_accepted(&arrived_settlements, "initial float arrives");

    let mut moved_a = fact("win-a", "out-a", 0);
    moved_a.rect = Some(moved_rect);
    let moved_windows = || vec![moved_a.clone(), fact("win-b", "out-a", 100)];
    let (_settled_reply, settled_settlements) =
        run_batched(&mut engine, &observe(4, moved_windows(), "win-a"), &[]);
    assert_accepted(&settled_settlements, "moved float settles");

    let unfloat_with_moved = WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: crate::directional::WindowSubject::CurrentFocus,
        float_rect: Some(moved_rect),
    });
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(5, moved_windows(), "win-a"),
        &[(2, unfloat_with_moved), (3, implicit_toggle_float())],
    );
    assert_eq!(settlements.len(), 2, "both toggles settle exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&settlements, "unfloat then re-float batch accepts");
    assert_eq!(
        settlements[0].result,
        Some(CommandResult::Float {
            window: WindowId("win-a".to_owned()),
            rect: None,
            no_space: false,
        })
    );
    assert_eq!(
        settlements[1].result,
        Some(CommandResult::Float {
            window: WindowId("win-a".to_owned()),
            rect: Some(moved_rect),
            no_space: false,
        })
    );
    let row_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row published");
    assert!(
        row_a.floating,
        "unfloat then re-float from floating must end floating: {row_a:?}"
    );
    assert_eq!(
        row_a.allocation, None,
        "floating row has no tile allocation"
    );
    assert_eq!(
        row_a.effective_frame,
        Some(moved_rect),
        "re-float must retain moved native frame: {row_a:?}"
    );
    let row_b = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row published");
    assert!(!row_b.floating, "win-b stays tiled: {row_b:?}");
    assert!(row_b.allocation.is_some(), "win-b keeps tile allocation");
    assert_eq!(
        reply.intent_desired.len(),
        1,
        "latest intent save for win-a publishes: {:?}",
        reply.intent_desired
    );
    assert_eq!(
        float_write_pair(&reply.intent_desired[0]),
        ("win-a".to_owned(), IntentData::new(Some(true), None))
    );
}

#[test]
fn rapid_input_tiled_float_then_unfloat_ends_tiled() {
    let mut engine = engine();
    setup_float_subject(&mut engine);
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let (reply, settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[(2, implicit_toggle_float()), (3, implicit_toggle_float())],
    );
    assert_eq!(settlements.len(), 2, "both toggles settle exactly once");
    assert_ready_settlement(&settlements[0]);
    assert_ready_settlement(&settlements[1]);
    assert_accepted(&settlements, "float then unfloat batch accepts");
    let row_a = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("win-a row published");
    let row_b = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row published");
    assert!(!row_a.floating && row_a.allocation.is_some(), "{row_a:?}");
    assert!(!row_b.floating && row_b.allocation.is_some(), "{row_b:?}");
    assert_eq!(
        reply.intent_desired.len(),
        1,
        "latest intent save for win-b publishes: {:?}",
        reply.intent_desired
    );
    assert_eq!(
        float_write_pair(&reply.intent_desired[0]),
        ("win-b".to_owned(), IntentData::new(Some(false), None))
    );
}

#[test]
fn rapid_input_single_command_float_and_unfloat_batches_unchanged() {
    let mut engine = engine();
    setup_float_subject(&mut engine);
    let windows = || vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let (float_reply, float_settlements) = run_batched(
        &mut engine,
        &observe(4, windows(), "win-b"),
        &[(2, implicit_toggle_float())],
    );
    assert_accepted(&float_settlements, "single float batch accepts");
    let row_b = float_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row published");
    assert!(row_b.floating && row_b.allocation.is_none(), "{row_b:?}");

    let moved_rect = crate::geometry::Rect {
        x: 180,
        y: 160,
        w: 580,
        h: 380,
    };
    let mut moved_b = fact("win-b", "out-a", 100);
    moved_b.rect = Some(moved_rect);
    let moved_windows = || vec![fact("win-a", "out-a", 0), moved_b.clone()];
    let (_settled_reply, settled_settlements) =
        run_batched(&mut engine, &observe(5, moved_windows(), "win-b"), &[]);
    assert_accepted(&settled_settlements, "moved float settles");

    let (unfloat_reply, unfloat_settlements) = run_batched(
        &mut engine,
        &observe(6, moved_windows(), "win-b"),
        &[(3, implicit_toggle_float())],
    );
    assert_accepted(&unfloat_settlements, "single unfloat batch accepts");
    let row_b_after = unfloat_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row published");
    assert!(
        !row_b_after.floating && row_b_after.allocation.is_some(),
        "{row_b_after:?}"
    );
}

fn cut_frame_roster() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

#[test]
fn cut_float_frame_comes_from_resolved_subject_until_its_float_state_changes() {
    use crate::directional::WindowSubject;
    use crate::geometry::Rect;
    use crate::world_report::FocusChanges;

    let retained = Rect {
        x: 120,
        y: 80,
        w: 400,
        h: 300,
    };
    let moved = Rect {
        x: 170,
        y: 90,
        w: 440,
        h: 320,
    };
    let explicit = Rect {
        x: 190,
        y: 100,
        w: 460,
        h: 340,
    };
    let roster = |frame| {
        let mut b = fact("win-b", "out-a", 100);
        b.rect = Some(frame);
        vec![fact("win-a", "out-a", 0), b]
    };
    let with_frame = |frame| {
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: WindowSubject::CurrentFocus,
            float_rect: Some(frame),
        })
    };
    for (case, sealed, fullscreen, toggles, expected) in [
        ("cut", true, false, vec![implicit_toggle_float()], moved),
        (
            "snapshot only",
            false,
            false,
            vec![implicit_toggle_float()],
            retained,
        ),
        (
            "explicit frame",
            true,
            false,
            vec![with_frame(explicit)],
            explicit,
        ),
        (
            "fullscreen",
            true,
            true,
            vec![implicit_toggle_float()],
            retained,
        ),
        (
            "already unfloated and re-floated",
            true,
            false,
            vec![
                implicit_toggle_float(),
                with_frame(explicit),
                implicit_toggle_float(),
            ],
            explicit,
        ),
    ] {
        let mut engine = engine();
        run_batched(&mut engine, &observe(1, cut_frame_roster(), "win-b"), &[]);
        let (_, setup) = run_batched(
            &mut engine,
            &observe(2, cut_frame_roster(), "win-b"),
            &[(1, with_frame(retained))],
        );
        assert_accepted(&setup, "initial float");
        let foreground = if fullscreen { "win-b" } else { "win-a" };
        let (arrived, _) = run_batched(&mut engine, &observe(3, roster(retained), foreground), &[]);
        let mut cut = observe(4, roster(moved), foreground);
        if fullscreen {
            cut.windows[1].fullscreen = Some(true);
            cut.windows[1].rect = Some(Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            });
        }
        if sealed {
            cut.focus_changes = Some(FocusChanges {
                after: Some(arrived.state_revision),
                events: Some(Vec::new()),
            });
        }
        // In the ordinary legs the observed foreground is A, but the FIFO
        // cursor advances to B before the first toggle.
        let batch: Vec<_> = (!fullscreen)
            .then(|| focus_cmd("win-b"))
            .into_iter()
            .chain(toggles)
            .chain(std::iter::once(implicit_toggle_float()))
            .zip(2..)
            .map(|(command, seq)| (seq, command))
            .collect();
        let (reply, settled) = run_batched(&mut engine, &cut, &batch);
        assert_accepted(&settled, case);
        assert_eq!(
            settled.last().unwrap().result,
            Some(CommandResult::Float {
                window: WindowId("win-b".to_owned()),
                rect: Some(expected),
                no_space: false,
            }),
            "{case}"
        );
        assert!(
            reply
                .windows
                .iter()
                .any(|row| row.lifetime.0 == "win-a" && !row.floating)
        );
    }
}

#[test]
fn cut_tiled_frame_becomes_first_float_geometry_in_place() {
    use crate::world_report::FocusChanges;

    let mut engine = engine();
    let (boot, _) = run_batched(&mut engine, &observe(1, cut_frame_roster(), "win-a"), &[]);
    let mut cut = observe(2, cut_frame_roster(), "win-a");
    cut.focus_changes = Some(FocusChanges {
        after: Some(boot.state_revision),
        events: Some(Vec::new()),
    });
    let (_, settled) = run_batched(
        &mut engine,
        &cut,
        &[
            (1, implicit_toggle_float()),
            (2, implicit_toggle_float()),
            (3, implicit_toggle_float()),
        ],
    );
    assert_accepted(&settled, "float, unfloat, re-float");
    let Some(CommandResult::Float {
        rect: Some(rect), ..
    }) = settled[0].result
    else {
        panic!("expected initial float frame, got {:?}", settled[0].result);
    };
    // A first float has no remembered placement: it lands at the tiled
    // subject's current frame, never the centered fallback.
    assert_eq!(Some(rect), cut.windows[0].rect);
    // The re-float after unfloat restores that same durable placement; the
    // pre-float cut frame is not re-adopted once the toggle changed state.
    assert_eq!(settled[2].result, settled[0].result);
}

#[test]
fn rectless_refloat_restores_placement_instead_of_new_observed_tile_frame() {
    let mut engine = engine();
    run_batched(&mut engine, &observe(1, cut_frame_roster(), "win-a"), &[]);
    let remembered = cut_frame_roster()[0].rect.expect("readable first frame");
    let expected = Some(CommandResult::Float {
        window: WindowId("win-a".to_owned()),
        rect: Some(remembered),
        no_space: false,
    });
    let (_, floated) = run_batched(
        &mut engine,
        &observe(2, cut_frame_roster(), "win-a"),
        &[(1, implicit_toggle_float())],
    );
    assert_accepted(&floated, "first float");
    assert_eq!(floated[0].result, expected);
    run_batched(&mut engine, &observe(3, cut_frame_roster(), "win-a"), &[]);

    let (unfloat_reply, unfloated) = run_batched(
        &mut engine,
        &observe(4, cut_frame_roster(), "win-a"),
        &[(2, implicit_toggle_float())],
    );
    assert_accepted(&unfloated, "unfloat");
    let tiled = unfloat_reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("subject row");
    assert!(!tiled.floating);
    let tile_frame = tiled.effective_frame.expect("tile frame after unfloat");
    assert_ne!(tile_frame, remembered);
    let mut windows = cut_frame_roster();
    windows[0].rect = Some(tile_frame);
    let (reply, refloated) = run_batched(
        &mut engine,
        &observe(5, windows, "win-a"),
        &[(3, implicit_toggle_float())],
    );
    assert_accepted(&refloated, "re-float after observing the new tile frame");
    assert_eq!(refloated[0].result, expected);
    let subject = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("subject row");
    assert!(subject.floating);
    assert_eq!(subject.effective_frame, Some(remembered));
}
