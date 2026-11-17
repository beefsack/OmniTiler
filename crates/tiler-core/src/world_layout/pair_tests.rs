use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Axis, Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{ExceptionFlags, OutputDomain};

use super::pair::PairAssemblyFailure;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};

fn bounds_100() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    }
}

fn domain_desc(
    output: &str,
    workspace: &str,
    adjacent: BTreeMap<Direction, OutputId>,
) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: bounds_100(),
        gap: 4,
        adjacent,
    }
}

fn pair_domains_directional() -> (OutputDomain, OutputDomain) {
    let mut s_adj = BTreeMap::new();
    s_adj.insert(Direction::Right, OutputId("out-2".to_owned()));
    let mut t_adj = BTreeMap::new();
    t_adj.insert(Direction::Left, OutputId("out-1".to_owned()));
    (
        domain_desc("out-1", "ws-1", s_adj),
        domain_desc("out-2", "ws-1", t_adj),
    )
}

fn two_leaf_domain(
    group_id: &str,
    leaf1: (&str, &str),
    leaf2: (&str, &str),
    shares: [u64; 2],
    revision: u64,
    outer_gap: i32,
) -> DomainLayout {
    let mut layout = DomainLayout::new(bounds_100(), 4, outer_gap);
    let n1 = NodeId(leaf1.1.to_owned());
    let n2 = NodeId(leaf2.1.to_owned());
    layout.tree = Some(Node::Group {
        id: NodeId(group_id.to_owned()),
        axis: Axis::Horizontal,
        shares: shares.to_vec(),
        children: vec![Node::Leaf { id: n1.clone() }, Node::Leaf { id: n2.clone() }],
    });
    layout
        .leaves
        .insert(WindowId(leaf1.0.to_owned()), n1.clone());
    layout
        .leaves
        .insert(WindowId(leaf2.0.to_owned()), n2.clone());
    layout.focused_leaf = Some(n1.clone());
    layout.focus_stack = vec![n2.clone(), n1.clone()];
    layout.focus_history = vec![n2, n1.clone()];
    layout.last_active = Some(n1);
    layout.accepted_revision = revision;
    layout
}

fn single_leaf_domain(win: &str, leaf: &str, revision: u64, outer_gap: i32) -> DomainLayout {
    let mut layout = DomainLayout::new(bounds_100(), 4, outer_gap);
    let n = NodeId(leaf.to_owned());
    layout.tree = Some(Node::Leaf { id: n.clone() });
    layout.leaves.insert(WindowId(win.to_owned()), n.clone());
    layout.focused_leaf = Some(n.clone());
    layout.focus_stack = vec![n.clone()];
    layout.focus_history = vec![n.clone()];
    layout.last_active = Some(n);
    layout.accepted_revision = revision;
    layout
}

#[test]
fn roundtrip_preserves_exact_tree_shares_ids_revision_and_focus() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    // Both domains intentionally share identical local node IDs ("g-0", "l-1", "l-2").
    let s_orig = two_leaf_domain("g-0", ("s1", "l-1"), ("s2", "l-2"), [3, 7], 4, 8);
    let t_orig = two_leaf_domain("g-0", ("t1", "l-1"), ("t2", "l-2"), [2, 5], 11, 12);

    let mut world = WorldLayout::new();
    world.domains.insert(s_key.clone(), s_orig.clone());
    world.domains.insert(t_key.clone(), t_orig.clone());

    let pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    assert_eq!(pair.pair_revision, 11);
    assert_eq!(pair.focused_domain.as_ref(), Some(&s_key));
    assert_eq!(pair.focused_leaf, Some(NodeId("l-1".to_owned())));
    // Target node IDs are namespaced into pair-target-N avoiding source IDs.
    assert_ne!(pair.target.tree, t_orig.tree);
    assert!(
        pair.target
            .leaves
            .values()
            .all(|id| id.0.starts_with("pair-target-"))
    );

    let (split_s, split_t) = pair.split().expect("pair splits");
    let split_t = split_t.expect("target retained");

    assert_eq!(split_s.tree, s_orig.tree);
    assert_eq!(split_s.leaves, s_orig.leaves);
    assert_eq!(split_s.focused_leaf, s_orig.focused_leaf);
    assert_eq!(split_s.focus_stack, s_orig.focus_stack);
    assert_eq!(split_s.last_active, s_orig.last_active);
    assert_eq!(split_s.accepted_revision, 11);

    assert_eq!(split_t.tree, t_orig.tree);
    assert_eq!(split_t.leaves, t_orig.leaves);
    assert_eq!(split_t.focused_leaf, t_orig.focused_leaf);
    assert_eq!(split_t.focus_stack, t_orig.focus_stack);
    assert_eq!(split_t.last_active, t_orig.last_active);
    assert_eq!(split_t.accepted_revision, 11);
}

