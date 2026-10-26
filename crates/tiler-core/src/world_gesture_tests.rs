//! Stage-4a gesture lifecycle coverage: normalized ingress, retained core
//! gesture/target/overlay/release, and edge-share policy.
//!
//! Pure decision tests pin the moved Windows derivations
//! (`classify_gesture`, `prefer_modal_move`, `drop_point_in_domain`,
//! pointer containment, drop-destination resolution).
//! Engine tests drive the retained lifecycle through the world contract:
//! start/update/release/cancel, Esc, stale/replacement, gaps, game pause
//! (cursor-less updates), cross-output send semantics, resize share
//! policy, close clearing, and incarnation reset.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::gesture::{
    DropDestination, GestureIngress, GestureIntent, GestureKind, GesturePhase, classify_gesture,
    drop_point_in_domain, pointer_output, prefer_modal_move, resolve_drop_destination,
};
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandResult, ScreenReport, TransferKind, WORLD_CONTRACT_VERSION, WindowFact, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen_full(output: &str, full: Rect, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(full),
        work_area: Some(full),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
    }
}

fn screens() -> Vec<ScreenReport> {
    vec![
        screen_full("out-a", rect(0, 0, 800, 600), true),
        screen_full("out-b", rect(800, 0, 800, 600), false),
    ]
}

fn fact(window: &str, output: &str, r: Rect) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(r),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
        native_workspace: None,
    }
}

fn ingress(
    window: &str,
    kind: GestureKind,
    cursor: Option<(i32, i32)>,
    phase: GesturePhase,
    start_frame: Rect,
) -> GestureIngress {
    ingress_id(window, kind, cursor, phase, start_frame, 1)
}

/// Same gesture with an explicit stable id. Fresh ids order above every
/// consumed terminal; reused ids replay.
fn ingress_id(
    window: &str,
    kind: GestureKind,
    cursor: Option<(i32, i32)>,
    phase: GesturePhase,
    start_frame: Rect,
    id: u64,
) -> GestureIngress {
    GestureIngress {
        id,
        window: WindowId(window.to_owned()),
        kind,
        cursor,
        start_cursor: None,
        phase,
        esc: false,
        modal_move: kind == GestureKind::Move,
        start_frame,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
    closed: Vec<WindowId>,
    inventory_complete: bool,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: screens(),
        windows,
        views: Vec::new(),
        closed,
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
        inventory_complete,
        gesture_barrier: gesture.is_some(),
        gesture,
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

/// Two tiled windows on out-a, focused on win-a: the stable gesture rig.
/// win-a starts at (0,0,40,90) like the fact helper; the Engine tiles the
/// pair across the 800px work area.
fn rig() -> (WorldEngine, Rect, Rect) {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let ra = rect(0, 0, 40, 90);
    let rb = rect(100, 0, 40, 90);
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "",
            None,
            vec![],
            true,
        ),
    );
    (engine, ra, rb)
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

fn preview_of(reply: &crate::world_report::WorldReply) -> Rect {
    let Some(CommandResult::DragPreview { rect, .. }) = reply.command_result.clone() else {
        panic!(
            "expected drag-preview result, got {:?}",
            reply.command_result
        );
    };
    rect
}

// --- Pure release-intent derivation (moved Windows policy) ---

#[test]
fn classify_move_needs_cursor_and_reports_drop_point() {
    let before = rect(10, 20, 100, 100);
    let after = rect(30, 20, 100, 100);
    assert_eq!(
        classify_gesture(&before, &after, Some((40, 60))),
        Some(GestureIntent::MoveDrop { x: 40, y: 60 })
    );
    assert_eq!(classify_gesture(&before, &after, None), None);
    assert_eq!(classify_gesture(&before, &before, Some((10, 20))), None);
}

#[test]
fn classify_resize_picks_edges_per_axis_with_origin_tiebreak() {
    // Left edge moved right: origin edge wins the axis.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(30, 20, 80, 100),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "left",
            boundary: 30,
            direction2: None,
            boundary2: None,
        })
    );
    // Right edge moved only: far edge wins.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(10, 20, 140, 100),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 150,
            direction2: None,
            boundary2: None,
        })
    );
    // Corner: both axes derive, horizontal first. The bottom edge moved
    // up (120 -> 80), so the vertical far edge ("down") wins at 80.
    assert_eq!(
        classify_gesture(
            &rect(10, 20, 100, 100),
            &rect(10, 20, 140, 60),
            Some((0, 0))
        ),
        Some(GestureIntent::PointerResize {
            direction: "right",
            boundary: 150,
            direction2: Some("down"),
            boundary2: Some(80),
        })
    );
}

