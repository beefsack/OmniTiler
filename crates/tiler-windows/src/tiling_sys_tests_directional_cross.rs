// Windows multi-output directional foundation (REQ-OUT-01..03, REQ-MOV-08)
// through a real `TileLoop` against the real retained Engine (no HWNDs,
// hooks, or live effects): synthetic two-output FULL/work topologies with
// negative coords and differing gaps, occupied/empty source/target,
// local-first and sole-root crossing, destination current-view history,
// and fence/arrival verification. Native setters stay user-owned live
// acceptance; these pin the portable pair construction, Engine planning,
// and project membership/focus verification.
use super::rmax03_adapter_tests::test_state;
use super::{MonitorArea, workspace_domain_for};
use tiler_core::boundary::{CoreCommand, CoreReply};
use tiler_core::directional::{CrossOutputTarget, Direction, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

fn area(device: &str, work: Rect, full: Rect) -> MonitorArea {
    MonitorArea {
        device: device.to_owned(),
        work,
        full,
    }
}

fn primary_pair() -> Vec<MonitorArea> {
    vec![
        area(
            "primary",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "right",
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ]
}

fn seed(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    bounds: Rect,
    members: &[&str],
    focused: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    let domain = crate::workspace_owner::workspace_domain(output, workspace, bounds, 8);
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = members
        .iter()
        .map(|token| {
            (
                WindowId((*token).to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            )
        })
        .collect();
    let revision = state
        .engine
        .session(&domain.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("directional-cross-seed").expect("correlation");
    let fp = crate::tiling::fingerprint(
        &members
            .iter()
            .map(|t| ((*t).to_owned(), bounds))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {reply:?}"
    );
    domain
}

#[allow(clippy::too_many_arguments)]
fn pair_event(
    state: &super::TileLoop,
    source: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    target: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    source_members: &[&str],
    target_members: &[&str],
    mover: &str,
    direction: Direction,
    command: CoreCommand,
) -> tiler_core::boundary::CoreEvent {
    let mut windows = Vec::new();
    for token in source_members {
        windows.push(tiler_core::seed::EngineWindow {
            window: WindowId((*token).to_owned()),
            output: source.1.output.clone(),
            workspace: source.1.workspace.clone(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        });
    }
    for token in target_members {
        windows.push(tiler_core::seed::EngineWindow {
            window: WindowId((*token).to_owned()),
            output: target.1.output.clone(),
            workspace: target.1.workspace.clone(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        });
    }
    let correlation = CorrelationId::parse("directional-cross").expect("correlation");
    let ((source_domain, source_key), (target_domain, target_key)) =
        crate::workspace_owner::directional_pair_with_adjacency(
            (source.0.clone(), source.1.clone()),
            (target.0.clone(), target.1.clone()),
            direction,
        );
    tiler_core::boundary::CoreEvent {
        owner: state.owner.clone(),
        generation: state.generation.clone(),
        correlation,
        revision: 0,
        fingerprint: 11,
        domain: source_domain.clone(),
        domain_key: source_key.clone(),
        outer_gap: 8,
        focused_window: WindowId(mover.to_owned()),
        windows,
        directional: Some(vec![
            (source_domain, source_key),
            (target_domain, target_key),
        ]),
        directional_target_outer_gap: Some(8),
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command,
    }
}

#[test]
fn sole_root_crosses_right_into_occupied_target_with_follow() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    // Distinct identities even for same-index workspaces.
    assert_ne!((&"mon-a".to_owned(), &ws_a), (&"mon-b".to_owned(), &ws_b));
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let source = seed(&mut state, "mon-a", &ws_a, bounds, &["mover"], "mover");
    let target = seed(&mut state, "mon-b", &ws_b, bounds, &["t1", "t2"], "t1");
    let event = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &["t1", "t2"],
        "mover",
        Direction::Right,
        CoreCommand::Move {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match state.engine.handle(&event) {
        CoreReply::MoveDirectional(plan) => {
            let cross = plan.cross.expect("sole root must cross");
            assert_eq!(cross.target, CrossOutputTarget::Occupied);
            assert_eq!(cross.target_output.0, "mon-b");
            // Follow focus names the mover on the target domain.
            assert_eq!(plan.focus_domain.output.0, "mon-b");
            assert!(!plan.geometry.is_empty(), "cross carries placement");
        }
        other => panic!("sole root must cross, got {other:?}"),
    }
}

#[test]
fn empty_target_supports_move_but_refuses_focus() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let source = seed(&mut state, "mon-a", &ws_a, bounds, &["mover"], "mover");
    // Empty target: no session seeded; the pair still carries its domain.
    let target = crate::workspace_owner::workspace_domain("mon-b", &ws_b, bounds, 8);
    let move_event = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &[],
        "mover",
        Direction::Right,
        CoreCommand::Move {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match state.engine.handle(&move_event) {
        CoreReply::MoveDirectional(plan) => {
            let cross = plan.cross.expect("empty target supports move");
            assert_eq!(cross.target, CrossOutputTarget::Empty);
        }
        other => panic!("empty move must plan, got {other:?}"),
    }
    // Fresh state for the focus leg: the move above committed the mover
    // away, so re-seed before asserting the focus refusal.
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let source = seed(&mut state, "mon-a", &ws_a, bounds, &["mover"], "mover");
    let target = crate::workspace_owner::workspace_domain("mon-b", &ws_b, bounds, 8);
    let focus_event = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &[],
        "mover",
        Direction::Right,
        CoreCommand::Focus {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            float_subject: false,
        },
    );
    match state.engine.handle(&focus_event) {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        CoreReply::SnapshotInvalid { detail, .. } => {
            panic!("empty focus must refuse unchanged, got snapshot-invalid {detail}")
        }
        other => panic!("empty focus must refuse, got {other:?}"),
    }
}

#[test]
fn local_restructure_wins_first_over_cross() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    // Vertical stack (selected default): the mover has a local Down
    // neighbor, so Down restructures locally instead of crossing even
    // with an adjacent target present.
    let source = seed(
        &mut state,
        "mon-a",
        &ws_a,
        bounds,
        &["mover", "peer"],
        "mover",
    );
    let target = seed(&mut state, "mon-b", &ws_b, bounds, &["t1"], "t1");
    let event = pair_event(
        &state,
        &source,
        &target,
        &["mover", "peer"],
        &["t1"],
        "mover",
        Direction::Down,
        CoreCommand::Move {
            window: "mover".to_owned(),
            direction: "down".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    match state.engine.handle(&event) {
        CoreReply::MoveDirectional(plan) => {
            assert!(
                plan.cross.is_none(),
                "local restructure must win first, got {plan:?}"
            );
        }
        other => panic!("local move must plan, got {other:?}"),
    }
}

#[test]
fn horizontal_focus_crosses_while_vertical_stays_local() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let source = seed(&mut state, "mon-a", &ws_a, bounds, &["mover"], "mover");
    let target = seed(&mut state, "mon-b", &ws_b, bounds, &["t1"], "t1");
    let horizontal = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &["t1"],
        "mover",
        Direction::Right,
        CoreCommand::Focus {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            float_subject: false,
        },
    );
    match state.engine.handle(&horizontal) {
        CoreReply::FocusDirectional(plan) => {
            assert!(
                plan.cross_operation.is_some(),
                "horizontal focus must cross"
            );
            assert_eq!(plan.to_window.0, "t1");
            // No geometry or membership writes for focus: the plan
            // carries the unmodified domains' geometry only.
            assert_eq!(plan.focus_domain.output.0, "mon-b");
        }
        other => panic!("horizontal focus must cross, got {other:?}"),
    }
    // Vertical never builds a pair in the adapter; the single-domain
    // local path reports the exhausted edge as unchanged. Build the
    // single-domain event with source windows only (pair windows would
    // correctly mismatch as cross-domain).
    let correlation = CorrelationId::parse("directional-cross").expect("correlation");
    let local = tiler_core::boundary::CoreEvent {
        owner: state.owner.clone(),
        generation: state.generation.clone(),
        correlation,
        revision: 0,
        fingerprint: 13,
        domain: source.0.clone(),
        domain_key: source.1.clone(),
        outer_gap: 8,
        focused_window: WindowId("mover".to_owned()),
        windows: vec![tiler_core::seed::EngineWindow {
            window: WindowId("mover".to_owned()),
            output: source.1.output.clone(),
            workspace: source.1.workspace.clone(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        }],
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::Focus {
            window: "mover".to_owned(),
            direction: "up".to_owned(),
            cross_output_transfer: false,
            float_subject: false,
        },
    };
    match state.engine.handle(&local) {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        other => panic!("vertical focus must stay local, got {other:?}"),
    }
}

#[test]
fn cross_focus_syncs_stale_engine_target_to_project_mru() {
    // Stale Engine remembered focus (t1, from seeding) versus fresh
    // project MRU (t2, from an external native focus move): the raw
    // pair request plans toward the stale target, while the production
    // keyboard route synchronizes remembered focus first and actuates
    // the fresh MRU with no view switch and no geometry writes.
    use super::{
        HintCx, ObservedWindow, RetainedRow, clear_pending_seams, inject_pending_elevated,
        inject_pending_focus, inject_pending_suspend, inject_pending_tag, run_directional_cross,
    };
    use crate::model::ProcessIdentity;
    use crate::snapkey::{QueuedIntent, SnapEdge, SnapOp, SnapOrigin};
    use crate::workspace::{MemberLoc, WindowKey};
    use std::collections::BTreeSet;
    clear_pending_seams();
    let mut state = test_state();
    let areas = primary_pair();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("primary");
    state.workspaces.ensure_output("right");
    let ws_a = state.workspaces.active_id("primary").expect("active");
    let ws_b = state.workspaces.active_id("right").expect("active");
    state.workspaces.set_tiled("primary", &ws_a, true);
    state.workspaces.set_tiled("right", &ws_b, true);
    // Seed bounds match the area-derived inset domains exactly so the
    // pair assembles on retained state.
    let src_bounds = workspace_domain_for("primary", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let dst_bounds = workspace_domain_for("right", &ws_b, &areas, 8, 8)
        .expect("dst domain")
        .0
        .bounds;
    fn key(hwnd: u64, pid: u32) -> WindowKey {
        WindowKey {
            hwnd,
            pid,
            creation: format!("c{hwnd:016x}"),
        }
    }
    let me = ProcessIdentity {
        pid: 5001,
        process_creation: "creation-me".to_owned(),
        user_sid: "S-1-focus".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    };
    let mover = key(0x4101, 7101);
    let t1 = key(0x4102, 7102);
    let t2 = key(0x4103, 7103);
    for (k, token, pid) in [
        (&mover, "mover", 7101u32),
        (&t1, "t1", 7102),
        (&t2, "t2", 7103),
    ] {
        state.member_tokens.insert(k.clone(), token.to_owned());
        state.member_identity.insert(
            k.clone(),
            ProcessIdentity {
                pid,
                process_creation: k.creation.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state
            .member_tags
            .insert(k.clone(), format!("tag-{:x}", k.hwnd));
        inject_pending_tag(k.hwnd, Some(format!("tag-{:x}", k.hwnd)));
    }
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "primary", &ws_a, false)
    );
    assert!(state.workspaces.assign(t1.clone(), "right", &ws_b, false));
    assert!(state.workspaces.assign(t2.clone(), "right", &ws_b, false));
    // Engine remembered focus lands on t1 (seed order); the project MRU
    // observes an external native focus move onto t2 instead.
    let source = seed(
        &mut state,
        "primary",
        &ws_a,
        src_bounds,
        &["mover"],
        "mover",
    );
    let target = seed(&mut state, "right", &ws_b, dst_bounds, &["t1", "t2"], "t1");
    state.workspaces.note_foreground(&t2);
    assert_eq!(
        state.workspaces.focus_target(
            "right",
            &ws_b,
            &[mover.clone(), t1.clone(), t2.clone()]
                .into_iter()
                .collect::<BTreeSet<_>>()
        ),
        Some(t2.clone()),
        "project MRU is t2"
    );
    let stale = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &["t1", "t2"],
        "mover",
        Direction::Right,
        CoreCommand::Focus {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            float_subject: false,
        },
    );
    match state.engine.handle(&stale) {
        CoreReply::FocusDirectional(plan) => {
            assert_eq!(
                plan.to_window.0, "t1",
                "unsynchronized plan aims at the stale target"
            );
        }
        other => panic!("stale focus must plan, got {other:?}"),
    }
    // The hook refuses unknown sessions and members without mutation.
    let nowhere = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId("nowhere".to_owned()),
        workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
    };
    assert!(
        !state
            .engine
            .remember_domain_focus(&nowhere, &WindowId("t2".to_owned()))
    );
    assert!(
        !state
            .engine
            .remember_domain_focus(&target.1, &WindowId("ghost".to_owned()))
    );
    // Production keyboard route: same pair request after the MRU sync
    // actuates the fresh target with no geometry writes.
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    fn window(key: &WindowKey, token: &str, rect: Rect, me: &ProcessIdentity) -> ObservedWindow {
        ObservedWindow {
            hwnd: key.hwnd,
            token: token.to_owned(),
            outer: rect,
            visible: rect,
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd: key.hwnd,
                pid: key.pid,
                process_creation: key.creation.clone(),
                exe_path: me.exe_path.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }
    }
    let observed: Vec<ObservedWindow> = vec![
        window(
            &mover,
            "mover",
            Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
        window(
            &t1,
            "t1",
            Rect {
                x: 2000,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
        window(
            &t2,
            "t2",
            Rect {
                x: 2500,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
    ];
    let retained: Vec<RetainedRow> = Vec::new();
    let mut hint_cx = HintCx::new();
    let loc = MemberLoc {
        output: "primary".to_owned(),
        workspace: ws_a.clone(),
        hidden: false,
    };
    let source_rows = vec![crate::workspace_owner::OwnerRow {
        token: "mover".to_owned(),
        rect: Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        },
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let intent = QueuedIntent {
        op: SnapOp::Focus,
        direction: Direction::Right,
        edge: SnapEdge::Down,
        origin: None,
        consumed: false,
        announce: false,
        tick: std::time::Instant::now(),
    };
    let origin = SnapOrigin {
        hwnd: mover.hwnd,
        token: "mover".to_owned(),
        pid: mover.pid,
        creation: mover.creation.clone(),
    };
    let correlation = CorrelationId::parse("corr-focus-mru").expect("correlation");
    let outcome = run_directional_cross(
        &mut state,
        &me,
        &fulls,
        &areas,
        &observed,
        &retained,
        &mut hint_cx,
        &loc,
        &loc,
        &mover,
        "mover",
        &source_rows,
        "right",
        &ws_b,
        &intent,
        &origin,
        7,
        &correlation,
        correlation.as_str(),
        false,
        Vec::new(),
        false,
        false,
    );
    assert_eq!(outcome, "focus-ok");
    assert_eq!(
        state.workspaces.focus_target(
            "right",
            &ws_b,
            &[mover.clone(), t1.clone(), t2.clone()]
                .into_iter()
                .collect::<BTreeSet<_>>()
        ),
        Some(t2.clone()),
        "fresh MRU survives the cross focus"
    );
    assert_eq!(
        state.workspaces.active_id("right").as_deref(),
        Some(ws_b.as_str()),
        "no destination view switch"
    );
    clear_pending_seams();
}

/// Empty-target move rig for commit-last refusal regressions: a sole
/// mover homed on `primary` with an empty `right` current view, so the
/// pair plan is a deterministic cross and the target rows are
/// vacuously complete.
struct EmptyTargetMoveRig {
    state: super::TileLoop,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    me: crate::model::ProcessIdentity,
    mover: crate::workspace::WindowKey,
    loc: crate::workspace::MemberLoc,
    target_output: String,
    target_ws: String,
    observed: Vec<super::ObservedWindow>,
    source_rows: Vec<crate::workspace_owner::OwnerRow>,
    intent: crate::snapkey::QueuedIntent,
    origin: crate::snapkey::SnapOrigin,
}

fn empty_target_move_rig() -> EmptyTargetMoveRig {
    use super::{
        ObservedWindow, clear_pending_seams, inject_pending_elevated, inject_pending_suspend,
        inject_pending_tag,
    };
    use crate::snapkey::{QueuedIntent, SnapEdge, SnapOp, SnapOrigin};
    use crate::workspace::{MemberLoc, WindowKey};
    clear_pending_seams();
    let mut state = test_state();
    let areas = primary_pair();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("primary");
    state.workspaces.ensure_output("right");
    let ws_a = state.workspaces.active_id("primary").expect("active");
    let ws_b = state.workspaces.active_id("right").expect("active");
    state.workspaces.set_tiled("primary", &ws_a, true);
    state.workspaces.set_tiled("right", &ws_b, true);
    let src_bounds = workspace_domain_for("primary", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let me = crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-me".to_owned(),
        user_sid: "S-1-move".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    };
    let mover = WindowKey {
        hwnd: 0x4101,
        pid: 7101,
        creation: format!("c{:016x}", 0x4101u64),
    };
    state
        .member_tokens
        .insert(mover.clone(), "mover".to_owned());
    state.member_identity.insert(
        mover.clone(),
        crate::model::ProcessIdentity {
            pid: 7101,
            process_creation: mover.creation.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            exe_path: me.exe_path.clone(),
        },
    );
    state
        .member_tags
        .insert(mover.clone(), format!("tag-{:x}", mover.hwnd));
    inject_pending_tag(mover.hwnd, Some(format!("tag-{:x}", mover.hwnd)));
    let loc = MemberLoc {
        output: "primary".to_owned(),
        workspace: ws_a.clone(),
        hidden: false,
    };
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "primary", &ws_a, false)
    );
    seed(
        &mut state,
        "primary",
        &ws_a,
        src_bounds,
        &["mover"],
        "mover",
    );
    let frame = Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let observed = vec![ObservedWindow {
        hwnd: mover.hwnd,
        token: "mover".to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd: mover.hwnd,
            pid: mover.pid,
            process_creation: mover.creation.clone(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }];
    let source_rows = vec![crate::workspace_owner::OwnerRow {
        token: "mover".to_owned(),
        rect: frame,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    EmptyTargetMoveRig {
        state,
        areas,
        fulls,
        me,
        mover: mover.clone(),
        loc,
        target_output: "right".to_owned(),
        target_ws: ws_b,
        observed,
        source_rows,
        intent: QueuedIntent {
            op: SnapOp::Move,
            direction: Direction::Right,
            edge: SnapEdge::Down,
            origin: None,
            consumed: false,
            announce: false,
            tick: std::time::Instant::now(),
        },
        origin: SnapOrigin {
            hwnd: mover.hwnd,
            token: "mover".to_owned(),
            pid: mover.pid,
            creation: mover.creation.clone(),
        },
    }
}

/// Predict the mover's planned arrival rect for the empty-target rig by
/// replaying the production pair construction on a cloned Engine (no
/// mutation): the success test needs it to stage the arrival proof.
fn predict_empty_target_mover_rect(rig: &EmptyTargetMoveRig, correlation: &CorrelationId) -> Rect {
    use super::{revision_for, same_axis_move_for, stamp_engine_event};
    let (source_base, source_key) =
        workspace_domain_for(&rig.loc.output, &rig.loc.workspace, &rig.areas, 8, 8)
            .expect("source domain");
    let (target_base, target_key) =
        workspace_domain_for(&rig.target_output, &rig.target_ws, &rig.areas, 8, 8)
            .expect("target domain");
    assert!(
        crate::settings::retained_gaps_match(&rig.state.engine, &source_key, 8, 8)
            && crate::settings::retained_gaps_match(&rig.state.engine, &target_key, 8, 8),
        "rig gaps must match so no adopt perturbs the predicted plan"
    );
    let ((source_domain, source_key), (target_domain, target_key)) =
        crate::workspace_owner::directional_pair_with_adjacency(
            (source_base, source_key),
            (target_base, target_key),
            Direction::Right,
        );
    let mut pairs: Vec<(String, Rect)> = rig
        .source_rows
        .iter()
        .map(|r| (r.token.clone(), r.rect))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let fp = crate::tiling::fingerprint(&pairs);
    let from = WindowId("mover".to_owned());
    let source_windows: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rig
        .source_rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &rig.state.owner,
        &rig.state.generation,
        correlation,
        revision_for(&rig.state, &rig.loc.output, &rig.loc.workspace),
        fp,
        &source_domain,
        &source_key,
        rig.state.outer_gap,
        &source_windows,
        Some(&from),
    );
    event.directional = Some(vec![
        (source_domain, source_key),
        (target_domain, target_key),
    ]);
    event.directional_target_outer_gap = Some(rig.state.outer_gap);
    event.command = CoreCommand::Move {
        window: "mover".to_owned(),
        direction: "right".to_owned(),
        cross_output_transfer: true,
        same_axis_move: same_axis_move_for(rig.state.settings_live.as_ref()),
    };
    stamp_engine_event(&rig.state, &mut event);
    let mut candidate = rig.state.engine.clone();
    let reply = candidate.handle(&event);
    let CoreReply::MoveDirectional(plan) = &reply else {
        panic!("empty-target move must plan, got {reply:?}");
    };
    assert!(plan.cross.is_some(), "sole mover must cross, got {plan:?}");
    crate::workspace_owner::planned_writes(&reply)
        .expect("plan writes")
        .iter()
        .find(|w| w.window.0 == "mover")
        .expect("mover write")
        .rect
}

fn run_empty_target_move(
    rig: &mut EmptyTargetMoveRig,
    correlation: &CorrelationId,
    tick: u64,
) -> &'static str {
    use super::{HintCx, run_directional_cross};
    let mut hint_cx = HintCx::new();
    run_directional_cross(
        &mut rig.state,
        &rig.me,
        &rig.fulls,
        &rig.areas,
        &rig.observed,
        &[],
        &mut hint_cx,
        &rig.loc,
        &rig.loc,
        &rig.mover,
        "mover",
        &rig.source_rows,
        &rig.target_output,
        &rig.target_ws,
        &rig.intent,
        &rig.origin,
        tick,
        correlation,
        correlation.as_str(),
        false,
        Vec::new(),
        false,
        false,
    )
}

#[test]
fn cross_move_verified_tail_commits_both_once_with_follow_focus() {
    use super::{
        clear_pending_seams, inject_monitor_areas, inject_pending_focus, inject_pending_observe,
        output_for_rect, pending_counters,
    };
    let mut rig = empty_target_move_rig();
    let correlation = CorrelationId::parse("corr-move-commit").expect("correlation");
    let planned = predict_empty_target_mover_rect(&rig, &correlation);
    assert_eq!(
        output_for_rect(&rig.areas, &planned),
        "right",
        "planned arrival must land on the destination monitor"
    );
    rig.state.stable.insert(rig.mover.hwnd, planned);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_empty_target_move(&mut rig, &correlation, 21);
    assert_eq!(outcome, "move-applied");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "right");
    assert_eq!(loc.workspace, rig.target_ws);
    assert_ne!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "verified transfer publishes the Engine once"
    );
    assert_eq!(
        pending_counters().focus,
        1,
        "verified transfer follows focus exactly once"
    );
    clear_pending_seams();
}

#[test]
fn cross_move_late_topology_drift_retains_source_and_engine() {
    use super::{
        clear_pending_seams, inject_monitor_areas, inject_pending_focus, inject_pending_observe,
        pending_counters,
    };
    let mut rig = empty_target_move_rig();
    let correlation = CorrelationId::parse("corr-move-topology").expect("correlation");
    let mut drifted = rig.areas.clone();
    drifted[1].full.x = 2000;
    inject_monitor_areas(Some(drifted));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_empty_target_move(&mut rig, &correlation, 22);
    assert_eq!(outcome, "move-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary");
    assert_eq!(loc.workspace, rig.loc.workspace);
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "late topology drift keeps the retained Engine"
    );
    let counters = pending_counters();
    assert_eq!(counters.focus, 0, "refused transfer skips follow-focus");
    assert!(
        counters.reconcile >= 1,
        "refused transfer reconciles with the fresh topology"
    );
    clear_pending_seams();
}

#[test]
fn cross_move_failed_readback_retains_source_and_engine() {
    use super::{
        clear_pending_seams, inject_monitor_areas, inject_pending_apply, inject_pending_focus,
        inject_pending_observe, pending_counters,
    };
    let mut rig = empty_target_move_rig();
    let correlation = CorrelationId::parse("corr-move-readback").expect("correlation");
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_apply(Some((false, 0)));
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_empty_target_move(&mut rig, &correlation, 23);
    assert_eq!(outcome, "move-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary");
    assert_eq!(loc.workspace, rig.loc.workspace);
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "failed readback keeps the retained Engine"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "refused transfer skips follow-focus"
    );
    clear_pending_seams();
}

#[test]
fn cross_move_arrival_mismatch_retains_source_and_engine() {
    use super::{
        clear_pending_seams, inject_monitor_areas, inject_pending_focus, inject_pending_observe,
        pending_counters,
    };
    let mut rig = empty_target_move_rig();
    let correlation = CorrelationId::parse("corr-move-arrival").expect("correlation");
    // Stable still shows the source frame (never arrived on `right`).
    let home = rig.observed[0].visible;
    rig.state.stable.insert(rig.mover.hwnd, home);
    inject_monitor_areas(Some(rig.areas.clone()));
    inject_pending_observe(rig.observed.clone(), Vec::new(), false);
    inject_pending_focus(Some("focus-ok"));
    let engine_before = format!("{:?}", rig.state.engine);
    let outcome = run_empty_target_move(&mut rig, &correlation, 24);
    assert_eq!(outcome, "move-unverified");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, "primary");
    assert_eq!(loc.workspace, rig.loc.workspace);
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "arrival mismatch keeps the retained Engine"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "refused transfer skips follow-focus"
    );
    clear_pending_seams();
}

#[test]
fn cross_focus_failed_actuation_discards_candidate_mru() {
    // Occupied target with a fresh project MRU (t2) ahead of the stale
    // Engine remembered focus (t1): a failed follow-focus must leave
    // both the retained Engine and the project MRU untouched.
    use super::{
        HintCx, ObservedWindow, clear_pending_seams, inject_pending_elevated, inject_pending_focus,
        inject_pending_suspend, inject_pending_tag, run_directional_cross,
    };
    use crate::snapkey::{QueuedIntent, SnapEdge, SnapOp, SnapOrigin};
    use crate::workspace::{MemberLoc, WindowKey};
    use std::collections::BTreeSet;
    clear_pending_seams();
    let mut state = test_state();
    let areas = primary_pair();
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.workspaces.ensure_output("primary");
    state.workspaces.ensure_output("right");
    let ws_a = state.workspaces.active_id("primary").expect("active");
    let ws_b = state.workspaces.active_id("right").expect("active");
    state.workspaces.set_tiled("primary", &ws_a, true);
    state.workspaces.set_tiled("right", &ws_b, true);
    let src_bounds = workspace_domain_for("primary", &ws_a, &areas, 8, 8)
        .expect("src domain")
        .0
        .bounds;
    let dst_bounds = workspace_domain_for("right", &ws_b, &areas, 8, 8)
        .expect("dst domain")
        .0
        .bounds;
    fn key(hwnd: u64, pid: u32) -> WindowKey {
        WindowKey {
            hwnd,
            pid,
            creation: format!("c{hwnd:016x}"),
        }
    }
    let me = crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-me".to_owned(),
        user_sid: "S-1-focus".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    };
    let mover = key(0x4101, 7101);
    let t1 = key(0x4102, 7102);
    let t2 = key(0x4103, 7103);
    for (k, token, pid) in [
        (&mover, "mover", 7101u32),
        (&t1, "t1", 7102),
        (&t2, "t2", 7103),
    ] {
        state.member_tokens.insert(k.clone(), token.to_owned());
        state.member_identity.insert(
            k.clone(),
            crate::model::ProcessIdentity {
                pid,
                process_creation: k.creation.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                exe_path: me.exe_path.clone(),
            },
        );
        state
            .member_tags
            .insert(k.clone(), format!("tag-{:x}", k.hwnd));
        inject_pending_tag(k.hwnd, Some(format!("tag-{:x}", k.hwnd)));
    }
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "primary", &ws_a, false)
    );
    assert!(state.workspaces.assign(t1.clone(), "right", &ws_b, false));
    assert!(state.workspaces.assign(t2.clone(), "right", &ws_b, false));
    seed(
        &mut state,
        "primary",
        &ws_a,
        src_bounds,
        &["mover"],
        "mover",
    );
    seed(&mut state, "right", &ws_b, dst_bounds, &["t1", "t2"], "t1");
    state.workspaces.note_foreground(&t2);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("vanished"));
    fn window(
        key: &WindowKey,
        token: &str,
        rect: Rect,
        me: &crate::model::ProcessIdentity,
    ) -> ObservedWindow {
        ObservedWindow {
            hwnd: key.hwnd,
            token: token.to_owned(),
            outer: rect,
            visible: rect,
            insets: crate::tiling::FrameInsets {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            identity: crate::tiling::ObservedTarget {
                hwnd: key.hwnd,
                pid: key.pid,
                process_creation: key.creation.clone(),
                exe_path: me.exe_path.clone(),
                user_sid: me.user_sid.clone(),
                session_id: me.session_id,
                tag: String::new(),
            },
            facts: crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: false,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            },
        }
    }
    let observed: Vec<ObservedWindow> = vec![
        window(
            &mover,
            "mover",
            Rect {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
        window(
            &t1,
            "t1",
            Rect {
                x: 2000,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
        window(
            &t2,
            "t2",
            Rect {
                x: 2500,
                y: 100,
                w: 400,
                h: 300,
            },
            &me,
        ),
    ];
    let mut hint_cx = HintCx::new();
    let loc = MemberLoc {
        output: "primary".to_owned(),
        workspace: ws_a.clone(),
        hidden: false,
    };
    let source_rows = vec![crate::workspace_owner::OwnerRow {
        token: "mover".to_owned(),
        rect: Rect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        },
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let intent = QueuedIntent {
        op: SnapOp::Focus,
        direction: Direction::Right,
        edge: SnapEdge::Down,
        origin: None,
        consumed: false,
        announce: false,
        tick: std::time::Instant::now(),
    };
    let origin = SnapOrigin {
        hwnd: mover.hwnd,
        token: "mover".to_owned(),
        pid: mover.pid,
        creation: mover.creation.clone(),
    };
    let correlation = CorrelationId::parse("corr-focus-discard").expect("correlation");
    let engine_before = format!("{:?}", state.engine);
    let outcome = run_directional_cross(
        &mut state,
        &me,
        &fulls,
        &areas,
        &observed,
        &[],
        &mut hint_cx,
        &loc,
        &loc,
        &mover,
        "mover",
        &source_rows,
        "right",
        &ws_b,
        &intent,
        &origin,
        25,
        &correlation,
        correlation.as_str(),
        false,
        Vec::new(),
        false,
        false,
    );
    assert_eq!(outcome, "vanished");
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "failed focus actuation discards the candidate MRU sync"
    );
    assert_eq!(
        state.workspaces.focus_target(
            "right",
            &ws_b,
            &[mover.clone(), t1.clone(), t2.clone()]
                .into_iter()
                .collect::<BTreeSet<_>>()
        ),
        Some(t2.clone()),
        "project MRU survives the refused focus"
    );
    clear_pending_seams();
}

#[test]
fn differing_work_full_gaps_and_negative_coords_resolve() {
    // Full rectangles drive selection (panel gaps never block); work
    // areas drive domain bounds with the outer-gap inset.
    let areas = vec![
        area(
            "left",
            Rect {
                x: -1920,
                y: -40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "primary",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ];
    let mut state = test_state();
    state.workspaces.ensure_output("left");
    state.workspaces.ensure_output("primary");
    let mover_rect = Rect {
        x: 100,
        y: 500,
        w: 200,
        h: 200,
    };
    // Helper indirection stays local to this test: build entries inline
    // so the assertion pins FULL-rectangle selection, not work areas.
    let entries: Vec<crate::workspace_owner::DirectionalOutput> = areas
        .iter()
        .map(|a| crate::workspace_owner::DirectionalOutput {
            id: a.device.clone(),
            full: a.full,
        })
        .collect();
    let selected = crate::workspace_owner::select_directional_target(
        Direction::Left,
        "primary",
        &mover_rect,
        &entries,
    )
    .expect("readable");
    assert_eq!(selected, Some("left".to_owned()));
    // Domain bounds follow the work area inset by the outer gap.
    let (domain, _) = workspace_domain_for("primary", "ws-1", &areas, 8, 8).expect("domain");
    assert_eq!(
        domain.bounds,
        Rect {
            x: 8,
            y: 48,
            w: 1904,
            h: 1024
        }
    );
    let pair = primary_pair();
    assert_eq!(pair.len(), 2);
}

#[test]
fn destination_current_view_survives_cross_without_history_record() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    assert_eq!(state.workspaces.history_previous("mon-b"), None);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let source = seed(&mut state, "mon-a", &ws_a, bounds, &["mover"], "mover");
    let target = seed(&mut state, "mon-b", &ws_b, bounds, &["t1"], "t1");
    let event = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &["t1"],
        "mover",
        Direction::Right,
        CoreCommand::Move {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    assert!(matches!(
        state.engine.handle(&event),
        CoreReply::MoveDirectional(_)
    ));
    // Current views do not change on a cross move: both outputs stay on
    // their workspaces, so no history records.
    assert_eq!(state.workspaces.active_id("mon-a"), Some(ws_a));
    assert_eq!(state.workspaces.active_id("mon-b"), Some(ws_b));
    assert_eq!(state.workspaces.history_previous("mon-b"), None);
}

#[test]
fn project_transfer_and_focus_follow_without_hiding() {
    use std::collections::BTreeSet;
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let mover = crate::workspace::WindowKey {
        hwnd: 501,
        pid: 1501,
        creation: "c0000000000001f5".to_owned(),
    };
    let peer = crate::workspace::WindowKey {
        hwnd: 502,
        pid: 1502,
        creation: "c0000000000001f6".to_owned(),
    };
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-a", &ws_a, false)
    );
    assert!(state.workspaces.assign(peer.clone(), "mon-b", &ws_b, false));
    // Pre-transfer: mover on source only.
    assert!(!crate::workspace_owner::verify_directional_transfer(
        &mover,
        &state.workspaces.workspace_members("mon-a", &ws_a),
        &state.workspaces.workspace_members("mon-b", &ws_b),
    ));
    // Engine-equivalent commit then project transfer.
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-b", &ws_b, false)
    );
    assert!(crate::workspace_owner::verify_directional_transfer(
        &mover,
        &state.workspaces.workspace_members("mon-a", &ws_a),
        &state.workspaces.workspace_members("mon-b", &ws_b),
    ));
    // Follow keeps the arrival visible: no hidden flag is set by a cross
    // move (both outputs show their current views simultaneously).
    assert!(!state.workspaces.is_hidden(&mover));
    // Failed arrival (mover missing on target) refuses without replay:
    // revert narrowly to the source only.
    let ghost = crate::workspace::WindowKey {
        hwnd: 503,
        pid: 1503,
        creation: "c0000000000001f7".to_owned(),
    };
    let source: BTreeSet<crate::workspace::WindowKey> =
        state.workspaces.workspace_members("mon-a", &ws_a);
    let target: BTreeSet<crate::workspace::WindowKey> =
        state.workspaces.workspace_members("mon-b", &ws_b);
    assert!(!crate::workspace_owner::verify_directional_transfer(
        &ghost, &source, &target
    ));
    // Stale fences refuse the tails before any native effect.
    let snapshot = crate::workspace_owner::DirectionalFenceSnapshot {
        inner_gap: 8,
        outer_gap: 8,
        source_output: "mon-a".to_owned(),
        source_workspace: ws_a.clone(),
        target_output: "mon-b".to_owned(),
        target_workspace: ws_b.clone(),
        source_tiled: true,
        target_tiled: true,
        direction: Direction::Right,
        overlay_maximized: false,
        overlay_fullscreen: false,
        source_full: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        },
        target_full: Rect {
            x: 1920,
            y: 0,
            w: 1920,
            h: 1080,
        },
        source_bounds: Rect {
            x: 8,
            y: 8,
            w: 1904,
            h: 1064,
        },
        target_bounds: Rect {
            x: 1928,
            y: 8,
            w: 1904,
            h: 1064,
        },
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
    };
    let mut live = crate::workspace_owner::DirectionalFenceLive {
        inner_gap: 8,
        outer_gap: 8,
        source_tiled: true,
        target_tiled: true,
        source_is_active: true,
        target_is_active: true,
        topology_ok: true,
        scope_ok: true,
        lifetime_ok: true,
        membership_ok: true,
        overlay_maximized: false,
        overlay_fullscreen: false,
        owner_ok: true,
    };
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        None
    );
    live.target_is_active = false;
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        Some("view-changed")
    );
    live.target_is_active = true;
    live.lifetime_ok = false;
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        Some("identity-changed")
    );
}

