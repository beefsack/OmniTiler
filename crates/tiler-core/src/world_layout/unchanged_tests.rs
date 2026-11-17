//! Direct tests for the conservative unchanged-domain proof. No oracle,
//! snapshots or harness: a hit is checked by the boolean plus the focus tail
//! it settles, and refusals are checked to leave retained state byte-identical.

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ExceptionFlags};
use crate::size_hints::WindowSizeHints;

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

fn group(id: &str, leaves: &[&str]) -> Node {
    Node::Group {
        id: node(id),
        axis: Axis::Horizontal,
        children: leaves.iter().map(|l| leaf(l)).collect(),
        shares: vec![1; leaves.len()],
    }
}

fn observed_window(id: &str, output: &str, workspace: &str) -> EngineWindow {
    EngineWindow {
        window: window(id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
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

/// One valid two-leaf-plus domain with scrambled (non-fixed-point) focus
/// bookkeeping, an untouched second domain, and the exact unhinted observation
/// the sweep would carry. Geometry is `(0,0,120,80)`, gap/outer-gap zero.
fn fixture() -> (
    WorldLayout,
    DomainKey,
    Rect,
    i32,
    i32,
    WindowId,
    Vec<EngineWindow>,
) {
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let k = key("out", "ws");
    let mut retained = DomainLayout::new(bounds, 0, 0);
    retained.tree = Some(group("g", &["a", "b", "c"]));
    retained.leaves.insert(window("w-a"), node("a"));
    retained.leaves.insert(window("w-b"), node("b"));
    retained.leaves.insert(window("w-c"), node("c"));
    retained.focused_leaf = Some(node("b"));
    retained.focus_stack = vec![node("a"), node("stale"), node("c")];
    retained.focus_history = vec![node("a"), node("stale"), node("b")];
    retained.last_active = Some(node("stale"));
    retained.accepted_revision = 7;

    let other_key = key("out-2", "ws");
    let mut other = DomainLayout::new(bounds, 0, 0);
    other.tree = Some(leaf("z"));
    other.leaves.insert(window("w-z"), node("z"));
    other.focused_leaf = Some(node("z"));
    other.focus_stack = vec![node("z")];
    other.focus_history = vec![node("z")];
    other.last_active = Some(node("z"));
    other.accepted_revision = 3;

    let mut layout = WorldLayout::new();
    layout.domains.insert(k.clone(), retained);
    layout.domains.insert(other_key, other);
    let observed = vec![
        observed_window("w-a", "out", "ws"),
        observed_window("w-b", "out", "ws"),
        observed_window("w-c", "out", "ws"),
    ];
    (layout, k, bounds, 0, 0, window("w-b"), observed)
}

#[test]
fn unchanged_hit_settles_focus_tail_without_revision_or_highwater_bump() {
    let (mut layout, k, bounds, gap, outer_gap, focus_win, observed) = fixture();
    let other_key = key("out-2", "ws");
    let revision = layout.domains[&k].accepted_revision;
    let highwater = layout.admission_seq_next;
    let other_before = format!("{:?}", layout.domains[&other_key]);

    assert!(layout.try_unchanged_reconcile(&k, bounds, gap, outer_gap, &focus_win, &observed, &[]));

    let retained = &layout.domains[&k];
    assert_eq!(retained.focused_leaf, Some(node("b")));
    assert_eq!(
        retained.focus_stack,
        vec![node("a"), node("c"), node("b")],
        "stale leaf pruned and resolving focus demoted to MRU tail"
    );
    assert_eq!(
        retained.focus_history,
        vec![node("a"), node("b")],
        "stale history pruned without appending observed focus"
    );
    assert_eq!(retained.last_active, Some(node("b")));
    assert_eq!(
        retained.accepted_revision, revision,
        "hit is revision-neutral"
    );
    assert_eq!(
        layout.admission_seq_next, highwater,
        "hit is high-water-neutral"
    );
    assert_eq!(
        format!("{:?}", layout.domains[&other_key]),
        other_before,
        "another domain must be untouched"
    );

    // An empty observed focus is admissible and retains the resolving focus.
    let (mut empty, k, bounds, gap, outer_gap, _, observed) = fixture();
    assert!(empty.try_unchanged_reconcile(
        &k,
        bounds,
        gap,
        outer_gap,
        &WindowId(String::new()),
        &observed,
        &[]
    ));
    assert_eq!(empty.domains[&k].focused_leaf, Some(node("b")));
}

#[test]
fn unchanged_misses_are_conservative_and_do_not_mutate() {
    let (base, k, bounds, gap, outer_gap, focus_win, observed) = fixture();
    let miss = |layout: &mut WorldLayout,
                b: Rect,
                g: i32,
                og: i32,
                obs: &[EngineWindow],
                fw: &WindowId,
                closed: &[WindowId]| {
        let before = format!("{layout:?}");
        assert!(!layout.try_unchanged_reconcile(&k, b, g, og, fw, obs, closed));
        assert_eq!(format!("{layout:?}"), before, "miss must not mutate");
    };

    let mut layout = base.clone();
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        &observed,
        &focus_win,
        &[window("w-a")],
    );
    miss(
        &mut layout,
        Rect {
            w: bounds.w + 1,
            ..bounds
        },
        gap,
        outer_gap,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap + 1,
        outer_gap,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap + 1,
        &observed,
        &focus_win,
        &[],
    );
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        &observed,
        &window("w-a"),
        &[],
    );

    let flag_miss = |set: fn(&mut EngineWindow)| {
        let mut obs = observed.clone();
        set(&mut obs[0]);
        let mut layout = base.clone();
        miss(&mut layout, bounds, gap, outer_gap, &obs, &focus_win, &[]);
    };
    flag_miss(|w| w.floating = true);
    flag_miss(|w| w.fit_excluded = true);
    flag_miss(|w| w.fullscreen = true);
    flag_miss(|w| w.maximized = true);
    flag_miss(|w| w.sticky = true);
    flag_miss(|w| w.fixed_auto = true);
    flag_miss(|w| w.fixed_suppress = true);
    flag_miss(|w| {
        w.hints = WindowSizeHints {
            min_w: Some(1),
            ..WindowSizeHints::none()
        }
    });
    flag_miss(|w| w.output = OutputId("other".to_owned()));

    let mut extra = observed.clone();
    extra.push(observed_window("new", "out", "ws"));
    let mut layout = base.clone();
    miss(&mut layout, bounds, gap, outer_gap, &extra, &focus_win, &[]);
    let missing = &observed[..2];
    miss(
        &mut layout,
        bounds,
        gap,
        outer_gap,
        missing,
        &focus_win,
        &[],
    );

    let mark_miss = |change: fn(&mut DomainLayout)| {
        let mut layout = base.clone();
        change(layout.domains.get_mut(&k).expect("key domain"));
        miss(
            &mut layout,
            bounds,
            gap,
            outer_gap,
            &observed,
            &focus_win,
            &[],
        );
    };
    mark_miss(|d| {
        d.exceptions.insert(
            window("x"),
            DomainException {
                flags: ExceptionFlags::none(),
                floating_geometry: None,
            },
        );
    });
    mark_miss(|d| {
        d.minimized_slots.insert(
            window("x"),
            DomainRestoreSlot {
                leaf: node("x"),
                tree: leaf("x"),
            },
        );
    });
    mark_miss(|d| {
        d.automatic_fixed.insert(window("x"));
    });
    mark_miss(|d| {
        d.fixed_tile_override.insert(window("x"));
    });
    mark_miss(|d| {
        d.capacity_floats.insert(window("x"));
    });
    mark_miss(|d| {
        d.admission_order.insert(window("x"), 9);
    });
    mark_miss(|d| {
        d.focused_leaf = Some(node("stale"));
    });
}
