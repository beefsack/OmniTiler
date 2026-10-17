#![forbid(unsafe_code)]
use super::{
    PreviewBound, PreviewFresh, PreviewGate, preview_hide_for_refusal, preview_sample_gate,
};

#[test]
fn sample_gate_needs_real_unlatched_movement() {
    // Zero movement (title click, stationary-hold start point) never
    // shows; an Esc-latched hold never shows even when moved.
    assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, false));
    assert!(!PreviewBound::sample_allowed(10, 10, 12, 10, true));
    assert!(!PreviewBound::sample_allowed(10, 10, 10, 10, true));
    assert!(PreviewBound::sample_allowed(10, 10, 11, 10, false));
    assert!(PreviewBound::sample_allowed(10, 10, 10, 11, false));
}

fn fresh_ok() -> PreviewFresh {
    PreviewFresh {
        dead: false,
        zero: false,
        identity_ok: true,
        tag_ok: true,
        token_ok: true,
        loc_ok: true,
        revision_ok: true,
        float_hold: false,
        inside: true,
    }
}

fn gate_outcome(fresh: &PreviewFresh) -> &'static str {
    match preview_sample_gate(fresh) {
        PreviewGate::Proceed => "proceed",
        PreviewGate::Transient(reason) => reason,
        PreviewGate::Dead(reason) => reason,
    }
}

#[test]
fn gate_proceeds_only_when_every_fence_holds() {
    assert_eq!(gate_outcome(&fresh_ok()), "proceed");
}

#[test]
fn gate_invalidation_dominates_a_static_pointer() {
    // Same pointer (zero) plus identity loss still fails closed: a
    // lingering rectangle must never survive mover death or reuse on a
    // stationary pointer.
    for fresh in [
        PreviewFresh {
            zero: true,
            identity_ok: false,
            ..fresh_ok()
        },
        PreviewFresh {
            zero: true,
            tag_ok: false,
            ..fresh_ok()
        },
        PreviewFresh {
            zero: true,
            token_ok: false,
            ..fresh_ok()
        },
    ] {
        assert_eq!(gate_outcome(&fresh), "identity-changed");
    }
    // Float/sticky capture fails closed too, even unmoved.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            zero: true,
            float_hold: true,
            ..fresh_ok()
        }),
        "floating"
    );
}

#[test]
fn gate_drift_before_first_sample_fails_closed() {
    // Output/workspace drift and revision drift fail the whole gesture
    // from the very first sample (START binding is authoritative; the
    // refresh never rebinds to current membership).
    assert_eq!(
        gate_outcome(&PreviewFresh {
            loc_ok: false,
            ..fresh_ok()
        }),
        "drift"
    );
    assert_eq!(
        gate_outcome(&PreviewFresh {
            revision_ok: false,
            ..fresh_ok()
        }),
        "revision-drift"
    );
}

#[test]
fn gate_dead_repeat_can_never_rebind() {
    // A drifted gesture stays dead on repeat samples even when every
    // fresh fact reads healthy again.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            dead: true,
            ..fresh_ok()
        }),
        "dead"
    );
}

#[test]
fn gate_zero_and_outside_hide_transiently() {
    // Zero movement and outside-work-area hide without killing the
    // gesture: the next moved sample may still show.
    assert_eq!(
        gate_outcome(&PreviewFresh {
            zero: true,
            ..fresh_ok()
        }),
        "zero"
    );
    assert_eq!(
        gate_outcome(&PreviewFresh {
            inside: false,
            ..fresh_ok()
        }),
        "outside"
    );
}

