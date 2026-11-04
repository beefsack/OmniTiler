//! Accepted-report command rejection: observations incorporate, command
//! refuses with a typed result, sequence consumes, no partial mutation.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameAttempt, FrameSetter, ScopeAuthority, ScreenReport,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
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
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0)],
        windows,
        views: Vec::new(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        native: Vec::new(),
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
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
        intent_records: Vec::new(),
    }
}

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation")
}

fn row_window_ids(reply: &crate::world_report::WorldReply) -> Vec<String> {
    reply.windows.iter().map(|w| w.lifetime.0.clone()).collect()
}

fn sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        sticky: Some(true),
        ..fact(window, output, x)
    }
}

#[test]
fn rejected_focus_unknown_incorporates_observation_with_typed_result() {
    let mut engine = engine();
    // Bootstrap with a real later-closed lifetime so the close below is
    // genuine, not a vacuous unknown id.
    let base = match engine.handle_report(&report(
        1,
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            fact("wClose", "out-a", 200),
        ],
        "w1",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("base accepted, got {other:?}"),
    };
    let base_rev = base.state_revision;
    assert!(
        row_window_ids(&base).contains(&"wClose".to_owned()),
        "closed lifetime present at bootstrap: {:?}",
        row_window_ids(&base)
    );
    // Rejected command carries a newly observed window plus an explicit
    // close of a real lifetime, plus a mechanical frame attempt: all
    // pre-command observation must incorporate despite the refusal.
    let mut rejected = report(
        2,
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            fact("w2", "out-a", 300),
        ],
        "w1",
        vec!["wClose"],
        Some(2),
        Some(WorldCommand::Focus {
            window: WindowId("w-unknown".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    rejected.monotonic_ms = 1000;
    rejected.frames = vec![FrameAttempt {
        window: WindowId("w0".to_owned()),
        target: Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        },
        setter: FrameSetter::Accepted,
        readback: None,
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 1000,
        operation_id: None,
        prewrite: None,
    }];
    let reply = match engine.handle_report(&rejected) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("rejection must accept with typed result, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::FocusUnknown
        }),
        "typed rejected result"
    );
    assert!(
        reply.state_revision > base_rev,
        "issuing rev advances: {} <= {base_rev}",
        reply.state_revision
    );
    let ids = row_window_ids(&reply);
    assert!(
        ids.contains(&"w2".to_owned()),
        "newly observed window publishes: {ids:?}"
    );
    assert!(
        !ids.contains(&"wClose".to_owned()),
        "closed lifetime drops from published rows: {ids:?}"
    );
    assert!(
        !engine.engine_knows_window_any(&WindowId("wClose".to_owned())),
        "closed lifetime drops from sessions"
    );
    assert!(
        !engine.members.contains_key(&WindowId("wClose".to_owned())),
        "closed lifetime drops from membership"
    );
    let w2 = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w2")
        .expect("w2 row");
    assert!(
        w2.allocation.is_some(),
        "new window carries a real allocation: {w2:?}"
    );
    assert_eq!(
        w2.membership_authority,
        ScopeAuthority::Controlled,
        "new window carries Controlled membership: {w2:?}"
    );
    assert!(
        !reply.effects.iter().any(|e| e.target.0 == "w-unknown"),
        "no effect names the missing target: {:?}",
        reply.effects
    );
    assert!(
        !reply.command_consumed,
        "fresh rejection publishes Accepted, not consumed"
    );
    // Pre-command frame settlement is adopted despite the refusal.
    assert_eq!(
        engine
            .consumed_frames
            .get(&WindowId("w0".to_owned()))
            .map(|a| a.attempt_ms),
        Some(1000),
        "frame attempt settles pre-command"
    );
    // A commandless observation-only followup carries no result.
    match engine.handle_report(&report(
        3,
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            fact("w2", "out-a", 300),
        ],
        "w1",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(follow) => {
            assert_eq!(
                follow.command_result, None,
                "observation-only followup has no result"
            );
            assert!(
                row_window_ids(&follow).contains(&"w2".to_owned()),
                "followup still publishes w2: {:?}",
                row_window_ids(&follow)
            );
            assert!(
                !row_window_ids(&follow).contains(&"wClose".to_owned()),
                "close stays gone: {:?}",
                row_window_ids(&follow)
            );
        }
        other => panic!("commandless followup accepted, got {other:?}"),
    }
}

