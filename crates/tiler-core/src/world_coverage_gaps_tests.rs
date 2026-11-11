//! Backfill for deleted native-world coverage (stage 2a): admission
//! focus-once discriminants, send stay/follow focus across both directions,
//! migration fences, and float-boundary regressions. Logical-first: screens
//! plus window facts only; native-identity/aligned-grid binding is
//! superseded (logical ids authorize homing), everything else below is live
//! v2 behavior through `WorldEngine::take_admission_focus`,
//! `settle_send`, and `start_migration`.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameSetter, HoldReason, MemberAttempt,
    PresentationIngressV4, PresentationRowV4, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority,
    ScreenReport, TransferKind, WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef, WorldCommand,
    WorldReportV4,
};
use crate::world_report_v4_checks::{
    grade_invalid_screens_v4 as grade_invalid_screens,
    validate_report_v4_reason as validate_report_reason,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_sized(output: &str, x: i32, w: i32, h: i32, _gap: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, w, h)),
        work_area: Some(rect(x, 0, w, h)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn roomy(output: &str, x: i32) -> ScreenReport {
    screen_sized(output, x, 800, 600, 0)
}

fn tiny(output: &str, x: i32) -> ScreenReport {
    screen_sized(output, x, 10, 10, 8)
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

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        transient: Some(true),
        ..fact(window, output, x)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    closed: Vec<&str>,
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
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        held_attempts: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn engine_with_zero_outer_gap() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "outer".to_owned(),
            crate::connection::RawSettingValue::Number(0.0),
        )],
    )
    .expect("generation")
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn view_of(reply: &crate::world_report::WorldReplyV4, output: &str) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

fn two_roomy() -> Vec<ScreenReport> {
    vec![roomy("out-a", 0), roomy("out-b", 800)]
}

#[test]
fn burst_batch_pins_last_eligible_newcomer_once() {
    // Two newcomers in one report carry exactly one focus intent, pinned
    // to the last eligible newcomer, never one per newcomer, never replayed.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("w2", "out-a", 200),
                fact("w3", "out-a", 300),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "focus never publishes as wire effects: {arrival:?}"
    );
    let request = arrival.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "w3");
    assert_eq!(request.id, format!("focus-{}", arrival.state_revision));
    assert_eq!(request.issued_revision, arrival.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("w2", "out-a", 200),
                fact("w3", "out-a", 300),
            ],
            "w3",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "burst never replays: {:?}",
        echo.effects
    );
}

#[test]
fn explicit_tile_retry_failure_mints_no_focus_and_never_loops() {
    // The tiny seed capacity-floats its focused anchor; an explicit
    // unfloat retry re-floats for lack of space (D5 no-space): the Float
    // result proves it, no focus mints, the row stays floated, and a
    // repeat proves the same without looping.
    let mut engine = engine_with_zero_outer_gap();
    let screens = vec![tiny("out-a", 0)];
    let seed: Vec<WindowFact> = (0..5)
        .map(|i| fact(&format!("w{i}"), "out-a", i * 10))
        .collect();
    let first = accepted(
        &mut engine,
        &report(1, screens.clone(), seed.clone(), "w0", vec![], None, None),
    );
    assert!(first.effects.is_empty(), "seed quiet: {:?}", first.effects);
    let floated: Vec<String> = first
        .windows
        .iter()
        .filter(|row| row.floating)
        .map(|row| row.lifetime.0.clone())
        .collect();
    assert!(
        !floated.is_empty(),
        "exhaustion floats: {:?}",
        first.windows
    );
    let victim = floated[0].clone();
    // Retries echo the floated observation (native truth), exactly like the
    // pre-replacement fixture.
    let echo: Vec<WindowFact> = seed
        .iter()
        .map(|w| {
            let mut w = w.clone();
            if w.window.0 == victim {
                w.transient = Some(true);
            }
            w
        })
        .collect();
    for seq in [2u64, 3] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                screens.clone(),
                echo.clone(),
                "w1",
                vec![],
                Some(seq),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: victim.clone(),
                    float_rect: None,
                })),
            ),
        );
        match &reply.command_result {
            Some(CommandResult::Float {
                window,
                rect,
                no_space,
            }) => {
                assert_eq!(window.0, victim, "retry names the victim");
                assert_eq!(*rect, None, "still no tile rect");
                assert!(*no_space, "still exhausted");
            }
            other => panic!("expected float result, got {other:?}"),
        }
        assert!(
            reply.effects.is_empty(),
            "failed retry mints no focus: {:?}",
            reply.effects
        );
        assert!(row_of(&reply, &victim).floating, "retry stays floated");
        assert!(
            reply.transfers.is_empty(),
            "no flight, never loops: {:?}",
            reply.transfers
        );
    }
}

#[test]
fn unexplained_excluded_newcomer_takes_no_activation() {
    // A safety-excluded newcomer (privileged game/overlay) takes no
    // membership row and never takes automatic activation; a later clean
    // report still mints nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let mut game = fact("game", "out-a", 200);
    game.privileged = Some(true);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), game],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "excluded takes no activation: {:?}",
        reply.effects
    );
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "game"),
        "safety exclusion carries no membership row: {:?}",
        reply.windows
    );
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), {
                let mut game = fact("game", "out-a", 200);
                game.privileged = Some(true);
                game
            }],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "nothing queued: {:?}",
        echo.effects
    );
    assert!(
        echo.windows.iter().all(|row| row.lifetime.0 != "game"),
        "still no membership row: {:?}",
        echo.windows
    );
}

#[test]
fn normalized_ordinary_float_newcomer_focuses_once() {
    // A newcomer arriving as an ordinary float carries the same one-shot
    // newcomer intent as a capacity float; only the unexplained shape is
    // unsafe. Re-observation mints nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                float_fact("w2", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "focus never publishes as wire effects: {arrival:?}"
    );
    let request = arrival.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "w2");
    assert_eq!(request.id, format!("focus-{}", arrival.state_revision));
    assert_eq!(request.issued_revision, arrival.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(row_of(&arrival, "w2").floating, "newcomer floats");
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                float_fact("w2", "out-a", 200),
            ],
            "w2",
            vec![],
            None,
            None,
        ),
    );
    assert!(echo.effects.is_empty(), "no replay: {:?}", echo.effects);
}

fn fixed_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: Some(400),
        max_h: Some(300),
    }
}

fn fixed_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        hints: Some(fixed_hints()),
        ..fact(window, output, x)
    }
}

#[test]
fn automatic_fixed_size_newcomer_takes_no_admission_focus() {
    // A tiled-first fixed-size arrival floats automatic (D1/D8) and never
    // takes admission focus; the spend holds across re-observation.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "automatic withholds: {:?}",
        arrival.effects
    );
    assert!(row_of(&arrival, "wf").floating, "fixed floats automatic");
    let mut wf = float_fact("wf", "out-a", 200);
    wf.hints = Some(fixed_hints());
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wf],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "spent, never replays: {:?}",
        echo.effects
    );
}

#[test]
fn adapter_asserted_automatic_float_newcomer_takes_no_admission_focus() {
    // A newcomer arriving already floating with the adapter-asserted
    // automatic origin (D6) withholds exactly like a classified arrival.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let mut wa = fact("wa", "out-a", 200);
    wa.hints = Some(fixed_hints());
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wa],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "asserted automatic withholds: {:?}",
        arrival.effects
    );
    assert!(row_of(&arrival, "wa").floating, "stays floated");
    let mut wa_echo = fact("wa", "out-a", 200);
    wa_echo.hints = Some(fixed_hints());
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wa_echo],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "spent, never replays: {:?}",
        echo.effects
    );
}

#[test]
fn fixed_tile_override_newcomer_still_mints_once() {
    // D3: an explicit user tile win is ordinary admission and keeps its
    // one-shot intent. The retired adapter verdict carries no override
    // anymore (raw facts only), and no raw-fact sequence mints the first
    // win (persisted intent records are a later increment), so the test
    // seeds the retained win unit-internally and exercises the approved
    // classifier/admission algorithm downstream of it.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    engine
        .engine
        .seed_fixed_tile_override_any(&WindowId("wt".to_owned()));
    let wt = fixed_fact("wt", "out-a", 200);
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wt],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "focus never publishes as wire effects: {arrival:?}"
    );
    let request = arrival.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "wt");
    assert_eq!(request.id, format!("focus-{}", arrival.state_revision));
    assert_eq!(request.issued_revision, arrival.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(
        !row_of(&arrival, "wt").floating,
        "override tiles: {:?}",
        row_of(&arrival, "wt")
    );
    let wt_echo = fixed_fact("wt", "out-a", 200);
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wt_echo],
            "wt",
            vec![],
            None,
            None,
        ),
    );
    assert!(echo.effects.is_empty(), "no replay: {:?}", echo.effects);
}

#[test]
fn explicit_focus_of_automatic_float_after_withhold_stays_quiet() {
    // Native focus landing on a withheld automatic float converges (the
    // float-origin lane) without minting and without wedging.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "automatic withholds: {:?}",
        arrival.effects
    );
    let mut wf = float_fact("wf", "out-a", 200);
    wf.hints = Some(fixed_hints());
    let explicit = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wf],
            "wf",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        explicit.effects.is_empty(),
        "explicit focus mints nothing: {:?}",
        explicit.effects
    );
    assert!(row_of(&explicit, "wf").floating, "still floated");
}

#[test]
fn automatic_fixed_float_keeps_withholding_across_hint_loss() {
    // Hint loss never reclassifies (D2): the retained automatic origin
    // keeps the float floated and focus-free.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "automatic withholds: {:?}",
        arrival.effects
    );
    let lost = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                float_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        lost.effects.is_empty(),
        "hint loss mints nothing: {:?}",
        lost.effects
    );
    assert!(row_of(&lost, "wf").floating, "hint loss stays floated");
}

#[test]
fn closed_automatic_lifetime_reused_as_ordinary_mints_again() {
    // Close drops the lifetime (spent set, automatic marker); reusing the
    // id as an ordinary window is a fresh admission with its own intent.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "automatic withholds: {:?}",
        arrival.effects
    );
    let closed = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w0",
            vec!["wf"],
            None,
            None,
        ),
    );
    assert!(
        closed.effects.is_empty(),
        "close mints nothing: {:?}",
        closed.effects
    );
    let readmit = accepted(
        &mut engine,
        &report(
            4,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        readmit.effects.is_empty(),
        "focus never publishes as wire effects: {readmit:?}"
    );
    let request = readmit.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "wf");
    assert_eq!(request.id, format!("focus-{}", readmit.state_revision));
    assert_eq!(request.issued_revision, readmit.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
}

