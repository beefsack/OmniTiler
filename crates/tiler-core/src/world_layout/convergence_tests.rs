use std::collections::BTreeSet;

use crate::contract::MAX_REVISION;
use crate::directional::{Node, NodeId, OutputId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{ExceptionFlags, ObservedWindow};

use super::*;

fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    }
}

fn leaf_node(id: &str) -> Node {
    Node::Leaf {
        id: NodeId(id.to_owned()),
    }
}

fn observed_window(id: &str, floating: bool) -> ObservedWindow {
    let key = key();
    ObservedWindow {
        window: WindowId(id.to_owned()),
        output: key.output,
        workspace: key.workspace,
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

fn tiled_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        0,
        0,
    );
    domain.tree = Some(leaf_node("a"));
    domain
        .leaves
        .insert(WindowId("w1".to_owned()), NodeId("a".to_owned()));
    domain.focus_history = vec![NodeId("a".to_owned())];
    layout.domains.insert(key(), domain);
    layout
}

fn float_bit_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        0,
        0,
    );
    domain.exceptions.insert(
        WindowId("w1".to_owned()),
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
    domain.automatic_fixed.insert(WindowId("w1".to_owned()));
    domain.fixed_tile_override.insert(WindowId("w1".to_owned()));
    layout.domains.insert(key(), domain);
    layout
}

#[test]
fn focus_only_exact_observation_keeps_revision_highwater_and_history() {
    let mut layout = tiled_layout();
    let key = key();
    layout.domains.get_mut(&key).unwrap().accepted_revision = 5;
    let observed = vec![observed_window("w1", false)];
    let focus = WindowId("w1".to_owned());
    assert!(
        layout
            .converge_domain(&key, &observed, Some(&focus), &BTreeSet::new())
            .is_ok()
    );
    let domain = layout.domain(&key).unwrap();
    assert_eq!(domain.accepted_revision, 5);
    assert_eq!(layout.admission_seq_next, 0);
    assert_eq!(domain.focus_history, vec![NodeId("a".to_owned())]);
    assert_eq!(domain.focused_leaf.as_ref(), Some(&NodeId("a".to_owned())));
}

