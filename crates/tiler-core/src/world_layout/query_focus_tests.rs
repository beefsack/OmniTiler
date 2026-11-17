use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;

use super::state::{DomainLayout, WorldLayout};
use super::topology::validate_domain_topology;

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn leaf(id: &str) -> Node {
    Node::Leaf {
        id: NodeId(id.to_owned()),
    }
}

fn group(id: &str, leaves: &[&str]) -> Node {
    Node::Group {
        id: NodeId(id.to_owned()),
        axis: Axis::Horizontal,
        children: leaves.iter().map(|id| leaf(id)).collect(),
        shares: vec![1; leaves.len()],
    }
}

fn node(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn domain(tree: Option<Node>, leaves: &[(&str, &str)], history: Vec<NodeId>) -> DomainLayout {
    let mut layout = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        0,
        0,
    );
    layout.tree = tree;
    for (win, leaf_id) in leaves {
        layout.leaves.insert(window(win), node(leaf_id));
    }
    layout.focus_history = history;
    layout
}

#[test]
fn focus_resolves_matches_leaves_only_even_when_group_repeats_leaf_id() {
    let domain_key = key("out", "ws");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        domain_key.clone(),
        domain(
            Some(Node::Group {
                id: node("dup"),
                axis: Axis::Horizontal,
                children: vec![leaf("dup"), leaf("real")],
                shares: vec![1, 1],
            }),
            &[("w1", "dup"), ("w2", "real")],
            Vec::new(),
        ),
    );
    assert!(layout.focus_resolves(&domain_key, &node("dup")));
    assert!(layout.focus_resolves(&domain_key, &node("real")));

    let group_key = key("out2", "ws");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        group_key.clone(),
        domain(
            Some(group("only-group", &["a", "b"])),
            &[("w3", "a")],
            Vec::new(),
        ),
    );
    assert!(!layout.focus_resolves(&group_key, &node("only-group")));
    assert!(layout.focus_resolves(&group_key, &node("a")));
    assert_eq!(
        layout.window_for_leaf(&group_key, &node("a")),
        Some(window("w3"))
    );
}

#[test]
fn sync_focus_does_not_update_focus_history() {
    let domain_key = key("out", "ws");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        domain_key.clone(),
        domain(
            Some(group("g", &["l1", "l2", "l3"])),
            &[("w1", "l1"), ("w2", "l2"), ("w3", "l3")],
            Vec::new(),
        ),
    );
    assert!(layout.remember_focus(&domain_key, &window("w1")));
    assert!(layout.remember_focus(&domain_key, &window("w2")));
    assert!(layout.remember_focus(&domain_key, &window("w3")));
    let history = layout.domains[&domain_key].focus_history.clone();
    assert_eq!(history, vec![node("l1"), node("l2"), node("l3")]);

    assert!(layout.sync_focus_from_window(&domain_key, &window("w1")));
    assert_eq!(layout.focus_leaf(&domain_key), Some(&node("l1")));
    assert_eq!(
        layout.domains[&domain_key].focus_history, history,
        "sync_focus_from_window must not update or reorder focus_history"
    );
}

#[test]
fn focus_pruning_drops_stale_and_domain_mismatched_keeping_order_and_duplicates() {
    let one = key("out-1", "ws-1");
    let two = key("out-2", "ws-1");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        one.clone(),
        domain(
            Some(group("g1", &["a", "b"])),
            &[("w-a", "a"), ("w-b", "b")],
            Vec::new(),
        ),
    );
    layout.domains.insert(
        two.clone(),
        domain(Some(leaf("c")), &[("w-c", "c")], Vec::new()),
    );

    let base_one = vec![node("a"), node("b"), node("a"), node("gone"), node("c")];
    let base_two = vec![node("c"), node("a")];
    let expected_one = vec![node("a"), node("b"), node("a")];
    let expected_two = vec![node("c")];
    assert_eq!(
        layout.updated_focus_stack_on(&one, &base_one, None),
        expected_one
    );
    assert_eq!(
        layout.updated_focus_history_on(&one, &base_one, None),
        expected_one
    );
    assert_eq!(
        layout.updated_focus_stack_on(&two, &base_two, None),
        expected_two
    );
    assert_eq!(
        layout.updated_focus_history_on(&two, &base_two, None),
        expected_two
    );
    let retired = key("gone", "ws-1");
    assert!(
        layout
            .updated_focus_history_on(&retired, &base_one, Some(&node("a")))
            .is_empty()
    );
    assert_eq!(
        layout.updated_focus_stack_on(&retired, &base_one, Some(&node("a"))),
        vec![node("a")]
    );
}

