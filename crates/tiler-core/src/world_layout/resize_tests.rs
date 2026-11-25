//! Direct tests for typed synchronous keyboard, pointer edge, and pointer
//! corner resize operations on `WorldLayout`.

use crate::contract::{MAX_REVISION, ResizeMode};
use crate::directional::{Axis, Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::LayoutFailure;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn node(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn leaf(id: &str) -> Node {
    Node::Leaf { id: node(id) }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn obs(id: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: window(id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn two_leaf_layout(bounds: Rect, gap: i32) -> (WorldLayout, DomainKey, Vec<ObservedWindow>) {
    let k = key("mon-a", "ws-1");
    let mut domain = DomainLayout::new(bounds, gap, 0);
    domain.tree = Some(Node::Group {
        id: node("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("l1"), leaf("l2")],
        shares: vec![1, 1],
    });
    domain.leaves.insert(window("w1"), node("l1"));
    domain.leaves.insert(window("w2"), node("l2"));
    domain.focused_leaf = Some(node("l2"));
    domain.focus_stack = vec![node("l1"), node("l2")];
    domain.focus_history = vec![node("l1"), node("l2")];
    domain.last_active = Some(node("l2"));
    domain.accepted_revision = 1;

    let mut layout = WorldLayout::new();
    layout.domains.insert(k.clone(), domain);
    let observed = vec![obs("w1", "mon-a", "ws-1"), obs("w2", "mon-a", "ws-1")];
    (layout, k, observed)
}

fn geom_for(geometry: &[crate::session::DesiredGeometry], win: &str) -> Rect {
    geometry
        .iter()
        .find(|g| g.window.0 == win)
        .unwrap_or_else(|| panic!("missing geometry for {win}"))
        .rect
}

#[test]
fn keyboard_resize_initial_12px_and_repeat_schedule_caps_at_20px() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, observed) = two_leaf_layout(bounds, 8);

    let first = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("initial outwards");
    assert_eq!(first.base_revision, 1);
    assert_eq!(first.revision, 2);
    assert_eq!(first.focus_leaf, node("l2"));
    assert_eq!(geom_for(&first.geometry, "w1"), rect(8, 8, 776, 884));
    assert_eq!(geom_for(&first.geometry, "w2"), rect(792, 8, 800, 884));
    assert_ne!(first.operation.old_shares, first.operation.new_shares);

    let mut prev_w = geom_for(&first.geometry, "w2").w;
    let mut deltas = vec![prev_w - 788];
    for press_index in 1..=5u32 {
        let outcome = layout
            .keyboard_resize(
                &k,
                &window("w2"),
                Direction::Left,
                ResizeMode::Outwards,
                press_index,
                &window("w2"),
                &observed,
                crate::policy::ScaledResize::unscaled(),
            )
            .expect("repeat press");
        let w1 = geom_for(&outcome.geometry, "w1").w;
        let w2 = geom_for(&outcome.geometry, "w2").w;
        assert_eq!(w1 + w2, 788 + 788);
        deltas.push(w2 - prev_w);
        prev_w = w2;
    }
    assert_eq!(deltas, vec![12, 14, 16, 18, 20, 20]);
}

#[test]
fn keyboard_resize_inwards_reverses_outwards() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, observed) = two_leaf_layout(bounds, 8);

    let grown = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("grow");
    assert_eq!(geom_for(&grown.geometry, "w2").w, 800);

    let shrunk = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Inwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("shrink");
    assert_eq!(geom_for(&shrunk.geometry, "w1"), rect(8, 8, 788, 884));
    assert_eq!(geom_for(&shrunk.geometry, "w2"), rect(804, 8, 788, 884));
}

#[test]
fn keyboard_resize_refusal_keeps_pre_op_focus_sync_local_and_other_domain_untouched() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, observed) = two_leaf_layout(bounds, 8);
    let other_key = key("mon-b", "ws-1");
    let mut other = DomainLayout::new(bounds, 0, 0);
    other.tree = Some(leaf("lz"));
    other.leaves.insert(window("wz"), node("lz"));
    other.focused_leaf = Some(node("lz"));
    other.accepted_revision = 4;
    layout.domains.insert(other_key.clone(), other);

    let before_domain = format!("{:?}", layout.domains[&k]);
    let before_other = format!("{:?}", layout.domains[&other_key]);
    let before_highwater = layout.admission_seq_next;

    // Observed focus points at w1, while subject is w1, but direction Left has
    // no neighbor on the outer edge -> refuses Unchanged. Pre-op focus sync to
    // w1 must NOT leak into retained state.
    let err = layout
        .keyboard_resize(
            &k,
            &window("w1"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w1"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("outer edge must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));

    // Targeting w2 while observed_focus points at w1 syncs local focus to w1
    // and must refuse FocusMismatch before directional Up exhaustion.
    let err = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Up,
            ResizeMode::Outwards,
            0,
            &window("w1"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("focus mismatch must refuse before cross-axis exhaustion");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::FocusMismatch));

    assert_eq!(format!("{:?}", layout.domains[&k]), before_domain);
    assert_eq!(format!("{:?}", layout.domains[&other_key]), before_other);
    assert_eq!(layout.admission_seq_next, before_highwater);
}

