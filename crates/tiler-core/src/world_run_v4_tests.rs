//! Focused genuine internal v4 accepted-run/finish and command dispatch
//! regressions: typed v4 observation and command runs on an already admitted
//! candidate, never through public ingress or the legacy runtime. No legacy
//! accepted-run seeds below; every run here is a pure v4 internal run.

use crate::boundary::CoreCommand;
use crate::connection::{HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::intent_data::{IntentData, encode_intent_data};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    AttemptOutcome, AuthorityScope, BatchSettlement, CommandRejectKind, CommandResult,
    FrameAttempt, FrameSetter, HeldPresentationRow, HeldWindowRow, IntentRecord, NativeObservation,
    NativeOccupancy, NativeWorkspaceFact, PresentationIngressV4, PresentationPagerFrameV4,
    PresentationRowV4, RenderTarget, RendererInput, RowDiagnosticKind, ScopeAuthority,
    ScreenReport, SendSource, StateAttempt, StateProperty, WORLD_CONTRACT_VERSION_V4, WindowFact,
    WorldCommand, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{fact, float_fact, screen};

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

fn issue(seq: u64) -> EventIssue {
    EventIssue {
        owner: OwnerId::parse("adapter-1").expect("owner"),
        correlation: CorrelationId::parse(&format!("req-{seq}")).expect("correlation"),
    }
}

fn ingress(fg: &str) -> PresentationIngressV4 {
    let frame = if fg.is_empty() {
        None
    } else {
        Some(Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        })
    };
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: frame,
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

#[allow(clippy::too_many_arguments)]
fn report_full(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    presentation: PresentationRowV4,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    renderer_input: Option<RendererInput>,
    intent_data: Vec<(String, String)>,
    closed: Vec<&str>,
    inventory_complete: bool,
    action_foreground: Option<&str>,
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
        screens,
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
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: action_foreground.map(|out| OutputId(out.to_owned())),
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete,
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
        presentation,
        intent_records: intent_data
            .into_iter()
            .map(|(win, data)| IntentRecord {
                win: WindowId(win),
                data,
            })
            .collect(),
        renderer_input,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: PresentationIngressV4,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    renderer_input: Option<RendererInput>,
    intent_data: Vec<(String, String)>,
) -> WorldReportV4 {
    report_full(
        seq,
        vec![screen("out-a", 0)],
        windows,
        focused,
        PresentationRowV4::Valid(ingress),
        command_seq,
        command,
        renderer_input,
        intent_data,
        Vec::new(),
        true,
        None,
    )
}

fn observe(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report(
        seq,
        windows,
        focused,
        ingress(focused),
        None,
        None,
        None,
        Vec::new(),
    )
}

fn run(engine: &mut WorldEngine, report: &WorldReportV4, consumed: bool) -> V4RunOutcome {
    let issuing = engine
        .state_revision()
        .checked_add(1)
        .expect("test issuing");
    engine.tx(|tx| tx.run_accepted_v4(report, &issue(report.request_seq), consumed, issuing))
}

fn run_with_settlements(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    consumed: bool,
) -> (V4RunOutcome, Vec<BatchSettlement>) {
    let issuing = engine
        .state_revision()
        .checked_add(1)
        .expect("test issuing");
    let (outcome, dispositions) = engine.tx(|tx| {
        tx.run_accepted_v4_batched(
            report,
            &issue(report.request_seq),
            consumed,
            issuing,
            issuing.checked_sub(1).filter(|prior| *prior != 0),
            &[],
            None,
        )
    });
    (
        outcome,
        crate::world_admit_v4::dispositions_to_legacy_settlements(&dispositions, &[]),
    )
}

fn accepted_reply(report_seq: u64, outcome: V4RunOutcome) -> crate::world_report::WorldReplyV4 {
    match outcome {
        V4RunOutcome::Accepted(reply) => {
            assert_eq!(reply.based_on_observation_seq, report_seq);
            assert!(!reply.command_consumed);
            reply
        }
        other => panic!("expected accepted v4 run, got {other:?}"),
    }
}

fn facts_pair() -> Vec<WindowFact> {
    vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)]
}

fn sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        sticky: Some(true),
        ..fact(window, output, x)
    }
}

fn accepted_frame(window: &str, attempt_ms: u64) -> FrameAttempt {
    FrameAttempt {
        window: WindowId(window.to_owned()),
        target: Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 90,
        },
        disposition: FrameSetter::Accepted,
        readback: None,
        dpi_before: None,
        dpi_after: None,
        attempt_ms,
        purpose: crate::world_report::FramePurpose::Layout,
    }
}

fn held_presentation() -> PresentationRowV4 {
    PresentationRowV4::Held(HeldPresentationRow {
        kind: RowDiagnosticKind::InvalidFact,
        diagnostic: None,
    })
}

fn pager_frames(reply: &crate::world_report::WorldReplyV4) -> Vec<(String, Rect)> {
    reply
        .presentation
        .pager
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .map(|window| (window.window.0.clone(), window.frame))
        .collect()
}

#[test]
fn bare_focus_direction_gate_splits_legacy_and_v4_envelopes() {
    // The bare v4 by-id Focus shape (empty direction) stays invalid in
    // the legacy envelope but valid in the v4 envelope; only the shared
    // dispatch gate admits it.
    let bare = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: None,
    };
    assert!(
        !crate::world_report::valid_command(&bare),
        "legacy envelope still refuses empty-direction Focus"
    );
    let envelope = report(
        1,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(bare),
        None,
        Vec::new(),
    );
    assert_eq!(
        crate::world_report_v4_checks::validate_report_v4_reason(&envelope),
        None,
        "v4 envelope admits the bare Focus shape"
    );
}

#[test]
fn observation_only_pure_v4_run_publishes_genuine_reply() {
    let mut engine = engine();
    let outcome = run(&mut engine, &observe(1, facts_pair(), "win-a"), false);
    let reply = accepted_reply(1, outcome);
    assert_eq!(reply.state_revision, 1);
    assert_eq!(reply.presentation.rev, 1);
    assert_eq!(engine.state_revision(), 1);
    assert_eq!(engine.world.presentation.latest_published_rev_v4, 1);
    assert!(!reply.scopes.is_empty());
    assert!(
        reply
            .windows
            .iter()
            .any(|window| window.lifetime.0 == "win-a")
    );
    assert!(
        reply
            .windows
            .iter()
            .any(|window| window.lifetime.0 == "win-b")
    );
    assert_eq!(engine.world.prior_context.prev_focused_global, "win-a");
    assert!(
        !engine.world.prior_context.empty_controlled.is_empty(),
        "accepted controlled-empty workspaces persist for shared lanes"
    );
}

#[test]
fn first_admission_intent_ingests_once_before_classification() {
    let mut engine = engine();
    let data = encode_intent_data(&IntentData::new(Some(true), None));
    let outcome = run(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            "",
            ingress(""),
            None,
            None,
            None,
            vec![("win-a".to_owned(), data)],
        ),
        false,
    );
    accepted_reply(1, outcome);
    let win = WindowId("win-a".to_owned());
    assert!(
        engine.world.intent.intent_admitted.contains(&win),
        "first-admission intent admits the same report it arrives on"
    );
    assert!(
        engine.world.intent.restored_floats.contains(&win),
        "pending float converts on healthy float success"
    );
    assert!(
        engine
            .world
            .workspace
            .members
            .get(&win)
            .is_some_and(|state| state.floating),
        "classifier saw the pending first-admission win before staging"
    );
}

#[test]
fn composed_sticky_clear_restages_and_keeps_tile_save() {
    // Composed sticky supersede through genuine v4 runs: null-sticky
    // facts stay live without the native capability, SetSticky(true)
    // establishes restored provenance, then a ToggleFloat with an
    // late imported (None,None) record clears through the clone/restage
    // path. The admitted lifetime ignores the late import independently
    // of restaging, so this is not an ingestion call-count check.
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
    .expect("generation");
    let mut null_row = fact("win-a", "out-a", 0);
    null_row.sticky = None;
    accepted_reply(
        1,
        run(
            &mut engine,
            &report(
                1,
                vec![null_row.clone()],
                "win-a",
                ingress("win-a"),
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let win = WindowId("win-a".to_owned());
    let (_set_on_outcome, set_on_settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            vec![null_row.clone()],
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(WorldCommand::SetSticky {
                window: crate::directional::WindowSubject::Window(win.clone()),
                value: true,
            }),
            None,
            Vec::new(),
        ),
        false,
    );
    assert_eq!(set_on_settlements.len(), 1);
    assert!(
        set_on_settlements[0].result.is_none(),
        "set-on executes: {:?}",
        set_on_settlements
    );
    assert_eq!(
        engine.world.intent.restored_sticky.get(&win),
        Some(&false),
        "tiled origin restores with a false prior"
    );
    let clear_data = encode_intent_data(&IntentData::new(None, None));
    let (_cleared_outcome, cleared_settlements) = run_with_settlements(
        &mut engine,
        &report(
            3,
            vec![null_row],
            "win-a",
            ingress("win-a"),
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                    "win-a".to_owned(),
                )),
                float_rect: None,
            })),
            None,
            vec![("win-a".to_owned(), clear_data)],
        ),
        false,
    );
    assert!(
        matches!(
            cleared_settlements.first().and_then(|s| s.result.as_ref()),
            Some(CommandResult::Float { rect: None, .. })
        ),
        "toggle-float clear mints the tile result: {:?}",
        cleared_settlements
    );
    assert!(
        !engine.world.intent.restored_sticky.contains_key(&win),
        "clear drops sticky provenance"
    );
    assert!(
        !engine.world.intent.pending_intent.contains_key(&win),
        "the admitted lifetime leaves no pending late import"
    );
    assert_eq!(
        engine
            .world
            .intent
            .intent_desired
            .get(&win)
            .and_then(|entry| entry.as_ref()),
        Some(&IntentData::new(Some(false), None)),
        "tile-base save adopted from staged facts"
    );
}

