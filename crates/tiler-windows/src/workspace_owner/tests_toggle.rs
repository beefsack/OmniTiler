use super::*;

#[test]
fn toggle_float_first_float_centers_sixty_percent() {
    // Tiled-to-float with no rect selects the centered 60% work-area
    // fallback (KDE parity): the reply carries the placement for native
    // actuation while sibling reflow excludes the floated window.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
    let event = float_event(
        &owner,
        &generation,
        "float-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), bounds, false),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "w1",
        "w1",
        None,
    );
    let reply = engine.handle(&event);
    let CoreReply::Tiled(plan) = reply else {
        panic!("float commits, got {reply:?}");
    };
    // Centered 60% of the 800x600 domain: 480x360 at (160, 120).
    let centered = Rect {
        x: 160,
        y: 120,
        w: 480,
        h: 360,
    };
    assert_eq!(plan.float_rect, Some(centered));
    assert!(
        !plan.geometry.iter().any(|g| g.window.0 == "w1"),
        "floated window leaves the tree"
    );
    assert!(
        plan.geometry.iter().any(|g| g.window.0 == "w2"),
        "sibling reflows"
    );
}

#[test]
fn toggle_float_retains_moved_frame() {
    // Unfloat carries the live frame rect so a user moved/resized float is
    // retained: floating again with no rect reuses the moved frame, never
    // the centered fallback.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
    let float = |engine: &tiler_core::engine::Engine,
                 rows: &[(WindowId, Rect, bool)],
                 correlation: &str,
                 window: &str,
                 float_rect: Option<Rect>| {
        float_event(
            &owner,
            &generation,
            correlation,
            revision_of(engine, &domain),
            &domain,
            rows,
            window,
            window,
            float_rect,
        )
    };
    let moved = Rect {
        x: 210,
        y: 150,
        w: 480,
        h: 360,
    };
    let reply = engine.handle(&float(
        &engine,
        &[
            (WindowId("w1".to_owned()), bounds, false),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "float-1",
        "w1",
        None,
    ));
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "float commits, got {reply:?}"
    );
    let reply = engine.handle(&float(
        &engine,
        &[
            (WindowId("w1".to_owned()), moved, true),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "float-2",
        "w1",
        Some(moved),
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("unfloat commits, got {reply:?}");
    };
    assert_eq!(plan.float_rect, None, "unfloat stages no float rect");
    assert!(
        plan.geometry.iter().any(|g| g.window.0 == "w2"),
        "sibling survives readmission"
    );
    let reply = engine.handle(&float(
        &engine,
        &[
            (
                WindowId("w1".to_owned()),
                plan.geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("admission places the mover")
                    .rect,
                false,
            ),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "float-3",
        "w1",
        None,
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("refloat commits, got {reply:?}");
    };
    assert_eq!(plan.float_rect, Some(moved));
}

#[allow(clippy::too_many_arguments)]
fn orientation_event(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &str,
    revision: u64,
    domain: &(OutputDomain, DomainKey),
    rows: &[(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )],
    focused: &str,
    window: &str,
) -> tiler_core::boundary::CoreEvent {
    use tiler_core::boundary::CoreCommand;
    let correlation = CorrelationId::parse(correlation).expect("correlation");
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(token, rect, _, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rows.to_vec();
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        owner,
        generation,
        &correlation,
        revision,
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        Some(&WindowId(focused.to_owned())),
    );
    event.command = CoreCommand::ToggleOrientation {
        window: window.to_owned(),
    };
    event
}

fn seed_pair_for_orientation(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    domain: &(OutputDomain, DomainKey),
    bounds: Rect,
    focused: &str,
) {
    use tiler_core::boundary::CoreReply;
    for (correlation, tokens, focus) in [
        ("ori-seed-1", vec!["w1"], "w1"),
        ("ori-seed-2", vec!["w1", "w2"], focused),
    ] {
        let rows: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = tokens
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
        let correlation_id = CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation_id,
            revision_of(engine, domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &rows,
            Some(&WindowId(focus.to_owned())),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed {correlation} converges, got {reply:?}"
        );
    }
}

fn root_group_of(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
) -> (tiler_core::directional::Axis, Vec<String>, Vec<u64>) {
    use tiler_core::directional::Node;
    let tree = engine
        .session(&domain.1)
        .expect("session")
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    match tree {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => (
            axis,
            children.iter().map(|child| child.id().0.clone()).collect(),
            shares,
        ),
        Node::Leaf { .. } => panic!("expected a root group, got a lone leaf"),
    }
}

fn focus_leaf_of(
    engine: &tiler_core::engine::Engine,
    domain: &(OutputDomain, DomainKey),
) -> Option<String> {
    engine
        .session(&domain.1)
        .expect("session")
        .focus()
        .1
        .map(|leaf| leaf.0.clone())
}

#[test]
fn toggle_orientation_flips_root_horizontal_to_vertical() {
    // Wide pair admits an H root; Win+O flips it to V, preserving child
    // order, shares, and focus with a matching Tiled kind.
    use tiler_core::boundary::{CoreReply, TiledKind};
    use tiler_core::directional::Axis;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
    let (axis_before, order_before, shares_before) = root_group_of(&engine, &domain);
    assert_eq!(axis_before, Axis::Horizontal);
    let focus_before = focus_leaf_of(&engine, &domain);
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ],
        "w1",
        "w1",
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("toggle commits, got {reply:?}");
    };
    assert_eq!(plan.kind, TiledKind::ToggleOrientation);
    let (axis_after, order_after, shares_after) = root_group_of(&engine, &domain);
    assert_eq!(axis_after, Axis::Vertical);
    assert_eq!(order_after, order_before, "child order retained");
    assert_eq!(shares_after, shares_before, "shares retained");
    assert_eq!(
        focus_leaf_of(&engine, &domain),
        focus_before,
        "focus retained"
    );
    assert_eq!(plan.focus_leaf.map(|leaf| leaf.0), focus_before);
    assert_eq!(plan.geometry.len(), 2);
}

#[test]
fn toggle_orientation_flips_root_vertical_to_horizontal() {
    // Tall pair admits a V root; Win+O flips it to H.
    use tiler_core::boundary::{CoreReply, TiledKind};
    use tiler_core::directional::Axis;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 800,
    };
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w2");
    let (axis_before, _, _) = root_group_of(&engine, &domain);
    assert_eq!(axis_before, Axis::Vertical);
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ],
        "w2",
        "w2",
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("toggle commits, got {reply:?}");
    };
    assert_eq!(plan.kind, TiledKind::ToggleOrientation);
    let (axis_after, _, _) = root_group_of(&engine, &domain);
    assert_eq!(axis_after, Axis::Horizontal);
}

