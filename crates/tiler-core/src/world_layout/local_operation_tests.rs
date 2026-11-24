use std::collections::BTreeMap;

use crate::contract::{MAX_REVISION, ResizeMode};
use crate::directional::{
    Axis, Direction, Node, NodeId, OutputId, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain, RefusalKind};
use crate::size_hints::{FixedSizePredicate, WindowSizeHints};

use super::local_operation::LocalOperationContext;
use super::outcome::{LayoutFailure, LocalOperationFailure};
use super::state::{DomainLayout, WorldLayout};

fn domain(output: &str, workspace: &str, w: i32, h: i32, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
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

fn window(id: &str, output: &str, workspace: &str, x: i32, w: i32) -> EngineWindow {
    EngineWindow {
        window: win(id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect { x, y: 0, w, h: 100 },
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

fn two_tile_domain(bounds: Rect, gap: i32, outer_gap: i32) -> DomainLayout {
    let mut d = DomainLayout::new(bounds, gap, outer_gap);
    d.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b")],
        shares: vec![500, 500],
    });
    d.leaves.insert(win("w1"), nid("a"));
    d.leaves.insert(win("w2"), nid("b"));
    d.focused_leaf = Some(nid("a"));
    d.focus_stack = vec![nid("b"), nid("a")];
    d.focus_history = vec![nid("b"), nid("a")];
    d.last_active = Some(nid("a"));
    d.admission_order.insert(win("w1"), 1);
    d.admission_order.insert(win("w2"), 2);
    d.accepted_revision = 1;
    d
}

#[test]
fn retained_geometry_mismatch_converged_no_reseed_vs_unconverged_reseed() {
    let dom = domain("out-1", "ws-1", 400, 100, 0);
    let k = dom.key();
    let mismatch_bounds = Rect {
        x: 0,
        y: 0,
        w: 300,
        h: 100,
    };
    let obs = vec![
        window("w1", "out-1", "ws-1", 0, 200),
        window("w2", "out-1", "ws-1", 200, 200),
    ];

    // Preconverged: mismatched retained domain stays retained and refuses PartialObservation.
    let mut converged_layout = WorldLayout::new();
    converged_layout
        .domains
        .insert(k.clone(), two_tile_domain(mismatch_bounds, 0, 0));
    let ctx_conv = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs,
        preconverged: true,
    };
    let err = converged_layout
        .run_toggle_orientation(&ctx_conv, &win("w1"))
        .expect_err("converged mismatch must refuse without reseed");
    assert_eq!(
        err,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    let kept = converged_layout
        .domain(&k)
        .expect("domain remains retained");
    assert_eq!(kept.bounds, mismatch_bounds);
    assert_eq!(kept.accepted_revision, 1);

    // Preconverged with ambiguous seed order: returns AmbiguousPlacement without reseed.
    let ambig_obs = vec![
        window("w1", "out-1", "ws-1", 0, 200),
        window("w2", "out-1", "ws-1", 0, 200),
        window("w3", "out-1", "ws-1", 0, 200),
    ];
    let ctx_ambig = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &ambig_obs,
        preconverged: true,
    };
    let err_ambig = converged_layout
        .run_toggle_orientation(&ctx_ambig, &win("w1"))
        .expect_err("ambiguous seed order refuses");
    assert_eq!(err_ambig, LocalOperationFailure::AmbiguousPlacement);
    assert_eq!(
        converged_layout.domain(&k).expect("still retained").bounds,
        mismatch_bounds
    );

    // Unconverged: retires mismatched domain and reseeds at requested bounds.
    let mut unconverged_layout = WorldLayout::new();
    unconverged_layout
        .domains
        .insert(k.clone(), two_tile_domain(mismatch_bounds, 0, 0));
    let ctx_unconv = LocalOperationContext {
        domain: &dom,
        outer_gap: 4,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs,
        preconverged: false,
    };
    let outcome = unconverged_layout
        .run_toggle_orientation(&ctx_unconv, &win("w1"))
        .expect("unconverged mismatch reseeds");
    assert_eq!(outcome.window, win("w1"));
    let reseeded = unconverged_layout.domain(&k).expect("reseeded domain");
    assert_eq!(reseeded.bounds, dom.bounds);
    assert_eq!(reseeded.outer_gap, 4);
}

#[test]
fn retained_refusal_preserves_state_and_outer_gap_is_not_usability_fence() {
    let dom = domain("out-1", "ws-1", 400, 100, 0);
    let k = dom.key();
    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(k.clone(), two_tile_domain(dom.bounds, 0, 2));
    layout.admission_seq_next = 7;

    let obs = vec![
        window("w1", "out-1", "ws-1", 0, 200),
        window("w2", "out-1", "ws-1", 200, 200),
    ];
    // Subject w2 does not match observed_focus w1 -> FocusMismatch refusal.
    let ctx = LocalOperationContext {
        domain: &dom,
        outer_gap: 10,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs,
        preconverged: true,
    };
    let err = layout
        .run_toggle_orientation(&ctx, &win("w2"))
        .expect_err("focus mismatch must refuse");
    assert_eq!(
        err,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
    let retained = layout.domain(&k).expect("retained domain preserved");
    assert_eq!(retained.accepted_revision, 1);
    assert_eq!(retained.outer_gap, 2);
    assert_eq!(layout.admission_seq_next, 7);

    // Different outer_gap (10 vs 2) is not a usability fence: succeeds and updates outer_gap.
    let ok = layout
        .run_toggle_orientation(&ctx, &win("w1"))
        .expect("different outer_gap succeeds");
    assert_eq!(ok.revision, 2);
    let updated = layout.domain(&k).expect("updated domain");
    assert_eq!(updated.outer_gap, 10);
    assert_eq!(updated.accepted_revision, 2);

    // Scaled outer_gap > 64 is retained unconditionally on successful install.
    let ctx_scaled = LocalOperationContext {
        outer_gap: 80,
        resize: crate::policy::ScaledResize::unscaled(),
        ..ctx
    };
    let ok_scaled = layout
        .run_toggle_orientation(&ctx_scaled, &win("w1"))
        .expect("scaled outer_gap > 64 succeeds");
    assert_eq!(ok_scaled.revision, 3);
    let updated_scaled = layout.domain(&k).expect("scaled domain retained");
    assert_eq!(updated_scaled.outer_gap, 80);
    assert_eq!(updated_scaled.accepted_revision, 3);
}

#[test]
fn absent_relocation_success_and_refusal_source_preservation() {
    let target_dom = domain("out-2", "ws-1", 500, 120, 0);
    let target_key = target_dom.key();
    let source_key = DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };

    // 1. Successful relocation removes source and installs target.
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        source_key.clone(),
        two_tile_domain(
            Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            0,
            0,
        ),
    );
    layout.admission_seq_next = 5;
    let obs = vec![
        window("w1", "out-2", "ws-1", 0, 250),
        window("w2", "out-2", "ws-1", 250, 250),
    ];
    let ctx = LocalOperationContext {
        domain: &target_dom,
        outer_gap: 6,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs,
        preconverged: false,
    };
    let outcome = layout
        .run_toggle_orientation(&ctx, &win("w1"))
        .expect("relocation succeeds");
    assert_eq!(outcome.output, OutputId("out-2".to_owned()));
    assert!(layout.domain(&source_key).is_none());
    let installed = layout.domain(&target_key).expect("target installed");
    assert_eq!(installed.bounds, target_dom.bounds);
    assert_eq!(installed.outer_gap, 6);

    // 2. Ordinary refusal on relocated candidate preserves source.
    let mut layout_refuse = WorldLayout::new();
    layout_refuse.domains.insert(
        source_key.clone(),
        two_tile_domain(
            Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            0,
            0,
        ),
    );
    let err = layout_refuse
        .run_toggle_orientation(&ctx, &win("w2"))
        .expect_err("focus mismatch on relocated op refuses");
    assert_eq!(
        err,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
    assert!(layout_refuse.domain(&source_key).is_some());
    assert!(layout_refuse.domain(&target_key).is_none());

    // 3. PartialObservation from relocated op falls through to legacy seed when !preconverged,
    //    preserving source.
    let mut layout_fallback = WorldLayout::new();
    layout_fallback.domains.insert(
        source_key.clone(),
        two_tile_domain(
            Rect {
                x: 0,
                y: 0,
                w: 300,
                h: 100,
            },
            0,
            0,
        ),
    );
    let obs_extra = vec![
        window("w1", "out-2", "ws-1", 0, 150),
        window("w2", "out-2", "ws-1", 150, 150),
        window("w3", "out-2", "ws-1", 300, 200),
    ];
    let ctx_fallback = LocalOperationContext {
        domain: &target_dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs_extra,
        preconverged: false,
    };
    let seeded_ok = layout_fallback
        .run_toggle_orientation(&ctx_fallback, &win("w1"))
        .expect("falls through to legacy seed");
    assert_eq!(seeded_ok.window, win("w1"));
    assert!(layout_fallback.domain(&source_key).is_some());
    assert!(layout_fallback.domain(&target_key).is_some());

    // 4. Same PartialObservation with preconverged=true hits no-reseed fence, preserving source.
    let mut layout_conv = WorldLayout::new();
    let mut max_rev_source = two_tile_domain(
        Rect {
            x: 0,
            y: 0,
            w: 300,
            h: 100,
        },
        0,
        0,
    );
    max_rev_source.accepted_revision = MAX_REVISION;
    layout_conv
        .domains
        .insert(source_key.clone(), max_rev_source);
    let ctx_conv = LocalOperationContext {
        domain: &target_dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs,
        preconverged: true,
    };
    let err_conv = layout_conv
        .run_toggle_orientation(&ctx_conv, &win("w1"))
        .expect_err("converged fence blocks reseed after RevisionLimit on relocation");
    assert_eq!(
        err_conv,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    assert!(layout_conv.domain(&source_key).is_some());
    assert!(layout_conv.domain(&target_key).is_none());
}

#[test]
fn legacy_seed_uses_default_fixed_policy_and_discards_rejected_candidate_with_allocator_burns() {
    let dom = domain("out-1", "ws-1", 400, 100, 0);
    let k = dom.key();

    // 1. Discriminating default-policy test: non-subject tiled overlay flags (fullscreen = true,
    //    floating = false) are tolerated by fixed_admission = true, but rejected as
    //    PartialObservation by fixed_admission = false.
    let w1 = window("w1", "out-1", "ws-1", 0, 200);
    let mut w2_overlay = window("w2", "out-1", "ws-1", 200, 200);
    w2_overlay.fullscreen = true;
    let overlay_obs = vec![w1.clone(), w2_overlay];

    // Seeded route: uses default fixed_admission = false despite layout.config.fixed_size_admission
    // = true, so it refuses PartialObservation with no installation and no counter commit.
    let mut seeded_layout = WorldLayout::new();
    seeded_layout.config.fixed_size_admission = true;
    seeded_layout.config.fixed_size_predicate = FixedSizePredicate::EitherAxis;
    seeded_layout.admission_seq_next = 10;
    let ctx_overlay = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &overlay_obs,
        preconverged: false,
    };
    let err_seed = seeded_layout
        .run_toggle_orientation(&ctx_overlay, &win("w1"))
        .expect_err("legacy seed uses default fixed_admission = false and rejects overlay");
    assert_eq!(
        err_seed,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    assert!(seeded_layout.domain(&k).is_none());
    assert_eq!(seeded_layout.admission_seq_next, 10);

    // Retained route with identical config (fixed_size_admission = true) and identical observed
    // roster: tolerates non-subject tiled overlay flags and succeeds.
    let mut retained_layout = WorldLayout::new();
    retained_layout.config.fixed_size_admission = true;
    retained_layout.config.fixed_size_predicate = FixedSizePredicate::EitherAxis;
    retained_layout
        .domains
        .insert(k.clone(), two_tile_domain(dom.bounds, 0, 0));
    let ok_retained = retained_layout
        .run_toggle_orientation(&ctx_overlay, &win("w1"))
        .expect("retained route uses current fixed_size_admission = true and succeeds");
    assert_eq!(ok_retained.window, win("w1"));

    // 2. Rejected candidate after ordinary seed discards domain & counter, while allocator burns
    //    survive; accepted ordinary seed commits the returned seed counter.
    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 10;
    let ordinary_obs = vec![w1, window("w2", "out-1", "ws-1", 200, 200)];
    let pre_alloc = layout.group_alloc.next_value();
    let ctx_ordinary = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &ordinary_obs,
        preconverged: false,
    };
    let err_refuse = layout
        .run_toggle_orientation(&ctx_ordinary, &win("w2"))
        .expect_err("op refuses after seed");
    assert_eq!(
        err_refuse,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
    assert!(layout.domain(&k).is_none());
    assert_eq!(layout.admission_seq_next, 10);
    assert!(layout.group_alloc.next_value() > pre_alloc);

    let ok_seed = layout
        .run_toggle_orientation(&ctx_ordinary, &win("w1"))
        .expect("ordinary legacy seed succeeds");
    assert_eq!(ok_seed.window, win("w1"));
    assert!(layout.domain(&k).is_some());
    assert_eq!(layout.admission_seq_next, 12);
}

#[test]
fn float_special_branch_convergence_persistence_and_invalid_descriptor() {
    let dom = domain("out-1", "ws-1", 400, 100, 0);
    let k = dom.key();

    // 1. Absent + floating observed + no unique source -> converges fresh, and toggle refusal
    //    persists the newly converged domain.
    let mut layout = WorldLayout::new();
    let mut w1 = window("w1", "out-1", "ws-1", 0, 200);
    w1.floating = true;
    let w2 = window("w2", "out-1", "ws-1", 200, 200);
    let obs = vec![w1, w2];
    let ctx = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w2"),
        observed: &obs,
        preconverged: false,
    };
    let err = layout
        .run_toggle_float(&ctx, &win("missing"), None)
        .expect_err("toggle on unknown window refuses");
    assert_eq!(
        err,
        LocalOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    let retained = layout
        .domain(&k)
        .expect("newly converged domain persists on toggle refusal");
    assert!(retained.exceptions.contains_key(&win("w1")));
    assert!(retained.leaves.contains_key(&win("w2")));

    // 2. Invalid descriptor falls through to legacy route (SeedFailed / AmbiguousPlacement)
    //    instead of surfacing Layout(MalformedInput) from converge_fresh_domain.
    let mut invalid_layout = WorldLayout::new();
    let invalid_dom = domain("out-1", "ws-1", 0, 100, 0);
    let ctx_invalid = LocalOperationContext {
        domain: &invalid_dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w2"),
        observed: &obs,
        preconverged: false,
    };
    let err_invalid = invalid_layout
        .run_toggle_float(&ctx_invalid, &win("w1"), None)
        .expect_err("invalid descriptor hits SeedFailed");
    assert_eq!(err_invalid, LocalOperationFailure::SeedFailed);
}

#[test]
fn all_seven_typed_entry_points_smoke() {
    let dom = domain("out-1", "ws-1", 1600, 900, 0);
    let k = dom.key();
    let obs_2 = vec![
        window("w1", "out-1", "ws-1", 0, 800),
        window("w2", "out-1", "ws-1", 800, 800),
    ];
    let ctx_2 = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs_2,
        preconverged: true,
    };
    let fresh_two = || {
        let mut layout = WorldLayout::new();
        layout
            .domains
            .insert(k.clone(), two_tile_domain(dom.bounds, 0, 0));
        layout
    };

    // 1. Local focus: moves focus from w1 to w2.
    let focus_out = fresh_two()
        .run_local_focus(&ctx_2, &win("w1"), Direction::Right)
        .expect("local_focus succeeds");
    assert_eq!(focus_out.from_window, win("w1"));
    assert_eq!(focus_out.to_window, win("w2"));

    // 2. Local move: moves w1 Down in a horizontal root.
    let move_out = fresh_two()
        .run_local_move(
            &ctx_2,
            &win("w1"),
            Direction::Down,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("local_move succeeds");
    assert_eq!(move_out.window, win("w1"));
    assert_eq!(move_out.direction, Direction::Down);

    // 3. Keyboard resize: grows w1 Right in a horizontal root.
    let kb_out = fresh_two()
        .run_keyboard_resize(
            &ctx_2,
            &win("w1"),
            Direction::Right,
            ResizeMode::Outwards,
            0,
        )
        .expect("keyboard_resize succeeds");
    assert_ne!(kb_out.operation.old_shares, kb_out.operation.new_shares);

    // 4. Pointer edge resize: moves w1's right boundary to 820.
    let ptr_out = fresh_two()
        .run_pointer_resize(&ctx_2, &win("w1"), Direction::Right, 820)
        .expect("pointer_resize succeeds");
    assert_ne!(ptr_out.operation.old_shares, ptr_out.operation.new_shares);

    // 5. Pointer corner resize: needs both Horizontal and Vertical ancestor groups.
    let mut corner_dom = DomainLayout::new(dom.bounds, 0, 0);
    corner_dom.tree = Some(Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Group {
                id: nid("g-left"),
                axis: Axis::Vertical,
                children: vec![leaf("a"), leaf("c")],
                shares: vec![500, 500],
            },
            leaf("b"),
        ],
        shares: vec![500, 500],
    });
    corner_dom.leaves.insert(win("w1"), nid("a"));
    corner_dom.leaves.insert(win("w2"), nid("b"));
    corner_dom.leaves.insert(win("w3"), nid("c"));
    corner_dom.focused_leaf = Some(nid("a"));
    corner_dom.focus_stack = vec![nid("b"), nid("c"), nid("a")];
    corner_dom.focus_history = vec![nid("b"), nid("c"), nid("a")];
    corner_dom.last_active = Some(nid("a"));
    corner_dom.accepted_revision = 1;
    let obs_3 = vec![
        window("w1", "out-1", "ws-1", 0, 800),
        window("w2", "out-1", "ws-1", 800, 800),
        window("w3", "out-1", "ws-1", 0, 800),
    ];
    let ctx_3 = LocalOperationContext {
        domain: &dom,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &win("w1"),
        observed: &obs_3,
        preconverged: true,
    };
    let mut corner_layout = WorldLayout::new();
    corner_layout.domains.insert(k.clone(), corner_dom);
    let corner_out = corner_layout
        .run_pointer_resize_corner(
            &ctx_3,
            &win("w1"),
            Direction::Right,
            820,
            Direction::Down,
            470,
        )
        .expect("pointer_resize_corner succeeds");
    assert_eq!(corner_out.direction_h, Direction::Right);
    assert_eq!(corner_out.direction_v, Direction::Down);

    // 6. Toggle orientation: flips g-root for w1.
    let orient_out = fresh_two()
        .run_toggle_orientation(&ctx_2, &win("w1"))
        .expect("toggle_orientation succeeds");
    assert_eq!(orient_out.group, nid("g-root"));

    // 7. Toggle float: floats w1.
    let float_out = fresh_two()
        .run_toggle_float(&ctx_2, &win("w1"), None)
        .expect("toggle_float succeeds");
    assert_eq!(float_out.float_window, Some(win("w1")));
    assert!(float_out.float_rect.is_some());
}