#[test]
fn revision_limit_refusal_keeps_state_but_burns_allocator() {
    let mut layout = tiled_layout();
    let key = key();
    layout.domains.get_mut(&key).unwrap().accepted_revision = MAX_REVISION;
    let observed = vec![observed_window("w1", false), observed_window("w2", false)];
    assert_eq!(
        layout.converge_domain(&key, &observed, None, &BTreeSet::new()),
        Err(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );
    let domain = layout.domain(&key).unwrap();
    assert_eq!(domain.accepted_revision, MAX_REVISION);
    assert_eq!(domain.tree, Some(leaf_node("a")));
    assert!(!domain.leaves.contains_key(&WindowId("w2".to_owned())));
    assert_eq!(layout.admission_seq_next, 0);
    assert!(layout.group_alloc.next_value() >= 1);
}

#[test]
fn hidden_vs_absent_automatic_override_retention() {
    let key = key();
    let w1 = WindowId("w1".to_owned());

    let mut with_hidden = float_bit_layout();
    let hidden = BTreeSet::from([w1.clone()]);
    assert!(
        with_hidden
            .converge_domain(&key, &[], None, &hidden)
            .is_ok()
    );
    let domain = with_hidden.domain(&key).unwrap();
    assert!(domain.exceptions.is_empty());
    assert!(domain.automatic_fixed.contains(&w1));
    assert!(domain.fixed_tile_override.contains(&w1));

    let mut without_hidden = float_bit_layout();
    assert!(
        without_hidden
            .converge_domain(&key, &[], None, &BTreeSet::new())
            .is_ok()
    );
    let domain = without_hidden.domain(&key).unwrap();
    assert!(domain.exceptions.is_empty());
    assert!(!domain.automatic_fixed.contains(&w1));
    assert!(!domain.fixed_tile_override.contains(&w1));
}

fn engine_window(id: &str) -> EngineWindow {
    let key = key();
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: key.output,
        workspace: key.workspace,
        rect: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}

#[test]
fn converge_retained_observation_cleans_closed_slots_and_preserves_hidden_lifetime() {
    use crate::directional::Axis;
    let key = key();
    let w1 = WindowId("w1".to_owned());
    let w_tiled_closed = WindowId("w-tiled-closed".to_owned());
    let w_closed = WindowId("w-closed".to_owned());
    let w_hidden = WindowId("w-hidden".to_owned());
    let mut layout = tiled_layout();
    {
        let domain = layout.domains.get_mut(&key).unwrap();
        domain.accepted_revision = 4;
        domain.tree = Some(Node::Group {
            id: NodeId("g1".to_owned()),
            axis: Axis::Horizontal,
            children: vec![leaf_node("a"), leaf_node("d")],
            shares: vec![1, 1],
        });
        domain
            .leaves
            .insert(w_tiled_closed.clone(), NodeId("d".to_owned()));
        domain.minimized_slots.insert(
            w_closed.clone(),
            DomainRestoreSlot {
                leaf: NodeId("b".to_owned()),
                tree: leaf_node("a"),
            },
        );
        domain.minimized_slots.insert(
            w_hidden.clone(),
            DomainRestoreSlot {
                leaf: NodeId("c".to_owned()),
                tree: leaf_node("a"),
            },
        );
        domain.automatic_fixed.insert(w_closed.clone());
        domain.fixed_tile_override.insert(w_closed.clone());
        domain.automatic_fixed.insert(w_hidden.clone());
        domain.fixed_tile_override.insert(w_hidden.clone());
    }

    let windows = vec![engine_window("w1")];
    let closed = vec![w_closed.clone(), w_tiled_closed.clone()];
    let result = layout.converge_retained_observation(&key, &windows, &w1, &closed);
    assert!(result.unwrap().is_some());

    let domain = layout.domain(&key).unwrap();
    assert!(!domain.leaves.contains_key(&w_tiled_closed));
    assert!(!domain.minimized_slots.contains_key(&w_tiled_closed));
    assert!(!domain.minimized_slots.contains_key(&w_closed));
    assert!(!domain.automatic_fixed.contains(&w_closed));
    assert!(!domain.fixed_tile_override.contains(&w_closed));
    assert!(domain.minimized_slots.contains_key(&w_hidden));
    assert!(domain.automatic_fixed.contains(&w_hidden));
    assert!(domain.fixed_tile_override.contains(&w_hidden));
    assert_eq!(domain.accepted_revision, 5);
}

#[test]
fn converge_retained_observation_refusal_keeps_slots_and_state_atomic() {
    let key = key();
    let w_closed = WindowId("w-closed".to_owned());
    let w2 = WindowId("w2".to_owned());
    let mut layout = tiled_layout();
    {
        let domain = layout.domains.get_mut(&key).unwrap();
        domain.accepted_revision = MAX_REVISION;
        domain.minimized_slots.insert(
            w_closed.clone(),
            DomainRestoreSlot {
                leaf: NodeId("b".to_owned()),
                tree: leaf_node("a"),
            },
        );
    }

    let windows = vec![engine_window("w1"), engine_window("w2")];
    let closed = vec![w_closed.clone()];
    let empty_focus = WindowId(String::new());
    assert_eq!(
        layout.converge_retained_observation(&key, &windows, &empty_focus, &closed),
        Err(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );

    let domain = layout.domain(&key).unwrap();
    assert!(domain.minimized_slots.contains_key(&w_closed));
    assert_eq!(domain.accepted_revision, MAX_REVISION);
    assert!(!domain.leaves.contains_key(&w2));
}

fn two_tile_layout() -> WorldLayout {
    use crate::directional::Axis;
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        0,
        0,
    );
    domain.tree = Some(Node::Group {
        id: NodeId("g-1".to_owned()),
        axis: Axis::Horizontal,
        children: vec![leaf_node("leaf-win-1"), leaf_node("leaf-win-2")],
        shares: vec![1, 1],
    });
    domain.leaves.insert(
        WindowId("win-1".to_owned()),
        NodeId("leaf-win-1".to_owned()),
    );
    domain.leaves.insert(
        WindowId("win-2".to_owned()),
        NodeId("leaf-win-2".to_owned()),
    );
    domain.focused_leaf = Some(NodeId("leaf-win-2".to_owned()));
    domain.focus_stack = vec![
        NodeId("leaf-win-1".to_owned()),
        NodeId("leaf-win-2".to_owned()),
    ];
    domain.focus_history = vec![
        NodeId("leaf-win-1".to_owned()),
        NodeId("leaf-win-2".to_owned()),
    ];
    domain.last_active = Some(NodeId("leaf-win-2".to_owned()));
    layout.domains.insert(key(), domain);
    layout
}

fn anonymize_tree(node: &Node) -> Node {
    match node {
        Node::Leaf { id } => Node::Leaf { id: id.clone() },
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => Node::Group {
            id: NodeId("g".to_owned()),
            axis: *axis,
            children: children.iter().map(anonymize_tree).collect(),
            shares: shares.clone(),
        },
    }
}

#[test]
fn converge_observation_skew_missing_new_and_exact() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("win-1".to_owned());
    let w2 = WindowId("win-2".to_owned());

    // Floating skew: retained tiled win-1 observed floating.
    let base = layout.domain(&k).unwrap().accepted_revision;
    let summary = layout
        .converge_domain(
            &k,
            &[
                observed_window("win-1", true),
                observed_window("win-2", false),
            ],
            Some(&w2),
            &BTreeSet::new(),
        )
        .expect("skew converges");
    assert_eq!(
        (summary.removed, summary.admitted, summary.flags_adopted),
        (0, 0, 1)
    );
    let domain = layout.domain(&k).unwrap();
    assert_eq!(domain.accepted_revision, base + 1);
    assert!(domain.exceptions.contains_key(&w1));
    assert!(!domain.leaves.contains_key(&w1));
    assert!(domain.leaves.contains_key(&w2));
    assert_eq!(domain.focused_leaf, Some(NodeId("leaf-win-2".to_owned())));

    // Missing exception (win-1) plus brand-new normal (win-3) in one observation.
    let base = domain.accepted_revision;
    let summary = layout
        .converge_domain(
            &k,
            &[
                observed_window("win-2", false),
                observed_window("win-3", false),
            ],
            Some(&w2),
            &BTreeSet::new(),
        )
        .expect("missing/new converges");
    assert_eq!(
        (summary.removed, summary.admitted, summary.flags_adopted),
        (1, 1, 0)
    );
    let domain = layout.domain(&k).unwrap();
    assert_eq!(domain.accepted_revision, base + 1);
    assert!(!domain.exceptions.contains_key(&w1));
    assert!(domain.leaves.contains_key(&w2));
    assert!(domain.leaves.contains_key(&WindowId("win-3".to_owned())));
    assert_eq!(domain.focused_leaf, Some(NodeId("leaf-win-2".to_owned())));

    // Exact match advances nothing.
    let base = domain.accepted_revision;
    let summary = layout
        .converge_domain(
            &k,
            &[
                observed_window("win-2", false),
                observed_window("win-3", false),
            ],
            Some(&w2),
            &BTreeSet::new(),
        )
        .expect("exact converges");
    assert_eq!(
        (summary.removed, summary.admitted, summary.flags_adopted),
        (0, 0, 0)
    );
    assert_eq!(layout.domain(&k).unwrap().accepted_revision, base);
}