#[test]
fn target_collision_remaps_to_pair_moved_without_burning_allocator() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let s_orig = single_leaf_domain("s1", "leaf-a", 1, 0);
    let t_orig = single_leaf_domain("t1", "leaf-t", 2, 0);

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s_orig);
    world.domains.insert(t_key, t_orig);

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // Simulate a source leaf ("leaf-t") moved onto target during pair ops:
    // its ID is not in `target_restore` keys, and collides with restored target ID "leaf-t".
    let remapped_t1 = pair
        .target
        .leaves
        .get(&WindowId("t1".to_owned()))
        .unwrap()
        .clone();
    let moved_id = NodeId("leaf-t".to_owned());
    pair.target.tree = Some(Node::Group {
        id: NodeId("pair-group-0".to_owned()),
        axis: Axis::Horizontal,
        shares: vec![1, 1],
        children: vec![
            Node::Leaf {
                id: remapped_t1.clone(),
            },
            Node::Leaf {
                id: moved_id.clone(),
            },
        ],
    });
    pair.target
        .leaves
        .insert(WindowId("s-moved".to_owned()), moved_id);

    let (_split_s, split_t) = pair.split().expect("split succeeds");
    let split_t = split_t.expect("target retained");

    assert_eq!(
        split_t.leaves.get(&WindowId("t1".to_owned())),
        Some(&NodeId("leaf-t".to_owned()))
    );
    assert_eq!(
        split_t.leaves.get(&WindowId("s-moved".to_owned())),
        Some(&NodeId("pair-moved-0".to_owned()))
    );
    // Neither assembly nor split burned any group_alloc serial (next mint is still grp-0).
    assert_eq!(
        world.group_alloc.mint(&BTreeSet::new()),
        Some(NodeId("grp-0".to_owned()))
    );
}

