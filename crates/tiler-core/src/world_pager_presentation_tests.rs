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
    HeldWindowRow, PresentationIngressV4, PresentationPagerFrameV4, PresentationRowV4,
    RowDiagnosticKind, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReplyV4,
    WorldReportV4,
};
use crate::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
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

fn pager_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    complete: bool,
) -> WorldReportV4 {
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
) -> WorldReportV4 {
    // Live rows mirror the capture mapping for ordinary members (live fact
    // per observed frame). v4 pager rows are exactly {win, live}
    // (world-contract.md:1211-1219), so the builder forwards only the
    // live source.
    let frames: Vec<PresentationPagerFrameV4> = windows
        .iter()
        .filter_map(|fact| {
            fact.rect.map(|rect| PresentationPagerFrameV4 {
                window: fact.window.clone(),
                live: Some(rect),
            })
        })
        .collect();
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: vec![],
        native: vec![],
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: complete,
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
        presentation: PresentationRowV4::Valid(PresentationIngressV4 {
            pager_order: order
                .iter()
                .map(|window| WindowId((*window).to_owned()))
                .collect(),
            pager_frames: frames,
            ..PresentationIngressV4::default()
        }),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn set_pager_frames(report: &mut WorldReportV4, frames: Vec<PresentationPagerFrameV4>) {
    match &mut report.presentation {
        PresentationRowV4::Valid(ingress) => ingress.pager_frames = frames,
        PresentationRowV4::Held(_) => panic!("expected valid presentation"),
    }
}

fn pager_row(window: &str, live: Option<Rect>) -> PresentationPagerFrameV4 {
    PresentationPagerFrameV4 {
        window: WindowId(window.to_owned()),
        live,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn admitted_pair() -> (WorldEngine, DomainKey) {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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

fn pager_of(reply: &WorldReplyV4) -> &[crate::world_presentation::PagerScreen] {
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
    // native order. Legacy forwarded the adapter's retained slot snapshot
    // in the float slot here; v4 pager rows carry only {win, live}, so the
    // port sends a live-empty row and the retained frame must still
    // resolve from the engine's per-lifetime retention.
    let mut fourth = pager_report_ordered(
        4,
        4,
        vec![fact("win-b", "out-a", 100), fact("win-c", "out-b", 800)],
        "win-a",
        false,
        &["win-a"],
    );
    set_pager_frames(&mut fourth, vec![pager_row("win-a", None)]);
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
        engine.presentation_order_v4,
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    // Report 4: win-b minimized (explicit minimized row, no live frame).
    // Legacy forwarded the adapter's retained slot snapshot in the tile
    // slot here; v4 pager rows carry only {win, live}, so the port sends
    // a live-empty row and the retained frame must still resolve from
    // the engine's per-lifetime retention.
    let mut minimized = fact("win-b", "out-a", 100);
    minimized.minimized = Some(true);
    minimized.rect = None;
    let mut fourth = pager_report(
        4,
        4,
        vec![fact("win-a", "out-a", 0), minimized],
        "win-a",
        true,
    );
    set_pager_frames(&mut fourth, vec![pager_row("win-b", None)]);
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
        engine.presentation_order_v4,
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
        engine.presentation_order_v4,
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
        engine.presentation_order_v4,
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
        engine.presentation_order_v4,
        vec![
            WindowId("win-c".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-a".to_owned())
        ]
    );
    // Unknown ingress identities never admit members and never render.
    // Complete inventory with no fact or held row prunes immediately:
    // pager order alone admits nothing.
    let reply = accepted(
        &mut engine,
        &pager_report_ordered(6, 6, members(), "win-a", true, &["ghost", "win-c", "win-a"]),
    );
    assert!(
        engine
            .adapter_homing(&WindowId("ghost".to_owned()))
            .is_none()
    );
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("ghost".to_owned())),
        "complete inventory prunes pager-order-only identities immediately"
    );
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "ghost"),
        "unadmitted candidates never render"
    );
    // The next report keeps the ghost pruned from the order.
    accepted(
        &mut engine,
        &pager_report_ordered(7, 7, members(), "win-a", true, &["win-c", "win-a"]),
    );
    assert!(
        !engine
            .presentation_order_v4
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
        engine.presentation_order_v4,
        vec![WindowId("win-b".to_owned()), WindowId("win-a".to_owned())]
    );
    assert!(
        !engine
            .presentation_order_v4
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
            .presentation_order_v4
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
    // Reuse: the same token re-verified in the order before re-admission
    // still renders nothing until admitted. Complete inventory with no
    // fact or held row prunes immediately: pager order alone admits
    // nothing.
    let sixth = accepted(
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
            .adapter_homing(&WindowId("win-new".to_owned()))
            .is_none()
    );
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("win-new".to_owned())),
        "complete inventory prunes the re-verified token immediately"
    );
    assert!(
        pager_of(&sixth)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .flat_map(|workspace| workspace.windows.iter())
            .all(|window| window.window.0 != "win-new"),
        "unadmitted candidates never render"
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
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("win-new".to_owned())),
        "the re-verified token stays pruned while unadmitted"
    );
}

