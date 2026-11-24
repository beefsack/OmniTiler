use std::collections::BTreeMap;

use crate::contract::MAX_REVISION;
use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags, OutputDomain, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::outcome::{LayoutFailure, PairOperationFailure, PairSendKind};
use super::pair::PairAssemblyFailure;
use super::pair_operation::PairOperationContext;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};

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

fn leaf(id: &str) -> Node {
    Node::Leaf {
        id: NodeId(id.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn eng(window: &str, key: &DomainKey) -> EngineWindow {
    EngineWindow {
        window: win(window),
        output: key.output.clone(),
        workspace: key.workspace.clone(),
        rect: Rect {
            x: 10,
            y: 20,
            w: 400,
            h: 400,
        },
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

fn std_bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 1000,
        h: 800,
    }
}

fn two_leaf_domain(
    bounds: Rect,
    outer_gap: i32,
    w1: &str,
    w2: &str,
    l1: &str,
    l2: &str,
) -> DomainLayout {
    let mut domain = DomainLayout::new(bounds, 0, outer_gap);
    domain.tree = Some(Node::Group {
        id: nid("g0"),
        axis: Axis::Horizontal,
        children: vec![leaf(l1), leaf(l2)],
        shares: vec![1, 1],
    });
    domain.leaves.insert(win(w1), nid(l1));
    domain.leaves.insert(win(w2), nid(l2));
    domain.focused_leaf = Some(nid(l1));
    domain.focus_stack = vec![nid(l2), nid(l1)];
    domain.last_active = Some(nid(l1));
    domain.accepted_revision = 1;
    domain
}

#[test]
fn tiled_workspace_send_follow_and_stay_focus_and_outer_gap() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    let mut src_dom = two_leaf_domain(std_bounds(), 12, "w1", "w2", "l1", "l2");
    src_dom.admission_order.insert(win("w1"), 1);
    src_dom.admission_order.insert(win("w2"), 2);
    layout.domains.insert(source.clone(), src_dom);

    let observed = vec![eng("w1", &source), eng("w2", &source)];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 12,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };

    let outcome = layout
        .run_pair_send(&ctx, &focus, false, false)
        .expect("stay workspace send");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(outcome.stay_focus_window, Some(win("w2")));
    assert!(!outcome.follow);
    assert!(layout.is_tiled_window(&source, &win("w2")));
    assert!(layout.is_tiled_window(&target, &win("w1")));
    assert_eq!(layout.domain_outer_gap(&target), Some(12));
    assert_eq!(layout.admission_sequence(&target, &win("w1")), Some(1));

    // Follow send of w2 to ws-2 empties source and focuses w2 on target.
    let observed2 = vec![eng("w2", &source), eng("w1", &target)];
    let focus2 = win("w2");
    let ctx2 = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 12,
        target_outer_gap: None,
        observed_focus: &focus2,
        observed: &observed2,
        closed: &[],
    };
    let outcome2 = layout
        .run_pair_send(&ctx2, &focus2, true, false)
        .expect("follow workspace send");
    assert_eq!(outcome2.kind, PairSendKind::Tiled);
    assert_eq!(outcome2.stay_focus_window, None);
    assert!(!layout.contains_domain(&source));
    assert!(layout.is_tiled_window(&target, &win("w1")));
    assert!(layout.is_tiled_window(&target, &win("w2")));
}