#[test]
fn ordinary_focus_command_dispatches_effect() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    let (outcome, settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(command),
            None,
            Vec::new(),
        ),
        false,
    );
    let reply = accepted_reply(2, outcome);
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target, WindowId("win-b".to_owned()));
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(1));
    assert_eq!(settlements.len(), 1);
    assert!(settlements[0].result.is_none());
    assert_eq!(engine.world.command_highwater, Some(1));
}

#[test]
fn consumed_command_reconciles_without_reexecution() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    let command_report = report(
        2,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(command.clone()),
        None,
        Vec::new(),
    );
    accepted_reply(2, run(&mut engine, &command_report, false));
    let (outcome, settlements) = run_with_settlements(&mut engine, &command_report, true);
    match outcome {
        V4RunOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert!(settlements.is_empty());
        }
        other => panic!("expected consumed v4 run, got {other:?}"),
    }
}

#[test]
fn consumed_rerun_with_new_observation_reconciles_without_revalidation() {
    // Fresh-then-consumed with a new observation and a stale renderer
    // input on the rerun: the consumed lane reconciles only, never
    // re-executes the original command and never re-validates the
    // renderer, while the new observation still publishes and the
    // revision stamps advance to the issuing revision.
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    accepted_reply(
        2,
        run(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                ingress("win-a"),
                Some(1),
                Some(command.clone()),
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let latest = engine.world.presentation.latest_published_rev_v4;
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let stale = RendererInput {
        rev: latest + 9,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let mut rerun_windows = facts_pair();
    rerun_windows.push(fact("w2", "out-a", 300));
    let issuing = engine.state_revision().checked_add(1).expect("issuing");
    let rerun = report(
        3,
        rerun_windows,
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(command),
        Some(stale),
        Vec::new(),
    );
    match engine.tx(|tx| {
        tx.run_accepted_v4_batched(
            &rerun,
            &issue(3),
            true,
            issuing,
            Some(issuing - 1),
            &[],
            None,
        )
    }) {
        (V4RunOutcome::CommandConsumed(reply), settlements) => {
            assert!(reply.command_consumed);
            assert_eq!(
                settlements,
                vec![crate::world_report::BatchDisposition {
                    seq: 1,
                    disposition: crate::world_report::CommandDisposition::AlreadyConsumed,
                }],
                "consumed reruns never re-validate the stale renderer: {:?}",
                settlements
            );
            assert_eq!(reply.state_revision, issuing);
            assert_eq!(reply.presentation.rev, issuing);
            assert_eq!(engine.world.presentation.latest_published_rev_v4, issuing);
            assert_eq!(engine.state_revision(), issuing);
            assert!(
                reply.windows.iter().any(|row| row.lifetime.0 == "w2"),
                "new observation publishes through the consumed lane: {:?}",
                reply.windows
            );
        }
        other => panic!("expected consumed rerun, got {other:?}"),
    }
}

#[test]
fn consumed_command_skips_renderer_validation() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let stale = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4 + 9,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    let command_report = report(
        2,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(command),
        Some(stale),
        Vec::new(),
    );
    let outcome = run(&mut engine, &command_report, true);
    match outcome {
        V4RunOutcome::CommandConsumed(_) => {}
        other => panic!("consumed runs skip validation, got {other:?}"),
    }
}

#[test]
fn refused_command_preserves_observation_without_command_mutation() {
    // Observation preservation without command mutation: the numbered
    // send refuses the sticky mover (`SendRefused`) while the same report
    // carries fresh observation (a new window, a real close, and a frame
    // attempt) that must incorporate despite the refusal. The refused run
    // must converge exactly like a commandless control over the same
    // observation (mapping, creation counter, retained flights,
    // membership) apart from the consumed sequence and typed result. This
    // path refuses before command mutation in `execute_send`; the
    // restore is defense-in-depth here, not rollback sensitivity.
    let mut engine = engine();
    let base_windows = || {
        vec![
            fact("w0", "out-a", 0),
            sticky_fact("wSticky", "out-a", 100),
            fact("wClose", "out-a", 200),
        ]
    };
    accepted_reply(
        1,
        run(&mut engine, &observe(1, base_windows(), "w0"), false),
    );
    accepted_reply(
        2,
        run(
            &mut engine,
            &report(
                2,
                base_windows(),
                "w0",
                ingress("w0"),
                Some(1),
                Some(WorldCommand::SelectTrailing { output: None }),
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let (_sent_outcome, sent_settlements) = run_with_settlements(
        &mut engine,
        &report(
            3,
            base_windows(),
            "w0",
            ingress("w0"),
            Some(2),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("w0".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: true,
            }),
            None,
            Vec::new(),
        ),
        false,
    );
    assert_eq!(sent_settlements.len(), 1);
    assert!(
        sent_settlements[0].result.is_none(),
        "occupying send executes, got {:?}",
        sent_settlements
    );
    assert!(
        engine
            .tx_ro(|tx| tx.ring_occupied(&OutputId("out-a".to_owned())))
            .contains("ws-2"),
        "trailing occupied so the numbered send must take the Create path"
    );
    // Fresh observation for the refused report: a new window, the real
    // close of a bootstrapped lifetime, and a mechanical frame attempt.
    let refused_windows = vec![
        fact("w0", "out-a", 0),
        sticky_fact("wSticky", "out-a", 100),
        fact("w2", "out-a", 300),
    ];
    let mut refused = report_full(
        4,
        vec![screen("out-a", 0)],
        refused_windows.clone(),
        "w0",
        PresentationRowV4::Valid(ingress("w0")),
        Some(3),
        Some(WorldCommand::SendNumbered {
            window: crate::directional::WindowSubject::Window(WindowId("wSticky".to_owned())),
            source: SendSource::Explicit(OutputId("out-a".to_owned())),
            index: 0,
            follow: false,
        }),
        None,
        Vec::new(),
        vec!["wClose"],
        true,
        None,
    );
    refused.monotonic_ms = 1000;
    refused.frames = vec![accepted_frame("w0", 1000)];
    // Commandless control over the same observation: the refused run must
    // converge identically apart from the consumed sequence and typed
    // result. The control never sees the refused command.
    let mut control = engine.clone();
    let mut control_obs = refused.clone();
    control_obs.command_seq = None;
    control_obs.command = None;
    accepted_reply(4, run(&mut control, &control_obs, false));
    let (reply_outcome, reply_settlements) = run_with_settlements(&mut engine, &refused, false);
    let reply = accepted_reply(4, reply_outcome);
    assert_eq!(
        reply_settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendRefused
        }),
        "sticky send refuses"
    );
    assert_eq!(engine.world.command_highwater, Some(3));
    assert_eq!(
        engine.world.workspace.mapping, control.world.workspace.mapping,
        "refusal preserves the commandless image"
    );
    assert_eq!(
        engine.world.workspace.creation_counter, control.world.workspace.creation_counter,
        "creation counter matches the commandless image"
    );
    assert_eq!(
        engine.world.settlement.retained, control.world.settlement.retained,
        "no transfer minted for the refused mover"
    );
    assert_eq!(
        engine.world.workspace.members, control.world.workspace.members,
        "membership matches the commandless image"
    );
    assert!(
        !engine
            .world.settlement.retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "wSticky")),
        "refused sticky leaves no flight: {:?}",
        engine.world.settlement.retained
    );
    let ids: Vec<String> = reply
        .windows
        .iter()
        .map(|window| window.lifetime.0.clone())
        .collect();
    assert!(
        ids.contains(&"w2".to_owned()),
        "new window publishes: {ids:?}"
    );
    assert!(
        !ids.contains(&"wClose".to_owned()),
        "closed lifetime drops from published rows: {ids:?}"
    );
    assert!(
        !engine
            .world
            .engine_knows_window_any(&WindowId("wClose".to_owned())),
        "closed lifetime drops from sessions"
    );
    assert!(
        !engine
            .world
            .workspace
            .members
            .contains_key(&WindowId("wClose".to_owned())),
        "closed lifetime drops from membership"
    );
    let w2 = reply
        .windows
        .iter()
        .find(|window| window.lifetime.0 == "w2")
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
    assert_eq!(
        engine
            .world
            .settlement
            .consumed_frames
            .get(&WindowId("w0".to_owned()))
            .and_then(crate::world_attempts::PropertyAttempt::as_frame)
            .map(|evidence| evidence.attempt_ms),
        Some(1000),
        "frame attempt settles pre-command"
    );
}

#[test]
fn frozen_migration_source_refuses_without_fallback() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let command = WorldCommand::MigrateDirectional {
        direction: crate::directional::Direction::Left,
    };
    let (outcome, settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(command),
            None,
            Vec::new(),
        ),
        false,
    );
    let _reply = accepted_reply(2, outcome);
    assert_eq!(
        settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        })
    );
    assert_eq!(engine.world.command_highwater, Some(1));
}

