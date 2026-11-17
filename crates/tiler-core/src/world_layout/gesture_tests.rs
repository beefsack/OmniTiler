use std::collections::BTreeMap;

use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::outcome::{LayoutFailure, PairOperationFailure};
use super::pair::PairAssemblyFailure;
use super::pair_operation::PairOperationContext;
use super::state::{DomainException, DomainLayout, WorldLayout};

fn ws_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn domain_desc(key: &DomainKey, bounds: Rect, gap: i32) -> OutputDomain {
    OutputDomain {
        id: key.output.clone(),
        workspace: key.workspace.clone(),
        bounds,
        gap,
        adjacent: BTreeMap::new(),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn leaf(id: &str) -> Node {
    Node::Leaf { id: nid(id) }
}

fn eng(window: &str, key: &DomainKey, x: i32, w: i32) -> EngineWindow {
    EngineWindow {
        window: win(window),
        output: key.output.clone(),
        workspace: key.workspace.clone(),
        rect: Rect { x, y: 0, w, h: 400 },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn two_tile_domain(
    bounds: Rect,
    outer_gap: i32,
    group: &str,
    w1: &str,
    l1: &str,
    w2: &str,
    l2: &str,
    rev: u64,
) -> DomainLayout {
    let mut d = DomainLayout::new(bounds, 0, outer_gap);
    d.tree = Some(Node::Group {
        id: nid(group),
        axis: Axis::Horizontal,
        children: vec![leaf(l1), leaf(l2)],
        shares: vec![500, 500],
    });
    d.leaves.insert(win(w1), nid(l1));
    d.leaves.insert(win(w2), nid(l2));
    d.focused_leaf = Some(nid(l1));
    d.focus_stack = vec![nid(l2), nid(l1)];
    d.focus_history = vec![nid(l2), nid(l1)];
    d.last_active = Some(nid(l1));
    d.admission_order.insert(win(w1), 1);
    d.admission_order.insert(win(w2), 2);
    d.accepted_revision = rev;
    d
}

#[test]
fn local_preview_does_not_install_topology_and_burns_group_serials_on_split() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let d = two_tile_domain(bounds, 0, "g-root", "w1", "a", "w2", "b", 3);
    let original_tree = d.tree.clone();
    layout.domains.insert(key.clone(), d);

    let obs = vec![eng("w1", &key, 0, 400), eng("w2", &key, 400, 400)];
    let before_mint = layout.group_alloc.mint(&std::collections::BTreeSet::new());

    // Pointer near top edge of w2 (600, 5) creates a new vertical split group
    // around b and a, burning a group serial without installing topology.
    let preview = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, 600, 5, None)
        .expect("local drag preview succeeds");

    assert_eq!(preview.base_revision, 3);
    assert_eq!(preview.domain, key);
    assert_eq!(preview.source_domain, key);
    assert_eq!(preview.target_window, win("w2"));
    assert_eq!(
        layout.domains.get(&key).and_then(|dom| dom.tree.clone()),
        original_tree
    );
    assert_eq!(
        layout.domains.get(&key).map(|d| d.accepted_revision),
        Some(3)
    );
    assert_eq!(before_mint, Some(nid("grp-0")));
    assert_ne!(
        layout.group_alloc.mint(&std::collections::BTreeSet::new()),
        Some(nid("grp-1"))
    );

    // Passing the returned hover prior back into a follow-up preview carries cleanly.
    let followup = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            600,
            5,
            Some(&preview.hover),
        )
        .expect("follow-up preview with carried hover prior succeeds");
    assert_eq!(followup.proposed_rect, preview.proposed_rect);
}

#[test]
fn local_preview_on_fresh_domain_leaves_layout_absent() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);
    let layout = WorldLayout::new();
    let obs = vec![eng("w1", &key, 0, 400), eng("w2", &key, 400, 400)];

    let preview = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, 750, 200, None)
        .expect("fresh local drag preview succeeds");

    assert_eq!(preview.domain, key);
    assert!(!layout.contains_domain(&key));
}

#[test]
fn local_drop_commits_topology_bumps_revision_and_settles_focus() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let d = two_tile_domain(bounds, 0, "g-root", "w1", "a", "w2", "b", 4);
    let original_tree = d.tree.clone();
    layout.domains.insert(key.clone(), d);

    let obs = vec![eng("w1", &key, 0, 400), eng("w2", &key, 400, 400)];
    let outcome = layout
        .run_local_drag_drop(&desc, 0, &win("w1"), &win("w1"), &obs, 780, 200, None)
        .expect("local drag drop commits");

    assert_eq!(outcome.base_revision, 4);
    assert_eq!(outcome.revision, 5);

    let committed = layout.domains.get(&key).expect("domain retained");
    assert_eq!(committed.accepted_revision, 5);
    assert_eq!(committed.focused_leaf, Some(nid("a")));
    assert_ne!(committed.tree, original_tree);
}

#[test]
fn pair_preview_converges_without_installing_preview_topology_and_burns_serials() {
    let bounds_a = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let desc_a = domain_desc(&key_a, bounds_a, 0);
    let desc_b = domain_desc(&key_b, bounds_b, 0);

    let mut layout = WorldLayout::new();
    let d_a = two_tile_domain(bounds_a, 0, "g-a", "w1", "a1", "w2", "a2", 2);
    let d_b = two_tile_domain(bounds_b, 0, "g-b", "w3", "b1", "w4", "b2", 5);
    let orig_a_tree = d_a.tree.clone();
    let orig_b_tree = d_b.tree.clone();
    layout.domains.insert(key_a.clone(), d_a);
    layout.domains.insert(key_b.clone(), d_b);

    let obs = vec![
        eng("w1", &key_a, 0, 400),
        eng("w2", &key_a, 400, 400),
        eng("w3", &key_b, 800, 400),
        eng("w4", &key_b, 1200, 400),
    ];
    let ctx = PairOperationContext {
        source_domain: &desc_a,
        target_domain: &desc_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &win("w1"),
        observed: &obs,
        closed: &[],
    };

    let before_mint = layout.group_alloc.mint(&std::collections::BTreeSet::new());
    let preview = layout
        .run_pair_drag_preview(&ctx, &win("w1"), 1000, 5, None)
        .expect("pair drag preview succeeds");

    assert_eq!(preview.source_domain, key_a);
    assert_eq!(preview.domain, key_b);
    assert_eq!(preview.base_revision, 5);
    assert_eq!(preview.target_window, win("w3"));
    assert_eq!(before_mint, Some(nid("grp-0")));
    assert_ne!(
        layout.group_alloc.mint(&std::collections::BTreeSet::new()),
        Some(nid("grp-1"))
    );

    // Neither domain has preview topology installed; w1 remains in source.
    assert_eq!(
        layout.domains.get(&key_a).and_then(|d| d.tree.clone()),
        orig_a_tree
    );
    assert_eq!(
        layout.domains.get(&key_b).and_then(|d| d.tree.clone()),
        orig_b_tree
    );
    assert!(
        layout
            .domains
            .get(&key_a)
            .unwrap()
            .leaves
            .contains_key(&win("w1"))
    );
    assert!(
        !layout
            .domains
            .get(&key_b)
            .unwrap()
            .leaves
            .contains_key(&win("w1"))
    );
}