#[test]
fn tiled_output_send_cross_output_and_scope_refusal() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-2", "ws-1");
    let same_out_target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);
    let same_out_desc = domain_desc(&same_out_target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source.clone(),
        two_leaf_domain(std_bounds(), 10, "w1", "w2", "l1", "l2"),
    );
    let mut tgt_dom = DomainLayout::new(std_bounds(), 0, 24);
    tgt_dom.tree = Some(leaf("t1"));
    tgt_dom.leaves.insert(win("w3"), nid("t1"));
    tgt_dom.focused_leaf = Some(nid("t1"));
    tgt_dom.focus_stack = vec![nid("t1")];
    tgt_dom.accepted_revision = 2;
    layout.domains.insert(target.clone(), tgt_dom);

    // Output send onto same output refuses as CrossDomainMismatch.
    let observed_same = vec![eng("w1", &source), eng("w2", &source)];
    let focus = win("w1");
    let bad_ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &same_out_desc,
        source_outer_gap: 10,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed_same,
        closed: &[],
    };
    let err = layout
        .run_pair_send(&bad_ctx, &focus, true, true)
        .expect_err("same-output output send must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );

    // Cross-output send succeeds and preserves target's retained outer_gap (24).
    let observed = vec![eng("w1", &source), eng("w2", &source), eng("w3", &target)];
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 10,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, true)
        .expect("cross-output send");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(outcome.base_revision, 2);
    assert_eq!(outcome.revision, 3);
    assert_eq!(layout.domain_outer_gap(&target), Some(24));
    assert!(layout.is_tiled_window(&target, &win("w1")));
    assert!(layout.is_tiled_window(&target, &win("w3")));
}

#[test]
fn deferred_exception_send_preserves_flags_geometry_and_follow_stay() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-2", "ws-1");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    let mut src_dom = DomainLayout::new(std_bounds(), 0, 0);
    src_dom.tree = Some(leaf("l1"));
    src_dom.leaves.insert(win("w1"), nid("l1"));
    src_dom.focused_leaf = Some(nid("l1"));
    src_dom.focus_stack = vec![nid("l1")];
    let float_rect = Rect {
        x: 50,
        y: 60,
        w: 320,
        h: 240,
    };
    src_dom.exceptions.insert(
        win("f1"),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: Some(float_rect),
        },
    );
    src_dom
        .retained_float_geometry
        .insert(win("f1"), float_rect);
    src_dom.admission_order.insert(win("f1"), 7);
    src_dom.accepted_revision = 1;
    layout.domains.insert(source.clone(), src_dom);

    let mut tgt_dom = DomainLayout::new(std_bounds(), 0, 0);
    tgt_dom.tree = Some(leaf("t1"));
    tgt_dom.leaves.insert(win("w2"), nid("t1"));
    tgt_dom.focused_leaf = Some(nid("t1"));
    tgt_dom.focus_stack = vec![nid("t1")];
    tgt_dom.accepted_revision = 1;
    layout.domains.insert(target.clone(), tgt_dom);

    let mut f1_obs = eng("f1", &source);
    f1_obs.floating = true;
    f1_obs.maximized = true; // approved combo: floating + one overlay
    let observed = vec![eng("w1", &source), f1_obs, eng("w2", &target)];
    let focus = win("f1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };

    let outcome = layout
        .run_pair_send(&ctx, &focus, false, true)
        .expect("deferred stay send");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert_eq!(outcome.stay_focus_window, Some(win("w1")));
    assert!(layout.is_exception(&target, &win("f1")));
    assert!(!layout.is_exception(&source, &win("f1")));
    assert_eq!(
        layout.retained_float_geometry(&target, &win("f1")),
        Some(float_rect)
    );
    assert_eq!(layout.admission_sequence(&target, &win("f1")), Some(7));
}