#[test]
fn toggle_orientation_flips_only_the_nested_immediate_parent() {
    // H[w1 V[w2 w3]] with w3 focused: only the inner V flips to H; the
    // root stays H with order/shares/focus retained.
    use tiler_core::boundary::{CoreReply, TiledKind};
    use tiler_core::directional::{Axis, Node};
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let domain = workspace_domain("mon-a", "ws-1", bounds, 0);
    let wide = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let tall = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 120,
    };
    let none = tiler_core::size_hints::WindowSizeHints::none();
    for (correlation, rows, focused) in [
        (
            "ori-seed-1",
            vec![(WindowId("w1".to_owned()), wide, none, false)],
            "w1",
        ),
        (
            "ori-seed-2",
            vec![
                (WindowId("w1".to_owned()), wide, none, false),
                (WindowId("w2".to_owned()), wide, none, false),
            ],
            "w2",
        ),
        (
            "ori-seed-3",
            vec![
                (WindowId("w1".to_owned()), wide, none, false),
                (WindowId("w2".to_owned()), wide, none, false),
                (WindowId("w3".to_owned()), tall, none, false),
            ],
            "w3",
        ),
    ] {
        let correlation_id = CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation_id,
            revision_of(&engine, &domain),
            fp,
            &domain.0,
            &domain.1,
            0,
            &rows,
            Some(&WindowId(focused.to_owned())),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed {correlation} converges, got {reply:?}"
        );
    }
    let (root_before, _, shares_before) = root_group_of(&engine, &domain);
    assert_eq!(root_before, Axis::Horizontal);
    let focus_before = focus_leaf_of(&engine, &domain);
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-nested",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), wide, none, false),
            (WindowId("w2".to_owned()), wide, none, false),
            (WindowId("w3".to_owned()), tall, none, false),
        ],
        "w3",
        "w3",
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("toggle commits, got {reply:?}");
    };
    assert_eq!(plan.kind, TiledKind::ToggleOrientation);
    let tree = engine
        .session(&domain.1)
        .expect("session")
        .snapshot()
        .domains
        .into_iter()
        .next()
        .expect("domain")
        .tree
        .expect("tree");
    match tree {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(axis, Axis::Horizontal, "root stays H");
            assert_eq!(shares, shares_before, "root shares retained");
            assert_eq!(children.len(), 2);
            assert_eq!(children[0].id().0, "leaf-w1");
            match &children[1] {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Horizontal, "inner V flips to H");
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-w2");
                    assert_eq!(children[1].id().0, "leaf-w3");
                }
                other => panic!("expected inner H[w2 w3], got {other:?}"),
            }
        }
        other => panic!("expected root H[w1 H[w2 w3]], got {other:?}"),
    }
    assert_eq!(
        focus_leaf_of(&engine, &domain),
        focus_before,
        "focus retained"
    );
    assert_eq!(plan.geometry.len(), 3);
}