#[test]
fn resolve_uses_destination_current_view_and_local_fallbacks() {
    // Dispatch-level preparation seam: production `resolve_...` over a
    // real `TileLoop` plus synthetic FULL/work areas (negative coords,
    // panel-inset work areas). No native effects.
    use tiler_core::directional::Direction;
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let areas = vec![
        area(
            "mon-a",
            Rect {
                x: -1920,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-b",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ];
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let mover = Rect {
        x: -200,
        y: 500,
        w: 200,
        h: 200,
    };
    assert_eq!(
        super::resolve_directional_target(&state, &areas, "mon-a", Direction::Right, &mover, false,),
        Some(("mon-b".to_owned(), ws_b.clone()))
    );
    // Vertical focus never crosses: local fallback.
    assert_eq!(
        super::resolve_directional_target(&state, &areas, "mon-a", Direction::Up, &mover, true,),
        None
    );
    // Floating destination never crosses: local fallback.
    assert!(state.workspaces.set_tiled("mon-b", &ws_b, false));
    assert_eq!(
        super::resolve_directional_target(&state, &areas, "mon-a", Direction::Right, &mover, false,),
        None
    );
    assert!(state.workspaces.set_tiled("mon-b", &ws_b, true));
    // No candidate (far gap) is a local fallback, never a wrap or a
    // refusal of the local command.
    let lonely = vec![area(
        "mon-a",
        Rect {
            x: 0,
            y: 40,
            w: 1920,
            h: 1040,
        },
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        },
    )];
    assert_eq!(
        super::resolve_directional_target(
            &state,
            &lonely,
            "mon-a",
            Direction::Right,
            &mover,
            false,
        ),
        None
    );
}

#[test]
fn live_fences_detect_gap_view_and_topology_drift() {
    // Dispatch-level fence seam over a real `TileLoop`: snapshot holds
    // except for the injected drift. Lifetime cannot hold without native
    // tags, so each case asserts the earlier refusal token, which is the
    // production ordering that matters before any mutation.
    use tiler_core::directional::Direction;
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let areas = vec![
        area(
            "mon-a",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-b",
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ];
    let mover = crate::workspace::WindowKey {
        hwnd: 601,
        pid: 1601,
        creation: "c000000000000259".to_owned(),
    };
    let snapshot = super::directional_snapshot(
        &state,
        &areas,
        "mon-a",
        &ws_a,
        "mon-b",
        &ws_b,
        Direction::Right,
        false,
        false,
    )
    .expect("snapshot");
    // Gap drift refuses before any mutation.
    state.inner_gap = 16;
    let live = super::directional_live(&state, &areas, &snapshot, &mover, true, false, false);
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        Some("gap-changed")
    );
    state.inner_gap = 8;
    // View drift: switch the destination to its sibling workspace.
    let sibling = state
        .workspaces
        .workspace_ids("mon-b")
        .into_iter()
        .find(|id| id != &ws_b)
        .expect("sibling workspace");
    assert!(state.workspaces.activate("mon-b", &sibling));
    let live = super::directional_live(&state, &areas, &snapshot, &mover, true, false, false);
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        Some("view-changed")
    );
    assert!(state.workspaces.activate("mon-b", &ws_b));
    // Topology drift: destination FULL rect moves.
    let mut moved = areas.clone();
    moved[1].full.x = 2000;
    let live = super::directional_live(&state, &moved, &snapshot, &mover, true, false, false);
    assert_eq!(
        crate::workspace_owner::directional_fences_hold(&snapshot, &live),
        Some("topology-changed")
    );
}

#[test]
fn arrival_proof_requires_planned_rect_on_target_monitor() {
    // Finalization seam: production `directional_arrival_verified` over
    // the just-published managed observation. No native effects.
    use tiler_core::boundary::{CoreReply, MovePlanReply};
    use tiler_core::directional::{Capability, NodeId, OutputId, Rule, WorkspaceId};
    let mut state = test_state();
    let areas = vec![
        area(
            "mon-a",
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-b",
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
    ];
    let planned = Rect {
        x: 2000,
        y: 100,
        w: 800,
        h: 600,
    };
    let reply = CoreReply::MoveDirectional(MovePlanReply {
        base_revision: 2,
        rule: Rule::R4,
        capability: Capability::CrossOutputTransfer,
        direction: Direction::Right,
        geometry: vec![tiler_core::session::DesiredGeometry {
            window: WindowId("mover".to_owned()),
            leaf: NodeId("leaf-mover".to_owned()),
            output: OutputId("mon-b".to_owned()),
            workspace: WorkspaceId("ws-b".to_owned()),
            rect: planned,
            overconstrained: false,
            client_clamped: false,
        }],
        focus_domain: tiler_core::session::DomainKey {
            output: OutputId("mon-b".to_owned()),
            workspace: WorkspaceId("ws-b".to_owned()),
        },
        focus_leaf: NodeId("leaf-mover".to_owned()),
        cross: None,
    });
    // Stable has no observation yet: no proof.
    assert!(!super::directional_arrival_verified(
        &state, &areas, &reply, "mover", 701, "mon-b"
    ));
    state.stable.insert(701, planned);
    assert!(super::directional_arrival_verified(
        &state, &areas, &reply, "mover", 701, "mon-b"
    ));
    // Readback mismatch on the target monitor: no proof.
    state.stable.insert(
        701,
        Rect {
            x: 2001,
            y: 100,
            w: 800,
            h: 600,
        },
    );
    assert!(!super::directional_arrival_verified(
        &state, &areas, &reply, "mover", 701, "mon-b"
    ));
    // Right shape, wrong monitor: no proof.
    state.stable.insert(
        701,
        Rect {
            x: 100,
            y: 100,
            w: 800,
            h: 600,
        },
    );
    assert!(!super::directional_arrival_verified(
        &state, &areas, &reply, "mover", 701, "mon-b"
    ));
    // Non-plan replies never prove arrival.
    let rejected = CoreReply::Rejected {
        kind: "unchanged",
        message: "no edge",
    };
    state.stable.insert(701, planned);
    assert!(!super::directional_arrival_verified(
        &state, &areas, &rejected, "mover", 701, "mon-b"
    ));
}

#[test]
fn differing_work_areas_place_each_domain_in_its_own_bounds() {
    // Two-domain geometry fixture over actually differing work areas
    // (panel inset on the source, smaller target): the cross plan must
    // keep the arrival inside the target bounds and the source survivor
    // inside the source bounds. Physical coordinates, no rescaling.
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let areas = vec![
        area(
            "mon-a",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-b",
            Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 1024,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 1024,
            },
        ),
    ];
    let (source_domain, source_key) =
        workspace_domain_for("mon-a", &ws_a, &areas, 8, 8).expect("source domain");
    let (target_domain, target_key) =
        workspace_domain_for("mon-b", &ws_b, &areas, 8, 8).expect("target domain");
    assert_ne!(source_domain.bounds, target_domain.bounds);
    let source = seed(
        &mut state,
        "mon-a",
        &ws_a,
        source_domain.bounds,
        &["mover"],
        "mover",
    );
    assert_eq!(source.0.bounds, source_domain.bounds);
    let target = seed(
        &mut state,
        "mon-b",
        &ws_b,
        target_domain.bounds,
        &["t1", "t2"],
        "t1",
    );
    assert_eq!(target.0.bounds, target_domain.bounds);
    let event = pair_event(
        &state,
        &source,
        &target,
        &["mover"],
        &["t1", "t2"],
        "mover",
        Direction::Right,
        CoreCommand::Move {
            window: "mover".to_owned(),
            direction: "right".to_owned(),
            cross_output_transfer: true,
            same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
        },
    );
    let contains = |outer: &Rect, inner: &Rect| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    };
    match state.engine.handle(&event) {
        CoreReply::MoveDirectional(plan) => {
            assert!(plan.cross.is_some(), "sole root must cross");
            for entry in &plan.geometry {
                if entry.window.0 == "mover" {
                    assert!(
                        contains(&target_domain.bounds, &entry.rect),
                        "arrival must sit in target bounds"
                    );
                } else {
                    let home = if ["t1", "t2"].contains(&entry.window.0.as_str()) {
                        &target_domain.bounds
                    } else {
                        &source_domain.bounds
                    };
                    assert!(contains(home, &entry.rect), "survivor stays in bounds");
                }
            }
            // Keep the keys distinct even though both domains tile.
            assert_ne!(source_key, target_key);
        }
        other => panic!("differing work areas must cross, got {other:?}"),
    }
}

