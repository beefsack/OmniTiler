use std::collections::BTreeMap;

use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{LayoutFailure, OrientationOutcome};
use super::state::{DomainLayout, WorldLayout};

fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn wide_domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn tiled_observed(window: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(window),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn complete_observed(layout: &WorldLayout, extra: Vec<ObservedWindow>) -> Vec<ObservedWindow> {
    let k = key();
    let mut windows = Vec::new();
    if let Some(domain) = layout.domain(&k) {
        for w in domain.leaves.keys() {
            windows.push(tiled_observed(&w.0));
        }
        for (w, exc) in &domain.exceptions {
            let mut row = tiled_observed(&w.0);
            row.floating = exc.flags.floating;
            row.fullscreen = exc.flags.fullscreen;
            row.maximized = exc.flags.maximized;
            row.sticky = exc.flags.sticky;
            windows.push(row);
        }
    }
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}

fn admit_in(layout: &mut WorldLayout, window: &str, placement: Rect) {
    let dom = wide_domain();
    let k = dom.key();
    if layout.domain(&k).is_none() {
        let obs = vec![tiled_observed(window)];
        let focus = win(window);
        layout
            .converge_fresh_domain(&dom, 0, &obs, Some(&focus))
            .expect("converge fresh");
        return;
    }
    let obs = complete_observed(layout, vec![tiled_observed(window)]);
    layout
        .admit_window(&k, &win(window), placement, false, false, &obs)
        .expect("admit");
}

fn toggle_in(layout: &mut WorldLayout, window: &str) -> OrientationOutcome {
    let k = key();
    let obs = complete_observed(layout, Vec::new());
    layout
        .toggle_orientation(&k, &win(window), &win(window), &obs)
        .expect("toggle_orientation")
}

fn root_axis(layout: &WorldLayout) -> Axis {
    match layout.tree(&key()).expect("tree") {
        Node::Group { axis, .. } => *axis,
        Node::Leaf { .. } => panic!("expected a root group"),
    }
}

fn root_children(layout: &WorldLayout) -> (Axis, Vec<NodeId>, Vec<u64>) {
    match layout.tree(&key()).expect("tree").clone() {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => (
            axis,
            children.iter().map(|child| child.id().clone()).collect(),
            shares,
        ),
        Node::Leaf { .. } => panic!("expected a root group"),
    }
}

fn pair_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    admit_in(
        &mut layout,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
    );
    // Wide placement selects the Horizontal root (long-edge rule).
    admit_in(
        &mut layout,
        "win-b",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
    );
    assert_eq!(root_axis(&layout), Axis::Horizontal);
    layout
}

fn domain_with_tree(tree: Node, leaves: &[(&str, &str)], focused: &str) -> WorldLayout {
    let dom = wide_domain();
    let k = dom.key();
    let mut d = DomainLayout::new(dom.bounds, dom.gap, 0);
    d.tree = Some(tree);
    for (w, l) in leaves {
        d.leaves.insert(win(w), nid(l));
    }
    d.focused_leaf = Some(nid(focused));
    d.focus_stack.push(nid(focused));
    d.focus_history.push(nid(focused));
    d.last_active = Some(nid(focused));
    let mut layout = WorldLayout::new();
    layout.domains.insert(k, d);
    layout
}

#[test]
fn root_horizontal_flips_to_vertical() {
    let mut layout = pair_layout();
    let k = key();
    let leaf_a = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-a"))
        .unwrap()
        .0
        .clone();
    let leaf_b = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-b"))
        .unwrap()
        .0
        .clone();
    let focus_leaf = layout.focus_leaf(&k).cloned();
    assert_eq!(focus_leaf.as_ref().expect("focus").0, leaf_b);

    let outcome = toggle_in(&mut layout, "win-b");
    assert_eq!(root_axis(&layout), Axis::Vertical);
    // Child order, shares, and focus are preserved; geometry is restacked.
    let (axis, children, shares) = root_children(&layout);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec![leaf_a.as_str(), leaf_b.as_str()]
    );
    assert_eq!(shares, vec![1, 1]);
    assert_eq!(layout.focus_leaf(&k).cloned(), focus_leaf);
    assert_eq!(outcome.geometry.len(), 2);
    for entry in &outcome.geometry {
        assert_eq!(entry.rect.w, 1200, "vertical split keeps full width");
        assert_eq!(entry.rect.h, 400, "vertical split stacks halves");
    }
    assert!(!outcome.geometry.iter().any(|entry| entry.overconstrained));
}