#[test]
fn mixed_batch_fixed_last_matches_sequential_ordinary_focus() {
    // Batch invariance: [ordinary, fixed] in one report mints exactly
    // what sequential arrivals mint (ordinary focus once, fixed
    // withheld), regardless of the fixed newcomer arriving last.
    let screens = vec![roomy("out-a", 0)];
    let seed = vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)];
    // Sequential reference: ordinary arrives first, fixed second.
    let mut sequential = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut sequential,
        &report(1, screens.clone(), seed.clone(), "", vec![], None, None),
    );
    let seq_ordinary = accepted(
        &mut sequential,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("wa", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        seq_ordinary.effects.is_empty(),
        "focus never publishes as wire effects: {seq_ordinary:?}"
    );
    let request = seq_ordinary
        .focus_request
        .as_ref()
        .expect("admission request");
    assert_eq!(request.target.0, "wa");
    assert_eq!(request.id, format!("focus-{}", seq_ordinary.state_revision));
    assert_eq!(request.issued_revision, seq_ordinary.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    // Sequential fixed arrival is tiled-first (the classifier path);
    // the batched echo below observes the already-floated result.
    let seq_fixed = accepted(
        &mut sequential,
        &report(
            3,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("wa", "out-a", 200),
                fixed_fact("wz", "out-a", 300),
            ],
            "wa",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        seq_fixed.effects.is_empty(),
        "sequential fixed withholds: {:?}",
        seq_fixed.effects
    );
    // Batched: both arrive together with the fixed newcomer last in
    // selection (id) order.
    let mut batched = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut batched,
        &report(1, screens.clone(), seed, "", vec![], None, None),
    );
    let arrival = accepted(
        &mut batched,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("wa", "out-a", 200),
                fixed_fact("wz", "out-a", 300),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        arrival.focus_request, seq_ordinary.focus_request,
        "batch matches sequential: {:?}",
        arrival.focus_request
    );
    // Report order within the batch changes nothing: the same members
    // listed fixed-first select identically.
    let mut reordered = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut reordered,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let reordered_arrival = accepted(
        &mut reordered,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wz", "out-a", 300),
                fact("wa", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        reordered_arrival.focus_request, seq_ordinary.focus_request,
        "reordered batch matches: {:?}",
        reordered_arrival.focus_request
    );
    let mut wz_echo = float_fact("wz", "out-a", 300);
    wz_echo.hints = Some(fixed_hints());
    let echo = accepted(
        &mut batched,
        &report(
            3,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fact("wa", "out-a", 200),
                wz_echo,
            ],
            "wa",
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        echo.focus_request, seq_fixed.focus_request,
        "echo matches sequential: {:?}",
        echo.focus_request
    );
}

#[test]
fn mixed_batch_fixed_first_mints_ordinary() {
    // Batch invariance, other order: [fixed, ordinary] still mints the
    // ordinary newcomer once and withholds (spends) the fixed one.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
                fact("wo", "out-a", 300),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "focus never publishes as wire effects: {arrival:?}"
    );
    let request = arrival.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "wo");
    assert_eq!(request.id, format!("focus-{}", arrival.state_revision));
    assert_eq!(request.issued_revision, arrival.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    assert!(
        row_of(&arrival, "wf").floating,
        "fixed still floats automatic"
    );
    let mut wf = float_fact("wf", "out-a", 200);
    wf.hints = Some(fixed_hints());
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                wf,
                fact("wo", "out-a", 300),
            ],
            "wo",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "spent, never replays: {:?}",
        echo.effects
    );
}

#[test]
fn two_automatic_fixed_newcomers_take_no_focus() {
    // [fixed, fixed]: every automatic newcomer spends, none mints.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
                fixed_fact("wg", "out-a", 300),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "both withhold: {:?}",
        arrival.effects
    );
    assert!(row_of(&arrival, "wf").floating, "wf floats automatic");
    assert!(row_of(&arrival, "wg").floating, "wg floats automatic");
    let mut wf = float_fact("wf", "out-a", 200);
    wf.hints = Some(fixed_hints());
    let mut wg = float_fact("wg", "out-a", 300);
    wg.hints = Some(fixed_hints());
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wf, wg],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "spent, never replays: {:?}",
        echo.effects
    );
}

#[test]
fn windows_profile_automatic_fixed_withholds_while_ordinary_mints() {
    // Windows-carrier authorization only: the exact Windows engine profile
    // from `world_sequencer.rs` (`fixed_send_admission: true`,
    // `maximized_focus_fence: false`) withholds an adapter-asserted
    // automatic fixed float (D6/D8 retained-origin shape) while an ordinary
    // newcomer still mints admission focus. This proves core authorization
    // under the Windows profile; it does NOT prove adapter actuation or
    // native behavior.
    let config = WorldEngineConfig {
        fixed_send_admission: true,
        maximized_focus_fence: false,
    };
    for case in ["automatic-fixed", "intentional-fixed", "ordinary"] {
        let mut engine = WorldEngine::new_test_bound_with_config_and_initial_settings(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities {
                // Fixtures carry native sticky and maximize facts; the
                // lane under test is the native route.
                native_sticky: true,
                maximize: true,
                ..Default::default()
            },
            config,
            Vec::new(),
        )
        .expect("generation");
        let screens = vec![roomy("out-a", 0)];
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "",
                vec![],
                None,
                None,
            ),
        );
        let newcomer = match case {
            "automatic-fixed" => {
                let mut wa = fact("wn", "out-a", 200);
                wa.hints = Some(fixed_hints());
                wa
            }
            "intentional-fixed" => {
                let mut wa = float_fact("wn", "out-a", 200);
                wa.hints = Some(fixed_hints());
                wa
            }
            _ => fact("wn", "out-a", 200),
        };
        let arrival = accepted(
            &mut engine,
            &report(
                2,
                screens,
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), newcomer],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        if case == "automatic-fixed" {
            assert!(
                arrival.effects.is_empty(),
                "windows profile withholds automatic fixed: {:?}",
                arrival.effects
            );
            assert!(
                row_of(&arrival, "wn").floating,
                "automatic fixed floats under windows profile"
            );
        } else {
            assert!(
                arrival.effects.is_empty(),
                "focus never publishes as wire effects: {arrival:?}"
            );
            let request = arrival.focus_request.as_ref().expect("admission request");
            assert_eq!(request.target.0, "wn");
            assert_eq!(request.id, format!("focus-{}", arrival.state_revision));
            assert_eq!(request.issued_revision, arrival.state_revision);
            assert_eq!(
                request.cause,
                crate::world_report::FocusRequestCause::AutomaticAdmission
            );
            assert_eq!(request.cmd_seq, None);
            assert_eq!(request.after_shown, None);
        }
    }
}

#[test]
fn explicit_focus_command_after_withhold_succeeds_without_admission_token() {
    // Explicit user focus rides the Focus command, never admission: after
    // a withheld automatic-fixed arrival, the command emits its own
    // correlated effect with no `af-` token.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "automatic withholds: {:?}",
        arrival.effects
    );
    let mut wf = float_fact("wf", "out-a", 200);
    wf.hints = Some(fixed_hints());
    let commanded = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wf],
            "w0",
            vec![],
            Some(10),
            Some(WorldCommand::Focus {
                window: WindowId("wf".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert!(
        commanded.effects.is_empty(),
        "focus never publishes as wire effects: {commanded:?}"
    );
    let request = commanded
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "wf");
    assert_eq!(request.id, format!("focus-{}", commanded.state_revision));
    assert_eq!(request.issued_revision, commanded.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(10));
    assert_eq!(request.after_shown, None);
}

#[test]
fn maximized_sibling_suppresses_admission_without_retry() {
    // D01 native activation ownership covers maximized siblings exactly
    // like fullscreen ones: the suppressed intent spends, never retries.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    let mut max = fact("max", "out-a", 0);
    max.maximized = Some(true);
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![max.clone(), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![max, fact("w1", "out-a", 100), fact("w2", "out-a", 200)],
            "max",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        arrival.effects.is_empty(),
        "suppressed behind maximized: {:?}",
        arrival.effects
    );
    assert!(
        row_of(&arrival, "w2").allocation.is_some(),
        "still converges"
    );
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![
                {
                    let mut max = fact("max", "out-a", 0);
                    max.maximized = Some(true);
                    max
                },
                fact("w1", "out-a", 100),
                fact("w2", "out-a", 200),
            ],
            "max",
            vec![],
            None,
            None,
        ),
    );
    assert!(echo.effects.is_empty(), "never retried: {:?}", echo.effects);
}

#[test]
fn restart_adoption_stays_quiet_without_origin() {
    // A fresh core on the same generation re-adopts persisted ordinary
    // floats with no intent and no diagnostic origin; disable/enable
    // re-adoption through tiling toggles is equally quiet.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let mut restarted = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let adopt = accepted(
        &mut restarted,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("w1", "out-a", 100)],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        adopt.effects.is_empty(),
        "restart adoption quiet: {:?}",
        adopt.effects
    );
    let row = row_of(&adopt, "w1");
    assert!(row.floating, "float intent survives: {row:?}");
    assert_eq!(row.float_origin, None, "fresh core carries no origin");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Untouched,
        "no frame writes: {row:?}"
    );
    let view_a = view_of(&adopt, "out-a");
    let disabled = accepted(
        &mut restarted,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("w1", "out-a", 100)],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    assert!(
        disabled.effects.is_empty(),
        "disable quiet: {:?}",
        disabled.effects
    );
    let enabled = accepted(
        &mut restarted,
        &report(
            4,
            screens,
            vec![fact("w0", "out-a", 0), float_fact("w1", "out-a", 100)],
            "w0",
            vec![],
            Some(12),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a,
                tiled: true,
            }),
        ),
    );
    assert!(
        enabled.effects.is_empty(),
        "enable re-adoption quiet: {:?}",
        enabled.effects
    );
    assert!(row_of(&enabled, "w1").floating, "float intent restored");
}

#[test]
fn close_each_position_reflows_controlled_survivors() {
    // Closing the focused window at the first, middle, or last tree
    // position keeps the scope controlled with reflowed survivors and no
    // invented intent once the survivor is observed focused.
    for victim in ["w0", "w1", "w2"] {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let screens = vec![roomy("out-a", 0)];
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![
                    fact("w0", "out-a", 0),
                    fact("w1", "out-a", 100),
                    fact("w2", "out-a", 200),
                ],
                victim,
                vec![],
                None,
                None,
            ),
        );
        let survivor = ["w0", "w1", "w2"]
            .into_iter()
            .find(|w| *w != victim)
            .unwrap();
        let remaining: Vec<WindowFact> = ["w0", "w1", "w2"]
            .into_iter()
            .filter(|w| *w != victim)
            .map(|w| fact(w, "out-a", 0))
            .collect();
        let reply = accepted(
            &mut engine,
            &report(2, screens, remaining, survivor, vec![victim], None, None),
        );
        assert!(
            reply.effects.is_empty(),
            "{victim}: close mints no intent: {:?}",
            reply.effects
        );
        assert!(
            reply.windows.iter().all(|row| row.lifetime.0 != victim),
            "{victim}: dropped"
        );
        let scope = reply
            .scopes
            .iter()
            .find(|s| s.scope.covers_workspace(&OutputId("out-a".into()), "ws-1"))
            .expect("scope");
        assert_eq!(
            scope.authority,
            ScopeAuthority::Controlled,
            "{victim}: {scope:?}"
        );
        for row in &reply.windows {
            assert!(
                row.allocation.is_some(),
                "{victim}: {} reflows: {row:?}",
                row.lifetime.0
            );
        }
    }
}