#[test]
fn toggle_orientation_twice_restores_exact_tree_with_unequal_shares() {
    // A resize makes root shares unequal; two toggles restore the exact
    // pre-toggle tree, geometry, and focus.
    use tiler_core::boundary::{CoreCommand, CoreReply, TiledKind};
    use tiler_core::directional::Axis;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w2");
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    let resize = |engine: &tiler_core::engine::Engine, correlation: &str| {
        let correlation_id = CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation_id,
            revision_of(engine, &domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &rows,
            Some(&WindowId("w2".to_owned())),
        );
        event.command = CoreCommand::Resize {
            window: "w2".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        };
        event
    };
    let reply = engine.handle(&resize(&engine, "ori-resize-1"));
    assert!(
        matches!(reply, CoreReply::Resize(_)),
        "resize commits, got {reply:?}"
    );
    let (_, _, shares) = root_group_of(&engine, &domain);
    assert_eq!(shares.len(), 2);
    assert_ne!(
        shares[0], shares[1],
        "resize makes root shares unequal: {shares:?}"
    );
    let before = root_group_of(&engine, &domain);
    assert_eq!(before.0, Axis::Horizontal);
    let focus_before = focus_leaf_of(&engine, &domain);
    let geometry_before: Vec<(String, Rect)> = match engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-first",
        revision_of(&engine, &domain),
        &domain,
        &rows,
        "w2",
        "w2",
    )) {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.kind, TiledKind::ToggleOrientation);
            plan.geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect()
        }
        reply => panic!("first toggle commits, got {reply:?}"),
    };
    let mid = root_group_of(&engine, &domain);
    assert_eq!(mid.0, Axis::Vertical);
    assert_eq!(mid.1, before.1, "order retained through the flip");
    assert_eq!(mid.2, before.2, "unequal shares retained through the flip");
    // Second toggle restores the exact pre-toggle tree, geometry, focus.
    let geometry_second: Vec<(String, Rect)> = match engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-second",
        revision_of(&engine, &domain),
        &domain,
        &rows,
        "w2",
        "w2",
    )) {
        CoreReply::Tiled(plan) => {
            assert_eq!(plan.kind, TiledKind::ToggleOrientation);
            plan.geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect()
        }
        reply => panic!("second toggle commits, got {reply:?}"),
    };
    // Geometry rects in the toggle plans describe the flipped allocation;
    // the committed tree is the exact roundtrip signal.
    let after = root_group_of(&engine, &domain);
    assert_eq!(after, before, "double toggle restores the exact tree");
    assert_eq!(
        focus_leaf_of(&engine, &domain),
        focus_before,
        "focus retained through both toggles"
    );
    assert_ne!(
        geometry_before, geometry_second,
        "the two toggles allocate opposite axes"
    );
}