#[test]
fn d2_capacity_fallback_and_b6_advisory_minima() {
    let source = ws_key("out-1", "ws-1");
    let target_same = ws_key("out-1", "ws-2");
    let narrow_bounds = Rect {
        x: 0,
        y: 0,
        w: 2,
        h: 1,
    };
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target_same, narrow_bounds, 0);

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source.clone(),
        two_leaf_domain(std_bounds(), 0, "w1", "w2", "l1", "l2"),
    );

    // Narrow positive target width=2, height=1 with two horizontal leaves (each 1x1):
    // inserting a third leaf exhausts geometric capacity and triggers D3 capacity-float.
    let mut tgt_dom = DomainLayout::new(narrow_bounds, 0, 0);
    tgt_dom.tree = Some(Node::Group {
        id: nid("tg0"),
        axis: Axis::Horizontal,
        children: vec![leaf("t1"), leaf("t2")],
        shares: vec![1, 1],
    });
    tgt_dom.leaves.insert(win("w3"), nid("t1"));
    tgt_dom.leaves.insert(win("w4"), nid("t2"));
    tgt_dom.focused_leaf = Some(nid("t1"));
    tgt_dom.focus_stack = vec![nid("t1")];
    tgt_dom.accepted_revision = 1;
    layout.domains.insert(target_same.clone(), tgt_dom);

    let mut w1_obs = eng("w1", &source);
    w1_obs.rect = Rect {
        x: 15,
        y: 25,
        w: 333,
        h: 444,
    };
    // B6: min_w is advisory; minimum overconstraint is NOT a projection failure.
    w1_obs.hints = WindowSizeHints {
        min_w: Some(900),
        min_h: None,
        max_w: None,
        max_h: None,
    };
    let observed = vec![
        w1_obs,
        eng("w2", &source),
        eng("w3", &target_same),
        eng("w4", &target_same),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };

    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("capacity float fallback");
    assert_eq!(outcome.kind, PairSendKind::CapacityFloat);
    assert!(layout.is_exception(&target_same, &win("w1")));
    assert!(layout.is_capacity_float(&target_same, &win("w1")));
    assert_eq!(
        layout.retained_float_geometry(&target_same, &win("w1")),
        Some(Rect {
            x: 15,
            y: 25,
            w: 333,
            h: 444,
        })
    );
}

#[test]
fn source_seed_hidden_closed_and_nonconsuming_install_on_refusal() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-2", "ws-1");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    // Target has a minimized slot (even if listed in ctx.closed) and a missing
    // known non-slotted exception with automatic_fixed/fixed_tile_override/admission_order.
    let mut tgt_dom = DomainLayout::new(std_bounds(), 0, 0);
    tgt_dom.tree = Some(leaf("t1"));
    tgt_dom.leaves.insert(win("w3"), nid("t1"));
    tgt_dom.minimized_slots.insert(
        win("w_closed"),
        DomainRestoreSlot {
            leaf: nid("tc"),
            tree: leaf("tc"),
        },
    );
    tgt_dom.exceptions.insert(
        win("w_missing_exc"),
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
    tgt_dom.automatic_fixed.insert(win("w_missing_exc"));
    tgt_dom.fixed_tile_override.insert(win("w_missing_exc"));
    tgt_dom.admission_order.insert(win("w_missing_exc"), 42);
    tgt_dom.accepted_revision = 1;
    layout.domains.insert(target.clone(), tgt_dom);

    // Source is initially absent; seeded from source-only observed windows.
    // Request a same-output workspace send onto a cross-output target ->
    // converges both domains (with hidden=empty and NO close-slot cleanup),
    // installs convergence, and THEN refuses with CrossDomainMismatch.
    let observed = vec![eng("w1", &source), eng("w2", &source), eng("w3", &target)];
    let closed = vec![win("w_closed")];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &closed,
    };

    let err = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect_err("workspace send across outputs must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );
    // Source was seeded and installed; closed slot was retained (no close-slot
    // cleanup on explicit send); missing non-slotted exception markers pruned
    // with hidden=empty.
    assert!(layout.is_tiled_window(&source, &win("w1")));
    assert!(layout.is_tiled_window(&source, &win("w2")));
    assert!(layout.has_minimized_slot(&target, &win("w_closed")));
    assert!(!layout.is_automatic_fixed_float(&target, &win("w_missing_exc")));
    assert!(!layout.has_fixed_tile_override(&target, &win("w_missing_exc")));
    assert_eq!(
        layout.admission_sequence(&target, &win("w_missing_exc")),
        None
    );
}