#[test]
fn converge_newcomers_match_sequential_seed_admits() {
    use crate::session::OutputDomain;

    let k = key();
    let mut converged = two_tile_layout();
    let base = converged.domain(&k).unwrap().accepted_revision;
    let obs_four = vec![
        observed_window("win-1", false),
        observed_window("win-2", false),
        observed_window("win-3", false),
        observed_window("win-4", false),
    ];
    let w4 = WindowId("win-4".to_owned());
    let summary = converged
        .converge_domain(&k, &obs_four, Some(&w4), &BTreeSet::new())
        .expect("newcomers converge");
    assert_eq!(
        (summary.removed, summary.admitted, summary.flags_adopted),
        (0, 2, 0)
    );
    assert_eq!(converged.domain(&k).unwrap().accepted_revision, base + 1);

    let mut sequential = two_tile_layout();
    let mut seq_obs = vec![
        observed_window("win-1", false),
        observed_window("win-2", false),
    ];
    for name in ["win-3", "win-4"] {
        seq_obs.push(observed_window(name, false));
        let dom = sequential.domain(&k).unwrap();
        let focused = dom.focused_leaf.clone();
        let bounds = focused
            .and_then(|leaf_id| {
                sequential
                    .project_domain_geometry(&k, &Default::default())
                    .and_then(|proj| proj.into_iter().find(|g| g.leaf == leaf_id).map(|g| g.rect))
            })
            .unwrap_or(dom.bounds);
        sequential
            .admit_window(
                &k,
                &WindowId(name.to_owned()),
                bounds,
                false,
                false,
                &seq_obs,
            )
            .expect("sequential admit");
    }

    let c_dom = converged.domain(&k).unwrap();
    let s_dom = sequential.domain(&k).unwrap();
    assert_eq!(c_dom.leaves, s_dom.leaves);
    assert_eq!(
        anonymize_tree(c_dom.tree.as_ref().unwrap()),
        anonymize_tree(s_dom.tree.as_ref().unwrap())
    );
    assert_eq!(c_dom.focused_leaf, s_dom.focused_leaf);

    // Focus naming a removed window falls back to surviving MRU focus.
    let mut fallback = two_tile_layout();
    let w2 = WindowId("win-2".to_owned());
    let summary = fallback
        .converge_domain(
            &k,
            &[observed_window("win-1", false)],
            Some(&w2),
            &BTreeSet::new(),
        )
        .expect("removed focus converges");
    assert_eq!(
        (summary.removed, summary.admitted, summary.flags_adopted),
        (1, 0, 0)
    );
    assert_eq!(
        fallback.domain(&k).unwrap().focused_leaf,
        Some(NodeId("leaf-win-1".to_owned()))
    );

    // Empty fresh domain plus empty observation is an exact no-op.
    let mut empty_layout = WorldLayout::new();
    let fresh_domain = OutputDomain {
        id: k.output.clone(),
        workspace: k.workspace.clone(),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        gap: 0,
        adjacent: Default::default(),
    };
    let empty_summary = empty_layout
        .converge_fresh_domain(&fresh_domain, 0, &[], None)
        .expect("empty exact");
    assert_eq!(
        (
            empty_summary.removed,
            empty_summary.admitted,
            empty_summary.flags_adopted
        ),
        (0, 0, 0)
    );
    assert!(empty_layout.domains.is_empty());
}

