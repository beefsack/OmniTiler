//! Accepted-report command rejection: observations incorporate, command
//! refuses with a typed result, sequence consumes, no partial mutation.

use std::collections::BTreeSet;

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::size_hints::WindowSizeHints;
use crate::world_commands::CommandApplied;
use crate::world_engine::{EventIssue, LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameAttempt, FrameSetter, PresentationIngressV4,
    PresentationRowV4, ScopeAuthority, ScreenReport, WORLD_CONTRACT_VERSION_LEGACY,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReport, WorldReportV4,
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
        contract_version: WORLD_CONTRACT_VERSION_LEGACY,
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
    let base = match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    let reply = match engine.handle_report_legacy(&rejected) {
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(follow) => {
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
    let base = match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    let rejected_reply = match engine.handle_report_legacy(&rejected_report) {
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    match engine.handle_report_legacy(&rejected_report) {
        LegacyWorldOutcome::Duplicate(cached) => {
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
    match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::CommandConsumed(reply) => {
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
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Unknown-window local move surfaces a legacy session refusal string
    // (unknown-window), which is not in the closed command catalog.
    let reply = match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    let _base = match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("base accepted, got {other:?}"),
    };
    engine.creation_counter = u64::MAX;
    // Occupy the live trailing empty so the next trailing selection must
    // create (the exhaustion path) instead of reusing.
    match engine.handle_report_legacy(&report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(10),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("trailing reuse accepted, got {other:?}"),
    }
    match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("send to trailing accepted, got {other:?}"),
    }
    let rev_before = match engine.handle_report_legacy(&report(
        4,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(reply) => reply.state_revision,
        other => panic!("observation base accepted, got {other:?}"),
    };
    let highwater_before = engine.command_highwater;
    let revision_before = engine.state_revision;
    match engine.handle_report_legacy(&report(
        5,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(12),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        LegacyWorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
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
    match engine.handle_report_legacy(&fresh) {
        LegacyWorldOutcome::Accepted(reply) => {
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
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Establish a retained view transfer (observation, not a command
    // result): switching to the trailing empty retains op-2.
    match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(reply) => {
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
    let reply = match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(reply) => reply,
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
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Occupy the trailing empty so the numbered trailing send must create.
    match engine.handle_report_legacy(&report(
        2,
        vec![fact("w0", "out-a", 0), sticky_fact("wSticky", "out-a", 100)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::SelectTrailing { output: None }),
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("trailing reuse accepted, got {other:?}"),
    }
    match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(_) => {}
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
    match control.handle_report_legacy(&control_obs) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("control accepted, got {other:?}"),
    }
    // send-refused originates only in execute_send, after the trailing
    // create in start_numbered_send (pre-create checks never return it),
    // so this result proves the creation was attempted then rolled back.
    let reply = match engine.handle_report_legacy(&report(
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
        LegacyWorldOutcome::Accepted(reply) => reply,
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

#[test]
fn keyboard_resize_unknown_subject_stays_operation_refused() {
    let mut engine = engine();
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // Unknown-subject keyboard resize surfaces a legacy session refusal
    // string (unknown-window), which is not in the closed command catalog.
    let reply = match engine.handle_report_legacy(&report(
        2,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "w-ghost".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    )) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("legacy refusal must accept with mapping, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::OperationRefused
        }),
        "non-catalog resize maps to operation-refused"
    );
    assert!(
        reply.transfers.is_empty(),
        "no transfer minted on refusal: {:?}",
        reply.transfers
    );
    assert!(
        row_window_ids(&reply).contains(&"w1".to_owned()),
        "observation still incorporates: {:?}",
        row_window_ids(&reply)
    );
}

#[test]
fn keyboard_resize_singleton_no_neighbor_stays_operation_refused() {
    let mut engine = engine();
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // A singleton tile has no resize neighbor in any direction: the
    // legitimate refusal keeps the generic wire kind without mutation.
    let reply = match engine.handle_report_legacy(&report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "w0".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    )) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("legacy refusal must accept with mapping, got {other:?}"),
    };
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::OperationRefused
        }),
        "no-neighbor resize maps to operation-refused"
    );
    assert!(
        reply.transfers.is_empty(),
        "no transfer minted on refusal: {:?}",
        reply.transfers
    );
    assert!(
        row_window_ids(&reply).contains(&"w0".to_owned()),
        "observation still incorporates: {:?}",
        row_window_ids(&reply)
    );
}

#[test]
fn resize_refusal_diagnostic_none_for_non_resize() {
    let engine = engine();
    let plain = report(7, vec![fact("w0", "out-a", 0)], "w0", vec![], None, None);
    assert_eq!(engine.resize_refusal_diagnostic(&plain, "unchanged"), None);
    let focus = report(
        7,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(9),
        Some(WorldCommand::Focus {
            window: WindowId("w0".to_owned()),
            direction: "left".to_owned(),
        }),
    );
    assert_eq!(
        engine.resize_refusal_diagnostic(&focus, "focus-unknown"),
        None
    );
}

#[test]
fn resize_refusal_diagnostic_sanitizes_injection() {
    let engine = engine();
    let mut resize = report(
        7,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(9),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "w\n0".to_owned(),
            direction: "LEFT".to_owned(),
            mode: "out\nwards".to_owned(),
            press_index: 0,
        })),
    );
    resize.request_id = "evil\nid".to_owned();
    let line = engine
        .resize_refusal_diagnostic(&resize, "weird kind!")
        .expect("diagnostic");
    assert!(!line.contains('\n'), "no log injection: {line:?}");
    for token in ["req=-", "subject=-", "dir=-", "mode=-", "internal=-"] {
        assert!(line.contains(token), "sanitized to {token}: {line}");
    }
    assert!(
        !line.contains("evil") && !line.contains("weird"),
        "raw injected text never echoes: {line}"
    );
}

#[test]
fn resize_refusal_diagnostic_reports_actual_internal_cause_with_context() {
    let mut engine = engine();
    match engine.handle_report_legacy(&report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        None,
        None,
    )) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    // The verbatim internal cause comes from the command seam itself,
    // never from a hand-picked test string.
    let command = WorldCommand::Operation(CoreCommand::Resize {
        window: "w0".to_owned(),
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        press_index: 0,
    });
    let issue = EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse("req-1").expect("correlation"),
    };
    let kind = match engine.execute_command(&issue, 2, &command, &BTreeSet::new(), None, None) {
        CommandApplied::Rejected { kind, .. } => kind,
        CommandApplied::NoEffect => panic!("singleton resize refuses, got NoEffect"),
        CommandApplied::Effect(_) => panic!("singleton resize refuses, got Effect"),
    };
    let probe = report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(2),
        Some(command),
    );
    let line = engine
        .resize_refusal_diagnostic(&probe, kind)
        .expect("diagnostic");
    let wire = CommandRejectKind::parse(kind)
        .unwrap_or(CommandRejectKind::OperationRefused)
        .as_str();
    assert!(
        line.contains(&format!("internal={kind}")),
        "actual cause: {line}"
    );
    assert!(
        line.contains(&format!("wire={wire}")),
        "honest mapping: {line}"
    );
    for token in [
        "subject=w0",
        "dir=left",
        "mode=outwards",
        "homing=out-a/",
        "subjgeom=0,0,40x90",
        "subjhints=min-x-/max-x-",
        "w0@0,0,40x90/min-x-/max-x-",
        "rowsTotal=1 rowsShown=1",
    ] {
        assert!(line.contains(token), "staged context {token}: {line}");
    }
    assert!(
        !line.contains("focus=-") && !line.contains("tree=-"),
        "retained focus renders: {line}"
    );
    assert!(
        !line.contains('\n') && line.len() <= 960,
        "bounded line: {line:?}"
    );
    assert!(
        !line.contains("synthetic.exe") && !line.contains("foreign-pid"),
        "no process metadata: {line}"
    );
}

fn staged_domain_rows(engine: &WorldEngine, window: &str) -> usize {
    let key = engine
        .staged
        .homing
        .get(&WindowId(window.to_owned()))
        .expect("homed subject")
        .clone();
    engine
        .staged
        .domains
        .iter()
        .find(|d| d.key == key)
        .map(|d| d.windows.len())
        .unwrap_or(0)
}

#[test]
fn resize_refusal_diagnostic_counts_row_truncation() {
    let mut engine = engine();
    let ids: Vec<String> = (0..6).map(|i| format!("w{i}")).collect();
    let windows: Vec<WindowFact> = ids
        .iter()
        .enumerate()
        .map(|(i, w)| fact(w, "out-a", (i as i32) * 100))
        .collect();
    match engine.handle_report_legacy(&report(1, windows, "w0", vec![], None, None)) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    let total = staged_domain_rows(&engine, "w0");
    let probe = report(
        2,
        ids.iter()
            .enumerate()
            .map(|(i, w)| fact(w, "out-a", (i as i32) * 100))
            .collect(),
        "w0",
        vec![],
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "w0".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    );
    let line = engine
        .resize_refusal_diagnostic(&probe, "unchanged")
        .expect("diagnostic");
    assert!(line.contains(&format!("rowsTotal={total}")));
    assert!(line.contains(&format!("rowsShown={}", total.min(4))));
    assert!(line.len() <= 960);
    if total > 4 {
        assert!(line.contains(&format!("+{}-more", total - 4)));
        assert!(
            !line.ends_with("..."),
            "count cut needs no byte cut: {line}"
        );
    }
}

#[test]
fn resize_refusal_diagnostic_byte_truncates_long_ids() {
    // 127-char opaque ids plus a 128-char request force the byte cap with
    // staged rows present.
    let mut engine = engine();
    let ids: Vec<String> = (0..6)
        .map(|i| format!("w{i}-{}", "p".repeat(124)))
        .collect();
    let windows: Vec<WindowFact> = ids
        .iter()
        .enumerate()
        .map(|(i, w)| fact(w, "out-a", (i as i32) * 100))
        .collect();
    match engine.handle_report_legacy(&report(1, windows, &ids[0], vec![], None, None)) {
        LegacyWorldOutcome::Accepted(_) => {}
        other => panic!("base accepted, got {other:?}"),
    }
    let total = staged_domain_rows(&engine, &ids[0]);
    let mut probe = report(
        2,
        ids.iter()
            .enumerate()
            .map(|(i, w)| fact(w, "out-a", (i as i32) * 100))
            .collect(),
        &ids[0],
        vec![],
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: ids[0].clone(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    );
    probe.request_id = "r".repeat(128);
    let line = engine
        .resize_refusal_diagnostic(&probe, "pair-below-minimum")
        .expect("diagnostic");
    assert_eq!(line.len(), 960, "byte cap is exact");
    assert!(line.ends_with("..."), "cut marker: {line:?}");
    assert!(!line.contains('\n'));
    assert!(
        line.contains(&format!("rowsTotal={total}")),
        "counts precede the cut"
    );
    assert!(line.contains("internal=pair-below-minimum"));
}

fn v4ingress(fg: &str) -> PresentationIngressV4 {
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: if fg.is_empty() {
            None
        } else {
            Some(rect(0, 0, 40, 90))
        },
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

fn v4report(
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
        presentation: PresentationRowV4::Valid(v4ingress(focused)),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[test]
fn keyboard_resize_singleton_no_neighbor_v4_stays_operation_refused() {
    let mut engine = engine();
    match engine.handle_report_v4_for_test(&v4report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        None,
        None,
    )) {
        Ok(_) => {}
        Err((kind, message)) => panic!("v4 base accepted, got {kind}: {message}"),
    }
    match engine.handle_report_v4_for_test(&v4report(
        2,
        vec![fact("w0", "out-a", 0)],
        "w0",
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::Resize {
            window: "w0".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        })),
    )) {
        Ok(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "v4 no-neighbor resize maps to operation-refused"
        ),
        Err((kind, message)) => panic!("v4 refusal maps in-band, got {kind}: {message}"),
    }
}

fn v4issue(report: &WorldReportV4) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

fn v4verbatim_kind(engine: &mut WorldEngine, seq: u64, command: &WorldCommand) -> &'static str {
    // The verbatim internal cause comes from the command seam itself, never
    // from a hand-picked test string (same pattern as the legacy diagnostic
    // test above).
    match engine.execute_command(
        &v4issue(&v4report(
            seq,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w0",
            None,
            None,
        )),
        seq,
        command,
        &BTreeSet::new(),
        None,
        None,
    ) {
        CommandApplied::Rejected { kind, .. } => kind,
        CommandApplied::NoEffect => panic!("expected refusal, got NoEffect"),
        CommandApplied::Effect(_) => panic!("expected refusal, got Effect"),
    }
}