#[test]
fn revision_limit_atomicity_and_allocator_burns_survive() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    let mut src_dom = two_leaf_domain(std_bounds(), 0, "w1", "w2", "l1", "l2");
    src_dom.accepted_revision = MAX_REVISION;
    layout.domains.insert(source.clone(), src_dom);

    let observed = vec![eng("w1", &source), eng("w2", &source)];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };

    let err = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect_err("MAX_REVISION must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );
    // Source remains intact and target was not created.
    assert!(layout.is_tiled_window(&source, &win("w1")));
    assert!(!layout.contains_domain(&target));
}

#[test]
fn target_empty_ignores_retained_residue_and_max_revision_and_source_error_priority() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    // Truly absent source -> SourceUnavailable ("canonical-source-unavailable").
    assert_eq!(
        layout
            .assemble_send_pair_with_target(&source_desc, &target_desc, false)
            .expect_err("absent source"),
        PairAssemblyFailure::SourceUnavailable
    );

    // Existing unusable/empty/mismatched source -> TargetUnusable ("canonical-pair-unusable").
    let empty_src = DomainLayout::new(std_bounds(), 0, 10);
    layout.domains.insert(source.clone(), empty_src);
    assert_eq!(
        layout
            .assemble_send_pair_with_target(&source_desc, &target_desc, false)
            .expect_err("empty existing source"),
        PairAssemblyFailure::TargetUnusable
    );

    // Valid source + retained target with MAX_REVISION, nonempty residue, and outer_gap=28.
    layout.domains.insert(
        source.clone(),
        two_leaf_domain(std_bounds(), 10, "w1", "w2", "l1", "l2"),
    );
    let mut tgt_dom = DomainLayout::new(std_bounds(), 0, 28);
    tgt_dom.tree = Some(leaf("t1"));
    tgt_dom.leaves.insert(win("stale"), nid("t1"));
    tgt_dom.accepted_revision = MAX_REVISION;
    layout.domains.insert(target.clone(), tgt_dom);

    // Empty target observation ignores retained target residue and MAX_REVISION.
    // It removes the empty target domain and its outer gap
    // during pre-send convergence, so the final send inherits source_outer_gap (10).
    let observed = vec![eng("w1", &source), eng("w2", &source)];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 10,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("empty target observation ignores retained target residue and MAX_REVISION");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(layout.domain_outer_gap(&target), Some(10));
    assert!(layout.is_tiled_window(&target, &win("w1")));
    assert!(!layout.is_tiled_window(&target, &win("stale")));
}

#[test]
fn legacy_seed_default_config_and_seed_failed_mapping() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    // Enable non-default config on WorldLayout; the send's first-time source seed
    // must still use legacy Session::new defaults (fixed_size_admission=false, BothAxes).
    layout.set_fixed_size_admission(true);
    layout.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
    layout.set_fresh_placement(FreshPlacement::CursorOutputCursorClosest);
    layout.set_fresh_cursor(Some((10, 10)));

    let mut w1_obs = eng("w1", &source);
    w1_obs.hints = WindowSizeHints {
        min_w: Some(400),
        max_w: Some(400),
        min_h: None,
        max_h: None,
    };
    let observed = vec![w1_obs, eng("w2", &source)];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("legacy seed defaults must tile w1 instead of fixed-size floating it");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert!(layout.is_tiled_window(&target, &win("w1")));

    // Invalid source domain descriptor on absent source maps to SeedFailed.
    let mut bad_layout = WorldLayout::new();
    let bad_source_desc = domain_desc(
        &source,
        Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 800,
        },
        0,
    );
    let bad_ctx = PairOperationContext {
        source_domain: &bad_source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let err = bad_layout
        .run_pair_send(&bad_ctx, &focus, true, false)
        .expect_err("invalid absent source seed must fail with SeedFailed");
    assert_eq!(err, PairOperationFailure::SeedFailed);
}

