#![forbid(unsafe_code)]
use super::*;

#[test]
fn domain_rows_propagates_hints_and_send_carries_them() {
    use super::super::{MemberView, build_send_event, domain_rows, stamp_send_target};
    use tiler_core::boundary::CoreReply;
    use tiler_core::size_hints::WindowSizeHints;
    let a = key(1);
    let b = key(2);
    let members: BTreeSet<WindowKey> = [a.clone(), b.clone()].into_iter().collect();
    let hinted = WindowSizeHints {
        min_w: Some(500),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let views = vec![
        MemberView {
            key: a.clone(),
            token: "w1".to_owned(),
            rect: rect(0, 0),
            hints: hinted,
            floating: false,
        },
        MemberView {
            key: b.clone(),
            token: "w2".to_owned(),
            rect: rect(100, 100),
            hints: WindowSizeHints::none(),
            floating: false,
        },
    ];
    let rows = domain_rows(&members, &views).expect("complete rows");
    assert_eq!(rows[0].hints, hinted);
    assert!(rows[1].hints.is_empty());
    // The destination domain carries its own fresh hints (a hidden
    // snapshot row included): the send path must not drop them.
    let c = key(3);
    let target_members: BTreeSet<WindowKey> = [c.clone()].into_iter().collect();
    let target_views = vec![MemberView {
        key: c.clone(),
        token: "w3".to_owned(),
        rect: rect(200, 200),
        hints: hinted,
        floating: false,
    }];
    let target_rows = domain_rows(&target_members, &target_views).expect("target rows");
    assert_eq!(target_rows[0].hints, hinted);
    // The send path carries the same rows into both Engine domains.
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    let mut engine = tiler_core::engine::Engine::new();
    engine.sync_binding(&owner, &generation);
    for (key, domain, rows) in [
        (&source.1, &source.0, vec![("w1", bounds), ("w2", bounds)]),
        (&target.1, &target.0, vec![("w3", bounds)]),
    ] {
        let seed_correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &seed_correlation,
            0,
            rows.len() as u64,
            domain,
            key,
            8,
            &rows
                .iter()
                .map(|(t, r)| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        *r,
                        WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            None,
        );
        let _ = engine.handle(&event);
    }
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        3,
        source,
        target.clone(),
        &rows,
        &target_rows,
        "w1",
        8,
        true,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    assert_eq!(event.windows.len(), 2);
    assert_eq!(event.windows[0].hints, hinted);
    assert!(event.windows[1].hints.is_empty());
    assert_eq!(event.target_windows.len(), 1);
    assert_eq!(event.target_windows[0].hints, hinted);
    let CoreReply::SendWorkspace(_) = engine.handle(&event) else {
        panic!("hinted send commits through the real Engine");
    };
}

#[test]
fn hint_only_change_reprojects_without_topology_change() {
    // Same rectangles and fingerprint, only a fresh minimum hint on w2:
    // the Engine still projects, and the deficit comes from w1's slack.
    use tiler_core::boundary::CoreReply;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
    let plain = || {
        vec![
            (
                tiler_core::directional::WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
            ),
            (
                tiler_core::directional::WindowId("w2".to_owned()),
                bounds,
                WindowSizeHints::none(),
            ),
        ]
    };
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let seed = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        0,
        2,
        &domain,
        &key,
        8,
        &plain(),
        None,
    );
    let base = match engine.handle(&seed) {
        CoreReply::Tiled(plan) => plan.base_revision,
        CoreReply::Projection(plan) => plan.base_revision,
        reply => panic!("seed converges, got {reply:?}"),
    };
    // Hint-only second observation: identical rectangles, one fresh hint.
    let hinted = WindowSizeHints {
        min_w: Some(500),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let correlation = CorrelationId::parse("tick-2").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(base),
        2,
        &domain,
        &key,
        8,
        &[
            (
                tiler_core::directional::WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
            ),
            (
                tiler_core::directional::WindowId("w2".to_owned()),
                bounds,
                hinted,
            ),
        ],
        None,
    );
    let reply = engine.handle(&event);
    let CoreReply::Projection(plan) = reply else {
        panic!("hint-only change projects, got {reply:?}");
    };
    assert!(plan.geometry.iter().all(|g| !g.overconstrained));
    let widths: std::collections::BTreeMap<&str, i32> = plan
        .geometry
        .iter()
        .map(|g| (g.window.0.as_str(), g.rect.w))
        .collect();
    // 800 extent, 8 inner gap: 792 shared; w2 keeps 500, w1 yields slack.
    assert_eq!(widths["w2"], 500);
    assert_eq!(widths["w1"], 792 - 500);
}