#[test]
fn frozen_new_view_refuses_despite_same_report_creation() {
    // Pre/post-source discriminating run: the hotplug screen resolves the
    // action output, but the pre-observation freeze binds nothing for lack
    // of a retained view. Same-report convergence creates that view, yet
    // the frozen None still refuses instead of retargeting.
    let mut engine = engine();
    let mut first = report_full(
        1,
        vec![screen("out-a", 0), screen("out-b", 800)],
        facts_pair(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
        Some("out-a"),
    );
    first.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    accepted_reply(1, run(&mut engine, &first, false));
    let mut hotplug = report_full(
        2,
        vec![
            screen("out-a", 0),
            screen("out-b", 800),
            screen("out-zzz", 1600),
        ],
        facts_pair(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(WorldCommand::MigrateDirectional {
            direction: crate::directional::Direction::Left,
        }),
        None,
        Vec::new(),
        Vec::new(),
        true,
        Some("out-zzz"),
    );
    hotplug.action_outputs = vec![
        OutputId("out-a".to_owned()),
        OutputId("out-b".to_owned()),
        OutputId("out-zzz".to_owned()),
    ];
    let (hotplug_outcome, hotplug_settlements) = run_with_settlements(&mut engine, &hotplug, false);
    let _reply = accepted_reply(2, hotplug_outcome);
    assert_eq!(
        hotplug_settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        }),
        "same-report view cannot authorize the frozen intent"
    );
    assert_eq!(engine.world.command_highwater, Some(1));
    assert!(
        engine.logical_view("out-zzz").is_some(),
        "convergence created the same-report source view, yet the refusal stands"
    );
}

#[test]
fn renderer_valid_target_proceeds_to_normal_command_gates() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-a".to_owned()))
        .expect("homed window")
        .clone();
    let renderer = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4,
        target: RenderTarget::Workspace {
            out: home.output.clone(),
            ws: home.workspace.clone(),
        },
    };
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-gone".to_owned())),
        direction: None,
    };
    let (outcome, settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(command),
            Some(renderer),
            Vec::new(),
        ),
        false,
    );
    let _reply = accepted_reply(2, outcome);
    assert_eq!(
        settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::FocusUnknown
        }),
        "valid renderer targets keep normal command gates"
    );
}

#[test]
fn renderer_unissued_revision_refuses_command() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let stale = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4 + 1,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    let (outcome, settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(command),
            Some(stale),
            Vec::new(),
        ),
        false,
    );
    let reply = accepted_reply(2, outcome);
    assert!(reply.focus_request.is_none());
    assert_eq!(
        settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StaleRenderTarget
        })
    );
    assert_eq!(engine.world.command_highwater, Some(1));
    assert_eq!(
        engine.world.presentation.latest_published_rev_v4, 2,
        "publication stamps its own revision after the refusal"
    );
}

#[test]
fn renderer_older_unchanged_target_executes_after_intervening_observation() {
    // An older-but-published renderer revision whose row is unchanged
    // still authorizes the command: only unissued revisions and
    // changed rows refuse.
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let older = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    accepted_reply(
        2,
        run(&mut engine, &observe(2, facts_pair(), "win-a"), false),
    );
    assert_eq!(
        engine.world.presentation.latest_published_rev_v4, 2,
        "intervening observation advances the published revision"
    );
    let (reply_outcome, reply_settlements) = run_with_settlements(
        &mut engine,
        &report(
            3,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(WorldCommand::Focus {
                window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
                direction: None,
            }),
            Some(older),
            Vec::new(),
        ),
        false,
    );
    let reply = accepted_reply(3, reply_outcome);
    assert_eq!(reply_settlements.len(), 1);
    assert!(reply_settlements[0].result.is_none());
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target, WindowId("win-b".to_owned()));
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(1));
}

#[test]
fn renderer_target_rehomed_during_genuine_run_refuses_as_stale() {
    // The stale target below was genuinely valid (the run-2 send carries
    // it and executes), then the send rehomes the window during that same
    // genuine run. The run-3 command reuses the old row and must refuse
    // with normal authority, never a publication hold.
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let old_home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-a".to_owned()))
        .expect("homed window")
        .clone();
    let old_renderer = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4,
        target: RenderTarget::Window {
            out: old_home.output.clone(),
            ws: old_home.workspace.clone(),
            win: WindowId("win-a".to_owned()),
        },
    };
    let (moved_outcome, moved_settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: true,
            }),
            Some(old_renderer.clone()),
            Vec::new(),
        ),
        false,
    );
    let _moved = accepted_reply(2, moved_outcome);
    assert_eq!(moved_settlements.len(), 1);
    assert!(
        moved_settlements[0].result.is_none(),
        "old target is still valid pre-move, got {:?}",
        moved_settlements
    );
    let new_home = engine
        .world
        .workspace
        .core_homing(&WindowId("win-a".to_owned()))
        .expect("rehomed window");
    assert_ne!(new_home, old_home, "the genuine run rehomed the target");
    let (reply_outcome, reply_settlements) = run_with_settlements(
        &mut engine,
        &report(
            3,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(2),
            Some(WorldCommand::Focus {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: None,
            }),
            Some(old_renderer),
            Vec::new(),
        ),
        false,
    );
    let reply = accepted_reply(3, reply_outcome);
    assert_eq!(
        reply_settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StaleRenderTarget
        })
    );
    assert_eq!(engine.world.command_highwater, Some(2));
    assert!(!reply.windows.is_empty(), "refusal still publishes rows");
    for window in &reply.windows {
        assert_eq!(
            window.membership_authority,
            ScopeAuthority::Controlled,
            "refusal publishes normal authority, never hold: {window:?}"
        );
    }
}

#[test]
fn renderer_equal_revision_with_exact_row_executes() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let home = engine
        .world
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let renderer = RendererInput {
        rev: engine.world.presentation.latest_published_rev_v4,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
        direction: None,
    };
    let (outcome, settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            facts_pair(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(command),
            Some(renderer),
            Vec::new(),
        ),
        false,
    );
    let reply = accepted_reply(2, outcome);
    assert_eq!(settlements.len(), 1);
    assert!(settlements[0].result.is_none());
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target, WindowId("win-b".to_owned()));
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(1));
}

