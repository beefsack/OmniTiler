//! No-focused-window directional focus (User 2026-10-26): with no focused
//! window, direction-only focus starts at the current workspace/output
//! resolved from the neutral action context and crosses to the adjacent
//! output by the existing cross-output selection (the destination domain's
//! remembered tiled leaf), otherwise it is a no-op. Never selects a
//! current-workspace window. Companion proof that host-caused focus loss
//! on an observation-only report emits no forced focus effect.

use crate::directional::{Direction, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    EffectKind, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_sized(output: &str, x: i32, y: i32, w: i32, h: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, y, w, h)),
        work_area: Some(rect(x, y, w, h)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn side_by_side() -> Vec<ScreenReport> {
    vec![
        screen_sized("out-a", 0, 0, 800, 600, true),
        screen_sized("out-b", 800, 0, 800, 600, false),
    ]
}

fn stacked() -> Vec<ScreenReport> {
    vec![
        screen_sized("out-a", 0, 0, 800, 600, true),
        screen_sized("out-c", 0, 600, 800, 600, false),
    ]
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

fn unreadable_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        rect: None,
        ..fact(window, output, 0)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    action_foreground: Option<&str>,
    cursor: Option<(i32, i32)>,
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
        views: views
            .into_iter()
            .map(|(output, view)| crate::world_report::ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: Vec::new(),
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
        cursor,
        cursor_hit: None,
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: action_foreground.map(|output| OutputId(output.to_owned())),
        action_cached: None,
        action_outputs: vec![
            OutputId("out-a".to_owned()),
            OutputId("out-b".to_owned()),
            OutputId("out-c".to_owned()),
        ],
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

fn no_focus_command(direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    }
}

fn focus_effects(
    reply: &crate::world_report::WorldReply,
) -> Vec<&crate::world_report::OneShotEffect> {
    reply
        .effects
        .iter()
        .filter(|effect| effect.kind == EffectKind::Focus)
        .collect()
}

fn retained_outcome(reply: &crate::world_report::WorldReply) -> (String, Option<String>) {
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Directional { outcome, cause, .. }) => {
            (outcome.as_str().to_owned(), cause.clone())
        }
        other => panic!("expected directional result, got {other:?}"),
    }
}

/// Bootstrap two side-by-side outputs and record explicit focus history:
/// win-a2 is the current source selection, win-b2 the remembered target
/// selection. Returns the engine ready for a no-focus command.
fn seeded_history() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    let windows = || {
        vec![
            fact("win-a1", "out-a", 10),
            fact("win-a2", "out-a", 100),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 900),
        ]
    };
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            windows(),
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            screens.clone(),
            windows(),
            vec![],
            "win-a2",
            None,
            None,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            screens.clone(),
            windows(),
            vec![],
            "win-b2",
            None,
            None,
            None,
            None,
        ),
    );
    // Host-caused focus loss: no command, no focused window.
    let loss = accepted(
        &mut engine,
        &report(4, screens, windows(), vec![], "", None, None, None, None),
    );
    assert!(
        focus_effects(&loss).is_empty(),
        "loss converges without forced focus: {:?}",
        loss.effects
    );
    engine
}

#[test]
fn no_focus_crosses_occupied_origin_to_existing_selection() {
    // Occupied source with two windows: the crossing still names the
    // destination's remembered selection, never a current-workspace
    // window.
    let mut engine = seeded_history();
    let reply = accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    let focus = focus_effects(&reply);
    assert_eq!(focus.len(), 1, "one focus effect: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "win-b2", "existing selection");
    assert!(
        focus[0].id.starts_with("fx-"),
        "one-shot id: {}",
        focus[0].id
    );
    assert!(
        reply.command_result.is_none(),
        "success rides the effect like focused crossing: {:?}",
        reply.command_result
    );
    assert!(
        engine.retained_transfers().is_empty(),
        "read-only selection retains nothing"
    );
}

#[test]
fn no_focus_crosses_empty_origin() {
    // Empty source workspace: no local subject exists, yet the adjacent
    // output's selection is still reached.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("win-b1", "out-b", 810)],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens,
            vec![fact("win-b1", "out-b", 810)],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    let focus = focus_effects(&reply);
    assert_eq!(
        focus.len(),
        1,
        "empty origin still crosses: {:?}",
        reply.effects
    );
    assert_eq!(focus[0].target.0, "win-b1");
}

#[test]
fn no_focus_resolves_source_from_cursor_when_foreground_unknown() {
    // No foreground leg: the pointer leg still resolves the current
    // output instead of inventing focus.
    let mut engine = seeded_history();
    let reply = accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("left")),
            None,
            Some((900, 10)),
        ),
    );
    let focus = focus_effects(&reply);
    assert_eq!(
        focus.len(),
        1,
        "cursor resolves source: {:?}",
        reply.effects
    );
    assert_eq!(
        focus[0].target.0, "win-a2",
        "left from out-b names out-a selection"
    );
}