#[test]
fn directional_sends_both_directions_name_exact_focus_without_replay() {
    // Stay right names the source MRU survivor; after exact arrival
    // settles the flight, follow left names the mover; the settled echo
    // replays nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = two_roomy();
    // Focus history: w2 then the mover, so the stay survivor is w2.
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w2", "out-a", 100),
                fact("wb", "out-b", 800),
            ],
            "w2",
            vec![],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w2", "out-a", 100),
                fact("wb", "out-b", 800),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    let stay = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w2", "out-a", 100),
                fact("wb", "out-b", 800),
            ],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("w0".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(
        stay.transfers.len(),
        1,
        "stay retains: {:?}",
        stay.transfers
    );
    assert!(!stay.transfers[0].follow, "stay carries follow=false");
    assert!(
        stay.effects.is_empty(),
        "focus never publishes as wire effects: {stay:?}"
    );
    let request = stay.focus_request.as_ref().expect("explicit focus request");
    assert_eq!(request.target.0, "w2");
    assert_eq!(request.id, format!("focus-{}", stay.state_revision));
    assert_eq!(request.issued_revision, stay.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    // Exact arrival settles; the echo replays nothing.
    let arrived = fact("w0", "out-b", 800);
    let settled = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![arrived, fact("w2", "out-a", 100), fact("wb", "out-b", 800)],
            "w2",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "arrival settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.effects.is_empty(),
        "no post-settlement replay: {:?}",
        settled.effects
    );
    // Follow left names the mover exactly.
    let view_a = view_of(&settled, "out-a");
    let _ = &view_a;
    let back = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            vec![
                fact("w0", "out-b", 800),
                fact("w2", "out-a", 100),
                fact("wb", "out-b", 800),
            ],
            "w0",
            vec![],
            Some(12),
            Some(WorldCommand::SendDirectional {
                window: WindowId("w0".to_owned()),
                direction: "left".to_owned(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        back.transfers.len(),
        1,
        "follow retains: {:?}",
        back.transfers
    );
    assert!(back.transfers[0].follow, "follow carries follow=true");
    assert!(
        back.effects.is_empty(),
        "focus never publishes as wire effects: {back:?}"
    );
    let request = back.focus_request.as_ref().expect("explicit focus request");
    assert_eq!(request.target.0, "w0");
    assert_eq!(request.id, format!("focus-{}", back.state_revision));
    assert_eq!(request.issued_revision, back.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    assert_eq!(request.after_shown, None);
    let echo = accepted(
        &mut engine,
        &report(
            6,
            screens.clone(),
            vec![
                fact("w0", "out-b", 800),
                fact("w2", "out-a", 100),
                fact("wb", "out-b", 800),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo.effects.is_empty(),
        "follow never replays: {:?}",
        echo.effects
    );
    // Mirror legs on a fresh engine: stay left names the out-b MRU
    // survivor; after exact settlement, follow right names the mover.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("wb", "out-b", 800),
                fact("wb2", "out-b", 900),
            ],
            "wb2",
            vec![],
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("wb", "out-b", 800),
                fact("wb2", "out-b", 900),
            ],
            "wb",
            vec![],
            None,
            None,
        ),
    );
    let stay_left = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("wb", "out-b", 800),
                fact("wb2", "out-b", 900),
            ],
            "wb",
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("wb".to_owned()),
                direction: "left".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(
        stay_left.transfers.len(),
        1,
        "stay retains: {:?}",
        stay_left.transfers
    );
    assert!(!stay_left.transfers[0].follow, "stay carries follow=false");
    assert!(
        stay_left.effects.is_empty(),
        "focus never publishes as wire effects: {stay_left:?}"
    );
    let request = stay_left
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "wb2");
    assert_eq!(request.id, format!("focus-{}", stay_left.state_revision));
    assert_eq!(request.issued_revision, stay_left.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    let view_a = view_of(&stay_left, "out-a");
    let _ = &view_a;
    let arrived_left = fact("wb", "out-a", 0);
    let settled_left = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![
                fact("w0", "out-a", 100),
                arrived_left,
                fact("wb2", "out-b", 900),
            ],
            "wb2",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        settled_left.transfers.is_empty(),
        "arrival settles: {:?}",
        settled_left.transfers
    );
    assert!(
        settled_left.effects.is_empty(),
        "no post-settlement replay: {:?}",
        settled_left.effects
    );
    let ahead = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            vec![
                fact("w0", "out-a", 100),
                fact("wb", "out-a", 0),
                fact("wb2", "out-b", 900),
            ],
            "wb",
            vec![],
            Some(12),
            Some(WorldCommand::SendDirectional {
                window: WindowId("wb".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        ahead.transfers.len(),
        1,
        "follow retains: {:?}",
        ahead.transfers
    );
    assert!(ahead.transfers[0].follow, "follow carries follow=true");
    assert!(
        ahead.effects.is_empty(),
        "focus never publishes as wire effects: {ahead:?}"
    );
    let request = ahead
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "wb");
    assert_eq!(request.id, format!("focus-{}", ahead.state_revision));
    assert_eq!(request.issued_revision, ahead.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    assert_eq!(request.after_shown, None);
    let echo_left = accepted(
        &mut engine,
        &report(
            6,
            screens,
            vec![
                fact("w0", "out-a", 100),
                fact("wb", "out-a", 0),
                fact("wb2", "out-b", 900),
            ],
            "wb",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        echo_left.effects.is_empty(),
        "follow never replays: {:?}",
        echo_left.effects
    );
}

#[test]
fn refused_and_held_sends_emit_no_focus_and_retain_nothing() {
    // Guarded sends (sticky, unexplained excluded) refuse with the fixed
    // vocabulary; sends touching a held scope refuse held. None mints
    // focus, none retains a flight.
    let screens = two_roomy();
    for (tag, window, x) in [("sticky", "pin", 300), ("excluded", "game", 400)] {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0)],
                "",
                vec![],
                None,
                None,
            ),
        );
        let ring: Vec<String> = first
            .workspaces
            .iter()
            .filter(|r| r.output.0 == "out-a")
            .map(|r| r.id.clone())
            .collect();
        let mut bad = fact(window, "out-a", x);
        if tag == "sticky" {
            bad.sticky = Some(true);
        } else {
            bad.privileged = Some(true);
        }
        let send = report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), bad],
            window,
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId(window.to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: true,
            }),
        );
        match engine.handle_report(&send, &[]).0 {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendRefused
                }),
                "{tag}: expected send-refused"
            ),
            other => panic!("{tag}: expected send-refused, got {other:?}"),
        }
        assert!(
            engine.retained_transfers().is_empty(),
            "{tag}: retains nothing"
        );
    }
    // Incomplete-target send: the neighbor screen without geometry is no
    // resolvable target; the fixed vocabulary refuses with no focus and no
    // flight.
    {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                two_roomy(),
                vec![fact("w0", "out-a", 0), fact("wb", "out-b", 800)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let blind_target = vec![
            roomy("out-a", 0),
            ScreenReport {
                output: OutputId("out-b".to_owned()),
                full_rect: None,
                work_area: None,
                complete: false,
                scale: 1.0,
                identity: crate::world_report::OutputIdentity::unknown(),
            },
        ];
        let send = report(
            2,
            blind_target,
            vec![fact("w0", "out-a", 0), fact("wb", "out-b", 800)],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("w0".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        );
        let revision_before = engine.state_revision();
        match engine.handle_report(&send, &[]).0 {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendTargetUnknown
                }),
                "blind target"
            ),
            other => panic!("expected send-target-unknown, got {other:?}"),
        }
        assert_eq!(
            engine.state_revision(),
            revision_before + 1,
            "refusal issues"
        );
        assert!(engine.retained_transfers().is_empty(), "retains nothing");
    }
    // Held source (inset exhaustion via an outer gap exhausting the work
    // area): the sweep holds the scope, and the send shares the held fence.
    // The clamped source still touches the neighbor so topology resolves
    // and the refusal comes from the held scope, not the target.
    {
        let mut engine = WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities {
                // Fixtures carry native sticky and maximize facts; the
                // lane under test is the native route.
                native_sticky: true,
                maximize: true,
                ..Default::default()
            },
            vec![(
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(50.0),
            )],
        )
        .expect("generation");
        let abutting = vec![
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(700, 0, 100, 100)),
                work_area: Some(rect(700, 0, 100, 100)),
                complete: true,
                scale: 1.0,
                identity: crate::world_report::OutputIdentity::unknown(),
            },
            roomy("out-b", 800),
        ];
        accepted(
            &mut engine,
            &report(
                1,
                abutting,
                vec![fact("w0", "out-a", 710), fact("wb", "out-b", 800)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let clamped = vec![
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(700, 0, 100, 100)),
                work_area: Some(rect(700, 0, 100, 100)),
                complete: true,
                scale: 1.0,
                identity: crate::world_report::OutputIdentity::unknown(),
            },
            roomy("out-b", 800),
        ];
        let send = report(
            2,
            clamped,
            vec![fact("w0", "out-a", 710), fact("wb", "out-b", 800)],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("w0".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        );
        let revision_before = engine.state_revision();
        match engine.handle_report(&send, &[]).0 {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::ScopeHeld
                }),
                "clamped source"
            ),
            other => panic!("expected scope-held, got {other:?}"),
        }
        assert_eq!(
            engine.state_revision(),
            revision_before + 1,
            "refusal issues"
        );
        assert!(engine.retained_transfers().is_empty(), "retains nothing");
    }
}

