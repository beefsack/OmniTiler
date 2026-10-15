use super::*;

fn d16_float_frame() -> Rect {
    Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    }
}

/// Build a D16 send event for the floated mover with the given observed
/// overlay shape, through the production wire helpers (`build_send_event`
/// plus the send-specific maximized stamp; fullscreen set on the mover
/// row like the fixed-admission stamp does production-side).
#[allow(clippy::too_many_arguments)]
fn d16_send_event(
    owner: &OwnerId,
    generation: &GenerationId,
    revision: u64,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    bounds: Rect,
    float_frame: Rect,
    mover_fullscreen: bool,
    mover_maximized: bool,
    follow: bool,
) -> tiler_core::boundary::CoreEvent {
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let source_rows = vec![
        OwnerRow {
            token: "wS".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
        OwnerRow {
            token: "wF".to_owned(),
            rect: float_frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: true,
        },
    ];
    let target_rows = vec![OwnerRow {
        token: "wT".to_owned(),
        rect: bounds,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let mut event = build_send_event(
        owner,
        generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &source_rows,
        &target_rows,
        "wF",
        8,
        follow,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    super::super::stamp_send_float_overlay(&mut event, "wF", true, mover_maximized);
    if mover_fullscreen {
        for entry in event
            .windows
            .iter_mut()
            .chain(event.target_windows.iter_mut())
        {
            if entry.window.0 == "wF" {
                entry.fullscreen = true;
            }
        }
    }
    event
}

#[test]
fn d16_ordinary_and_combo_carry_deferred_without_geometry_writes() {
    // Both combos x follow/stay through the real Engine: ordinary float,
    // floating+fullscreen, and floating+maximized all plan slotless
    // MoveDeferred with no leaf, keep the float frame verbatim, leave
    // both trees and every survivor allocation untouched, take no mover
    // geometry, and book follow/stay focus like the ordinary transfer.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    for (mover_fullscreen, mover_maximized, shape) in [
        (false, false, "ordinary"),
        (true, false, "fullscreen"),
        (false, true, "maximized"),
    ] {
        for follow in [true, false] {
            let mut engine = tiler_core::engine::Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let source = workspace_domain("mon-a", "ws-1", bounds, 8);
            let target = workspace_domain("mon-a", "ws-2", bounds, 8);
            seed_d16_float_domains(
                &mut engine,
                &owner,
                &generation,
                &source,
                &target,
                bounds,
                float_frame,
            );
            let tree_before_source = engine
                .session(&source.1)
                .map(|s| s.tree_for(&source.1).cloned());
            let tree_before_target = engine
                .session(&target.1)
                .map(|s| s.tree_for(&target.1).cloned());
            let revision = engine
                .session(&source.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            let event = d16_send_event(
                &owner,
                &generation,
                revision,
                &source,
                &target,
                bounds,
                float_frame,
                mover_fullscreen,
                mover_maximized,
                follow,
            );
            // Wire shape binds the exact deferred arm.
            let mover = event
                .windows
                .iter()
                .find(|w| w.window.0 == "wF")
                .expect("mover row");
            assert!(mover.floating, "{shape}: mover rides floating");
            assert_eq!(
                mover.fullscreen, mover_fullscreen,
                "{shape}: fullscreen rides the wire"
            );
            assert_eq!(
                mover.maximized, mover_maximized,
                "{shape}: maximized rides the send-specific path"
            );
            let reply = engine.handle(&event);
            let CoreReply::SendWorkspace(plan) = reply else {
                panic!("{shape} follow={follow}: expected SendWorkspace, got {reply:?}");
            };
            match &plan.operation {
                tiler_core::contract::LifecycleOperation::MoveDeferred {
                    window,
                    source_output,
                    source_workspace,
                    target_output,
                    target_workspace,
                } => {
                    assert_eq!(window.0, "wF", "{shape}: deferred mover");
                    assert_eq!(source_output.0, "mon-a");
                    assert_eq!(source_workspace.0, "ws-1");
                    assert_eq!(target_output.0, "mon-a");
                    assert_eq!(target_workspace.0, "ws-2");
                }
                other => panic!("{shape}: expected MoveDeferred, got {other:?}"),
            }
            assert_eq!(plan.follow, follow, "{shape}: intent echoes");
            // Membership-only: neither tree mutates and no allocation
            // reflows on either side.
            assert_eq!(
                engine
                    .session(&source.1)
                    .map(|s| s.tree_for(&source.1).cloned()),
                tree_before_source,
                "{shape} follow={follow}: source tree untouched"
            );
            assert_eq!(
                engine
                    .session(&target.1)
                    .map(|s| s.tree_for(&target.1).cloned()),
                tree_before_target,
                "{shape} follow={follow}: target tree untouched"
            );
            assert!(
                plan.geometry.iter().all(|g| g.window.0 != "wF"),
                "{shape} follow={follow}: slotless mover takes no allocation"
            );
            for survivor in ["wS", "wT"] {
                let entry = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == survivor)
                    .unwrap_or_else(|| panic!("{shape}: survivor {survivor} kept"));
                assert_eq!(entry.rect, bounds, "{shape}: {survivor} never reflows");
            }
            // Exact arrival: still floating on the destination with the
            // frame and restart memory verbatim.
            let session = engine.session(&target.1).expect("target session");
            assert!(
                session.is_exception(&WindowId("wF".to_owned())),
                "{shape}: mover stays floating on arrival"
            );
            assert_eq!(
                session.floating_geometry(&WindowId("wF".to_owned())),
                Some(float_frame),
                "{shape}: arrival frame exact"
            );
            assert_eq!(
                session.retained_float_geometry(&WindowId("wF".to_owned())),
                Some(float_frame),
                "{shape}: restart memory follows the window"
            );
            // Follow books the target MRU tile (never the slotless
            // mover); stay falls back through the source MRU.
            if follow {
                let focus = plan.focus_domain.clone().expect("follow focus");
                assert_eq!(focus, target.1, "{shape}: follow books the target");
            } else {
                let focus = plan.focus_domain.clone().expect("stay focus");
                assert_eq!(focus, source.1, "{shape}: stay books the source");
            }
        }
    }
}

#[test]
fn d16_roundtrip_retains_float_both_legs() {
    // Ordinary/combo WS1 -> WS2 -> WS1 roundtrip: both legs plan
    // MoveDeferred and the arrival frame/flags survive each leg.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    for (mover_fullscreen, mover_maximized, shape) in [
        (false, false, "ordinary"),
        (true, false, "fullscreen"),
        (false, true, "maximized"),
    ] {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let ws1 = workspace_domain("mon-a", "ws-1", bounds, 8);
        let ws2 = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_d16_float_domains(
            &mut engine,
            &owner,
            &generation,
            &ws1,
            &ws2,
            bounds,
            float_frame,
        );
        for (source, target, leg) in [(&ws1, &ws2, "out"), (&ws2, &ws1, "back")] {
            let revision = engine
                .session(&source.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            // Rebuild rows per leg: the mover rides the current source.
            let mover_on_source = engine
                .session(&source.1)
                .is_some_and(|s| s.is_exception(&WindowId("wF".to_owned())));
            assert!(mover_on_source, "{shape} {leg}: mover homed on source");
            let correlation = CorrelationId::parse("tick-1").expect("correlation");
            let (source_rows, target_rows) = if leg == "out" {
                (
                    vec![
                        OwnerRow {
                            token: "wS".to_owned(),
                            rect: bounds,
                            hints: tiler_core::size_hints::WindowSizeHints::none(),
                            floating: false,
                        },
                        OwnerRow {
                            token: "wF".to_owned(),
                            rect: float_frame,
                            hints: tiler_core::size_hints::WindowSizeHints::none(),
                            floating: true,
                        },
                    ],
                    vec![OwnerRow {
                        token: "wT".to_owned(),
                        rect: bounds,
                        hints: tiler_core::size_hints::WindowSizeHints::none(),
                        floating: false,
                    }],
                )
            } else {
                (
                    vec![
                        OwnerRow {
                            token: "wT".to_owned(),
                            rect: bounds,
                            hints: tiler_core::size_hints::WindowSizeHints::none(),
                            floating: false,
                        },
                        OwnerRow {
                            token: "wF".to_owned(),
                            rect: float_frame,
                            hints: tiler_core::size_hints::WindowSizeHints::none(),
                            floating: true,
                        },
                    ],
                    vec![OwnerRow {
                        token: "wS".to_owned(),
                        rect: bounds,
                        hints: tiler_core::size_hints::WindowSizeHints::none(),
                        floating: false,
                    }],
                )
            };
            let mut event = build_send_event(
                &owner,
                &generation,
                &correlation,
                revision,
                42,
                source.clone(),
                target.clone(),
                &source_rows,
                &target_rows,
                "wF",
                8,
                true,
            )
            .expect("event");
            stamp_send_target(&mut event, &target.1);
            super::super::stamp_send_float_overlay(&mut event, "wF", true, mover_maximized);
            if mover_fullscreen {
                for entry in event
                    .windows
                    .iter_mut()
                    .chain(event.target_windows.iter_mut())
                {
                    if entry.window.0 == "wF" {
                        entry.fullscreen = true;
                    }
                }
            }
            let reply = engine.handle(&event);
            let CoreReply::SendWorkspace(plan) = reply else {
                panic!("{shape} {leg}: expected SendWorkspace, got {reply:?}");
            };
            assert!(
                matches!(
                    plan.operation,
                    tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
                ),
                "{shape} {leg}: slotless deferred leg"
            );
            let session = engine.session(&target.1).expect("target session");
            assert!(
                session.is_exception(&WindowId("wF".to_owned())),
                "{shape} {leg}: still floating on arrival"
            );
            assert_eq!(
                session.floating_geometry(&WindowId("wF".to_owned())),
                Some(float_frame),
                "{shape} {leg}: exact arrival frame"
            );
        }
    }
}

#[test]
fn d16_refusals_sticky_automatic_fixed_and_dual_overlay() {
    // Sticky, automatic-fixed, and dual-overlay movers are ineligible
    // for the deferred transfer and the Engine refuses the send; the
    // production adapter gate owns the exact refusal vocabulary
    // (`send-refused-sticky` / `send-refused-floating`) before the
    // Engine. Ordinary tiled sends still plan MoveTiled.
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_d16_float_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        bounds,
        float_frame,
    );
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    // Eligibility first, session-level and exact: only the ordinary and
    // single-overlay shapes admit.
    let mover_of = |event: &tiler_core::boundary::CoreEvent| {
        event
            .windows
            .iter()
            .find(|w| w.window.0 == "wF")
            .expect("mover row")
            .clone()
    };
    let is_eligible = |session: &tiler_core::session::Session,
                       mover: &tiler_core::seed::EngineWindow| {
        d16_eligible(session, &source.1, mover)
    };
    let ordinary = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        false,
        false,
        true,
    );
    assert!(
        engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&ordinary))),
        "ordinary admits"
    );
    let mut fullscreen = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        true,
        false,
        true,
    );
    assert!(
        engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&fullscreen))),
        "fullscreen combo admits"
    );
    let maximized = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        false,
        true,
        true,
    );
    // The send-specific maximized stamp is what makes this shape bind.
    assert!(
        mover_of(&maximized).maximized,
        "maximized rides the send-specific path"
    );
    assert!(
        engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&maximized))),
        "maximized combo admits"
    );
    // Sticky mover: same floating row plus sticky on the wire refuses.
    for entry in fullscreen
        .windows
        .iter_mut()
        .chain(fullscreen.target_windows.iter_mut())
    {
        if entry.window.0 == "wF" {
            entry.sticky = true;
        }
    }
    assert!(
        !engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&fullscreen))),
        "sticky ineligible"
    );
    match engine.handle(&fullscreen) {
        CoreReply::Rejected { kind, .. } => assert!(
            kind == "not-tiled" || kind == "focus-mismatch",
            "sticky refuses, got {kind}"
        ),
        reply => panic!("sticky must refuse, got {reply:?}"),
    }
    // Automatic-fixed mover refuses.
    let mut automatic = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        false,
        false,
        true,
    );
    for entry in automatic
        .windows
        .iter_mut()
        .chain(automatic.target_windows.iter_mut())
    {
        if entry.window.0 == "wF" {
            entry.fixed_auto = true;
        }
    }
    assert!(
        !engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&automatic))),
        "automatic ineligible"
    );
    match engine.handle(&automatic) {
        CoreReply::Rejected { kind, .. } => assert!(
            kind == "not-tiled" || kind == "focus-mismatch",
            "automatic refuses, got {kind}"
        ),
        reply => panic!("automatic must refuse, got {reply:?}"),
    }
    // Dual overlay (floating + fullscreen + maximized) refuses.
    let mut dual = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        true,
        true,
        true,
    );
    for entry in dual
        .windows
        .iter_mut()
        .chain(dual.target_windows.iter_mut())
    {
        if entry.window.0 == "wF" {
            entry.fullscreen = true;
            entry.maximized = true;
        }
    }
    assert!(
        !engine
            .session(&source.1)
            .is_some_and(|s| is_eligible(s, &mover_of(&dual))),
        "dual overlay ineligible"
    );
    match engine.handle(&dual) {
        CoreReply::Rejected { kind, .. } => assert!(
            kind == "not-tiled" || kind == "focus-mismatch",
            "dual refuses, got {kind}"
        ),
        reply => panic!("dual overlay must refuse, got {reply:?}"),
    }
    // Ordinary tiled send is unchanged.
    let _ = maximized;
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let rows = vec![
        OwnerRow {
            token: "wS".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        },
        OwnerRow {
            token: "wF".to_owned(),
            rect: float_frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: true,
        },
    ];
    let mut tiled = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &rows,
        &[OwnerRow {
            token: "wT".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }],
        "wS",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut tiled, &target.1);
    // The tiled mover is the focused window for the ordinary route.
    tiled.focused_window = WindowId("wS".to_owned());
    if let CoreCommand::SendToWorkspace { window, .. } = &mut tiled.command {
        *window = "wS".to_owned();
    }
    match engine.handle(&tiled) {
        CoreReply::SendWorkspace(plan) => assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveTiled { .. }
            ),
            "tiled sends stay MoveTiled"
        ),
        reply => panic!("tiled send must plan, got {reply:?}"),
    }
}