#[test]
fn focus_stack_appends_unresolved_leaf_but_history_does_not() {
    let domain_key = key("out", "ws");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        domain_key.clone(),
        domain(
            Some(group("g", &["a", "b"])),
            &[("w-a", "a"), ("w-b", "b")],
            Vec::new(),
        ),
    );
    let base = vec![node("a"), node("b")];

    let resolving = node("a");
    let stack = layout.updated_focus_stack_on(&domain_key, &base, Some(&resolving));
    assert_eq!(stack, vec![node("b"), node("a")]);
    let history = layout.updated_focus_history_on(&domain_key, &base, Some(&resolving));
    assert_eq!(history, stack);

    let missing = node("gone");
    let stack = layout.updated_focus_stack_on(&domain_key, &base, Some(&missing));
    assert_eq!(stack, vec![node("a"), node("b"), missing]);
    let history = layout.updated_focus_history_on(&domain_key, &base, Some(&node("gone")));
    assert_eq!(history, base);
}

#[test]
fn validate_topology_rejects_malformed_tree() {
    let domain_key = key("out-1", "ws-1");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        domain_key.clone(),
        domain(
            Some(Node::Group {
                id: node("dup"),
                axis: Axis::Horizontal,
                children: vec![leaf("dup"), leaf("b")],
                shares: vec![1, 1],
            }),
            &[("w1", "dup"), ("w2", "b")],
            Vec::new(),
        ),
    );
    assert!(!validate_domain_topology(
        &domain_key,
        &layout.domains[&domain_key]
    ));

    let mut layout = WorldLayout::new();
    layout.domains.insert(
        domain_key.clone(),
        domain(
            Some(Node::Group {
                id: node("g"),
                axis: Axis::Horizontal,
                children: vec![leaf("dup"), leaf("b")],
                shares: vec![0, 1],
            }),
            &[("w1", "dup"), ("w2", "b")],
            Vec::new(),
        ),
    );
    assert!(!validate_domain_topology(
        &domain_key,
        &layout.domains[&domain_key]
    ));
}

#[test]
fn settle_focus_bookkeeping_demotes_selected_leaf_without_observed_history_reorder() {
    let one = key("out-1", "ws-1");
    let two = key("out-2", "ws-1");
    let mut layout = WorldLayout::new();
    let mut first = domain(
        Some(group("g", &["a", "b", "c"])),
        &[("w-a", "a"), ("w-b", "b"), ("w-c", "c")],
        vec![node("a"), node("gone"), node("b")],
    );
    first.focused_leaf = None;
    first.last_active = Some(node("b"));
    layout.domains.insert(one.clone(), first);

    let mut other = domain(Some(leaf("z")), &[("w-z", "z")], vec![node("z")]);
    other.focused_leaf = Some(node("z"));
    other.focus_stack = vec![node("z")];
    other.last_active = Some(node("z"));
    layout.domains.insert(two.clone(), other);

    let stack_base = vec![node("gone"), node("a"), node("b")];
    layout.settle_focus_bookkeeping(&one, &stack_base, Some(node("a")));

    let selected = &layout.domains[&one];
    assert_eq!(selected.focused_leaf, Some(node("a")));
    assert_eq!(
        selected.focus_stack,
        vec![node("b"), node("a")],
        "selected leaf must be demoted and re-appended after pruning"
    );
    assert_eq!(
        selected.focus_history,
        vec![node("a"), node("b")],
        "observed focus must not reorder newer accepted history"
    );
    assert_eq!(selected.last_active, Some(node("a")));

    let untouched = &layout.domains[&two];
    assert_eq!(untouched.focused_leaf, Some(node("z")));
    assert_eq!(untouched.focus_stack, vec![node("z")]);
    assert_eq!(untouched.focus_history, vec![node("z")]);
    assert_eq!(untouched.last_active, Some(node("z")));
}

#[test]
fn validates_memory_target_checks_only_the_named_domain() {
    let good = key("out-1", "ws-1");
    let malformed = key("out-2", "ws-1");
    let unknown = key("out-3", "ws-1");
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        good.clone(),
        domain(Some(leaf("a")), &[("w-a", "a")], Vec::new()),
    );
    layout.domains.insert(
        malformed.clone(),
        domain(
            Some(Node::Group {
                id: node("g"),
                axis: Axis::Horizontal,
                children: vec![leaf("x"), leaf("y")],
                shares: vec![0, 1],
            }),
            &[("w-x", "x"), ("w-y", "y")],
            Vec::new(),
        ),
    );

    assert!(layout.validates_memory_target(&good, &window("w-a")));
    assert!(!layout.validates_memory_target(&good, &window("missing")));
    assert!(!layout.validates_memory_target(&malformed, &window("w-x")));
    assert!(!layout.validates_memory_target(&unknown, &window("w-a")));
}

