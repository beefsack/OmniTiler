//! Stage-4b cursor policy tests: core-owned focus-follows-cursor and
//! cross-output cursor-warp over normalized report facts.
//!
//! The adapter reports one cursor sample per report plus the two retained
//! behavior settings (both default off); the core owns every focus/warp
//! decision, pausing both under a focused fullscreen game, incomplete
//! inventory, and gesture barriers. Warp is cross-output only (the
//! approved i3/sway preset behavior): same-output focus never warps.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> crate::world_report::ScreenReport {
    crate::world_report::ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
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

fn fullscreen_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        fullscreen: true,
        ..fact(window, output, x)
    }
}

fn minimized_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        rect: None,
        minimized: true,
        ..fact(window, output, 0)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    cursor_hit: Option<&str>,
    focus_follows_cursor: bool,
    cursor_warp: bool,
    inventory_complete: bool,
    gesture_barrier: bool,
) -> WorldReport {
    command_report(
        request_seq,
        observation_seq,
        windows,
        focused,
        cursor,
        cursor_hit,
        focus_follows_cursor,
        cursor_warp,
        inventory_complete,
        gesture_barrier,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn command_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    cursor_hit: Option<&str>,
    focus_follows_cursor: bool,
    cursor_warp: bool,
    inventory_complete: bool,
    gesture_barrier: bool,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor,
        cursor_warp,
        cursor,
        cursor_hit: cursor_hit.map(|hit| WindowId(hit.to_owned())),
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete,
        gesture_barrier,
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

/// Two tiled windows on out-a, focused on win-a: the hover rig. The first
/// report bootstraps (quiet); the second carries the cursor/settings under
/// test.
fn rig() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((10, 10)),
            None,
            false,
            false,
            true,
            false,
        ),
    );
    engine
}

#[test]
fn focus_follows_off_by_default_mints_no_effect() {
    let mut engine = rig();
    // Cursor over win-b while win-a stays focused, both settings off.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            None,
            false,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
    assert_eq!(reply.cursor_warp, None);
}

#[test]
fn focus_follows_on_mints_hover_effect() {
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].id, "ffc-win-b");
    assert_eq!(reply.effects[0].target.0, "win-b");
    // Hover coupling is same-output here: no warp.
    assert_eq!(reply.cursor_warp, None);
}

#[test]
fn focus_follows_holds_when_already_focused() {
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((10, 10)),
            Some("win-a"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_paused_under_focused_fullscreen_game() {
    let mut engine = rig();
    // The focused window reports fullscreen: a game pause. Hover coupling
    // must not steal focus away from (or toward) the game.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
            ],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(
        reply
            .effects
            .iter()
            .all(|effect| effect.target.0 != "win-b"),
        "game pause must mint no hover focus"
    );
}

#[test]
fn focus_follows_paused_on_incomplete_inventory() {
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            false,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_paused_under_gesture_barrier() {
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            true,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_ignores_cursor_outside_every_window() {
    let mut engine = rig();
    // Cursor on empty area (past both frames): no hover target, no effect.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((500, 500)),
            None,
            true,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

/// Two-output rig: win-a on out-a focused, win-b on out-b.
fn cross_rig() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((10, 10)),
            None,
            false,
            false,
            true,
            false,
        ),
    );
    engine
}

#[test]
fn warp_off_by_default_publishes_no_point() {
    let mut engine = cross_rig();
    // Explicit cross-output focus with warp off: effect rides, no point.
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((10, 10)),
            None,
            false,
            false,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-b");
    assert_eq!(reply.cursor_warp, None);
}

#[test]
fn warp_publishes_cross_output_centre() {
    let mut engine = cross_rig();
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((10, 10)),
            None,
            false,
            true,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-b");
    // Win-b's desired row carries its effective frame; the warp point is
    // that frame's centre, inside out-b's work area.
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("win-b row");
    let frame = row.effective_frame.expect("tiled target has a frame");
    let want = (
        frame.x.saturating_add(frame.w / 2),
        frame.y.saturating_add(frame.h / 2),
    );
    assert_eq!(reply.cursor_warp, Some(want));
    let area = rect(800, 0, 800, 560);
    let (x, y) = want;
    assert!(x >= area.x && x < area.x + area.w);
    assert!(y >= area.y && y < area.y + area.h);
}

#[test]
fn warp_skips_same_output_focus() {
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((10, 10)),
            None,
            false,
            true,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.cursor_warp, None);
}

#[test]
fn warp_paused_under_focused_fullscreen_game() {
    let mut engine = cross_rig();
    // Even with an explicit cross-output focus effect, a focused
    // fullscreen game pauses the warp point: the cursor must not move.
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![
                fullscreen_fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
            ],
            "win-a",
            Some((10, 10)),
            None,
            false,
            true,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.cursor_warp, None);
}