#[test]
fn duplicate_guards_and_cross_category_permissions() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    // 1. Leaf vs exception across source & target -> DuplicateState.
    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key.clone(), single_leaf_domain("shared", "l-1", 1, 0));
    let mut t_exc = DomainLayout::new(bounds_100(), 4, 0);
    t_exc.exceptions.insert(
        WindowId("shared".to_owned()),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                ..ExceptionFlags::none()
            },
            floating_geometry: None,
        },
    );
    world.domains.insert(t_key.clone(), t_exc);
    assert_eq!(
        world
            .assemble_directional_pair(&s_desc, &t_desc)
            .expect_err("leaf vs exception"),
        PairAssemblyFailure::DuplicateState
    );

    // 2. Duplicate retained_float_geometry keys -> DuplicateState.
    let mut world = WorldLayout::new();
    let mut s = single_leaf_domain("s1", "l-1", 1, 0);
    s.retained_float_geometry
        .insert(WindowId("orphan-g".to_owned()), bounds_100());
    let mut t = single_leaf_domain("t1", "l-2", 1, 0);
    t.retained_float_geometry
        .insert(WindowId("orphan-g".to_owned()), bounds_100());
    world.domains.insert(s_key.clone(), s);
    world.domains.insert(t_key.clone(), t);
    assert_eq!(
        world
            .assemble_directional_pair(&s_desc, &t_desc)
            .expect_err("duplicate retained geometry"),
        PairAssemblyFailure::DuplicateState
    );

    // 3. Duplicate minimized_slots keys -> DuplicateState (checked before stale prune).
    let mut world = WorldLayout::new();
    let mut s = single_leaf_domain("s1", "l-1", 1, 0);
    s.minimized_slots.insert(
        WindowId("min-1".to_owned()),
        DomainRestoreSlot {
            leaf: NodeId("m-1".to_owned()),
            tree: Node::Leaf {
                id: NodeId("m-1".to_owned()),
            },
        },
    );
    let mut t = single_leaf_domain("t1", "l-2", 1, 0);
    t.minimized_slots.insert(
        WindowId("min-1".to_owned()),
        DomainRestoreSlot {
            leaf: NodeId("m-2".to_owned()),
            tree: Node::Leaf {
                id: NodeId("m-2".to_owned()),
            },
        },
    );
    world.domains.insert(s_key.clone(), s);
    world.domains.insert(t_key.clone(), t);
    assert_eq!(
        world
            .assemble_directional_pair(&s_desc, &t_desc)
            .expect_err("duplicate minimized slot"),
        PairAssemblyFailure::DuplicateState
    );

    // 4. Cross-category overlaps allowed:
    //    - retained_float_geometry in source + leaf in target: allowed.
    //    - minimized_slot in source + leaf in target: allowed, and stale slot is dropped!
    let mut world = WorldLayout::new();
    let mut s = single_leaf_domain("s1", "l-1", 1, 0);
    s.retained_float_geometry
        .insert(WindowId("t1".to_owned()), bounds_100());
    s.minimized_slots.insert(
        WindowId("t1".to_owned()),
        DomainRestoreSlot {
            leaf: NodeId("m-1".to_owned()),
            tree: Node::Leaf {
                id: NodeId("m-1".to_owned()),
            },
        },
    );
    let t = single_leaf_domain("t1", "l-2", 1, 0);
    world.domains.insert(s_key, s);
    world.domains.insert(t_key, t);
    let pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("cross-category allowed");
    assert!(
        !pair
            .source
            .minimized_slots
            .contains_key(&WindowId("t1".to_owned()))
    );
}

#[test]
fn error_priorities_and_adjacency_modes() {
    let (s_desc, t_desc) = pair_domains_directional();
    let mut world = WorldLayout::new();

    // Missing source -> SourceUnavailable even if descriptors have no adjacency.
    let no_adj_s = domain_desc("out-1", "ws-1", BTreeMap::new());
    let no_adj_t = domain_desc("out-2", "ws-1", BTreeMap::new());
    assert_eq!(
        world
            .assemble_directional_pair(&no_adj_s, &no_adj_t)
            .expect_err("missing source"),
        PairAssemblyFailure::SourceUnavailable
    );

    // Present unusable (empty) target -> TargetUnusable before DomainMismatch.
    world
        .domains
        .insert(s_desc.key(), single_leaf_domain("s1", "l-1", 1, 0));
    world
        .domains
        .insert(t_desc.key(), DomainLayout::new(bounds_100(), 4, 0));
    assert_eq!(
        world
            .assemble_directional_pair(&no_adj_s, &no_adj_t)
            .expect_err("unusable target"),
        PairAssemblyFailure::TargetUnusable
    );

    // Valid source + absent target + missing reciprocal adjacency:
    // directional fails with DomainMismatch, explicit send succeeds.
    world.domains.remove(&t_desc.key());
    assert_eq!(
        world
            .assemble_directional_pair(&no_adj_s, &no_adj_t)
            .expect_err("directional requires reciprocal adjacency"),
        PairAssemblyFailure::DomainMismatch
    );
    world
        .assemble_send_pair(&no_adj_s, &no_adj_t)
        .expect("send skips cross-output reciprocal adjacency");

    // Explicit send STILL validates adjacency structure (non-reciprocal edge fails).
    let mut one_way_adj = BTreeMap::new();
    one_way_adj.insert(Direction::Right, OutputId("out-2".to_owned()));
    let one_way_s = domain_desc("out-1", "ws-1", one_way_adj);
    assert_eq!(
        world
            .assemble_send_pair(&one_way_s, &no_adj_t)
            .expect_err("send validates adjacency structure"),
        PairAssemblyFailure::DomainMismatch
    );
}