#[test]
fn keyboard_resize_minimum_clamps_shrink_and_below_minimum_refuses() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, mut observed) = two_leaf_layout(bounds, 8);
    observed[0].hints = WindowSizeHints {
        min_w: Some(784),
        min_h: None,
        max_w: None,
        max_h: None,
    };

    let clamped = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("clamped grow");
    assert_eq!(geom_for(&clamped.geometry, "w1"), rect(8, 8, 784, 884));
    assert_eq!(geom_for(&clamped.geometry, "w2"), rect(800, 8, 792, 884));

    // Narrow domain where pair_sum < COSMIC horizontal pair minimum (720px).
    let narrow_bounds = rect(0, 0, 700, 600);
    let (mut narrow_layout, nk, narrow_obs) = two_leaf_layout(narrow_bounds, 0);
    let err = narrow_layout
        .keyboard_resize(
            &nk,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &narrow_obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("pair below minimum");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::PairBelowMinimum));
    assert_eq!(narrow_layout.accepted_revision(&nk), 1);
}

#[test]
fn resize_commit_tail_preserves_last_active_history_and_capacity_provenance() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, mut observed) = two_leaf_layout(bounds, 8);
    let domain = layout.domains.get_mut(&k).expect("domain");
    domain.focused_leaf = Some(node("l2"));
    domain.last_active = Some(node("l1"));
    domain.focus_stack = vec![node("l1"), node("stale-leaf"), node("l2")];
    domain.focus_history = vec![node("l2"), node("stale-leaf"), node("l1")];
    domain.automatic_fixed.insert(window("w1"));
    domain.automatic_fixed.insert(window("gone-win"));
    domain.fixed_tile_override.insert(window("w2"));
    domain.fixed_tile_override.insert(window("gone-win"));
    domain.capacity_floats.insert(window("cap-win"));
    domain.admission_order.insert(window("w1"), 10);
    domain.admission_order.insert(window("cap-win"), 11);
    domain.minimized_slots.insert(
        window("min-win"),
        DomainRestoreSlot {
            leaf: node("l-min"),
            tree: leaf("l-min"),
        },
    );
    domain.exceptions.insert(
        window("cap-win"),
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
    let mut cap_obs = obs("cap-win", "mon-a", "ws-1");
    cap_obs.floating = true;
    observed.push(cap_obs);

    let outcome = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("resize");
    assert_eq!(outcome.revision, 2);

    let updated = &layout.domains[&k];
    // `verify_resize` preserves `last_active` and `focus_history` order (pruned
    // only), prunes fixed-size provenance, and keeps `capacity_floats` and
    // `admission_order` intact.
    assert_eq!(updated.last_active, Some(node("l1")));
    assert_eq!(updated.focus_stack, vec![node("l1"), node("l2")]);
    assert_eq!(updated.focus_history, vec![node("l2"), node("l1")]);
    assert!(updated.automatic_fixed.contains(&window("w1")));
    assert!(!updated.automatic_fixed.contains(&window("gone-win")));
    assert!(updated.fixed_tile_override.contains(&window("w2")));
    assert!(!updated.fixed_tile_override.contains(&window("gone-win")));
    assert!(updated.capacity_floats.contains(&window("cap-win")));
    assert_eq!(updated.admission_order.get(&window("cap-win")), Some(&11));
}