#[test]
fn pager_frames_retain_through_finish() {
    let mut engine = engine();
    let frame = Rect {
        x: 0,
        y: 0,
        w: 40,
        h: 90,
    };
    let mut fresh_ingress = ingress("win-a");
    fresh_ingress.pager_order = vec![WindowId("win-a".to_owned())];
    fresh_ingress.pager_frames = vec![PresentationPagerFrameV4 {
        window: WindowId("win-a".to_owned()),
        live: Some(frame),
    }];
    let first = accepted_reply(
        1,
        run(
            &mut engine,
            &report(
                1,
                facts_pair(),
                "win-a",
                fresh_ingress,
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(
        first
            .presentation
            .pager
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .any(|window| window.window == WindowId("win-a".to_owned()) && window.frame == frame),
        "fresh v4 frames publish through finish"
    );
    let reply = accepted_reply(
        2,
        run(&mut engine, &observe(2, facts_pair(), "win-a"), false),
    );
    assert!(
        reply
            .presentation
            .pager
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .any(|window| window.window == WindowId("win-a".to_owned()) && window.frame == frame),
        "pager retention survives a frameless complete report"
    );
}

#[test]
fn pager_finish_retains_held_and_gaps_prunes_complete_and_close() {
    // Held presentation and incomplete-inventory gaps retain the live
    // frame; complete absence and explicit close prune it. All through
    // genuine v4 runs.
    let frame = Rect {
        x: 0,
        y: 0,
        w: 40,
        h: 90,
    };
    let fresh = || {
        let mut ingress = ingress("win-a");
        ingress.pager_order = vec![WindowId("win-a".to_owned())];
        ingress.pager_frames = vec![PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(frame),
        }];
        ingress
    };
    // Held + complete absence on one engine.
    let mut main = engine();
    accepted_reply(
        1,
        run(
            &mut main,
            &report(
                1,
                facts_pair(),
                "win-a",
                fresh(),
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let held = accepted_reply(
        2,
        run(
            &mut main,
            &report_full(
                2,
                vec![screen("out-a", 0)],
                facts_pair(),
                "win-a",
                held_presentation(),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                true,
                None,
            ),
            false,
        ),
    );
    assert!(
        pager_frames(&held)
            .iter()
            .any(|(window, rect)| window == "win-a" && *rect == frame),
        "held presentation retains the frame: {:?}",
        pager_frames(&held)
    );
    let pruned = accepted_reply(
        3,
        run(
            &mut main,
            &report_full(
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-b", "out-a", 100)],
                "win-b",
                PresentationRowV4::Valid(ingress("win-b")),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                true,
                None,
            ),
            false,
        ),
    );
    assert!(
        !pager_frames(&pruned)
            .iter()
            .any(|(window, _)| window == "win-a"),
        "complete absence prunes: {:?}",
        pager_frames(&pruned)
    );
    // Incomplete gap retains on a separate engine.
    let mut gapped = engine();
    accepted_reply(
        1,
        run(
            &mut gapped,
            &report(
                1,
                facts_pair(),
                "win-a",
                fresh(),
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let gap = accepted_reply(
        2,
        run(
            &mut gapped,
            &report_full(
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-b", "out-a", 100)],
                "win-b",
                PresentationRowV4::Valid(ingress("win-b")),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                false,
                None,
            ),
            false,
        ),
    );
    assert!(
        pager_frames(&gap)
            .iter()
            .any(|(window, rect)| window == "win-a" && *rect == frame),
        "incomplete absence retains the gap: {:?}",
        pager_frames(&gap)
    );
    // Held window row retains on a complete report with no raw row.
    let mut held_win = engine();
    accepted_reply(
        1,
        run(
            &mut held_win,
            &report(
                1,
                facts_pair(),
                "win-a",
                fresh(),
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let mut held_row_report = report_full(
        2,
        vec![screen("out-a", 0)],
        vec![fact("win-b", "out-a", 100)],
        "win-b",
        PresentationRowV4::Valid(ingress("win-b")),
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
        None,
    );
    held_row_report.held_windows = vec![HeldWindowRow {
        window: WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(OutputId("out-a".to_owned())),
        has_native_workspace: false,
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    let held_row = accepted_reply(2, run(&mut held_win, &held_row_report, false));
    assert!(
        pager_frames(&held_row)
            .iter()
            .any(|(window, rect)| window == "win-a" && *rect == frame),
        "held window row retains without false absence pruning: {:?}",
        pager_frames(&held_row)
    );
    // Explicit close prunes even with incomplete inventory, discriminating
    // it from the gap above.
    let mut closed_engine = engine();
    accepted_reply(
        1,
        run(
            &mut closed_engine,
            &report(
                1,
                facts_pair(),
                "win-a",
                fresh(),
                None,
                None,
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    let closed = accepted_reply(
        2,
        run(
            &mut closed_engine,
            &report_full(
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-b", "out-a", 100)],
                "win-b",
                PresentationRowV4::Valid(ingress("win-b")),
                None,
                None,
                None,
                Vec::new(),
                vec!["win-a"],
                false,
                None,
            ),
            false,
        ),
    );
    assert!(
        !pager_frames(&closed)
            .iter()
            .any(|(window, _)| window == "win-a"),
        "explicit close prunes: {:?}",
        pager_frames(&closed)
    );
}

#[test]
fn prior_context_and_stamp_progress_across_runs() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let reply = accepted_reply(
        2,
        run(&mut engine, &observe(2, facts_pair(), "win-b"), false),
    );
    assert_eq!(reply.state_revision, 2);
    assert_eq!(reply.presentation.rev, 2);
    assert_eq!(engine.state_revision(), 2);
    assert_eq!(engine.world.prior_context.prev_focused_global, "win-b");
}

#[test]
fn exhausted_revision_refuses_admitted_candidate() {
    let mut engine = engine();
    engine.world.state_revision = u64::MAX;
    let report = observe(1, facts_pair(), "win-a");
    match engine.tx(|tx| tx.run_accepted_v4(&report, &issue(1), false, 1)) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, "revision-exhausted"),
        other => panic!("expected revision exhaustion, got {other:?}"),
    }
    assert!(
        engine.world.workspace.members.is_empty(),
        "exhausted runs mutate nothing"
    );
}

#[test]
fn creation_exhaustion_propagates_as_envelope_refusal() {
    let mut engine = engine();
    let single = || vec![fact("win-a", "out-a", 0)];
    accepted_reply(1, run(&mut engine, &observe(1, single(), "win-a"), false));
    engine.world.workspace.creation_counter = u64::MAX;
    // Occupy the live trailing empty with a same-output send so the next
    // trailing selection must create (the exhaustion path) instead of
    // reusing. The sole window relocates its session onto the target.
    let (_occupy_outcome, occupy_settlements) = run_with_settlements(
        &mut engine,
        &report(
            2,
            single(),
            "win-a",
            ingress("win-a"),
            Some(1),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow: false,
            }),
            None,
            Vec::new(),
        ),
        false,
    );
    assert_eq!(occupy_settlements.len(), 1);
    assert!(
        occupy_settlements[0].result.is_none(),
        "occupying send executes, got {:?}",
        occupy_settlements
    );
    let revision_before = engine.state_revision();
    let outcome = run(
        &mut engine,
        &report(
            3,
            single(),
            "win-a",
            ingress("win-a"),
            Some(2),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            None,
            Vec::new(),
        ),
        false,
    );
    match outcome {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, "creation-exhausted"),
        other => panic!("exhaustion stays envelope, got {other:?}"),
    }
    assert_eq!(
        engine.world.command_highwater,
        Some(1),
        "envelope consumes no sequence"
    );
    assert_eq!(
        engine.state_revision(),
        revision_before,
        "envelope mutates no revision"
    );
}

#[test]
fn migration_unknown_minimized_refuses_before_engine_commit() {
    // Unknown sticky refuses migration before commit while preserving new
    // observations from the same report.
    let two_screens = || vec![screen("out-a", 0), screen("out-b", 800)];
    let mut engine = engine();
    let mut first = report_full(
        1,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
        Some("out-a"),
    );
    first.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    accepted_reply(1, run(&mut engine, &first, false));
    let source_view = engine.logical_view("out-a").expect("source retains a view");
    let mut minimized = fact("win-m", "out-a", 200);
    minimized.minimized = Some(true);
    minimized.sticky = None;
    let refused_windows = vec![
        fact("win-a", "out-a", 0),
        minimized,
        fact("w2", "out-a", 300),
    ];
    let mut refused = report_full(
        2,
        two_screens(),
        refused_windows.clone(),
        "win-a",
        PresentationRowV4::Valid(ingress("win-a")),
        Some(1),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: crate::directional::WorkspaceId(source_view.clone()),
        }),
        None,
        Vec::new(),
        Vec::new(),
        true,
        Some("out-a"),
    );
    refused.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    let mut control = engine.clone();
    let mut control_obs = refused.clone();
    control_obs.command_seq = None;
    control_obs.command = None;
    accepted_reply(2, run(&mut control, &control_obs, false));
    let (reply_outcome, reply_settlements) = run_with_settlements(&mut engine, &refused, false);
    let reply = accepted_reply(2, reply_outcome);
    assert_eq!(
        reply_settlements.first().and_then(|s| s.result.clone()),
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        }),
        "unknown minimized sticky refuses migration"
    );
    assert_eq!(engine.world.command_highwater, Some(1));
    assert_eq!(
        engine.world.workspace.mapping, control.world.workspace.mapping,
        "refusal preserves the commandless image"
    );
    assert_eq!(
        engine.world.workspace.members, control.world.workspace.members,
        "membership matches the commandless image"
    );
    assert_eq!(
        engine.world.settlement.retained, control.world.settlement.retained,
        "no migration flight survives the refusal"
    );
    let target_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(source_view.clone()),
    };
    assert!(
        !engine.world.layout.contains_domain(&target_key),
        "target session absent after refusal"
    );
    let source_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(source_view),
    };
    assert!(
        engine.world.layout.contains_domain(&source_key),
        "source session intact after refusal"
    );
    let ids: Vec<String> = reply
        .windows
        .iter()
        .map(|window| window.lifetime.0.clone())
        .collect();
    assert!(
        ids.contains(&"w2".to_owned()),
        "new window publishes: {ids:?}"
    );
}

const HOST_N1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000d1";
const HOST_N2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000d2";

fn host_owned_engine_v4() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
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
    .expect("valid generation")
}

fn native_row(output: &str, order: &[&str], current: Option<&str>) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: current.map(str::to_owned),
        occupancy: order
            .iter()
            .map(|s| NativeOccupancy {
                native: (*s).to_owned(),
                count: None,
            })
            .collect(),
    }
}

fn known_fact(window: &str, output: &str, x: i32, native: &str) -> WindowFact {
    let mut row = fact(window, output, x);
    row.native_workspace = NativeWorkspaceFact::Value(native.to_owned());
    row
}

fn unknown_fact(window: &str, output: &str, x: i32, sticky: Option<bool>) -> WindowFact {
    let mut row = fact(window, output, x);
    row.native_workspace = NativeWorkspaceFact::Omitted;
    row.sticky = sticky;
    row
}

fn host_report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    native: Vec<NativeObservation>,
) -> WorldReportV4 {
    let mut out = report_full(
        seq,
        vec![screen("out-a", 0)],
        windows,
        focused,
        PresentationRowV4::Valid(ingress(focused)),
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
        true,
        None,
    );
    out.native = native;
    out
}

fn host_command(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    native: Vec<NativeObservation>,
    command_seq: u64,
    command: WorldCommand,
) -> WorldReportV4 {
    let mut out = host_report(seq, windows, focused, native);
    out.command_seq = Some(command_seq);
    out.command = Some(command);
    out
}

fn sticky_state_row(
    reply: &crate::world_report::WorldReplyV4,
    window: &str,
) -> Option<(bool, ScopeAuthority, Option<u64>)> {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .and_then(|row| {
            row.state
                .iter()
                .find(|target| target.property == StateProperty::Sticky)
                .map(|target| (target.value, target.authority, target.command_seq))
        })
}