#[test]
fn pair_drop_moves_leaf_across_domains_and_installs_atomically() {
    let bounds_a = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let desc_a = domain_desc(&key_a, bounds_a, 0);
    let desc_b = domain_desc(&key_b, bounds_b, 0);

    let mut layout = WorldLayout::new();
    let d_a = two_tile_domain(bounds_a, 0, "g-a", "w1", "a1", "w2", "a2", 2);
    let d_b = two_tile_domain(bounds_b, 0, "g-b", "w3", "b1", "w4", "b2", 5);
    layout.domains.insert(key_a.clone(), d_a);
    layout.domains.insert(key_b.clone(), d_b);

    let obs = vec![
        eng("w1", &key_a, 0, 400),
        eng("w2", &key_a, 400, 400),
        eng("w3", &key_b, 800, 400),
        eng("w4", &key_b, 1200, 400),
    ];
    let ctx = PairOperationContext {
        source_domain: &desc_a,
        target_domain: &desc_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &win("w1"),
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_pair_drag_drop(&ctx, &win("w1"), 820, 200, None)
        .expect("pair drag drop commits");

    assert_eq!(outcome.base_revision, 5);
    assert_eq!(outcome.revision, 6);

    let src = layout.domains.get(&key_a).expect("source retained");
    let dst = layout.domains.get(&key_b).expect("target retained");
    assert!(!src.leaves.contains_key(&win("w1")));
    assert!(src.leaves.contains_key(&win("w2")));
    assert!(dst.leaves.contains_key(&win("w1")));
    assert_eq!(dst.focused_leaf, Some(nid("a1")));
    assert_eq!(dst.accepted_revision, 6);
}

#[test]
fn move_exception_float_with_placement_moves_atomically_and_refuses_invalid() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");

    let mut layout = WorldLayout::new();
    let mut d_a = DomainLayout::new(bounds, 0, 0);
    d_a.exceptions.insert(
        win("f1"),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: None,
        },
    );
    d_a.retained_float_geometry.insert(
        win("f1"),
        Rect {
            x: 10,
            y: 10,
            w: 200,
            h: 150,
        },
    );
    d_a.admission_order.insert(win("f1"), 7);
    let d_b = DomainLayout::new(bounds, 0, 0);
    layout.domains.insert(key_a.clone(), d_a);
    layout.domains.insert(key_b.clone(), d_b);

    let new_rect = Rect {
        x: 900,
        y: 50,
        w: 300,
        h: 220,
    };

    // Invalid rect refuses without mutating either domain.
    assert!(!layout.move_exception_float_with_placement(
        &key_a,
        &key_b,
        &win("f1"),
        Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 10,
        },
    ));
    assert!(
        layout
            .domains
            .get(&key_a)
            .unwrap()
            .exceptions
            .contains_key(&win("f1"))
    );

    // Valid move transfers exception, float geometry, and admission order atomically.
    assert!(layout.move_exception_float_with_placement(&key_a, &key_b, &win("f1"), new_rect));
    let src = layout.domains.get(&key_a).unwrap();
    let dst = layout.domains.get(&key_b).unwrap();
    assert!(!src.exceptions.contains_key(&win("f1")));
    assert!(!src.retained_float_geometry.contains_key(&win("f1")));
    assert!(!src.admission_order.contains_key(&win("f1")));
    assert!(dst.exceptions.contains_key(&win("f1")));
    assert_eq!(dst.retained_float_geometry.get(&win("f1")), Some(&new_rect));
    assert_eq!(dst.admission_order.get(&win("f1")), Some(&7));
}

#[test]
fn local_drop_refuses_unknown_window_without_mutation() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        key.clone(),
        two_tile_domain(bounds, 0, "g-root", "w1", "a", "w2", "b", 1),
    );
    let obs = vec![eng("w1", &key, 0, 400), eng("w2", &key, 400, 400)];

    let err = layout
        .run_local_drag_drop(&desc, 0, &win("missing"), &win("w1"), &obs, 600, 200, None)
        .unwrap_err();
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::UnknownWindow));
    assert_eq!(layout.domains.get(&key).unwrap().accepted_revision, 1);
}

#[test]
fn fresh_drag_uses_legacy_defaults_over_nondefault_world_config() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    layout.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    layout.set_fresh_placement(FreshPlacement::CursorOutputCursorClosest);
    layout.set_fresh_cursor(Some((10, 10)));

    let mut fixed_single_axis = eng("w2", &key, 400, 400);
    fixed_single_axis.hints = WindowSizeHints {
        min_w: Some(400),
        max_w: Some(400),
        min_h: None,
        max_h: None,
    };
    let obs = vec![eng("w1", &key, 0, 400), fixed_single_axis];

    // Under nondefault fixed_size_admission=true + SingleAxis, w2 would float
    // and leave w1 alone (refusing with Unchanged). Legacy fresh drag config
    // keeps fixed_size_admission=false, BothAxes, default placement, cursor=None,
    // so both windows tile and drag preview/drop succeed.
    let preview = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, 750, 200, None)
        .expect("fresh preview uses legacy defaults");
    assert_eq!(preview.target_window, win("w2"));
    assert!(!layout.contains_domain(&key));

    let dropped = layout
        .run_local_drag_drop(&desc, 0, &win("w1"), &win("w1"), &obs, 750, 200, None)
        .expect("fresh drop uses legacy defaults");
    assert_eq!(dropped.revision, dropped.base_revision + 1);
    let installed = layout.domains.get(&key).expect("fresh domain installed");
    assert!(installed.leaves.contains_key(&win("w1")));
    assert!(installed.leaves.contains_key(&win("w2")));
    assert!(installed.exceptions.is_empty());
}

#[test]
fn pair_drag_refuses_missing_target_and_preserves_unusable_vs_absent_priority() {
    let bounds_a = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let desc_a = domain_desc(&key_a, bounds_a, 0);
    let desc_b = domain_desc(&key_b, bounds_b, 0);

    // 1. Missing target refuses before convergence, leaving domains untouched
    // and burning zero allocator serials.
    let mut layout = WorldLayout::new();
    let d_a = two_tile_domain(bounds_a, 0, "g-a", "w1", "a1", "w2", "a2", 2);
    let orig_a_tree = d_a.tree.clone();
    layout.domains.insert(key_a.clone(), d_a);
    let obs = vec![eng("w1", &key_a, 0, 400), eng("w2", &key_a, 400, 400)];
    let ctx = PairOperationContext {
        source_domain: &desc_a,
        target_domain: &desc_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &win("w1"),
        observed: &obs,
        closed: &[],
    };

    let err = layout
        .run_pair_drag_preview(&ctx, &win("w1"), 900, 200, None)
        .unwrap_err();
    assert_eq!(
        err,
        PairOperationFailure::Assembly(PairAssemblyFailure::TargetUnavailable)
    );
    assert!(!layout.contains_domain(&key_b));
    assert_eq!(
        layout.domains.get(&key_a).and_then(|d| d.tree.clone()),
        orig_a_tree
    );
    assert_eq!(
        layout.group_alloc.mint(&std::collections::BTreeSet::new()),
        Some(nid("grp-0"))
    );

    let drop_err = layout
        .run_pair_drag_drop(&ctx, &win("w1"), 900, 200, None)
        .unwrap_err();
    assert_eq!(
        drop_err,
        PairOperationFailure::Assembly(PairAssemblyFailure::TargetUnavailable)
    );
    assert!(!layout.contains_domain(&key_b));

    // 2. Absent source returns SourceUnavailable even when target is absent.
    let mut absent_source_layout = WorldLayout::new();
    let err_absent = absent_source_layout
        .run_pair_drag_preview(&ctx, &win("w1"), 900, 200, None)
        .unwrap_err();
    assert_eq!(
        err_absent,
        PairOperationFailure::Assembly(PairAssemblyFailure::SourceUnavailable)
    );

    // 3. Present unusable/empty source returns TargetUnusable ("canonical-pair-unusable")
    // before checking target.
    let mut unusable_source_layout = WorldLayout::new();
    unusable_source_layout
        .domains
        .insert(key_a.clone(), DomainLayout::new(bounds_a, 0, 0));
    let err_unusable = unusable_source_layout
        .run_pair_drag_preview(&ctx, &win("w1"), 900, 200, None)
        .unwrap_err();
    assert_eq!(
        err_unusable,
        PairOperationFailure::Assembly(PairAssemblyFailure::TargetUnusable)
    );
}

