use crate::contract::MAX_REVISION;
use crate::directional::{
    Axis, Capability, Direction, Node, NodeId, OutputId, Rule, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::LayoutFailure;
use super::state::{DomainException, DomainLayout, WorldLayout};

fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    }
}

fn other_key() -> DomainKey {
    DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
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

fn obs(window: &str) -> ObservedWindow {
    let k = key();
    ObservedWindow {
        window: win(window),
        output: k.output,
        workspace: k.workspace,
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn base_domain(tree: Node, leaves: &[(&str, &str)], focused: &str) -> DomainLayout {
    let mut domain = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 1200,
            h: 800,
        },
        0,
        0,
    );
    domain.tree = Some(tree);
    for (w, l) in leaves {
        domain.leaves.insert(win(w), nid(l));
    }
    domain.focused_leaf = Some(nid(focused));
    domain.focus_stack = vec![nid(focused)];
    domain.last_active = Some(nid(focused));
    domain
}

#[test]
fn local_move_r1_r2a_r2b_r2c_r3_smoke_and_same_axis_modes() {
    let k = key();

    // R1: horizontal root [a, b], move a Down -> wraps perpendicular Vertical.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b")],
        shares: vec![500, 500],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b")], "a"),
    );
    let observed = vec![obs("w1"), obs("w2")];

    let r1 = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Down,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &observed,
        )
        .expect("R1 wrap perpendicular");
    assert_eq!(r1.rule, Rule::R1);
    assert_eq!(r1.capability, Capability::WrapPerpendicular);
    assert_eq!(r1.base_revision, 0);
    assert_eq!(r1.revision, 1);

    // R2a: horizontal binary [a, b], move a Right -> SwapNeighbor.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b")],
        shares: vec![300, 700],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b")], "a"),
    );
    let r2a = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &observed,
        )
        .expect("R2a swap");
    assert_eq!(r2a.rule, Rule::R2a);
    assert_eq!(r2a.capability, Capability::SwapNeighbor);
    match layout.tree(&k).expect("tree") {
        Node::Group {
            children, shares, ..
        } => {
            assert_eq!(children, &[leaf("b"), leaf("a")]);
            assert_eq!(shares, &[700, 300]);
        }
        other => panic!("expected group, got {other:?}"),
    }

    // R2b InsertIntoGroup: binary [a, g-sub(H: [b, c])], move a Right -> inserts at near edge of g-sub.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("a"),
            Node::Group {
                id: nid("g-sub"),
                axis: Axis::Horizontal,
                children: vec![leaf("b"), leaf("c")],
                shares: vec![500, 500],
            },
        ],
        shares: vec![500, 500],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b"), ("w3", "c")], "a"),
    );
    let obs3 = vec![obs("w1"), obs("w2"), obs("w3")];
    let r2b_insert = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &obs3,
        )
        .expect("R2b insert into group");
    assert_eq!(r2b_insert.rule, Rule::R2b);
    assert_eq!(r2b_insert.capability, Capability::InsertChild);

    // R2b SplitGroupChild: binary [a, g-sub(V: [b, c, d])], move a Right -> splits midpoint c.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("a"),
            Node::Group {
                id: nid("g-sub"),
                axis: Axis::Vertical,
                children: vec![leaf("b"), leaf("c"), leaf("d")],
                shares: vec![333, 334, 333],
            },
        ],
        shares: vec![500, 500],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(
            tree,
            &[("w1", "a"), ("w2", "b"), ("w3", "c"), ("w4", "d")],
            "a",
        ),
    );
    let obs4 = vec![obs("w1"), obs("w2"), obs("w3"), obs("w4")];
    let r2b_split = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &obs4,
        )
        .expect("R2b split group child");
    assert_eq!(r2b_split.rule, Rule::R2b);
    assert_eq!(r2b_split.capability, Capability::SplitGroupChild);

    // R2c WrapNeighbor under GroupWithNeighbor vs SwapNeighbor under SwapWithNeighbor.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b"), leaf("c")],
        shares: vec![300, 400, 300],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree.clone(), &[("w1", "a"), ("w2", "b"), ("w3", "c")], "a"),
    );
    let r2c_wrap = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &obs3,
        )
        .expect("R2c wrap neighbor");
    assert_eq!(r2c_wrap.rule, Rule::R2c);
    assert_eq!(r2c_wrap.capability, Capability::WrapSiblings);

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b"), ("w3", "c")], "a"),
    );
    let r2c_swap = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::SwapWithNeighbor,
            &win("w1"),
            &obs3,
        )
        .expect("R2c swap neighbor");
    assert_eq!(r2c_swap.rule, Rule::R2c);
    assert_eq!(r2c_swap.capability, Capability::SwapNeighbor);

    // R3 EscapeParent: g-root(H: [g-sub(H: [a, b]), c]), move b Right -> escapes to g-root.
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            Node::Group {
                id: nid("g-sub"),
                axis: Axis::Horizontal,
                children: vec![leaf("a"), leaf("b")],
                shares: vec![500, 500],
            },
            leaf("c"),
        ],
        shares: vec![500, 500],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b"), ("w3", "c")], "b"),
    );
    let r3 = layout
        .local_move(
            &k,
            &win("w2"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w2"),
            &obs3,
        )
        .expect("R3 escape parent");
    assert_eq!(r3.rule, Rule::R3);
    assert_eq!(r3.capability, Capability::ReparentLeaf);
}

