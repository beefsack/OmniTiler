//! Relocated per-domain drag preview/drop agreement tests. Moved from the
//! Windows adapter suite with the per-domain route removal; the `WorldLayout`
//! refusal kinds below are asserted raw (the adapter hide-mapping stays covered
//! by the adapter suite against literal kinds).
/// Same-output preview/drop agreement through `WorldLayout::run_local_drag_preview`
/// and `run_local_drag_drop` (`DragPreview` samples with a carried `hover_prior`
/// into a `DragDrop` with that prior): the final mover geometry equals
/// the preview's proposed rect, previews never store, and
/// centre/self/outside refuse with no plan. Fresh size hints ride both
/// paths identically (same row assembly, same resolver).
#[test]
fn preview_samples_agree_with_final_drop() {
    use crate::directional::{OutputId, WindowId, WorkspaceId};
    use crate::geometry::Rect;
    use crate::session::{OutputDomain, RefusalKind};
    use crate::world_layout::{LayoutFailure, WorldLayout};

    let mut layout = WorldLayout::new();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds,
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let key = domain.key();
    let window = |token: &str, min_w: Option<i32>| crate::seed::EngineWindow {
        window: WindowId(token.to_owned()),
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: bounds,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints {
            min_w,
            min_h: None,
            max_w: None,
            max_h: None,
        },
    };
    let rows = || vec![window("w1", Some(200)), window("w2", None)];
    let w1 = WindowId("w1".to_owned());
    // Converge once to read the authoritative projected layout: the
    // edge sample below derives from WorldLayout geometry, never a guessed
    // half-window rectangle.
    let geometry: Vec<(String, Rect)> = layout
        .reconcile_fresh(&domain, 8, &w1, &rows())
        .expect("seed must converge")
        .projection
        .geometry
        .into_iter()
        .map(|g| (g.window.0, g.rect))
        .collect();
    let target = geometry
        .iter()
        .find(|(token, _)| token == "w2")
        .map(|(_, rect)| *rect)
        .expect("sibling geometry");
    let mover_rect = geometry
        .iter()
        .find(|(token, _)| token == "w1")
        .map(|(_, rect)| *rect)
        .expect("mover geometry");
    // Two pixels inside the sibling's left edge: a window-edge target
    // that changes topology (insert before), from WorldLayout geometry.
    let edge = (target.x + 2, target.y + target.h / 2);
    let revision = layout.accepted_revision(&key);
    let (proposed, prior) = {
        let plan = layout
            .run_local_drag_preview(
                &domain,
                8,
                crate::policy::ScaledResize::unscaled(),
                &w1,
                &w1,
                &rows(),
                edge.0,
                edge.1,
                None,
            )
            .expect("edge preview must resolve");
        assert_eq!(plan.base_revision, revision);
        (plan.proposed_rect, plan.hover.clone())
    };
    assert_ne!(proposed, mover_rect, "edge drop must re-place the mover");
    // The preview stores nothing: revision and topology untouched.
    assert_eq!(layout.accepted_revision(&key), revision);
    // A second sample carrying the exact prior resolves identically:
    // preview/drop agreement with the sticky carry in place.
    let (proposed2, prior2) = {
        let plan = layout
            .run_local_drag_preview(
                &domain,
                8,
                crate::policy::ScaledResize::unscaled(),
                &w1,
                &w1,
                &rows(),
                edge.0,
                edge.1,
                Some(&prior),
            )
            .expect("carried preview must resolve");
        (plan.proposed_rect, plan.hover.clone())
    };
    assert_eq!(proposed2, proposed, "carried prior keeps the target");
    // Rejected points show no rectangle: centre-stack, self, outside.
    // These run before the drop mutates the layout, so the sampled
    // points still name the intended targets.
    let centre = (target.x + target.w / 2, target.y + target.h / 2);
    assert_eq!(
        layout.run_local_drag_preview(
            &domain,
            8,
            crate::policy::ScaledResize::unscaled(),
            &w1,
            &w1,
            &rows(),
            centre.0,
            centre.1,
            None
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnsupportedCapability))
    );
    let itself = (
        mover_rect.x + mover_rect.w / 2,
        mover_rect.y + mover_rect.h / 2,
    );
    assert_eq!(
        layout.run_local_drag_preview(
            &domain,
            8,
            crate::policy::ScaledResize::unscaled(),
            &w1,
            &w1,
            &rows(),
            itself.0,
            itself.1,
            None
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(
        layout.run_local_drag_preview(
            &domain,
            8,
            crate::policy::ScaledResize::unscaled(),
            &w1,
            &w1,
            &rows(),
            -50,
            -50,
            None
        ),
        Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );
    // The final drop forwards the carried prior and lands exactly on
    // the preview's proposed rect.
    let _drop_out = layout
        .run_local_drag_drop(
            &domain,
            8,
            crate::policy::ScaledResize::unscaled(),
            &w1,
            &w1,
            &rows(),
            edge.0,
            edge.1,
            Some(&prior2),
        )
        .expect("carried drop must plan");
    let hints: std::collections::BTreeMap<_, _> =
        rows().into_iter().map(|r| (r.window, r.hints)).collect();
    let drop_geo = layout
        .project_domain_geometry(&key, &hints)
        .expect("drop geometry");
    let placed = drop_geo
        .iter()
        .find(|g| g.window.0 == "w1")
        .expect("mover placement");
    assert_eq!(placed.rect, proposed, "drop must equal preview");
}
/// Sticky group-edge journey through `WorldLayout::run_local_drag_preview` and
/// `run_local_drag_drop`: adopt three windows, sample a group-gap strip inside
/// the 32px top-edge zone (GroupEdge with a sticky `Some` prior), step away
/// past 32px but inside the 80px sticky depth (same GroupEdge only
/// because the exact prior carried), then drop with that prior and land
/// exactly on the preview rect. Gap geometry derives from WorldLayout
/// geometry, never hardcoded topology.
#[test]
fn preview_sticky_journey_agrees_with_drop() {
    use crate::directional::{OutputId, WindowId, WorkspaceId};
    use crate::geometry::Rect;
    use crate::session::OutputDomain;
    use crate::world_layout::WorldLayout;

    let mut layout = WorldLayout::new();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 900,
        h: 600,
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds,
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let window = |token: &str| crate::seed::EngineWindow {
        window: WindowId(token.to_owned()),
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: bounds,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    };
    let rows = || vec![window("w1"), window("w2"), window("w3")];
    let w1 = WindowId("w1".to_owned());
    let geometry: Vec<(String, Rect)> = layout
        .reconcile_fresh(&domain, 8, &w1, &rows())
        .expect("seed must converge")
        .projection
        .geometry
        .into_iter()
        .map(|g| (g.window.0, g.rect))
        .collect();
    // Inter-tile gap strip from WorldLayout geometry: adjacent pair
    // overlapping in y with a real x gap.
    let mut rects: Vec<Rect> = geometry.iter().map(|(_, r)| *r).collect();
    rects.sort_by_key(|r| (r.x, r.y));
    let mut gap = None;
    for pair in rects.windows(2) {
        let (r1, r2) = (pair[0], pair[1]);
        if r1.x + r1.w < r2.x && r1.y < r2.y + r2.h && r2.y < r1.y + r1.h {
            let top = r1.y.max(r2.y);
            let bot = (r1.y + r1.h).min(r2.y + r2.h);
            if bot - top >= 120 {
                gap = Some(((r1.x + r1.w + r2.x) / 2, top));
                break;
            }
        }
    }
    let (gap_x, strip_top) = gap.expect("inter-tile gap strip in 3-window layout");
    let leg1 = (gap_x, strip_top + 12);
    let leg2 = (gap_x, strip_top + 50);
    let (proposed1, prior1) = {
        let plan = layout
            .run_local_drag_preview(
                &domain,
                8,
                crate::policy::ScaledResize::unscaled(),
                &w1,
                &w1,
                &rows(),
                leg1.0,
                leg1.1,
                None,
            )
            .expect("gap leg must resolve a group edge");
        assert!(
            plan.hover.prior.is_some(),
            "gap leg must set a sticky prior, got {:?}",
            plan
        );
        (plan.proposed_rect, plan.hover.clone())
    };
    // Past the 32px zone but inside the 80px sticky depth: the same
    // GroupEdge resolves only through the carried prior. Contrast:
    // without the prior the same point must NOT return the sticky
    // result (refusal or a different interior placement).
    let no_prior_sticky = match layout.run_local_drag_preview(
        &domain,
        8,
        crate::policy::ScaledResize::unscaled(),
        &w1,
        &w1,
        &rows(),
        leg2.0,
        leg2.1,
        None,
    ) {
        Ok(plan) => plan.proposed_rect == proposed1 && plan.hover.prior.is_some(),
        Err(_) => false,
    };
    assert!(
        !no_prior_sticky,
        "leg2 without prior must not reproduce the sticky target"
    );
    let (proposed2, prior2) = {
        let plan = layout
            .run_local_drag_preview(
                &domain,
                8,
                crate::policy::ScaledResize::unscaled(),
                &w1,
                &w1,
                &rows(),
                leg2.0,
                leg2.1,
                Some(&prior1),
            )
            .expect("sticky leg must resolve");
        (plan.proposed_rect, plan.hover.clone())
    };
    assert_eq!(proposed2, proposed1, "sticky leg keeps the target");
    assert_eq!(prior2.prior, prior1.prior, "sticky prior carries unchanged");
    let _drop_out = layout
        .run_local_drag_drop(
            &domain,
            8,
            crate::policy::ScaledResize::unscaled(),
            &w1,
            &w1,
            &rows(),
            leg2.0,
            leg2.1,
            Some(&prior2),
        )
        .expect("sticky drop must plan");
    let hints: std::collections::BTreeMap<_, _> =
        rows().into_iter().map(|r| (r.window, r.hints)).collect();
    let drop_geo = layout
        .project_domain_geometry(&domain.key(), &hints)
        .expect("drop geometry");
    let placed = drop_geo
        .iter()
        .find(|g| g.window.0 == "w1")
        .expect("mover placement");
    assert_eq!(placed.rect, proposed1, "sticky drop must equal preview");
}