fn scope_authority(
    reply: &crate::world_report::WorldReplyV4,
    output: &str,
    workspace: &str,
) -> Option<ScopeAuthority> {
    reply
        .scopes
        .iter()
        .find(|status| {
            status.scope
                == AuthorityScope::Workspace {
                    output: OutputId(output.to_owned()),
                    workspace: crate::directional::WorkspaceId(workspace.to_owned()),
                }
        })
        .map(|status| status.authority)
}

#[test]
fn retained_all_desktop_core_sticky_on_off_roundtrip() {
    // Core origin: tiled admit -> SetSticky(true) command -> adapter
    // reports sticky=true with no native membership (positive
    // all-desktop) -> SetSticky(false) command executes (not scope-held)
    // -> correlated attempt plus readable clear lands the prior tiled
    // home. Sibling stays tiled/readable throughout.
    let mut engine = host_owned_engine_v4();
    let native = || vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))];
    let first = accepted_reply(
        1,
        run(
            &mut engine,
            &host_report(
                1,
                vec![
                    known_fact("win-a", "out-a", 0, HOST_N1),
                    known_fact("win-s", "out-a", 100, HOST_N1),
                ],
                "win-a",
                native(),
            ),
            false,
        ),
    );
    let home = engine
        .world
        .workspace
        .core_homing(&WindowId("win-s".to_owned()))
        .expect("win-s admitted with a retained home");
    let sibling_first = first
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("sibling publishes on admit");
    assert_eq!(
        (
            sibling_first.membership_authority,
            sibling_first.frame_authority
        ),
        (ScopeAuthority::Controlled, ScopeAuthority::Controlled),
        "sibling stays readable from admit: {sibling_first:?}"
    );
    assert!(
        sibling_first.allocation.is_some(),
        "sibling keeps geometry from admit: {sibling_first:?}"
    );
    let (on_outcome, on_settlements) = run_with_settlements(
        &mut engine,
        &host_command(
            2,
            vec![
                known_fact("win-a", "out-a", 0, HOST_N1),
                known_fact("win-s", "out-a", 100, HOST_N1),
            ],
            "win-a",
            native(),
            1,
            WorldCommand::SetSticky {
                window: crate::directional::WindowSubject::Window(WindowId("win-s".to_owned())),
                value: true,
            },
        ),
        false,
    );
    let on_reply = accepted_reply(2, on_outcome);
    assert_eq!(on_settlements.len(), 1);
    assert!(
        on_settlements[0].result.is_none(),
        "sticky-on executes: {:?}",
        on_settlements
    );
    assert_eq!(
        sticky_state_row(&on_reply, "win-s"),
        Some((true, ScopeAuthority::Controlled, Some(1))),
        "native Sticky=true target publishes: {:?}",
        on_reply.windows
    );
    let on_rev = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("on binding")
        .target
        .revision;
    let mut sticky = fact("win-s", "out-a", 100);
    sticky.sticky = Some(true);
    sticky.native_workspace = NativeWorkspaceFact::Omitted;
    let mut on_proof = host_report(
        3,
        vec![known_fact("win-a", "out-a", 0, HOST_N1), sticky],
        "win-a",
        native(),
    );
    on_proof.state_attempts = vec![StateAttempt {
        window: WindowId("win-s".to_owned()),
        property: StateProperty::Sticky,
        target: true,
        revision: on_rev,
        disposition: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(true),
        attempt_ms: 7,
    }];
    let all_desktop = accepted_reply(3, run(&mut engine, &on_proof, false));
    let on_binding = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("on binding");
    assert_eq!(on_binding.disposition, AttemptOutcome::Applied);
    assert!(on_binding.spent, "on-proof correlates");
    // Collected here, asserted only after the off command below so a
    // fencing regression fails at the off command with ScopeHeld proof.
    let on_home = engine
        .world
        .workspace
        .core_homing(&WindowId("win-s".to_owned()));
    let on_hold = engine.world.hold_reasons.clone();
    let on_target = all_desktop
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-s");
    let on_sibling = all_desktop
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a");
    let mut sticky_off = fact("win-s", "out-a", 100);
    sticky_off.sticky = Some(true);
    sticky_off.native_workspace = NativeWorkspaceFact::Omitted;
    let (off_outcome, off_settlements) = run_with_settlements(
        &mut engine,
        &host_command(
            4,
            vec![known_fact("win-a", "out-a", 0, HOST_N1), sticky_off],
            "win-a",
            native(),
            2,
            WorldCommand::SetSticky {
                window: crate::directional::WindowSubject::Window(WindowId("win-s".to_owned())),
                value: false,
            },
        ),
        false,
    );
    let off_reply = accepted_reply(4, off_outcome);
    assert_eq!(off_settlements.len(), 1);
    assert!(
        off_settlements[0].result.is_none(),
        "sticky-off executes, not scope-held: {:?}",
        off_settlements
    );
    assert_eq!(
        on_home,
        Some(home.clone()),
        "positive all-desktop never re-homes"
    );
    assert!(
        on_hold.is_empty() && !engine.world.hold_reasons.contains_key(&home),
        "retained all-desktop home fences nothing: before={on_hold:?} after={:?}",
        engine.world.hold_reasons
    );
    for (tag, slot) in [("subject", on_target), ("sibling", on_sibling)] {
        let row = slot.unwrap_or_else(|| panic!("{tag} publishes while all-desktop"));
        if tag == "subject" {
            assert!(row.floating, "sticky-on runs as sticky-float: {row:?}");
            assert!(
                row.state
                    .iter()
                    .any(|target| target.property == StateProperty::Sticky && target.value),
                "readable sticky true publishes while all-desktop: {row:?}"
            );
            continue;
        }
        assert_eq!(
            (row.membership_authority, row.frame_authority),
            (ScopeAuthority::Controlled, ScopeAuthority::Controlled),
            "{tag} tiled presentation stays readable: {row:?}"
        );
        assert!(row.allocation.is_some(), "{tag} keeps geometry: {row:?}");
    }
    assert_eq!(
        sticky_state_row(&off_reply, "win-s"),
        Some((false, ScopeAuthority::Controlled, Some(2))),
        "native Sticky=false target publishes: {:?}",
        off_reply.windows
    );
    let rev = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("off binding")
        .target
        .revision;
    let mut proof = host_report(
        5,
        vec![
            known_fact("win-a", "out-a", 0, HOST_N1),
            known_fact("win-s", "out-a", 100, HOST_N1),
        ],
        "win-a",
        native(),
    );
    proof.windows[1].sticky = Some(false);
    proof.state_attempts = vec![StateAttempt {
        window: WindowId("win-s".to_owned()),
        property: StateProperty::Sticky,
        target: false,
        revision: rev,
        disposition: FrameSetter::Accepted,
        pre: Some(true),
        post: Some(false),
        attempt_ms: 7,
    }];
    let cleared = accepted_reply(5, run(&mut engine, &proof, false));
    let binding = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    let member = engine
        .world
        .workspace
        .members
        .get(&WindowId("win-s".to_owned()))
        .expect("member");
    assert!(!member.floating, "prior tiled fresh-admits tiled");
    let sibling = cleared
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("sibling publishes");
    assert_eq!(
        (sibling.membership_authority, sibling.frame_authority),
        (ScopeAuthority::Controlled, ScopeAuthority::Controlled)
    );
    assert!(sibling.allocation.is_some());
}

#[test]
fn retained_all_desktop_unknown_membership_still_fenced() {
    // Non-sticky unknown membership with a retained home keeps the home
    // but fences the scope exactly as today.
    let mut engine = host_owned_engine_v4();
    let first = host_report(
        1,
        vec![
            known_fact("win-a", "out-a", 0, HOST_N1),
            known_fact("win-b", "out-a", 100, HOST_N2),
        ],
        "win-a",
        vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))],
    );
    accepted_reply(1, run(&mut engine, &first, false));
    let home = engine
        .world
        .workspace
        .core_homing(&WindowId("win-a".to_owned()))
        .expect("win-a admitted");
    let second = host_report(
        2,
        vec![
            unknown_fact("win-a", "out-a", 0, Some(false)),
            known_fact("win-b", "out-a", 100, HOST_N2),
        ],
        "win-b",
        vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))],
    );
    let reply = accepted_reply(2, run(&mut engine, &second, false));
    let kept = engine
        .world
        .workspace
        .core_homing(&WindowId("win-a".to_owned()))
        .expect("unknown keeps its retained home");
    assert_eq!(kept, home, "unknown never collapses");
    assert_eq!(
        scope_authority(&reply, &home.output.0, &home.workspace.0),
        Some(ScopeAuthority::Hold(
            crate::world_report::HoldReason::Unreadable
        )),
        "unknown membership still fences: {:?}",
        reply.scopes
    );
    assert!(reply.transfers.is_empty(), "fence never re-homes");
}

#[test]
fn retained_all_desktop_newcomer_cannot_invent_home() {
    // A new all-desktop sticky lifetime without a retained home invents
    // nothing: it publishes no row and authorizes no scope.
    let mut engine = host_owned_engine_v4();
    let reply = accepted_reply(
        1,
        run(
            &mut engine,
            &host_report(
                1,
                vec![
                    known_fact("win-a", "out-a", 0, HOST_N1),
                    unknown_fact("win-new", "out-a", 100, Some(true)),
                ],
                "win-a",
                vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))],
            ),
            false,
        ),
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-new"),
        "new all-desktop newcomer publishes no row: {:?}",
        reply.windows
    );
    assert!(
        engine
            .world
            .workspace
            .core_homing(&WindowId("win-new".to_owned()))
            .is_none(),
        "newcomer invents no retained home"
    );
    assert!(reply.transfers.is_empty());
}