#[test]
fn warp_needs_a_known_cursor_output() {
    for cursor in [None, Some((5000, 5000))] {
        let mut engine = cross_rig();
        let reply = accepted(
            &mut engine,
            &command_report(
                2,
                2,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
                "win-a",
                cursor,
                None,
                false,
                true,
                true,
                false,
                Some(11),
                Some(WorldCommand::Focus {
                    window: WindowId("win-b".to_owned()),
                    direction: "right".to_owned(),
                }),
            ),
        );
        assert_eq!(reply.cursor_warp, None, "cursor {cursor:?}");
    }
}

/// Three tiled windows on out-a, win-c exactly overlapping win-b.
fn overlap_rig() -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((10, 10)),
            None,
            false,
            false,
            true,
            false,
        ),
    );
    engine
}

#[test]
fn focus_follows_overlap_uses_topmost_hit() {
    // win-b and win-c overlap exactly; the native hit names the topmost
    // one and the core couples to it without guessing by area or order.
    let mut engine = overlap_rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 100),
            ],
            "win-a",
            Some((110, 10)),
            Some("win-c"),
            true,
            false,
            true,
            false,
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].id, "ffc-win-c");
    assert_eq!(reply.effects[0].target.0, "win-c");
}

#[test]
fn focus_follows_absent_hit_mints_nothing() {
    // The cursor sits over an eligible window but the adapter reports no
    // hit (foreign or occluded pixels): no FFC, never a rectangle guess.
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            None,
            true,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_hit_beats_rectangles() {
    // The hit names win-b while the cursor point sits inside win-a's
    // frame: the native hit decides, not the cursor math.
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((10, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-b");
}

#[test]
fn focus_follows_minimized_hit_mints_nothing() {
    // A hit naming a minimized lifetime is not a visible hover target:
    // no FFC, even with a moved cursor and no stronger focus.
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), minimized_fact("win-b", "out-a")],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_stale_hit_refuses() {
    // A hit naming no reported window (stale or recycled id) refuses the
    // report instead of focusing blindly.
    let mut engine = rig();
    match engine.handle_report(&report(
        2,
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        Some((110, 10)),
        Some("win-ghost"),
        true,
        false,
        true,
        false,
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected rejection, got {other:?}"),
    }
}

#[test]
fn focus_follows_hit_without_cursor_refuses() {
    // A hit without the cursor position it was resolved from refuses:
    // the two always travel together.
    let mut engine = rig();
    match engine.handle_report(&report(
        2,
        2,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        None,
        Some("win-b"),
        true,
        false,
        true,
        false,
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected rejection, got {other:?}"),
    }
}

#[test]
fn focus_follows_ignores_noncurrent_view() {
    // After the view on out-b moves away, win-b's fact is a non-current
    // snapshot: hover must not couple to it.
    let mut engine = cross_rig();
    let current = engine.logical_view("out-b").expect("out-b view");
    let other = engine
        .logical_ring("out-b")
        .into_iter()
        .find(|id| id != &current)
        .expect("second workspace on out-b");
    accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((10, 10)),
            Some("win-a"),
            true,
            false,
            true,
            false,
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-b".to_owned()),
                workspace: other,
            }),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            "win-a",
            Some((810, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(reply.effects.is_empty());
}

#[test]
fn focus_follows_static_cursor_never_resteals() {
    // Hover couples once on movement; a later keyboard command moving
    // focus away must not be undone by the unchanged cursor.
    let mut engine = rig();
    let coupled = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert_eq!(coupled.effects.len(), 1);
    assert_eq!(coupled.effects[0].target.0, "win-b");
    // Adapter applies the coupling: focus arrives on win-b.
    let idle = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(idle.effects.is_empty());
    // Keyboard command moves focus back to win-a; the static cursor over
    // win-b must not steal it back, neither now nor on the next
    // observation.
    let commanded = accepted(
        &mut engine,
        &command_report(
            4,
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-b",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    assert_eq!(commanded.effects.len(), 1);
    assert_eq!(commanded.effects[0].target.0, "win-a");
    let settled = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert!(settled.effects.is_empty());
}

#[test]
fn focus_follows_yields_to_fresh_admission() {
    // A newcomer on the same report takes the only focus effect; hover
    // coupling stands down even with a moved cursor over another window.
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-c");
}

#[test]
fn focus_follows_yields_to_explicit_command_focus() {
    // An explicit focus command wins; hover coupling mints nothing
    // alongside it even with a moved cursor over the command target.
    let mut engine = rig();
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((110, 10)),
            Some("win-b"),
            true,
            false,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-b".to_owned()),
                direction: "right".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-b");
}

#[test]
fn warp_follows_last_focus_effect_not_last_cross_output() {
    // Admission focuses cross-output first, then the explicit command
    // focuses same-output last: the resulting focus stays on the cursor
    // output, so no warp rides the earlier cross-output effect.
    let mut engine = cross_rig();
    let reply = accepted(
        &mut engine,
        &command_report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-b", 900),
            ],
            "win-a",
            Some((10, 10)),
            None,
            true,
            true,
            true,
            false,
            Some(11),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    assert_eq!(reply.effects.len(), 2);
    assert_eq!(reply.effects[0].target.0, "win-c");
    assert_eq!(reply.effects[1].target.0, "win-a");
    assert_eq!(reply.cursor_warp, None);
}