#[test]
fn local_focus_history_descent_edge_and_nonfixed_point_tails() {
    let k = key();
    let mut layout = WorldLayout::new();
    // g-root(H: [a, g-sub(V: [b, c])]), focus on a, focus_history has c newest -> Right picks c.
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("a"),
            Node::Group {
                id: nid("g-sub"),
                axis: Axis::Vertical,
                children: vec![leaf("b"), leaf("c")],
                shares: vec![500, 500],
            },
        ],
        shares: vec![500, 500],
    };
    let mut domain = base_domain(tree, &[("w1", "a"), ("w2", "b"), ("w3", "c")], "a");
    domain.focus_history = vec![nid("b"), nid("c"), nid("stale")];
    // Add an orphan capacity_floats id and orphan automatic_fixed id to verify
    // focus prunes fixed only, while move prunes both fixed and capacity.
    domain.automatic_fixed.insert(win("orphan-fixed"));
    domain.capacity_floats.insert(win("orphan-cap"));
    layout.domains.insert(k.clone(), domain);

    let obs3 = vec![obs("w1"), obs("w2"), obs("w3")];
    let focus_out = layout
        .local_focus(&k, &win("w1"), Direction::Right, &win("w1"), &obs3)
        .expect("focus descends via history");
    assert_eq!(focus_out.to_window, win("w3"));
    assert_eq!(focus_out.to_leaf, nid("c"));
    assert_eq!(focus_out.base_revision, 0);
    assert_eq!(focus_out.revision, 1);

    // Focus tail: focus_history pruned stale and demoted/appended c; last_active is c;
    // fixed provenance pruned, capacity provenance untouched.
    let after_focus = layout.domain(&k).expect("domain");
    assert_eq!(after_focus.focus_history, vec![nid("b"), nid("c")]);
    assert_eq!(after_focus.last_active, Some(nid("c")));
    assert!(!after_focus.automatic_fixed.contains(&win("orphan-fixed")));
    assert!(after_focus.capacity_floats.contains(&win("orphan-cap")));

    // Edge direction refuses Unchanged without mutating state.
    let edge_err = layout
        .local_focus(&k, &win("w3"), Direction::Right, &win("w3"), &obs3)
        .expect_err("right edge refuses Unchanged");
    assert_eq!(edge_err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    assert_eq!(layout.domain(&k).expect("domain").accepted_revision, 1);

    // Now sync focus to w2 via observed_focus on local_move (w2 Down swaps with w3),
    // verifying move updates focus_stack/last_active, prunes focus_history WITHOUT
    // appending b, and prunes capacity provenance too.
    let move_out = layout
        .local_move(
            &k,
            &win("w2"),
            Direction::Down,
            SameAxisMove::GroupWithNeighbor,
            &win("w2"),
            &obs3,
        )
        .expect("move w2 down");
    assert_eq!(move_out.revision, 2);
    let after_move = layout.domain(&k).expect("domain");
    assert_eq!(after_move.focus_history, vec![nid("b"), nid("c")]);
    assert_eq!(after_move.last_active, Some(nid("b")));
    assert_eq!(after_move.focus_stack.last(), Some(&nid("b")));
    assert!(!after_move.capacity_floats.contains(&win("orphan-cap")));
}