#[test]
fn pager_held_without_fact_retains_order_slot() {
    let (mut engine, _) = admitted_pair();
    // Complete inventory, win-c absent from facts but carried as a held
    // row and named in pager order: held identities retain absent
    // identities, so the slot and live frame survive.
    let mut report = pager_report_ordered(
        3,
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        "win-a",
        true,
        &["win-c", "win-a"],
    );
    report.held_windows = vec![HeldWindowRow {
        window: WindowId("win-c".to_owned()),
        ordinal: 0,
        output: None,
        fields: Vec::new(),
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    }];
    accepted(&mut engine, &report);
    assert!(
        engine
            .presentation_order_v4
            .contains(&WindowId("win-c".to_owned())),
        "held identity with no fact retains its order slot"
    );
    assert!(
        engine
            .presentation_live_v4
            .contains_key(&WindowId("win-c".to_owned())),
        "held identity with no fact retains its live frame"
    );
}

#[test]
fn pager_frame_sources_resolve_live_then_float_then_tile() {
    let (mut engine, _) = admitted_pair();
    let live = rect(0, 0, 40, 90);
    // v4 pager rows are exactly {win, live} (world-contract.md:1211-1219:
    // keep live source, remove retained float/tile), so no slot-priority
    // rule applies on the v4 lane; the adapter forwards any retained
    // snapshot as the single live source.
    let members = || {
        vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-b", 800),
        ]
    };
    // A retained overlay row carries its slot as the fact frame (model
    // grounding), while the fresh native frame rides the live source:
    // the content must paint the live frame, never the slot. Here the
    // report fact is unreadable and only the live source rides, so the
    // live source resolves for the held member.
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
    set_pager_frames(&mut third, vec![pager_row("win-a", Some(live))]);
    let reply = accepted(&mut engine, &third);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("live source resolves the held member")
        .frame;
    assert_eq!(painted, live);
    // v4 retains the last-known live frame until the lifetime departs
    // (world-contract.md:1225; retain_pager_presentation_v4): a follow-up
    // report with no fresh row for win-a still paints the retained frame.
    let mut fourth = pager_report_ordered(
        4,
        4,
        {
            let mut members = members();
            members[0] = stalled;
            members
        },
        "win-a",
        true,
        &["win-a", "win-b", "win-c"],
    );
    set_pager_frames(&mut fourth, vec![pager_row("win-a", None)]);
    let reply = accepted(&mut engine, &fourth);
    let painted = pager_of(&reply)
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .flat_map(|workspace| workspace.windows.iter())
        .find(|window| window.window.0 == "win-a")
        .expect("retained live frame survives a missing row")
        .frame;
    assert_eq!(painted, live);
}

#[test]
fn pager_overlay_fresh_frame_beats_tile_slot() {
    let (mut engine, _) = admitted_pair();
    // A retained overlay row carries its slot as the fact frame for
    // model grounding, while the fresh native frame rides the live slot:
    // the content paints the fresh frame, never the slot. Legacy also
    // carried the slot in the row's tile frame; v4 rows are {win, live}
    // only, so the port drops that redundant slot.
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
    set_pager_frames(&mut report, vec![pager_row("win-a", Some(fresh))]);
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

fn trailing_plan_ids(reply: &WorldReplyV4, output: &str) -> Vec<(String, bool)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| (row.id.clone(), row.trailing))
        .collect()
}