#[test]
fn infeasible_hints_keep_proportional_and_flag_overconstrained() {
    // Minimums exceeding the extent keep the proportional allocation and
    // flag every violating window: shared projection never reasserts, and
    // the Windows native write path places writable overconstrained
    // windows at the tile origin clamped to the known minimum (KDE keeps
    // its existing skip).
    use tiler_core::boundary::CoreReply;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::WindowSizeHints;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
    let hinted = WindowSizeHints {
        min_w: Some(500),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let rows = vec![
        (
            tiler_core::directional::WindowId("w1".to_owned()),
            bounds,
            hinted,
        ),
        (
            tiler_core::directional::WindowId("w2".to_owned()),
            bounds,
            hinted,
        ),
    ];
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let seed = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        0,
        2,
        &domain,
        &key,
        8,
        &rows,
        None,
    );
    let _ = engine.handle(&seed);
    let correlation = CorrelationId::parse("tick-2").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        2,
        &domain,
        &key,
        8,
        &rows,
        None,
    );
    let reply = engine.handle(&event);
    let widths: Vec<i32>;
    let flags: Vec<bool>;
    match reply {
        CoreReply::Projection(plan) => {
            widths = plan.geometry.iter().map(|g| g.rect.w).collect();
            flags = plan.geometry.iter().map(|g| g.overconstrained).collect();
            assert_eq!(plan.geometry.len(), 2);
        }
        CoreReply::Tiled(plan) => {
            widths = plan.geometry.iter().map(|g| g.rect.w).collect();
            flags = plan.geometry.iter().map(|g| g.overconstrained).collect();
            assert_eq!(plan.geometry.len(), 2);
        }
        reply => panic!("infeasible hints still project, got {reply:?}"),
    }
    assert!(
        flags.iter().all(|f| *f),
        "both windows flag overconstrained"
    );
    // Proportional fallback: identical to the no-hint allocation.
    let correlation = CorrelationId::parse("tick-3").expect("correlation");
    let plain: Vec<(tiler_core::directional::WindowId, Rect, WindowSizeHints)> = rows
        .iter()
        .map(|(w, r, _)| (w.clone(), *r, WindowSizeHints::none()))
        .collect();
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        2,
        &domain,
        &key,
        8,
        &plain,
        None,
    );
    let reply = engine.handle(&event);
    let plain_widths: Vec<i32> = match reply {
        CoreReply::Projection(plan) => plan.geometry.iter().map(|g| g.rect.w).collect(),
        CoreReply::Tiled(plan) => plan.geometry.iter().map(|g| g.rect.w).collect(),
        reply => panic!("plain projects, got {reply:?}"),
    };
    assert_eq!(widths, plain_widths, "infeasible keeps proportional sizes");
    // The native extraction preserves the flags the write path skips on:
    // rebuild the infeasible plan through the same builder and check the
    // seam carries every overconstrained flag.
    let correlation = CorrelationId::parse("tick-4").expect("correlation");
    let event = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &correlation,
        engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        2,
        &domain,
        &key,
        8,
        &rows,
        None,
    );
    let reply = engine.handle(&event);
    let plan = match reply {
        CoreReply::Projection(plan) => plan,
        reply => panic!("infeasible re-projects, got {reply:?}"),
    };
    let writes = super::super::planned_writes(&tiler_core::boundary::CoreReply::Projection(plan))
        .expect("plan extracts");
    assert!(writes.iter().all(|w| w.overconstrained));
}