#[test]
fn four_way_occupied_cross_selects_and_contains() {
    // Four-way topology around `mon-a`: negative-x west, smaller east,
    // stacked north/south with panel-inset work areas and differing
    // physical sizes. FULL rects select; per-output inset bounds contain.
    let areas = vec![
        area(
            "mon-a",
            Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-l",
            Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: -1920,
                y: 0,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-r",
            Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 1024,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 1024,
            },
        ),
        area(
            "mon-t",
            Rect {
                x: 0,
                y: -1080,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: -1080,
                w: 1920,
                h: 1080,
            },
        ),
        area(
            "mon-b",
            Rect {
                x: 0,
                y: 1120,
                w: 1920,
                h: 1040,
            },
            Rect {
                x: 0,
                y: 1080,
                w: 1920,
                h: 1080,
            },
        ),
    ];
    let mover_rect = Rect {
        x: 800,
        y: 400,
        w: 200,
        h: 200,
    };
    let entries: Vec<crate::workspace_owner::DirectionalOutput> = areas
        .iter()
        .map(|a| crate::workspace_owner::DirectionalOutput {
            id: a.device.clone(),
            full: a.full,
        })
        .collect();
    for (direction, expected) in [
        (Direction::Left, "mon-l"),
        (Direction::Right, "mon-r"),
        (Direction::Up, "mon-t"),
        (Direction::Down, "mon-b"),
    ] {
        let selected = crate::workspace_owner::select_directional_target(
            direction,
            "mon-a",
            &mover_rect,
            &entries,
        )
        .expect("readable");
        assert_eq!(selected, Some(expected.to_owned()), "{direction:?}");
    }
    // Real occupied-cross Engine moves for the three directions Right
    // does not already cover; each arrival stays in its own inset bounds
    // (physical coordinates, no rescaling).
    let contains = |outer: &Rect, inner: &Rect| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    };
    for (direction, target_output, name) in [
        (Direction::Left, "mon-l", "left"),
        (Direction::Up, "mon-t", "up"),
        (Direction::Down, "mon-b", "down"),
    ] {
        let mut state = test_state();
        for output in ["mon-a", target_output] {
            state.workspaces.ensure_output(output);
        }
        let ws_a = state.workspaces.active_id("mon-a").expect("active");
        let ws_b = state.workspaces.active_id(target_output).expect("active");
        let (source_domain, _) =
            workspace_domain_for("mon-a", &ws_a, &areas, 8, 8).expect("src domain");
        let (target_domain, _) =
            workspace_domain_for(target_output, &ws_b, &areas, 8, 8).expect("dst domain");
        assert_ne!(source_domain.bounds, target_domain.bounds);
        let source = seed(
            &mut state,
            "mon-a",
            &ws_a,
            source_domain.bounds,
            &["mover"],
            "mover",
        );
        let target = seed(
            &mut state,
            target_output,
            &ws_b,
            target_domain.bounds,
            &["t1", "t2"],
            "t1",
        );
        let event = pair_event(
            &state,
            &source,
            &target,
            &["mover"],
            &["t1", "t2"],
            "mover",
            direction,
            CoreCommand::Move {
                window: "mover".to_owned(),
                direction: name.to_owned(),
                cross_output_transfer: true,
                same_axis_move: tiler_core::directional::SameAxisMove::GroupWithNeighbor,
            },
        );
        match state.engine.handle(&event) {
            CoreReply::MoveDirectional(plan) => {
                let cross = plan.cross.expect("sole root must cross");
                assert_eq!(cross.target, CrossOutputTarget::Occupied);
                assert_eq!(plan.focus_domain.output.0, target_output);
                assert!(!plan.geometry.is_empty(), "cross carries placement");
                for entry in &plan.geometry {
                    let home = if entry.window.0 == "mover"
                        || ["t1", "t2"].contains(&entry.window.0.as_str())
                    {
                        &target_domain.bounds
                    } else {
                        &source_domain.bounds
                    };
                    assert!(
                        contains(home, &entry.rect),
                        "{name} placement must sit in its own bounds"
                    );
                }
            }
            other => panic!("{name} must cross, got {other:?}"),
        }
    }
}