#[test]
fn v4_queued_resize_refusal_keeps_executing_identity_and_internal_kind() {
    let mut engine = engine();
    match engine.handle_report_v4_for_test(&v4report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        None,
        None,
    )) {
        Ok(_) => {}
        Err((kind, message)) => panic!("v4 base accepted, got {kind}: {message}"),
    }
    let command = WorldCommand::Operation(CoreCommand::Resize {
        window: "w0".to_owned(),
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        press_index: 0,
    });
    let kind = v4verbatim_kind(&mut engine, 2, &command);
    // The queued batch shape carries no inline command; the per-command
    // diagnostics bind the executing identity anyway (never the absent
    // report pair).
    let probe = v4report(2, vec![fact("w0", "out-a", 0)], "w0", None, None);
    assert_eq!(
        probe.command, None,
        "queued batch shape carries no inline command"
    );
    // The drained batch settles the queued command under its own sequence
    // with a typed refusal.
    let queued = vec![(9, command.clone())];
    let (outcome, settlements) = engine.handle_report_v4_batched(&probe, &v4issue(&probe), &queued);
    let wire_kind = CommandRejectKind::parse(kind).unwrap_or(CommandRejectKind::OperationRefused);
    match outcome {
        crate::world_run_v4::V4RunOutcome::Accepted(_) => {}
        other => panic!("batch accepted, got {other:?}"),
    }
    assert_eq!(settlements.len(), 1, "one queued command settles once");
    assert_eq!(
        settlements[0].seq, 9,
        "settlement keeps the queued sequence"
    );
    assert_eq!(
        settlements[0].result,
        Some(CommandResult::Rejected { kind: wire_kind }),
        "settlement carries the mapped wire kind"
    );
    // The per-command diagnostic joins that same sequence to the resolved
    // bookkeeping identity and the verbatim internal kind (not just the
    // mapped wire token).
    let bookkeeping = engine.bookkeeping_command(&command);
    let line = engine.command_refusal_diagnostic_v4(&probe, &bookkeeping, 9, kind);
    let wire = wire_kind.as_str();
    for token in [
        "cmdseq=9",
        "op=resize",
        "subject=w0",
        "dir=left",
        &format!("internal={kind}"),
        &format!("wire={wire}"),
    ] {
        assert!(line.contains(token), "executing identity {token}: {line}");
    }
    assert!(
        !line.contains('\n') && line.len() <= 512,
        "bounded line: {line:?}"
    );
    // The queued resize keeps its staged-context detail line too, bound to
    // the executing sequence instead of the absent report pair.
    let detail = engine
        .resize_refusal_diagnostic_v4_for_command(&probe, &bookkeeping, 9, kind)
        .expect("queued resize keeps detail");
    for token in [
        "cmdseq=9",
        "subject=w0",
        &format!("internal={kind}"),
        "homing=out-a/",
        "subjgeom=0,0,40x90",
    ] {
        assert!(detail.contains(token), "staged context {token}: {detail}");
    }
    assert!(
        !detail.contains("synthetic.exe") && !detail.contains("foreign-pid"),
        "no process metadata: {detail:?}"
    );
}