#[test]
fn prefer_modal_move_overrides_rescale_misread_resize_only() {
    let resize = GestureIntent::PointerResize {
        direction: "right",
        boundary: 150,
        direction2: None,
        boundary2: None,
    };
    assert_eq!(
        prefer_modal_move(resize, Some((9, 9)), true),
        GestureIntent::MoveDrop { x: 9, y: 9 }
    );
    // Cursor-less resizes keep the geometry route: no point to place.
    assert_eq!(prefer_modal_move(resize, None, true), resize);
    // Move verdicts pass through even with modal truth.
    let mov = GestureIntent::MoveDrop { x: 1, y: 2 };
    assert_eq!(prefer_modal_move(mov, Some((9, 9)), true), mov);
    // Without modal truth the classified resize stands.
    assert_eq!(prefer_modal_move(resize, Some((9, 9)), false), resize);
}

#[test]
fn domain_fence_rejects_outside_and_degenerate() {
    assert!(drop_point_in_domain(0, 0, 800, 600, 799, 599));
    assert!(!drop_point_in_domain(0, 0, 800, 600, 800, 600));
    assert!(!drop_point_in_domain(0, 0, 800, 600, -1, 0));
    assert!(!drop_point_in_domain(0, 0, 0, 600, 0, 0));
}

#[test]
fn pointer_output_uses_full_containment_half_open() {
    let fulls = vec![
        (OutputId("a".to_owned()), rect(0, 0, 800, 600)),
        (OutputId("b".to_owned()), rect(800, 0, 800, 600)),
    ];
    assert_eq!(
        pointer_output(&fulls, 799, 10).map(|o| o.0),
        Some("a".to_owned())
    );
    // Half-open: the shared edge belongs to the right screen.
    assert_eq!(
        pointer_output(&fulls, 800, 10).map(|o| o.0),
        Some("b".to_owned())
    );
    assert_eq!(pointer_output(&fulls, 5000, 10), None);
    let flat = vec![(OutputId("z".to_owned()), rect(0, 0, 0, 600))];
    assert_eq!(pointer_output(&flat, 0, 10), None);
}

#[test]
fn drop_destination_names_same_cross_and_refusals() {
    let fulls = vec![
        (OutputId("a".to_owned()), rect(0, 0, 800, 600)),
        (OutputId("b".to_owned()), rect(800, 0, 800, 600)),
    ];
    let src = OutputId("a".to_owned());
    let inset_a = Some(rect(0, 0, 800, 600));
    let tiled = |_: &OutputId| Some(true);
    let inset_b = |o: &OutputId| (o.0 == "b").then_some(rect(800, 0, 800, 600));
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 100, 100, inset_a, tiled, inset_b),
        DropDestination::SameOutput
    );
    // Same output but outside the inset work area refuses.
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            100,
            100,
            Some(rect(0, 0, 50, 50)),
            tiled,
            inset_b
        ),
        DropDestination::Refused("outside-work-area")
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 900, 100, inset_a, tiled, inset_b),
        DropDestination::Cross {
            output: OutputId("b".to_owned())
        }
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 5000, 100, inset_a, tiled, inset_b),
        DropDestination::Refused("outside-output")
    );
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            900,
            100,
            inset_a,
            |_: &OutputId| Some(false),
            inset_b
        ),
        DropDestination::Refused("floating-dest")
    );
    assert_eq!(
        resolve_drop_destination(
            &fulls,
            &src,
            900,
            100,
            inset_a,
            |_: &OutputId| None,
            inset_b
        ),
        DropDestination::Refused("no-view")
    );
    assert_eq!(
        resolve_drop_destination(&fulls, &src, 900, 100, inset_a, tiled, |_: &OutputId| None),
        DropDestination::Refused("unknown-output")
    );
}