#[test]
fn d16_fixed_hints_never_perturb_deferred_shape() {
    // Send flags ride the exact overlay snapshot; size hints stay
    // advisory. A mover carrying fixed-size hints with an ordinary
    // float shape still plans the slotless deferred transfer with the
    // frame verbatim (fixed opt-in off: hints never gate).
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    let fixed = tiler_core::size_hints::WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: Some(640),
        max_h: Some(480),
    };
    assert!(
        tiler_core::size_hints::is_fixed_size_with(
            fixed,
            tiler_core::size_hints::FixedSizePredicate::default(),
        ),
        "mover hints read fixed"
    );
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_d16_float_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        bounds,
        float_frame,
    );
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &[
            OwnerRow {
                token: "wS".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            OwnerRow {
                token: "wF".to_owned(),
                rect: float_frame,
                hints: fixed,
                floating: true,
            },
        ],
        &[OwnerRow {
            token: "wT".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }],
        "wF",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    let mover = event
        .windows
        .iter()
        .find(|w| w.window.0 == "wF")
        .expect("mover row");
    assert!(mover.floating, "float flag exact");
    assert!(
        !mover.fullscreen && !mover.maximized && !mover.sticky,
        "no overlay inferred from hints"
    );
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = reply else {
        panic!("hinted float send must plan, got {reply:?}");
    };
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
        ),
        "hints never perturb the deferred shape"
    );
    let session = engine.session(&target.1).expect("target session");
    assert!(
        session.is_exception(&WindowId("wF".to_owned())),
        "hinted mover stays floating on arrival"
    );
    assert_eq!(
        session.floating_geometry(&WindowId("wF".to_owned())),
        Some(float_frame),
        "arrival frame exact"
    );
}