#[test]
fn converge_refuses_cross_homing() {
    let k1 = key();
    let k2 = DomainKey {
        output: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    };
    let mut layout = WorldLayout::new();
    let mut d1 = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        0,
        0,
    );
    d1.tree = Some(leaf_node("leaf-a"));
    d1.leaves
        .insert(WindowId("win-a".to_owned()), NodeId("leaf-a".to_owned()));
    d1.focused_leaf = Some(NodeId("leaf-a".to_owned()));
    layout.domains.insert(k1.clone(), d1);
    layout.domains.insert(
        k2.clone(),
        DomainLayout::new(
            Rect {
                x: 120,
                y: 0,
                w: 120,
                h: 80,
            },
            0,
            0,
        ),
    );

    let mut moved = observed_window("win-a", false);
    moved.output = k2.output.clone();
    let err = layout
        .converge_domain(&k1, &[moved], None, &BTreeSet::new())
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)
    );
    let d1_after = layout.domain(&k1).unwrap();
    assert_eq!(d1_after.accepted_revision, 0);
    assert!(d1_after.leaves.contains_key(&WindowId("win-a".to_owned())));
}

fn conv_group_ids(node: &Node, out: &mut BTreeSet<NodeId>) {
    if let Node::Group { id, children, .. } = node {
        out.insert(id.clone());
        for child in children {
            conv_group_ids(child, out);
        }
    }
}

fn domain_conv_group_ids(layout: &WorldLayout, k: &DomainKey) -> BTreeSet<NodeId> {
    let mut ids = BTreeSet::new();
    if let Some(tree) = layout.tree(k) {
        conv_group_ids(tree, &mut ids);
    }
    ids
}