#[test]
fn gesture_wire_tokens_roundtrip_and_reject_unknown() {
    assert_eq!(GestureKind::parse_wire("move"), Some(GestureKind::Move));
    assert_eq!(GestureKind::parse_wire("resize"), Some(GestureKind::Resize));
    assert_eq!(GestureKind::parse_wire("drag"), None);
    assert_eq!(
        GesturePhase::parse_wire("released"),
        Some(GesturePhase::Released)
    );
    assert_eq!(GesturePhase::parse_wire("dropped"), None);
}

// --- Retained lifecycle through the world contract ---

#[test]
fn update_without_bind_is_stale_until_first_update_binds() {
    let (mut engine, ra, rb) = rig();
    // Unknown lifetime never binds: no overlay, no session.
    let unknown = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(unknown.command_result.is_none());
    assert!(unknown.transfers.is_empty());
    // A valid update binds implicitly and previews.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let rect = preview_of(&previewed);
    assert!(rect.w > 0 && rect.h > 0, "bound update previews");
}

#[test]
fn move_update_previews_and_clears_on_release_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Bind + preview in one update: insertion neighbor is win-b, never
    // the mover.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let overlay = preview_of(&previewed);
    assert!(overlay.w > 0 && overlay.h > 0);
    assert!(previewed.transfers.is_empty(), "preview never retains");
    // Release with the frame moved into the left half: commits the
    // resolver placement (win-a leaves its right slot).
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        released.command_result.is_none(),
        "release rides the snapshot"
    );
    assert_ne!(
        row_of(&released, "win-a").allocation,
        before_alloc,
        "release commits the drop"
    );
    assert!(
        released.transfers.is_empty(),
        "same-output drop retains nothing"
    );
    // Terminal: the old session is gone. A repeated release with the
    // same carried frames re-derives the same drop against the already
    // committed layout: self, unchanged, snap-back with no re-commit.
    let rerelease = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(rerelease.command_result.is_none());
    assert!(rerelease.transfers.is_empty(), "no double commit");
    assert_eq!(
        row_of(&rerelease, "win-a").allocation,
        row_of(&released, "win-a").allocation,
        "re-release is a no-op"
    );
}

#[test]
fn release_without_movement_snaps_back_without_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Same frame plus a cursor is no change: clear, no commit.
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(row_of(&released, "win-a").allocation, before_alloc);
    assert!(released.transfers.is_empty());
}

#[test]
fn cursorless_move_release_clears_without_commit() {
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(500, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                None,
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(released.command_result.is_none());
    assert!(released.transfers.is_empty());
}

#[test]
fn cursorless_update_holds_with_no_overlay_work() {
    // Game pause falls out naturally: the adapter unsamples the cursor
    // under a focused fullscreen game, so updates hold with no overlay
    // and the session survives for the terminal phase.
    let (mut engine, ra, rb) = rig();
    let held = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                None,
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(held.command_result.is_none(), "no cursor, no overlay");
    // The session survives: a cursor-bearing update previews.
    let previewed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cancel_and_esc_clear_without_commit() {
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Bind, then cancel: cleared, nothing committed.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let cancelled = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.command_result.is_none());
    assert_eq!(row_of(&cancelled, "win-a").allocation, before_alloc);
    // Host Esc edge clears regardless of phase.
    accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let mut esc = ingress(
        "win-a",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Updated,
        ra,
    );
    esc.esc = true;
    let cleared = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(esc),
            vec![],
            true,
        ),
    );
    assert!(cleared.command_result.is_none());
}