#[test]
fn second_migration_explicit_refuses_while_directional_resolves_current() {
    // One flight per source workspace: a retained migration fences an
    // explicit second commit naming the same source workspace with the
    // migration vocabulary and no mutation. A direction-only retry cannot
    // name that stale workspace: it resolves the current source view and
    // retains its own flight (the same commit the old production route
    // made by pinning the live view at dispatch).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = two_roomy();
    let first = accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let revision_before = engine.state_revision();
    let flight = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0)],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    assert!(
        flight
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "flight: {:?}",
        flight.transfers
    );
    // The explicit retry names the outstanding source workspace and
    // refuses on the flight fence with no mutation.
    let mut retry_explicit = report(
        3,
        screens.clone(),
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(12),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: view_a.clone(),
        }),
    );
    retry_explicit.action_foreground = Some(OutputId("out-a".to_owned()));
    retry_explicit.action_outputs =
        vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    match engine.handle_report(&retry_explicit, &[]).0 {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "explicit retry"
        ),
        other => panic!("explicit retry: expected migration-refused, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        revision_before + 2,
        "explicit refusal issues"
    );
    // The direction-only retry resolves the current source view with the
    // same staged context and retains its own flight.
    let mut retry_directional = report(
        4,
        screens.clone(),
        vec![fact("w0", "out-a", 0)],
        "w0",
        vec![],
        Some(13),
        Some(WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }),
    );
    retry_directional.action_foreground = Some(OutputId("out-a".to_owned()));
    retry_directional.action_outputs =
        vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    let applied = match engine.handle_report(&retry_directional, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("directional retry: expected apply, got {other:?}"),
    };
    let flight = applied
        .command_result
        .clone()
        .and_then(|result| match result {
            crate::world_report::CommandResult::Migration { source, .. } => Some(source),
            _ => None,
        })
        .expect("migration provenance");
    assert_eq!(flight.output.0, "out-a", "directional resolves live source");
    assert_eq!(
        engine.state_revision(),
        revision_before + 3,
        "directional retry commits its own flight"
    );
    // The original non-empty flight survives with stable identity; the
    // direction-only retry migrated an empty workspace, so it retains
    // no flight (zero members means zero membership to await) while
    // still reporting Migration provenance above.
    assert!(
        engine
            .retained_transfers()
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11"),
        "original flight stable: {:?}",
        engine.retained_transfers()
    );
    assert!(
        engine
            .retained_transfers()
            .iter()
            .all(|t| !(matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-13")),
        "empty directional retains no flight: {:?}",
        engine.retained_transfers()
    );
}

#[test]
fn floating_source_migration_carries_without_engine_tree() {
    // Accepted behavior: a floating workspace holds no retained session,
    // so migration carries by sessionless core-owned membership remap (no
    // fabricated tree or focus). Members, tiling mode, source refill, and
    // target view commit; target-homed arrival retires the parent with no
    // echo proof; proven refusal rolls back to observed.
    let screens = two_roomy();
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "",
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    let Some(CommandResult::Migration {
        source,
        target,
        members,
        floats,
        ..
    }) = reply.command_result.clone()
    else {
        panic!(
            "expected migration provenance, got {:?}",
            reply.command_result
        );
    };
    assert_eq!(
        (source.output.0.as_str(), source.workspace.0.as_str()),
        ("out-a", view_a.as_str())
    );
    assert_eq!(
        (target.output.0.as_str(), target.workspace.0.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(
        members.iter().map(|w| w.0.clone()).collect::<Vec<_>>(),
        vec!["w0".to_owned(), "wf".to_owned()],
        "sessionless carry keeps the members"
    );
    assert_eq!(floats, 1, "relocated float exception count");
    // The floating mode rides with the moved unit.
    let target_row = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-b" && row.id == view_a)
        .expect("target row");
    assert_eq!(
        target_row.tiling_mode,
        crate::world_report::TilingMode::Floating,
        "mode rides the id"
    );
    let migration = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration transfer");
    assert_eq!(migration.operation_id, "op-12");
    // No refill child rides the native-free route; the frozen binding
    // covers both outputs with distinct selected homes instead.
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::View)),
        "no refill child: {:?}",
        reply.transfers
    );
    let affected = {
        let binding = engine
            .retained_selection
            .values()
            .find(|binding| binding.parent_op == "op-12")
            .expect("frozen selection binding");
        let mut affected = binding.affected.clone();
        affected.sort();
        affected
    };
    assert_eq!(
        affected,
        vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())]
    );
    assert!(
        reply.effects.is_empty(),
        "no focus writes: {:?}",
        reply.effects
    );
    assert!(
        row_of(&reply, "w0").allocation.is_none(),
        "no frame writes: {:?}",
        row_of(&reply, "w0")
    );
    // No fake engine tree is built for either side.
    let target_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    assert!(
        engine.engine().session(&target_key).is_none(),
        "sessionless carry builds no tree"
    );
    // Unchanged membership stays stable with no replay: one migration
    // flight, desired homing at the target, same identities.
    let lag = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert_eq!(
        lag.transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Migration))
            .count(),
        1,
        "single flight survives: {:?}",
        lag.transfers
    );
    assert!(
        lag.effects.is_empty(),
        "lag replays nothing: {:?}",
        lag.effects
    );
    for window in ["w0", "wf"] {
        assert_eq!(
            (
                row_of(&lag, window).output.0.as_str(),
                row_of(&lag, window).workspace.as_str()
            ),
            ("out-b", view_a.as_str()),
            "{window} desired homing at the target"
        );
    }
    // Arrival retires the sessionless carry with no echo proof: the
    // parent drops once every member homing names the target (v4 has no
    // view-echo lane; the legacy views echo here was inert).
    let arrived = fact("w0", "out-b", 800);
    let arrived_float = float_fact("wf", "out-b", 900);
    let arrival = report(
        5,
        screens.clone(),
        vec![arrived, arrived_float],
        "w0",
        vec![],
        None,
        None,
    );
    let settled = accepted(&mut engine, &arrival);
    assert!(
        settled.transfers.is_empty(),
        "arrival retires with no echo wait: {:?}",
        settled.transfers
    );
}

#[test]
fn floating_source_migration_refusal_after_ack_rolls_back() {
    // Proven refusal with members still observed at the source rolls the
    // sessionless carry back with stable identities: no transfers, the
    // logical id and its floating mode return to the source ring, and
    // members rehome at the source. No tree is fabricated either way.
    let screens = two_roomy();
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let pre_index: usize = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .position(|id| id == view_a)
        .expect("pre index");
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "",
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    assert_eq!(engine.retained_transfers().len(), 1, "flight retained");
    let mut refused = report(
        4,
        screens.clone(),
        vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
        "w0",
        vec![],
        None,
        None,
    );
    refused.member_attempts = vec![
        MemberAttempt {
            operation_id: "op-12".to_owned(),
            window: WindowId("w0".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            target_workspace: view_a.clone(),
            setter: FrameSetter::Refused,
            pre: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(view_a.clone()),
            }),
            post: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(view_a.clone()),
            }),
            attempt_ms: 4,
        },
        MemberAttempt {
            operation_id: "op-12".to_owned(),
            window: WindowId("wf".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            target_workspace: view_a.clone(),
            setter: FrameSetter::Refused,
            pre: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(view_a.clone()),
            }),
            post: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(view_a.clone()),
            }),
            attempt_ms: 4,
        },
    ];
    let settled = accepted(&mut engine, &refused);
    assert!(
        settled.transfers.is_empty(),
        "refusal settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.effects.is_empty(),
        "no focus writes: {:?}",
        settled.effects
    );
    let post_ring: Vec<String> = settled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(
        post_ring.iter().position(|id| id == &view_a),
        Some(pre_index),
        "ring position restored: {post_ring:?}"
    );
    assert_eq!(
        settled
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .as_deref(),
        Some(view_a.as_str()),
        "source view restored"
    );
    // SAME-reply coherence: the rollback reply itself must publish the
    // restored source scope as Untouched with members homed there, and
    // no stale target scope/member rows. No verdict replay, no effects.
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations on floating rollback: {:?}",
        settled.windows
    );
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope.covers_workspace(&OutputId("out-a".into()), &view_a))
        .expect("floating source scope on rollback reply");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Untouched,
        "floating source Untouched on rollback: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| {
            !matches!(&s.scope, crate::world_report::AuthorityScope::Workspace { output, workspace }
                if output.0 == "out-b" && workspace.0 == view_a)
        }),
        "no stale floating target scope: {:?}",
        settled.scopes
    );
    for window in ["w0", "wf"] {
        let row = row_of(&settled, window);
        assert_eq!(
            (row.output.0.as_str(), row.workspace.as_str()),
            ("out-a", view_a.as_str()),
            "{window} rehomed at source on rollback reply: {row:?}"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Untouched,
            "{window} membership Untouched on rollback: {row:?}"
        );
        assert!(
            row.allocation.is_none() && row.effective_frame.is_none(),
            "{window} no float frame writes on rollback: {row:?}"
        );
    }
    assert!(
        settled
            .windows
            .iter()
            .all(|row| row.workspace != view_a || row.output.0 == "out-a"),
        "no stale floating target member rows: {:?}",
        settled.windows
    );
    assert!(
        engine.retained_transfers().is_empty(),
        "nothing retained after rollback"
    );
    // The next clean report proves the floating mode rode back with the
    // id: the scope publishes untouched again with no residue elsewhere.
    let calm = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        calm.transfers.is_empty(),
        "stays settled: {:?}",
        calm.transfers
    );
    assert!(calm.effects.is_empty(), "stays quiet: {:?}", calm.effects);
    let scope = calm
        .scopes
        .iter()
        .find(|s| s.scope.covers_workspace(&OutputId("out-a".into()), &view_a))
        .expect("scope row");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Untouched,
        "floating mode restored: {scope:?}"
    );
    assert!(
        calm.scopes.iter().all(|s| {
            !matches!(&s.scope, crate::world_report::AuthorityScope::Workspace { output, workspace }
                    if output.0 == "out-b" && workspace.0 == view_a)
        }),
        "no residue at the target: {:?}",
        calm.scopes
    );
    for window in ["w0", "wf"] {
        let row = row_of(&calm, window);
        assert_eq!(
            (row.output.0.as_str(), row.workspace.as_str()),
            ("out-a", view_a.as_str()),
            "{window} rehomed at the source: {row:?}"
        );
    }
}

#[test]
fn tiled_workspace_migration_carries_float_members() {
    // A tiled workspace carrying a float exception migrates with the
    // float as membership (no frame writes), proven by the Migration
    // result. The shared commit path serves both this and the floating
    // source carry.
    let screens = two_roomy();
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    // Admit the float exception first so the migration report carries
    // no newcomer admission alongside the command.
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_fact("wf", "out-a", 200)],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
        ),
    );
    let migration = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration flight");
    assert_eq!(migration.target_output.0, "out-b");
    let Some(CommandResult::Migration {
        members, floats, ..
    }) = reply.command_result.clone()
    else {
        panic!(
            "expected migration provenance, got {:?}",
            reply.command_result
        );
    };
    assert!(
        members.iter().any(|w| w.0 == "w0"),
        "tiled member: {members:?}"
    );
    assert!(
        members.iter().any(|w| w.0 == "wf"),
        "float exception carried: {members:?}"
    );
    assert_eq!(floats, 1, "relocated float count");
    assert!(
        reply.effects.is_empty(),
        "migration mints no focus: {:?}",
        reply.effects
    );
}