#[test]
fn frame_gate_matches_either_lane_and_rejects_resizes() {
    use super::preview_same_size;
    // Pure move: both lanes match (no shadows).
    assert!(preview_same_size(800, 600, 800, 600, 800, 600));
    // Shadow padding shifts only the outer lane: the visible lane
    // still matches, so a move is not misread as a resize.
    assert!(preview_same_size(800, 600, 816, 616, 800, 600));
    // Outer-fallback pre-gesture rect matches the outer lane.
    assert!(preview_same_size(816, 616, 816, 616, 800, 600));
    // Genuine resizes move both lanes: width-only, height-only, both,
    // and shrinkage all fail the move gate (preview stays hidden).
    assert!(!preview_same_size(800, 600, 900, 600, 884, 600));
    assert!(!preview_same_size(800, 600, 800, 700, 800, 684));
    assert!(!preview_same_size(800, 600, 900, 700, 884, 684));
    assert!(!preview_same_size(800, 600, 700, 500, 684, 484));
    // Degenerate live frames never match a real pre-gesture rect.
    assert!(!preview_same_size(800, 600, 0, 0, 0, 0));
}

#[test]
fn prior_descriptor_is_closed_without_raw_ids() {
    use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
    use tiler_core::session::{DomainKey, DragHoverPrior};
    // No prior at all, and a cleared (non-group-edge) hover, both
    // describe as `none`.
    assert_eq!(super::preview_prior_desc(&None), "none");
    let cleared = DragHoverPrior {
        domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        source_leaf: NodeId("leaf-w1".to_owned()),
        source_window: WindowId("w1".to_owned()),
        revision: 3,
        prior: None,
        prior_domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
    };
    assert_eq!(super::preview_prior_desc(&Some(cleared)), "none");
    // A sticky group-edge carry describes only its side: the group
    // NodeId never reaches the log.
    let sticky = DragHoverPrior {
        domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
        source_leaf: NodeId("leaf-w1".to_owned()),
        source_window: WindowId("w1".to_owned()),
        revision: 3,
        prior: Some(tiler_core::policy::PriorGroupEdge {
            group: NodeId("grp-secret".to_owned()),
            edge: tiler_core::contract::DragSide::Top,
        }),
        prior_domain: DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        },
    };
    let desc = super::preview_prior_desc(&Some(sticky));
    assert_eq!(desc, "group-edge:top");
    assert!(!desc.contains("grp-secret"));
}

#[test]
fn refusal_kinds_map_to_no_rectangle_hides() {
    // Self, centre-stack, and outside refusals show no rectangle; every
    // other kind rides through for attribution.
    assert_eq!(preview_hide_for_refusal("unchanged"), "self");
    assert_eq!(preview_hide_for_refusal("unsupported-capability"), "centre");
    assert_eq!(preview_hide_for_refusal("cross-domain-mismatch"), "outside");
    assert_eq!(
        preview_hide_for_refusal("domain-mismatch"),
        "domain-mismatch"
    );
}