#[test]
fn replacement_window_is_ignored_while_one_is_retained() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // A different lifetime never replaces the retained session: the old
    // one holds and the newcomer is ignored, never silently adopted.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-b",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                rb,
            )),
            vec![],
            true,
        ),
    );
    assert!(held.command_result.is_none());
    // The retained session still previews.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn invalid_starts_refuse_without_session() {
    // Minimized, unreadable, fullscreen, floating, sticky, and incomplete
    // starts all fail closed: no overlay on the next update.
    let (mut engine, ra, rb) = rig();
    let mut seq = 2u64;
    let mut minimized = fact("win-a", "out-a", ra);
    minimized.minimized = true;
    minimized.rect = None;
    for (tag, facts) in [
        (
            "minimized",
            vec![minimized.clone(), fact("win-b", "out-a", rb)],
        ),
        (
            "unreadable",
            vec![
                WindowFact {
                    rect: None,
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "fullscreen",
            vec![
                WindowFact {
                    fullscreen: true,
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "floating",
            vec![
                WindowFact {
                    floating: true,
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
        (
            "sticky",
            vec![
                WindowFact {
                    sticky: true,
                    ..fact("win-a", "out-a", ra)
                },
                fact("win-b", "out-a", rb),
            ],
        ),
    ] {
        let refused = accepted(
            &mut engine,
            &report(
                seq,
                seq,
                facts,
                "win-a",
                Some(ingress(
                    "win-a",
                    GestureKind::Move,
                    Some((150, 50)),
                    GesturePhase::Updated,
                    ra,
                )),
                vec![],
                true,
            ),
        );
        seq += 1;
        assert!(
            refused.command_result.is_none(),
            "{tag} start must not preview"
        );
    }
    // Incomplete inventory never binds.
    let gapped = accepted(
        &mut engine,
        &report(
            seq,
            seq,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            false,
        ),
    );
    assert!(gapped.command_result.is_none());
}

#[test]
fn gap_holds_and_recovers_without_replay() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // An incomplete report holds the session with no overlay.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            false,
        ),
    );
    assert!(held.command_result.is_none());
    // Recovery previews again with no replay of the held update.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn close_clears_the_session() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // Explicit close clears even across the gap-safe retention.
    let closed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert!(closed.command_result.is_none());
    // The next update for the closed lifetime is stale.
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.command_result.is_none());
}

#[test]
fn cross_output_release_retains_follow_and_settles_on_receipt() {
    let (mut engine, ra, rb) = rig();
    // Release the mover onto out-b: membership move plus follow, no
    // pointer placement (existing send semantics).
    let moved_a = rect(900, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-b", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((950, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        released.command_result.is_none(),
        "release rides snapshot+transfer"
    );
    let transfer = released
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("cross release retains a window transfer");
    assert_eq!(transfer.target_output.0, "out-b");
    assert!(transfer.follow, "cross drop follows like the native path");
    assert_eq!(row_of(&released, "win-a").output.0, "out-b");
    // Truthful arrival receipt settles the retained flight: the
    // membership receipt plus the exact target-view echo (cross follow
    // proves the view like a send).
    let target_view = transfer.target_workspace.clone();
    let mut arrived_a = fact("win-a", "out-b", moved_a);
    arrived_a.observed_workspace = Some(target_view.clone());
    let mut settle = report(
        3,
        3,
        vec![arrived_a, fact("win-b", "out-a", rb)],
        "win-a",
        None,
        vec![],
        true,
    );
    settle.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-b".to_owned()),
        view: target_view,
    }];
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")),
        "arrival receipt retires the gesture transfer"
    );
}

#[test]
fn cross_release_to_unplaceable_points_refuses() {
    let (mut engine, ra, rb) = rig();
    // Outside every output refuses with no transfer and no session.
    let moved_a = rect(5000, 0, 40, 90);
    let refused = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-b", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((5050, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.transfers.is_empty());
    assert!(refused.command_result.is_none());
}