#[test]
fn double_toggle_restores_root() {
    let mut layout = pair_layout();
    let k = key();
    let before = layout.tree(&k).expect("tree").clone();
    toggle_in(&mut layout, "win-b");
    assert_eq!(root_axis(&layout), Axis::Vertical);
    let outcome = toggle_in(&mut layout, "win-b");
    assert_eq!(layout.tree(&k).expect("tree"), &before);
    // Geometry returns to the side-by-side allocation.
    assert_eq!(outcome.geometry.len(), 2);
    for entry in &outcome.geometry {
        assert_eq!(entry.rect.w, 600, "horizontal split halves the width");
        assert_eq!(entry.rect.h, 800, "horizontal split keeps full height");
    }
}

#[test]
fn nested_toggle_flips_only_the_immediate_parent() {
    // R-LAY-05 shape with unequal shares: H[A, V[B*, C]], focus on B.
    let before = Node::Group {
        id: nid("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf { id: nid("a") },
            Node::Group {
                id: nid("g"),
                axis: Axis::Vertical,
                children: vec![Node::Leaf { id: nid("b") }, Node::Leaf { id: nid("c") }],
                shares: vec![1, 3],
            },
        ],
        shares: vec![2, 1],
    };
    let mut layout = domain_with_tree(
        before.clone(),
        &[("win-a", "a"), ("win-b", "b"), ("win-c", "c")],
        "b",
    );
    let k = key();
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");

    // First toggle flips only the immediate (nested) parent to H.
    let outcome = toggle_in(&mut layout, "win-b");
    assert_eq!(outcome.geometry.len(), 3);
    let after_first = layout.tree(&k).expect("tree").clone();
    match &after_first {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal, "root axis untouched");
            assert_eq!(*shares, vec![2, 1], "root shares untouched");
            assert_eq!(children[0].id().0, "a", "root order untouched");
            match &children[1] {
                Node::Group {
                    axis,
                    children,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Horizontal, "nested parent flips V->H");
                    assert_eq!(*shares, vec![1, 3], "nested shares untouched");
                    assert_eq!(
                        children
                            .iter()
                            .map(|child| child.id().0.as_str())
                            .collect::<Vec<_>>(),
                        vec!["b", "c"],
                        "nested order untouched"
                    );
                }
                Node::Leaf { .. } => panic!("expected the nested group"),
            }
        }
        Node::Leaf { .. } => panic!("expected the root group"),
    }
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");

    // Second toggle restores the exact original tree.
    toggle_in(&mut layout, "win-b");
    assert_eq!(layout.tree(&k).expect("tree"), &before);
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");
}

#[test]
fn unequal_shares_order_and_focus_survive_the_flip() {
    let tree = Node::Group {
        id: nid("root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf { id: nid("a") },
            Node::Leaf { id: nid("b") },
            Node::Leaf { id: nid("c") },
        ],
        shares: vec![1, 2, 3],
    };
    let mut layout = domain_with_tree(tree, &[("win-a", "a"), ("win-b", "b"), ("win-c", "c")], "b");
    let k = key();
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");

    toggle_in(&mut layout, "win-b");
    let (axis, children, shares) = root_children(&layout);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(shares, vec![1, 2, 3], "shares travel with the flip");
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");

    // A second toggle restores the exact original tree, shares included.
    toggle_in(&mut layout, "win-b");
    match layout.tree(&k).expect("tree") {
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 2, 3]);
            assert_eq!(
                children
                    .iter()
                    .map(|child| child.id().0.as_str())
                    .collect::<Vec<_>>(),
                vec!["a", "b", "c"]
            );
        }
        Node::Leaf { .. } => panic!("expected the root group"),
    }
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, "b");
}

