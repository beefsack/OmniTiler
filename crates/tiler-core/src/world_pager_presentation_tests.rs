//! Stage-4c pager presentation coverage: normalized frame retention,
//! native order policy, and the read-only content query.
//!
//! The adapter forwards verified live frames, saved float/tile snapshots,
//! and the single-enumeration native order over opaque lifetimes; the core
//! retains presentation facts, resolves the live > float > tile fallback,
//! and publishes per-screen rings, labels, current/occupied/focused flags,
//! and bottom-first paint lists. Unknown lifetimes never admit members and
//! never appear publicly before admission.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_presentation::{
    PagerPresentationObservation, PagerWindow, PagerWorkspace, pager_workspace_label,
};
use crate::world_report::{
    ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldReply, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: x == 0,
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
    }
}

fn pager_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    complete: bool,
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
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        views: Vec::new(),
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        default_tiled: true,
        inventory_complete: complete,
        gesture_barrier: false,
        gesture: None,
        feedback: vec![],
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn admitted_pair() -> (WorldEngine, DomainKey) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    // A solo window on out-b keeps the second output homed so sends
    // have a live target domain.
    let members = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-b", 800),
    ];
    let first = accepted(
        &mut engine,
        &pager_report(1, 1, members.clone(), "win-a", true),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    // A second identical report lets managed focus latch behind the
    // newcomer before any query reads retained focus.
    accepted(&mut engine, &pager_report(2, 2, members, "win-a", true));
    let home = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(view_a),
    };
    (engine, home)
}

fn pager_obs(
    live: &[(&str, Rect)],
    floats: &[(&str, Rect)],
    tiles: &[(&str, Rect)],
    native: &[&str],
) -> PagerPresentationObservation {
    PagerPresentationObservation {
        live: live
            .iter()
            .map(|(window, frame)| (WindowId((*window).to_owned()), *frame))
            .collect(),
        float_frames: floats
            .iter()
            .map(|(window, frame)| (WindowId((*window).to_owned()), *frame))
            .collect(),
        tile_frames: tiles
            .iter()
            .map(|(window, frame)| (WindowId((*window).to_owned()), *frame))
            .collect(),
        native_order_top_to_bottom: native
            .iter()
            .map(|window| WindowId((*window).to_owned()))
            .collect(),
    }
}

#[test]
fn pager_labels_rings_current_occupied_and_focus() {
    let (mut engine, home) = admitted_pair();
    assert_eq!(pager_workspace_label(1), "Workspace 1");
    assert_eq!(pager_workspace_label(12), "Workspace 12");
    let live_a = rect(0, 0, 40, 90);
    let live_b = rect(100, 0, 40, 90);
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", live_a), ("win-b", live_b)],
        &[],
        &[],
        &["win-a", "win-b"],
    ));
    let screens = engine.pager_screens();
    assert_eq!(screens.len(), 2);
    assert_eq!(screens[0].output, "out-a");
    assert_eq!(screens[1].output, "out-b");
    // Ring order, 1-based ordinals, labels, and the retained view.
    let ring: Vec<String> = engine.logical_ring("out-a");
    assert!(!ring.is_empty());
    let view = engine.logical_view("out-a").expect("view");
    assert_eq!(screens[0].workspaces.len(), ring.len());
    for (index, workspace) in screens[0].workspaces.iter().enumerate() {
        assert_eq!(workspace.id, ring[index]);
        assert_eq!(workspace.ordinal, index as u32 + 1);
        assert_eq!(workspace.label, pager_workspace_label(workspace.ordinal));
        assert_eq!(workspace.current, workspace.id == view);
    }
    // Focused win-a homes to `home`: exactly its workspace contains focus.
    let focused: Vec<&PagerWorkspace> = screens
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .filter(|workspace| workspace.contains_focused)
        .collect();
    assert_eq!(focused.len(), 1);
    assert_eq!(focused[0].id, home.workspace.0);
    // Occupied flags follow membership; paint lists resolve live frames
    // bottom first (native top-to-bottom reversed).
    let homed = screens[0]
        .workspaces
        .iter()
        .find(|workspace| workspace.id == home.workspace.0)
        .expect("homed workspace");
    assert!(homed.occupied);
    assert_eq!(
        homed.windows,
        vec![
            PagerWindow {
                window: WindowId("win-b".to_owned()),
                frame: live_b,
            },
            PagerWindow {
                window: WindowId("win-a".to_owned()),
                frame: live_a,
            },
        ]
    );
}