#[test]
fn resize_release_commits_share_change() {
    // Shrink win-b (the left leaf) from its right edge: the release
    // derives a single-axis pointer resize, not a move, and commits the
    // share change through the existing resize policy.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-b").allocation;
    // Right edge moved in (140 -> 200 in observed space): resize right/200.
    let shrunk_b = rect(100, 0, 100, 90);
    let mut start = ingress(
        "win-b",
        GestureKind::Resize,
        Some((200, 50)),
        GesturePhase::Updated,
        rb,
    );
    start.modal_move = false;
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            Some(start),
            vec![],
            true,
        ),
    );
    let mut release = ingress(
        "win-b",
        GestureKind::Resize,
        Some((200, 50)),
        GesturePhase::Released,
        rb,
    );
    release.modal_move = false;
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", shrunk_b)],
            "win-b",
            Some(release),
            vec![],
            true,
        ),
    );
    assert!(
        released.command_result.is_none(),
        "resize rides the snapshot"
    );
    assert_ne!(
        row_of(&released, "win-b").allocation,
        before_alloc,
        "resize release commits the share change, got {:?}",
        row_of(&released, "win-b").allocation,
    );
    assert!(released.transfers.is_empty());
}

#[test]
fn modal_move_release_treats_rescale_as_move() {
    // A caption-started loop whose system rescale changed the frame size
    // releases as a move (modal truth), not a resize.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-a").allocation;
    // Slightly rescaled frame at a new origin with a move cursor.
    let rescaled_a = rect(100, 0, 44, 94);
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", rescaled_a),
                fact("win-b", "out-a", rb),
            ],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_ne!(
        row_of(&released, "win-a").allocation,
        before_alloc,
        "modal move commits the drop despite the rescale"
    );
    assert!(
        released.transfers.is_empty(),
        "same-output move retains nothing"
    );
}

#[test]
fn consumed_command_reports_still_carry_gesture_preview() {
    // A fresh report carrying an already-consumed command reconciles the
    // observation without re-executing, and still carries the gesture
    // overlay: gesture ingress is observation, never command replay.
    use crate::world_report::WorldCommand;
    let (mut engine, ra, rb) = rig();
    let preview = GestureIngress {
        modal_move: true,
        ..ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Updated,
            ra,
        )
    };
    let mut with_command = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(preview),
        vec![],
        true,
    );
    with_command.command_seq = Some(7);
    with_command.command = Some(WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "right".to_owned(),
    });
    let first = accepted(&mut engine, &with_command);
    assert!(first.command_result.is_some() || first.command_consumed);
    // Resend the consumed sequence with a fresh observation and a fresh
    // gesture sample: consumed, but the overlay still resolves.
    let preview2 = GestureIngress {
        modal_move: true,
        ..ingress(
            "win-a",
            GestureKind::Move,
            Some((160, 50)),
            GesturePhase::Updated,
            ra,
        )
    };
    let mut resend = report(
        3,
        3,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(preview2),
        vec![],
        true,
    );
    resend.command_seq = Some(7);
    resend.command = Some(WorldCommand::Focus {
        window: WindowId("win-a".to_owned()),
        direction: "right".to_owned(),
    });
    match engine.handle_report(&resend) {
        WorldOutcome::CommandConsumed(reply) => {
            preview_of(&reply);
        }
        other => panic!("expected command-consumed with gesture overlay, got {other:?}"),
    }
}

#[test]
fn incarnation_reset_clears_the_session() {
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    // A new adapter incarnation resets retained world state, including
    // the gesture session: the next update is stale, never a preview
    // over reset state.
    let mut reset = report(
        1,
        3,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Updated,
            ra,
        )),
        vec![],
        true,
    );
    reset.adapter_incarnation = "adapter-2".to_owned();
    let stale = accepted(&mut engine, &reset);
    assert!(stale.command_result.is_none());
}