#[test]
fn float_send_into_empty_and_occupied_trailings_stays_controlled() {
    // Regression: a floating exception sent into an empty tiled trailing
    // publishes Controlled membership with an untouched frame (exactly
    // like the occupied path), holds across lag, and settles on the exact
    // receipt. No Unreadable anywhere.
    let screens = two_roomy();
    // Empty trailing, lone float, stay: emptied source names no focus.
    {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0)],
                "",
                vec![],
                None,
                None,
            ),
        );
        let ring: Vec<String> = first
            .workspaces
            .iter()
            .filter(|r| r.output.0 == "out-a")
            .map(|r| r.id.clone())
            .collect();
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![float_fact("wf", "out-a", 200)],
                "wf",
                vec![],
                None,
                None,
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![float_fact("wf", "out-a", 200)],
                "wf",
                vec![],
                Some(11),
                Some(WorldCommand::Send {
                    window: WindowId("wf".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: ring[1].clone(),
                    follow: false,
                }),
            ),
        );
        assert_eq!(
            reply.transfers.len(),
            1,
            "flight retained: {:?}",
            reply.transfers
        );
        let row = row_of(&reply, "wf");
        assert_eq!(
            row.workspace, ring[1],
            "homed at the empty trailing: {row:?}"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{row:?}"
        );
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
        assert!(row.allocation.is_none(), "{row:?}");
        assert!(
            reply
                .scopes
                .iter()
                .all(|s| matches!(s.authority, ScopeAuthority::Controlled)),
            "no holds: {:?}",
            reply.scopes
        );
        assert!(
            reply.effects.is_empty(),
            "emptied source names nothing: {:?}",
            reply.effects
        );
        // Lag keeps the flight with no replay; the member-attempt proof settles.
        let lag = accepted(
            &mut engine,
            &report(
                4,
                screens.clone(),
                vec![float_fact("wf", "out-a", 200)],
                "wf",
                vec![],
                None,
                None,
            ),
        );
        assert_eq!(lag.transfers.len(), 1, "lag holds: {:?}", lag.transfers);
        assert!(
            lag.effects.is_empty(),
            "lag replays nothing: {:?}",
            lag.effects
        );
        let arrived = float_fact("wf", "out-a", 200);
        let mut settle = report(5, screens.clone(), vec![arrived], "wf", vec![], None, None);
        settle.member_attempts.push(MemberAttempt {
            operation_id: "op-11".to_owned(),
            window: WindowId("wf".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: ring[1].clone(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId(ring[1].clone()),
            }),
            attempt_ms: 11,
        });
        let settled = accepted(&mut engine, &settle);
        assert!(
            settled.transfers.is_empty(),
            "exact receipt settles: {:?}",
            settled.transfers
        );
        assert!(
            settled
                .scopes
                .iter()
                .all(|s| matches!(s.authority, ScopeAuthority::Controlled)),
            "settled clean: {:?}",
            settled.scopes
        );
    }
    // Occupied trailing with a tiled survivor: stay names the survivor.
    {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0)],
                "",
                vec![],
                None,
                None,
            ),
        );
        let ring: Vec<String> = first
            .workspaces
            .iter()
            .filter(|r| r.output.0 == "out-a")
            .map(|r| r.id.clone())
            .collect();
        let resident = fact("wr", "out-a", 300);
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![
                    fact("w0", "out-a", 0),
                    float_fact("wf", "out-a", 100),
                    resident.clone(),
                ],
                "wf",
                vec![],
                None,
                None,
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![
                    fact("w0", "out-a", 0),
                    float_fact("wf", "out-a", 100),
                    resident,
                ],
                "wf",
                vec![],
                Some(11),
                Some(WorldCommand::Send {
                    window: WindowId("wf".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: ring[1].clone(),
                    follow: false,
                }),
            ),
        );
        let row = row_of(&reply, "wf");
        assert_eq!(
            (row.workspace.as_str(), row.output.0.as_str()),
            (ring[1].as_str(), "out-a"),
            "{row:?}"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{row:?}"
        );
        assert_eq!(row.frame_authority, ScopeAuthority::Untouched, "{row:?}");
        assert!(
            reply.effects.is_empty(),
            "focus never publishes as wire effects: {reply:?}"
        );
        let request = reply
            .focus_request
            .as_ref()
            .expect("explicit focus request");
        assert_eq!(request.target.0, "w0");
        assert_eq!(request.id, format!("focus-{}", reply.state_revision));
        assert_eq!(request.issued_revision, reply.state_revision);
        assert_eq!(
            request.cause,
            crate::world_report::FocusRequestCause::ExplicitCommand
        );
        assert_eq!(request.cmd_seq, Some(11));
        assert_eq!(request.after_shown, None);
        assert!(
            reply
                .scopes
                .iter()
                .all(|s| matches!(s.authority, ScopeAuthority::Controlled)),
            "no holds: {:?}",
            reply.scopes
        );
    }
}

#[test]
fn fullscreen_toggle_float_round_trip_on_world_command_path() {
    // Real ENGINE regression for the fullscreen float toggle (not Session
    // only, no mock broker): the ToggleFloat WorldReport command path must
    // stage/commit the world lifecycle exactly like the Session unit does.
    // Single toggle under fullscreen floats beneath the overlay (subject
    // row floated, sibling reflows, no subject-targeted effects); a second
    // toggle under the retained fullscreen overlay re-tiles invisibly.
    // Independent fresh-engine legs then prove the actual native exit
    // (fullscreen cleared, no command) converges: float stays floated on
    // its durable placement, re-tiled stays tiled.
    let area = |r: Rect| i64::from(r.w) * i64::from(r.h);
    // Centered 60% fallback over the 800x600 roomy work area.
    let fallback = rect(160, 120, 480, 360);
    let fullscreen_frame = rect(0, 0, 800, 600);
    let mut engine = engine_with_zero_outer_gap();
    let screens = vec![roomy("out-a", 0)];
    let seed = vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)];
    let first = accepted(
        &mut engine,
        &report(1, screens.clone(), seed, "w0", vec![], None, None),
    );
    let w0_tile_alloc = row_of(&first, "w0").allocation;
    assert!(
        row_of(&first, "w0").allocation.is_some() && row_of(&first, "w1").allocation.is_some(),
        "seed tiles both: {:?}",
        first.windows
    );
    let w0_seed_area = area(w0_tile_alloc.expect("w0 seed allocation"));

    // Single toggle: subject observed fullscreen over its full-screen
    // frame, no explicit rect, so the centered 60% fallback is the
    // durable placement.
    let mut fullscreen_w1 = fact("w1", "out-a", 100);
    fullscreen_w1.fullscreen = Some(true);
    fullscreen_w1.rect = Some(fullscreen_frame);
    let floated = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fullscreen_w1],
            "w0",
            vec![],
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "w1".to_owned(),
                float_rect: None,
            })),
        ),
    );
    match &floated.command_result {
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "w1", "float names the subject");
            assert_eq!(*rect, Some(fallback), "exact fallback placement");
            assert!(!no_space, "fallback placement, not no-space");
        }
        other => panic!("expected float result, got {other:?}"),
    }
    let subject = row_of(&floated, "w1");
    assert!(subject.floating, "subject floats: {subject:?}");
    assert_eq!(
        subject.allocation, None,
        "floated subject holds no tile allocation: {subject:?}"
    );
    assert_eq!(
        subject.effective_frame,
        Some(fallback),
        "durable fallback rides the row: {subject:?}"
    );
    assert_eq!(
        subject.frame_authority,
        ScopeAuthority::Untouched,
        "no float frame writes: {subject:?}"
    );
    assert_eq!(
        subject.frame_operation.as_deref(),
        Some("op-2"),
        "one-shot placement token: {subject:?}"
    );
    let sibling = row_of(&floated, "w0");
    let sibling_alloc = sibling.allocation.expect("sibling tile");
    assert_ne!(
        Some(sibling_alloc),
        w0_tile_alloc,
        "sibling leaves its own pre-toggle tile: {sibling:?}"
    );
    assert!(
        area(sibling_alloc) > w0_seed_area,
        "sibling expands into the freed tile: {sibling:?} vs seed {w0_tile_alloc:?}"
    );
    assert_eq!(
        (sibling_alloc.w, sibling_alloc.h),
        (800, 600),
        "sibling takes the full domain: {sibling:?}"
    );
    assert!(
        floated.effects.iter().all(|e| e.target.0 != "w1"),
        "no subject-targeted effects while fullscreen: {:?}",
        floated.effects
    );
    assert!(
        floated.transfers.is_empty(),
        "float mints no transfer: {:?}",
        floated.transfers
    );

    // Native truth echo: still fullscreen over the full-screen frame,
    // still floating. Observation-only must not flap the membership or
    // mint subject effects.
    let mut still_out = fact("w1", "out-a", 100);
    still_out.transient = Some(true);
    still_out.fullscreen = Some(true);
    still_out.rect = Some(fullscreen_frame);
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), still_out],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        row_of(&echo, "w1").floating,
        "echo stays floated: {:?}",
        echo.windows
    );
    assert_eq!(
        row_of(&echo, "w1").effective_frame,
        Some(fallback),
        "echo keeps the durable placement"
    );
    assert!(
        echo.effects.iter().all(|e| e.target.0 != "w1"),
        "echo mints no subject effects: {:?}",
        echo.effects
    );
    assert!(
        echo.transfers.is_empty(),
        "echo retains no flight: {:?}",
        echo.transfers
    );

    // Double toggle: floating + fullscreen observed, so the unfloat
    // fresh-admits to tiling with no subject geometry write. The Float
    // result carries no rect (tiled placement rides the snapshot) and the
    // durable placement survives (never the fullscreen frame).
    let mut float_full = fact("w1", "out-a", 100);
    float_full.transient = Some(true);
    float_full.fullscreen = Some(true);
    float_full.rect = Some(fullscreen_frame);
    let retiled = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![fact("w0", "out-a", 0), float_full],
            "w0",
            vec![],
            Some(4),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "w1".to_owned(),
                float_rect: None,
            })),
        ),
    );
    match &retiled.command_result {
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "w1", "unfloat names the subject");
            assert_eq!(*rect, None, "tile rides the snapshot");
            assert!(!no_space, "clean unfloat");
        }
        other => panic!("expected unfloat float result, got {other:?}"),
    }
    let subject = row_of(&retiled, "w1");
    assert!(!subject.floating, "subject re-tiles: {subject:?}");
    assert!(
        subject.allocation.is_some(),
        "subject holds a tile again: {subject:?}"
    );
    let reshrunk = row_of(&retiled, "w0").allocation.expect("sibling tile");
    assert!(
        area(reshrunk) < area(sibling_alloc),
        "sibling shrinks once the subject re-tiles: {reshrunk:?} vs {sibling_alloc:?}"
    );
    assert!(
        row_of(&retiled, "w0").allocation.is_some(),
        "sibling keeps a tile: {:?}",
        retiled.windows
    );
    assert!(
        retiled.effects.iter().all(|e| e.target.0 != "w1"),
        "fresh admission carries no subject effects: {:?}",
        retiled.effects
    );
    assert!(
        retiled.transfers.is_empty(),
        "unfloat mints no transfer: {:?}",
        retiled.transfers
    );

    // Actual native exit after the single toggle: fresh engine, toggle
    // under fullscreen, then the overlay clears with the float frame
    // observed and no command. The row stays floated on the exact
    // durable fallback with untouched frame authority.
    {
        let mut engine = engine_with_zero_outer_gap();
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let mut fullscreen_w1 = fact("w1", "out-a", 100);
        fullscreen_w1.fullscreen = Some(true);
        fullscreen_w1.rect = Some(fullscreen_frame);
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fullscreen_w1],
                "w0",
                vec![],
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
        );
        let mut exited = fact("w1", "out-a", 100);
        exited.transient = Some(true);
        exited.rect = Some(fallback);
        let reply = accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![fact("w0", "out-a", 0), exited],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let row = row_of(&reply, "w1");
        assert!(row.floating, "exit keeps the float: {row:?}");
        assert_eq!(row.allocation, None, "still no tile: {row:?}");
        assert_eq!(
            row.effective_frame,
            Some(fallback),
            "exit keeps the durable fallback: {row:?}"
        );
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Untouched,
            "exit writes no frame: {row:?}"
        );
        assert!(
            reply.effects.iter().all(|e| e.target.0 != "w1"),
            "exit mints no subject effects: {:?}",
            reply.effects
        );
        assert!(
            reply.transfers.is_empty(),
            "exit mints no transfer: {:?}",
            reply.transfers
        );
    }

    // Actual native exit after the double toggle: fresh engine, float
    // then unfloat under the retained overlay, then the overlay clears
    // with a plain tiled observation and no command. Both rows tile.
    {
        let mut engine = engine_with_zero_outer_gap();
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let mut fullscreen_w1 = fact("w1", "out-a", 100);
        fullscreen_w1.fullscreen = Some(true);
        fullscreen_w1.rect = Some(fullscreen_frame);
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fullscreen_w1],
                "w0",
                vec![],
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
        );
        let mut float_full = fact("w1", "out-a", 100);
        float_full.transient = Some(true);
        float_full.fullscreen = Some(true);
        float_full.rect = Some(fullscreen_frame);
        accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![fact("w0", "out-a", 0), float_full],
                "w0",
                vec![],
                Some(3),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
        );
        let reply = accepted(
            &mut engine,
            &report(
                4,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let row = row_of(&reply, "w1");
        assert!(!row.floating, "exit stays tiled: {row:?}");
        let alloc = row.allocation.expect("tiled after exit");
        assert_eq!(
            row.effective_frame,
            Some(alloc),
            "tiled frame rides the allocation: {row:?}"
        );
        assert!(
            reply.effects.iter().all(|e| e.target.0 != "w1"),
            "exit mints no subject effects: {:?}",
            reply.effects
        );
        assert!(
            reply.transfers.is_empty(),
            "exit mints no transfer: {:?}",
            reply.transfers
        );
    }

    // Durable previously floated placement beats the fallback: fresh
    // engine, an explicit float placement persists through an unfloat,
    // so a later fullscreen toggle reuses it instead of the fallback.
    {
        let durable = rect(10, 10, 100, 100);
        let mut engine = engine_with_zero_outer_gap();
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: Some(durable),
                })),
            ),
        );
        let mut floated = fact("w1", "out-a", 100);
        floated.transient = Some(true);
        floated.rect = Some(durable);
        accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![fact("w0", "out-a", 0), floated],
                "w0",
                vec![],
                Some(3),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
        );
        let mut fullscreen_w1 = fact("w1", "out-a", 100);
        fullscreen_w1.fullscreen = Some(true);
        fullscreen_w1.rect = Some(fullscreen_frame);
        let reply = accepted(
            &mut engine,
            &report(
                4,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fullscreen_w1],
                "w0",
                vec![],
                Some(4),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
        );
        match &reply.command_result {
            Some(CommandResult::Float { window, rect, .. }) => {
                assert_eq!(window.0, "w1");
                assert_eq!(*rect, Some(durable), "durable beats fallback");
            }
            other => panic!("expected durable float result, got {other:?}"),
        }
        assert_eq!(
            row_of(&reply, "w1").effective_frame,
            Some(durable),
            "row keeps the durable placement"
        );
    }

    // Maximize still refuses on the same command path, both directions.
    let mut maxed = fact("w1", "out-a", 100);
    maxed.maximized = Some(true);
    match engine
        .handle_report(
            &report(
                5,
                screens.clone(),
                vec![fact("w0", "out-a", 0), maxed],
                "w0",
                vec![],
                Some(5),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "maximized toggle must refuse"
        ),
        other => panic!("maximized toggle must refuse, got {other:?}"),
    }

    // Floating + maximized unfloat refuses too (post-clear observation
    // required): fresh engine, plain float, then the maximized overlay.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let seed = vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)];
    accepted(
        &mut engine,
        &report(1, screens.clone(), seed, "w0", vec![], None, None),
    );
    let floated = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w0",
            vec![],
            Some(2),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "w1".to_owned(),
                float_rect: None,
            })),
        ),
    );
    assert!(
        row_of(&floated, "w1").floating,
        "plain float stages: {:?}",
        floated.windows
    );
    let mut maxed_float = fact("w1", "out-a", 100);
    maxed_float.transient = Some(true);
    maxed_float.maximized = Some(true);
    match engine
        .handle_report(
            &report(
                3,
                screens.clone(),
                vec![fact("w0", "out-a", 0), maxed_float],
                "w0",
                vec![],
                Some(3),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: None,
                })),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            "maximized unfloat must refuse"
        ),
        other => panic!("maximized unfloat must refuse, got {other:?}"),
    }
}

