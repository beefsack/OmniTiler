#![forbid(unsafe_code)]
// Windows cross-output drag-drop (REQ-MOU-03) through a real `TileLoop`
// against the real retained Engine (no HWNDs, hooks, or live effects):
// pointer-resolved destination current views over a negative-x secondary
// with a panel-inset work area, occupied/empty destinations, preview/drop
// agreement through the production pair and singleton shapes, Engine plus
// project transfer without hiding or view switches, stale/misrouted
// refusals, arrival proof (missing and clamped), drop-destination routing,
// and native move versus resize classification. Native setters stay
// user-owned live acceptance; these pin the portable pair construction,
// Engine planning, and the settle verification.
use super::rmax03_adapter_tests::test_state;
use super::{DropDestination, MonitorArea, directional_arrival_verified, resolve_drop_destination};
use tiler_core::boundary::{CoreCommand, CoreReply, TiledKind};
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

fn area(device: &str, work: Rect, full: Rect) -> MonitorArea {
    MonitorArea {
        device: device.to_owned(),
        work,
        full,
    }
}

/// Sideways pair with a negative-x secondary carrying a bottom panel
/// strip: FULL rectangles decide the pointer output while inset work
/// areas govern placement. Mixed-DPI hosts behave the same (physical
/// coordinates throughout, no rescaling); the odd sizes here prove it.
fn areas() -> Vec<MonitorArea> {
    vec![
        area(
            "mon-a",
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1400,
            },
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440,
            },
        ),
        area(
            "mon-b",
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
    let correlation = CorrelationId::parse("cross-drag-seed").expect("correlation");
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

fn engine_row(token: &str, output: &str, workspace: &str) -> tiler_core::seed::EngineWindow {
    tiler_core::seed::EngineWindow {
        window: WindowId(token.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    }
}

/// Pair drag event without adjacency entries (pointer-resolved): the
/// destination domain carries the request, the combined rows ride
/// `windows`, and the source-first pair scopes the transaction. Drop
/// coordinates ride inside `command`.
#[allow(clippy::too_many_arguments)]
fn drag_pair_event(
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
    command: CoreCommand,
    correlation: &str,
) -> tiler_core::boundary::CoreEvent {
    let mut windows = Vec::new();
    for token in source_members {
        windows.push(engine_row(token, &source.1.output.0, &source.1.workspace.0));
    }
    for token in target_members {
        windows.push(engine_row(token, &target.1.output.0, &target.1.workspace.0));
    }
    tiler_core::boundary::CoreEvent {
        owner: state.owner.clone(),
        generation: state.generation.clone(),
        correlation: CorrelationId::parse(correlation).expect("correlation"),
        revision: 0,
        fingerprint: 7,
        domain: target.0.clone(),
        domain_key: target.1.clone(),
        outer_gap: 8,
        focused_window: WindowId(mover.to_owned()),
        windows,
        directional: Some(vec![
            (source.0.clone(), source.1.clone()),
            (target.0.clone(), target.1.clone()),
        ]),
        directional_target_outer_gap: Some(8),
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command,
    }
}

fn mover_key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 7000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

#[test]
fn occupied_pair_preview_drop_agree_and_transfer_without_hiding() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    // SourceBrowser domain bounds span the 2560-wide primary inset by 8;
    // the destination spans its panel-inset work area at negative x.
    let source_bounds = Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1384,
    };
    let dest_bounds = Rect {
        x: -1912,
        y: 8,
        w: 1904,
        h: 1024,
    };
    let source = seed(
        &mut state,
        "mon-a",
        &ws_a,
        source_bounds,
        &["mover", "sib"],
        "mover",
    );
    let target = seed(&mut state, "mon-b", &ws_b, dest_bounds, &["t1", "t2"], "t1");
    // Left edge of the destination bounds: the mover inserts before the
    // first destination tile.
    let (x, y) = (dest_bounds.x + 2, dest_bounds.y + dest_bounds.h / 2);
    let preview = drag_pair_event(
        &state,
        &source,
        &target,
        &["mover", "sib"],
        &["t1", "t2"],
        "mover",
        CoreCommand::DragPreview {
            window: "mover".to_owned(),
            x,
            y,
            hover_prior: None,
            source: Some(source.1.clone()),
        },
        "corr-drag-pair-pv",
    );
    let previewed = match state.engine.handle(&preview) {
        CoreReply::DragPreview(plan) => plan.preview,
        other => panic!("pair preview must project, got {other:?}"),
    };
    assert_eq!(previewed.domain, target.1);
    assert_eq!(previewed.source_window.0, "mover");
    let drop = drag_pair_event(
        &state,
        &source,
        &target,
        &["mover", "sib"],
        &["t1", "t2"],
        "mover",
        CoreCommand::DragDrop {
            window: "mover".to_owned(),
            x,
            y,
            hover_prior: None,
            source: Some(source.1.clone()),
        },
        "corr-drag-pair",
    );
    let mover_rect = match state.engine.handle(&drop) {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.kind, TiledKind::DragDrop);
            assert_eq!(plan.focus_domain, Some(target.1.clone()));
            plan.geometry
                .iter()
                .find(|g| g.window.0 == "mover")
                .expect("mover placement")
                .rect
        }
        other => panic!("pair drop must plan, got {other:?}"),
    };
    // Preview and drop agree exactly, inside the destination bounds.
    assert_eq!(mover_rect, previewed.proposed_rect);
    assert!(mover_rect.x >= dest_bounds.x);
    assert!(mover_rect.x + mover_rect.w <= dest_bounds.x + dest_bounds.w);
    // Project transfer without hiding or view switches: the mover leaves
    // the source set, joins the destination set, stays visible, and both
    // outputs keep their current workspaces.
    let mover = mover_key(0x3101);
    assert!(
        state
            .workspaces
            .assign(mover.clone(), "mon-a", &ws_a, false)
    );
    let peer = mover_key(0x3102);
    assert!(state.workspaces.assign(peer, "mon-a", &ws_a, false));
    let host = mover_key(0x3103);
    assert!(state.workspaces.assign(host, "mon-b", &ws_b, false));
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
    assert!(!state.workspaces.is_hidden(&mover));
    assert_eq!(
        state.workspaces.active_id("mon-a").as_deref(),
        Some(ws_a.as_str())
    );
    assert_eq!(
        state.workspaces.active_id("mon-b").as_deref(),
        Some(ws_b.as_str())
    );
}