#[test]
fn engine_sessionless_gesture_never_fabricates() {
    // A gesture for a lifetime the core never homed refuses even under
    // complete inventory: exact-lifetime binding only.
    let (mut engine, ra, rb) = rig();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-c",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                rect(100, 0, 40, 90),
            )),
            vec![],
            true,
        ),
    );
    assert!(reply.command_result.is_none());
    assert!(reply.transfers.is_empty());
}

#[test]
fn missing_ingress_with_close_clears_the_session() {
    // A retained session with no further ingress holds, but an explicit
    // close of the retained lifetime clears exactly: the next update for
    // the closed lifetime is stale with no overlay.
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let cleared = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert!(cleared.command_result.is_none());
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.command_result.is_none());
}

#[test]
fn started_truth_is_immutable_and_zero_gates_preview() {
    // The START cursor is immutable: a later update carrying a different
    // cursor never rebinds it, so a sample back on the down point holds
    // with no overlay even after movement.
    let (mut engine, ra, rb) = rig();
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.modal_move = true;
    start.start_cursor = Some((10, 10));
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    // Same point as START: zero, no overlay.
    let zero = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(zero.command_result.is_none());
    // A moved sample previews.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
    // Back on the down point: zero again, no overlay, session survives.
    let back = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Updated,
                rect(999, 999, 10, 10),
            )),
            vec![],
            true,
        ),
    );
    assert!(back.command_result.is_none());
}

#[test]
fn terminal_replay_with_same_start_frame_refuses() {
    // Release commits, then an exact replay of the terminal (same stable
    // id on a new observation) refuses without a new commit or transfer:
    // at-most-once orders by id, never by window plus frame.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let alloc = row_of(&released, "win-a").allocation;
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "move release records a committed terminal"
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.command_result.is_none());
    assert!(replay.transfers.is_empty());
    assert_eq!(row_of(&replay, "win-a").allocation, alloc);
}

#[test]
fn started_supersedes_a_consumed_terminal() {
    // After a release consumes id 1, a fresh `Started` with id 2 binds
    // anew even with an identical START frame: only the stable id
    // orders sessions, never the frame.
    let (mut engine, ra, rb) = rig();
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        first.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false)),
        "no-change release records uncommitted terminal"
    );
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((150, 50)),
        GesturePhase::Started,
        ra,
        2,
    );
    start.modal_move = true;
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cancelled_without_session_never_binds_or_shields() {
    // A lone cancel for an unbound lifetime binds nothing but still
    // consumes its id: a replay with the same id stays dead, while the
    // next gesture with a fresh id binds and previews.
    let (mut engine, ra, rb) = rig();
    let cancelled = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(cancelled.command_result.is_none());
    assert_eq!(
        cancelled.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false)),
        "lone cancel records an uncommitted terminal"
    );
    // Same id replays dead: no bind, no shield, no overlay.
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.command_result.is_none());
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn cursorless_resize_release_never_commits() {
    // A resize whose release frame changed but whose cursor is missing
    // fails closed: no commit, no transfer.
    let (mut engine, ra, rb) = rig();
    let before = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![],
            true,
        ),
    );
    let before_alloc = row_of(&before, "win-b").allocation;
    let shrunk_b = rect(100, 0, 100, 90);
    let mut release = ingress(
        "win-b",
        GestureKind::Resize,
        None,
        GesturePhase::Released,
        rb,
    );
    release.modal_move = false;
    let released = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", shrunk_b)],
            "win-b",
            Some(release),
            vec![],
            true,
        ),
    );
    assert_eq!(row_of(&released, "win-b").allocation, before_alloc);
    assert!(released.transfers.is_empty());
}