#[test]
fn retained_all_desktop_native_sticky_off_no_prior_on() {
    // Native origin: float admit with a known native id, then the adapter
    // flips to sticky with no native membership and no core command ever
    // issued. SetSticky(false) still executes (not scope-held), and the
    // correlated clear keeps the prior float instead of fresh-admitting
    // tiled.
    let mut engine = host_owned_engine_v4();
    let native = || vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))];
    let mut floated = float_fact("win-f", "out-a", 100, 100);
    floated.native_workspace = NativeWorkspaceFact::Value(HOST_N1.to_owned());
    accepted_reply(
        1,
        run(
            &mut engine,
            &host_report(
                1,
                vec![known_fact("win-a", "out-a", 0, HOST_N1), floated],
                "win-a",
                native(),
            ),
            false,
        ),
    );
    assert!(
        engine
            .world
            .workspace
            .members
            .get(&WindowId("win-f".to_owned()))
            .expect("float member")
            .floating,
        "float admission stays floating"
    );
    let mut adopted = float_fact("win-f", "out-a", 100, 100);
    adopted.sticky = Some(true);
    adopted.native_workspace = NativeWorkspaceFact::Omitted;
    accepted_reply(
        2,
        run(
            &mut engine,
            &host_report(
                2,
                vec![known_fact("win-a", "out-a", 0, HOST_N1), adopted],
                "win-a",
                native(),
            ),
            false,
        ),
    );
    let home = engine
        .world
        .workspace
        .core_homing(&WindowId("win-f".to_owned()))
        .expect("adopted win-f keeps its home");
    // Collected here, asserted only after the off command below so a
    // fencing regression fails at the off command with ScopeHeld proof.
    let adopted_hold = engine.world.hold_reasons.clone();
    assert!(
        engine
            .world
            .workspace
            .members
            .get(&WindowId("win-f".to_owned()))
            .expect("member")
            .floating,
        "adopted sticky keeps prior float"
    );
    let mut adopted_cmd = float_fact("win-f", "out-a", 100, 100);
    adopted_cmd.sticky = Some(true);
    adopted_cmd.native_workspace = NativeWorkspaceFact::Omitted;
    let (off_outcome, off_settlements) = run_with_settlements(
        &mut engine,
        &host_command(
            3,
            vec![known_fact("win-a", "out-a", 0, HOST_N1), adopted_cmd],
            "win-a",
            native(),
            1,
            WorldCommand::SetSticky {
                window: crate::directional::WindowSubject::Window(WindowId("win-f".to_owned())),
                value: false,
            },
        ),
        false,
    );
    let off_reply = accepted_reply(3, off_outcome);
    assert_eq!(off_settlements.len(), 1);
    assert!(
        off_settlements[0].result.is_none(),
        "native sticky-off executes, not scope-held: {:?}",
        off_settlements
    );
    assert!(
        adopted_hold.is_empty() && !engine.world.hold_reasons.contains_key(&home),
        "adopted native sticky fences nothing: before={adopted_hold:?} after={:?}",
        engine.world.hold_reasons
    );
    assert_eq!(
        sticky_state_row(&off_reply, "win-f"),
        Some((false, ScopeAuthority::Controlled, Some(1))),
        "native Sticky=false target publishes: {:?}",
        off_reply.windows
    );
    let rev = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-f".to_owned()), StateProperty::Sticky))
        .expect("off binding")
        .target
        .revision;
    let mut clear = float_fact("win-f", "out-a", 100, 100);
    clear.native_workspace = NativeWorkspaceFact::Value(HOST_N1.to_owned());
    clear.sticky = Some(false);
    let mut proof = host_report(
        4,
        vec![known_fact("win-a", "out-a", 0, HOST_N1), clear],
        "win-a",
        native(),
    );
    proof.state_attempts = vec![StateAttempt {
        window: WindowId("win-f".to_owned()),
        property: StateProperty::Sticky,
        target: false,
        revision: rev,
        disposition: FrameSetter::Accepted,
        pre: Some(true),
        post: Some(false),
        attempt_ms: 7,
    }];
    accepted_reply(4, run(&mut engine, &proof, false));
    let binding = engine
        .world
        .settlement
        .retained_states
        .get(&(WindowId("win-f".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(
        engine
            .world
            .workspace
            .members
            .get(&WindowId("win-f".to_owned()))
            .expect("member")
            .floating,
        "native clear keeps prior float"
    );
}

#[test]
fn retained_all_desktop_unknown_sticky_missing_membership_still_fenced() {
    // sticky=None with each unknown membership spelling keeps the retained
    // home but still fences the scope: only positive sticky opts out.
    for (tag, native) in [
        ("omitted", NativeWorkspaceFact::Omitted),
        ("null", NativeWorkspaceFact::Null),
        ("empty", NativeWorkspaceFact::Value(String::new())),
    ] {
        let mut engine = host_owned_engine_v4();
        accepted_reply(
            1,
            run(
                &mut engine,
                &host_report(
                    1,
                    vec![
                        known_fact("win-a", "out-a", 0, HOST_N1),
                        known_fact("win-b", "out-a", 100, HOST_N2),
                    ],
                    "win-a",
                    vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))],
                ),
                false,
            ),
        );
        let home = engine
            .world
            .workspace
            .core_homing(&WindowId("win-a".to_owned()))
            .expect("win-a admitted");
        let mut unknown = fact("win-a", "out-a", 0);
        unknown.sticky = None;
        unknown.native_workspace = native;
        let reply = accepted_reply(
            2,
            run(
                &mut engine,
                &host_report(
                    2,
                    vec![unknown, known_fact("win-b", "out-a", 100, HOST_N2)],
                    "win-b",
                    vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))],
                ),
                false,
            ),
        );
        assert_eq!(
            engine
                .world
                .workspace
                .core_homing(&WindowId("win-a".to_owned())),
            Some(home.clone()),
            "{tag}: unknown keeps its retained home"
        );
        assert_eq!(
            scope_authority(&reply, &home.output.0, &home.workspace.0),
            Some(ScopeAuthority::Hold(
                crate::world_report::HoldReason::Unreadable
            )),
            "{tag}: sticky-None unknown still fences: {:?}",
            reply.scopes
        );
        assert!(reply.transfers.is_empty(), "{tag}: fence never re-homes");
    }
}

#[test]
fn q1b_queued_and_inline_handle_report_emit_batch_settlements_and_non_issuing_stay_empty() {
    let mut engine = engine();
    let bootstrap = report(
        1,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let (boot_outcome, boot_settlements) = engine.handle_report(&bootstrap, &[]);
    assert!(matches!(boot_outcome, V4RunOutcome::Accepted(_)));
    assert!(boot_settlements.is_empty());

    // Inline `report.command` (seq 1, SetSticky -> None outcome) + queued
    // command (seq 2, ToggleFloat -> Float outcome) both settle in
    // inline-then-queued order on the issuing `handle_report` call.
    let issuing_report = report(
        2,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(WorldCommand::SetSticky {
            window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            value: true,
        }),
        None,
        Vec::new(),
    );
    let queued = [(
        2,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                "win-b".to_owned(),
            )),
            float_rect: None,
        }),
    )];
    let (issuing_outcome, issuing_settlements) = engine.handle_report(&issuing_report, &queued);
    assert!(matches!(issuing_outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(issuing_settlements.len(), 2);
    assert_eq!(issuing_settlements[0].seq, 1);
    assert!(
        issuing_settlements[0].result.is_none(),
        "inline SetSticky settles with None outcome: {:?}",
        issuing_settlements[0]
    );
    assert_eq!(issuing_settlements[1].seq, 2);
    assert!(
        matches!(
            issuing_settlements[1].result,
            Some(CommandResult::Float { .. })
        ),
        "queued ToggleFloat settles with Float outcome: {:?}",
        issuing_settlements[1]
    );

    // Exact Duplicate (same observation_seq + request_id) does not rerun and
    // returns empty settlements.
    let (dup_outcome, dup_settlements) = engine.handle_report(&issuing_report, &queued);
    assert!(matches!(dup_outcome, V4RunOutcome::Duplicate(_)));
    assert!(
        dup_settlements.is_empty(),
        "Duplicate must not rerun or reissue settlements: {dup_settlements:?}"
    );

    // Later commandless handle_report returns empty settlements.
    let commandless = report(
        3,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let (quiet_outcome, quiet_settlements) = engine.handle_report(&commandless, &[]);
    assert!(matches!(quiet_outcome, V4RunOutcome::Accepted(_)));
    assert!(
        quiet_settlements.is_empty(),
        "commandless handle_report returns empty Vec: {quiet_settlements:?}"
    );

    // CommandConsumed (newer observation_seq reusing consumed command_seq)
    // does not rerun and returns empty settlements.
    let consumed_report = report(
        4,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        Some(2),
        Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                "win-a".to_owned(),
            )),
            float_rect: None,
        })),
        None,
        Vec::new(),
    );
    let (consumed_outcome, consumed_settlements) = engine.handle_report(&consumed_report, &[]);
    assert!(matches!(consumed_outcome, V4RunOutcome::CommandConsumed(_)));
    assert!(
        consumed_settlements.is_empty(),
        "CommandConsumed must not rerun or reissue settlements: {consumed_settlements:?}"
    );
}

