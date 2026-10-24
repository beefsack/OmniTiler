//! Stage-5 uniform pager presentation coverage: report-driven frame
//! retention, native order policy, and the per-reply pager content.
//!
//! The adapter carries the single native stacking snapshot (`pager_order`)
//! plus live frames on the report; the core retains the last live frame per
//! roster lifetime plus the stacking order, and publishes per-screen rings,
//! labels, current/occupied/focused flags, and bottom-first paint lists in
//! the reply. Unknown lifetimes never admit members and never appear
//! publicly before admission.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_presentation::{PagerWindow, PagerWorkspace, pager_workspace_label};
use crate::world_report::{
    PresentationIngress, PresentationPagerFrame, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact,
    WorldReply, WorldReport,
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
        native_workspace: None,
    }
}

fn pager_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    complete: bool,
) -> WorldReport {
    pager_report_ordered(
        request_seq,
        observation_seq,
        windows,
        focused,
        complete,
        &[],
    )
}

fn pager_report_ordered(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    complete: bool,
    order: &[&str],
) -> WorldReport {
    // Live rows mirror the capture mapping for ordinary members (live fact
    // per observed frame); overlay tests override the rows explicitly
    // because the fact frame carries the tile slot while the fresh frame
    // rides the live source.
    let frames: Vec<PresentationPagerFrame> = windows
        .iter()
        .filter_map(|fact| {
            fact.rect.map(|rect| PresentationPagerFrame {
                window: fact.window.clone(),
                live: Some(rect),
                float_frame: None,
                tile_frame: None,
            })
        })
        .collect();
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
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        presentation: PresentationIngress {
            pager_order: order
                .iter()
                .map(|window| WindowId((*window).to_owned()))
                .collect(),
            pager_frames: frames,
            ..PresentationIngress::disabled()
        },
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
        &pager_report_ordered(
            1,
            1,
            members.clone(),
            "win-a",
            true,
            &["win-a", "win-b", "win-c"],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    // A second identical report lets managed focus latch behind the
    // newcomer before any content reads retained focus.
    accepted(
        &mut engine,
        &pager_report_ordered(2, 2, members, "win-a", true, &["win-a", "win-b", "win-c"]),
    );
    let home = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId(view_a),
    };
    (engine, home)
}

fn pager_of(reply: &WorldReply) -> &[crate::world_presentation::PagerScreen] {
    &reply.presentation.pager
}

#[test]
fn pager_labels_rings_current_occupied_and_focus() {
    let (mut engine, home) = admitted_pair();
    assert_eq!(pager_workspace_label(1), "Workspace 1");
    assert_eq!(pager_workspace_label(12), "Workspace 12");
    let live_a = rect(0, 0, 40, 90);
    let live_b = rect(100, 0, 40, 90);
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-b", 800),
            ],
            "win-a",
            true,
            &["win-a", "win-b"],
        ),
    );
    let screens = pager_of(&reply);
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
fn pager_retains_last_live_frame_across_reports() {
    let (mut engine, _) = admitted_pair();
    let live = rect(0, 0, 40, 90);
    // Report 3 refreshes the live frame for win-a.
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-b", 800),
            ],
            "win-a",
            true,
            &["win-a"],
        ),
    );
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("win-a paints")
        .frame;
    assert_eq!(painted, live);
    // Report 4 drops the live fact under incomplete inventory but keeps the
    // native order and forwards the retained slot snapshot (mirroring the
    // adapter's member-rects table): the snapshot still resolves and the
    // order slot survives.
    let mut fourth = pager_report_ordered(
        4,
        4,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-b", 800)],
        "win-a",
        false,
        &["win-a"],
    );
    fourth.presentation.pager_frames = vec![crate::world_report::PresentationPagerFrame {
        window: WindowId("win-a".to_owned()),
        live: None,
        float_frame: Some(live),
        tile_frame: None,
    }];
    let reply = accepted(&mut engine, &fourth);
    // win-a stays homed while incomplete hides nothing by absence.
    assert!(
        engine
            .adapter_homing(&WindowId("win-a".to_owned()))
            .is_some()
    );
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("win-a still paints from its retained frame")
        .frame;
    assert_eq!(painted, live);
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-c".to_owned())
        ]
    );
    // Unknown lifetimes resolve to nothing, never a guess.
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "ghost")
    );
}