#[test]
fn source_priority_admission_order_and_target_orphan_transfer_to_source() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut s = single_leaf_domain("s1", "l-1", 1, 0);
    s.admission_order.insert(WindowId("dup".to_owned()), 10);
    s.minimized_slots.insert(
        WindowId("s-slot".to_owned()),
        DomainRestoreSlot {
            leaf: NodeId("sm".to_owned()),
            tree: Node::Leaf {
                id: NodeId("sm".to_owned()),
            },
        },
    );
    s.admission_order.insert(WindowId("s-slot".to_owned()), 11);

    let mut t = single_leaf_domain("t1", "l-2", 1, 0);
    t.admission_order.insert(WindowId("dup".to_owned()), 99);
    t.retained_float_geometry
        .insert(WindowId("t-orphan".to_owned()), bounds_100());
    t.automatic_fixed.insert(WindowId("t-orphan".to_owned()));
    t.fixed_tile_override
        .insert(WindowId("t-orphan".to_owned()));
    t.capacity_floats.insert(WindowId("t-orphan".to_owned()));
    t.admission_order
        .insert(WindowId("t-orphan".to_owned()), 42);
    t.minimized_slots.insert(
        WindowId("t-slot".to_owned()),
        DomainRestoreSlot {
            leaf: NodeId("tm".to_owned()),
            tree: Node::Leaf {
                id: NodeId("tm".to_owned()),
            },
        },
    );
    t.admission_order.insert(WindowId("t-slot".to_owned()), 55);

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s);
    world.domains.insert(t_key, t);

    let pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair");
    let (split_s, split_t) = pair.split().expect("split");
    let split_t = split_t.expect("target present");

    // Source admission_order wins duplicate key and receives all orphans (including from target).
    assert_eq!(
        split_s.admission_order.get(&WindowId("dup".to_owned())),
        Some(&10)
    );
    assert!(
        !split_t
            .admission_order
            .contains_key(&WindowId("dup".to_owned()))
    );

    assert!(
        split_s
            .retained_float_geometry
            .contains_key(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        split_s
            .automatic_fixed
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        split_s
            .fixed_tile_override
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        split_s
            .capacity_floats
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert_eq!(
        split_s
            .admission_order
            .get(&WindowId("t-orphan".to_owned())),
        Some(&42)
    );

    assert!(
        !split_t
            .retained_float_geometry
            .contains_key(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        !split_t
            .automatic_fixed
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        !split_t
            .fixed_tile_override
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        !split_t
            .capacity_floats
            .contains(&WindowId("t-orphan".to_owned()))
    );
    assert!(
        !split_t
            .admission_order
            .contains_key(&WindowId("t-orphan".to_owned()))
    );

    // Minimized slots keep their admission_order homed with the slot's domain.
    assert_eq!(
        split_s.admission_order.get(&WindowId("s-slot".to_owned())),
        Some(&11)
    );
    assert_eq!(
        split_t.admission_order.get(&WindowId("t-slot".to_owned())),
        Some(&55)
    );
}

#[test]
fn stale_mru_and_last_active_preserved_while_focus_history_pruned_and_final_stack_top_only() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let s = single_leaf_domain("s1", "l-1", 1, 0);
    let mut t = single_leaf_domain("t1", "l-2", 1, 0);
    // Put a valid leaf followed by a stale leaf at the top of target's focus_stack,
    // and a stale leaf in target's focus_history and last_active.
    let stale = NodeId("stale-leaf".to_owned());
    t.focus_stack = vec![NodeId("l-2".to_owned()), stale.clone()];
    t.focus_history = vec![stale.clone(), NodeId("l-2".to_owned())];
    t.last_active = Some(stale.clone());

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s);
    world.domains.insert(t_key, t);

    let pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair");
    let (_split_s, split_t) = pair.split().expect("split");
    let split_t = split_t.expect("target present");

    // Stale MRU and last_active are preserved verbatim; focus_history is pruned.
    assert_eq!(
        split_t.focus_stack,
        vec![NodeId("l-2".to_owned()), stale.clone()]
    );
    assert_eq!(split_t.last_active, Some(stale));
    assert_eq!(split_t.focus_history, vec![NodeId("l-2".to_owned())]);
    // Because final stack top is stale, target_focus is None (NO reverse-find).
    assert_eq!(split_t.focused_leaf, None);
}