#[test]
fn focused_fullscreen_game_bars_update_and_release() {
    // A focused fullscreen game (different window, staged fullscreen
    // fact) pauses the gesture: updates hold with no overlay and the
    // release refuses with no commit.
    let (mut engine, ra, rb) = rig();
    let mut game = fact("game", "out-a", rect(0, 0, 800, 600));
    game.fullscreen = true;
    game.fit_excluded = true;
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", ra),
                fact("win-b", "out-a", rb),
                game.clone(),
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", ra),
                fact("win-b", "out-a", rb),
                game.clone(),
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(held.command_result.is_none());
    let moved_a = rect(100, 0, 40, 90);
    let refused = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", moved_a),
                fact("win-b", "out-a", rb),
                game,
            ],
            "game",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.command_result.is_none());
    assert!(refused.transfers.is_empty());
}

#[test]
fn terminal_id_zero_never_binds() {
    // A zero id is invalid on the wire and can never bind: the report
    // refuses before any session work.
    let (mut engine, ra, rb) = rig();
    let mut bad = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress_id(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
            0,
        )),
        vec![],
        true,
    );
    let _ = &mut bad;
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected rejection for id 0, got {other:?}"),
    }
    assert!(engine.gesture_terminal.is_none());
}

#[test]
fn terminal_consumes_unbindable_identity() {
    // A terminal for a never-homed lifetime consumes its id even though
    // nothing binds: a retry with the same id stays dead, while a fresh
    // id still binds.
    let (mut engine, ra, rb) = rig();
    let refused = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(refused.command_result.is_none());
    assert!(refused.transfers.is_empty());
    assert_eq!(
        refused.gesture_settled.map(|t| (t.id, t.committed)),
        Some((7, false)),
        "unbindable terminal still records"
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "ghost",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                7,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.command_result.is_none());
    assert!(replay.transfers.is_empty());
    // A fresh id is unaffected by the consumed one.
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                8,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn new_id_supersedes_older_open_session() {
    // A fresh Started with a newer id replaces a dead retained session
    // without waiting for the old terminal: the old id's later terminal
    // cannot erase the new session.
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((10, 10)),
                GesturePhase::Started,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    // Stale terminal for the superseded id: no clear, no commit.
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale.command_result.is_none());
    assert!(stale.transfers.is_empty());
    // The newer session is intact and still previews.
    let previewed = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn stale_cancel_cannot_erase_newer_session() {
    // A cancel carrying an older id than the retained session is ignored:
    // the newer gesture survives it.
    let (mut engine, ra, rb) = rig();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    let ignored = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(ignored.command_result.is_none());
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}

#[test]
fn repeat_updated_and_released_same_id_has_no_replay() {
    // One lifecycle, then the exact same Updated and Released repeat:
    // neither rebinds nor recommits, and the reply carries the terminal
    // exactly once with the commit flag set.
    let (mut engine, ra, rb) = rig();
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.start_cursor = Some((10, 10));
    let started = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    assert!(started.gesture_settled.is_none());
    let moved_a = rect(100, 0, 40, 90);
    let updated = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(updated.gesture_settled.is_none());
    let released = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    let alloc = row_of(&released, "win-a").allocation;
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    assert!(engine.gesture_terminal.is_none());
    // Exact Updated replay: no session, no overlay.
    let stale_update = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(stale_update.command_result.is_none());
    // Exact Released replay: no recommit.
    let rerelease = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(rerelease.command_result.is_none());
    assert!(rerelease.transfers.is_empty());
    assert_eq!(row_of(&rerelease, "win-a").allocation, alloc);
}

#[test]
fn fresh_id_same_frame_fast_end_commits() {
    // Two terminals sharing one START frame but carrying fresh ids are
    // two gestures, not a replay: a coalesced START+END on a new id
    // commits even right after an earlier terminal consumed its own id.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        first.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    // Same START frame, fresh id, fast END onto the other output: not
    // a replay, so the release re-derives and commits the cross-output
    // send with its follow transfer.
    let again = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((900, 50)),
                GesturePhase::Released,
                ra,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        again
            .transfers
            .iter()
            .any(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a")),
        "fresh id commits the cross-output send"
    );
    assert_eq!(
        again.gesture_settled.map(|t| (t.id, t.committed)),
        Some((2, true)),
        "fresh id re-derives and commits, not a replay"
    );
}

#[test]
fn missing_ingress_close_records_terminal() {
    // An explicit close with no further ingress clears the session and
    // records the terminal in the reply, so the adapter observes
    // incorporation from the fresh reply.
    let (mut engine, ra, rb) = rig();
    let updated = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert!(updated.gesture_settled.is_none());
    let closed = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-b", "out-a", rb)],
            "win-b",
            None,
            vec![WindowId("win-a".to_owned())],
            true,
        ),
    );
    assert_eq!(
        closed.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false))
    );
}