#[test]
fn fresh_empty_cross_source_singleton_preview_and_drop_preserve_ordinary_projection() {
    let mut layout = WorldLayout::new();
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let desc_b = domain_desc(&key_b, bounds_b, 0);
    let obs = vec![eng("w1", &key_b, 810, 300)];

    let preview = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_a),
        )
        .expect("fresh cross-output singleton preview succeeds");
    assert_eq!(preview.domain, key_b);
    assert_eq!(preview.source_domain, key_a);
    assert_eq!(preview.proposed_rect, bounds_b);
    assert_eq!(preview.hover.prior, None);
    assert!(!layout.contains_domain(&key_b));

    let drop = layout
        .run_local_drag_drop_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_a),
        )
        .expect("fresh cross-output singleton drop commits");
    assert_eq!(drop.base_revision, 1);
    assert_eq!(drop.revision, 1);
    assert!(layout.contains_domain(&key_b));
    assert!(layout.is_tiled_window(&key_b, &win("w1")));
    assert_eq!(layout.accepted_revision(&key_b), 1);
}

#[test]
fn same_output_singleton_drag_with_source_refuses_unchanged() {
    let mut layout = WorldLayout::new();
    let key_b = ws_key("out-b", "ws-1");
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let desc_b = domain_desc(&key_b, bounds_b, 0);
    let obs = vec![eng("w1", &key_b, 810, 300)];

    let preview_err = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_b),
        )
        .unwrap_err();
    assert_eq!(
        preview_err,
        LayoutFailure::Semantics(RefusalKind::Unchanged)
    );

    let drop_err = layout
        .run_local_drag_drop_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_b),
        )
        .unwrap_err();
    assert_eq!(drop_err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert!(!layout.contains_domain(&key_b));
}

#[test]
fn fresh_mixed_one_tile_plus_float_exception_cross_source_singleton_preview_and_drop_succeed() {
    let mut layout = WorldLayout::new();
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let desc_b = domain_desc(&key_b, bounds_b, 0);
    let mut float_win = eng("w-float", &key_b, 850, 200);
    float_win.floating = true;
    let obs = vec![eng("w1", &key_b, 810, 300), float_win];

    let preview = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_a),
        )
        .expect("mixed one-tile + float-exception singleton preview succeeds");
    assert_eq!(preview.domain, key_b);
    assert_eq!(preview.source_domain, key_a);
    assert_eq!(preview.proposed_rect, bounds_b);
    assert!(!layout.contains_domain(&key_b));

    let drop = layout
        .run_local_drag_drop_with_source(
            &desc_b,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            900,
            200,
            None,
            Some(&key_a),
        )
        .expect("mixed one-tile + float-exception singleton drop commits");
    assert_eq!(drop.base_revision, 1);
    assert_eq!(drop.revision, 1);
    assert!(layout.contains_domain(&key_b));
    assert!(layout.is_tiled_window(&key_b, &win("w1")));
    assert!(layout.is_exception(&key_b, &win("w-float")));
}

#[test]
fn fresh_empty_singleton_producer_shape_projects() {
    let mut layout = WorldLayout::new();
    let source_key = ws_key("o1", "ws1");
    let source_desc = domain_desc(
        &source_key,
        Rect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1380,
        },
        8,
    );
    let (source_dom, counter) = layout
        .build_tiled_seed(&source_desc, 8, &[eng("w29", &source_key, 8, 2544)])
        .expect("seeds sole-mover source");
    layout.admission_seq_next = counter;
    layout.domains.insert(source_key.clone(), source_dom);

    let dest_key = ws_key("o2", "ws1");
    let dest_desc = domain_desc(
        &dest_key,
        Rect {
            x: 2560,
            y: 355,
            w: 1920,
            h: 1032,
        },
        8,
    );
    assert!(!layout.contains_domain(&dest_key));

    let mover = EngineWindow {
        window: win("w29"),
        output: dest_key.output.clone(),
        workspace: dest_key.workspace.clone(),
        rect: Rect {
            x: 8,
            y: 8,
            w: 2544,
            h: 1364,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints {
            min_w: Some(864),
            min_h: Some(617),
            max_w: None,
            max_h: None,
        },
    };
    let obs = vec![mover];

    let preview = layout
        .run_local_drag_preview_with_source(
            &dest_desc,
            8,
            &win("w29"),
            &win("w29"),
            &obs,
            3520,
            871,
            None,
            Some(&source_key),
        )
        .expect("producer singleton preview projects");
    assert!(!layout.contains_domain(&dest_key));

    let drop = layout
        .run_local_drag_drop_with_source(
            &dest_desc,
            8,
            &win("w29"),
            &win("w29"),
            &obs,
            3520,
            871,
            None,
            Some(&source_key),
        )
        .expect("producer singleton drop admits");
    assert_eq!(drop.revision, 1);
    let hints = BTreeMap::from([(
        win("w29"),
        WindowSizeHints {
            min_w: Some(864),
            min_h: Some(617),
            max_w: None,
            max_h: None,
        },
    )]);
    let geom = layout
        .project_domain_geometry(&dest_key, &hints)
        .expect("projects dropped domain");
    assert_eq!(geom.len(), 1);
    assert_eq!(geom[0].rect, preview.proposed_rect);
    assert!(layout.contains_domain(&dest_key));
}

#[test]
fn pair_drop_top_edge_wraps_root_and_retires_sole_source() {
    let bounds_a = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let bounds_b = Rect {
        x: -1920,
        y: -200,
        w: 1920,
        h: 1080,
    };
    let key_a = ws_key("out-1", "ws-1");
    let key_b = ws_key("out-2", "ws-1");
    let desc_a = domain_desc(&key_a, bounds_a, 0);
    let desc_b = domain_desc(&key_b, bounds_b, 0);

    let mut layout = WorldLayout::new();
    let (d_a, c1) = layout
        .build_tiled_seed(&desc_a, 0, &[eng("win-b", &key_a, 0, 800)])
        .expect("seed a");
    let (d_b, c2) = layout
        .build_tiled_seed(&desc_b, 0, &[eng("win-c", &key_b, -1920, 1920)])
        .expect("seed b");
    layout.admission_seq_next = c1.max(c2);
    layout.domains.insert(key_a.clone(), d_a);
    layout.domains.insert(key_b.clone(), d_b);

    let obs = vec![
        eng("win-b", &key_a, 0, 800),
        eng("win-c", &key_b, -1920, 1920),
    ];
    let ctx = PairOperationContext {
        source_domain: &desc_a,
        target_domain: &desc_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &win("win-b"),
        observed: &obs,
        closed: &[],
    };

    let outcome = layout
        .run_pair_drag_drop(&ctx, &win("win-b"), -960, -190, None)
        .expect("cross-output wrap drop commits");
    assert_eq!(outcome.revision, outcome.base_revision + 1);
    let geom = layout
        .project_domain_geometry(&key_b, &BTreeMap::new())
        .expect("projects destination");
    assert_eq!(geom.len(), 2);
    let mover_rect = geom
        .iter()
        .find(|g| g.window == win("win-b"))
        .expect("mover geometry")
        .rect;
    assert_eq!(
        mover_rect,
        Rect {
            x: -1920,
            y: -200,
            w: 1920,
            h: 540,
        }
    );
    assert!(!layout.contains_domain(&key_a), "sole source retires");
    let dst = layout.domain(&key_b).expect("dest retained");
    assert!(dst.leaves.contains_key(&win("win-b")));
    assert!(dst.leaves.contains_key(&win("win-c")));
}