/// Same-output preview/drop agreement through the exact production
/// command shapes (`DragPreview` samples with a carried `hover_prior`
/// into a `DragDrop` with that prior): the final mover geometry equals
/// the preview's proposed rect, previews never store, and
/// centre/self/outside refuse with no plan. Fresh size hints ride both
/// paths identically (same row assembly, same resolver).
#[test]
fn preview_samples_agree_with_final_drop() {
    use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::engine::Engine;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::session::OutputDomain;

    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("preview-1").expect("generation");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);
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
    let window = |token: &str, min_w: Option<i32>| tiler_core::seed::EngineWindow {
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
        hints: tiler_core::size_hints::WindowSizeHints {
            min_w,
            min_h: None,
            max_w: None,
            max_h: None,
        },
    };
    let event =
        |command: CoreCommand, correlation: &str, windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                closed_windows: Vec::new(),
                command,
            }
        };
    let rows = || vec![window("w1", Some(200)), window("w2", None)];
    // Converge once to read the authoritative projected layout: the
    // edge sample below derives from Engine geometry, never a guessed
    // half-window rectangle.
    let geometry = match engine.handle(&event(CoreCommand::Reconcile, "corr-preview-seed", rows()))
    {
        CoreReply::Projection(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        other => panic!("seed must converge, got {other:?}"),
    };
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
    // that changes topology (insert before), from Engine geometry.
    let edge = (target.x + 2, target.y + target.h / 2);
    let revision = engine
        .session(&domain.key())
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let preview_cmd = |x: i32, y: i32, prior: Option<tiler_core::session::DragHoverPrior>| {
        CoreCommand::DragPreview {
            window: "w1".to_owned(),
            x,
            y,
            hover_prior: prior,
            source: None,
        }
    };
    let (proposed, prior) = match engine.handle(&event(
        preview_cmd(edge.0, edge.1, None),
        "corr-preview-1",
        rows(),
    )) {
        CoreReply::DragPreview(plan) => {
            assert_eq!(plan.base_revision, revision);
            (plan.preview.proposed_rect, plan.preview.hover_prior())
        }
        other => panic!("edge preview must resolve, got {other:?}"),
    };
    assert_ne!(proposed, mover_rect, "edge drop must re-place the mover");
    // The preview stores nothing: revision and topology untouched.
    assert_eq!(
        engine.session(&domain.key()).map(|s| s.accepted_revision()),
        Some(revision)
    );
    // A second sample carrying the exact prior resolves identically:
    // preview/drop agreement with the sticky carry in place.
    let (proposed2, prior2) = match engine.handle(&event(
        preview_cmd(edge.0, edge.1, Some(prior.clone())),
        "corr-preview-2",
        rows(),
    )) {
        CoreReply::DragPreview(plan) => (plan.preview.proposed_rect, plan.preview.hover_prior()),
        other => panic!("carried preview must resolve, got {other:?}"),
    };
    assert_eq!(proposed2, proposed, "carried prior keeps the target");
    // Rejected points show no rectangle: centre-stack, self, outside.
    // These run before the drop mutates the layout, so the sampled
    // points still name the intended targets.
    let centre = (target.x + target.w / 2, target.y + target.h / 2);
    match engine.handle(&event(
        preview_cmd(centre.0, centre.1, None),
        "corr-preview-centre",
        rows(),
    )) {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(preview_hide_for_refusal(kind), "centre")
        }
        other => panic!("centre preview must refuse, got {other:?}"),
    }
    let itself = (
        mover_rect.x + mover_rect.w / 2,
        mover_rect.y + mover_rect.h / 2,
    );
    match engine.handle(&event(
        preview_cmd(itself.0, itself.1, None),
        "corr-preview-self",
        rows(),
    )) {
        CoreReply::Rejected { kind, .. } => assert_eq!(preview_hide_for_refusal(kind), "self"),
        other => panic!("self preview must refuse, got {other:?}"),
    }
    match engine.handle(&event(
        preview_cmd(-50, -50, None),
        "corr-preview-outside",
        rows(),
    )) {
        CoreReply::Rejected { kind, .. } => {
            assert_eq!(preview_hide_for_refusal(kind), "outside")
        }
        other => panic!("outside preview must refuse, got {other:?}"),
    }
    // The final drop forwards the carried prior and lands exactly on
    // the preview's proposed rect.
    match engine.handle(&event(
        CoreCommand::DragDrop {
            window: "w1".to_owned(),
            x: edge.0,
            y: edge.1,
            hover_prior: Some(prior2),
            source: None,
        },
        "corr-preview-drop",
        rows(),
    )) {
        CoreReply::Tiled(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("mover placement");
            assert_eq!(placed.rect, proposed, "drop must equal preview");
        }
        other => panic!("carried drop must plan, got {other:?}"),
    }
}