#[test]
fn empty_singleton_admits_then_source_reconciles() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let source_bounds = Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1384,
    };
    let dest_bounds = Rect {
        x: -1912,
        y: 8,
        w: 1904,
        h: 1024,
    };
    let source = seed(
        &mut state,
        "mon-a",
        &ws_a,
        source_bounds,
        &["mover", "sib"],
        "mover",
    );
    // No destination session: the empty flow seeds fresh.
    assert!(
        !state
            .engine
            .contains(&crate::workspace_owner::workspace_domain("mon-b", &ws_b, dest_bounds, 8).1)
    );
    let (dest_domain, dest_key) =
        crate::workspace_owner::workspace_domain("mon-b", &ws_b, dest_bounds, 8);
    let mover_frame = Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let correlation = CorrelationId::parse("corr-drag-empty").expect("correlation");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        0,
        crate::tiling::fingerprint(&[("mover".to_owned(), mover_frame)]),
        &dest_domain,
        &dest_key,
        8,
        &[(
            WindowId("mover".to_owned()),
            mover_frame,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        )],
        Some(&WindowId("mover".to_owned())),
    );
    event.command = CoreCommand::DragDrop {
        window: "mover".to_owned(),
        x: dest_bounds.x + 10,
        y: dest_bounds.y + 10,
        hover_prior: None,
        source: Some(source.1.clone()),
    };
    let sole = match state.engine.handle(&event) {
        CoreReply::Tiled(plan) => plan,
        other => panic!("empty drop must admit the singleton, got {other:?}"),
    };
    assert_eq!(sole.geometry.len(), 1);
    assert_eq!(sole.geometry[0].rect, dest_bounds);
    // Source reconcile without the mover removes it Engine-side and
    // reflows the survivor over the full source bounds.
    let correlation = CorrelationId::parse("corr-drag-empty-src").expect("correlation");
    let mut reconcile = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        state
            .engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        crate::tiling::fingerprint(&[("sib".to_owned(), source_bounds)]),
        &source.0,
        &source.1,
        8,
        &[(
            WindowId("sib".to_owned()),
            source_bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        )],
        Some(&WindowId("sib".to_owned())),
    );
    reconcile.command = CoreCommand::Reconcile;
    match state.engine.handle(&reconcile) {
        CoreReply::Tiled(_) | CoreReply::Projection(_) => {}
        other => panic!("source reconcile must converge, got {other:?}"),
    }
    let source_windows: Vec<String> = state
        .engine
        .session(&source.1)
        .expect("source kept")
        .snapshot()
        .windows
        .iter()
        .map(|l| l.window.0.clone())
        .collect();
    assert_eq!(source_windows, vec!["sib".to_owned()]);
}