#[test]
fn toggle_maximize_command_decides_and_reports() {
    // World-owned maximize toggle (KDE route): the focused subject's
    // fullscreen/maximized preimage decides the correlated result while
    // the snapshot IS the outcome (no retain, no topology mutation).
    fn toggle(window: &str) -> WorldCommand {
        WorldCommand::ToggleMaximize {
            window: WindowId(window.to_owned()),
        }
    }
    fn maximize_of(reply: &crate::world_report::WorldReplyV4) -> (bool, bool, bool) {
        match &reply.command_result {
            Some(CommandResult::Maximize {
                window,
                maximized,
                clear_fullscreen,
                was_maximized,
            }) => {
                assert_eq!(window.0, "w1", "result names the subject");
                (*maximized, *clear_fullscreen, *was_maximized)
            }
            other => panic!("expected maximize result, got {other:?}"),
        }
    }
    let screens = vec![roomy("out-a", 0)];
    // Native paired-clear legs need fullscreen authority alongside the
    // native maximize facts the fixtures carry.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            fullscreen: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let seed = vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)];
    let first = accepted(
        &mut engine,
        &report(1, screens.clone(), seed, "w1", vec![], None, None),
    );
    let w0_alloc = row_of(&first, "w0").allocation.expect("w0 seed slot");
    let w1_alloc = row_of(&first, "w1").allocation.expect("w1 seed slot");

    // Focused tiled fullscreen maximizes with a fullscreen clear; the
    // exact subject/sibling allocations survive, nothing retains.
    let mut fs = fact("w1", "out-a", 100);
    fs.fullscreen = Some(true);
    // The native paired clear needs proven exitability.
    fs.fullscreen_exitable = Some(true);
    let toggled = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fs],
            "w1",
            vec![],
            Some(2),
            Some(toggle("w1")),
        ),
    );
    assert_eq!(
        maximize_of(&toggled),
        (true, true, false),
        "fullscreen preimage maximizes with clear"
    );
    assert_eq!(
        row_of(&toggled, "w1").allocation,
        Some(w1_alloc),
        "subject slot survives"
    );
    assert_eq!(
        row_of(&toggled, "w0").allocation,
        Some(w0_alloc),
        "sibling slot survives"
    );
    assert!(toggled.transfers.is_empty(), "no transfers");
    assert!(
        toggled.effects.iter().all(|e| e.target.0 != "w1"),
        "no subject effects: {:?}",
        toggled.effects
    );
    // Exact result bytes: the structural estimate matches the existing
    // serializer byte-for-byte (locks the fixed count, no guesses; the
    // 10 bytes are the shared `,"result":` section prefix).
    let mut encoded = String::new();
    crate::world_sizing::encode_command_result(&mut encoded, &toggled.command_result);
    assert_eq!(
        crate::world_sizing::command_result_estimate(&toggled.command_result),
        encoded.len() + 10,
        "result estimate is exact: {encoded}"
    );

    // Maximized observation toggles back off with no clear; the base
    // tile (seed slot) is the outcome.
    let mut mx = fact("w1", "out-a", 100);
    mx.maximized = Some(true);
    let untoggled = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), mx],
            "w1",
            vec![],
            Some(3),
            Some(toggle("w1")),
        ),
    );
    assert_eq!(
        maximize_of(&untoggled),
        (false, false, true),
        "maximized preimage unmaximizes without clear"
    );
    assert_eq!(
        row_of(&untoggled, "w1").allocation,
        Some(w1_alloc),
        "second toggle keeps the base tile"
    );
    assert_eq!(
        row_of(&untoggled, "w0").allocation,
        Some(w0_alloc),
        "sibling still exact"
    );
    // Both-flags observation maximizes with a clear (fullscreen wins).
    let mut both = fact("w1", "out-a", 100);
    both.fullscreen = Some(true);
    both.fullscreen_exitable = Some(true);
    both.maximized = Some(true);
    let both_reply = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![fact("w0", "out-a", 0), both],
            "w1",
            vec![],
            Some(4),
            Some(toggle("w1")),
        ),
    );
    assert_eq!(
        maximize_of(&both_reply),
        (true, true, true),
        "both flags maximize with clear"
    );
    // Ordinary plain tile maximizes with no clear.
    let plain = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w1",
            vec![],
            Some(5),
            Some(toggle("w1")),
        ),
    );
    assert_eq!(
        maximize_of(&plain),
        (true, false, false),
        "plain tile maximizes without clear"
    );
    assert_eq!(
        row_of(&plain, "w1").allocation,
        Some(w1_alloc),
        "plain toggle keeps the tile"
    );
    // Exact command bytes: the estimate delta over a commandless twin
    // equals the wire delta (locks the fixed count, no guesses).
    let wire = r#"{"op":"toggle-maximize","win":"w1"}"#;
    let with_cmd = report(
        6,
        screens.clone(),
        vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
        "w1",
        vec![],
        Some(6),
        Some(toggle("w1")),
    );
    let mut bare = with_cmd.clone();
    bare.command = None;
    assert_eq!(
        crate::world_sizing_v4::report_estimate_v4(&with_cmd)
            - crate::world_sizing_v4::report_estimate_v4(&bare),
        wire.len() - 4,
        "command estimate is exact"
    );
    // Commandless reports carry no result.
    let quiet = accepted(
        &mut engine,
        &report(
            6,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "w1",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        quiet.command_result.is_none(),
        "commandless carries nothing"
    );
    // A newer report reusing a consumed sequence reconciles only: the
    // stale command never re-emits its result.
    let replay = match engine
        .handle_report(
            &report(
                7,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w1",
                vec![],
                Some(2),
                Some(toggle("w1")),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("consumed sequence replays quiet, got {other:?}"),
    };
    assert!(
        replay.command_result.is_none(),
        "consumed sequence stays quiet"
    );

    // Floating fullscreen toggles to maximized with a clear while the
    // float keeps its durable placement and untouched frames.
    {
        let durable = rect(10, 10, 100, 100);
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities {
                native_sticky: true,
                maximize: true,
                fullscreen: true,
                ..Default::default()
            },
        )
        .expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w1",
                vec![],
                None,
                None,
            ),
        );
        accepted(
            &mut engine,
            &report(
                2,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w1",
                vec![],
                Some(2),
                Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                    window: "w1".to_owned(),
                    float_rect: Some(durable),
                })),
            ),
        );
        let mut floated_fs = fact("w1", "out-a", 100);
        floated_fs.transient = Some(true);
        floated_fs.fullscreen = Some(true);
        floated_fs.fullscreen_exitable = Some(true);
        floated_fs.rect = Some(rect(0, 0, 800, 600));
        let floated = accepted(
            &mut engine,
            &report(
                3,
                screens.clone(),
                vec![fact("w0", "out-a", 0), floated_fs],
                "w1",
                vec![],
                Some(3),
                Some(toggle("w1")),
            ),
        );
        match &floated.command_result {
            Some(CommandResult::Maximize {
                maximized,
                clear_fullscreen,
                was_maximized,
                ..
            }) => assert_eq!(
                (*maximized, *clear_fullscreen, *was_maximized),
                (true, true, false),
                "floating fullscreen maximizes with clear"
            ),
            other => panic!("expected maximize result, got {other:?}"),
        }
        let row = row_of(&floated, "w1");
        assert!(row.floating, "toggle keeps the float: {row:?}");
        assert_eq!(row.allocation, None, "still no tile: {row:?}");
        assert_eq!(
            row.effective_frame,
            Some(durable),
            "durable placement survives: {row:?}"
        );
        assert!(floated.transfers.is_empty(), "no transfers");
    }

    // Refusal: a command naming an unfocused window never executes.
    match engine
        .handle_report(
            &report(
                8,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                Some(8),
                Some(toggle("w1")),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown
            }),
            "unfocused toggle refuses"
        ),
        other => panic!("unfocused toggle refuses, got {other:?}"),
    }

    // Refusal: a geometrically held scope never executes.
    {
        let mut held_engine = WorldEngine::new_test_bound_with_initial_settings(
            "gen-1",
            "adapter-1",
            crate::connection::HostCapabilities {
                // Fixtures carry native sticky and maximize facts; the
                // lane under test is the native route.
                native_sticky: true,
                maximize: true,
                ..Default::default()
            },
            vec![(
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(50.0),
            )],
        )
        .expect("generation");
        let abutting = vec![
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(700, 0, 100, 100)),
                work_area: Some(rect(700, 0, 100, 100)),
                complete: true,
                scale: 1.0,
                identity: crate::world_report::OutputIdentity::unknown(),
            },
            roomy("out-b", 800),
        ];
        accepted(
            &mut held_engine,
            &report(
                1,
                abutting,
                vec![fact("w0", "out-a", 710), fact("wb", "out-b", 800)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let clamped = vec![
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(700, 0, 100, 100)),
                work_area: Some(rect(700, 0, 100, 100)),
                complete: true,
                scale: 1.0,
                identity: crate::world_report::OutputIdentity::unknown(),
            },
            roomy("out-b", 800),
        ];
        match held_engine
            .handle_report(
                &report(
                    2,
                    clamped,
                    vec![fact("w0", "out-a", 710), fact("wb", "out-b", 800)],
                    "w0",
                    vec![],
                    Some(11),
                    Some(toggle("w0")),
                ),
                &[],
            )
            .0
        {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::ScopeHeld
                }),
                "held toggle refuses"
            ),
            other => panic!("held toggle refuses, got {other:?}"),
        }
    }

    // Refusal: a retained window transfer fences a same-window toggle.
    // (Explicit sends are same-output only; directional sends own the
    // crossing, so the flight here rides an ordinary same-output send.)
    {
        let mut flight_engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
        let seed = accepted(
            &mut flight_engine,
            &report(
                1,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                None,
                None,
            ),
        );
        let ring: Vec<String> = seed
            .workspaces
            .iter()
            .filter(|r| r.output.0 == "out-a")
            .map(|r| r.id.clone())
            .collect();
        let sent = accepted(
            &mut flight_engine,
            &report(
                2,
                screens.clone(),
                vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                "w0",
                vec![],
                Some(2),
                Some(WorldCommand::Send {
                    window: WindowId("w0".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: ring[1].clone(),
                    follow: false,
                }),
            ),
        );
        assert!(
            sent.transfers
                .iter()
                .any(|t| matches!(&t.kind, TransferKind::Window(_))),
            "send retains a window transfer"
        );
        match flight_engine
            .handle_report(
                &report(
                    3,
                    screens.clone(),
                    vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
                    "w0",
                    vec![],
                    Some(3),
                    Some(toggle("w0")),
                ),
                &[],
            )
            .0
        {
            WorldOutcome::Accepted(reply) => assert_eq!(
                reply.command_result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::TransferInFlight
                }),
                "in-flight toggle refuses"
            ),
            other => panic!("in-flight toggle refuses, got {other:?}"),
        }
    }
}