#[test]
fn pager_hidden_and_minimized_keep_retained_frame() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &pager_report_ordered(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-a", "win-b"],
        ),
    );
    accepted(
        &mut engine,
        &pager_report_ordered(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-a", "win-b"],
        ),
    );
    let hidden_frame = rect(100, 0, 40, 90);
    // Report 3: win-b hidden. Production capture forwards the hidden
    // snapshot as the live fact (hidden lane), so the member resolves
    // through the live tier; absence alone never infers a close under
    // incomplete inventory.
    let reply = accepted(
        &mut engine,
        &pager_report(
            3,
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            false,
        ),
    );
    assert!(
        engine
            .adapter_homing(&WindowId("win-b".to_owned()))
            .is_some()
    );
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-b")
        .expect("hidden win-b still paints")
        .frame;
    assert_eq!(painted, hidden_frame);
    // Report 4: win-b minimized (explicit minimized row, no live frame)
    // with the adapter's retained slot snapshot forwarded: the snapshot
    // resolves and the order slot survives.
    let mut minimized = fact("win-b", "out-a", 100);
    minimized.minimized = true;
    minimized.rect = None;
    let mut fourth = pager_report(
        4,
        4,
        vec![fact("win-a", "out-a", 0), minimized],
        "win-a",
        true,
    );
    fourth.presentation.pager_frames = vec![crate::world_report::PresentationPagerFrame {
        window: WindowId("win-b".to_owned()),
        live: None,
        float_frame: None,
        tile_frame: Some(hidden_frame),
    }];
    let reply = accepted(&mut engine, &fourth);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-b")
        .expect("minimized win-b still paints")
        .frame;
    assert_eq!(painted, hidden_frame);
    assert_eq!(
        engine.presentation_order,
        vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())]
    );
}

#[test]
fn pager_native_order_retention_reordering_duplicates_pruning() {
    let (mut engine, _) = admitted_pair();
    let members = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ]
    };
    // Report 3: fresh native order [win-a, win-b, win-c].
    accepted(
        &mut engine,
        &pager_report_ordered(3, 3, members(), "win-a", true, &["win-a", "win-b", "win-c"]),
    );
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-c".to_owned())
        ]
    );
    // Duplicate ingress entries keep their first occurrence.
    accepted(
        &mut engine,
        &pager_report_ordered(
            4,
            4,
            members(),
            "win-a",
            true,
            &["win-a", "win-a", "win-b", "win-c"],
        ),
    );
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-c".to_owned())
        ]
    );
    // Reordering follows the fresh ingress sequence exactly.
    accepted(
        &mut engine,
        &pager_report_ordered(5, 5, members(), "win-a", true, &["win-c", "win-b", "win-a"]),
    );
    assert_eq!(
        engine.presentation_order,
        vec![
            WindowId("win-c".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-a".to_owned())
        ]
    );
    // Unknown ingress identities never admit members, but hold an order
    // slot until the next report prunes them once absent.
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(6, 6, members(), "win-a", true, &["ghost", "win-c", "win-a"]),
    );
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
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "ghost"),
        "unadmitted candidates never render"
    );
    // The next report drops the ghost from the order.
    accepted(
        &mut engine,
        &pager_report_ordered(7, 7, members(), "win-a", true, &["win-c", "win-a"]),
    );
    assert!(
        !engine
            .presentation_order
            .contains(&WindowId("ghost".to_owned()))
    );
    // Closing win-c prunes its slot on the next report.
    accepted(
        &mut engine,
        &pager_report_ordered(
            8,
            8,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-b", "win-a"],
        ),
    );
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
    let live = rect(200, 0, 40, 90);
    // Same-report admission with no second observation: the first publish
    // resolves the report's own live frame.
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-new", "out-a", 200),
            ],
            "win-a",
            true,
            &["win-new", "win-a"],
        ),
    );
    assert!(
        engine
            .adapter_homing(&WindowId("win-new".to_owned()))
            .is_some()
    );
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-new")
        .expect("admitted candidate paints on the first publish")
        .frame;
    assert_eq!(painted, live);
    // Pruning: once closed and absent from the next report, the frame and
    // the order slot both drop.
    accepted(
        &mut engine,
        &pager_report_ordered(
            4,
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-a"],
        ),
    );
    assert!(
        !engine
            .presentation_order
            .contains(&WindowId("win-new".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            5,
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-a"],
        ),
    );
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "win-new")
    );
    // Reuse: the same token re-verified in the order before re-admission is
    // retained again but still renders nothing until admitted.
    accepted(
        &mut engine,
        &pager_report_ordered(
            6,
            6,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-new", "win-a"],
        ),
    );
    assert!(
        engine
            .presentation_order
            .contains(&WindowId("win-new".to_owned()))
    );
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            7,
            7,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            true,
            &["win-new", "win-a"],
        ),
    );
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "win-new"),
        "unadmitted candidates never render"
    );
}