#[test]
fn minima_reproject_through_the_existing_hints_path() {
    let mut layout = pair_layout();
    let k = key();
    let mut windows = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    windows[0].hints = WindowSizeHints {
        min_w: Some(200),
        min_h: Some(200),
        max_w: None,
        max_h: None,
    };
    windows[1].hints = WindowSizeHints {
        min_w: Some(800),
        min_h: Some(500),
        max_w: None,
        max_h: None,
    };
    let outcome = layout
        .toggle_orientation(&k, &win("win-b"), &win("win-b"), &windows)
        .expect("toggle with satisfiable hints");
    assert_eq!(root_axis(&layout), Axis::Vertical);
    let rect_of = |outcome: &OrientationOutcome, window: &str| {
        outcome
            .geometry
            .iter()
            .find(|entry| entry.window.0 == window)
            .unwrap_or_else(|| panic!("geometry for {window}"))
            .rect
    };
    let rect_a = rect_of(&outcome, "win-a");
    let rect_b = rect_of(&outcome, "win-b");
    assert!(!outcome.geometry.iter().any(|entry| entry.overconstrained));
    assert_eq!(
        rect_a,
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 300
        }
    );
    assert_eq!(
        rect_b,
        Rect {
            x: 0,
            y: 300,
            w: 1200,
            h: 500
        }
    );
    let (_, _, shares) = root_children(&layout);
    assert_eq!(shares, vec![1, 1], "projection never rewrites shares");

    // Unsatisfiable minima keep the proportional fallback flagged overconstrained.
    let mut tight_layout = pair_layout();
    let mut tight = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    tight[0].hints = WindowSizeHints {
        min_w: None,
        min_h: Some(300),
        max_w: None,
        max_h: None,
    };
    tight[1].hints = WindowSizeHints {
        min_w: None,
        min_h: Some(700),
        max_w: None,
        max_h: None,
    };
    let tight_outcome = tight_layout
        .toggle_orientation(&k, &win("win-b"), &win("win-b"), &tight)
        .expect("toggle with unsatisfiable hints");
    let entry = tight_outcome
        .geometry
        .iter()
        .find(|entry| entry.window.0 == "win-b")
        .expect("mover geometry");
    assert!(
        entry.overconstrained,
        "unsatisfiable minimum flags the window"
    );
}

#[test]
fn lone_root_leaf_is_an_unchanged_noop() {
    let mut layout = WorldLayout::new();
    admit_in(
        &mut layout,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
    );
    let k = key();
    assert!(matches!(layout.tree(&k).expect("tree"), Node::Leaf { .. }));
    let obs = complete_observed(&layout, Vec::new());
    let err = layout
        .toggle_orientation(&k, &win("win-a"), &win("win-a"), &obs)
        .expect_err("lone root leaf must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));

    // No orientation hint is saved by the refusal: admitting B on the wide
    // fixture keeps the long-edge rule (Horizontal root, R-LAY-06 shape).
    admit_in(
        &mut layout,
        "win-b",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
    );
    let (axis, children, _) = root_children(&layout);
    assert_eq!(
        axis,
        Axis::Horizontal,
        "long-edge admission, no toggle hint"
    );
    let leaf_a = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-a"))
        .unwrap()
        .0
        .clone();
    let leaf_b = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-b"))
        .unwrap()
        .0
        .clone();
    assert_eq!(
        children.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        vec![leaf_a.as_str(), leaf_b.as_str()]
    );
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, leaf_b);
}