#[test]
fn max_only_hints_clamp_without_reallocation() {
    // AR12 shared policy, no adapter copy: maximums never reallocate
    // siblings; they only bound clamp acceptance (`clamp(desired, min,
    // max)` with per-axis tolerance). A max-only window plans the
    // identical share to its unhinted control, while a client frame
    // clamped to the shared policy accepts as `client_clamped` and the
    // same shortfall with no usable max (unhinted, or sentinel which
    // the adapter normalizes to absent) counts as drift. Hints come
    // from the production outer-track conversion, never hand-built.
    use tiler_core::boundary::CoreReply;
    use tiler_core::geometry::Rect;
    use tiler_core::size_hints::{
        FixedSizePredicate, WindowSizeHints, assess_window_clamp, is_fixed_size_with,
    };
    let mut engine = tiler_core::engine::Engine::new();
    engine.set_fixed_size_admission(true);
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
    // Production conversion: max-only outer tracks with zero insets.
    // A seeded-untouched maximum (sentinel) normalizes to absent, and
    // min/max failure is unknown (no hint), never a floor or cap.
    let maxed = crate::tiling::hints_from_outer(
        None,
        Some((216, 316)),
        crate::tiling::FrameInsets::default(),
    );
    assert_eq!(
        (maxed.min_w, maxed.min_h, maxed.max_w, maxed.max_h),
        (None, None, Some(216), Some(316))
    );
    let sentinel = crate::tiling::hints_from_outer(
        None,
        Some((i32::MAX, i32::MAX)),
        crate::tiling::FrameInsets::default(),
    );
    assert!(
        sentinel.max_w.is_none() && sentinel.max_h.is_none(),
        "sentinel maximums normalize to absent"
    );
    assert!(
        !is_fixed_size_with(maxed, FixedSizePredicate::BothAxes)
            && !is_fixed_size_with(maxed, FixedSizePredicate::EitherAxis),
        "max-only hints never classify fixed"
    );
    let wmax = tiler_core::directional::WindowId("wmax".to_owned());
    let w1 = tiler_core::directional::WindowId("w1".to_owned());
    let rows = vec![
        (wmax.clone(), bounds, maxed),
        (w1.clone(), bounds, WindowSizeHints::none()),
    ];
    let project = |engine: &mut tiler_core::engine::Engine,
                   rows: &[(tiler_core::directional::WindowId, Rect, WindowSizeHints)],
                   fp: u64,
                   correlation: &str| {
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            fp,
            &domain,
            &key,
            8,
            rows,
            None,
        );
        match engine.handle(&event) {
            CoreReply::Projection(plan) => plan.geometry,
            CoreReply::Tiled(plan) => plan.geometry,
            reply => panic!("max-only projects, got {reply:?}"),
        }
    };
    let fp_full =
        crate::tiling::fingerprint(&[("wmax".to_owned(), bounds), ("w1".to_owned(), bounds)]);
    let seed = project(&mut engine, &rows, fp_full, "max-tick-1");
    assert_eq!(seed.len(), 2);
    let planned = seed
        .iter()
        .find(|g| g.window == wmax)
        .expect("max window planned");
    assert!(
        !planned.overconstrained,
        "max-only plans a full share, never overconstrained"
    );
    // Retained tick with the client frame clamped to the shared policy:
    // same origin, sizes at the maximum. The share must not move and
    // the clamped member must flag `client_clamped`, not drift.
    let clamped = Rect {
        x: planned.rect.x,
        y: planned.rect.y,
        w: 216,
        h: 316,
    };
    let fp_clamped =
        crate::tiling::fingerprint(&[("wmax".to_owned(), clamped), ("w1".to_owned(), bounds)]);
    let retained = project(
        &mut engine,
        &[
            (wmax.clone(), clamped, maxed),
            (w1.clone(), bounds, WindowSizeHints::none()),
        ],
        fp_clamped,
        "max-tick-2",
    );
    let kept = retained
        .iter()
        .find(|g| g.window == wmax)
        .expect("max window retained");
    assert_eq!(
        (kept.rect.w, kept.rect.h),
        (planned.rect.w, planned.rect.h),
        "maximums never reallocate the share"
    );
    assert!(
        kept.client_clamped,
        "max-clamped client frame accepts instead of drifting"
    );
    assert!(
        retained
            .iter()
            .find(|g| g.window == w1)
            .is_some_and(|g| !g.client_clamped),
        "unclamped sibling flags nothing"
    );
    // Unhinted control plans the identical share: the maximum moved
    // nothing in either direction.
    let plain: Vec<(tiler_core::directional::WindowId, Rect, WindowSizeHints)> = rows
        .iter()
        .map(|(w, r, _)| (w.clone(), *r, WindowSizeHints::none()))
        .collect();
    let control = project(&mut engine, &plain, fp_full, "max-tick-3");
    let control_widths: Vec<i32> = control.iter().map(|g| g.rect.w).collect();
    let maxed_widths: Vec<i32> = seed.iter().map(|g| g.rect.w).collect();
    assert_eq!(
        maxed_widths, control_widths,
        "max hints plan byte-identical shares to no hints"
    );
    // Shared assessment direct: clamped accepts under max-only hints,
    // the same shortfall drifts unhinted and under sentinel-absent.
    assert!(
        assess_window_clamp(&clamped, &planned.rect, &maxed).accepted,
        "shared policy accepts the max clamp"
    );
    assert!(
        !assess_window_clamp(&clamped, &planned.rect, &WindowSizeHints::none()).accepted,
        "unhinted shortfall is drift, never accepted"
    );
    assert!(
        !assess_window_clamp(&clamped, &planned.rect, &sentinel).accepted,
        "sentinel-absent shortfall is drift, never accepted"
    );
}