#[test]
fn reply_carries_terminal_and_bounds_wire_bytes() {
    // The terminal rides the reply, not a sidechannel: id plus commit
    // flag, canonical bytes carry it, and the structural estimate plus
    // the exact encoding both fit the reply budget.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    let wire = crate::world_report::encode_reply_canonical(&released);
    assert!(
        wire.contains("\"gestureSettled\":{\"id\":\"1\",\"committed\":true}"),
        "canonical bytes carry the terminal: {wire}"
    );
    let estimate = crate::world_report::reply_estimate(&released);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= crate::world_report::WORLD_MAX_REPLY_BYTES);
}

#[test]
fn terminal_does_not_leak_into_next_report() {
    // Per accepted report only: a duplicate repeats the cached terminal,
    // but the next unrelated report publishes `None`.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let terminal_report = report(
        2,
        2,
        vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
        )),
        vec![],
        true,
    );
    let released = accepted(&mut engine, &terminal_report);
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    match engine.handle_report(&terminal_report) {
        WorldOutcome::Duplicate(cached) => assert_eq!(
            cached.gesture_settled.map(|t| (t.id, t.committed)),
            Some((1, true)),
            "duplicate repeats the cached terminal"
        ),
        other => panic!("expected duplicate, got {other:?}"),
    }
    let next = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            None,
            vec![],
            true,
        ),
    );
    assert!(
        next.gesture_settled.is_none(),
        "unrelated report carries no terminal"
    );
}

#[test]
fn terminal_replay_on_new_report_records_nothing() {
    // Same id on a new observation is a consumed replay: no rebind, no
    // recommit, and the fresh reply carries no terminal of its own. The
    // id/observation fences stay safe without repeating the record.
    let (mut engine, ra, rb) = rig();
    let moved_a = rect(100, 0, 40, 90);
    let released = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true))
    );
    let replay = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", moved_a), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                ra,
            )),
            vec![],
            true,
        ),
    );
    assert!(replay.gesture_settled.is_none());
    assert!(replay.command_result.is_none());
    assert!(replay.transfers.is_empty());
}

#[test]
fn rejected_report_consumes_no_gesture_identity() {
    // A refused report (zero id) is transactional: the watermark never
    // advances, so the next valid gesture still binds and previews.
    let (mut engine, ra, rb) = rig();
    let bad = report(
        2,
        2,
        vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
        "win-a",
        Some(ingress_id(
            "win-a",
            GestureKind::Move,
            Some((150, 50)),
            GesturePhase::Released,
            ra,
            0,
        )),
        vec![],
        true,
    );
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { .. } => {}
        other => panic!("expected rejection, got {other:?}"),
    }
    let mut start = ingress_id(
        "win-a",
        GestureKind::Move,
        Some((10, 10)),
        GesturePhase::Started,
        ra,
        1,
    );
    start.start_cursor = Some((10, 10));
    let started = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(start),
            vec![],
            true,
        ),
    );
    assert!(started.gesture_settled.is_none());
    let previewed = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", ra), fact("win-b", "out-a", rb)],
            "win-a",
            Some(ingress_id(
                "win-a",
                GestureKind::Move,
                Some((160, 50)),
                GesturePhase::Updated,
                ra,
                1,
            )),
            vec![],
            true,
        ),
    );
    preview_of(&previewed);
}