#[test]
fn rejected_seq_consumed_then_duplicate_replays() {
    let mut engine = engine();
    let base = match engine.handle_report(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("base accepted, got {other:?}"),
    };
    let rejected_report = report(
        2,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        vec![],
        Some(5),
        Some(WorldCommand::Focus {
            window: WindowId("w-ghost".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    let rejected_reply = match engine.handle_report(&rejected_report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("rejection accepts, got {other:?}"),
    };
    assert_eq!(
        rejected_reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::FocusUnknown
        })
    );
    // The refused command still publishes its observation: the
    // presentation is recomputed for this publication and labeled with
    // its own revision, advancing past the base.
    assert_eq!(
        rejected_reply.presentation.rev,
        rejected_reply.state_revision
    );
    assert_ne!(
        rejected_reply.presentation.rev, base.presentation.rev,
        "rejected-but-published advances the label"
    );
    let mut published = String::new();
    crate::world_sizing::encode_presentation(&mut published, &rejected_reply.presentation);
    // Exact duplicate replays the cached reply with no new revision.
    match engine.handle_report(&rejected_report) {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.state_revision, rejected_reply.state_revision);
            assert_eq!(cached.command_result, rejected_reply.command_result);
            assert_eq!(cached.presentation.rev, rejected_reply.presentation.rev);
            let mut replayed = String::new();
            crate::world_sizing::encode_presentation(&mut replayed, &cached.presentation);
            assert_eq!(published, replayed, "duplicate replays identical bytes");
        }
        other => panic!("exact repeat replays duplicate, got {other:?}"),
    }
    // Same consumed command sequence with fresh observation reconciles
    // only: no re-execution, no result re-emission.
    match engine.handle_report(&report(
        3,
        vec![
            fact("w0", "out-a", 0),
            fact("w1", "out-a", 100),
            fact("w2", "out-a", 200),
        ],
        "w0",
        vec![],
        Some(5),
        Some(WorldCommand::Focus {
            window: WindowId("w-ghost".to_owned()),
            direction: "left".to_owned(),
        }),
    )) {
        WorldOutcome::CommandConsumed(reply) => {
            assert_eq!(reply.command_result, None, "consumed emits no result");
            assert!(
                row_window_ids(&reply).contains(&"w2".to_owned()),
                "fresh obs still incorporates: {:?}",
                row_window_ids(&reply)
            );
            // Consumed reconciliations publish too: fresh observation,
            // fresh label on the consumed revision.
            assert_eq!(reply.presentation.rev, reply.state_revision);
        }
        other => panic!("consumed seq reconciles, got {other:?}"),
    }
}

#[test]
fn noncatalog_legacy_refusal_normalizes_to_operation_refused_without_mutation() {
    let mut engine = engine();
    match engine.handle_report(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Unknown-window local move surfaces a legacy session refusal string
    // (unknown-window), which is not in the closed command catalog.
    let reply = match engine.handle_report(&report(
        2,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Move {
            window: "w-ghost".to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
        })),
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("legacy refusal must accept with mapping, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::OperationRefused
        }),
        "non-catalog maps to operation-refused"
    );
    assert!(
        reply.transfers.is_empty(),
        "no transfer minted on refusal: {:?}",
        reply.transfers
    );
    assert!(
        reply.effects.is_empty() || !reply.effects.iter().any(|e| e.target.0 == "w-ghost"),
        "no effect for the unknown subject: {:?}",
        reply.effects
    );
    assert!(
        row_window_ids(&reply).contains(&"w1".to_owned()),
        "observation still incorporates: {:?}",
        row_window_ids(&reply)
    );
}