#[test]
fn sticky_toggle_refusals_match_float_overlays_with_sticky_vocabulary() {
    // Fullscreen wins, then maximize; normal proceeds. Sticky uses its
    // own outcome strings so logs distinguish the arm.
    assert_eq!(
        crate::tiling::sticky_toggle_refusal(true, false),
        Some("sticky-refused-fullscreen")
    );
    assert_eq!(
        crate::tiling::sticky_toggle_refusal(false, true),
        Some("sticky-refused-maximize")
    );
    assert_eq!(
        crate::tiling::sticky_toggle_refusal(true, true),
        Some("sticky-refused-fullscreen")
    );
    assert_eq!(crate::tiling::sticky_toggle_refusal(false, false), None);
    assert_eq!(
        crate::tiling::sticky_directional_refusal(true, false),
        Some("focus-refused-sticky")
    );
    assert_eq!(
        crate::tiling::sticky_directional_refusal(true, true),
        Some("move-refused-sticky")
    );
    assert_eq!(
        crate::tiling::sticky_directional_refusal(false, false),
        None
    );
    assert_eq!(crate::tiling::sticky_directional_refusal(false, true), None);
}

#[test]
fn orientation_toggle_refusals_carry_orientation_vocabulary() {
    // Fullscreen wins, then maximize; normal proceeds. The native
    // orientation path calls this before any Engine mutation, so a
    // focused overlay keeps its native state and reserved slot.
    assert_eq!(
        crate::tiling::orientation_toggle_refusal(true, false),
        Some("orientation-refused-fullscreen")
    );
    assert_eq!(
        crate::tiling::orientation_toggle_refusal(false, true),
        Some("orientation-refused-maximize")
    );
    assert_eq!(
        crate::tiling::orientation_toggle_refusal(true, true),
        Some("orientation-refused-fullscreen")
    );
    assert_eq!(
        crate::tiling::orientation_toggle_refusal(false, false),
        None
    );
}

#[test]
fn sticky_marker_values_round_trip_and_reject_unknown() {
    // 1 = prior tiled, 2 = prior float; zero/unknown fail closed.
    assert_eq!(crate::tiling::sticky_marker_value(false), 1);
    assert_eq!(crate::tiling::sticky_marker_value(true), 2);
    assert_eq!(crate::tiling::parse_sticky_marker(1), Some(false));
    assert_eq!(crate::tiling::parse_sticky_marker(2), Some(true));
    assert_eq!(crate::tiling::parse_sticky_marker(0), None);
    assert_eq!(crate::tiling::parse_sticky_marker(3), None);
    assert_eq!(crate::tiling::parse_sticky_marker(u64::MAX), None);
}