#[test]
fn same_output_capacity_send_updates_prior_source_durable_geometry() {
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let narrow_bounds = Rect {
        x: 0,
        y: 0,
        w: 2,
        h: 1,
    };
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, narrow_bounds, 0);

    let mut layout = WorldLayout::new();
    let mut src_dom = two_leaf_domain(std_bounds(), 0, "w1", "w2", "l1", "l2");
    src_dom.retained_float_geometry.insert(
        win("w1"),
        Rect {
            x: 1,
            y: 2,
            w: 100,
            h: 100,
        },
    );
    src_dom.admission_order.insert(win("w1"), 9);
    layout.domains.insert(source.clone(), src_dom);

    let mut tgt_dom = DomainLayout::new(narrow_bounds, 0, 0);
    tgt_dom.tree = Some(Node::Group {
        id: nid("tg0"),
        axis: Axis::Horizontal,
        children: vec![leaf("t1"), leaf("t2")],
        shares: vec![1, 1],
    });
    tgt_dom.leaves.insert(win("w3"), nid("t1"));
    tgt_dom.leaves.insert(win("w4"), nid("t2"));
    tgt_dom.focused_leaf = Some(nid("t1"));
    tgt_dom.focus_stack = vec![nid("t1")];
    tgt_dom.accepted_revision = 1;
    layout.domains.insert(target.clone(), tgt_dom);

    let mut w1_obs = eng("w1", &source);
    let live_rect = Rect {
        x: 15,
        y: 25,
        w: 333,
        h: 444,
    };
    w1_obs.rect = live_rect;
    let observed = vec![
        w1_obs,
        eng("w2", &source),
        eng("w3", &target),
        eng("w4", &target),
    ];
    let focus = win("w1");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };

    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("same-output capacity send with prior source durable geometry");
    assert_eq!(outcome.kind, PairSendKind::CapacityFloat);
    assert_eq!(
        layout.retained_float_geometry(&target, &win("w1")),
        Some(live_rect)
    );
    assert_eq!(layout.retained_float_geometry(&source, &win("w1")), None);
    assert_eq!(layout.admission_sequence(&target, &win("w1")), Some(9));
    assert!(layout.is_capacity_float(&target, &win("w1")));
}

fn collect_pair_group_ids(node: &Node, out: &mut std::collections::BTreeSet<NodeId>) {
    if let Node::Group { id, children, .. } = node {
        out.insert(id.clone());
        for child in children {
            collect_pair_group_ids(child, out);
        }
    }
}

fn all_layout_group_ids(layout: &WorldLayout) -> std::collections::BTreeSet<NodeId> {
    let mut ids = std::collections::BTreeSet::new();
    for domain in layout.domains.values() {
        if let Some(tree) = domain.tree.as_ref() {
            collect_pair_group_ids(tree, &mut ids);
        }
    }
    ids
}