#[test]
fn empty_focus_run_refuses_without_writes_and_retains_mru() {
    // Full `run_directional_cross` focus leg into the empty `right` view:
    // the Engine refuses unchanged with zero geometry/membership/focus
    // writes and the retained Engine plus source homing survive.
    use super::{
        HintCx, clear_pending_seams, inject_pending_elevated, inject_pending_suspend,
        pending_counters, run_directional_cross,
    };
    use crate::snapkey::{QueuedIntent, SnapEdge, SnapOp};
    let mut rig = empty_target_move_rig();
    let correlation = CorrelationId::parse("corr-focus-empty-run").expect("correlation");
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    let engine_before = format!("{:?}", rig.state.engine);
    let home = rig.loc.clone();
    let mut hint_cx = HintCx::new();
    let intent = QueuedIntent {
        op: SnapOp::Focus,
        direction: Direction::Right,
        edge: SnapEdge::Down,
        origin: None,
        consumed: false,
        announce: false,
        tick: std::time::Instant::now(),
    };
    let outcome = run_directional_cross(
        &mut rig.state,
        &rig.me,
        &rig.fulls,
        &rig.areas,
        &rig.observed,
        &[],
        &mut hint_cx,
        &rig.loc,
        &rig.loc,
        &rig.mover,
        "mover",
        &rig.source_rows,
        &rig.target_output,
        &rig.target_ws,
        &intent,
        &rig.origin,
        26,
        &correlation,
        correlation.as_str(),
        false,
        Vec::new(),
        false,
        false,
    );
    assert_eq!(outcome, "unchanged");
    let loc = rig.state.workspaces.member_loc(&rig.mover).expect("homed");
    assert_eq!(loc.output, home.output);
    assert_eq!(loc.workspace, home.workspace);
    assert_eq!(
        format!("{:?}", rig.state.engine),
        engine_before,
        "refused empty focus keeps the retained Engine"
    );
    assert_eq!(pending_counters().focus, 0, "refused focus never actuates");
    clear_pending_seams();
}