fn trailing_pager_ids(reply: &WorldReplyV4, output: &str) -> Vec<(String, bool)> {
    pager_of(reply)
        .iter()
        .find(|screen| screen.output == output)
        .map(|screen| {
            screen
                .workspaces
                .iter()
                .map(|workspace| (workspace.id.clone(), workspace.trailing))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn trailing_marks_only_the_final_spare_in_both_streams() {
    let (mut engine, _) = admitted_pair();
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
            &["win-a", "win-b", "win-c"],
        ),
    );
    for output in engine.logical_outputs() {
        let ring = engine.logical_ring(&output);
        assert!(!ring.is_empty(), "ring publishes for {output}");
        let plan = trailing_plan_ids(&reply, &output);
        assert_eq!(
            plan.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            ring,
            "plan rows follow the final ring for {output}"
        );
        let pager = trailing_pager_ids(&reply, &output);
        assert_eq!(
            pager.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            ring,
            "pager rows follow the final ring for {output}"
        );
        for (index, (_, marked)) in plan.iter().enumerate() {
            assert_eq!(
                *marked,
                index + 1 == plan.len(),
                "only the final plan row marks trailing for {output}"
            );
        }
        for (index, (_, marked)) in pager.iter().enumerate() {
            assert_eq!(
                *marked,
                index + 1 == pager.len(),
                "only the final pager row marks trailing for {output}"
            );
        }
        assert_eq!(
            plan.last().map(|(id, _)| id),
            pager.last().map(|(id, _)| id),
            "both streams agree on the spare for {output}"
        );
    }
    // The typed flags carry the key exactly when marked (one plan plus
    // one pager mark per output); the v4 lane has no full-reply
    // canonical encoder in core (wire encoding lives in the protocol
    // crate), so the port counts typed marks and checks the structural
    // estimate against the v4 exact byte size instead of wire bytes.
    let marked: usize = engine
        .logical_outputs()
        .iter()
        .map(|output| {
            trailing_plan_ids(&reply, output)
                .iter()
                .filter(|(_, m)| *m)
                .count()
                + trailing_pager_ids(&reply, output)
                    .iter()
                    .filter(|(_, m)| *m)
                    .count()
        })
        .sum();
    assert_eq!(
        marked,
        2 * engine.logical_outputs().len(),
        "one plan plus one pager mark per output"
    );
    let estimate = reply_estimate_v4(&reply);
    let exact = reply_exact_bytes_v4(&reply);
    assert!(
        estimate >= exact,
        "estimate {estimate} bounds exact {exact}"
    );
}

#[test]
fn trailing_fails_closed_on_incomplete_inventory() {
    let (mut engine, _) = admitted_pair();
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
            false,
            &["win-a", "win-b", "win-c"],
        ),
    );
    assert!(
        reply.workspaces.iter().all(|row| !row.trailing),
        "incomplete inventory marks no plan row"
    );
    assert!(
        pager_of(&reply)
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .all(|workspace| !workspace.trailing),
        "incomplete inventory marks no pager row"
    );
    // No full-reply canonical encoder exists on the v4 lane in core, so
    // the legacy wire-omission check is covered by the typed
    // all-unmarked assertions above (the protocol crate owns omission).
}

#[test]
fn trailing_fails_closed_per_incomplete_screen() {
    let (mut engine, _) = admitted_pair();
    let mut report = pager_report_ordered(
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
    report
        .screens
        .iter_mut()
        .find(|screen| screen.output.0 == "out-b")
        .expect("out-b screen")
        .complete = false;
    let reply = accepted(&mut engine, &report);
    let complete = trailing_plan_ids(&reply, "out-a");
    assert_eq!(
        complete.iter().filter(|(_, marked)| *marked).count(),
        1,
        "the complete screen still marks its final spare"
    );
    assert!(
        trailing_plan_ids(&reply, "out-b")
            .iter()
            .all(|(_, marked)| !marked),
        "the incomplete screen marks no plan row"
    );
    assert!(
        trailing_pager_ids(&reply, "out-b")
            .iter()
            .all(|(_, marked)| !marked),
        "the incomplete screen marks no pager row"
    );
}