#[test]
fn toggle_orientation_honors_minimum_allocation() {
    // A satisfiable minimum on w1 takes slack through the hints-aware
    // projector, exactly like every other workflow.
    use tiler_core::boundary::{CoreReply, TiledKind};
    use tiler_core::size_hints::WindowSizeHints;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
    let hints = WindowSizeHints {
        min_w: None,
        min_h: Some(400),
        max_w: None,
        max_h: None,
    };
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-min",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), bounds, hints, false),
            (
                WindowId("w2".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            ),
        ],
        "w1",
        "w1",
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("toggle commits, got {reply:?}");
    };
    assert_eq!(plan.kind, TiledKind::ToggleOrientation);
    let placed = plan
        .geometry
        .iter()
        .find(|g| g.window.0 == "w1")
        .expect("w1 placed");
    assert!(
        placed.rect.h >= 400,
        "minimum takes slack: {:?}",
        placed.rect
    );
    assert!(
        plan.geometry.iter().all(|g| !g.overconstrained),
        "satisfiable minima stay unconstrained"
    );
}

#[test]
fn toggle_orientation_lone_leaf_noop_then_long_edge_admission() {
    // A sole root leaf refuses as unchanged with no plan; admitting a
    // second window afterwards still uses the long-edge rule (no saved
    // orientation hint from the no-op).
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let correlation_id = CorrelationId::parse("ori-lone-seed").expect("correlation");
    let seed_rows = [(WindowId("w1".to_owned()), bounds, none, false)];
    let fp = crate::tiling::fingerprint(
        &seed_rows
            .iter()
            .map(|(token, rect, _, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let seed = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation_id,
        revision_of(&engine, &domain),
        fp,
        &domain.0,
        &domain.1,
        8,
        &seed_rows,
        Some(&WindowId("w1".to_owned())),
    );
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "lone seed converges, got {reply:?}"
    );
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-lone",
        revision_of(&engine, &domain),
        &domain,
        &seed_rows,
        "w1",
        "w1",
    ));
    match reply {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        reply => panic!("lone toggle refuses unchanged, got {reply:?}"),
    }
    // Fresh admission beside the lone leaf still splits the long edge.
    let admit_rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    let admit_id = CorrelationId::parse("ori-lone-admit").expect("correlation");
    let admit_fp = crate::tiling::fingerprint(
        &admit_rows
            .iter()
            .map(|(token, rect, _, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let admit = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &admit_id,
        revision_of(&engine, &domain),
        admit_fp,
        &domain.0,
        &domain.1,
        8,
        &admit_rows,
        Some(&WindowId("w2".to_owned())),
    );
    let reply = engine.handle(&admit);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "admission converges, got {reply:?}"
    );
    let (axis, _, _) = root_group_of(&engine, &domain);
    assert_eq!(
        axis,
        tiler_core::directional::Axis::Horizontal,
        "wide bounds still admit along the long edge"
    );
}