#[test]
fn pair_drop_refuses_absent_mover_without_mutating_domains() {
    let bounds_a = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let bounds_b = Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 400,
    };
    let key_a = ws_key("out-a", "ws-1");
    let key_b = ws_key("out-b", "ws-1");
    let desc_a = domain_desc(&key_a, bounds_a, 0);
    let desc_b = domain_desc(&key_b, bounds_b, 0);

    let mut layout = WorldLayout::new();
    let d_a = two_tile_domain(bounds_a, 0, "g-a", "w1", "a1", "w2", "a2", 2);
    let d_b = two_tile_domain(bounds_b, 0, "g-b", "w3", "b1", "w4", "b2", 5);
    let orig_b_tree = d_b.tree.clone();
    layout.domains.insert(key_a.clone(), d_a);
    layout.domains.insert(key_b.clone(), d_b);

    // Mover w2 is absent from the combined observation: source converges the
    // departure, then the drop refuses without committing any drop to target.
    let obs = vec![
        eng("w1", &key_a, 0, 400),
        eng("w3", &key_b, 800, 400),
        eng("w4", &key_b, 1200, 400),
    ];
    let ctx = PairOperationContext {
        source_domain: &desc_a,
        target_domain: &desc_b,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &win("w1"),
        observed: &obs,
        closed: &[],
    };

    let err = layout
        .run_pair_drag_drop(&ctx, &win("w2"), 820, 200, None)
        .unwrap_err();
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );
    let dst = layout.domain(&key_b).expect("dest retained");
    assert_eq!(dst.tree, orig_b_tree);
    assert_eq!(dst.leaves.len(), 2);
    assert!(!dst.leaves.contains_key(&win("w2")));
    assert!(
        !layout
            .domain(&key_a)
            .unwrap()
            .leaves
            .contains_key(&win("w2"))
    );
}

#[test]
fn retained_empty_arrival_preview_matches_drop() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let key_a = ws_key("out-1", "ws-1");
    let key_b = ws_key("out-2", "ws-1");
    let desc_b = domain_desc(&key_b, bounds, 0);

    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(key_b.clone(), DomainLayout::new(bounds, 0, 0));
    let obs = vec![eng("win-a", &key_b, 0, 100)];

    let preview = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("win-a"),
            &win("win-a"),
            &obs,
            400,
            300,
            None,
            Some(&key_a),
        )
        .expect("retained empty preview projects arrival");
    assert!(layout.domain(&key_b).unwrap().leaves.is_empty());

    layout
        .converge_retained_observation(&key_b, &obs, &win("win-a"), &[])
        .expect("converges retained empty arrival")
        .expect("domain present");
    let drop = layout
        .run_local_drag_drop_with_source(
            &desc_b,
            0,
            &win("win-a"),
            &win("win-a"),
            &obs,
            400,
            300,
            None,
            Some(&key_a),
        )
        .expect("retained empty drop commits");
    assert_eq!(drop.revision, 1);
    let geom = layout
        .project_domain_geometry(&key_b, &BTreeMap::new())
        .expect("projects dropped domain");
    assert_eq!(geom[0].rect, preview.proposed_rect);
    assert!(
        layout
            .domain(&key_b)
            .unwrap()
            .leaves
            .contains_key(&win("win-a"))
    );
}

#[test]
fn drag_preview_drop_parity_with_hints_and_prior() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let key_a = ws_key("out-1", "ws-1");
    let key_b = ws_key("out-2", "ws-1");
    let desc_b = domain_desc(&key_b, bounds, 0);

    let mut layout = WorldLayout::new();
    let (d_b, counter) = layout
        .build_tiled_seed(&desc_b, 0, &[eng("win-a", &key_b, 0, 800)])
        .expect("seed retained destination");
    layout.admission_seq_next = counter;
    layout.domains.insert(key_b.clone(), d_b);

    let mut win_a = eng("win-a", &key_b, 0, 800);
    win_a.hints.min_w = Some(200);
    let mut win_m = eng("win-m", &key_b, 0, 200);
    win_m.hints.min_w = Some(200);
    let obs = vec![win_a, win_m];

    let preview = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("win-m"),
            &win("win-m"),
            &obs,
            5,
            300,
            None,
            Some(&key_a),
        )
        .expect("hinted preview projects");
    assert!(
        !layout
            .domain(&key_b)
            .unwrap()
            .leaves
            .contains_key(&win("win-m"))
    );

    let followup = layout
        .run_local_drag_preview_with_source(
            &desc_b,
            0,
            &win("win-m"),
            &win("win-m"),
            &obs,
            5,
            300,
            Some(&preview.hover),
            Some(&key_a),
        )
        .expect("carried prior preview projects");
    assert_eq!(followup.proposed_rect, preview.proposed_rect);

    layout
        .converge_retained_observation(&key_b, &obs, &win("win-m"), &[])
        .expect("converges arrival before drop")
        .expect("domain present");
    let drop = layout
        .run_local_drag_drop_with_source(
            &desc_b,
            0,
            &win("win-m"),
            &win("win-m"),
            &obs,
            5,
            300,
            Some(&preview.hover),
            Some(&key_a),
        )
        .expect("hinted drop commits");
    assert_eq!(drop.revision, drop.base_revision + 1);
    let hints = BTreeMap::from([
        (
            win("win-a"),
            WindowSizeHints {
                min_w: Some(200),
                ..WindowSizeHints::none()
            },
        ),
        (
            win("win-m"),
            WindowSizeHints {
                min_w: Some(200),
                ..WindowSizeHints::none()
            },
        ),
    ]);
    let geom = layout
        .project_domain_geometry(&key_b, &hints)
        .expect("projects hinted destination");
    let moved = geom
        .iter()
        .find(|g| g.window == win("win-m"))
        .expect("mover geometry");
    assert_eq!(moved.rect, preview.proposed_rect);
    assert!(
        layout
            .domain(&key_b)
            .unwrap()
            .leaves
            .contains_key(&win("win-m"))
    );
}

#[test]
fn local_drag_insertion_index_sixty_five_succeeds_without_count_cap() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 6600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut children = Vec::with_capacity(66);
    let mut leaves = BTreeMap::new();
    let mut focus_stack = Vec::with_capacity(66);
    let mut obs = Vec::with_capacity(66);
    for i in 0..66 {
        let wid = format!("w{i}");
        let lid = format!("leaf-{i}");
        children.push(crate::directional::Node::Leaf { id: nid(&lid) });
        leaves.insert(win(&wid), nid(&lid));
        focus_stack.push(nid(&lid));
        obs.push(eng(&wid, &key, (i as i32) * 100, 100));
    }

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        key.clone(),
        super::state::DomainLayout {
            tree: Some(crate::directional::Node::Group {
                id: nid("g-root"),
                axis: crate::directional::Axis::Horizontal,
                children,
                shares: vec![1; 66],
            }),
            leaves,
            bounds,
            gap: 0,
            outer_gap: 0,
            exceptions: BTreeMap::new(),
            focused_leaf: Some(nid("leaf-0")),
            focus_stack,
            focus_history: Vec::new(),
            last_active: None,
            retained_float_geometry: BTreeMap::new(),
            automatic_fixed: std::collections::BTreeSet::new(),
            fixed_tile_override: std::collections::BTreeSet::new(),
            capacity_floats: std::collections::BTreeSet::new(),
            admission_order: BTreeMap::new(),
            minimized_slots: BTreeMap::new(),
            accepted_revision: 1,
        },
    );

    // Drag w0 (index 0) onto the right edge of w65 (index 65 after w0 is
    // removed -> insertion_index == 65) and verify no count cap blocks it.
    let outcome = layout
        .run_local_drag_drop(&desc, 0, &win("w0"), &win("w0"), &obs, 6595, 200, None)
        .expect("65+ insertion index drop commits");
    assert_eq!(outcome.base_revision, 1);
    assert_eq!(outcome.revision, 2);

    let geom = layout
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("projects 66 leaves");
    assert_eq!(geom.len(), 66);
    let w0_rect = geom
        .iter()
        .find(|g| g.window == win("w0"))
        .expect("w0 geometry")
        .rect;
    let w65_rect = geom
        .iter()
        .find(|g| g.window == win("w65"))
        .expect("w65 geometry")
        .rect;
    assert!(w0_rect.x > w65_rect.x, "w0 inserted after w65 at index 65");
}

