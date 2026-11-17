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
