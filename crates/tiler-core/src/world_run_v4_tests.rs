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
use crate::world_directional::tests::{fact, float_fact, screen};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    AttemptOutcome, AuthorityScope, CommandRejectKind, CommandResult, EffectKind, FrameAttempt,
    FrameSetter, HeldPresentationRow, HeldWindowRow, IntentRecord, NativeObservation,
    NativeOccupancy, NativeWorkspaceFact, PresentationIngressV4, PresentationPagerFrameV4,
    PresentationRowV4, RenderTarget, RendererInput, RowDiagnosticKind, ScopeAuthority,
    ScreenReport, SendSource, StateAttempt, StateProperty, WORLD_CONTRACT_VERSION_V4, WindowFact,
    WorldCommand, WorldReportV4,
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
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
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
    let issuing = engine.state_revision.checked_add(1).expect("test issuing");
    engine.run_accepted_v4(report, &issue(report.request_seq), consumed, issuing)
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
        setter: FrameSetter::Accepted,
        readback: None,
        dpi_before: None,
        dpi_after: None,
        attempt_ms,
        operation_id: None,
        prewrite: None,
    }
}

fn held_presentation() -> PresentationRowV4 {
    PresentationRowV4::Held(HeldPresentationRow {
        fields: Vec::new(),
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
        window: WindowId("win-a".to_owned()),
        direction: String::new(),
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
    assert_eq!(engine.state_revision, 1);
    assert_eq!(engine.latest_published_presentation_rev_v4, 1);
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
    assert_eq!(engine.prior_context.prev_focused_global, "win-a");
    assert!(
        !engine.prior_context.empty_controlled.is_empty(),
        "accepted controlled-empty workspaces persist for shared lanes"
    );
    assert!(
        engine.last_report.is_none(),
        "v4 runs never touch legacy replay"
    );
    assert!(
        engine.last_reply.is_none(),
        "v4 runs never touch legacy replay"
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
        engine.intent_admitted.contains(&win),
        "first-admission intent admits the same report it arrives on"
    );
    assert!(
        engine.restored_floats.contains(&win),
        "pending float converts on healthy float success"
    );
    assert!(
        engine.members.get(&win).is_some_and(|state| state.floating),
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
    let set_on = accepted_reply(
        2,
        run(
            &mut engine,
            &report(
                2,
                vec![null_row.clone()],
                "win-a",
                ingress("win-a"),
                Some(1),
                Some(WorldCommand::SetSticky {
                    window: win.clone(),
                    value: true,
                }),
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(
        set_on.command_result.is_none(),
        "set-on executes: {:?}",
        set_on.command_result
    );
    assert_eq!(
        engine.restored_sticky.get(&win),
        Some(&false),
        "tiled origin restores with a false prior"
    );
    let clear_data = encode_intent_data(&IntentData::new(None, None));
    let cleared = accepted_reply(
        3,
        run(
            &mut engine,
            &report(
                3,
                vec![null_row],
                "win-a",
                ingress("win-a"),
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "win-a".to_owned(),
                    float_rect: None,
                })),
                None,
                vec![("win-a".to_owned(), clear_data)],
            ),
            false,
        ),
    );
    assert!(
        matches!(
            cleared.command_result,
            Some(CommandResult::Float { rect: None, .. })
        ),
        "toggle-float clear mints the tile result: {:?}",
        cleared.command_result
    );
    assert!(
        !engine.restored_sticky.contains_key(&win),
        "clear drops sticky provenance"
    );
    assert!(
        !engine.pending_intent.contains_key(&win),
        "the admitted lifetime leaves no pending late import"
    );
    assert_eq!(
        engine.sticky_saved.get(&win),
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
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    let outcome = run(
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
    assert!(
        reply.effects.iter().any(|effect| {
            effect.kind == EffectKind::Focus
                && effect.target == WindowId("win-b".to_owned())
                && effect.command_seq == Some(1)
        }),
        "fresh command dispatches its focus effect"
    );
    assert!(reply.command_result.is_none());
    assert_eq!(engine.command_highwater, Some(1));
}

#[test]
fn consumed_command_reconciles_without_reexecution() {
    let mut engine = engine();
    accepted_reply(
        1,
        run(&mut engine, &observe(1, facts_pair(), "win-a"), false),
    );
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
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
    let outcome = run(&mut engine, &command_report, true);
    match outcome {
        V4RunOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert!(
                !reply
                    .effects
                    .iter()
                    .any(|effect| effect.command_seq == Some(1)),
                "consumed sequences never re-execute"
            );
            assert!(reply.command_result.is_none());
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
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
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
    let latest = engine.latest_published_presentation_rev_v4;
    let home = engine
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
    let issuing = engine.state_revision.checked_add(1).expect("issuing");
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
    match engine.run_accepted_v4(&rerun, &issue(3), true, issuing) {
        V4RunOutcome::CommandConsumed(reply) => {
            assert!(reply.command_consumed);
            assert!(
                !reply
                    .effects
                    .iter()
                    .any(|effect| effect.command_seq == Some(1)),
                "consumed reruns never re-execute: {:?}",
                reply.effects
            );
            assert!(
                reply.command_result.is_none(),
                "consumed reruns never re-validate the stale renderer: {:?}",
                reply.command_result
            );
            assert_eq!(reply.state_revision, issuing);
            assert_eq!(reply.presentation.rev, issuing);
            assert_eq!(engine.latest_published_presentation_rev_v4, issuing);
            assert_eq!(engine.state_revision, issuing);
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
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let stale = RendererInput {
        rev: engine.latest_published_presentation_rev_v4 + 9,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
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
    let sent = accepted_reply(
        3,
        run(
            &mut engine,
            &report(
                3,
                base_windows(),
                "w0",
                ingress("w0"),
                Some(2),
                Some(WorldCommand::Send {
                    window: WindowId("w0".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-2".to_owned(),
                    follow: true,
                }),
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(
        sent.command_result.is_none(),
        "occupying send executes, got {:?}",
        sent.command_result
    );
    assert!(
        engine
            .ring_occupied(&OutputId("out-a".to_owned()))
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
            window: WindowId("wSticky".to_owned()),
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
    let reply = accepted_reply(4, run(&mut engine, &refused, false));
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::SendRefused
        }),
        "sticky send refuses"
    );
    assert_eq!(engine.command_highwater, Some(3));
    assert_eq!(
        engine.mapping, control.mapping,
        "refusal preserves the commandless image"
    );
    assert_eq!(
        engine.creation_counter, control.creation_counter,
        "creation counter matches the commandless image"
    );
    assert_eq!(
        engine.retained, control.retained,
        "no transfer minted for the refused mover"
    );
    assert_eq!(
        engine.members, control.members,
        "membership matches the commandless image"
    );
    assert!(
        !engine
            .retained
            .iter()
            .any(|transfer| matches!(&transfer.target.kind, crate::world_report::TransferKind::Window(w) if w.0 == "wSticky")),
        "refused sticky leaves no flight: {:?}",
        engine.retained
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
    assert!(
        !reply
            .effects
            .iter()
            .any(|effect| effect.target.0 == "wSticky"),
        "no effect for the refused mover: {:?}",
        reply.effects
    );
    assert_eq!(
        engine
            .consumed_frames
            .get(&WindowId("w0".to_owned()))
            .map(|attempt| attempt.attempt_ms),
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
        direction: "left".to_owned(),
    };
    let outcome = run(
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
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        })
    );
    assert_eq!(engine.command_highwater, Some(1));
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
            direction: "left".to_owned(),
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
    let reply = accepted_reply(2, run(&mut engine, &hotplug, false));
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        }),
        "same-report view cannot authorize the frozen intent"
    );
    assert_eq!(engine.command_highwater, Some(1));
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
        .staged
        .homing
        .get(&WindowId("win-a".to_owned()))
        .expect("homed window")
        .clone();
    let renderer = RendererInput {
        rev: engine.latest_published_presentation_rev_v4,
        target: RenderTarget::Workspace {
            out: home.output.clone(),
            ws: home.workspace.clone(),
        },
    };
    let command = WorldCommand::Focus {
        window: WindowId("win-gone".to_owned()),
        direction: String::new(),
    };
    let outcome = run(
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
    assert_eq!(
        reply.command_result,
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
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let stale = RendererInput {
        rev: engine.latest_published_presentation_rev_v4 + 1,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    let outcome = run(
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
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StaleRenderTarget
        })
    );
    assert!(
        !reply
            .effects
            .iter()
            .any(|effect| effect.command_seq == Some(1)),
        "refused commands dispatch nothing"
    );
    assert_eq!(engine.command_highwater, Some(1));
    assert_eq!(
        engine.latest_published_presentation_rev_v4, 2,
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
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let older = RendererInput {
        rev: engine.latest_published_presentation_rev_v4,
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
        engine.latest_published_presentation_rev_v4, 2,
        "intervening observation advances the published revision"
    );
    let reply = accepted_reply(
        3,
        run(
            &mut engine,
            &report(
                3,
                facts_pair(),
                "win-a",
                ingress("win-a"),
                Some(1),
                Some(WorldCommand::Focus {
                    window: WindowId("win-b".to_owned()),
                    direction: String::new(),
                }),
                Some(older),
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(
        reply.effects.iter().any(|effect| {
            effect.kind == EffectKind::Focus
                && effect.target == WindowId("win-b".to_owned())
                && effect.command_seq == Some(1)
        }),
        "older unchanged renderer rows proceed: {:?}",
        reply.effects
    );
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
        .staged
        .homing
        .get(&WindowId("win-a".to_owned()))
        .expect("homed window")
        .clone();
    let old_renderer = RendererInput {
        rev: engine.latest_published_presentation_rev_v4,
        target: RenderTarget::Window {
            out: old_home.output.clone(),
            ws: old_home.workspace.clone(),
            win: WindowId("win-a".to_owned()),
        },
    };
    let moved = accepted_reply(
        2,
        run(
            &mut engine,
            &report(
                2,
                facts_pair(),
                "win-a",
                ingress("win-a"),
                Some(1),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-2".to_owned(),
                    follow: true,
                }),
                Some(old_renderer.clone()),
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(
        moved.command_result.is_none(),
        "old target is still valid pre-move, got {:?}",
        moved.command_result
    );
    let new_home = engine
        .core_homing(&WindowId("win-a".to_owned()))
        .expect("rehomed window");
    assert_ne!(new_home, old_home, "the genuine run rehomed the target");
    let reply = accepted_reply(
        3,
        run(
            &mut engine,
            &report(
                3,
                facts_pair(),
                "win-a",
                ingress("win-a"),
                Some(2),
                Some(WorldCommand::Focus {
                    window: WindowId("win-a".to_owned()),
                    direction: String::new(),
                }),
                Some(old_renderer),
                Vec::new(),
            ),
            false,
        ),
    );
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::StaleRenderTarget
        })
    );
    assert_eq!(engine.command_highwater, Some(2));
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
        .staged
        .homing
        .get(&WindowId("win-b".to_owned()))
        .expect("homed window")
        .clone();
    let renderer = RendererInput {
        rev: engine.latest_published_presentation_rev_v4,
        target: RenderTarget::Window {
            out: home.output.clone(),
            ws: home.workspace.clone(),
            win: WindowId("win-b".to_owned()),
        },
    };
    let command = WorldCommand::Focus {
        window: WindowId("win-b".to_owned()),
        direction: String::new(),
    };
    let outcome = run(
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
    assert!(reply.command_result.is_none());
    assert!(
        reply.effects.iter().any(|effect| {
            effect.kind == EffectKind::Focus
                && effect.target == WindowId("win-b".to_owned())
                && effect.command_seq == Some(1)
        }),
        "exact rows at the latest published revision proceed"
    );
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
        fields: Vec::new(),
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
    assert_eq!(engine.state_revision, 2);
    assert_eq!(engine.prior_context.prev_focused_global, "win-b");
}

#[test]
fn exhausted_revision_refuses_admitted_candidate() {
    let mut engine = engine();
    engine.state_revision = u64::MAX;
    let report = observe(1, facts_pair(), "win-a");
    match engine.run_accepted_v4(&report, &issue(1), false, 1) {
        V4RunOutcome::Rejected { kind, .. } => assert_eq!(kind, "revision-exhausted"),
        other => panic!("expected revision exhaustion, got {other:?}"),
    }
    assert!(engine.members.is_empty(), "exhausted runs mutate nothing");
}

#[test]
fn creation_exhaustion_propagates_as_envelope_refusal() {
    let mut engine = engine();
    let single = || vec![fact("win-a", "out-a", 0)];
    accepted_reply(1, run(&mut engine, &observe(1, single(), "win-a"), false));
    engine.creation_counter = u64::MAX;
    // Occupy the live trailing empty with a same-output send so the next
    // trailing selection must create (the exhaustion path) instead of
    // reusing. The sole window relocates its session onto the target.
    let occupy = accepted_reply(
        2,
        run(
            &mut engine,
            &report(
                2,
                single(),
                "win-a",
                ingress("win-a"),
                Some(1),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-2".to_owned(),
                    follow: false,
                }),
                None,
                Vec::new(),
            ),
            false,
        ),
    );
    assert!(
        occupy.command_result.is_none(),
        "occupying send executes, got {:?}",
        occupy.command_result
    );
    let revision_before = engine.state_revision;
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
        engine.command_highwater,
        Some(1),
        "envelope consumes no sequence"
    );
    assert_eq!(
        engine.state_revision, revision_before,
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
            workspace: source_view.clone(),
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
    let reply = accepted_reply(2, run(&mut engine, &refused, false));
    assert_eq!(
        reply.command_result,
        Some(CommandResult::Rejected {
            kind: CommandRejectKind::MigrationRefused
        }),
        "unknown minimized sticky refuses migration"
    );
    assert_eq!(engine.command_highwater, Some(1));
    assert_eq!(
        engine.mapping, control.mapping,
        "refusal preserves the commandless image"
    );
    assert_eq!(
        engine.members, control.members,
        "membership matches the commandless image"
    );
    assert_eq!(
        engine.retained, control.retained,
        "no migration flight survives the refusal"
    );
    let target_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(source_view.clone()),
    };
    assert!(
        engine.engine.session(&target_key).is_none(),
        "target session absent after refusal"
    );
    let source_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(source_view),
    };
    assert!(
        engine.engine.session(&source_key).is_some(),
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
    let on_reply = accepted_reply(
        2,
        run(
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
                    window: WindowId("win-s".to_owned()),
                    value: true,
                },
            ),
            false,
        ),
    );
    assert!(
        on_reply.command_result.is_none(),
        "sticky-on executes: {:?}",
        on_reply.command_result
    );
    assert_eq!(
        sticky_state_row(&on_reply, "win-s"),
        Some((true, ScopeAuthority::Controlled, Some(1))),
        "native Sticky=true target publishes: {:?}",
        on_reply.windows
    );
    let on_rev = engine
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
        setter: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(true),
        attempt_ms: 7,
    }];
    let all_desktop = accepted_reply(3, run(&mut engine, &on_proof, false));
    let on_binding = engine
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("on binding");
    assert_eq!(on_binding.disposition, AttemptOutcome::Applied);
    assert!(on_binding.spent, "on-proof correlates");
    // Collected here, asserted only after the off command below so a
    // fencing regression fails at the off command with ScopeHeld proof.
    let on_home = engine.core_homing(&WindowId("win-s".to_owned()));
    let on_hold = engine.hold_reasons.clone();
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
    let off_reply = accepted_reply(
        4,
        run(
            &mut engine,
            &host_command(
                4,
                vec![known_fact("win-a", "out-a", 0, HOST_N1), sticky_off],
                "win-a",
                native(),
                2,
                WorldCommand::SetSticky {
                    window: WindowId("win-s".to_owned()),
                    value: false,
                },
            ),
            false,
        ),
    );
    assert!(
        off_reply.command_result.is_none(),
        "sticky-off executes, not scope-held: {:?}",
        off_reply.command_result
    );
    assert_eq!(
        on_home,
        Some(home.clone()),
        "positive all-desktop never re-homes"
    );
    assert!(
        on_hold.is_empty() && !engine.hold_reasons.contains_key(&home),
        "retained all-desktop home fences nothing: before={on_hold:?} after={:?}",
        engine.hold_reasons
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
        setter: FrameSetter::Accepted,
        pre: Some(true),
        post: Some(false),
        attempt_ms: 7,
    }];
    let cleared = accepted_reply(5, run(&mut engine, &proof, false));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-s".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(binding.sticky_meta.as_ref().expect("meta").followup_done);
    let member = engine
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
        .core_homing(&WindowId("win-f".to_owned()))
        .expect("adopted win-f keeps its home");
    // Collected here, asserted only after the off command below so a
    // fencing regression fails at the off command with ScopeHeld proof.
    let adopted_hold = engine.hold_reasons.clone();
    assert!(
        engine
            .members
            .get(&WindowId("win-f".to_owned()))
            .expect("member")
            .floating,
        "adopted sticky keeps prior float"
    );
    let mut adopted_cmd = float_fact("win-f", "out-a", 100, 100);
    adopted_cmd.sticky = Some(true);
    adopted_cmd.native_workspace = NativeWorkspaceFact::Omitted;
    let off_reply = accepted_reply(
        3,
        run(
            &mut engine,
            &host_command(
                3,
                vec![known_fact("win-a", "out-a", 0, HOST_N1), adopted_cmd],
                "win-a",
                native(),
                1,
                WorldCommand::SetSticky {
                    window: WindowId("win-f".to_owned()),
                    value: false,
                },
            ),
            false,
        ),
    );
    assert!(
        off_reply.command_result.is_none(),
        "native sticky-off executes, not scope-held: {:?}",
        off_reply.command_result
    );
    assert!(
        adopted_hold.is_empty() && !engine.hold_reasons.contains_key(&home),
        "adopted native sticky fences nothing: before={adopted_hold:?} after={:?}",
        engine.hold_reasons
    );
    assert_eq!(
        sticky_state_row(&off_reply, "win-f"),
        Some((false, ScopeAuthority::Controlled, Some(1))),
        "native Sticky=false target publishes: {:?}",
        off_reply.windows
    );
    let rev = engine
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
        setter: FrameSetter::Accepted,
        pre: Some(true),
        post: Some(false),
        attempt_ms: 7,
    }];
    accepted_reply(4, run(&mut engine, &proof, false));
    let binding = engine
        .retained_states
        .get(&(WindowId("win-f".to_owned()), StateProperty::Sticky))
        .expect("binding");
    assert_eq!(binding.disposition, AttemptOutcome::Applied);
    assert!(
        engine
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
            engine.core_homing(&WindowId("win-a".to_owned())),
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