#[test]
fn local_drop_perpendicular_edge_wraps_root_and_self_drop_refuses() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let d = two_tile_domain(bounds, 0, "g-root", "w1", "a", "w2", "b", 4);
    let original_tree = d.tree.clone();
    layout.domains.insert(key.clone(), d);

    let obs = vec![eng("w1", &key, 0, 400), eng("w2", &key, 400, 400)];

    // Self-drop onto w1 refuses without mutating topology or revision.
    assert!(
        layout
            .run_local_drag_drop(&desc, 0, &win("w1"), &win("w1"), &obs, 200, 200, None)
            .is_err()
    );
    assert_eq!(
        layout.domains.get(&key).map(|d| d.accepted_revision),
        Some(4)
    );
    assert_eq!(
        layout.domains.get(&key).and_then(|d| d.tree.clone()),
        original_tree
    );

    // Dropping w1 onto the top edge of w2 removes w1 from the 2-leaf root,
    // leaving w2 as sole root, and wraps the root into a vertical group at
    // insertion_index 0.
    let outcome = layout
        .run_local_drag_drop(&desc, 0, &win("w1"), &win("w1"), &obs, 600, 5, None)
        .expect("root wrap drop commits");
    assert_eq!(outcome.base_revision, 4);
    assert_eq!(outcome.revision, 5);

    let geom = layout
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("projects wrapped root");
    let w1_rect = geom.iter().find(|g| g.window == win("w1")).unwrap().rect;
    let w2_rect = geom.iter().find(|g| g.window == win("w2")).unwrap().rect;
    assert_eq!(
        w1_rect,
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 200
        }
    );
    assert_eq!(
        w2_rect,
        Rect {
            x: 0,
            y: 200,
            w: 800,
            h: 200
        }
    );
}

#[test]
fn drag_sides_map_to_axis_and_order_across_horizontal_and_vertical_parents() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    // 1) Two-wide H[w1, w2], drag w2 left onto w1 -> collapses root to w1, wraps H[w2, w1].
    let mut layout = WorldLayout::new();
    let d2 = two_tile_domain(bounds, 0, "g-root", "w1", "l1", "w2", "l2", 1);
    layout.domains.insert(key.clone(), d2);
    let obs2 = vec![eng("w1", &key, 0, 300), eng("w2", &key, 300, 300)];
    let left = layout
        .run_local_drag_preview(&desc, 0, &win("w2"), &win("w2"), &obs2, 10, 200, None)
        .expect("left preview");
    assert_eq!(left.target_window, win("w1"));
    assert_eq!(
        left.proposed_rect,
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 400
        }
    );

    // 2) Three-wide H[w1, w2, w3], drag w3 right onto w1 -> H[w1, w3, w2] (same-axis, no wrap).
    let mut layout3 = WorldLayout::new();
    let mut d3 = DomainLayout::new(bounds, 0, 0);
    d3.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("l1"), leaf("l2"), leaf("l3")],
        shares: vec![1, 1, 1],
    });
    d3.leaves.insert(win("w1"), nid("l1"));
    d3.leaves.insert(win("w2"), nid("l2"));
    d3.leaves.insert(win("w3"), nid("l3"));
    d3.focused_leaf = Some(nid("l3"));
    d3.focus_stack = vec![nid("l1"), nid("l2"), nid("l3")];
    d3.accepted_revision = 1;
    layout3.domains.insert(key.clone(), d3);
    let obs3 = vec![
        eng("w1", &key, 0, 200),
        eng("w2", &key, 200, 200),
        eng("w3", &key, 400, 200),
    ];
    let right_drop = layout3
        .run_local_drag_drop(&desc, 0, &win("w3"), &win("w3"), &obs3, 190, 200, None)
        .expect("right drop");
    assert_eq!(right_drop.revision, 2);
    match layout3.domain(&key).unwrap().tree.as_ref().unwrap() {
        Node::Group {
            axis: Axis::Horizontal,
            children,
            ..
        } => {
            assert_eq!(children, &vec![leaf("l1"), leaf("l3"), leaf("l2")]);
        }
        other => panic!("expected H group, got {other:?}"),
    }

    // 3) Nested H[w1, V[w2, w3, w4]], drag w4 top/bottom onto w2 -> stays in V parent.
    let mut layout4 = WorldLayout::new();
    let mut d4 = DomainLayout::new(bounds, 0, 0);
    d4.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("l1"),
            Node::Group {
                id: nid("g-v"),
                axis: Axis::Vertical,
                children: vec![leaf("l2"), leaf("l3"), leaf("l4")],
                shares: vec![1, 1, 1],
            },
        ],
        shares: vec![1, 1],
    });
    d4.leaves.insert(win("w1"), nid("l1"));
    d4.leaves.insert(win("w2"), nid("l2"));
    d4.leaves.insert(win("w3"), nid("l3"));
    d4.leaves.insert(win("w4"), nid("l4"));
    d4.focused_leaf = Some(nid("l4"));
    d4.focus_stack = vec![nid("l1"), nid("l2"), nid("l3"), nid("l4")];
    d4.accepted_revision = 1;
    layout4.domains.insert(key.clone(), d4);
    let obs4 = vec![
        eng("w1", &key, 0, 300),
        eng("w2", &key, 300, 300),
        eng("w3", &key, 300, 300),
        eng("w4", &key, 300, 300),
    ];
    let top = layout4
        .run_local_drag_preview(&desc, 0, &win("w4"), &win("w4"), &obs4, 450, 10, None)
        .expect("top preview");
    assert_eq!(top.target_window, win("w2"));
    assert_eq!(
        top.proposed_rect,
        Rect {
            x: 300,
            y: 0,
            w: 300,
            h: 133
        }
    );
    let bottom_drop = layout4
        .run_local_drag_drop(&desc, 0, &win("w4"), &win("w4"), &obs4, 450, 120, None)
        .expect("bottom drop");
    assert_eq!(bottom_drop.revision, 2);
    match layout4.domain(&key).unwrap().tree.as_ref().unwrap() {
        Node::Group { children, .. } => match &children[1] {
            Node::Group {
                id,
                axis: Axis::Vertical,
                children,
                ..
            } => {
                assert_eq!(id, &nid("g-v"));
                assert_eq!(children, &vec![leaf("l2"), leaf("l4"), leaf("l3")]);
            }
            other => panic!("expected V group, got {other:?}"),
        },
        other => panic!("expected H root, got {other:?}"),
    }
}