#[test]
fn floating_subject_and_focus_mismatch_refuse() {
    let mut layout = pair_layout();
    let k = key();
    let obs = complete_observed(&layout, Vec::new());
    layout
        .toggle_float(
            &k,
            &win("win-b"),
            Some(Rect {
                x: 240,
                y: 160,
                w: 720,
                h: 480,
            }),
            &obs,
        )
        .expect("float win-b");
    let leaf_a = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-a"))
        .unwrap()
        .0
        .clone();
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, leaf_a);
    let obs = complete_observed(&layout, Vec::new());
    let err = layout
        .toggle_orientation(&k, &win("win-b"), &win("win-a"), &obs)
        .expect_err("floating subject must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::NotTiled));

    // Floating focus with no tiled focus left: a lone window floated away.
    let mut single = WorldLayout::new();
    admit_in(
        &mut single,
        "win-a",
        Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 50,
        },
    );
    let obs = complete_observed(&single, Vec::new());
    single
        .toggle_float(&k, &win("win-a"), None, &obs)
        .expect("float win-a");
    assert_eq!(single.focus_leaf(&k), None);
    let obs = complete_observed(&single, Vec::new());
    let err = single
        .toggle_orientation(&k, &win("win-a"), &win("win-a"), &obs)
        .expect_err("floating focus must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::NotTiled));

    // A tiled but unfocused window mismatches focus; unknown windows refuse.
    let mut fresh = pair_layout();
    let obs = complete_observed(&fresh, Vec::new());
    let err = fresh
        .toggle_orientation(&k, &win("win-a"), &win("win-b"), &obs)
        .expect_err("unfocused window must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::FocusMismatch));
    let err = fresh
        .toggle_orientation(&k, &win("win-zzz"), &win("win-b"), &obs)
        .expect_err("unknown window must refuse");
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::UnknownWindow));
}

#[test]
fn overlay_flagged_focus_never_reaches_proposal() {
    let mut layout = pair_layout();
    let k = key();
    let before = layout.tree(&k).expect("tree").clone();
    let mut windows = vec![tiled_observed("win-a"), tiled_observed("win-b")];
    windows[1].fullscreen = true;
    let err = layout
        .toggle_orientation(&k, &win("win-b"), &win("win-b"), &windows)
        .expect_err("flagged observation must refuse");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
    assert_eq!(
        layout.tree(&k).expect("tree"),
        &before,
        "refusal mutates nothing"
    );
}

#[test]
fn subsequent_admission_keeps_the_long_edge_rule() {
    let mut layout = pair_layout();
    let k = key();
    toggle_in(&mut layout, "win-b");
    assert_eq!(root_axis(&layout), Axis::Vertical);
    // The focused leaf now projects wide (1200x400): admission wraps it in
    // a Horizontal group while the toggled root stays Vertical.
    admit_in(
        &mut layout,
        "win-c",
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 400,
        },
    );
    let (axis, children, _) = root_children(&layout);
    assert_eq!(axis, Axis::Vertical, "toggled root keeps its axis");
    assert_eq!(children.len(), 2);
    let nested = match layout.tree(&k).expect("tree") {
        Node::Group { children, .. } => children[1].clone(),
        Node::Leaf { .. } => panic!("expected a root group"),
    };
    let leaf_b = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-b"))
        .unwrap()
        .0
        .clone();
    let leaf_c = layout
        .leaves(&k)
        .unwrap()
        .get(&win("win-c"))
        .unwrap()
        .0
        .clone();
    match nested {
        Node::Group { axis, children, .. } => {
            assert_eq!(axis, Axis::Horizontal, "long-edge admission wraps wide");
            assert_eq!(
                children
                    .iter()
                    .map(|child| child.id().0.as_str())
                    .collect::<Vec<_>>(),
                vec![leaf_b.as_str(), leaf_c.as_str()]
            );
        }
        Node::Leaf { .. } => panic!("expected an admission group"),
    }
    assert_eq!(layout.focus_leaf(&k).expect("focus").0, leaf_c);
}