#[test]
fn alloc_restore_rejoin_mints_fresh_never_reissues() {
    let k = key();
    let mut layout = WorldLayout::new();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    layout
        .domains
        .insert(k.clone(), DomainLayout::new(bounds, 0, 0));

    let obs_list = |names: &[&str]| -> Vec<ObservedWindow> {
        names
            .iter()
            .map(|name| observed_window(name, false))
            .collect()
    };

    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a"]),
            Some(&WindowId("win-a".to_owned())),
            &BTreeSet::new(),
        )
        .expect("converge a");
    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a", "win-b"]),
            Some(&WindowId("win-b".to_owned())),
            &BTreeSet::new(),
        )
        .expect("converge ab");
    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a", "win-b", "win-c"]),
            Some(&WindowId("win-c".to_owned())),
            &BTreeSet::new(),
        )
        .expect("converge abc");

    let seeded_groups = domain_conv_group_ids(&layout, &k);
    assert_eq!(seeded_groups.len(), 2);
    let seeded_next = layout.group_alloc.next_value();

    // Omit win-b: bare absence stores a restore slot.
    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a", "win-c"]),
            Some(&WindowId("win-c".to_owned())),
            &BTreeSet::new(),
        )
        .expect("omit win-b");
    assert!(
        layout
            .domain(&k)
            .unwrap()
            .minimized_slots
            .contains_key(&WindowId("win-b".to_owned())),
        "omission stores a restore slot"
    );

    // Admit win-d so restore cannot take the exact-tree path and must regroup.
    layout
        .admit_window(
            &k,
            &WindowId("win-d".to_owned()),
            bounds,
            false,
            false,
            &obs_list(&["win-a", "win-c", "win-d"]),
        )
        .expect("admit win-d");
    let pre_restore = domain_conv_group_ids(&layout, &k);
    let pre_next = layout.group_alloc.next_value();
    assert!(pre_next > seeded_next);

    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a", "win-c", "win-d", "win-b"]),
            Some(&WindowId("win-b".to_owned())),
            &BTreeSet::new(),
        )
        .expect("restore win-b");
    assert!(
        !layout
            .domain(&k)
            .unwrap()
            .minimized_slots
            .contains_key(&WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );
    let post_restore = domain_conv_group_ids(&layout, &k);
    let fresh: Vec<NodeId> = post_restore.difference(&pre_restore).cloned().collect();
    assert_eq!(fresh.len(), 1, "restore regroups exactly one fresh group");
    assert!(
        !seeded_groups.contains(&fresh[0]),
        "restored group never reissues a seeded id: {:?}",
        fresh[0]
    );
    assert_eq!(
        layout.group_alloc.next_value(),
        pre_next + 1,
        "restore burns exactly one serial"
    );
    assert_eq!(layout.tiled_count(&k), 4);

    // Explicitly close win-b and re-observe: fresh admission mints a group
    // never seen before.
    let mut seen = seeded_groups;
    seen.extend(pre_restore);
    seen.extend(post_restore);
    let restore_next = layout.group_alloc.next_value();

    let carried: Vec<EngineWindow> = ["win-a", "win-c", "win-d"]
        .into_iter()
        .map(engine_window)
        .collect();
    layout
        .converge_retained_observation(
            &k,
            &carried,
            &WindowId("win-c".to_owned()),
            &[WindowId("win-b".to_owned())],
        )
        .expect("close win-b");

    layout
        .converge_domain(
            &k,
            &obs_list(&["win-a", "win-c", "win-d", "win-b"]),
            Some(&WindowId("win-b".to_owned())),
            &BTreeSet::new(),
        )
        .expect("remint win-b");
    let repeat_groups = domain_conv_group_ids(&layout, &k);
    let live_fresh: Vec<NodeId> = repeat_groups
        .iter()
        .filter(|id| !seen.contains(*id))
        .cloned()
        .collect();
    assert_eq!(live_fresh.len(), 1);
    assert!(live_fresh[0].0.starts_with("grp-"));
    assert!(layout.group_alloc.next_value() > restore_next);
    assert_eq!(layout.tiled_count(&k), 4);
}

#[test]
fn alloc_exhausted_converge_refuses_instead_of_floating() {
    let k = key();
    let mut layout = tiled_layout();
    layout.group_alloc = super::super::group_alloc::GroupAllocator::test_with_next(u64::MAX);
    let observed = vec![observed_window("w1", false), observed_window("w2", false)];
    let err = layout
        .converge_domain(
            &k,
            &observed,
            Some(&WindowId("w1".to_owned())),
            &BTreeSet::new(),
        )
        .expect_err("exhausted converge refuses");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::MalformedTopology),
        "fail-closed refusal, got {err:?}"
    );
    assert!(
        !layout.is_capacity_float(&k, &WindowId("w2".to_owned())),
        "exhausted newcomer never silently floats"
    );
    assert!(!layout.is_exception(&k, &WindowId("w2".to_owned())));
    assert_eq!(layout.tiled_count(&k), 1);
}