#[test]
fn creation_exhaustion_stays_envelope_without_adoption() {
    let mut engine = engine();
    let _base = match engine.handle_report(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("base accepted, got {other:?}"),
    };
    engine.creation_counter = u64::MAX;
    // Occupy the live trailing empty so the next trailing selection must
    // create (the exhaustion path) instead of reusing.
    match engine.handle_report(&report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(10),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("trailing reuse accepted, got {other:?}"),
    }
    match engine.handle_report(&report(
        3,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(11),
        Some(WorldCommand::Send {
            window: WindowId("w0".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-2".to_owned(),
            follow: true,
        }),
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("send to trailing accepted, got {other:?}"),
    }
    let rev_before = match engine.handle_report(&report(
        4,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(reply) => reply.state_revision,
        other => panic!("observation base accepted, got {other:?}"),
    };
    let highwater_before = engine.command_highwater;
    let revision_before = engine.state_revision;
    match engine.handle_report(&report(
        5,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(12),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
        other => panic!("creation exhaustion stays envelope, got {other:?}"),
    }
    // Envelope rejects adopt nothing: revision, command watermark, and
    // cached reply all stay exactly as before.
    assert_eq!(
        engine.state_revision, revision_before,
        "envelope mutates no revision"
    );
    assert_eq!(
        engine.command_highwater, highwater_before,
        "envelope consumes no sequence"
    );
    assert_eq!(
        engine.last_reply.as_ref().map(|reply| reply.state_revision),
        Some(rev_before),
        "envelope publishes no reply"
    );
    // No adoption: the next fresh command sequence still executes.
    let fresh = report(
        6,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        vec![],
        Some(12),
        Some(WorldCommand::Focus {
            window: WindowId("w-ghost".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    match engine.handle_report(&fresh) {
        WorldOutcome::Accepted(reply) => {
            assert_eq!(
                reply.state_revision,
                rev_before + 1,
                "next accepted revision is exactly one past the envelope base"
            );
            assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::FocusUnknown
                })
            );
        }
        other => panic!("post-envelope sequence still fresh, got {other:?}"),
    }
}

#[test]
fn precommand_view_transfer_survives_rejection() {
    let mut engine = engine();
    match engine.handle_report(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Establish a retained view transfer (observation, not a command
    // result): switching to the trailing empty retains op-2.
    match engine.handle_report(&report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::SwitchView {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-2".to_owned(),
        }),
    )) {
        WorldOutcome::Accepted(reply) => {
            assert!(
                reply.transfers.iter().any(|t| t.operation_id == "op-2"),
                "view flight retained: {:?}",
                reply.transfers
            );
        }
        other => panic!("view switch accepted, got {other:?}"),
    }
    // A later rejected command must keep the pre-command transfer and
    // still incorporate the fresh observation.
    let reply = match engine.handle_report(&report(
        3,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        vec![],
        Some(3),
        Some(WorldCommand::Focus {
            window: WindowId("w-ghost".to_owned()),
            direction: "left".to_owned(),
        }),
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("rejection accepts, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::FocusUnknown
        })
    );
    assert!(
        reply.transfers.iter().any(|t| t.operation_id == "op-2"),
        "pre-command view flight survives: {:?}",
        reply.transfers
    );
    assert!(
        row_window_ids(&reply).contains(&"w1".to_owned()),
        "fresh obs incorporates: {:?}",
        row_window_ids(&reply)
    );
}

#[test]
fn sticky_trailing_create_refusal_rolls_back_creation() {
    let mut engine = engine();
    match engine.handle_report(&report(
        1,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        None,
        None,
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Occupy the trailing empty so the numbered trailing send must create.
    match engine.handle_report(&report(
        2,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("trailing reuse accepted, got {other:?}"),
    }
    match engine.handle_report(&report(
        3,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        Some(3),
        Some(WorldCommand::Send {
            window: WindowId("w0".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-2".to_owned(),
            follow: true,
        }),
    )) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("send to trailing accepted, got {other:?}"),
    }
    let retained_before = engine.retained.len();
    assert!(
        engine
            .ring_occupied(&OutputId("out-a".to_owned()))
            .contains("ws-2"),
        "trailing occupied so the numbered send must take the Create path"
    );
    // Commandless control with the same observation: the rejected run
    // must converge identically (same lifecycle trailing, same flights)
    // apart from the consumed sequence and typed result.
    let mut control = engine.clone();
    let control_obs = report(
        4,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        None,
        None,
    );
    match control.handle_report(&control_obs) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("control accepted, got {other:?}"),
    }
    // send-refused originates only in execute_send, after the trailing
    // create in start_numbered_send (pre-create checks never return it),
    // so this result proves the creation was attempted then rolled back.
    let reply = match engine.handle_report(&report(
        4,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        Some(4),
        Some(WorldCommand::SendNumbered {
            window: WindowId("wSticky".to_owned()),
            source: crate::world_report::SendSource::Explicit(OutputId("out-a".to_owned())),
            index: 0,
            follow: false,
        }),
    )) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("sticky refusal must accept with mapping, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendRefused
        }),
        "sticky send refuses with its catalog kind"
    );
    assert_eq!(
        engine.mapping, control.mapping,
        "trailing creation rolls back to control: {:?} vs {:?}",
        engine.mapping, control.mapping
    );
    assert_eq!(
        engine.creation_counter, control.creation_counter,
        "creation counter matches control"
    );
    assert_eq!(
        engine.retained.len(),
        retained_before,
        "no transfer minted for the refused mover: {:?}",
        engine.retained
    );
    assert!(
        !engine
            .retained
            .iter()
            .any(|r| matches!(&r.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "wSticky")),
        "refused sticky leaves no flight: {:?}",
        engine.retained
    );
}