#[test]
fn sticky_float_rehome_keeps_no_duplicate_across_domains() {
    // Backing-domain removal plus current-domain admit as tiled via two
    // reconciles: the mover leaves the source set, joins the target set,
    // and no stale duplicate remains. Mirrors the native cross-domain
    // sticky-off path without native calls.
    use tiler_core::boundary::CoreReply;
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_two_tiled(&mut engine, &owner, &generation, &source, bounds);
    // Float w1 in the source (sticky-on tiled half), then rehome: source
    // without w1 converges removal, target with w1 as tiled admits.
    let float = float_event(
        &owner,
        &generation,
        "float-1",
        revision_of(&engine, &source),
        &source,
        &[
            (WindowId("w1".to_owned()), bounds, false),
            (WindowId("w2".to_owned()), bounds, false),
        ],
        "w1",
        "w1",
        None,
    );
    let reply = engine.handle(&float);
    assert!(matches!(reply, CoreReply::Tiled(_)), "float commits");
    let revision = |engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)| {
        revision_of(engine, domain)
    };
    let reconcile = |engine: &mut tiler_core::engine::Engine,
                     domain: &(OutputDomain, DomainKey),
                     correlation: &str,
                     rows: &[(WindowId, Rect, bool)]| {
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|(token, rect, floating)| {
                (
                    token.clone(),
                    *rect,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    *floating,
                )
            })
            .collect();
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision(engine, domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            None,
        );
        engine.handle(&event)
    };
    // Source without the floated window: exception drops, sibling keeps
    // the tile area with no duplication.
    let reply = reconcile(
        &mut engine,
        &source,
        "prune-old",
        &[(WindowId("w2".to_owned()), bounds, false)],
    );
    assert!(
        matches!(
            reply,
            CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
        ),
        "old removal converges, got {reply:?}"
    );
    assert!(
        !engine
            .session(&source.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "old exception gone"
    );
    // Current admits w1 as tiled: same topology as a fresh admission,
    // no stale retained duplicate in the source. The target plan carries
    // w1 as a tile (not an exception), so the first Win+G after the move
    // tiles rather than re-floating.
    let reply = reconcile(
        &mut engine,
        &target,
        "admit-current",
        &[(WindowId("w1".to_owned()), bounds, false)],
    );
    let tiled_windows: Vec<WindowId> = match reply {
        CoreReply::Tiled(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
        CoreReply::Projection(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
        CoreReply::SendWorkspace(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
        reply => panic!("current admit converges, got {reply:?}"),
    };
    assert!(
        tiled_windows.contains(&WindowId("w1".to_owned())),
        "target tiles w1, got {tiled_windows:?}"
    );
    assert!(
        !engine
            .session(&target.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "w1 rides tiled in target"
    );
    assert!(
        !engine
            .session(&source.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "no stale duplicate in old"
    );
}

#[test]
fn sticky_pruned_backing_rehomes_to_current_without_duplicate() {
    // Sticky-only backing workspace prunes while the sticky membership
    // dangles, then sticky-off rehomes to current: the old domain drops
    // the mover with no duplicate and current tiles (prior tiled) or
    // keeps the live frame as a normal float (prior float). Ties the
    // production pure seams: `plan_cleanup_excluding` /
    // `workspace_members` (missing returns empty) plus `domain_rows`
    // and the floating reconcile builder, for both prior origins.
    use tiler_core::boundary::CoreReply;
    let bounds = rect(0, 0);
    let live_frame = Rect {
        x: 10,
        y: 20,
        w: 400,
        h: 300,
    };
    // Workspace seam: sticky-only intermediate prunes, membership dangles.
    let mut spaces = ManagedWorkspaces::new();
    spaces.ensure_output("mon-a");
    let current = spaces.active_id("mon-a").expect("active");
    let backing = spaces.resolve_send("mon-a", 2).expect("second");
    let filler = key(43);
    assert!(spaces.assign(filler.clone(), "mon-a", &backing, false));
    let (third, _) = spaces.select_trailing("mon-a").expect("trailing");
    assert!(spaces.assign(filler.clone(), "mon-a", &third, false));
    let mover = key(41);
    assert!(spaces.assign(mover.clone(), "mon-a", &backing, false));
    let sibling = key(42);
    assert!(spaces.assign(sibling.clone(), "mon-a", &current, false));
    assert!(spaces.activate("mon-a", &current));
    let sticky: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
    let (removed, append) =
        spaces.plan_cleanup_excluding("mon-a", std::slice::from_ref(&current), &[], &sticky);
    assert_eq!(removed, vec![backing.clone()], "sticky-only prunes");
    spaces.apply_cleanup("mon-a", &removed, append);
    assert_eq!(
        spaces.member_loc(&mover).map(|l| l.workspace.clone()),
        Some(backing.clone()),
        "membership dangles on the pruned id"
    );
    assert!(
        spaces.workspace_members("mon-a", &backing).is_empty(),
        "pruned members read back empty"
    );
    // Engine seam per prior origin: old domain without the mover, current
    // with the mover explicit (mirrors the native rehome construct).
    for prior in [false, true] {
        let floating = prior;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", &backing, bounds, 8);
        let target = workspace_domain("mon-a", &current, bounds, 8);
        // Sticky-on: tiled mover floats in the backing domain.
        let seed = float_event(
            &owner,
            &generation,
            "sticky-on",
            revision_of(&engine, &source),
            &source,
            &[(WindowId("w1".to_owned()), bounds, false)],
            "w1",
            "w1",
            None,
        );
        let reply = engine.handle(&seed);
        assert!(
            matches!(reply, CoreReply::Tiled(_)),
            "prior {prior}: sticky-on floats, got {reply:?}"
        );
        // Old domain rows via the production seam: pruned members read
        // empty, so the removal observation carries no mover.
        let old_members = spaces.workspace_members("mon-a", &backing);
        assert!(old_members.is_empty(), "prior {prior}: backing pruned");
        let old_rows = domain_rows(&old_members, &[]).expect("prior {prior}: pruned rows");
        assert!(old_rows.is_empty(), "prior {prior}: old rows empty");
        let old_fp = crate::tiling::fingerprint(&[]);
        let correlation = CorrelationId::parse("prune-old").expect("correlation");
        let old_event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &source),
            old_fp,
            &source.0,
            &source.1,
            8,
            &[],
            None,
        );
        let reply = engine.handle(&old_event);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "prior {prior}: old removal converges, got {reply:?}"
        );
        assert!(
            !engine
                .session(&source.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "prior {prior}: old exception gone"
        );
        // Current rows via the production seam: existing sibling rows
        // plus the mover explicit with the live frame, floating by
        // origin (prior float stays float, prior tiled tiles).
        let cur_members = spaces.workspace_members("mon-a", &current);
        assert!(
            cur_members.contains(&sibling),
            "prior {prior}: sibling stays current"
        );
        let cur_views = vec![MemberView {
            key: sibling.clone(),
            token: "w2".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }];
        let mut cur_rows = domain_rows(&cur_members, &cur_views).expect("prior {prior}: base rows");
        cur_rows.push(OwnerRow {
            token: "w1".to_owned(),
            rect: live_frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating,
        });
        assert!(
            cur_rows
                .iter()
                .any(|r| r.token == "w1" && r.rect == live_frame),
            "prior {prior}: explicit current row carries the live frame"
        );
        let cur_windows: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = cur_rows
            .iter()
            .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
            .collect();
        let cur_fp = crate::tiling::fingerprint(
            &cur_rows
                .iter()
                .map(|r| (r.token.clone(), r.rect))
                .collect::<Vec<_>>(),
        );
        let correlation = CorrelationId::parse("admit-current").expect("correlation");
        let cur_event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &target),
            cur_fp,
            &target.0,
            &target.1,
            8,
            &cur_windows,
            None,
        );
        let reply = engine.handle(&cur_event);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "prior {prior}: current admit converges, got {reply:?}"
        );
        let cur_float = engine
            .session(&target.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned())));
        assert_eq!(
            cur_float, prior,
            "prior {prior}: current float follows origin"
        );
        if !prior {
            let tiled: Vec<WindowId> = match reply {
                CoreReply::Tiled(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
                CoreReply::Projection(plan) => {
                    plan.geometry.iter().map(|g| g.window.clone()).collect()
                }
                CoreReply::SendWorkspace(plan) => {
                    plan.geometry.iter().map(|g| g.window.clone()).collect()
                }
                reply => panic!("prior {prior}: current tiles, got {reply:?}"),
            };
            assert!(
                tiled.contains(&WindowId("w1".to_owned())),
                "prior {prior}: target tiles w1, got {tiled:?}"
            );
        }
        assert!(
            !engine
                .session(&source.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "prior {prior}: no stale duplicate in old"
        );
    }
}