#[test]
fn absent_empty_retirement_gap_inheritance_atomic_refusal_and_non_consuming_install() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    // 1. Absent target inherits source_outer_gap and retires if still empty.
    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key.clone(), single_leaf_domain("s1", "l-1", 1, 6));
    world.admission_seq_next = 5;

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair with absent target");
    pair.admission_seq_next = 9;
    world.install_pair(&pair, 10, None).expect("installs");
    assert_eq!(world.domain(&s_key).unwrap().outer_gap, 10);
    assert!(world.domain(&t_key).is_none());
    assert_eq!(world.admission_seq_next, 9);

    // 2. Non-consuming install keeps pair working view intact, and negative gap refuses atomically.
    assert_eq!(
        world
            .install_pair(&pair, -1, None)
            .expect_err("negative outer gap refuses"),
        PairAssemblyFailure::DomainMismatch
    );
    assert_eq!(world.domain(&s_key).unwrap().outer_gap, 10);

    // 3. Existing target retains its own outer_gap unless explicitly overridden.
    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key.clone(), single_leaf_domain("s1", "l-1", 1, 6));
    world
        .domains
        .insert(t_key.clone(), single_leaf_domain("t1", "l-2", 2, 14));
    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair with present target");
    let before_target_ids: BTreeSet<_> = pair.target.leaves.values().cloned().collect();
    pair.admission_seq_next = 12;
    world.install_pair(&pair, 20, None).expect("installs");
    assert_eq!(world.domain(&s_key).unwrap().outer_gap, 20);
    assert_eq!(world.domain(&t_key).unwrap().outer_gap, 14);
    assert_eq!(world.admission_seq_next, 12);
    // Working view still holds the same pair-target-N IDs and target_restore.
    let after_target_ids: BTreeSet<_> = pair.target.leaves.values().cloned().collect();
    assert_eq!(before_target_ids, after_target_ids);
    assert!(!pair.target_restore.is_empty());

    // Explicit target outer_gap override applies on install.
    world
        .install_pair(&pair, 20, Some(30))
        .expect("override installs");
    assert_eq!(world.domain(&t_key).unwrap().outer_gap, 30);
}

#[test]
fn valid_global_target_focus_with_stale_embedded_source_focus_splits_with_stack_top_fallback() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let s = two_leaf_domain("g-s", ("s1", "l-1"), ("s2", "l-2"), [1, 1], 1, 0);
    let t = single_leaf_domain("t1", "l-t", 2, 0);

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s);
    world.domains.insert(t_key.clone(), t);

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // Simulate pair edit: remove l-1 from source leaving stale embedded source.focused_leaf = Some("l-1"),
    // update source.focus_stack top to resolving "l-2", and move pair-global focus to target.
    pair.source.tree = Some(Node::Leaf {
        id: NodeId("l-2".to_owned()),
    });
    pair.source.leaves.remove(&WindowId("s1".to_owned()));
    pair.source.focus_stack = vec![NodeId("l-1".to_owned()), NodeId("l-2".to_owned())];
    assert_eq!(pair.source.focused_leaf, Some(NodeId("l-1".to_owned())));

    let t_leaf_pair = pair
        .target
        .leaves
        .get(&WindowId("t1".to_owned()))
        .unwrap()
        .clone();
    pair.focused_domain = Some(t_key);
    pair.focused_leaf = Some(t_leaf_pair);

    let (split_s, split_t) = pair
        .split()
        .expect("stale embedded focus does not block split");
    let split_t = split_t.expect("target retained");
    assert_eq!(split_s.focused_leaf, Some(NodeId("l-2".to_owned())));
    assert_eq!(split_t.focused_leaf, Some(NodeId("l-t".to_owned())));
}

