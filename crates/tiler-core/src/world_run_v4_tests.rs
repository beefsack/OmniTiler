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
use crate::world_directional::tests::{fact, screen};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_report::{
    CommandRejectKind, CommandResult, EffectKind, FrameAttempt, FrameSetter, HeldPresentationRow,
    HeldWindowRow, IntentRecord, PresentationIngressV4, PresentationPagerFrameV4,
    PresentationRowV4, RenderTarget, RendererInput, RowDiagnosticKind, ScopeAuthority,
    ScreenReport, SendSource, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand, WorldReportV4,
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