#[test]
fn v4_command_refusal_diagnostic_covers_focus_and_move_with_actual_kind() {
    let mut engine = engine();
    match engine.handle_report_v4_for_test(&v4report(
        1,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        None,
        None,
    )) {
        Ok(_) => {}
        Err((kind, message)) => panic!("v4 base accepted, got {kind}: {message}"),
    }
    let focus = WorldCommand::Focus {
        window: WindowId("w-ghost".to_owned()),
        direction: String::new(),
    };
    // A ghost-window local move stays production-reachable (the subject
    // closes between park and drain) and refuses through the operation
    // route instead of the directional NoEffect-with-cause lane.
    let mov = WorldCommand::Operation(CoreCommand::Move {
        window: "w-ghost".to_owned(),
        direction: "left".to_owned(),
        cross_output_transfer: false,
        same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
    });
    let focus_kind = v4verbatim_kind(&mut engine, 2, &focus);
    let move_kind = v4verbatim_kind(&mut engine, 3, &mov);
    let probe = v4report(
        2,
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w0",
        None,
        None,
    );
    let queued = vec![(7, focus.clone()), (8, mov.clone())];
    let (outcome, settlements) = engine.handle_report_v4_batched(&probe, &v4issue(&probe), &queued);
    match outcome {
        crate::world_run_v4::V4RunOutcome::Accepted(_) => {}
        other => panic!("batch accepted, got {other:?}"),
    }
    assert_eq!(settlements.len(), 2, "every queued command settles once");
    for settlement in &settlements {
        assert!(
            matches!(settlement.result, Some(CommandResult::Rejected { .. })),
            "seq {} settles refused; got {:?}",
            settlement.seq,
            settlement.result
        );
    }
    for (seq, command, kind) in [(7, &focus, focus_kind), (8, &mov, move_kind)] {
        let bookkeeping = engine.bookkeeping_command(command);
        let line = engine.command_refusal_diagnostic_v4(&probe, &bookkeeping, seq, kind);
        let wire = CommandRejectKind::parse(kind)
            .unwrap_or(CommandRejectKind::OperationRefused)
            .as_str();
        for token in [
            format!("cmdseq={seq}"),
            "subject=w-ghost".to_owned(),
            format!("internal={kind}"),
            format!("wire={wire}"),
        ] {
            assert!(line.contains(&token), "refusal joins seq {seq}: {line}");
        }
        assert!(
            !line.contains('\n') && line.len() <= 512,
            "bounded line: {line:?}"
        );
    }
    let focus_line = engine.command_refusal_diagnostic_v4(
        &probe,
        &engine.bookkeeping_command(&focus),
        7,
        focus_kind,
    );
    assert!(focus_line.contains("op=focus"), "focus op: {focus_line}");
    let move_line = engine.command_refusal_diagnostic_v4(
        &probe,
        &engine.bookkeeping_command(&mov),
        8,
        move_kind,
    );
    assert!(
        move_line.contains("op=move") && move_line.contains("dir=left"),
        "move op and direction: {move_line}"
    );
    // Non-resize commands keep no resize detail line.
    assert_eq!(
        engine.resize_refusal_diagnostic_v4_for_command(
            &probe,
            &engine.bookkeeping_command(&focus),
            7,
            focus_kind
        ),
        None,
        "focus keeps the generic line only"
    );
}