#[test]
fn drag_same_axis_nary_reorder_and_focus_preserved() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let mut d = DomainLayout::new(bounds, 0, 0);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("l1"), leaf("l2"), leaf("l3")],
        shares: vec![1, 1, 1],
    });
    d.leaves.insert(win("w1"), nid("l1"));
    d.leaves.insert(win("w2"), nid("l2"));
    d.leaves.insert(win("w3"), nid("l3"));
    d.focused_leaf = Some(nid("l3"));
    d.focus_stack = vec![nid("l1"), nid("l2"), nid("l3")];
    d.accepted_revision = 1;
    layout.domains.insert(key.clone(), d);
    let obs = vec![
        eng("w1", &key, 0, 200),
        eng("w2", &key, 200, 200),
        eng("w3", &key, 400, 200),
    ];

    layout
        .run_local_drag_drop(&desc, 0, &win("w3"), &win("w3"), &obs, 10, 200, None)
        .expect("reorder drop");
    let dom = layout.domain(&key).unwrap();
    assert_eq!(dom.focused_leaf, Some(nid("l3")));
    match dom.tree.as_ref().unwrap() {
        Node::Group {
            axis: Axis::Horizontal,
            children,
            ..
        } => {
            assert_eq!(children, &vec![leaf("l3"), leaf("l1"), leaf("l2")]);
        }
        other => panic!("expected H root, got {other:?}"),
    }
}

#[test]
fn drag_perpendicular_wraps_only_target_subtree() {
    // H[w1, w2, w3] focus w3 top onto w1: remove w3 -> H[w1, w2], wrap w1 in V[w3, w1];
    // w2 untouched at slot 1 with original share.
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let mut d = DomainLayout::new(bounds, 0, 0);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("l1"), leaf("l2"), leaf("l3")],
        shares: vec![1, 1, 1],
    });
    d.leaves.insert(win("w1"), nid("l1"));
    d.leaves.insert(win("w2"), nid("l2"));
    d.leaves.insert(win("w3"), nid("l3"));
    d.focused_leaf = Some(nid("l3"));
    d.focus_stack = vec![nid("l1"), nid("l2"), nid("l3")];
    d.accepted_revision = 1;
    layout.domains.insert(key.clone(), d);
    let obs = vec![
        eng("w1", &key, 0, 200),
        eng("w2", &key, 200, 200),
        eng("w3", &key, 400, 200),
    ];

    layout
        .run_local_drag_drop(&desc, 0, &win("w3"), &win("w3"), &obs, 100, 10, None)
        .expect("perpendicular wrap drop");
    let dom = layout.domain(&key).unwrap();
    assert_eq!(dom.focused_leaf, Some(nid("l3")));
    match dom.tree.as_ref().unwrap() {
        Node::Group {
            axis: Axis::Horizontal,
            children,
            shares,
            ..
        } => {
            assert_eq!(shares, &vec![1, 1]);
            assert_eq!(children.len(), 2);
            assert_eq!(children[1], leaf("l2"));
            match &children[0] {
                Node::Group {
                    axis: Axis::Vertical,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(shares, &vec![1, 1]);
                    assert_eq!(children, &vec![leaf("l3"), leaf("l1")]);
                }
                other => panic!("expected V wrapper, got {other:?}"),
            }
        }
        other => panic!("expected H root, got {other:?}"),
    }
}

#[test]
fn drag_deep_nesting_collapse_and_wrap() {
    // H[w1, V[w2, w3, w4]] focus w4 left onto w2:
    // remove w4 -> V[w2, w3], wrap w2 in H[w4, w2] at V index 0; w1 and w3 untouched.
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let mut d = DomainLayout::new(bounds, 0, 0);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("l1"),
            Node::Group {
                id: nid("g-v"),
                axis: Axis::Vertical,
                children: vec![leaf("l2"), leaf("l3"), leaf("l4")],
                shares: vec![1, 1, 1],
            },
        ],
        shares: vec![1, 1],
    });
    d.leaves.insert(win("w1"), nid("l1"));
    d.leaves.insert(win("w2"), nid("l2"));
    d.leaves.insert(win("w3"), nid("l3"));
    d.leaves.insert(win("w4"), nid("l4"));
    d.focused_leaf = Some(nid("l4"));
    d.focus_stack = vec![nid("l1"), nid("l2"), nid("l3"), nid("l4")];
    d.accepted_revision = 1;
    layout.domains.insert(key.clone(), d);
    let obs = vec![
        eng("w1", &key, 0, 300),
        eng("w2", &key, 300, 300),
        eng("w3", &key, 300, 300),
        eng("w4", &key, 300, 300),
    ];

    layout
        .run_local_drag_drop(&desc, 0, &win("w4"), &win("w4"), &obs, 310, 60, None)
        .expect("deep collapse and wrap drop");
    match layout.domain(&key).unwrap().tree.as_ref().unwrap() {
        Node::Group { children, .. } => {
            assert_eq!(children.len(), 2);
            assert_eq!(children[0], leaf("l1"));
            match &children[1] {
                Node::Group {
                    children, shares, ..
                } => {
                    assert_eq!(shares, &vec![1, 1]);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[1], leaf("l3"));
                    match &children[0] {
                        Node::Group {
                            axis: Axis::Horizontal,
                            children,
                            shares,
                            ..
                        } => {
                            assert_eq!(shares, &vec![1, 1]);
                            assert_eq!(children, &vec![leaf("l4"), leaf("l2")]);
                        }
                        other => panic!("expected H wrapper, got {other:?}"),
                    }
                }
                other => panic!("expected V inner, got {other:?}"),
            }
        }
        other => panic!("expected H root, got {other:?}"),
    }
}

#[test]
fn drag_preview_and_drop_refuse_center_and_bounds_without_mutation() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 600,
        h: 400,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 0);

    let mut layout = WorldLayout::new();
    let d = two_tile_domain(bounds, 0, "g-root", "w1", "l1", "w2", "l2", 3);
    let original_tree = d.tree.clone();
    layout.domains.insert(key.clone(), d);
    let obs = vec![eng("w1", &key, 0, 300), eng("w2", &key, 300, 300)];

    // Self drop refuses Unchanged.
    assert_eq!(
        layout
            .run_local_drag_preview(&desc, 0, &win("w2"), &win("w2"), &obs, 450, 200, None)
            .unwrap_err(),
        LayoutFailure::Semantics(RefusalKind::Unchanged)
    );
    // Target center refuses UnsupportedCapability.
    assert_eq!(
        layout
            .run_local_drag_preview(&desc, 0, &win("w2"), &win("w2"), &obs, 150, 200, None)
            .unwrap_err(),
        LayoutFailure::Semantics(RefusalKind::UnsupportedCapability)
    );
    assert_eq!(
        layout
            .run_local_drag_drop(&desc, 0, &win("w2"), &win("w2"), &obs, 150, 200, None)
            .unwrap_err(),
        LayoutFailure::Semantics(RefusalKind::UnsupportedCapability)
    );
    // Outside work area refuses CrossDomainMismatch.
    assert_eq!(
        layout
            .run_local_drag_preview(&desc, 0, &win("w2"), &win("w2"), &obs, 700, 200, None)
            .unwrap_err(),
        LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)
    );
    assert_eq!(
        layout
            .run_local_drag_preview(&desc, 0, &win("w2"), &win("w2"), &obs, -1, 200, None)
            .unwrap_err(),
        LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)
    );

    let dom = layout.domain(&key).unwrap();
    assert_eq!(dom.accepted_revision, 3);
    assert_eq!(dom.tree, original_tree);
}