#[test]
fn d16_sole_source_and_empty_target_carry_deferred() {
    // Sole-window source stay resolves the genuine null plan (no setter);
    // an empty target admits the slotless mover with no placement.
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    // Sole source: WS1 [wF] floated, WS2 [wT]; stay empties the source.
    {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        for (domain, tokens, focused) in [
            (&target, vec!["wT"], None),
            (&source, vec!["wF"], Some(WindowId("wF".to_owned()))),
        ] {
            let correlation = CorrelationId::parse("seed").expect("correlation");
            let revision = engine
                .session(&domain.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                revision,
                tokens.len() as u64,
                &domain.0,
                &domain.1,
                8,
                &tokens
                    .iter()
                    .map(|t| {
                        (
                            WindowId((*t).to_owned()),
                            bounds,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                focused.as_ref(),
            );
            let _ = engine.handle(&event);
        }
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let event = float_event(
            &owner,
            &generation,
            "float",
            revision,
            &source,
            &[(WindowId("wF".to_owned()), bounds, false)],
            "wF",
            "wF",
            Some(float_frame),
        );
        let _ = engine.handle(&event);
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &[OwnerRow {
                token: "wF".to_owned(),
                rect: float_frame,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: true,
            }],
            &[OwnerRow {
                token: "wT".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            }],
            "wF",
            8,
            false,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = reply else {
            panic!("sole stay must plan, got {reply:?}");
        };
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
            ),
            "sole float stay is deferred"
        );
        assert_eq!(plan.focus_domain, None, "sole stay has no focus domain");
        assert_eq!(plan.focus_leaf, None, "sole stay has no focus leaf");
        let plan_null = plan.focus_domain.is_none() || plan.focus_leaf.is_none();
        assert_eq!(
            super::super::classify_stay_focus(plan_null, false, false, false, false, false),
            super::super::StayFocusClass::Null,
            "sole deferred stay resolves Null with no setter"
        );
    }
    // Empty target: WS1 [wS, wF floated], WS2 [] admits slotless.
    {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            2,
            &source.0,
            &source.1,
            8,
            &[
                (
                    WindowId("wS".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
                (
                    WindowId("wF".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
            ],
            Some(&WindowId("wF".to_owned())),
        );
        let _ = engine.handle(&event);
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let event = float_event(
            &owner,
            &generation,
            "float",
            revision,
            &source,
            &[
                (WindowId("wS".to_owned()), bounds, false),
                (WindowId("wF".to_owned()), bounds, false),
            ],
            "wF",
            "wF",
            Some(float_frame),
        );
        let _ = engine.handle(&event);
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &[
                OwnerRow {
                    token: "wS".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
                OwnerRow {
                    token: "wF".to_owned(),
                    rect: float_frame,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: true,
                },
            ],
            &[],
            "wF",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = reply else {
            panic!("empty-target send must plan, got {reply:?}");
        };
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
            ),
            "empty target carries deferred"
        );
        assert!(
            plan.geometry.iter().all(|g| g.window.0 != "wF"),
            "slotless mover takes no allocation on an empty target"
        );
    }
}

#[test]
fn d16_destination_exit_still_floating_and_mover_never_writable() {
    // Destination overlay exit returns to the floating frame, never tiled
    // admission; the float mover never enters the writable subset.
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_d16_float_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        bounds,
        float_frame,
    );
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    // Maximized combo arrival first.
    let event = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        false,
        true,
        true,
    );
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = reply else {
        panic!("combo send must plan, got {reply:?}");
    };
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
        ),
        "combo arrival is deferred"
    );
    // Simulated native overlay exit through a real destination
    // re-observation: the target domain reconverges carrying the mover
    // with the exact live snapshot (still floating, overlays gone) at
    // the remembered frame beside the tiled survivor. The mover must
    // stay a slotless exception with its frame and restart memory
    // verbatim: never tiled admission, never a mover allocation.
    let revision = engine
        .session(&target.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let exit_fp =
        crate::tiling::fingerprint(&[("wT".to_owned(), bounds), ("wF".to_owned(), float_frame)]);
    let exit = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("tick-2").expect("correlation"),
        revision,
        exit_fp,
        &target.0,
        &target.1,
        8,
        &[
            (
                WindowId("wT".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("wF".to_owned()),
                float_frame,
                tiler_core::size_hints::WindowSizeHints::none(),
                true,
            ),
        ],
        Some(&WindowId("wT".to_owned())),
    );
    let reply = engine.handle(&exit);
    match &reply {
        CoreReply::Projection(_) => {}
        CoreReply::Tiled(plan) => {
            assert!(
                plan.geometry.iter().all(|g| g.window.0 != "wF"),
                "exited mover takes no allocation"
            );
            let survivor = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "wT")
                .expect("survivor kept");
            assert_eq!(survivor.rect, bounds, "survivor never reflows on exit");
        }
        other => panic!("exit reconverges, got {other:?}"),
    }
    let session = engine.session(&target.1).expect("target session");
    assert!(
        session.is_exception(&WindowId("wF".to_owned())),
        "exited mover stays floating, never tiled admission"
    );
    assert!(
        session
            .snapshot()
            .windows
            .iter()
            .all(|link| link.window.0 != "wF"),
        "no tiled link for the exited mover"
    );
    assert_eq!(
        session.floating_geometry(&WindowId("wF".to_owned())),
        Some(float_frame),
        "exit returns to the floating frame"
    );
    assert_eq!(
        session.retained_float_geometry(&WindowId("wF".to_owned())),
        Some(float_frame),
        "restart memory survives the exit"
    );
    // Writable subset never includes a transferred-out mover: after the
    // deferred carry the mover is absent from the source set, so the
    // source reflow scope (the same `writable_subset` production calls)
    // covers exactly the survivor while the moved token stays out.
    // Hidden targets never write either.
    let mover = key(21);
    let survivor = key(22);
    let source_members: BTreeSet<WindowKey> = [survivor.clone()].into_iter().collect();
    let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([
        (mover.clone(), "wF".to_owned()),
        (survivor.clone(), "wS".to_owned()),
    ]);
    let fresh: HashSet<String> = HashSet::from(["wF".to_owned(), "wS".to_owned()]);
    let writable = super::super::writable_subset(&source_members, |_| false, &token_of, &fresh);
    assert!(
        !writable.contains("wF"),
        "moved token not writable in source"
    );
    assert!(
        writable.contains("wS"),
        "visible source survivor stays writable"
    );
}