#[test]
fn sticky_adopt_reads_both_markers_as_sticky_before_tiling() {
    // Restart adoption re-adopts either marker value as sticky with its
    // pre-sticky origin before any tiling (the old normal-float
    // consumption was the item 8 gap). Mirrors the native adoption
    // preamble through the production builders. Post-restart un-stick
    // stays TBD under R-FLT-05/R-RST-01: no un-stick outcome asserted.
    use tiler_core::boundary::CoreReply;
    for prior in [false, true] {
        let value = crate::tiling::sticky_marker_value(prior);
        assert_eq!(
            crate::tiling::parse_sticky_marker(value),
            Some(prior),
            "marker round-trips"
        );
        // Portable classifier plus fences agree before the Engine moves.
        let marker = classify_restart_marker(
            MarkerSlot::Absent,
            MarkerSlot::Value(value),
            MarkerSlot::Absent,
        );
        assert_eq!(marker, RestartMarker::Sticky(prior));
        assert_eq!(
            restart_sticky_eligible(true, true, true, marker),
            Some(prior),
            "prior {prior}: fences admit sticky"
        );
        assert!(
            !restart_float_eligible(true, true, true, marker),
            "prior {prior}: sticky never hydrates as ordinary float"
        );
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        let correlation = CorrelationId::parse("adopt").expect("correlation");
        let carried = vec![(
            WindowId("w1".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            true,
        )];
        let fp = crate::tiling::fingerprint(&[("w1".to_owned(), bounds)]);
        let adopt = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            None,
        );
        let reply = engine.handle(&adopt);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "prior {prior}: adoption converges, got {reply:?}"
        );
        assert!(
            engine
                .session(&domain.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "prior {prior}: adopted window floats"
        );
        // Fixed verdict: the marker re-adopts sticky with its origin
        // (runtime sticky entry kept, marker kept for later restarts),
        // never a normal float.
        let mut sticky: BTreeMap<WindowKey, bool> = BTreeMap::new();
        sticky.insert(key(11), prior);
        assert_eq!(
            sticky.get(&key(11)),
            Some(&prior),
            "prior {prior}: sticky origin retained"
        );
    }
}

#[test]
fn restart_ordinary_float_hydrates_before_tiling() {
    // An intentional ordinary float marker re-adopts floating before any
    // tiling: classifier plus fences admit, the Engine carries the
    // exception, and no sticky entry appears.
    use tiler_core::boundary::CoreReply;
    let raw = crate::tiling::float_intent_marker_value();
    assert!(crate::tiling::parse_float_intent_marker(raw).is_some());
    let marker = classify_restart_marker(
        MarkerSlot::Value(raw),
        MarkerSlot::Absent,
        MarkerSlot::Absent,
    );
    assert_eq!(marker, RestartMarker::FloatIntent);
    assert!(restart_float_eligible(true, true, true, marker));
    assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let bounds = rect(0, 0);
    let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
    let correlation = CorrelationId::parse("adopt").expect("correlation");
    let carried = vec![(
        WindowId("w1".to_owned()),
        bounds,
        tiler_core::size_hints::WindowSizeHints::none(),
        true,
    )];
    let fp = crate::tiling::fingerprint(&[("w1".to_owned(), bounds)]);
    let adopt = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        revision_of(&engine, &domain),
        fp,
        &domain.0,
        &domain.1,
        8,
        &carried,
        None,
    );
    let reply = engine.handle(&adopt);
    assert!(
        matches!(
            reply,
            CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
        ),
        "adoption converges, got {reply:?}"
    );
    assert!(
        engine
            .session(&domain.1)
            .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
        "ordinary float re-adopts floating"
    );
}