#[test]
fn drag_bounded_invariant_loop_preserves_domain_isolation() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let key_a = ws_key("out-1", "ws-1");
    let key_b = ws_key("out-2", "ws-1");
    let desc_a = domain_desc(&key_a, bounds, 0);

    let mut layout = WorldLayout::new();
    let mut d_a = DomainLayout::new(bounds, 0, 0);
    d_a.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("l1"), leaf("l2"), leaf("l3")],
        shares: vec![1, 1, 1],
    });
    d_a.leaves.insert(win("w1"), nid("l1"));
    d_a.leaves.insert(win("w2"), nid("l2"));
    d_a.leaves.insert(win("w3"), nid("l3"));
    d_a.focused_leaf = Some(nid("l3"));
    d_a.focus_stack = vec![nid("l1"), nid("l2"), nid("l3")];
    d_a.accepted_revision = 2;
    let orig_tree = d_a.tree.clone();
    layout.domains.insert(key_a.clone(), d_a);
    layout
        .domains
        .insert(key_b.clone(), DomainLayout::new(bounds, 0, 0));

    let obs = vec![
        eng("w1", &key_a, 0, 40),
        eng("w2", &key_a, 40, 40),
        eng("w3", &key_a, 80, 40),
    ];
    let mut ok = 0u32;
    let mut refused = 0u32;
    for x in (0..120).step_by(10) {
        for y in (0..80).step_by(10) {
            match layout.run_local_drag_preview(
                &desc_a,
                0,
                &win("w3"),
                &win("w3"),
                &obs,
                x,
                y,
                None,
            ) {
                Ok(preview) => {
                    ok += 1;
                    assert_eq!(preview.domain, key_a);
                    assert_eq!(preview.source_domain, key_a);
                    assert_ne!(preview.target_window, win("w3"));
                    assert!(preview.proposed_rect.w > 0 && preview.proposed_rect.h > 0);
                }
                Err(LayoutFailure::Semantics(_)) => refused += 1,
                Err(other) => panic!("preview must not fail with {other:?}"),
            }
            assert_eq!(layout.domain(&key_a).unwrap().tree, orig_tree);
            assert_eq!(layout.domain(&key_a).unwrap().accepted_revision, 2);
            assert!(layout.domain(&key_b).unwrap().tree.is_none());
        }
    }
    assert!(ok > 0);
    assert!(refused > 0);
}

fn gap_nested_layout() -> (WorldLayout, OutputDomain, DomainKey, Vec<EngineWindow>) {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 8);
    let mut layout = WorldLayout::new();
    let mut d = DomainLayout::new(bounds, 8, 0);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("l1"),
            Node::Group {
                id: nid("g-v"),
                axis: Axis::Vertical,
                children: vec![leaf("l2"), leaf("l3")],
                shares: vec![1, 1],
            },
        ],
        shares: vec![1, 1],
    });
    d.leaves.insert(win("w1"), nid("l1"));
    d.leaves.insert(win("w2"), nid("l2"));
    d.leaves.insert(win("w3"), nid("l3"));
    d.focused_leaf = Some(nid("l1"));
    d.focus_stack = vec![nid("l2"), nid("l3"), nid("l1")];
    d.accepted_revision = 1;
    layout.domains.insert(key.clone(), d);
    let obs = vec![
        eng("w1", &key, 0, 400),
        eng("w2", &key, 400, 400),
        eng("w3", &key, 400, 400),
    ];
    (layout, desc, key, obs)
}

fn inner_v_group_geom(layout: &WorldLayout, key: &DomainKey) -> (Rect, Vec<i64>, Vec<Rect>) {
    let geom = layout
        .project_domain_geometry(key, &BTreeMap::new())
        .expect("project");
    let r2 = geom.iter().find(|g| g.window == win("w2")).unwrap().rect;
    let r3 = geom.iter().find(|g| g.window == win("w3")).unwrap().rect;
    let min_x = r2.x.min(r3.x);
    let min_y = r2.y.min(r3.y);
    let max_r = (r2.x + r2.w).max(r3.x + r3.w);
    let max_b = (r2.y + r2.h).max(r3.y + r3.h);
    let group_rect = Rect {
        x: min_x,
        y: min_y,
        w: max_r - min_x,
        h: max_b - min_y,
    };
    let leaf_rects: Vec<Rect> = geom.into_iter().map(|g| g.rect).collect();
    (
        group_rect,
        vec![i64::from(r2.y), i64::from(r3.y)],
        leaf_rects,
    )
}

fn rect_contains(r: &Rect, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}

#[test]
fn drag_prior_carry_validates_source_and_restores_sticky_edge() {
    let (mut layout, desc, key, obs) = gap_nested_layout();
    let (inner_rect, _, leaf_rects) = inner_v_group_geom(&layout, &key);
    let cx = inner_rect.x + inner_rect.w / 2;
    let gy = (inner_rect.y..inner_rect.y + inner_rect.h)
        .find(|&y| {
            !leaf_rects.iter().any(|r| rect_contains(r, cx, y))
                && y - inner_rect.y >= 32
                && inner_rect.y + inner_rect.h - y > 32
        })
        .expect("inner gap row");
    let gx = inner_rect.x + 5;
    let sticky_x = inner_rect.x + 50;
    assert!(!leaf_rects.iter().any(|r| rect_contains(r, gx, gy)));
    assert!(!leaf_rects.iter().any(|r| rect_contains(r, sticky_x, gy)));

    let edge = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, gx, gy, None)
        .expect("edge preview");
    let prior = edge
        .hover
        .prior
        .clone()
        .expect("group edge hover arms sticky prior");
    assert_eq!(prior.group, nid("g-v"));
    assert_eq!(prior.edge, crate::contract::DragSide::Left);

    // Without prior, sticky_x (50px inside) misses the Left edge (32px default depth).
    let missed = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, sticky_x, gy, None)
        .expect("uncarried probe");
    assert_ne!(
        missed.hover.prior.map(|p| (p.group, p.edge)),
        Some((nid("g-v"), crate::contract::DragSide::Left))
    );

    // Mismatched carried priors (wrong revision / wrong source_window / wrong prior_domain)
    // are ignored without steering.
    let mut bad_rev = edge.hover.clone();
    bad_rev.revision += 1;
    let missed_rev = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            sticky_x,
            gy,
            Some(&bad_rev),
        )
        .expect("bad revision ignored");
    assert_eq!(missed_rev.proposed_rect, missed.proposed_rect);

    let mut bad_src = edge.hover.clone();
    bad_src.source_window = win("w2");
    let missed_src = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            sticky_x,
            gy,
            Some(&bad_src),
        )
        .expect("bad source ignored");
    assert_eq!(missed_src.proposed_rect, missed.proposed_rect);

    let mut bad_dom = edge.hover.clone();
    bad_dom.prior_domain = ws_key("out-9", "ws-1");
    let missed_dom = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            sticky_x,
            gy,
            Some(&bad_dom),
        )
        .expect("bad prior_domain ignored");
    assert_eq!(missed_dom.proposed_rect, missed.proposed_rect);

    // Exact carried prior restores the 80px sticky Left edge for preview and drop.
    let sticky = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            sticky_x,
            gy,
            Some(&edge.hover),
        )
        .expect("sticky preview");
    assert_eq!(
        sticky.hover.prior.map(|p| (p.group, p.edge)),
        Some((nid("g-v"), crate::contract::DragSide::Left))
    );
    layout
        .run_local_drag_drop(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            sticky_x,
            gy,
            Some(&edge.hover),
        )
        .expect("sticky drop");
    let geom = layout
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("post-drop geometry");
    let w1_rect = geom.iter().find(|g| g.window == win("w1")).unwrap().rect;
    assert_eq!(w1_rect, sticky.proposed_rect);
}

