//! Backfill for deleted native-world coverage (stage 2a): admission
//! focus-once discriminants, send stay/follow focus across both directions,
//! migration fences, and float-boundary regressions. Logical-first: screens
//! plus window facts only; native-identity/aligned-grid binding is
//! superseded (logical ids authorize homing), everything else below is live
//! v2 behavior through `WorldEngine::take_admission_focus`,
//! `settle_send`, and `start_migration`.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandResult, FrameSetter, MemberAttempt, ScopeAuthority, ScreenReport, TransferKind,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_sized(output: &str, x: i32, w: i32, h: i32, primary: bool, gap: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, w, h)),
        work_area: Some(rect(x, 0, w, h)),
        gap,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn roomy(output: &str, x: i32, primary: bool) -> ScreenReport {
    screen_sized(output, x, 800, 600, primary, 0)
}

fn tiny(output: &str, x: i32, primary: bool) -> ScreenReport {
    screen_sized(output, x, 10, 10, primary, 8)
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect(x, 0, 40, 90)),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
        native_workspace: None,
    }
}

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        floating: true,
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
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: Vec::new(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn view_of(reply: &crate::world_report::WorldReply, output: &str) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

fn two_roomy() -> Vec<ScreenReport> {
    vec![roomy("out-a", 0, true), roomy("out-b", 800, false)]
}

#[test]
fn burst_batch_pins_last_eligible_newcomer_once() {
    // Two newcomers in one report carry exactly one focus intent, pinned
    // to the last eligible newcomer, never one per newcomer, never replayed.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
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
    assert_eq!(
        arrival.effects.len(),
        1,
        "one burst intent: {:?}",
        arrival.effects
    );
    assert_eq!(arrival.effects[0].target.0, "w3", "last newcomer");
    assert_eq!(arrival.effects[0].id, "af-w3");
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![tiny("out-a", 0, true)];
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
                w.floating = true;
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
    // A game/overlay newcomer (fit-excluded with no floating, fullscreen,
    // or maximized origin) converges honestly but never takes automatic
    // activation; a later clean report still mints nothing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
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
    game.fit_excluded = true;
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
        row_of(&reply, "game").allocation.is_some(),
        "still converges honestly"
    );
    let echo = accepted(
        &mut engine,
        &report(
            3,
            screens,
            vec![fact("w0", "out-a", 0), fact("w1", "out-a", 100), {
                let mut game = fact("game", "out-a", 200);
                game.fit_excluded = true;
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
}

#[test]
fn normalized_ordinary_float_newcomer_focuses_once() {
    // A newcomer arriving as an ordinary float carries the same one-shot
    // newcomer intent as a capacity float; only the unexplained shape is
    // unsafe. Re-observation mints nothing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
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
    assert_eq!(
        arrival.effects.len(),
        1,
        "one newcomer intent: {:?}",
        arrival.effects
    );
    assert_eq!(arrival.effects[0].target.0, "w2");
    assert_eq!(arrival.effects[0].id, "af-w2");
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

#[test]
fn maximized_sibling_suppresses_admission_without_retry() {
    // D01 native activation ownership covers maximized siblings exactly
    // like fullscreen ones: the suppressed intent spends, never retries.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
    let mut max = fact("max", "out-a", 0);
    max.maximized = true;
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
                    max.maximized = true;
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
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
    let mut restarted = WorldEngine::new("gen-1").expect("generation");
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let screens = vec![roomy("out-a", 0, true)];
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
            .find(|s| s.scope.starts_with("out-a/"))
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    assert_eq!(
        stay.effects.len(),
        1,
        "stay names exactly one focus: {:?}",
        stay.effects
    );
    assert_eq!(stay.effects[0].target.0, "w2", "source MRU survivor");
    assert!(stay.effects[0].is_explicit_focus_for(11));
    // Exact arrival settles; the echo replays nothing.
    let view_b = view_of(&stay, "out-b");
    let mut arrived = fact("w0", "out-b", 800);
    arrived.observed_workspace = Some(view_b.clone());
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
    assert_eq!(
        back.effects.len(),
        1,
        "follow names exactly one focus: {:?}",
        back.effects
    );
    assert_eq!(back.effects[0].target.0, "w0", "exact mover");
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    assert_eq!(
        stay_left.effects.len(),
        1,
        "stay names exactly one focus: {:?}",
        stay_left.effects
    );
    assert_eq!(stay_left.effects[0].target.0, "wb2", "out-b MRU survivor");
    let view_a = view_of(&stay_left, "out-a");
    let mut arrived_left = fact("wb", "out-a", 0);
    arrived_left.observed_workspace = Some(view_a.clone());
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
    assert_eq!(
        ahead.effects.len(),
        1,
        "follow names exactly one focus: {:?}",
        ahead.effects
    );
    assert_eq!(ahead.effects[0].target.0, "wb", "exact mover");
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
            bad.sticky = true;
        } else {
            bad.fit_excluded = true;
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
        match engine.handle_report(&send) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "send-refused", "{tag}"),
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
            roomy("out-a", 0, true),
            ScreenReport {
                output: OutputId("out-b".to_owned()),
                full_rect: None,
                work_area: None,
                gap: 0,
                outer_gap: 0,
                complete: false,
                primary: false,
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
        match engine.handle_report(&send) {
            WorldOutcome::Rejected { kind, .. } => {
                assert_eq!(kind, "send-target-unknown", "blind target")
            }
            other => panic!("expected send-target-unknown, got {other:?}"),
        }
        assert_eq!(engine.state_revision(), revision_before, "no mutation");
        assert!(engine.retained_transfers().is_empty(), "retains nothing");
    }
    // Held source (inset exhaustion via an outer gap exhausting the work
    // area): the sweep holds the scope, and the send shares the held fence.
    // The clamped source still touches the neighbor so topology resolves
    // and the refusal comes from the held scope, not the target.
    {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let abutting = vec![
            ScreenReport {
                output: OutputId("out-a".to_owned()),
                full_rect: Some(rect(700, 0, 100, 100)),
                work_area: Some(rect(700, 0, 100, 100)),
                gap: 0,
                outer_gap: 0,
                complete: true,
                primary: true,
            },
            roomy("out-b", 800, false),
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
                gap: 0,
                outer_gap: 64,
                complete: true,
                primary: true,
            },
            roomy("out-b", 800, false),
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
        match engine.handle_report(&send) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "scope-held", "clamped source"),
            other => panic!("expected scope-held, got {other:?}"),
        }
        assert_eq!(engine.state_revision(), revision_before, "no mutation");
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    match engine.handle_report(&retry_explicit) {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, "migration-refused", "explicit retry")
        }
        other => panic!("explicit retry: expected migration-refused, got {other:?}"),
    }
    assert_eq!(
        engine.state_revision(),
        revision_before + 1,
        "explicit refusal mutates nothing"
    );
    // The direction-only retry resolves the current source view with the
    // same staged context and retains its own flight.
    let mut retry_directional = report(
        3,
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
    let applied = match engine.handle_report(&retry_directional) {
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
        revision_before + 2,
        "directional retry commits its own flight"
    );
    // Both migration flights survive with stable identity: the original
    // plus the direction-only retry's own commit.
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
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-13"),
        "directional flight stable: {:?}",
        engine.retained_transfers()
    );
}

#[test]
fn floating_source_migration_carries_without_engine_tree() {
    // Accepted behavior: a floating workspace holds no retained session,
    // so migration carries by sessionless core-owned membership remap (no
    // fabricated tree or focus). Members, tiling mode, source refill, and
    // target view commit; arrival with both views settles; proven refusal
    // rolls back to observed.
    let screens = two_roomy();
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    let refill = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("core refill view");
    assert_eq!(refill.operation_id, "op-12-refill");
    assert_eq!(refill.target_output.0, "out-a");
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
    // Arrival with both views actuated settles the sessionless carry.
    let refill_ws = refill.target_workspace.clone();
    let mut arrived = fact("w0", "out-b", 800);
    arrived.observed_workspace = Some(view_a.clone());
    let mut arrived_float = float_fact("wf", "out-b", 900);
    arrived_float.observed_workspace = Some(view_a.clone());
    let mut arrival = report(
        5,
        screens.clone(),
        vec![arrived, arrived_float],
        "w0",
        vec![],
        None,
        None,
    );
    arrival.views = vec![
        crate::world_report::ScreenView {
            output: OutputId("out-a".to_owned()),
            view: refill_ws,
        },
        crate::world_report::ScreenView {
            output: OutputId("out-b".to_owned()),
            view: view_a.clone(),
        },
    ];
    let settled = accepted(&mut engine, &arrival);
    assert!(
        settled.transfers.is_empty(),
        "both views settle: {:?}",
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    assert_eq!(engine.retained_transfers().len(), 2, "flight retained");
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
            pre: Some((OutputId("out-a".to_owned()), view_a.clone())),
            post: Some((OutputId("out-a".to_owned()), view_a.clone())),
            native_pre: None,
            native_post: None,
            attempt_ms: 4,
        },
        MemberAttempt {
            operation_id: "op-12".to_owned(),
            window: WindowId("wf".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            target_workspace: view_a.clone(),
            setter: FrameSetter::Refused,
            pre: Some((OutputId("out-a".to_owned()), view_a.clone())),
            post: Some((OutputId("out-a".to_owned()), view_a.clone())),
            native_pre: None,
            native_post: None,
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
    let source_scope = format!("out-a/{view_a}");
    let target_scope = format!("out-b/{view_a}");
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope == source_scope)
        .expect("floating source scope on rollback reply");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Untouched,
        "floating source Untouched on rollback: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| s.scope != target_scope),
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
        .find(|s| s.scope == format!("out-a/{view_a}"))
        .expect("scope row");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Untouched,
        "floating mode restored: {scope:?}"
    );
    assert!(
        calm.scopes
            .iter()
            .all(|s| s.scope != format!("out-b/{view_a}")),
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        // Lag keeps the flight with no replay; exact receipt settles.
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
        let mut arrived = float_fact("wf", "out-a", 200);
        arrived.observed_workspace = Some(ring[1].clone());
        let settled = accepted(
            &mut engine,
            &report(5, screens.clone(), vec![arrived], "wf", vec![], None, None),
        );
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        let mut resident = fact("wr", "out-a", 300);
        resident.observed_workspace = Some(ring[1].clone());
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
        assert_eq!(
            reply.effects.len(),
            1,
            "stay names the survivor: {:?}",
            reply.effects
        );
        assert_eq!(reply.effects[0].target.0, "w0");
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![roomy("out-a", 0, true)];
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
    fullscreen_w1.fullscreen = true;
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
    still_out.floating = true;
    still_out.fullscreen = true;
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
    float_full.floating = true;
    float_full.fullscreen = true;
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        fullscreen_w1.fullscreen = true;
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
        exited.floating = true;
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        fullscreen_w1.fullscreen = true;
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
        float_full.floating = true;
        float_full.fullscreen = true;
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
        let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        floated.floating = true;
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
        fullscreen_w1.fullscreen = true;
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
    maxed.maximized = true;
    match engine.handle_report(&report(
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
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "not-tiled"),
        other => panic!("maximized toggle must refuse, got {other:?}"),
    }

    // Floating + maximized unfloat refuses too (post-clear observation
    // required): fresh engine, plain float, then the maximized overlay.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
    maxed_float.floating = true;
    maxed_float.maximized = true;
    match engine.handle_report(&report(
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
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "partial-observation"),
        other => panic!("maximized unfloat must refuse, got {other:?}"),
    }
}