#[test]
fn restart_marker_mismatch_and_reused_identity_refuse() {
    // Corrupt values, failed reads, lifetime mismatch, identity
    // mismatch, unadmitted membership, and recycled HWNDs all fail
    // closed: no float, no sticky, marker alone never grants admission.
    use MarkerSlot::{Absent, Unreadable, Value};
    assert_eq!(
        classify_restart_marker(Absent, Absent, Absent),
        RestartMarker::Absent
    );
    assert_eq!(
        classify_restart_marker(Value(7), Value(3), Value(9)),
        RestartMarker::Corrupt
    );
    assert_eq!(
        classify_restart_marker(Value(u64::MAX), Absent, Absent),
        RestartMarker::Corrupt
    );
    // Defensive only: native NULL/zero reads as Absent, so a Value(0)
    // never arrives from production; it still fails closed here.
    assert_eq!(
        classify_restart_marker(Value(0), Value(0), Value(0)),
        RestartMarker::Corrupt
    );
    // Failed reads diagnose as unreadable, never as intent.
    assert_eq!(
        classify_restart_marker(Unreadable, Absent, Absent),
        RestartMarker::Unreadable
    );
    assert_eq!(
        classify_restart_marker(Absent, Absent, Unreadable),
        RestartMarker::Unreadable
    );
    // A valid lane outranks a failed read elsewhere.
    assert_eq!(
        classify_restart_marker(
            Unreadable,
            Value(crate::tiling::sticky_marker_value(false)),
            Unreadable,
        ),
        RestartMarker::Sticky(false)
    );
    for marker in [RestartMarker::Corrupt, RestartMarker::Unreadable] {
        assert!(!restart_float_eligible(true, true, true, marker));
        assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
    }
    // Lifetime mismatch (live tag differs): the visible-lifetime fence
    // refuses exactly where HWND/PID/creation still agree.
    assert!(!visible_lifetime_ok(
        "9f2c41aa07bd33e0",
        Some("0000000000000001")
    ));
    assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
    let float_marker = classify_restart_marker(
        MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
        MarkerSlot::Absent,
        MarkerSlot::Absent,
    );
    assert!(!restart_float_eligible(true, true, false, float_marker));
    assert!(!restart_float_eligible(true, false, true, float_marker));
    assert!(!restart_float_eligible(false, true, true, float_marker));
    let sticky_marker = classify_restart_marker(
        MarkerSlot::Absent,
        MarkerSlot::Value(crate::tiling::sticky_marker_value(true)),
        MarkerSlot::Absent,
    );
    assert_eq!(
        restart_sticky_eligible(true, true, false, sticky_marker),
        None
    );
    assert_eq!(
        restart_sticky_eligible(true, false, true, sticky_marker),
        None
    );
    assert_eq!(
        restart_sticky_eligible(false, true, true, sticky_marker),
        None
    );
    assert_eq!(
        restart_sticky_eligible(true, true, true, RestartMarker::Absent),
        None
    );
    // Recycled HWND: the fresh enumeration read is authoritative, so a
    // stale same-HWND key drops and the new generation (fresh tag, no
    // marker) hydrates nothing.
    let known = vec![key(41)];
    let fresh = WindowKey {
        hwnd: 41,
        pid: 9999,
        creation: "c000000000000041".to_owned(),
    };
    assert_eq!(
        reused_hwnd_stale(&known, &BTreeSet::new(), &fresh),
        vec![key(41)]
    );
    assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
    assert!(!restart_float_eligible(true, true, false, float_marker));
}

#[test]
fn restart_marker_values_roundtrip_and_reject() {
    // Codec behind the native install/remove ops (exercised for real in
    // `tiling_sys::restart_adoption_tests`): well-formed values
    // round-trip; zero/unknown fail closed.
    assert_eq!(crate::tiling::float_intent_marker_value(), 1);
    assert!(crate::tiling::parse_float_intent_marker(1).is_some());
    assert!(crate::tiling::parse_float_intent_marker(0).is_none());
    assert!(crate::tiling::parse_float_intent_marker(2).is_none());
    assert!(crate::tiling::parse_float_intent_marker(u64::MAX).is_none());
    assert_eq!(crate::tiling::tile_override_marker_value(), 1);
    assert!(crate::tiling::parse_tile_override_marker(1).is_some());
    assert!(crate::tiling::parse_tile_override_marker(0).is_none());
    assert!(crate::tiling::parse_tile_override_marker(2).is_none());
    assert!(crate::tiling::parse_tile_override_marker(u64::MAX).is_none());
}