#[test]
fn victims_sort_newest_first_with_untracked_last() {
    use crate::directional::Axis;
    let k = key();
    let mut layout = WorldLayout::new();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    };
    let mut domain = DomainLayout::new(bounds, 1, 0);
    let names = ["untracked-b", "oldest", "newest", "untracked-a", "middle"];
    domain.tree = Some(Node::Group {
        id: NodeId("root".to_owned()),
        axis: Axis::Horizontal,
        children: names
            .iter()
            .map(|n| leaf_node(&format!("leaf-{n}")))
            .collect(),
        shares: vec![1; 5],
    });
    for n in &names {
        domain
            .leaves
            .insert(WindowId((*n).to_owned()), NodeId(format!("leaf-{n}")));
    }
    domain
        .admission_order
        .insert(WindowId("oldest".to_owned()), 0);
    domain
        .admission_order
        .insert(WindowId("middle".to_owned()), 1);
    domain
        .admission_order
        .insert(WindowId("newest".to_owned()), 2);
    domain.focused_leaf = Some(NodeId("leaf-oldest".to_owned()));
    domain.focus_stack = names.iter().map(|n| NodeId(format!("leaf-{n}"))).collect();
    layout.domains.insert(k.clone(), domain);

    let observed: Vec<EngineWindow> = names.iter().map(|n| engine_window(n)).collect();
    // Gap 25 on 100px width with 5 leaves cannot project (4*25 = 100), so
    // repair evicts newest-first until 4 leaves remain (3*25 = 75 < 100).
    layout
        .update_domain_gaps(&k, bounds, 25, 0, &WindowId("oldest".to_owned()), &observed)
        .expect("repair evicts one victim");
    assert!(layout.is_capacity_float(&k, &WindowId("newest".to_owned())));
    assert_eq!(layout.tiled_count(&k), 4);

    // Gap 34 on 100px width needs 2 evictions (2*34 = 68 < 100, 3*34 = 102):
    // evicts newest then middle, leaving oldest and both untracked tiled.
    layout
        .update_domain_gaps(&k, bounds, 34, 0, &WindowId("oldest".to_owned()), &observed)
        .expect("repair evicts second victim");
    assert!(layout.is_capacity_float(&k, &WindowId("middle".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("oldest".to_owned())));

    // Gap 50 on 100px width needs 3 evictions (1*50 < 100, 2*50 = 100):
    // evicts oldest next, keeping untracked last.
    layout
        .update_domain_gaps(
            &k,
            bounds,
            50,
            0,
            &WindowId("untracked-a".to_owned()),
            &observed,
        )
        .expect("repair evicts oldest before untracked");
    assert!(layout.is_capacity_float(&k, &WindowId("oldest".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("untracked-a".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("untracked-b".to_owned())));

    // Gap 50 on 50px width leaves 1 survivor: untracked-b (identity descending)
    // evicts before untracked-a.
    let narrow = Rect { w: 50, ..bounds };
    layout
        .update_domain_gaps(
            &k,
            narrow,
            50,
            0,
            &WindowId("untracked-a".to_owned()),
            &observed,
        )
        .expect("repair breaks untracked tie descending");
    assert!(layout.is_capacity_float(&k, &WindowId("untracked-b".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("untracked-a".to_owned())));
}

#[test]
fn victims_break_sequence_ties_by_identity_descending() {
    use crate::directional::Axis;
    let k = key();
    let mut layout = WorldLayout::new();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 100,
    };
    let mut domain = DomainLayout::new(bounds, 1, 0);
    let names = ["a", "c", "b"];
    domain.tree = Some(Node::Group {
        id: NodeId("root".to_owned()),
        axis: Axis::Horizontal,
        children: names
            .iter()
            .map(|n| leaf_node(&format!("leaf-{n}")))
            .collect(),
        shares: vec![1; 3],
    });
    for n in &names {
        domain
            .leaves
            .insert(WindowId((*n).to_owned()), NodeId(format!("leaf-{n}")));
        domain.admission_order.insert(WindowId((*n).to_owned()), 7);
    }
    domain.focused_leaf = Some(NodeId("leaf-a".to_owned()));
    domain.focus_stack = names.iter().map(|n| NodeId(format!("leaf-{n}"))).collect();
    layout.domains.insert(k.clone(), domain);

    let observed: Vec<EngineWindow> = names.iter().map(|n| engine_window(n)).collect();
    // Gap 50 on 100px width with 3 leaves (2*50 = 100) evicts 1 victim: "c" first.
    layout
        .update_domain_gaps(&k, bounds, 50, 0, &WindowId("a".to_owned()), &observed)
        .expect("repair evicts c first");
    assert!(layout.is_capacity_float(&k, &WindowId("c".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("b".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("a".to_owned())));

    // Gap 50 on 50px width evicts "b" next, leaving "a".
    let narrow = Rect { w: 50, ..bounds };
    layout
        .update_domain_gaps(&k, narrow, 50, 0, &WindowId("a".to_owned()), &observed)
        .expect("repair evicts b second");
    assert!(layout.is_capacity_float(&k, &WindowId("b".to_owned())));
    assert!(layout.is_tiled_window(&k, &WindowId("a".to_owned())));
}