#[test]
fn no_focus_without_adjacent_output_is_noop() {
    // Single output: no strict neighbor, so no effect and no
    // current-workspace pick.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen_sized("out-a", 0, 0, 800, 600, true)];
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("win-a1", "out-a", 10), fact("win-a2", "out-a", 100)],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens,
            vec![fact("win-a1", "out-a", 10), fact("win-a2", "out-a", 100)],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(reply.effects.is_empty(), "no neighbor: {:?}", reply.effects);
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("no-adjacent-output".to_owned())
        )
    );
}

#[test]
fn no_focus_without_destination_selection_is_noop() {
    // Adjacent output exists but is empty: no remembered selection, so
    // no effect rather than an invented target.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("win-a1", "out-a", 10)],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens,
            vec![fact("win-a1", "out-a", 10)],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "empty target: {:?}",
        reply.effects
    );
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("no-target".to_owned())
        )
    );
}

#[test]
fn no_focus_without_resolvable_source_is_noop() {
    // No action-context leg resolves: the command retains without
    // retargeting to registry order.
    let mut engine = seeded_history();
    let mut crossing = report(
        5,
        side_by_side(),
        vec![
            fact("win-a1", "out-a", 10),
            fact("win-a2", "out-a", 100),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 900),
        ],
        vec![],
        "",
        Some(11),
        Some(no_focus_command("right")),
        None,
        None,
    );
    crossing.action_outputs.clear();
    let reply = accepted(&mut engine, &crossing);
    assert!(reply.effects.is_empty(), "no source: {:?}", reply.effects);
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("no-source".to_owned())
        )
    );
}

#[test]
fn no_focus_up_down_cross_stacked_outputs() {
    // With no local tree to prefer, vertical directions cross stacked
    // outputs by the same existing selection.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = stacked();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("win-a1", "out-a", 10), fact("win-c1", "out-c", 10)],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens,
            vec![fact("win-a1", "out-a", 10), fact("win-c1", "out-c", 10)],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("down")),
            Some("out-a"),
            None,
        ),
    );
    let focus = focus_effects(&reply);
    assert_eq!(focus.len(), 1, "down crosses: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "win-c1");
}

#[test]
fn no_focus_contradictory_focus_present_is_noop() {
    // The adapter claimed no subject while the report carries a focused
    // window: fail closed instead of crossing.
    let mut engine = seeded_history();
    let reply = accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "win-a1",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "contradiction: {:?}",
        reply.effects
    );
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("focus-present".to_owned())
        )
    );
}

#[test]
fn no_focus_unknown_window_stays_stale() {
    // A named but unknown window is a stale subject, not a no-focus
    // start: existing refusal vocabulary is unchanged.
    let mut engine = seeded_history();
    let reply = accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: WindowId("win-gone".to_owned()),
                direction: "right".to_owned(),
            }),
            Some("out-a"),
            None,
        ),
    );
    assert!(reply.effects.is_empty(), "stale: {:?}", reply.effects);
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Directional { outcome, cause, .. }) => {
            assert_eq!(outcome.as_str(), "focus-float-refused-stale");
            assert_eq!(cause.as_deref(), Some("unknown-window"));
        }
        other => panic!("expected stale refusal, got {other:?}"),
    }
}

#[test]
fn no_focus_invalid_direction_rejects() {
    // The direction vocabulary gate runs before any no-focus resolution.
    let mut engine = seeded_history();
    let report = report(
        5,
        side_by_side(),
        vec![
            fact("win-a1", "out-a", 10),
            fact("win-a2", "out-a", 100),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 900),
        ],
        vec![],
        "",
        Some(11),
        Some(WorldCommand::FocusDirectional {
            window: WindowId(String::new()),
            direction: "sideways".to_owned(),
        }),
        Some("out-a"),
        None,
    );
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { message, .. } => assert_eq!(message, "command-invalid"),
        other => panic!("expected validation refusal, got {other:?}"),
    }
}

#[test]
fn host_focus_loss_observation_emits_no_focus() {
    // Desktop/taskbar click or minimize drops the global focus with no
    // command: the tiler never forces focus back. Empty and occupied
    // origins both stay quiet.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![fact("win-a1", "out-a", 10), fact("win-b1", "out-b", 810)],
            vec![],
            "win-a1",
            None,
            None,
            None,
            None,
        ),
    );
    for (seq, windows) in [
        (
            2u64,
            vec![fact("win-a1", "out-a", 10), fact("win-b1", "out-b", 810)],
        ),
        (3u64, vec![fact("win-b1", "out-b", 810)]),
    ] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                screens.clone(),
                windows,
                vec![],
                "",
                None,
                None,
                None,
                None,
            ),
        );
        assert!(
            focus_effects(&reply).is_empty(),
            "loss emits no focus (seq {seq}): {:?}",
            reply.effects
        );
        assert!(
            reply.command_result.is_none(),
            "loss mints no result (seq {seq}): {:?}",
            reply.command_result
        );
    }
}