#[test]
fn focus_overlay_rule_and_gate_order_differences() {
    let k = key();
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b")],
        shares: vec![500, 500],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(tree, &[("w1", "a"), ("w2", "b")], "a"),
    );

    let mut w1_max = obs("w1");
    w1_max.maximized = true;
    let obs_max = vec![w1_max, obs("w2")];

    // Under fixed_size_admission, maximized tiled subject refuses Unchanged in all
    // directions for local_focus, while local_move tolerates the overlay flag.
    for dir in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        let err = layout
            .local_focus(&k, &win("w1"), dir, &win("w1"), &obs_max)
            .expect_err("maximized subject skips siblings");
        assert_eq!(err, LayoutFailure::Semantics(RefusalKind::Unchanged));
    }
    let moved = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &obs_max,
        )
        .expect("move tolerates overlay flag under fixed_size_admission");
    assert_eq!(moved.rule, Rule::R2a);

    // Gate order difference: empty subject + incomplete observation.
    // FOCUS checks subject-empty BEFORE observed shapes/completeness -> MalformedInput.
    // MOVE checks subject-empty AFTER exact membership -> PartialObservation.
    let partial_obs = vec![obs("w1")];
    let focus_err = layout
        .local_focus(&k, &win(""), Direction::Right, &win("w1"), &partial_obs)
        .expect_err("focus empty subject first");
    assert_eq!(
        focus_err,
        LayoutFailure::Semantics(RefusalKind::MalformedInput)
    );
    let move_err = layout
        .local_move(
            &k,
            &win(""),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &partial_obs,
        )
        .expect_err("move checks membership before empty subject");
    assert_eq!(
        move_err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}

#[test]
fn atomic_refusal_and_revision_bound_preserves_allocator_burns() {
    let k = key();
    let other = other_key();
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("g-root"),
        axis: Axis::Horizontal,
        children: vec![leaf("a"), leaf("b")],
        shares: vec![500, 500],
    };
    let mut domain = base_domain(tree.clone(), &[("w1", "a"), ("w2", "b")], "a");
    domain.accepted_revision = MAX_REVISION;
    domain.exceptions.insert(
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
    layout.domains.insert(k.clone(), domain);
    let other_domain = DomainLayout::new(
        Rect {
            x: 1200,
            y: 0,
            w: 1000,
            h: 800,
        },
        0,
        0,
    );
    layout.domains.insert(other.clone(), other_domain);

    let mut f1_obs = obs("f1");
    f1_obs.floating = true;
    let obs_all = vec![obs("w1"), obs("w2"), f1_obs];

    // R1 wrap mints a new group id before hitting MAX_REVISION; candidate is discarded
    // atomically, but the allocator burn survives.
    let alloc_before = layout.group_alloc.next_value();
    let err = layout
        .local_move(
            &k,
            &win("w1"),
            Direction::Down,
            SameAxisMove::GroupWithNeighbor,
            &win("w1"),
            &obs_all,
        )
        .expect_err("revision limit");
    assert_eq!(
        err,
        LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        }
    );
    assert_eq!(layout.tree(&k), Some(&tree));
    assert_eq!(
        layout.domain(&k).expect("domain").accepted_revision,
        MAX_REVISION
    );
    assert!(
        layout
            .domain(&other)
            .expect("foreign untouched")
            .tree
            .is_none()
    );
    assert!(layout.group_alloc.next_value() > alloc_before);
}