#[test]
fn drag_group_interior_predecessor_agreement_and_invariants() {
    let (mut layout, desc, key, obs) = gap_nested_layout();
    let (inner_rect, child_starts, leaf_rects) = inner_v_group_geom(&layout, &key);
    let cx = inner_rect.x + inner_rect.w / 2;
    let gy = (inner_rect.y..inner_rect.y + inner_rect.h)
        .find(|&y| {
            !leaf_rects.iter().any(|r| rect_contains(r, cx, y))
                && y - inner_rect.y >= 32
                && inner_rect.y + inner_rect.h - y > 32
        })
        .expect("inner interior gap row");

    let preview = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, cx, gy, None)
        .expect("group interior preview");
    let pred = crate::cosmic_v1::insertion_index_for_offset(&child_starts, i64::from(gy));
    assert_eq!(pred, 0);

    layout
        .run_local_drag_drop(&desc, 0, &win("w1"), &win("w1"), &obs, cx, gy, None)
        .expect("group interior drop");
    let dom = layout.domain(&key).unwrap();
    assert_eq!(dom.focused_leaf, Some(nid("l1")));
    match dom.tree.as_ref().unwrap() {
        Node::Group {
            id,
            axis: Axis::Vertical,
            children,
            ..
        } => {
            assert_eq!(id, &nid("g-v"));
            assert_eq!(children, &vec![leaf("l2"), leaf("l1"), leaf("l3")]);
        }
        other => panic!("expected collapsed V root, got {other:?}"),
    }
    let geom = layout
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("post-drop geometry");
    assert_eq!(geom.len(), 3);
    assert_eq!(
        geom.iter().find(|g| g.window == win("w1")).unwrap().rect,
        preview.proposed_rect
    );
}

#[test]
fn drag_group_edge_perpendicular_wraps_and_sticky_prior() {
    let (mut layout, desc, key, obs) = gap_nested_layout();
    let (inner_rect, _, leaf_rects) = inner_v_group_geom(&layout, &key);
    let cx = inner_rect.x + inner_rect.w / 2;
    let gy = (inner_rect.y..inner_rect.y + inner_rect.h)
        .find(|&y| !leaf_rects.iter().any(|r| rect_contains(r, cx, y)))
        .expect("inner gap row");
    let gx = inner_rect.x + 5;
    assert!(!leaf_rects.iter().any(|r| rect_contains(r, gx, gy)));

    let preview = layout
        .run_local_drag_preview(&desc, 0, &win("w1"), &win("w1"), &obs, gx, gy, None)
        .expect("group edge preview");
    assert_eq!(
        preview
            .hover
            .prior
            .as_ref()
            .map(|p| (p.group.clone(), p.edge)),
        Some((nid("g-v"), crate::contract::DragSide::Left))
    );

    let sticky = layout
        .run_local_drag_preview(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            gx,
            gy + 2,
            Some(&preview.hover),
        )
        .expect("sticky edge preview");
    assert_eq!(sticky.proposed_rect, preview.proposed_rect);

    layout
        .run_local_drag_drop(
            &desc,
            0,
            &win("w1"),
            &win("w1"),
            &obs,
            gx,
            gy + 2,
            Some(&preview.hover),
        )
        .expect("group edge drop");
    let geom = layout
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("post-drop geometry");
    assert_eq!(
        geom.iter().find(|g| g.window == win("w1")).unwrap().rect,
        sticky.proposed_rect
    );
}

#[test]
fn drag_group_edge_same_axis_nary_first_last_and_interior_placement() {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let key = ws_key("out-1", "ws-1");
    let desc = domain_desc(&key, bounds, 8);

    let make_layout = || {
        let mut layout = WorldLayout::new();
        let mut d = DomainLayout::new(bounds, 8, 0);
        d.tree = Some(Node::Group {
            id: nid("g-root"),
            axis: Axis::Vertical,
            children: vec![
                Node::Group {
                    id: nid("g-h"),
                    axis: Axis::Horizontal,
                    children: vec![leaf("la"), leaf("lb"), leaf("lc")],
                    shares: vec![1, 1, 1],
                },
                leaf("lm"),
            ],
            shares: vec![1, 1],
        });
        d.leaves.insert(win("wa"), nid("la"));
        d.leaves.insert(win("wb"), nid("lb"));
        d.leaves.insert(win("wc"), nid("lc"));
        d.leaves.insert(win("wm"), nid("lm"));
        d.focused_leaf = Some(nid("lm"));
        d.focus_stack = vec![nid("la"), nid("lb"), nid("lc"), nid("lm")];
        d.accepted_revision = 1;
        layout.domains.insert(key.clone(), d);
        layout
    };

    let obs = vec![
        eng("wa", &key, 0, 260),
        eng("wb", &key, 270, 260),
        eng("wc", &key, 540, 260),
        eng("wm", &key, 0, 800),
    ];

    let base = make_layout();
    let geom = base
        .project_domain_geometry(&key, &BTreeMap::new())
        .expect("project");
    let ra = geom.iter().find(|g| g.window == win("wa")).unwrap().rect;
    let rc = geom.iter().find(|g| g.window == win("wc")).unwrap().rect;
    let h_rect = Rect {
        x: ra.x,
        y: ra.y,
        w: (rc.x + rc.w) - ra.x,
        h: ra.h,
    };
    let leaf_rects: Vec<Rect> = geom.into_iter().map(|g| g.rect).collect();
    let cy = h_rect.y + h_rect.h / 2;
    let gx_interior = (h_rect.x..h_rect.x + h_rect.w)
        .find(|&x| {
            !leaf_rects.iter().any(|r| rect_contains(r, x, cy))
                && x - h_rect.x >= 32
                && h_rect.x + h_rect.w - x > 32
        })
        .expect("inner H interior gap col");
    let gx_left_gap = (ra.x + ra.w..h_rect.x + h_rect.w)
        .find(|&x| !leaf_rects.iter().any(|r| rect_contains(r, x, cy)))
        .expect("gap col");

    // 1) Top strip on H group gap wraps whole group in V[m, H[a,b,c]] -> collapses V[g-h, m] then wraps g-h.
    let mut top_layout = make_layout();
    top_layout
        .run_local_drag_drop(
            &desc,
            0,
            &win("wm"),
            &win("wm"),
            &obs,
            gx_left_gap,
            h_rect.y + 5,
            None,
        )
        .expect("top wrap drop");
    match top_layout.domain(&key).unwrap().tree.as_ref().unwrap() {
        Node::Group {
            axis: Axis::Vertical,
            children,
            ..
        } => {
            assert_eq!(children[0], leaf("lm"));
            assert!(matches!(&children[1], Node::Group { id, .. } if id == &nid("g-h")));
        }
        other => panic!("expected V wrapper, got {other:?}"),
    }

    // 2) Interior gap col inserts into g-h at min(3, pred+1) = 1 -> H[a, m, b, c].
    let mut int_layout = make_layout();
    int_layout
        .run_local_drag_drop(
            &desc,
            0,
            &win("wm"),
            &win("wm"),
            &obs,
            gx_interior,
            cy,
            None,
        )
        .expect("interior drop");
    match int_layout.domain(&key).unwrap().tree.as_ref().unwrap() {
        Node::Group {
            id,
            axis: Axis::Horizontal,
            children,
            ..
        } => {
            assert_eq!(id, &nid("g-h"));
            assert_eq!(
                children,
                &vec![leaf("la"), leaf("lm"), leaf("lb"), leaf("lc")]
            );
        }
        other => panic!("expected collapsed g-h root, got {other:?}"),
    }
}