/// Sticky group-edge journey through the exact production command
/// shapes: adopt three windows, sample a group-gap strip inside the
/// 32px top-edge zone (GroupEdge with a sticky `Some` prior), step away
/// past 32px but inside the 80px sticky depth (same GroupEdge only
/// because the exact prior carried), then drop with that prior and land
/// exactly on the preview rect. Gap geometry derives from Engine
/// geometry, never hardcoded topology.
#[test]
fn preview_sticky_journey_agrees_with_drop() {
    use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply};
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::engine::Engine;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    use tiler_core::session::OutputDomain;

    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("preview-sticky").expect("generation");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);
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
    let window = |token: &str| tiler_core::seed::EngineWindow {
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
        hints: tiler_core::size_hints::WindowSizeHints::none(),
    };
    let event =
        |command: CoreCommand, correlation: &str, windows: Vec<tiler_core::seed::EngineWindow>| {
            CoreEvent {
                owner: owner.clone(),
                generation: generation.clone(),
                correlation: CorrelationId::parse(correlation).expect("correlation"),
                revision: 0,
                fingerprint: 7,
                domain: domain.clone(),
                domain_key: domain.key(),
                outer_gap: 8,
                focused_window: WindowId("w1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: None,
                target_windows: Vec::new(),
                closed_windows: Vec::new(),
                command,
            }
        };
    let rows = || vec![window("w1"), window("w2"), window("w3")];
    let geometry = match engine.handle(&event(CoreCommand::Reconcile, "corr-sticky-seed", rows())) {
        CoreReply::Projection(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect::<Vec<_>>(),
        other => panic!("seed must converge, got {other:?}"),
    };
    // Inter-tile gap strip from Engine geometry: adjacent pair
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
    let preview_at =
        |x: i32, y: i32, prior: Option<tiler_core::session::DragHoverPrior>, correlation: &str| {
            event(
                CoreCommand::DragPreview {
                    window: "w1".to_owned(),
                    x,
                    y,
                    hover_prior: prior,
                    source: None,
                },
                correlation,
                rows(),
            )
        };
    let (proposed1, prior1) =
        match engine.handle(&preview_at(leg1.0, leg1.1, None, "corr-sticky-1")) {
            CoreReply::DragPreview(plan) => {
                assert!(
                    plan.preview.hover_prior().prior.is_some(),
                    "gap leg must set a sticky prior, got {:?}",
                    plan.preview
                );
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("gap leg must resolve a group edge, got {other:?}"),
        };
    // Past the 32px zone but inside the 80px sticky depth: the same
    // GroupEdge resolves only through the carried prior. Contrast:
    // without the prior the same point must NOT return the sticky
    // result (refusal or a different interior placement).
    let no_prior_sticky =
        match engine.handle(&preview_at(leg2.0, leg2.1, None, "corr-sticky-noprior")) {
            CoreReply::DragPreview(plan) => {
                plan.preview.proposed_rect == proposed1
                    && plan.preview.hover_prior().prior.is_some()
            }
            _ => false,
        };
    assert!(
        !no_prior_sticky,
        "leg2 without prior must not reproduce the sticky target"
    );
    let (proposed2, prior2) = match engine.handle(&preview_at(
        leg2.0,
        leg2.1,
        Some(prior1.clone()),
        "corr-sticky-2",
    )) {
        CoreReply::DragPreview(plan) => (plan.preview.proposed_rect, plan.preview.hover_prior()),
        other => panic!("sticky leg must resolve, got {other:?}"),
    };
    assert_eq!(proposed2, proposed1, "sticky leg keeps the target");
    assert_eq!(
        super::preview_prior_desc(&Some(prior2.clone())),
        super::preview_prior_desc(&Some(prior1.clone())),
        "sticky prior carries unchanged"
    );
    match engine.handle(&event(
        CoreCommand::DragDrop {
            window: "w1".to_owned(),
            x: leg2.0,
            y: leg2.1,
            hover_prior: Some(prior2),
            source: None,
        },
        "corr-sticky-drop",
        rows(),
    )) {
        CoreReply::Tiled(plan) => {
            let placed = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("mover placement");
            assert_eq!(placed.rect, proposed1, "sticky drop must equal preview");
        }
        other => panic!("sticky drop must plan, got {other:?}"),
    }
}