#[test]
fn retained_drag_preview_persists_across_skipped_publication_and_commandless_report() {
    let mut engine = engine();
    let bootstrap = report(
        1,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    assert!(matches!(
        engine.handle_report(&bootstrap, &[]).0,
        V4RunOutcome::Accepted(_)
    ));

    let mut gesture_report = report(
        2,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    gesture_report.gesture_barrier = true;
    gesture_report.gesture = Some(crate::gesture::GestureIngress {
        id: 7,
        window: WindowId("win-a".to_owned()),
        kind: crate::gesture::GestureKind::Move,
        cursor: Some((160, 50)),
        start_cursor: Some((10, 10)),
        phase: crate::gesture::GesturePhase::Updated,
        esc: false,
        modal_move: true,
        start_frame: Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
    });
    // Caller receives the accepted reply but skips publishing it.
    let (skipped_outcome, _) = engine.handle_report(&gesture_report, &[]);
    let expected_preview = match skipped_outcome {
        V4RunOutcome::Accepted(reply) => reply
            .drag_preview
            .expect("gesture update produces drag_preview"),
        other => panic!("expected accepted gesture update, got {other:?}"),
    };

    // Subsequent commandless / omitted-ingress report still publishes the
    // retained drag_preview.
    let commandless = report(
        3,
        facts_pair(),
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let (next_outcome, next_settlements) = engine.handle_report(&commandless, &[]);
    assert!(next_settlements.is_empty());
    match next_outcome {
        V4RunOutcome::Accepted(reply) => {
            assert_eq!(reply.drag_preview, Some(expected_preview));
        }
        other => panic!("expected accepted commandless reply, got {other:?}"),
    }
}

#[test]
fn restage_classification_preserves_native_echo_across_same_batch_command() {
    let mut engine = host_owned_engine_v4();
    let out_a = OutputId("out-a".to_owned());
    let native_n1 = || vec![native_row("out-a", &[HOST_N1, HOST_N2], Some(HOST_N1))];
    let windows_n1 = || {
        vec![
            known_fact("win-a", "out-a", 0, HOST_N1),
            known_fact("win-b", "out-a", 0, HOST_N2),
        ]
    };

    let boot = host_report(1, windows_n1(), "win-a", native_n1());
    assert!(matches!(
        engine.handle_report(&boot, &[]).0,
        V4RunOutcome::Accepted(_)
    ));
    let l1 = engine
        .world
        .workspace
        .views
        .get("out-a")
        .expect("initial retained view on out-a")
        .clone();
    let l2 = engine
        .world
        .workspace
        .mapping
        .get("out-a")
        .expect("ring on out-a")
        .iter()
        .find(|id| *id != &l1)
        .expect("second workspace on out-a")
        .clone();

    // Retained view moves to l2 while native current remains HOST_N1 (l1).
    let switch = host_command(
        2,
        windows_n1(),
        "win-a",
        native_n1(),
        1,
        WorldCommand::SelectWorkspace {
            output: out_a.clone(),
            workspace: crate::directional::WorkspaceId(l2.clone()),
        },
    );
    assert!(matches!(
        engine.handle_report(&switch, &[]).0,
        V4RunOutcome::Accepted(_)
    ));
    assert_eq!(engine.world.workspace.views.get("out-a"), Some(&l2));

    // Same-batch command sequence on a report where native current is still
    // HOST_N1 (l1) != retained view (l2): ToggleFloat triggers
    // restage_classification(), which must preserve staged.echoes["out-a"].
    let batch = host_command(
        3,
        windows_n1(),
        "win-a",
        native_n1(),
        2,
        WorldCommand::Operation(CoreCommand::ToggleFloat {
            window: crate::directional::WindowSubject::Window(crate::directional::WindowId(
                "win-a".to_owned(),
            )),
            float_rect: None,
        }),
    );
    let queued = [(
        3,
        WorldCommand::SetSticky {
            window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            value: true,
        },
    )];
    let (outcome, settlements) = engine.handle_report(&batch, &queued);
    assert!(matches!(outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(settlements.len(), 2);

    assert_eq!(
        engine.tx_ro(|tx| tx.known_echo(&out_a)),
        Some(l1.clone()),
        "native echo must survive restage_classification"
    );
    assert_eq!(
        engine.tx_ro(|tx| tx.effective_view(&out_a)),
        Some(l1),
        "effective_view must keep native echo over retained view"
    );
}

#[test]
fn ac1_ordered_inline_plus_queue_cap_128_and_whole_batch_refusal() {
    use crate::directional::WindowSubject;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_report::{BatchDisposition, CommandDisposition, ReportIncorporation};

    let gate = StructuralCoreAdmissionGate;
    let focus_a = || WorldCommand::Focus {
        window: WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: None,
    };

    // 1. Cap 128 including inline: 1 inline + 127 queued succeeds;
    //    1 inline + 128 queued (= 129) refuses whole batch with NotRun.
    let mut eng = engine();
    let seed = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let seed_res = eng
        .handle_report_with_gate_and_prepare(&seed, &[], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert_eq!(seed_res.incorporation, ReportIncorporation::Incorporated);

    let mut r128 = report(
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(focus_a()),
        None,
        Vec::new(),
    );
    let q127: Vec<(u64, WorldCommand)> = (2..=128).map(|s| (s, focus_a())).collect();
    let res128 = eng
        .handle_report_with_gate_and_prepare(&r128, &q127, &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(matches!(res128.outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(res128.incorporation, ReportIncorporation::Incorporated);
    assert_eq!(res128.dispositions.len(), 128);
    assert!(
        res128
            .dispositions
            .iter()
            .all(|d| matches!(d.disposition, CommandDisposition::Executed(_)))
    );

    r128.observation_seq = 3;
    r128.request_seq = 3;
    r128.request_id = "req-3".to_owned();
    r128.command_seq = Some(129);
    let q128: Vec<(u64, WorldCommand)> = (130..=257).map(|s| (s, focus_a())).collect();
    let res129 = eng
        .handle_report_with_gate_and_prepare(&r128, &q128, &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(
        matches!(
            res129.outcome,
            V4RunOutcome::Rejected {
                kind: "snapshot-invalid",
                message: "section-over-cap",
            }
        ),
        "129 total commands must refuse whole batch with section-over-cap: {:?}",
        res129.outcome
    );
    assert_eq!(res129.incorporation, ReportIncorporation::Rejected);
    assert!(res129.dispositions.is_empty());

    // 2. Whole-batch command-order-invalid / command-invalid refusals:
    //    [7, 6], [7, 7], queued-only SwitchView alongside a valid queued command,
    //    and invalid queued command.
    let rev_before = eng.state_revision();
    type BadBatch = (
        Option<u64>,
        Option<WorldCommand>,
        Vec<(u64, WorldCommand)>,
        &'static str,
    );
    let bad_batches: Vec<BadBatch> = vec![
        (
            Some(200),
            Some(focus_a()),
            vec![(199, focus_a())],
            "command-order-invalid",
        ),
        (
            Some(200),
            Some(focus_a()),
            vec![(200, focus_a())],
            "command-order-invalid",
        ),
        (
            None,
            None,
            vec![
                (201, focus_a()),
                (
                    202,
                    WorldCommand::SwitchView {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                    },
                ),
            ],
            "command-invalid",
        ),
        (
            Some(200),
            Some(focus_a()),
            vec![(
                201,
                WorldCommand::Focus {
                    window: WindowSubject::Window(WindowId("win-a".to_owned())),
                    direction: Some(crate::directional::Direction::Right),
                },
            )],
            "command-invalid",
        ),
    ];
    for (idx, (inline_seq, inline_cmd, queued, expected_msg)) in bad_batches.into_iter().enumerate()
    {
        let r = report(
            4 + idx as u64,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            ingress("win-a"),
            inline_seq,
            inline_cmd.clone(),
            None,
            Vec::new(),
        );
        let res = eng
            .handle_report_with_gate_and_prepare(&r, &queued, &gate, |_| {
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap_or_else(|e| match e {});
        assert!(
            matches!(
                res.outcome,
                V4RunOutcome::Rejected {
                    kind: "snapshot-invalid",
                    message,
                } if message == expected_msg
            ),
            "case {idx} must refuse whole batch with {expected_msg}: {:?}",
            res.outcome
        );
        assert_eq!(res.incorporation, ReportIncorporation::Rejected);
        assert!(res.dispositions.is_empty());
        assert_eq!(eng.state_revision(), rev_before);
    }

    // 3. Stale inline followed by fresh queue + renderer_input proof with FIFO:
    //    inline gets AlreadyConsumed, queued executes normally.
    let stale_inline = report(
        10,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        Some(128),
        Some(focus_a()),
        Some(RendererInput {
            rev: rev_before,
            target: RenderTarget::Window {
                out: OutputId("out-a".to_owned()),
                ws: crate::directional::WorkspaceId("ws-1".to_owned()),
                win: WindowId("win-a".to_owned()),
            },
        }),
        Vec::new(),
    );
    let stale_res = eng
        .handle_report_with_gate_and_prepare(&stale_inline, &[(129, focus_a())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(
        matches!(stale_res.outcome, V4RunOutcome::CommandConsumed(_)),
        "stale inline surfaces CommandConsumed while still executing fresh queue: {:?}",
        stale_res.outcome
    );
    assert_eq!(stale_res.incorporation, ReportIncorporation::Incorporated);
    assert_eq!(
        stale_res.dispositions,
        vec![
            BatchDisposition {
                seq: 128,
                disposition: CommandDisposition::AlreadyConsumed,
            },
            BatchDisposition {
                seq: 129,
                disposition: CommandDisposition::Executed(None),
            },
        ]
    );

    // 4. Fresh inline + renderer_input proof + queued command execute in FIFO order.
    let rev_before_fifo = eng.state_revision();
    let inline_fifo = report(
        11,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        Some(130),
        Some(focus_a()),
        Some(RendererInput {
            rev: rev_before_fifo,
            target: RenderTarget::Window {
                out: OutputId("out-a".to_owned()),
                ws: crate::directional::WorkspaceId("ws-1".to_owned()),
                win: WindowId("win-a".to_owned()),
            },
        }),
        Vec::new(),
    );
    let fifo_res = eng
        .handle_report_with_gate_and_prepare(&inline_fifo, &[(131, focus_a())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(matches!(fifo_res.outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(fifo_res.incorporation, ReportIncorporation::Incorporated);
    assert_eq!(
        fifo_res.dispositions,
        vec![
            BatchDisposition {
                seq: 130,
                disposition: CommandDisposition::Executed(None),
            },
            BatchDisposition {
                seq: 131,
                disposition: CommandDisposition::Executed(None),
            },
        ]
    );
}

#[test]
fn ac1_first_entry_queued_frozen_migration_source_vintage() {
    use crate::directional::WindowSubject;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_report::{CommandDisposition, CommandRejectKind};

    let gate = StructuralCoreAdmissionGate;
    // Two adjacent outputs (out-a at x=0, out-b at x=800) so Right has a real
    // neighbor. Contrast MigrateDirectional(Right) on unbootstrapped r1
    // (empty pre-observation views -> MigrationRefused) vs bootstrapped r2
    // both as first entry and after a stale/refused first entry (Executed).
    let mut r1 = report(
        1,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    r1.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    r1.action_foreground = Some(OutputId("out-a".to_owned()));
    r1.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];

    let migrate_right = WorldCommand::MigrateDirectional {
        direction: crate::directional::Direction::Right,
    };

    let mut eng_unbootstrapped = engine();
    let res_unbootstrapped = eng_unbootstrapped
        .handle_report_with_gate_and_prepare(&r1, &[(1, migrate_right.clone())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(matches!(
        res_unbootstrapped.outcome,
        V4RunOutcome::Accepted(_)
    ));
    assert_eq!(res_unbootstrapped.dispositions.len(), 1);
    assert!(
        matches!(
            res_unbootstrapped.dispositions[0].disposition,
            CommandDisposition::Refused(CommandRejectKind::MigrationRefused)
        ),
        "unbootstrapped pre-observation views on first queued entry must refuse Right: {:?}",
        res_unbootstrapped.dispositions
    );

    let mut r2 = report(
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    r2.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    r2.action_foreground = Some(OutputId("out-a".to_owned()));
    r2.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];

    let mut eng_first = eng_unbootstrapped.clone();
    let res_first = eng_first
        .handle_report_with_gate_and_prepare(&r2, &[(2, migrate_right.clone())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(matches!(res_first.outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(res_first.dispositions.len(), 1);
    assert!(
        matches!(
            res_first.dispositions[0].disposition,
            CommandDisposition::Executed(_)
        ),
        "bootstrapped r2 as first queued entry must execute Right: {:?}",
        res_first.dispositions
    );

    let mut eng_after_stale_and_refused = eng_unbootstrapped;
    let res_after = eng_after_stale_and_refused
        .handle_report_with_gate_and_prepare(
            &r2,
            &[
                (
                    1,
                    WorldCommand::Focus {
                        window: WindowSubject::Window(WindowId("win-missing".to_owned())),
                        direction: None,
                    },
                ),
                (
                    2,
                    WorldCommand::Focus {
                        window: WindowSubject::Window(WindowId("win-missing".to_owned())),
                        direction: None,
                    },
                ),
                (3, migrate_right),
            ],
            &gate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    assert!(matches!(res_after.outcome, V4RunOutcome::Accepted(_)));
    assert_eq!(res_after.dispositions.len(), 3);
    assert!(matches!(
        res_after.dispositions[0].disposition,
        CommandDisposition::AlreadyConsumed
    ));
    assert!(matches!(
        res_after.dispositions[1].disposition,
        CommandDisposition::Refused(_)
    ));
    assert!(
        matches!(
            res_after.dispositions[2].disposition,
            CommandDisposition::Executed(_)
        ),
        "after stale and refused first entries, MigrateDirectional(Right) must execute: {:?}",
        res_after.dispositions
    );
}

#[test]
fn ac2_incorporation_variants_dispositions_and_rollback() {
    use crate::directional::WindowSubject;
    use crate::world_admit_v4::StructuralCoreAdmissionGate;
    use crate::world_report::{
        BatchDisposition, CommandDisposition, CommandRejectKind, ReportIncorporation,
    };

    let gate = StructuralCoreAdmissionGate;
    let focus_a = WorldCommand::Focus {
        window: WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: None,
    };
    let toggle_a = WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: WindowSubject::Window(WindowId("win-a".to_owned())),
        float_rect: None,
    });
    let toggle_missing = WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: WindowSubject::Window(WindowId("win-missing".to_owned())),
        float_rect: None,
    });

    // 1. Pre-settings (BootstrapIntentOnly) leaves queued commands NotRun.
    let mut pre = WorldEngine::new("gen-1").expect("gen");
    let hs = pre.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "hs-1".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: None,
            executable: Some("fixture".to_owned()),
            process_id: Some("pid-1".to_owned()),
        },
        capabilities: HostCapabilities::default(),
    });
    assert_eq!(hs.generation.as_deref(), Some("gen-1"));
    let pre_rep = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let pre_res = pre
        .handle_report_with_gate_and_prepare(&pre_rep, &[(1, focus_a.clone())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert_eq!(
        pre_res.incorporation,
        ReportIncorporation::BootstrapIntentOnly
    );
    assert_eq!(
        pre_res.dispositions,
        vec![BatchDisposition {
            seq: 1,
            disposition: CommandDisposition::NotRun,
        }]
    );

    // 2. Incorporated with Executed(Some) and Executed(None), plus Replay
    //    ignoring a changed/invalid queue -> NotRun.
    let mut eng = engine();
    let r1 = report(
        1,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        Some(1),
        Some(toggle_a.clone()),
        None,
        Vec::new(),
    );
    let res1 = eng
        .handle_report_with_gate_and_prepare(&r1, &[(2, focus_a.clone())], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert_eq!(res1.incorporation, ReportIncorporation::Incorporated);
    assert!(matches!(
        res1.dispositions.as_slice(),
        [
            BatchDisposition {
                seq: 1,
                disposition: CommandDisposition::Executed(Some(_)),
            },
            BatchDisposition {
                seq: 2,
                disposition: CommandDisposition::Executed(None),
            },
        ]
    ));

    // Exact report replay with a different/invalid queued command must return
    // Replay + Duplicate and mark the supplied queue NotRun without executing.
    let invalid_queued = WorldCommand::Focus {
        window: WindowSubject::Window(WindowId("win-a".to_owned())),
        direction: Some(crate::directional::Direction::Right),
    };
    let replay_res = eng
        .handle_report_with_gate_and_prepare(&r1, &[(3, invalid_queued)], &gate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    assert!(matches!(replay_res.outcome, V4RunOutcome::Duplicate(_)));
    assert_eq!(replay_res.incorporation, ReportIncorporation::Replay);
    assert_eq!(
        replay_res.dispositions,
        vec![
            BatchDisposition {
                seq: 1,
                disposition: CommandDisposition::NotRun,
            },
            BatchDisposition {
                seq: 3,
                disposition: CommandDisposition::NotRun,
            },
        ]
    );

    // 3. Per-command refusal consumes sequence and rolls back only that
    //    command's effects while subsequent commands in the batch execute.
    let r2 = report(
        2,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let res2 = eng
        .handle_report_with_gate_and_prepare(
            &r2,
            &[(3, toggle_missing), (4, toggle_a)],
            &gate,
            |_| Ok::<(), std::convert::Infallible>(()),
        )
        .unwrap_or_else(|e| match e {});
    assert_eq!(res2.incorporation, ReportIncorporation::Incorporated);
    assert!(matches!(
        res2.dispositions.as_slice(),
        [
            BatchDisposition {
                seq: 3,
                disposition: CommandDisposition::Refused(CommandRejectKind::OperationRefused),
            },
            BatchDisposition {
                seq: 4,
                disposition: CommandDisposition::Executed(Some(_)),
            },
        ]
    ));

    // 4. Late prepare failure rolls back engine state completely.
    let rev_before = eng.state_revision();
    let r3 = report(
        3,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        ingress("win-a"),
        None,
        None,
        None,
        Vec::new(),
    );
    let err = eng.handle_report_with_gate_and_prepare(&r3, &[(5, focus_a)], &gate, |_| {
        Err("late-prepare-abort")
    });
    assert_eq!(err, Err("late-prepare-abort"));
    assert_eq!(eng.state_revision(), rev_before);
}