fn all_layout_node_ids(layout: &WorldLayout) -> std::collections::BTreeSet<NodeId> {
    fn walk(node: &Node, out: &mut std::collections::BTreeSet<NodeId>) {
        out.insert(node.id().clone());
        if let Node::Group { children, .. } = node {
            for child in children {
                walk(child, out);
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for domain in layout.domains.values() {
        if let Some(tree) = domain.tree.as_ref() {
            walk(tree, &mut ids);
        }
    }
    ids
}

#[test]
fn alloc_send_to_occupied_shares_counter() {
    use crate::session::ObservedWindow;
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);
    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(source.clone(), DomainLayout::new(std_bounds(), 0, 0));
    layout
        .domains
        .insert(target.clone(), DomainLayout::new(std_bounds(), 0, 0));

    let obs_for = |name: &str, k: &DomainKey| ObservedWindow {
        window: win(name),
        output: k.output.clone(),
        workspace: k.workspace.clone(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    let wide = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 50,
    };

    layout
        .admit_window(
            &source,
            &win("a"),
            wide,
            false,
            false,
            &[obs_for("a", &source)],
        )
        .expect("admit a");
    layout
        .admit_window(
            &source,
            &win("b"),
            wide,
            false,
            false,
            &[obs_for("a", &source), obs_for("b", &source)],
        )
        .expect("admit b");
    layout
        .admit_window(
            &target,
            &win("d"),
            wide,
            false,
            false,
            &[obs_for("d", &target)],
        )
        .expect("admit d");
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0")])
    );

    let observed = vec![eng("a", &source), eng("b", &source), eng("d", &target)];
    let focus = win("b");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("send to occupied");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-1")])
    );
    assert_eq!(layout.group_alloc.next_value(), 2);
    assert!(layout.is_tiled_window(&target, &win("b")));
}

#[test]
fn alloc_send_fallback_burns_and_never_reissues_on_repeat() {
    use crate::session::ObservedWindow;
    let source = ws_key("o", "ws-1");
    let target = ws_key("o", "ws-2");
    let tight_bounds = Rect {
        x: 0,
        y: 0,
        w: 12,
        h: 12,
    };
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, tight_bounds, 4);

    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(source.clone(), DomainLayout::new(std_bounds(), 0, 0));
    layout
        .domains
        .insert(target.clone(), DomainLayout::new(tight_bounds, 4, 0));

    let obs_for = |name: &str, k: &DomainKey| ObservedWindow {
        window: win(name),
        output: k.output.clone(),
        workspace: k.workspace.clone(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    let wide = Rect {
        x: 0,
        y: 0,
        w: 12,
        h: 4,
    };
    let tall = Rect {
        x: 0,
        y: 0,
        w: 4,
        h: 12,
    };

    layout
        .admit_window(
            &target,
            &win("a"),
            wide,
            false,
            false,
            &[obs_for("a", &target)],
        )
        .expect("admit a");
    layout
        .admit_window(
            &target,
            &win("b"),
            wide,
            false,
            false,
            &[obs_for("a", &target), obs_for("b", &target)],
        )
        .expect("admit b");
    layout
        .admit_window(
            &target,
            &win("c"),
            tall,
            false,
            false,
            &[
                obs_for("a", &target),
                obs_for("b", &target),
                obs_for("c", &target),
            ],
        )
        .expect("admit c");
    layout
        .admit_window(
            &source,
            &win("d"),
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            false,
            false,
            &[obs_for("d", &source)],
        )
        .expect("admit d");
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0"), nid("grp-1")])
    );
    assert_eq!(layout.group_alloc.next_value(), 2);

    let observed = vec![
        eng("a", &target),
        eng("b", &target),
        eng("c", &target),
        eng("d", &source),
    ];
    let focus = win("d");
    let ctx = PairOperationContext {
        source_domain: &source_desc,
        target_domain: &target_desc,
        source_outer_gap: 0,
        target_outer_gap: None,
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    let outcome = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("send fallback succeeds");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    // Failed ordinary wrap of c burned grp-2; largest fallback split of a minted grp-3.
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0"), nid("grp-1"), nid("grp-3")])
    );
    assert_eq!(layout.group_alloc.next_value(), 4);

    fn send_sibling(node: &Node, leaf_id: &NodeId) -> Option<NodeId> {
        match node {
            Node::Leaf { .. } => None,
            Node::Group { children, .. } => {
                if children.len() == 2 && children.iter().any(|c| c.id() == leaf_id) {
                    return children
                        .iter()
                        .find(|c| c.id() != leaf_id)
                        .map(|c| c.id().clone());
                }
                children.iter().find_map(|c| send_sibling(c, leaf_id))
            }
        }
    }
    let d_leaf = layout.domain(&target).unwrap().leaves[&win("d")].clone();
    let a_leaf = layout.domain(&target).unwrap().leaves[&win("a")].clone();
    assert_eq!(
        send_sibling(layout.tree(&target).unwrap(), &d_leaf),
        Some(a_leaf.clone())
    );

    // Remove d from target, return it to source, restore anchor c, and send again:
    // repeat burns grp-4 (ordinary) and grp-5 (fallback) and never reissues grp-3.
    layout
        .converge_domain(
            &target,
            &[
                obs_for("a", &target),
                obs_for("b", &target),
                obs_for("c", &target),
            ],
            Some(&win("c")),
            &std::collections::BTreeSet::new(),
        )
        .expect("remove d from target");
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0"), nid("grp-1")])
    );
    layout
        .domains
        .insert(source.clone(), DomainLayout::new(std_bounds(), 0, 0));
    layout
        .admit_window(
            &source,
            &win("d"),
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            false,
            false,
            &[obs_for("d", &source)],
        )
        .expect("readmit d to source");

    let outcome2 = layout
        .run_pair_send(&ctx, &focus, true, false)
        .expect("repeat send fallback succeeds");
    assert_eq!(outcome2.kind, PairSendKind::Tiled);
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0"), nid("grp-1"), nid("grp-5")])
    );
    assert_eq!(layout.group_alloc.next_value(), 6);
    let d_leaf2 = layout.domain(&target).unwrap().leaves[&win("d")].clone();
    assert_eq!(
        send_sibling(layout.tree(&target).unwrap(), &d_leaf2),
        Some(a_leaf)
    );
}