#[test]
fn d16_relative_target_resolves_then_carries_deferred() {
    // Numbered/relative sends share the Engine carry: the scoped ring
    // step from the mover source resolves the same destination id, and
    // the deferred transfer to that id plans MoveDeferred.
    use tiler_core::boundary::CoreReply;
    let bounds = rect(0, 0);
    let float_frame = d16_float_frame();
    let mut spaces = ManagedWorkspaces::new();
    spaces.ensure_output("mon-a");
    let ids = spaces.workspace_ids("mon-a");
    assert!(ids.len() >= 2, "ring needs source plus target");
    let source_id = ids[0].clone();
    let expected = spaces
        .resolve_relative_from("mon-a", &source_id, 1)
        .expect("relative target");
    assert_eq!(expected, ids[1], "ring step resolves the numbered neighbor");
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", &source_id, bounds, 8);
    let target = workspace_domain("mon-a", &expected, bounds, 8);
    seed_d16_float_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        bounds,
        float_frame,
    );
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let event = d16_send_event(
        &owner,
        &generation,
        revision,
        &source,
        &target,
        bounds,
        float_frame,
        false,
        false,
        true,
    );
    let reply = engine.handle(&event);
    assert!(
        matches!(
            reply,
            CoreReply::SendWorkspace(plan)
                if matches!(
                    plan.operation,
                    tiler_core::contract::LifecycleOperation::MoveDeferred { .. }
                )
        ),
        "relative-resolved target carries deferred"
    );
}