#[test]
fn pager_frame_fallback_prefers_live_then_float_then_tile() {
    let (mut engine, _) = admitted_pair();
    let live = rect(0, 0, 40, 90);
    let float = rect(100, 0, 40, 90);
    let tile = rect(200, 0, 40, 90);
    // Slot alone resolves.
    engine.observe_pager_presentation(pager_obs(&[], &[], &[("win-a", tile)], &[]));
    assert_eq!(
        engine.pager_frame(&WindowId("win-a".to_owned())),
        Some(tile)
    );
    // Float beats the slot.
    engine.observe_pager_presentation(pager_obs(&[], &[("win-a", float)], &[("win-a", tile)], &[]));
    assert_eq!(
        engine.pager_frame(&WindowId("win-a".to_owned())),
        Some(float)
    );
    // Live identity-verified frame beats both.
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", live)],
        &[("win-a", float)],
        &[("win-a", tile)],
        &["win-a"],
    ));
    assert_eq!(
        engine.pager_frame(&WindowId("win-a".to_owned())),
        Some(live)
    );
    // Unknown lifetimes resolve to nothing, never a guess.
    assert_eq!(engine.pager_frame(&WindowId("ghost".to_owned())), None);
}

#[test]
fn pager_hidden_and_minimized_resolve_through_snapshots() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &pager_report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
        ),
    );
    accepted(
        &mut engine,
        &pager_report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
        ),
    );
    let hidden_tile = rect(100, 0, 40, 90);
    // Tick 1: both verified live, native top-to-bottom [win-a, win-b].
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", rect(0, 0, 40, 90)), ("win-b", hidden_tile)],
        &[],
        &[("win-b", hidden_tile)],
        &["win-a", "win-b"],
    ));
    // Tick 2: win-b hidden (absent under incomplete inventory, so never
    // inferred closed) and absent from live: the tile snapshot resolves.
    accepted(
        &mut engine,
        &pager_report(3, 3, vec![fact("win-a", "out-a", 0)], "win-a", false),
    );
    assert!(
        engine
            .adapter_homing(&WindowId("win-b".to_owned()))
            .is_some()
    );
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", rect(0, 0, 40, 90))],
        &[],
        &[("win-b", hidden_tile)],
        &["win-a"],
    ));
    assert_eq!(
        engine.pager_frame(&WindowId("win-b".to_owned())),
        Some(hidden_tile)
    );
    // Tick 3: win-b minimized (explicit minimized row, no live frame):
    // the snapshot still resolves and the order slot survives.
    let mut minimized = fact("win-b", "out-a", 100);
    minimized.minimized = true;
    minimized.rect = None;
    accepted(
        &mut engine,
        &pager_report(
            4,
            4,
            vec![fact("win-a", "out-a", 0), minimized],
            "win-a",
            true,
        ),
    );
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", rect(0, 0, 40, 90))],
        &[],
        &[("win-b", hidden_tile)],
        &["win-a"],
    ));
    assert_eq!(
        engine.pager_frame(&WindowId("win-b".to_owned())),
        Some(hidden_tile)
    );
    assert_eq!(
        engine.presentation_order,
        vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())]
    );
}

#[test]
fn pager_native_order_retention_reordering_duplicates_pruning() {
    let (mut engine, _) = admitted_pair();
    let frame = |x: i32| rect(x, 0, 40, 90);
    // Tick 1: fresh native order [win-a, win-b, win-c].
    engine.observe_pager_presentation(pager_obs(
        &[
            ("win-a", frame(0)),
            ("win-b", frame(100)),
            ("win-c", frame(800)),
        ],
        &[],
        &[],
        &["win-a", "win-b", "win-c"],
    ));
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-c".to_owned())
        ]
    );
    // Duplicate native entries keep their first occurrence.
    engine.observe_pager_presentation(pager_obs(
        &[
            ("win-a", frame(0)),
            ("win-b", frame(100)),
            ("win-c", frame(800)),
        ],
        &[],
        &[],
        &["win-a", "win-a", "win-b", "win-c"],
    ));
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-c".to_owned())
        ]
    );
    // Reordering follows the fresh native sequence exactly.
    engine.observe_pager_presentation(pager_obs(
        &[
            ("win-a", frame(0)),
            ("win-b", frame(100)),
            ("win-c", frame(800)),
        ],
        &[],
        &[],
        &["win-c", "win-b", "win-a"],
    ));
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-c".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-a".to_owned())
        ]
    );
    // Unknown native identities never admit members, but hold an order
    // slot until the next observation prunes them once absent.
    engine.observe_pager_presentation(pager_obs(
        &[
            ("win-a", frame(0)),
            ("win-b", frame(100)),
            ("win-c", frame(800)),
        ],
        &[],
        &[],
        &["ghost", "win-c", "win-a"],
    ));
    assert!(
        engine
            .presentation_order
            .contains(&WindowId("ghost".to_owned()))
    );
    assert!(
        engine
            .adapter_homing(&WindowId("ghost".to_owned()))
            .is_none()
    );
    let screens = engine.pager_screens();
    assert!(
        screens
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "ghost"),
        "unadmitted candidates never render"
    );
    // Candidate frames are retained but never public before admission.
    assert_eq!(engine.pager_frame(&WindowId("ghost".to_owned())), None);
    engine.observe_pager_presentation(pager_obs(
        &[
            ("win-a", frame(0)),
            ("win-b", frame(100)),
            ("win-c", frame(800)),
        ],
        &[],
        &[],
        &["win-c", "win-a"],
    ));
    assert!(
        !engine
            .presentation_order
            .contains(&WindowId("ghost".to_owned()))
    );
    // Closing win-c prunes its slot on the next observation.
    accepted(
        &mut engine,
        &pager_report(
            3,
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
        ),
    );
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", frame(0)), ("win-b", frame(100))],
        &[],
        &[],
        &["win-b", "win-a"],
    ));
    assert_eq!(
        engine.presentation_order,
        vec![WindowId("win-b".to_owned()), WindowId("win-a".to_owned())]
    );
    assert!(
        !engine
            .presentation_order
            .contains(&WindowId("win-c".to_owned()))
    );
}