#[test]
fn invalid_half_or_unknown_global_focus_rejects_atomically() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key.clone(), single_leaf_domain("s1", "l-1", 1, 6));
    world
        .domains
        .insert(t_key.clone(), single_leaf_domain("t1", "l-2", 2, 8));
    world.admission_seq_next = 3;

    let base_pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // Half-populated (Some domain, None leaf)
    let mut half_domain = base_pair.clone();
    half_domain.focused_leaf = None;
    half_domain.admission_seq_next = 99;
    assert_eq!(
        world
            .install_pair(&half_domain, 10, None)
            .expect_err("half global focus refuses"),
        PairAssemblyFailure::TargetUnusable
    );

    // Half-populated (None domain, Some leaf)
    let mut half_leaf = base_pair.clone();
    half_leaf.focused_domain = None;
    half_leaf.admission_seq_next = 99;
    assert_eq!(
        world
            .install_pair(&half_leaf, 10, None)
            .expect_err("half global leaf refuses"),
        PairAssemblyFailure::TargetUnusable
    );

    // Unknown domain key
    let mut unknown_domain = base_pair.clone();
    unknown_domain.focused_domain = Some(crate::session::DomainKey {
        output: OutputId("out-99".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    });
    assert_eq!(
        world
            .install_pair(&unknown_domain, 10, None)
            .expect_err("unknown focused domain refuses"),
        PairAssemblyFailure::TargetUnusable
    );

    // Unresolvable global leaf
    let mut bad_leaf = base_pair;
    bad_leaf.focused_leaf = Some(NodeId("missing-leaf".to_owned()));
    assert_eq!(
        world
            .install_pair(&bad_leaf, 10, None)
            .expect_err("unresolvable global leaf refuses"),
        PairAssemblyFailure::TargetUnusable
    );

    // Retained state and high-water are completely untouched.
    assert_eq!(world.domain(&s_key).unwrap().outer_gap, 6);
    assert_eq!(world.domain(&t_key).unwrap().outer_gap, 8);
    assert_eq!(world.admission_seq_next, 3);
}

#[test]
fn duplicate_linked_leaf_across_domains_rejects_while_duplicate_group_id_is_allowed() {
    let (s_desc, t_desc) = pair_domains_directional();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world.domains.insert(
        s_key,
        two_leaf_domain("g-shared", ("s1", "l-1"), ("s2", "l-2"), [1, 1], 1, 0),
    );
    world.domains.insert(
        t_key,
        two_leaf_domain("g-shared", ("t1", "l-3"), ("t2", "l-4"), [1, 1], 2, 0),
    );

    let base_pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // Duplicate GROUP ID across source and target (trial edit) is allowed when within-domain unique.
    let mut shared_group_pair = base_pair.clone();
    if let Some(Node::Group { id, .. }) = shared_group_pair.target.tree.as_mut() {
        *id = NodeId("g-shared".to_owned());
    }
    assert!(shared_group_pair.validate_pair_topology());
    shared_group_pair
        .split()
        .expect("duplicate group ID across pair domains splits cleanly");

    // Duplicate linked LEAF ID across source and target rejects.
    let mut dup_leaf_pair = base_pair;
    let dup_leaf = NodeId("l-1".to_owned());
    if let Some(Node::Group { children, .. }) = dup_leaf_pair.target.tree.as_mut() {
        children[0] = Node::Leaf {
            id: dup_leaf.clone(),
        };
    }
    dup_leaf_pair
        .target
        .leaves
        .insert(WindowId("t1".to_owned()), dup_leaf);
    assert!(!dup_leaf_pair.validate_pair_topology());
    assert_eq!(
        dup_leaf_pair
            .split()
            .expect_err("duplicate linked leaf across domains refuses"),
        PairAssemblyFailure::TargetUnusable
    );
}