#[test]
fn resize_revision_limit_refuses_without_mutation() {
    let bounds = rect(8, 8, 1584, 884);
    let (mut layout, k, observed) = two_leaf_layout(bounds, 8);
    layout
        .domains
        .get_mut(&k)
        .expect("domain")
        .accepted_revision = MAX_REVISION;
    let before = format!("{:?}", layout.domains[&k]);

    let err = layout
        .keyboard_resize(
            &k,
            &window("w2"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("revision limit");
    assert_eq!(
        err,
        LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        }
    );
    assert_eq!(format!("{:?}", layout.domains[&k]), before);
}

#[test]
fn pointer_resize_non_focused_and_overlay_flags_distinction() {
    let bounds = rect(0, 0, 1600, 900);
    let (mut layout, k, mut observed) = two_leaf_layout(bounds, 0);
    // w2 is active focus and carries an advisory fullscreen overlay flag;
    // pointer resize targets non-focused w1 and must succeed while preserving
    // active focus on l2.
    observed[1].fullscreen = true;

    let outcome = layout
        .pointer_resize(
            &k,
            &window("w1"),
            Direction::Right,
            700,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("pointer resize on non-focused w1");
    assert_eq!(outcome.base_revision, 1);
    assert_eq!(outcome.revision, 2);
    assert_eq!(outcome.focus_leaf, node("l2"));
    assert_eq!(outcome.operation.focused_window, window("w1"));
    assert_eq!(outcome.operation.focused_leaf, node("l1"));
    assert_eq!(geom_for(&outcome.geometry, "w1"), rect(0, 0, 700, 900));
    assert_eq!(geom_for(&outcome.geometry, "w2"), rect(700, 0, 900, 900));

    // Subject itself carrying an overlay flag refuses PartialObservation (from
    // pointer observation binding).
    observed[0].floating = true;
    let err = layout
        .pointer_resize(
            &k,
            &window("w1"),
            Direction::Right,
            650,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("flagged subject refuses");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );

    // Out-of-work-area coordinate refuses MalformedInput.
    observed[0].floating = false;
    let err = layout
        .pointer_resize(
            &k,
            &window("w1"),
            Direction::Right,
            1601,
            &window("w2"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("out of bounds refuses");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::MalformedInput));
}

#[test]
fn pointer_resize_corner_normalizes_axes_and_refuses_atomically() {
    // Root H[g_left: V[l1, l2], l3] on (0, 0, 1600, 900), gap 0.
    // Dragging l1's bottom-right corner resizes g_root (H) and g_left (V).
    let bounds = rect(0, 0, 1600, 900);
    let k = key("mon-a", "ws-1");
    let mut domain = DomainLayout::new(bounds, 0, 0);
    domain.tree = Some(Node::Group {
        id: node("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Group {
                id: node("g-left"),
                axis: Axis::Vertical,
                children: vec![leaf("l1"), leaf("l2")],
                shares: vec![1, 1],
            },
            leaf("l3"),
        ],
        shares: vec![1, 1],
    });
    domain.leaves.insert(window("w1"), node("l1"));
    domain.leaves.insert(window("w2"), node("l2"));
    domain.leaves.insert(window("w3"), node("l3"));
    domain.focused_leaf = Some(node("l1"));
    domain.focus_stack = vec![node("l3"), node("l2"), node("l1")];
    domain.accepted_revision = 5;

    let mut layout = WorldLayout::new();
    layout.domains.insert(k.clone(), domain);
    let observed = vec![
        obs("w1", "mon-a", "ws-1"),
        obs("w2", "mon-a", "ws-1"),
        obs("w3", "mon-a", "ws-1"),
    ];

    // Same-axis directions refuse MalformedInput.
    let err = layout
        .pointer_resize_corner(
            &k,
            &window("w1"),
            Direction::Left,
            700,
            Direction::Right,
            900,
            &window("w1"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("same axis must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::MalformedInput));

    // Atomicity: horizontal (Right -> 700) is valid, but vertical (Up has no
    // neighbor for l1) is Unchanged -> whole corner refuses Unchanged and leaves
    // horizontal shares and revision untouched.
    let before = format!("{:?}", layout.domains[&k]);
    let err = layout
        .pointer_resize_corner(
            &k,
            &window("w1"),
            Direction::Right,
            700,
            Direction::Up,
            300,
            &window("w1"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect_err("partial corner must refuse atomically");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert_eq!(format!("{:?}", layout.domains[&k]), before);

    // Supply vertical first, horizontal second: normalized to H then V, single
    // revision increment (5 -> 6).
    let outcome = layout
        .pointer_resize_corner(
            &k,
            &window("w1"),
            Direction::Down,
            400,
            Direction::Right,
            700,
            &window("w1"),
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("corner resize");
    assert_eq!(outcome.base_revision, 5);
    assert_eq!(outcome.revision, 6);
    assert_eq!(outcome.direction_h, Direction::Right);
    assert_eq!(outcome.boundary_h, 700);
    assert_eq!(outcome.direction_v, Direction::Down);
    assert_eq!(outcome.boundary_v, 400);
    assert_eq!(geom_for(&outcome.geometry, "w1"), rect(0, 0, 700, 400));
    assert_eq!(geom_for(&outcome.geometry, "w2"), rect(0, 400, 700, 500));
    assert_eq!(geom_for(&outcome.geometry, "w3"), rect(700, 0, 900, 900));
}