#[test]
fn alloc_pair_split_uses_shared_counter_for_new_groups() {
    use crate::session::ObservedWindow;
    let source = ws_key("out-1", "ws-1");
    let target = ws_key("out-1", "ws-2");
    let source_desc = domain_desc(&source, std_bounds(), 0);
    let target_desc = domain_desc(&target, std_bounds(), 0);

    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(source.clone(), DomainLayout::new(std_bounds(), 0, 0));
    layout
        .domains
        .insert(target.clone(), DomainLayout::new(std_bounds(), 0, 0));

    let obs_for = |name: &str, k: &DomainKey| ObservedWindow {
        window: win(name),
        output: k.output.clone(),
        workspace: k.workspace.clone(),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    let placed = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 50,
    };

    layout
        .admit_window(
            &source,
            &win("win-1"),
            placed,
            false,
            false,
            &[obs_for("win-1", &source)],
        )
        .expect("admit win-1");
    layout
        .admit_window(
            &source,
            &win("win-2"),
            placed,
            false,
            false,
            &[obs_for("win-1", &source), obs_for("win-2", &source)],
        )
        .expect("admit win-2");
    layout
        .admit_window(
            &target,
            &win("win-t"),
            placed,
            false,
            false,
            &[obs_for("win-t", &target)],
        )
        .expect("admit win-t");

    let pair = layout
        .assemble_send_pair(&source_desc, &target_desc)
        .expect("assemble send pair");
    layout.install_pair(&pair, 0, None).expect("install split");

    for id in all_layout_node_ids(&layout) {
        assert!(
            !id.0.starts_with("pair-target-"),
            "temporary pair-target id escaped: {id:?}"
        );
        assert!(
            !id.0.starts_with("pair-moved-"),
            "temporary pair-moved id escaped: {id:?}"
        );
    }

    // Minting after split continues the shared counter past grp-0.
    layout
        .admit_window(
            &source,
            &win("win-3"),
            placed,
            false,
            false,
            &[
                obs_for("win-1", &source),
                obs_for("win-2", &source),
                obs_for("win-3", &source),
            ],
        )
        .expect("admit win-3 after split");
    assert_eq!(
        all_layout_group_ids(&layout),
        std::collections::BTreeSet::from([nid("grp-0"), nid("grp-1")])
    );
    assert_eq!(layout.group_alloc.next_value(), 2);
}