#[test]
fn pager_candidate_frame_survives_same_report_admission() {
    let (mut engine, _) = admitted_pair();
    let live = rect(400, 0, 40, 90);
    let float = rect(401, 0, 40, 90);
    let tile = rect(402, 0, 40, 90);
    // Pre-observation verifies a not-yet-admitted candidate with all three
    // frame lanes populated: the slot is retained but stays non-public.
    engine.observe_pager_presentation(pager_obs(
        &[("win-new", live)],
        &[("win-new", float)],
        &[("win-new", tile)],
        &["win-new", "win-a"],
    ));
    assert!(
        engine
            .adapter_homing(&WindowId("win-new".to_owned()))
            .is_none()
    );
    assert_eq!(engine.pager_frame(&WindowId("win-new".to_owned())), None);
    assert!(
        engine
            .pager_screens()
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "win-new"),
        "unadmitted candidates never render"
    );
    // Same-report admission with no second presentation observation: the
    // first query resolves the retained candidate frame, live first.
    accepted(
        &mut engine,
        &pager_report(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-new", "out-a", 200),
            ],
            "win-a",
            true,
        ),
    );
    assert!(
        engine
            .adapter_homing(&WindowId("win-new".to_owned()))
            .is_some()
    );
    assert_eq!(
        engine.pager_frame(&WindowId("win-new".to_owned())),
        Some(live)
    );
    let painted = engine
        .pager_screens()
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-new")
        .expect("admitted candidate paints on the first query")
        .frame;
    assert_eq!(painted, live);
    // Pruning: once closed and absent from the next observation, the frame
    // and the order slot both drop.
    accepted(
        &mut engine,
        &pager_report(
            4,
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
        ),
    );
    engine.observe_pager_presentation(pager_obs(
        &[("win-a", rect(0, 0, 40, 90))],
        &[],
        &[],
        &["win-a"],
    ));
    assert_eq!(engine.pager_frame(&WindowId("win-new".to_owned())), None);
    assert!(
        !engine
            .presentation_order
            .contains(&WindowId("win-new".to_owned()))
    );
    // Reuse: the same token re-verified before re-admission is retained
    // again but still renders nothing until admitted.
    engine.observe_pager_presentation(pager_obs(
        &[("win-new", live)],
        &[],
        &[],
        &["win-new", "win-a"],
    ));
    assert!(
        engine
            .presentation_order
            .contains(&WindowId("win-new".to_owned()))
    );
    assert_eq!(engine.pager_frame(&WindowId("win-new".to_owned())), None);
}

#[test]
fn pager_observation_never_mutates_mapping_or_membership() {
    let (mut engine, _) = admitted_pair();
    let revision = engine.state_revision();
    let outputs = engine.logical_outputs();
    let rings: Vec<(String, Vec<String>)> = outputs
        .iter()
        .map(|output| (output.clone(), engine.logical_ring(output)))
        .collect();
    let views: Vec<(String, Option<String>)> = outputs
        .iter()
        .map(|output| (output.clone(), engine.logical_view(output)))
        .collect();
    let members = engine.logical_members_all();
    // Foreign frames, unknown order, and unadmitted candidates: the
    // observation retains presentation facts only.
    engine.observe_pager_presentation(pager_obs(
        &[("ghost", rect(0, 0, 10, 10))],
        &[("ghost", rect(1, 1, 10, 10))],
        &[("ghost", rect(2, 2, 10, 10))],
        &["ghost", "win-a"],
    ));
    assert_eq!(engine.state_revision(), revision);
    assert_eq!(engine.logical_outputs(), outputs);
    for (output, ring) in &rings {
        assert_eq!(engine.logical_ring(output), *ring);
    }
    for (output, view) in &views {
        assert_eq!(engine.logical_view(output), *view);
    }
    assert_eq!(engine.logical_members_all(), members);
    assert!(
        engine
            .adapter_homing(&WindowId("ghost".to_owned()))
            .is_none()
    );
}