#[test]
fn toggle_orientation_refuses_non_tiled_subjects() {
    // Float subjects, focus mismatches, and unknown windows refuse with
    // no plan: the adapter settles these as no-write outcomes.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    // Float w1, then toggle it: not tiled.
    let reply = engine.handle(&float_event(
        &owner,
        &generation,
        "ori-float-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), bounds, false),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "w1",
        "w1",
        None,
    ));
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "float commits, got {reply:?}"
    );
    let floated_rows = [
        (WindowId("w1".to_owned()), bounds, none, true),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-float-toggle",
        revision_of(&engine, &domain),
        &domain,
        &floated_rows,
        "w1",
        "w1",
    ));
    match reply {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "not-tiled"),
        reply => panic!("float toggle refuses not-tiled, got {reply:?}"),
    }
    // Unfloat w1 again so later legs run tiled.
    let reply = engine.handle(&float_event(
        &owner,
        &generation,
        "ori-unfloat-1",
        revision_of(&engine, &domain),
        &domain,
        &[
            (WindowId("w1".to_owned()), bounds, true),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "w1",
        "w1",
        Some(bounds),
    ));
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "unfloat commits, got {reply:?}"
    );
    // Toggle w2 while w1 is focused: focus mismatch.
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-mismatch",
        revision_of(&engine, &domain),
        &domain,
        &rows,
        "w1",
        "w2",
    ));
    match reply {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "focus-mismatch"),
        reply => panic!("mismatched toggle refuses, got {reply:?}"),
    }
    // Toggle an unknown window: unknown window.
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-unknown",
        revision_of(&engine, &domain),
        &domain,
        &rows,
        "w1",
        "w9",
    ));
    match reply {
        CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unknown-window"),
        reply => panic!("unknown toggle refuses, got {reply:?}"),
    }
}

#[test]
fn toggle_orientation_fail_closed_on_focused_overlay_flags() {
    // A focused window observed with overlay flags refuses with no plan.
    // Production never sends flags (the adapter zeroes them and refuses
    // focused overlays first with orientation vocabulary); this pins the
    // shared fail-closed fence behind it.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    for (correlation, flag) in [
        ("ori-overlay-max", "maximized"),
        ("ori-overlay-full", "fullscreen"),
    ] {
        let mut event = orientation_event(
            &owner,
            &generation,
            correlation,
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w1",
            "w1",
        );
        let focused = event
            .windows
            .iter_mut()
            .find(|w| w.window.0 == "w1")
            .expect("focused row");
        if flag == "maximized" {
            focused.maximized = true;
        } else {
            focused.fullscreen = true;
        }
        match engine.handle(&event) {
            CoreReply::Rejected { .. } => {}
            reply => panic!("flagged focus refuses, got {reply:?}"),
        }
    }
}

#[test]
fn toggle_orientation_plan_carries_every_tile_for_write_scoping() {
    // The ToggleOrientation plan reprojects every tile (sibling reserved
    // slots included); the portable writable fence scopes native writes
    // to fresh non-hidden members, so hidden siblings take no writes.
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use tiler_core::boundary::{CoreReply, TiledKind};
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
    let none = tiler_core::size_hints::WindowSizeHints::none();
    let rows = [
        (WindowId("w1".to_owned()), bounds, none, false),
        (WindowId("w2".to_owned()), bounds, none, false),
    ];
    let reply = engine.handle(&orientation_event(
        &owner,
        &generation,
        "ori-scope",
        revision_of(&engine, &domain),
        &domain,
        &rows,
        "w1",
        "w1",
    ));
    let CoreReply::Tiled(plan) = reply else {
        panic!("toggle commits, got {reply:?}");
    };
    assert_eq!(plan.kind, TiledKind::ToggleOrientation);
    let writes = super::super::planned_writes(&CoreReply::Tiled(plan)).expect("plan extracts");
    assert_eq!(writes.len(), 2, "both tiles reproject");
    // Hidden members converge membership but never take geometry writes.
    let members: BTreeSet<WindowKey> = [key(1), key(2)].into_iter().collect();
    let mut token_of = BTreeMap::new();
    token_of.insert(key(1), "w1".to_owned());
    let fresh: HashSet<String> = ["w1".to_owned()].into_iter().collect();
    let scoped = super::super::writable_subset(&members, |k| *k == key(2), &token_of, &fresh);
    assert_eq!(scoped, fresh, "hidden w2 takes no writes");
}