#[test]
fn sticky_query_mutators_are_revision_neutral_and_validate_exceptions() {
    use crate::session::ExceptionFlags;

    use super::state::{DomainException, DomainRestoreSlot};

    let domain_key = key("out", "ws");
    let w_float = window("w-float");
    let w_sticky = window("w-sticky");
    let w_min = window("w-min");
    let mut layout = WorldLayout::new();
    let mut dom = domain(Some(leaf("l1")), &[("w-tiled", "l1")], Vec::new());
    dom.accepted_revision = 7;
    dom.exceptions.insert(
        w_float.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                sticky: false,
                fullscreen: false,
                maximized: false,
            },
            floating_geometry: None,
        },
    );
    dom.exceptions.insert(
        w_sticky.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                sticky: true,
                fullscreen: false,
                maximized: false,
            },
            floating_geometry: None,
        },
    );
    dom.minimized_slots.insert(
        w_min.clone(),
        DomainRestoreSlot {
            leaf: node("l1"),
            tree: leaf("l1"),
        },
    );
    layout.domains.insert(domain_key.clone(), dom);

    let target = Rect {
        x: 10,
        y: 20,
        w: 300,
        h: 200,
    };
    let bad = Rect {
        x: 10,
        y: 20,
        w: 0,
        h: 200,
    };

    assert!(!layout.adopt_float_placement(&domain_key, &w_float, bad));
    assert!(!layout.adopt_float_placement(&domain_key, &w_sticky, target));
    assert!(!layout.adopt_float_placement(&domain_key, &window("missing"), target));
    assert_eq!(layout.retained_float_geometry(&domain_key, &w_float), None);

    assert!(layout.adopt_float_placement(&domain_key, &w_float, target));
    assert_eq!(
        layout.retained_float_geometry(&domain_key, &w_float),
        Some(target)
    );
    assert_eq!(
        layout
            .exception_record(&domain_key, &w_float)
            .and_then(|record| record.floating_geometry),
        Some(target)
    );

    assert!(layout.forget_minimized_slot(&domain_key, &w_min));
    assert!(!layout.forget_minimized_slot(&domain_key, &w_min));
    assert_eq!(layout.accepted_revision(&domain_key), 7);
}

#[test]
fn validate_topology_accepts_valid_and_rejects_invalid_domain_cases() {
    use crate::session::ExceptionFlags;

    use super::state::DomainException;

    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");

    let valid_layout = || {
        let mut layout = WorldLayout::new();
        layout.domains.insert(
            k1.clone(),
            domain(Some(leaf("a")), &[("win-a", "a")], Vec::new()),
        );
        layout.domains.insert(
            k2.clone(),
            domain(
                Some(group("g", &["b", "c"])),
                &[("win-b", "b"), ("win-c", "c")],
                Vec::new(),
            ),
        );
        layout
    };

    let mut layout = valid_layout();
    assert!(layout.validate_topology());
    layout.domains.get_mut(&k1).unwrap().focused_leaf = Some(node("a"));
    assert!(layout.validate_topology());
    layout.domains.get_mut(&k1).unwrap().focused_leaf = None;
    layout.domains.get_mut(&k2).unwrap().focused_leaf = Some(node("c"));
    assert!(layout.validate_topology());

    // Link leaf missing from domain's tree.
    let mut wrong_tree = valid_layout();
    wrong_tree
        .domains
        .get_mut(&k2)
        .unwrap()
        .leaves
        .insert(window("win-a"), node("a"));
    assert!(!wrong_tree.validate_topology());

    // Leaf in tree without a window link.
    let mut unlinked = valid_layout();
    unlinked
        .domains
        .get_mut(&k2)
        .unwrap()
        .leaves
        .remove(&window("win-c"));
    assert!(!unlinked.validate_topology());

    // Empty output/workspace key and empty window/leaf fields.
    let mut empty_key = valid_layout();
    empty_key.domains.insert(
        key("", "ws-1"),
        domain(Some(leaf("z")), &[("win-z", "z")], Vec::new()),
    );
    assert!(!empty_key.validate_topology());

    let mut empty_leaf = valid_layout();
    empty_leaf
        .domains
        .get_mut(&k1)
        .unwrap()
        .leaves
        .insert(window("win-empty"), node(""));
    assert!(!empty_leaf.validate_topology());

    // Window/exception overlap in the same domain.
    let mut overlap = valid_layout();
    overlap.domains.get_mut(&k1).unwrap().exceptions.insert(
        window("win-a"),
        DomainException {
            flags: ExceptionFlags::none(),
            floating_geometry: None,
        },
    );
    assert!(!overlap.validate_topology());
}