/// D16 deferred-send fixture: WS1 [wS survivor, wF mover floated with
/// the pinned frame], WS2 [wT]. Returns the Engine plus both domains.
fn seed_d16_float_domains(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    bounds: Rect,
    float_frame: Rect,
) {
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    // Target converges first so the source focus (the mover) is last.
    for (domain, tokens, focused) in [
        (target, vec!["wT"], None),
        (source, vec!["wS", "wF"], Some(WindowId("wF".to_owned()))),
    ] {
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let revision = engine
            .session(&domain.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for(
            owner,
            generation,
            &correlation,
            revision,
            tokens.len() as u64,
            &domain.0,
            &domain.1,
            8,
            &tokens
                .iter()
                .map(|t| {
                    (
                        WindowId((*t).to_owned()),
                        bounds,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            focused.as_ref(),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    // Floatify the mover with the pinned frame through the real Engine.
    let correlation = CorrelationId::parse("float").expect("correlation");
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let event = float_event(
        owner,
        generation,
        "float",
        revision,
        source,
        &[
            (WindowId("wS".to_owned()), bounds, false),
            (WindowId("wF".to_owned()), bounds, false),
        ],
        "wF",
        "wF",
        Some(float_frame),
    );
    let reply = engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "floatify commits, got {reply:?}"
    );
    assert!(
        engine
            .session(&source.1)
            .is_some_and(|s| s.is_exception(&WindowId("wF".to_owned()))),
        "mover floats slotless"
    );
    let _ = (correlation, bounds);
}

/// Session-level D16 eligibility probe for one mover wire row: the
/// survivor rides tiled beside it, exactly like the send event.
fn d16_eligible(
    session: &tiler_core::session::Session,
    source: &DomainKey,
    mover: &tiler_core::seed::EngineWindow,
) -> bool {
    use tiler_core::directional::WindowId;
    let observed = tiler_core::seed::observed_window_from_engine(mover);
    session.deferred_send_eligible(
        &WindowId("wF".to_owned()),
        source,
        &[
            tiler_core::session::ObservedWindow {
                window: WindowId("wS".to_owned()),
                output: source.output.clone(),
                workspace: source.workspace.clone(),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
            },
            observed,
        ],
    )
}