#[test]
fn hidden_snapshot_rect_prefers_live_float_then_tiled() {
    // Intentional floats ride the live float snapshot; every other member
    // (including born holds, which never seed a float snapshot) rides the
    // tiled allocation. Each lane falls back to the other snapshot so a
    // hidden member is never dropped while either exists.
    use super::super::hidden_snapshot_rect;
    let float_rect = Some(Rect {
        x: 1,
        y: 2,
        w: 3,
        h: 4,
    });
    let member_rect = Some(Rect {
        x: 5,
        y: 6,
        w: 7,
        h: 8,
    });
    assert_eq!(
        hidden_snapshot_rect(true, float_rect, member_rect),
        float_rect
    );
    assert_eq!(hidden_snapshot_rect(true, None, member_rect), member_rect);
    assert_eq!(
        hidden_snapshot_rect(false, float_rect, member_rect),
        member_rect
    );
    assert_eq!(hidden_snapshot_rect(false, float_rect, None), float_rect);
    assert_eq!(hidden_snapshot_rect(true, None, None), None);
    assert_eq!(hidden_snapshot_rect(false, None, None), None);
}

#[test]
fn unfloat_matches_fresh_admission_topology() {
    // H[w1 V[w2 w3]] floats w3 to H[w1 w2]; unfloating w3 must equal a
    // fresh admission of a new window into H[w1 w2]: same tree topology
    // and same admitted geometry, never the old slot or domain bounds.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::{Axis, Node};
    fn wide() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        }
    }
    fn tall() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        }
    }
    fn same_topology(left: &Node, right: &Node) -> bool {
        match (left, right) {
            (Node::Leaf { id: l }, Node::Leaf { id: r }) => l == r,
            (
                Node::Group {
                    axis: l_axis,
                    children: l_children,
                    shares: l_shares,
                    ..
                },
                Node::Group {
                    axis: r_axis,
                    children: r_children,
                    shares: r_shares,
                    ..
                },
            ) => {
                l_axis == r_axis
                    && l_shares == r_shares
                    && l_children.len() == r_children.len()
                    && l_children
                        .iter()
                        .zip(r_children.iter())
                        .all(|(l, r)| same_topology(l, r))
            }
            _ => false,
        }
    }
    fn tree_of(engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)) -> Node {
        engine
            .session(&domain.1)
            .expect("session")
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree")
    }
    fn assert_nested_h_v(tree: &Node) {
        match tree {
            Node::Group { axis, children, .. } => {
                assert_eq!(*axis, Axis::Horizontal);
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].id().0, "leaf-w1");
                match &children[1] {
                    Node::Group { axis, children, .. } => {
                        assert_eq!(*axis, Axis::Vertical);
                        assert_eq!(children.len(), 2);
                        assert_eq!(children[0].id().0, "leaf-w2");
                        assert_eq!(children[1].id().0, "leaf-w3");
                    }
                    other => panic!("expected inner V[w2 w3], got {other:?}"),
                }
            }
            other => panic!("expected root H[w1 V[w2 w3]], got {other:?}"),
        }
    }
    fn reconcile(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &(OutputDomain, DomainKey),
        correlation: &str,
        rows: &[(WindowId, Rect)],
        focused: &str,
    ) -> CoreReply {
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|(token, rect)| {
                (
                    token.clone(),
                    *rect,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                )
            })
            .collect();
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation,
            revision_of(engine, domain),
            fp,
            &domain.0,
            &domain.1,
            0,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        engine.handle(&event)
    }
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let domain = workspace_domain("mon-a", "ws-1", bounds, 0);
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    let w3 = WindowId("w3".to_owned());
    // Engine A: seed H[w1 V[w2 w3]], float w3, unfloat with the live frame.
    let mut engine_a = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine_a.sync_binding(&owner, &generation);
    for (correlation, rows, focused) in [
        ("seed-1", vec![(w1.clone(), wide())], "w1"),
        (
            "seed-2",
            vec![(w1.clone(), wide()), (w2.clone(), wide())],
            "w2",
        ),
        (
            "seed-3",
            vec![
                (w1.clone(), wide()),
                (w2.clone(), wide()),
                (w3.clone(), tall()),
            ],
            "w3",
        ),
    ] {
        let reply = reconcile(
            &mut engine_a,
            &owner,
            &generation,
            &domain,
            correlation,
            &rows,
            focused,
        );
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "seed {correlation} converges, got {reply:?}"
        );
    }
    assert_nested_h_v(&tree_of(&engine_a, &domain));
    let reply = engine_a.handle(&float_event(
        &owner,
        &generation,
        "float-a",
        revision_of(&engine_a, &domain),
        &domain,
        &[
            (w1.clone(), wide(), false),
            (w2.clone(), wide(), false),
            (w3.clone(), tall(), false),
        ],
        "w3",
        "w3",
        None,
    ));
    let CoreReply::Tiled(float_plan) = reply else {
        panic!("float commits, got {reply:?}");
    };
    let live = float_plan.float_rect.expect("float places");
    let reply = engine_a.handle(&float_event(
        &owner,
        &generation,
        "unfloat-a",
        revision_of(&engine_a, &domain),
        &domain,
        &[
            (w1.clone(), wide(), false),
            (w2.clone(), wide(), false),
            (w3.clone(), live, true),
        ],
        "w3",
        "w3",
        Some(live),
    ));
    let CoreReply::Tiled(unfloat_plan) = reply else {
        panic!("unfloat commits, got {reply:?}");
    };
    let admitted_a = unfloat_plan
        .geometry
        .iter()
        .find(|g| g.window.0 == "w3")
        .expect("admission places w3")
        .rect;
    let tree_a = tree_of(&engine_a, &domain);
    assert_nested_h_v(&tree_a);
    // Engine B: seed H[w1 w2], then freshly admit w3 with the same frame.
    let mut engine_b = tiler_core::engine::Engine::new();
    engine_b.sync_binding(&owner, &generation);
    for (correlation, rows, focused) in [
        ("seed-1", vec![(w1.clone(), wide())], "w1"),
        (
            "seed-2",
            vec![(w1.clone(), wide()), (w2.clone(), wide())],
            "w2",
        ),
    ] {
        let reply = reconcile(
            &mut engine_b,
            &owner,
            &generation,
            &domain,
            correlation,
            &rows,
            focused,
        );
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "seed {correlation} converges, got {reply:?}"
        );
    }
    let reply = reconcile(
        &mut engine_b,
        &owner,
        &generation,
        &domain,
        "admit-b",
        &[
            (w1.clone(), wide()),
            (w2.clone(), wide()),
            (w3.clone(), live),
        ],
        "w3",
    );
    assert!(
        matches!(
            reply,
            CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
        ),
        "fresh admission converges, got {reply:?}"
    );
    let tree_b = tree_of(&engine_b, &domain);
    assert!(
        same_topology(&tree_a, &tree_b),
        "unfloat reuses the fresh admission axis: {tree_a:?} vs {tree_b:?}"
    );
    // Admitted geometry matches the same fresh admission: compare via the
    // converged session projection rather than the old slot.
    let geometry_b: Vec<(String, Rect)> = match reply {
        CoreReply::Projection(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect(),
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect(),
        CoreReply::SendWorkspace(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect(),
        other => panic!("fresh admission converges, got {other:?}"),
    };
    let admitted_b = geometry_b
        .iter()
        .find(|(token, _)| token == "w3")
        .expect("fresh admission places w3")
        .1;
    assert_eq!(admitted_a, admitted_b, "unfloat places like a new window");
}