#[test]
fn no_focus_floating_source_stops_navigation() {
    // A floating current workspace stops directional navigation like the
    // subject path: same refusal vocabulary, no crossing.
    let mut engine = seeded_history();
    let view_a = engine.logical_view("out-a").expect("view");
    accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a,
                tiled: false,
            }),
            Some("out-a"),
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            6,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(12),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "floating source: {:?}",
        reply.effects
    );
    match &reply.command_result {
        Some(crate::world_report::CommandResult::Directional { outcome, cause, .. }) => {
            assert_eq!(outcome.as_str(), "focus-refused-workspace-floating");
            assert!(cause.is_none());
        }
        other => panic!("expected workspace-floating refusal, got {other:?}"),
    }
}

#[test]
fn no_focus_floating_destination_is_noop() {
    // A floating destination holds no retained session, so there is no
    // existing tiled selection: a quiet no-op, never a floating pick.
    let mut engine = seeded_history();
    let view_b = engine.logical_view("out-b").expect("view");
    accepted(
        &mut engine,
        &report(
            5,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-b".to_owned()),
                workspace: view_b,
                tiled: false,
            }),
            Some("out-a"),
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            6,
            side_by_side(),
            vec![
                fact("win-a1", "out-a", 10),
                fact("win-a2", "out-a", 100),
                fact("win-b1", "out-b", 810),
                fact("win-b2", "out-b", 900),
            ],
            vec![],
            "",
            Some(12),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "floating destination: {:?}",
        reply.effects
    );
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("no-target".to_owned())
        )
    );
}

#[test]
fn no_focus_unreadable_source_refuses() {
    // An unreadable current workspace never authorizes a crossing: refuse
    // with the operation vocabulary and no mutation.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![
                unreadable_fact("win-a1", "out-a"),
                fact("win-b1", "out-b", 810),
            ],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let revision_before = engine.state_revision();
    let report = report(
        2,
        screens,
        vec![
            unreadable_fact("win-a1", "out-a"),
            fact("win-b1", "out-b", 810),
        ],
        vec![],
        "",
        Some(11),
        Some(no_focus_command("right")),
        Some("out-a"),
        None,
    );
    match engine.handle_report(&report) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "operation-refused");
            assert_eq!(message, "operation route refused without mutation");
        }
        other => panic!("expected operation refusal, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision_before, "no mutation");
    assert!(engine.retained_transfers().is_empty());
}

#[test]
fn no_focus_unreadable_destination_is_noop() {
    // An unreadable destination view resolves past to the readable spare,
    // which holds no selection: a quiet no-op that never selects through
    // the unreadable scope.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = side_by_side();
    accepted(
        &mut engine,
        &report(
            1,
            screens.clone(),
            vec![
                fact("win-a1", "out-a", 10),
                unreadable_fact("win-b1", "out-b"),
            ],
            vec![],
            "",
            None,
            None,
            None,
            None,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            screens,
            vec![
                fact("win-a1", "out-a", 10),
                unreadable_fact("win-b1", "out-b"),
            ],
            vec![],
            "",
            Some(11),
            Some(no_focus_command("right")),
            Some("out-a"),
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "unreadable destination: {:?}",
        reply.effects
    );
    assert_eq!(
        retained_outcome(&reply),
        (
            "focus-unfocused-retained".to_owned(),
            Some("no-target".to_owned())
        )
    );
    assert!(engine.retained_transfers().is_empty());
}

#[test]
fn no_focus_held_source_refuses_without_mutation() {
    // A held current scope refuses before any resolution: unit over the
    // branch with a staged engine and an explicit held set.
    use std::collections::{BTreeMap, BTreeSet};
    let mut engine = seeded_history();
    let view_a = engine.logical_view("out-a").expect("view");
    let source_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(view_a),
    };
    let owned: Vec<crate::seed::EngineWindow> = engine
        .staged
        .domains
        .iter()
        .flat_map(|domain| domain.windows.iter().cloned())
        .collect();
    let by_id: BTreeMap<&WindowId, &crate::seed::EngineWindow> = owned
        .iter()
        .map(|window| (&window.window, window))
        .collect();
    let report = report(
        5,
        side_by_side(),
        vec![
            fact("win-a1", "out-a", 10),
            fact("win-a2", "out-a", 100),
            fact("win-b1", "out-b", 810),
            fact("win-b2", "out-b", 900),
        ],
        vec![],
        "",
        Some(11),
        Some(no_focus_command("right")),
        Some("out-a"),
        None,
    );
    let mut held = BTreeSet::new();
    held.insert(source_key);
    let revision_before = engine.state_revision();
    match engine.focus_directional_no_focus(&report, 11, Direction::Right, &held, &by_id) {
        crate::world_commands::CommandApplied::Rejected { kind, .. } => {
            assert_eq!(kind, "scope-held")
        }
        _ => panic!("expected scope-held rejection"),
    }
    assert_eq!(engine.state_revision(), revision_before, "no mutation");
}