fn engine_with_gaps(inner: f64, outer: f64) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![
            (
                "gap".to_owned(),
                crate::connection::RawSettingValue::Number(inner),
            ),
            (
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(outer),
            ),
        ],
    )
    .expect("generation")
}

fn roomy_scaled(output: &str, x: i32, scale: f64) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 600)),
        complete: true,
        scale,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

#[test]
fn scale_once_ties_bounds_and_overflow() {
    // floor(logical * scale + 0.5), checked: ties round up, overflow is
    // None (never a 64-bit clamp).
    assert_eq!(WorldEngine::scale_once(8, 1.0), Some(8));
    assert_eq!(WorldEngine::scale_once(8, 1.5), Some(12));
    assert_eq!(WorldEngine::scale_once(1, 1.0), Some(1));
    assert_eq!(WorldEngine::scale_once(1, 1.5), Some(2));
    assert_eq!(WorldEngine::scale_once(0, 1.5), Some(0));
    assert_eq!(WorldEngine::scale_once(8, 1.25), Some(10));
    assert_eq!(WorldEngine::scale_once(i32::MAX, 2.0), None);
    assert_eq!(WorldEngine::scale_once(8, 1e308), None);
}

#[test]
fn per_output_scale_derives_physical_gaps() {
    // Logical 8/8 with scale 1 stages physical 8/8; scale 1.5 stages
    // 12/12 on that output only. Real engine, through the staged domains.
    for (scale, physical) in [(1.0, 8), (1.5, 12)] {
        let mut engine = engine_with_gaps(8.0, 8.0);
        accepted(
            &mut engine,
            &report(
                1,
                vec![roomy_scaled("out-a", 0, scale)],
                vec![fact("w0", "out-a", 0)],
                "",
                vec![],
                None,
                None,
            ),
        );
        let domain = engine
            .staged
            .domains
            .iter()
            .find(|domain| domain.key.output.0 == "out-a")
            .expect("staged domain");
        assert_eq!(domain.gap, physical, "scale {scale}");
        assert_eq!(domain.outer_gap, physical, "scale {scale}");
        assert!(domain.readable, "scale {scale}");
        assert_eq!(domain.hold, None, "scale {scale}");
    }
}

#[test]
fn scale_overflow_holds_scope_unreadable_with_diagnostic() {
    // An overflowing scale holds the scope as unreadable with one staged
    // Unreadable screen diagnostic, never a clamped gap. The reply is
    // still accepted; the scope authorizes nothing.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let reply = accepted(
        &mut engine,
        &report(
            1,
            vec![roomy_scaled("out-a", 0, 1e308)],
            vec![fact("w0", "out-a", 0)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let domain = engine
        .staged
        .domains
        .iter()
        .find(|domain| domain.key.output.0 == "out-a")
        .expect("staged domain");
    assert!(!domain.readable);
    assert_eq!(
        domain.hold,
        Some(crate::world_report::HoldReason::Unreadable)
    );
    assert_eq!(reply.diagnostics.len(), 1);
    let diagnostic = &reply.diagnostics[0];
    assert_eq!(
        diagnostic.kind,
        crate::world_report::RowDiagnosticKind::Unreadable
    );
    assert_eq!(
        diagnostic.row,
        crate::world_report::RowDiagnosticLane::Screen
    );
    assert_eq!(diagnostic.id.as_deref(), Some("out-a"));
    // The hold rides the scope, not the output row: uniform per-output
    // authorities collapse into one Output-level scope (publish folds
    // them), the screen is complete and unreadable domains never enter
    // held_scopes, so plan_outputs leaves the row unheld.
    let scope = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-a"
            )
        })
        .expect("output scope");
    assert_eq!(
        scope.authority,
        crate::world_report::ScopeAuthority::Hold(crate::world_report::HoldReason::Unreadable)
    );
    let output = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("output row");
    assert_eq!(output.hold, None);
}

fn held_unreadable_row(output: &str, ordinal: usize) -> crate::world_report::HeldScreenRow {
    crate::world_report::HeldScreenRow {
        output: OutputId(output.to_owned()),
        ordinal,
        fields: vec![
            (
                "out".to_owned(),
                crate::connection::RawSettingValue::String(output.to_owned()),
            ),
            (
                "scale".to_owned(),
                crate::connection::RawSettingValue::String("bad".to_owned()),
            ),
        ],
        diagnostic: crate::world_report::RowDiagnostic {
            scope: crate::world_report::AuthorityScope::Output(OutputId(output.to_owned())),
            row: crate::world_report::RowDiagnosticLane::Screen,
            id: Some(output.to_owned()),
            kind: crate::world_report::RowDiagnosticKind::Unreadable,
        },
    }
}

#[test]
fn bootstrap_first_bad_identifiable_output_has_no_authority() {
    // Before readiness latches, an identifiable bad screen row authorizes
    // nothing: the report bootstraps with an empty reply.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let mut bad_first = report(1, Vec::new(), Vec::new(), "", vec![], None, None);
    bad_first.held_screens = vec![held_unreadable_row("out-b", 0)];
    let reply = match engine.handle_report(&bad_first, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected bootstrap, got {other:?}"),
    };
    assert!(!reply.ready);
    assert!(reply.scopes.is_empty());
    assert!(reply.diagnostics.is_empty());
    assert!(reply.workspaces.is_empty());
    assert!(reply.windows.is_empty());
    assert!(!engine.is_ready());
    assert_eq!(engine.state_revision(), 0);
}

#[test]
fn after_ready_bad_screen_holds_only_affected_output() {
    // Once ready latches, a bad screen holds only its own output scope
    // with its typed diagnostic while the healthy output stays controlled.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let screens = vec![roomy("out-a", 0), roomy("out-b", 800)];
    let _ = accepted(
        &mut engine,
        &report(
            1,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(engine.is_ready());
    let mut narrow = report(
        2,
        vec![roomy("out-a", 0)],
        vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
        "",
        vec![],
        None,
        None,
    );
    narrow.held_screens = vec![held_unreadable_row("out-b", 1)];
    let reply = match engine.handle_report(&narrow, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted narrow hold, got {other:?}"),
    };
    assert!(reply.ready);
    assert_eq!(reply.diagnostics.len(), 1);
    let diagnostic = &reply.diagnostics[0];
    assert_eq!(
        diagnostic.kind,
        crate::world_report::RowDiagnosticKind::Unreadable
    );
    assert_eq!(
        diagnostic.row,
        crate::world_report::RowDiagnosticLane::Screen
    );
    assert_eq!(diagnostic.id.as_deref(), Some("out-b"));
    let healthy = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-a"
            )
        })
        .expect("healthy output scope");
    assert_eq!(healthy.authority, ScopeAuthority::Controlled);
    let held = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-b"
            )
        })
        .expect("held output scope");
    assert_eq!(
        held.authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::Unreadable)
    );
    let good = row_of(&reply, "w0");
    assert_eq!(good.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(good.membership_authority, ScopeAuthority::Controlled);
    let bad = row_of(&reply, "w1");
    assert!(
        matches!(bad.frame_authority, ScopeAuthority::Hold(_)),
        "held window holds frame: {bad:?}"
    );
    assert!(
        matches!(bad.membership_authority, ScopeAuthority::Hold(_)),
        "held window holds membership: {bad:?}"
    );
}