#[test]
fn pager_frame_sources_resolve_live_then_float_then_tile() {
    use crate::world_report::PresentationPagerFrame;
    let (mut engine, _) = admitted_pair();
    let live = rect(0, 0, 40, 90);
    let float = rect(1, 1, 42, 92);
    let tile = rect(2, 2, 44, 94);
    let row = |live: Option<Rect>, float_frame: Option<Rect>, tile_frame: Option<Rect>| {
        PresentationPagerFrame {
            window: WindowId("win-a".to_owned()),
            live,
            float_frame,
            tile_frame,
        }
    };
    let members = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ]
    };
    // A retained overlay row carries its tile slot as the fact frame
    // (model grounding), while the fresh native frame rides the live
    // source: the content must paint the live frame, never the slot.
    // Here the report fact is unreadable and only the float/tile slots
    // ride, so the float slot resolves.
    let mut stalled = fact("win-a", "out-a", 0);
    stalled.rect = None;
    let mut third = pager_report_ordered(
        3,
        3,
        {
            let mut members = members();
            members[0] = stalled.clone();
            members
        },
        "win-a",
        true,
        &["win-a", "win-b", "win-c"],
    );
    third.presentation.pager_frames = vec![row(None, Some(float), Some(tile))];
    let reply = accepted(&mut engine, &third);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("float slot resolves the held member")
        .frame;
    assert_eq!(painted, float);
    // A live source beats both retained slots.
    let mut fourth =
        pager_report_ordered(4, 4, members(), "win-a", true, &["win-a", "win-b", "win-c"]);
    fourth.presentation.pager_frames = vec![row(Some(live), Some(float), Some(tile))];
    let reply = accepted(&mut engine, &fourth);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("win-a paints")
        .frame;
    assert_eq!(painted, live);
    // Tile alone resolves when live and float are absent.
    let mut fifth = pager_report_ordered(
        5,
        5,
        {
            let mut members = members();
            members[0] = stalled.clone();
            members
        },
        "win-a",
        true,
        &["win-a", "win-b", "win-c"],
    );
    fifth.presentation.pager_frames = vec![row(None, None, Some(tile))];
    let reply = accepted(&mut engine, &fifth);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("tile slot resolves the held member")
        .frame;
    assert_eq!(painted, tile);
    // Dropped rows resolve nothing again, exactly like the retired
    // wholesale per-pass maps (no accumulation across reports).
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(
            6,
            6,
            {
                let mut members = members();
                members[0] = stalled;
                members
            },
            "win-a",
            true,
            &["win-a", "win-b", "win-c"],
        ),
    );
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "win-a"),
        "dropped rows resolve nothing"
    );
}

#[test]
fn pager_overlay_fresh_frame_beats_tile_slot() {
    use crate::world_report::PresentationPagerFrame;
    let (mut engine, _) = admitted_pair();
    // A retained overlay row carries its tile slot as the fact frame for
    // model grounding, while the fresh native frame rides the live slot:
    // the content paints the fresh frame, never the slot.
    let slot = rect(100, 0, 40, 90);
    let fresh = rect(101, 1, 42, 92);
    let mut overlay_fact = fact("win-a", "out-a", 0);
    overlay_fact.rect = Some(slot);
    let mut report = pager_report_ordered(
        3,
        3,
        vec![
            overlay_fact,
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ],
        "win-a",
        true,
        &["win-a", "win-b", "win-c"],
    );
    report.presentation.pager_frames = vec![PresentationPagerFrame {
        window: WindowId("win-a".to_owned()),
        live: Some(fresh),
        float_frame: None,
        tile_frame: Some(slot),
    }];
    let reply = accepted(&mut engine, &report);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("win-a paints")
        .frame;
    assert_eq!(painted, fresh);
}

#[test]
fn pager_paused_report_publishes_empty_content() {
    let (mut engine, _) = admitted_pair();
    let mut stalled = pager_report_ordered(
        3,
        3,
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ],
        "win-a",
        true,
        &["win-a", "win-b", "win-c"],
    );
    stalled.presentation.paused = true;
    let reply = accepted(&mut engine, &stalled);
    assert!(
        reply.presentation.pager.is_empty(),
        "the game lane skips pager work"
    );
    assert!(matches!(
        reply.presentation.border,
        crate::visual::BorderPresentation {
            visible: false,
            reason: Some(crate::visual::BorderHideReason::Suspended),
            ..
        }
    ));
    assert!(matches!(
        reply.presentation.underlay,
        crate::world_presentation::PresentationUnderlay::Hidden { .. }
    ));
}