#[test]
fn v4_command_refusal_diagnostic_is_safe_bounded_and_pure() {
    let mut engine = engine();
    match engine.handle_report_v4_for_test(&v4report(
        1,
        vec![fact("w0", "out-a", 0)],
        "w0",
        None,
        None,
    )) {
        Ok(_) => {}
        Err((kind, message)) => panic!("v4 base accepted, got {kind}: {message}"),
    }
    let mut probe = v4report(2, vec![fact("w0", "out-a", 0)], "w0", None, None);
    probe.request_id = "evil\nid".to_owned();
    let hostile = WorldCommand::Focus {
        window: WindowId("w\n0".to_owned()),
        direction: "LEFT".to_owned(),
    };
    let line = engine.command_refusal_diagnostic_v4(&probe, &hostile, 11, "weird kind!");
    assert!(!line.contains('\n'), "no log injection: {line:?}");
    for token in [
        "req=-",
        "cmdseq=11",
        "op=focus",
        "subject=-",
        "dir=-",
        "internal=-",
        "wire=operation-refused",
    ] {
        assert!(line.contains(token), "sanitized to {token}: {line}");
    }
    assert!(
        !line.contains("evil")
            && !line.contains("weird")
            && !line.contains("synthetic.exe")
            && !line.contains("foreign-pid"),
        "raw injected text and process metadata never echo: {line:?}"
    );
    assert!(line.len() <= 512, "bounded line: {line:?}");
    // Pure formatting: repeated calls agree and engine state is untouched,
    // so the surviving batch settles exactly as it would without the probe.
    let again = engine.command_refusal_diagnostic_v4(&probe, &hostile, 11, "weird kind!");
    assert_eq!(line, again, "diagnostic is deterministic");
    let revision_before = engine.state_revision();
    let clean = v4report(2, vec![fact("w0", "out-a", 0)], "w0", None, None);
    let resize = WorldCommand::Operation(CoreCommand::Resize {
        window: "w0".to_owned(),
        direction: "left".to_owned(),
        mode: "outwards".to_owned(),
        press_index: 0,
    });
    let queued = vec![(12, resize)];
    let (outcome, settlements) = engine.handle_report_v4_batched(&clean, &v4issue(&clean), &queued);
    match outcome {
        crate::world_run_v4::V4RunOutcome::Accepted(_) => {}
        other => panic!("batch accepted, got {other:?}"),
    }
    assert_eq!(settlements.len(), 1);
    assert!(
        matches!(settlements[0].result, Some(CommandResult::Rejected { .. })),
        "diagnostic probes change no outcome: {:?}",
        settlements[0].result
    );
    assert!(
        engine.state_revision() > revision_before,
        "only the batch advances revision"
    );
}