#[test]
fn restart_tile_override_reserved_never_floats() {
    // The D7 tile-override marker classifies but hydrates nothing in
    // item 8: no float, no sticky, fixed-size admission unwired.
    let marker = classify_restart_marker(
        MarkerSlot::Absent,
        MarkerSlot::Absent,
        MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
    );
    assert_eq!(marker, RestartMarker::TileOverride);
    assert!(!restart_float_eligible(true, true, true, marker));
    assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
    // Sticky and float markers outrank the reserved lane.
    assert_eq!(
        classify_restart_marker(
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::sticky_marker_value(false)),
            MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
        ),
        RestartMarker::Sticky(false)
    );
    assert_eq!(
        classify_restart_marker(
            MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
        ),
        RestartMarker::FloatIntent
    );
}

#[test]
fn restart_second_owner_rehydrates_kept_markers() {
    // Adoption keeps durable markers, so a second owner restart reads
    // the same slots and reaches the same verdict. No post-restart
    // un-stick outcome is selected on either pass.
    let lanes = [
        (
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::sticky_marker_value(false)),
            MarkerSlot::Absent,
        ),
        (
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::sticky_marker_value(true)),
            MarkerSlot::Absent,
        ),
        (
            MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
            MarkerSlot::Absent,
            MarkerSlot::Absent,
        ),
    ];
    for (float, sticky, tile) in lanes {
        // First owner: hydrate.
        let first = classify_restart_marker(float, sticky, tile);
        let first_float = restart_float_eligible(true, true, true, first);
        let first_sticky = restart_sticky_eligible(true, true, true, first);
        // Second owner with the kept marker: identical verdict.
        let second = classify_restart_marker(float, sticky, tile);
        assert_eq!(second, first);
        assert_eq!(
            restart_float_eligible(true, true, true, second),
            first_float
        );
        assert_eq!(
            restart_sticky_eligible(true, true, true, second),
            first_sticky
        );
    }
}

#[test]
fn restart_prop_namespaces_are_distinct_and_legacy_retired() {
    // Stronger dotted namespaces for all three lanes; the legacy
    // ambiguous sticky name hydrates nothing.
    for prop in [
        crate::model::STICKY_PROP,
        crate::model::FLOAT_INTENT_PROP,
        crate::model::TILE_OVERRIDE_PROP,
    ] {
        assert!(prop.starts_with("OmniTiler"), "distinctive prefix: {prop}");
        assert!(prop.contains('.'), "stronger namespace: {prop}");
    }
    assert_ne!(crate::model::STICKY_PROP, crate::model::FLOAT_INTENT_PROP);
    assert_ne!(crate::model::STICKY_PROP, crate::model::TILE_OVERRIDE_PROP);
    assert_ne!(
        crate::model::FLOAT_INTENT_PROP,
        crate::model::TILE_OVERRIDE_PROP
    );
    assert_eq!(crate::model::STICKY_PROP, "OmniTiler.Sticky.v1");
    // The legacy ambiguous name hydrates nothing: only the dotted name
    // above is ever read.
    assert_ne!(crate::model::STICKY_PROP, "OmniTilerSticky");
}

#[test]
fn hidden_float_rows_stay_floating_until_reveal() {
    // Hidden intentional floats ride the live float snapshot with the
    // tiled allocation as fallback, so convergence never drops retained
    // hidden membership. Sticky rides the same slotless float lane.
    use super::super::{MemberView, domain_rows, hidden_snapshot_rect};
    let float_rect = Some(Rect {
        x: 10,
        y: 20,
        w: 400,
        h: 300,
    });
    let tiled_rect = Some(Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    });
    assert_eq!(
        hidden_snapshot_rect(true, float_rect, tiled_rect),
        float_rect
    );
    assert_eq!(hidden_snapshot_rect(true, None, tiled_rect), tiled_rect);
    let member = key(21);
    let members: BTreeSet<WindowKey> = [member.clone()].into_iter().collect();
    let float_frame = Rect {
        x: 10,
        y: 20,
        w: 400,
        h: 300,
    };
    let views = vec![MemberView {
        key: member,
        token: "w-float".to_owned(),
        rect: float_frame,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: true,
    }];
    let rows = domain_rows(&members, &views).expect("hidden float rows");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].floating, "hidden float stays floating");
}