#[test]
fn stale_pair_keys_and_missing_destination_refuse() {
    let mut state = test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    let source_bounds = Rect {
        x: 8,
        y: 8,
        w: 2544,
        h: 1384,
    };
    let dest_bounds = Rect {
        x: -1912,
        y: 8,
        w: 1904,
        h: 1024,
    };
    let source = seed(
        &mut state,
        "mon-a",
        &ws_a,
        source_bounds,
        &["mover", "sib"],
        "mover",
    );
    let target = seed(&mut state, "mon-b", &ws_b, dest_bounds, &["t1"], "t1");
    // Reversed pair order disagrees with the request addressing.
    let mut swapped = drag_pair_event(
        &state,
        &source,
        &target,
        &["mover", "sib"],
        &["t1"],
        "mover",
        CoreCommand::DragDrop {
            window: "mover".to_owned(),
            x: dest_bounds.x + 2,
            y: dest_bounds.y + 10,
            hover_prior: None,
            source: Some(source.1.clone()),
        },
        "corr-drag-swapped",
    );
    swapped.directional = Some(vec![
        (target.0.clone(), target.1.clone()),
        (source.0.clone(), source.1.clone()),
    ]);
    match state.engine.handle(&swapped) {
        CoreReply::SnapshotInvalid { detail, .. } => assert_eq!(detail, "domain-invalid"),
        other => panic!("swapped pair must refuse, got {other:?}"),
    }
    // A destination with no retained session misrouted into the pair
    // path refuses instead of fabricating membership.
    let (missing_domain, missing_key) =
        crate::workspace_owner::workspace_domain("mon-z", "ws-9", dest_bounds, 8);
    let misrouted = drag_pair_event(
        &state,
        &source,
        &(missing_domain, missing_key),
        &["mover", "sib"],
        &[],
        "mover",
        CoreCommand::DragDrop {
            window: "mover".to_owned(),
            x: dest_bounds.x + 2,
            y: dest_bounds.y + 10,
            hover_prior: None,
            source: Some(source.1.clone()),
        },
        "corr-drag-missing",
    );
    match state.engine.handle(&misrouted) {
        CoreReply::SnapshotInvalid { detail, .. } => {
            assert_eq!(detail, "canonical-target-unavailable")
        }
        other => panic!("missing destination must refuse, got {other:?}"),
    }
}

#[test]
fn drop_destination_routing_observes_pointer_output_current_view() {
    let mut state = test_state();
    let all = areas();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let ws_a = state.workspaces.active_id("mon-a").expect("active");
    let ws_b = state.workspaces.active_id("mon-b").expect("active");
    state.workspaces.set_tiled("mon-a", &ws_a, true);
    state.workspaces.set_tiled("mon-b", &ws_b, true);
    // Same-output points keep the local route.
    assert!(matches!(
        resolve_drop_destination(&state, &all, "mon-a", 100, 100),
        DropDestination::SameOutput
    ));
    // Pointer on the secondary joins its current workspace.
    match resolve_drop_destination(&state, &all, "mon-a", -100, 100) {
        DropDestination::Cross { output, workspace } => {
            assert_eq!(output, "mon-b");
            assert_eq!(workspace, ws_b);
        }
        other => panic!("secondary point must cross, got {other:?}"),
    }
    // Panel strip: inside FULL but outside the inset work area.
    assert!(matches!(
        resolve_drop_destination(&state, &all, "mon-a", -100, 1070),
        DropDestination::Refused("outside-work-area")
    ));
    // Outside every output refuses.
    assert!(matches!(
        resolve_drop_destination(&state, &all, "mon-a", 9999, 9999),
        DropDestination::Refused("outside-output")
    ));
    // Floating destination refuses.
    state.workspaces.set_tiled("mon-b", &ws_b, false);
    assert!(matches!(
        resolve_drop_destination(&state, &all, "mon-a", -100, 100),
        DropDestination::Refused("floating-dest")
    ));
    state.workspaces.set_tiled("mon-b", &ws_b, true);
    // Unknown pointer output (in areas, never managed) has no view.
    let mut extended = all.clone();
    extended.push(area(
        "mon-ghost",
        Rect {
            x: 2560,
            y: 0,
            w: 800,
            h: 600,
        },
        Rect {
            x: 2560,
            y: 0,
            w: 800,
            h: 600,
        },
    ));
    assert!(matches!(
        resolve_drop_destination(&state, &extended, "mon-a", 2600, 100),
        DropDestination::Refused("no-view")
    ));
}