#[test]
fn direct_invalid_scale_first_report_grades_to_hold_without_authority() {
    // Causal-equivalence target (finding B): a directly constructed
    // identifiable invalid-scale row must grade to a narrow hold like its
    // protocol-decoded twin, not refuse the report. Before readiness
    // latches there is no authority: bootstrap reply with empty scopes and
    // diagnostics. The pure shape gate still names the violation.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let bad = report(
        1,
        vec![roomy_scaled("out-b", 0, 0.0)],
        Vec::new(),
        "",
        vec![],
        None,
        None,
    );
    assert_eq!(validate_report_reason(&bad), Some("screen-scale-invalid"));
    let reply = match engine.handle_report(&bad, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected bootstrap narrow hold, got {other:?}"),
    };
    assert!(!reply.ready);
    assert!(reply.scopes.is_empty());
    assert!(reply.diagnostics.is_empty());
    assert!(reply.workspaces.is_empty());
    assert!(reply.windows.is_empty());
    assert!(!engine.is_ready());
}

#[test]
fn direct_invalid_scale_after_ready_holds_only_affected_output() {
    // Post-ready causal equivalence: the healthy output stays controlled
    // while the directly constructed bad-scale output holds with its typed
    // InvalidFact diagnostic, exactly like the decoded narrow hold.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let _ = accepted(
        &mut engine,
        &report(
            1,
            vec![roomy("out-a", 0), roomy("out-b", 800)],
            vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
            "",
            vec![],
            None,
            None,
        ),
    );
    assert!(engine.is_ready());
    let narrow = report(
        2,
        vec![roomy("out-a", 0), roomy_scaled("out-b", 800, 0.0)],
        vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
        "",
        vec![],
        None,
        None,
    );
    assert_eq!(
        validate_report_reason(&narrow),
        Some("screen-scale-invalid")
    );
    let reply = match engine.handle_report(&narrow, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted narrow hold, got {other:?}"),
    };
    assert!(reply.ready);
    assert_eq!(reply.diagnostics.len(), 1);
    let diagnostic = &reply.diagnostics[0];
    assert_eq!(diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(diagnostic.row, RowDiagnosticLane::Screen);
    assert_eq!(diagnostic.id.as_deref(), Some("out-b"));
    let healthy = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-a"
            )
        })
        .expect("healthy output scope");
    assert_eq!(healthy.authority, ScopeAuthority::Controlled);
    let held = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-b"
            )
        })
        .expect("held output scope");
    assert_eq!(held.authority, ScopeAuthority::Hold(HoldReason::Unreadable));
    let good = row_of(&reply, "w0");
    assert_eq!(good.frame_authority, ScopeAuthority::Controlled);
    let bad_row = row_of(&reply, "w1");
    assert!(
        matches!(bad_row.frame_authority, ScopeAuthority::Hold(_)),
        "held window holds frame: {bad_row:?}"
    );
}

#[test]
fn direct_nonfinite_scale_grades_to_hold_without_null_encoding() {
    // A non-finite typed scale is retained internally as a held number
    // fact but no encoder converts it to `null`: the checked writers err.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let _ = accepted(
        &mut engine,
        &report(
            1,
            vec![roomy("out-a", 0), roomy("out-b", 800)],
            vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let narrow = report(
        2,
        vec![roomy("out-a", 0), roomy_scaled("out-b", 800, f64::NAN)],
        vec![fact("w0", "out-a", 0), fact("w1", "out-b", 800)],
        "",
        vec![],
        None,
        None,
    );
    assert_eq!(
        validate_report_reason(&narrow),
        Some("screen-scale-invalid")
    );
    let graded = grade_invalid_screens(&narrow).expect("grades");
    assert_eq!(graded.held_screens.len(), 1);
    assert!(
        crate::world_report::encode_held_screen_canonical(&graded.held_screens[0]).is_err(),
        "retained non-finite fact never spells null"
    );
    let reply = match engine.handle_report(&narrow, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted narrow hold, got {other:?}"),
    };
    assert!(reply.ready);
    assert_eq!(reply.diagnostics.len(), 1);
    assert_eq!(reply.diagnostics[0].kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(reply.diagnostics[0].id.as_deref(), Some("out-b"));
}

#[test]
fn direct_invalid_scale_with_preheld_row_keeps_ordinals() {
    // A newly graded row takes the free carried-array slot while the
    // pre-existing held row keeps its original ordinal; both scopes hold
    // with their own diagnostics and the healthy output stays controlled.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let _ = accepted(
        &mut engine,
        &report(
            1,
            vec![roomy("out-a", 0), roomy("out-b", 800), roomy("out-c", 1600)],
            vec![fact("w0", "out-a", 0)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let mut mixed = report(
        2,
        vec![roomy("out-a", 0), roomy_scaled("out-b", 800, 0.0)],
        vec![fact("w0", "out-a", 0)],
        "",
        vec![],
        None,
        None,
    );
    mixed.held_screens = vec![held_unreadable_row("out-c", 1)];
    let graded = grade_invalid_screens(&mixed).expect("grades");
    let ordinals: Vec<usize> = graded
        .held_screens
        .iter()
        .map(|held| held.ordinal)
        .collect();
    assert_eq!(ordinals, vec![1, 2]);
    let reply = match engine.handle_report(&mixed, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted mixed hold, got {other:?}"),
    };
    assert!(reply.ready);
    assert_eq!(reply.diagnostics.len(), 2);
    let healthy = reply
        .scopes
        .iter()
        .find(|status| {
            matches!(
                &status.scope,
                crate::world_report::AuthorityScope::Output(output)
                    if output.0 == "out-a"
            )
        })
        .expect("healthy output scope");
    assert_eq!(healthy.authority, ScopeAuthority::Controlled);
    for held in ["out-b", "out-c"] {
        reply
            .scopes
            .iter()
            .find(|status| {
                matches!(
                    &status.scope,
                    crate::world_report::AuthorityScope::Output(output)
                        if output.0 == held
                )
            })
            .unwrap_or_else(|| panic!("held output scope {held}"));
    }
}

#[test]
fn direct_invalid_scale_with_structural_violation_still_refuses() {
    // Grading never masks structural gates: a bad id, a duplicate output,
    // or a bad rect beside a gradable scale refuses the whole report.
    let mut engine = engine_with_gaps(8.0, 8.0);
    let mut bad_id = report(
        1,
        vec![roomy_scaled("out-b", 0, 0.0)],
        Vec::new(),
        "",
        vec![],
        None,
        None,
    );
    bad_id.screens[0].output = OutputId(String::new());
    match engine.handle_report(&bad_id, &[]).0 {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "snapshot-invalid");
            assert_eq!(message, "screen-id-invalid");
        }
        other => panic!("bad id must refuse, got {other:?}"),
    }
    let duplicate = report(
        1,
        vec![roomy("out-b", 0), roomy_scaled("out-b", 800, 0.0)],
        Vec::new(),
        "",
        vec![],
        None,
        None,
    );
    match engine.handle_report(&duplicate, &[]).0 {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "snapshot-invalid");
            assert_eq!(message, "duplicate-screen");
        }
        other => panic!("duplicate must refuse, got {other:?}"),
    }
    assert!(!engine.is_ready());
}

#[test]
fn scale_once_rejects_nonfinite_without_clamp() {
    assert_eq!(WorldEngine::scale_once(8, f64::NAN), None);
    assert_eq!(WorldEngine::scale_once(8, f64::INFINITY), None);
    assert_eq!(WorldEngine::scale_once(8, f64::NEG_INFINITY), None);
}

#[test]
fn explicit_tile_override_via_real_toggle_float_survives_lifetime() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let screens = vec![roomy("out-a", 0)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100)],
            "",
            vec![],
            None,
            None,
        ),
    );
    let arrival = accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(arrival.effects.is_empty());
    assert!(row_of(&arrival, "wf").floating);
    let mut wf_float = float_fact("wf", "out-a", 200);
    wf_float.hints = Some(fixed_hints());
    let untiled = accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), wf_float],
            "w0",
            vec![],
            Some(10),
            Some(WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: "wf".to_owned(),
                float_rect: None,
            })),
        ),
    );
    match &untiled.command_result {
        Some(CommandResult::Float {
            window,
            rect,
            no_space,
        }) => {
            assert_eq!(window.0, "wf");
            assert_eq!(*rect, None);
            assert!(!*no_space);
        }
        other => panic!("expected unfloat tile result, got {other:?}"),
    }
    assert!(
        untiled.effects.is_empty(),
        "unfloat mints no admission focus: {:?}",
        untiled.effects
    );
    assert!(untiled.transfers.is_empty());
    assert!(!row_of(&untiled, "wf").floating);
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned())),
        "live command records override"
    );
    let echo = accepted(
        &mut engine,
        &report(
            4,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "wf",
            vec![],
            None,
            None,
        ),
    );
    assert!(echo.effects.is_empty());
    assert!(!row_of(&echo, "wf").floating);
    let mut hidden_wf = fixed_fact("wf", "out-a", 200);
    hidden_wf.minimized = Some(true);
    let _hidden = accepted(
        &mut engine,
        &report(
            5,
            screens.clone(),
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), hidden_wf],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned())),
        "override survives hide"
    );
    let shown = accepted(
        &mut engine,
        &report(
            6,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            None,
            None,
        ),
    );
    assert!(!row_of(&shown, "wf").floating, "override survives show");
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );
    let view_a = view_of(&shown, "out-a");
    let disabled = accepted(
        &mut engine,
        &report(
            7,
            screens.clone(),
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    assert!(disabled.effects.is_empty());
    let enabled = accepted(
        &mut engine,
        &report(
            8,
            screens,
            vec![
                fact("w0", "out-a", 0),
                fact("w1", "out-a", 100),
                fixed_fact("wf", "out-a", 200),
            ],
            "w0",
            vec![],
            Some(12),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a,
                tiled: true,
            }),
        ),
    );
    assert!(
        !row_of(&enabled, "wf").floating,
        "override survives re-adoption"
    );
    assert!(
        engine
            .engine
            .has_fixed_tile_override_any(&WindowId("wf".to_owned()))
    );
    assert!(
        enabled.effects.is_empty(),
        "re-adoption stays quiet: {:?}",
        enabled.effects
    );
}