#[test]
fn rmov09_flat_swap_and_wrap_shares_preserve_traveling_shares() {
    let k = key();

    // R-MOV-09 both directions: H[A, B*, C, D] shares [1, 2, 3, 4].
    // Move B right -> H[A, C, B, D] shares [1, 3, 2, 4]; move C left -> same.
    for (focused_win, focused_leaf, direction) in
        [("w2", "B", Direction::Right), ("w3", "C", Direction::Left)]
    {
        let mut layout = WorldLayout::new();
        let tree = Node::Group {
            id: nid("root"),
            axis: Axis::Horizontal,
            children: vec![leaf("A"), leaf("B"), leaf("C"), leaf("D")],
            shares: vec![1, 2, 3, 4],
        };
        layout.domains.insert(
            k.clone(),
            base_domain(
                tree,
                &[("w1", "A"), ("w2", "B"), ("w3", "C"), ("w4", "D")],
                focused_leaf,
            ),
        );
        let observed = vec![obs("w1"), obs("w2"), obs("w3"), obs("w4")];
        let out = layout
            .local_move(
                &k,
                &win(focused_win),
                direction,
                SameAxisMove::SwapWithNeighbor,
                &win(focused_win),
                &observed,
            )
            .expect("flat swap applies");
        assert_eq!(out.rule, Rule::R2c);
        assert_eq!(out.capability, Capability::SwapNeighbor);
        assert_eq!(out.focus_leaf, nid(focused_leaf));
        match layout.tree(&k).expect("tree") {
            Node::Group {
                children, shares, ..
            } => {
                let ids: Vec<&str> = children.iter().map(|c| c.id().0.as_str()).collect();
                assert_eq!(ids, vec!["A", "C", "B", "D"]);
                assert_eq!(shares, &[1, 3, 2, 4]);
            }
            other => panic!("expected root group, got {other:?}"),
        }
    }

    // Same world under GroupWithNeighbor nests B+C with summed pair share [1, 2, 1].
    let mut layout = WorldLayout::new();
    let tree = Node::Group {
        id: nid("root"),
        axis: Axis::Horizontal,
        children: vec![leaf("A"), leaf("B"), leaf("C"), leaf("D")],
        shares: vec![1, 1, 1, 1],
    };
    layout.domains.insert(
        k.clone(),
        base_domain(
            tree,
            &[("w1", "A"), ("w2", "B"), ("w3", "C"), ("w4", "D")],
            "B",
        ),
    );
    let observed = vec![obs("w1"), obs("w2"), obs("w3"), obs("w4")];
    let out = layout
        .local_move(
            &k,
            &win("w2"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("w2"),
            &observed,
        )
        .expect("wrap applies");
    assert_eq!(out.rule, Rule::R2c);
    assert_eq!(out.capability, Capability::WrapSiblings);
    match layout.tree(&k).expect("tree") {
        Node::Group {
            children, shares, ..
        } => {
            assert_eq!(children.len(), 3);
            assert_eq!(shares, &[1, 2, 1]);
        }
        other => panic!("expected root group, got {other:?}"),
    }
}

#[test]
fn rmov10_group_neighbor_wrap_shares_under_both_modes() {
    let k = key();
    let make_tree = || Node::Group {
        id: nid("root"),
        axis: Axis::Horizontal,
        children: vec![
            leaf("A"),
            leaf("B"),
            Node::Group {
                id: nid("V"),
                axis: Axis::Vertical,
                children: vec![leaf("C"), leaf("D")],
                shares: vec![1, 1],
            },
            leaf("E"),
        ],
        shares: vec![1, 2, 3, 4],
    };
    let leaves = [
        ("w1", "A"),
        ("w2", "B"),
        ("w3", "C"),
        ("w4", "D"),
        ("w5", "E"),
    ];
    let observed = vec![obs("w1"), obs("w2"), obs("w3"), obs("w4"), obs("w5")];

    for mode in [
        SameAxisMove::GroupWithNeighbor,
        SameAxisMove::SwapWithNeighbor,
    ] {
        let mut layout = WorldLayout::new();
        layout
            .domains
            .insert(k.clone(), base_domain(make_tree(), &leaves, "B"));
        let out = layout
            .local_move(
                &k,
                &win("w2"),
                Direction::Right,
                mode,
                &win("w2"),
                &observed,
            )
            .expect("group-neighbor wrap applies under both modes");
        assert_eq!(out.focus_leaf, nid("B"));
        match layout.tree(&k).expect("tree") {
            Node::Group {
                children, shares, ..
            } => {
                assert_eq!(shares, &[1, 5, 4]);
                assert_eq!(children.len(), 3);
                assert_eq!(children[0].id().0, "A");
                assert_eq!(children[2].id().0, "E");
                match &children[1] {
                    Node::Group {
                        children, shares, ..
                    } => {
                        assert_eq!(shares, &[1, 1]);
                        assert_eq!(children[0].id().0, "B");
                        match &children[1] {
                            Node::Group {
                                id,
                                children,
                                shares,
                                ..
                            } => {
                                assert_eq!(id.0, "V");
                                assert_eq!(shares, &[1, 1]);
                                let v_ids: Vec<&str> =
                                    children.iter().map(|c| c.id().0.as_str()).collect();
                                assert_eq!(v_ids, vec!["C", "D"]);
                            }
                            other => panic!("expected V group, got {other:?}"),
                        }
                    }
                    other => panic!("expected wrapper group, got {other:?}"),
                }
            }
            other => panic!("expected root group, got {other:?}"),
        }
    }
}

#[test]
fn alloc_move_wrap_mints_shared_serial() {
    use crate::directional::{OutputId, WorkspaceId};
    use crate::seed::EngineWindow;
    use crate::session::{ExceptionFlags, OutputDomain};
    use crate::size_hints::WindowSizeHints;
    use crate::world_layout::{PairMoveOutcome, PairOperationContext};
    use std::collections::{BTreeMap, BTreeSet};

    fn move_group_ids(node: &Node, out: &mut BTreeSet<NodeId>) {
        if let Node::Group { id, children, .. } = node {
            out.insert(id.clone());
            for child in children {
                move_group_ids(child, out);
            }
        }
    }

    fn all_move_groups(layout: &WorldLayout) -> BTreeSet<NodeId> {
        let mut out = BTreeSet::new();
        for domain in layout.domains.values() {
            if let Some(tree) = domain.tree.as_ref() {
                move_group_ids(tree, &mut out);
            }
        }
        out
    }

    let bounds1 = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let bounds2 = Rect {
        x: 120,
        y: 0,
        w: 120,
        h: 80,
    };
    let k1 = key();
    let k2 = other_key();

    let mut adj1 = BTreeMap::new();
    adj1.insert(Direction::Right, k2.output.clone());
    let d1 = OutputDomain {
        id: k1.output.clone(),
        workspace: k1.workspace.clone(),
        bounds: bounds1,
        gap: 0,
        adjacent: adj1,
    };
    let mut adj2 = BTreeMap::new();
    adj2.insert(Direction::Left, k1.output.clone());
    let d2 = OutputDomain {
        id: k2.output.clone(),
        workspace: k2.workspace.clone(),
        bounds: bounds2,
        gap: 0,
        adjacent: adj2,
    };

    let mut layout = WorldLayout::new();
    layout
        .domains
        .insert(k1.clone(), DomainLayout::new(bounds1, 0, 0));
    layout
        .domains
        .insert(k2.clone(), DomainLayout::new(bounds2, 0, 0));

    let obs_in = |name: &str, k: &DomainKey| ObservedWindow {
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
    for (i, name) in ["win-1", "win-2", "win-3"].iter().enumerate() {
        let list: Vec<ObservedWindow> = ["win-1", "win-2", "win-3"][..=i]
            .iter()
            .map(|n| obs_in(n, &k1))
            .collect();
        layout
            .admit_window(
                &k1,
                &win(name),
                ExceptionFlags::none(),
                None,
                bounds1,
                false,
                false,
                &list,
            )
            .expect("admit source");
    }
    layout
        .admit_window(
            &k2,
            &win("win-4"),
            ExceptionFlags::none(),
            None,
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            false,
            false,
            &[obs_in("win-4", &k2)],
        )
        .expect("admit dest");

    let pre_groups = all_move_groups(&layout);
    assert_eq!(pre_groups.len(), 2);
    let pre_next = layout.group_alloc.next_value();

    // Local R3 escape first, then R4 cross-output wrap.
    layout
        .local_move(
            &k1,
            &win("win-3"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("win-3"),
            &[
                obs_in("win-1", &k1),
                obs_in("win-2", &k1),
                obs_in("win-3", &k1),
            ],
        )
        .expect("local R3 move");

    let eng = |name: &str, k: &DomainKey| EngineWindow {
        window: win(name),
        output: OutputId(k.output.0.clone()),
        workspace: WorkspaceId(k.workspace.0.clone()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 40,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    };
    let observed_eng = vec![
        eng("win-1", &k1),
        eng("win-2", &k1),
        eng("win-3", &k1),
        eng("win-4", &k2),
    ];
    let focus_w = win("win-3");
    let ctx = PairOperationContext {
        source_domain: &d1,
        target_domain: &d2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus_w,
        observed: &observed_eng,
        closed: &[],
    };

    // Trial on a discarded clone first: burns serials that live never reissues.
    let mut trial = layout.clone();
    let trial_out = trial
        .run_directional_pair_move(
            &ctx,
            &win("win-3"),
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("trial R4 move");
    assert!(matches!(
        trial_out,
        PairMoveOutcome::Moved { cross: Some(_), .. }
    ));
    let trial_groups = all_move_groups(&trial);
    let trial_fresh: Vec<NodeId> = trial_groups.difference(&pre_groups).cloned().collect();
    assert!(!trial_fresh.is_empty());
    drop(trial);

    let trial_next = layout.group_alloc.next_value();
    assert!(trial_next > pre_next);

    let live_out = layout
        .run_directional_pair_move(
            &ctx,
            &win("win-3"),
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor,
        )
        .expect("live R4 move");
    assert!(matches!(
        live_out,
        PairMoveOutcome::Moved { cross: Some(_), .. }
    ));
    assert!(layout.group_alloc.next_value() > trial_next);

    let post_groups = all_move_groups(&layout);
    let fresh: Vec<NodeId> = post_groups.difference(&pre_groups).cloned().collect();
    assert!(!fresh.is_empty());
    for id in &fresh {
        let serial: u64 = id.0.strip_prefix("grp-").unwrap().parse().unwrap();
        assert!(serial >= trial_next);
        assert!(!trial_fresh.contains(id));
    }

    // Remove win-3 at destination (collapsing R4 wrapper), readmit at source,
    // and move right until R4 fires again: never reissues removed wrappers.
    let old_wraps = fresh;
    layout
        .converge_domain(
            &k2,
            &[obs_in("win-4", &k2)],
            Some(&win("win-4")),
            &BTreeSet::new(),
        )
        .expect("remove win-3 at dest");
    for id in &old_wraps {
        assert!(!all_move_groups(&layout).contains(id));
    }

    layout
        .admit_window(
            &k1,
            &win("win-3"),
            ExceptionFlags::none(),
            None,
            bounds1,
            false,
            false,
            &[
                obs_in("win-1", &k1),
                obs_in("win-2", &k1),
                obs_in("win-3", &k1),
            ],
        )
        .expect("readmit win-3");
    let before_repeat = all_move_groups(&layout);
    let mut wrapped = false;
    for _ in 0..3 {
        let step = layout
            .run_directional_pair_move(
                &ctx,
                &win("win-3"),
                Direction::Right,
                true,
                SameAxisMove::GroupWithNeighbor,
            )
            .expect("repeat move");
        if matches!(step, PairMoveOutcome::Moved { cross: Some(_), .. }) {
            wrapped = true;
            break;
        }
    }
    assert!(wrapped, "repeat move reaches R4 cross-output wrap");
    let post_repeat = all_move_groups(&layout);
    for id in &old_wraps {
        assert!(!post_repeat.contains(id));
    }
    let fresh_repeat: Vec<NodeId> = post_repeat.difference(&before_repeat).cloned().collect();
    assert!(!fresh_repeat.is_empty());
}