#[test]
fn drag_arrival_needs_planned_rect_or_clamped_effective_on_target() {
    use tiler_core::boundary::TiledPlan;
    use tiler_core::contract::LIFECYCLE_POLICY_VERSION;
    let mut state = test_state();
    let all = areas();
    let planned = Rect {
        x: -1912,
        y: 8,
        w: 952,
        h: 1024,
    };
    let reply = CoreReply::Tiled(TiledPlan {
        base_revision: 2,
        policy_version: LIFECYCLE_POLICY_VERSION,
        kind: TiledKind::DragDrop,
        geometry: vec![tiler_core::session::DesiredGeometry {
            window: WindowId("mover".to_owned()),
            leaf: NodeId("leaf-mover".to_owned()),
            output: OutputId("mon-b".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            rect: planned,
            overconstrained: false,
            client_clamped: false,
        }],
        focus_domain: Some(tiler_core::session::DomainKey {
            output: OutputId("mon-b".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        }),
        focus_leaf: Some(NodeId("leaf-mover".to_owned())),
        float_window: None,
        float_rect: None,
    });
    // Missing readback never proves arrival.
    assert!(!directional_arrival_verified(
        &state, &all, &reply, "mover", 0x3201, "mon-b"
    ));
    // Exact readback on the destination proves it.
    state.stable.insert(0x3201, planned);
    assert!(directional_arrival_verified(
        &state, &all, &reply, "mover", 0x3201, "mon-b"
    ));
    // Clamped readback proves only the hint-effective rect: the raw
    // planned rect alone is not proof when overconstrained.
    let mut clamped = reply.clone();
    let CoreReply::Tiled(plan) = &mut clamped else {
        panic!("tiled reply");
    };
    plan.geometry[0].overconstrained = true;
    state.hint_logged.insert(
        "mover".to_owned(),
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(1000),
            min_h: None,
            max_w: None,
            max_h: None,
        },
    );
    let effective = crate::tiling::overconstrained_effective(
        planned,
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(1000),
            min_h: None,
            max_w: None,
            max_h: None,
        },
    );
    assert_ne!(effective, planned);
    state.stable.insert(0x3201, planned);
    assert!(
        !directional_arrival_verified(&state, &all, &clamped, "mover", 0x3201, "mon-b"),
        "raw planned rect must not prove a clamped arrival"
    );
    state.stable.insert(0x3201, effective);
    assert!(directional_arrival_verified(
        &state, &all, &clamped, "mover", 0x3201, "mon-b"
    ));
    state.hint_logged.remove("mover");
}

#[test]
fn gesture_classification_keeps_native_move_apart_from_resize() {
    use crate::tiling::{GestureIntent, classify_gesture};
    // Native title-bar drags move the frame before END: same size far
    // from the source origin (even onto another output) is a move drop
    // at the release cursor, never a resize.
    let before = Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let moved = Rect {
        x: -1800,
        y: 50,
        w: 400,
        h: 300,
    };
    assert_eq!(
        classify_gesture(&before, &moved, Some((-1700, 200))),
        Some(GestureIntent::MoveDrop { x: -1700, y: 200 })
    );
    // Any size change is a resize even across outputs.
    let resized = Rect {
        x: -1800,
        y: 50,
        w: 500,
        h: 300,
    };
    assert!(matches!(
        classify_gesture(&before, &resized, Some((-1700, 200))),
        Some(GestureIntent::PointerResize { .. })
    ));
    // Zero movement routes nothing.
    assert_eq!(classify_gesture(&before, &before, Some((100, 100))), None);
}
