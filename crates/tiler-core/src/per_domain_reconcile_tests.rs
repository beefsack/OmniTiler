//! Relocated per-domain Engine regression tests (reconcile/admit/snap-back,
//! margins, pointer-resize, directional focus/move, update-gaps topology).
//! Moved from the Windows adapter suite when the per-domain adapter route was
//! removed; behavior is portable core behavior, so the tests live with the
//! core per-domain suites and build observations via [`crate::per_domain_fixture`].

use crate::boundary::CoreCommand;
use crate::boundary::CoreReply;
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::per_domain_fixture::reconcile_event_for;
use crate::session::{DomainKey, OutputDomain};
use crate::size_hints::WindowSizeHints;

/// Retired adapter parity values (inner gap 8, outer gap 8) for the
/// relocated observation fixtures below.
const INNER_GAP: i32 = 8;
const OUTER_GAP: i32 = 8;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

#[allow(clippy::too_many_arguments)]
fn ev(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fp: u64,
    bounds: Rect,
    windows: &[(WindowId, Rect)],
    focused: Option<&WindowId>,
) -> crate::boundary::CoreEvent {
    let hinted: Vec<(WindowId, Rect, crate::size_hints::WindowSizeHints)> = windows
        .iter()
        .map(|(w, r)| (w.clone(), *r, crate::size_hints::WindowSizeHints::none()))
        .collect();
    let domain = OutputDomain {
        id: OutputId("display-0".to_owned()),
        workspace: WorkspaceId("workspace-0".to_owned()),
        bounds,
        gap: INNER_GAP,
        adjacent: std::collections::BTreeMap::new(),
    };
    let domain_key = DomainKey {
        output: OutputId("display-0".to_owned()),
        workspace: WorkspaceId("workspace-0".to_owned()),
    };
    reconcile_event_for(
        owner,
        generation,
        correlation,
        revision,
        fp,
        &domain,
        &domain_key,
        OUTER_GAP,
        &hinted,
        focused,
    )
}
#[test]
fn engine_admits_then_removes_windows() {
    use crate::boundary::CoreReply;
    use crate::engine::Engine;

    let owner = OwnerId::parse("tiler-windows").expect("valid");
    let generation = GenerationId::parse("abcdef0123456789").expect("valid");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);

    // Two windows admitted through fresh reconcile.
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-1").expect("valid"),
        0,
        1,
        rect(0, 0, 1600, 900),
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    let reply = engine.handle(&event);
    let base = match &reply {
        CoreReply::Tiled(plan) => {
            assert!(!plan.geometry.is_empty());
            plan.base_revision
        }
        CoreReply::Projection(plan) => plan.base_revision,
        other => panic!("unexpected reply: {other:?}"),
    };

    // Retained reconcile at the committed revision projects geometry.
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-2").expect("valid"),
        base,
        2,
        rect(0, 0, 1600, 900),
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => assert_eq!(plan.geometry.len(), 2),
        other => panic!("unexpected reply: {other:?}"),
    }

    // Close one window: convergence removes it, projection covers the survivor.
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-3").expect("valid"),
        base,
        3,
        rect(0, 0, 1600, 900),
        &[(WindowId("w1".to_owned()), rect(0, 0, 800, 884))],
        None,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            assert!(plan.geometry.iter().all(|g| g.window.0 == "w1"));
        }
        other => panic!("unexpected reply: {other:?}"),
    }
}
#[test]
fn engine_manual_geometry_never_adopted() {
    use crate::boundary::CoreReply;
    use crate::engine::Engine;

    let owner = OwnerId::parse("tiler-windows").expect("valid");
    let generation = GenerationId::parse("abcdef0123456789").expect("valid");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-1").expect("valid"),
        0,
        1,
        rect(0, 0, 1600, 900),
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    let base = match engine.handle(&event) {
        CoreReply::Tiled(plan) => plan.base_revision,
        CoreReply::Projection(plan) => plan.base_revision,
        other => panic!("unexpected reply: {other:?}"),
    };
    // User drags w1 elsewhere without a drag-drop op: retained reconcile
    // reprojects from topology (snap-back), proving the gesture route is
    // required to honor manual moves.
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-2").expect("valid"),
        base,
        2,
        rect(0, 0, 1600, 900),
        &[
            (WindowId("w1".to_owned()), rect(400, 200, 800, 600)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    match engine.handle(&event) {
        CoreReply::Projection(plan) => {
            let w1 = plan
                .geometry
                .iter()
                .find(|g| g.window.0 == "w1")
                .expect("w1 projected");
            assert_ne!(w1.rect, rect(400, 200, 800, 600));
        }
        other => panic!("unexpected reply: {other:?}"),
    }
}
#[test]
fn engine_projects_visible_margins_and_inner_gap() {
    use crate::boundary::CoreReply;
    use crate::engine::Engine;

    // Full rcWork in, inset domain out: 8px visible margin on every side plus
    // the 8px inner gap between tiles. Desired rects are visible-frame rects
    // the adapter converts to outer rects per app; the invisible border may
    // land outside rcWork and that is correct.
    let work = rect(0, 0, 1600, 900);
    let domain = crate::geometry::inset_bounds(work, OUTER_GAP).expect("inset");
    let owner = OwnerId::parse("tiler-windows").expect("valid");
    let generation = GenerationId::parse("abcdef0123456789").expect("valid");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-1").expect("valid"),
        0,
        1,
        domain,
        &[
            (WindowId("w1".to_owned()), rect(0, 0, 800, 884)),
            (WindowId("w2".to_owned()), rect(800, 0, 800, 884)),
        ],
        None,
    );
    let geometry = match engine.handle(&event) {
        CoreReply::Tiled(plan) => plan.geometry,
        CoreReply::Projection(plan) => plan.geometry,
        other => panic!("unexpected reply: {other:?}"),
    };
    assert_eq!(geometry.len(), 2);
    let mut by_x = geometry.clone();
    by_x.sort_by_key(|g| g.rect.x);
    let (left, right) = (&by_x[0], &by_x[1]);
    assert_eq!((left.rect.x, left.rect.y), (8, 8));
    assert_eq!((left.rect.w, left.rect.h), (788, 884));
    assert_eq!((right.rect.x, right.rect.w), (8 + 788 + INNER_GAP, 788));
    assert!(!left.overconstrained && !left.client_clamped);
}
#[test]
fn engine_pointer_resize_returns_resize_plan() {
    use crate::boundary::{CoreCommand, CoreReply};
    use crate::engine::Engine;

    let owner = OwnerId::parse("tiler-windows").expect("valid");
    let generation = GenerationId::parse("abcdef0123456789").expect("valid");
    let mut engine = Engine::new();
    engine.sync_binding(&owner, &generation);
    let domain = crate::geometry::inset_bounds(rect(0, 0, 1600, 900), OUTER_GAP).expect("inset");
    let event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-1").expect("valid"),
        0,
        1,
        domain,
        &[
            (WindowId("w1".to_owned()), rect(8, 8, 788, 884)),
            (WindowId("w2".to_owned()), rect(804, 8, 788, 884)),
        ],
        None,
    );
    let base = match engine.handle(&event) {
        CoreReply::Tiled(plan) => plan.base_revision,
        CoreReply::Projection(plan) => plan.base_revision,
        other => panic!("unexpected reply: {other:?}"),
    };
    let w1 = WindowId("w1".to_owned());
    let mut event = ev(
        &owner,
        &generation,
        &CorrelationId::parse("tick-2").expect("valid"),
        base,
        2,
        domain,
        &[
            (WindowId("w1".to_owned()), rect(8, 8, 788, 884)),
            (WindowId("w2".to_owned()), rect(804, 8, 788, 884)),
        ],
        Some(&w1),
    );
    event.command = CoreCommand::PointerResize {
        // w1 holds the right leaf (see margin test): its left edge borders
        // w2, so "left" moves the shared boundary. A "right" resize on the
        // rightmost leaf has no neighbor and correctly refuses `Unchanged`.
        window: "w1".to_owned(),
        direction: "left".to_owned(),
        boundary: 700,
        direction2: None,
        boundary2: None,
    };
    match engine.handle(&event) {
        CoreReply::Resize(plan) => assert!(!plan.geometry.is_empty()),
        other => panic!("unexpected reply: {other:?}"),
    }
}
fn engine_harness(
    windows: &[(WindowId, Rect)],
    focused: &WindowId,
) -> (Engine, crate::boundary::CoreEvent) {
    let mut engine = Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("valid");
    let generation = GenerationId::parse("gen-1").expect("valid");
    engine.sync_binding(&owner, &generation);
    let pairs: Vec<(String, Rect)> = windows.iter().map(|(w, r)| (w.0.clone(), *r)).collect();
    let hinted: Vec<(WindowId, Rect, WindowSizeHints)> = windows
        .iter()
        .map(|(w, r)| (w.clone(), *r, WindowSizeHints::none()))
        .collect();
    let domain = OutputDomain {
        id: OutputId("display-0".to_owned()),
        workspace: WorkspaceId("workspace-0".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1600,
            h: 900,
        },
        gap: INNER_GAP,
        adjacent: std::collections::BTreeMap::new(),
    };
    let domain_key = DomainKey {
        output: OutputId("display-0".to_owned()),
        workspace: WorkspaceId("workspace-0".to_owned()),
    };
    let event = reconcile_event_for(
        &owner,
        &generation,
        &CorrelationId::parse("snap-test").expect("valid"),
        0,
        crate::per_domain_fixture::fingerprint(&pairs),
        &domain,
        &domain_key,
        OUTER_GAP,
        &hinted,
        Some(focused),
    );
    (engine, event)
}
#[test]
fn engine_focus_moves_through_nested_topology() {
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    let w3 = WindowId("w3".to_owned());
    let observed = vec![
        (w1.clone(), rect(8, 8, 500, 884)),
        (w2.clone(), rect(516, 8, 500, 884)),
        (w3.clone(), rect(1024, 8, 568, 884)),
    ];
    let (mut engine, seed) = engine_harness(&observed, &w1);
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {reply:?}"
    );
    // Focus right from w1 lands on its neighbor with full geometry.
    let mut focus = seed.clone();
    focus.command = CoreCommand::Focus {
        window: w1.0.clone(),
        direction: "right".to_owned(),
        cross_output_transfer: false,
        float_subject: false,
    };
    assert_eq!(focus.command.op(), "focus");
    let reply = engine.handle(&focus);
    match reply {
        CoreReply::FocusDirectional(plan) => {
            assert_eq!(plan.to_window, w2);
            assert_eq!(plan.geometry.len(), 3);
        }
        other => panic!("focus must plan directionally, got {other:?}"),
    }
    // Move uses the move route with full desired geometry. The moved window
    // is the focused one, mirroring the owner drain. Explicit default-wrap
    // fixture: production threads the live `core.same_axis_move` here.
    let mut mv = seed.clone();
    mv.focused_window = w2.clone();
    mv.command = CoreCommand::Move {
        window: w2.0.clone(),
        direction: "right".to_owned(),
        cross_output_transfer: false,
        same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
    };
    assert_eq!(mv.command.op(), "move");
    let reply = engine.handle(&mv);
    match reply {
        CoreReply::MoveDirectional(plan) => {
            assert_eq!(plan.geometry.len(), 3);
        }
        other => panic!("move must plan directionally, got {other:?}"),
    }
}
#[test]
fn retained_update_gaps_preserves_topology() {
    use crate::boundary::{CoreCommand, CoreReply};
    use crate::directional::{OutputId, WindowId, WorkspaceId};
    use crate::geometry::Rect;
    use crate::ids::{CorrelationId, GenerationId, OwnerId};
    use crate::session::{DomainKey, OutputDomain};

    fn rect() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        }
    }
    let mut engine = crate::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let key = DomainKey {
        output: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId("mon-a".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: rect(),
        gap: 8,
        adjacent: std::collections::BTreeMap::new(),
    };
    let rows = vec![
        (
            WindowId("w1".to_owned()),
            rect(),
            crate::size_hints::WindowSizeHints::none(),
            false,
        ),
        (
            WindowId("w2".to_owned()),
            rect(),
            crate::size_hints::WindowSizeHints::none(),
            false,
        ),
    ];
    let fingerprint = crate::per_domain_fixture::fingerprint(
        &rows
            .iter()
            .map(|(t, r, _, _)| (t.0.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse("seed").expect("correlation");
    let mut seed = crate::per_domain_fixture::reconcile_event_for_floating(
        &owner,
        &generation,
        &correlation,
        0,
        fingerprint,
        &domain,
        &key,
        8,
        &rows,
        Some(&WindowId("w1".to_owned())),
    );
    assert!(seed.command == CoreCommand::Reconcile);
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed converges, got {reply:?}"
    );
    let tree_before = format!(
        "{:?}",
        engine.session(&key).expect("session").snapshot().domains
    );
    // Plain reconcile with drifted gaps refuses instead of adopting.
    seed.command = CoreCommand::Reconcile;
    seed.domain.gap = 4;
    seed.outer_gap = 12;
    let reply = engine.handle(&seed);
    let CoreReply::Rejected { kind, .. } = reply else {
        panic!("gap drift must refuse, got {reply:?}");
    };
    assert_eq!(kind, "domain-mismatch");
    // The retained update-gaps path adopts without destroying topology.
    // A retained mismatch issues `UpdateGaps` so topology survives: plain
    // reconcile refuses gap drift instead of adopting it.
    let retained_matches = |engine: &crate::engine::Engine, gap: i32, outer_gap: i32| {
        engine.session(&key).is_some_and(|session| {
            session
                .domains()
                .iter()
                .any(|domain| domain.key() == key && domain.gap == gap)
        }) && engine.outer_gap_matches(&key, outer_gap)
    };
    assert!(!retained_matches(&engine, 4, 12));
    seed.command = CoreCommand::UpdateGaps;
    let reply = engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_)),
        "update-gaps adopts, got {reply:?}"
    );
    let tree_after = format!(
        "{:?}",
        engine.session(&key).expect("session").snapshot().domains
    );
    assert_eq!(tree_before, tree_after, "topology survives the gap update");
    assert!(retained_matches(&engine, 4, 12));
    assert_eq!(engine.outer_gap(&key), Some(12));
}

#[test]
fn engine_edge_focus_is_a_no_op_rejection() {
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    let observed = vec![
        (w1.clone(), rect(8, 8, 788, 884)),
        (w2.clone(), rect(804, 8, 788, 884)),
    ];
    let (mut engine, seed) = engine_harness(&observed, &w1);
    let seed_reply = engine.handle(&seed);
    assert!(
        matches!(seed_reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {seed_reply:?}"
    );
    // Focus left off the left edge: no neighbor, no actuation.
    let mut edge = seed.clone();
    edge.command = CoreCommand::Focus {
        window: w1.0.clone(),
        direction: "left".to_owned(),
        cross_output_transfer: false,
        float_subject: false,
    };
    let reply = engine.handle(&edge);
    assert!(
        matches!(reply, CoreReply::Rejected { .. }),
        "edge focus must refuse without geometry, got {reply:?}"
    );
}
