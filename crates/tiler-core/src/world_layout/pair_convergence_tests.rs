use std::collections::{BTreeMap, BTreeSet};

use crate::contract::MAX_REVISION;
use crate::directional::{Axis, Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::LayoutFailure;
use super::state::{DomainException, DomainLayout, WorldLayout};

fn bounds_200() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
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
        bounds: bounds_200(),
        gap: 4,
        adjacent,
    }
}

fn pair_domains() -> (OutputDomain, OutputDomain) {
    let mut s_adj = BTreeMap::new();
    s_adj.insert(Direction::Right, OutputId("out-2".to_owned()));
    let mut t_adj = BTreeMap::new();
    t_adj.insert(Direction::Left, OutputId("out-1".to_owned()));
    (
        domain_desc("out-1", "ws-1", s_adj),
        domain_desc("out-2", "ws-1", t_adj),
    )
}

fn obs(window: &str, domain: &OutputDomain, floating: bool) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: domain.id.clone(),
        workspace: domain.workspace.clone(),
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn single_leaf_domain(win: &str, leaf: &str, revision: u64) -> DomainLayout {
    let mut domain = DomainLayout::new(bounds_200(), 4, 0);
    let n = NodeId(leaf.to_owned());
    domain.tree = Some(Node::Leaf { id: n.clone() });
    domain.leaves.insert(WindowId(win.to_owned()), n.clone());
    domain.focused_leaf = Some(n.clone());
    domain.focus_stack = vec![n.clone()];
    domain.focus_history = vec![n.clone()];
    domain.last_active = Some(n);
    domain.accepted_revision = revision;
    domain
}

fn two_leaf_domain(
    group_id: &str,
    w1: (&str, &str),
    w2: (&str, &str),
    last_active_leaf: &str,
    revision: u64,
) -> DomainLayout {
    let mut domain = DomainLayout::new(bounds_200(), 4, 0);
    let n1 = NodeId(w1.1.to_owned());
    let n2 = NodeId(w2.1.to_owned());
    domain.tree = Some(Node::Group {
        id: NodeId(group_id.to_owned()),
        axis: Axis::Horizontal,
        shares: vec![1, 1],
        children: vec![Node::Leaf { id: n1.clone() }, Node::Leaf { id: n2.clone() }],
    });
    domain.leaves.insert(WindowId(w1.0.to_owned()), n1.clone());
    domain.leaves.insert(WindowId(w2.0.to_owned()), n2.clone());
    domain.focused_leaf = Some(NodeId(last_active_leaf.to_owned()));
    domain.focus_stack = vec![n1.clone(), n2.clone()];
    domain.focus_history = vec![n1, n2];
    domain.last_active = Some(NodeId(last_active_leaf.to_owned()));
    domain.accepted_revision = revision;
    domain
}

fn collect_all_ids(node: &Node, out: &mut BTreeSet<NodeId>) {
    match node {
        Node::Leaf { id } => {
            out.insert(id.clone());
        }
        Node::Group { id, children, .. } => {
            out.insert(id.clone());
            for child in children {
                collect_all_ids(child, out);
            }
        }
    }
}

#[test]
fn carried_order_cross_domain_fresh_anchor_shared_ids_and_highwater() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key, single_leaf_domain("s1", "l-1", 2));
    world.domains.insert(
        t_key,
        two_leaf_domain("g-0", ("t1", "l-1"), ("t2", "l-2"), "l-2", 3),
    );

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    let remapped_t2 = pair.target.leaves[&WindowId("t2".to_owned())].clone();
    assert_eq!(pair.target.last_active.as_ref(), Some(&remapped_t2));

    // Carried order: s_new (source), t_new (target), s_after (source).
    let observed = vec![
        obs("s1", &s_desc, false),
        obs("t1", &t_desc, false),
        obs("t2", &t_desc, false),
        obs("s_new", &s_desc, false),
        obs("t_new", &t_desc, false),
        obs("s_after", &s_desc, false),
    ];

    let counts = world
        .converge_pair(&mut pair, &observed, None, &BTreeSet::new())
        .expect("pair converges");
    assert_eq!(counts.admitted, 3);

    // One shared trial counter in carried admission order.
    assert_eq!(pair.admission_seq_next, 3);
    assert_eq!(
        pair.source
            .admission_order
            .get(&WindowId("s_new".to_owned())),
        Some(&0)
    );
    assert_eq!(
        pair.target
            .admission_order
            .get(&WindowId("t_new".to_owned())),
        Some(&1)
    );
    assert_eq!(
        pair.source
            .admission_order
            .get(&WindowId("s_after".to_owned())),
        Some(&2)
    );

    // Switching back to source does not revive s_new as evolving anchor:
    // s_after falls back to source's original last_active (l-1 = s1), so s1
    // is split again rather than s_new.
    let s1_leaf = &pair.source.leaves[&WindowId("s1".to_owned())];
    let s_after_leaf = &pair.source.leaves[&WindowId("s_after".to_owned())];
    match pair.source.tree.as_ref().expect("source tree") {
        Node::Group { children, .. } => {
            let has_s1_and_s_after_sibling = children.iter().any(|child| match child {
                Node::Group { children: sub, .. } => {
                    sub.iter().any(|c| c.id() == s1_leaf)
                        && sub.iter().any(|c| c.id() == s_after_leaf)
                }
                _ => false,
            });
            assert!(
                has_s1_and_s_after_sibling,
                "s_after should split s1 (original last_active), not revive s_new"
            );
        }
        other => panic!("expected group, got {other:?}"),
    }

    // t_new split remapped_t2 (target's remapped last_active), not pre-remap l-2.
    let t_new_leaf = &pair.target.leaves[&WindowId("t_new".to_owned())];
    let mut t_ids = BTreeSet::new();
    collect_all_ids(pair.target.tree.as_ref().expect("target tree"), &mut t_ids);
    let mut s_ids = BTreeSet::new();
    collect_all_ids(pair.source.tree.as_ref().expect("source tree"), &mut s_ids);
    assert!(
        s_ids.is_disjoint(&t_ids),
        "shared node IDs must be disjoint"
    );
    assert!(
        t_ids.contains(t_new_leaf) && t_ids.contains(&remapped_t2),
        "target tree should contain remapped t2 and t_new"
    );
}

#[test]
fn global_marker_source_priority_retention_and_revision_semantics() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut s_dom = single_leaf_domain("s1", "l-1", 4);
    let float_rect = Rect {
        x: 10,
        y: 20,
        w: 80,
        h: 60,
    };
    // Retained float geometry stored on source for a window currently tiled on target.
    s_dom
        .retained_float_geometry
        .insert(WindowId("t1".to_owned()), float_rect);

    let mut t_dom = single_leaf_domain("t1", "l-1", 9);
    // Orphan automatic marker stored on target for a hidden window.
    t_dom
        .automatic_fixed
        .insert(WindowId("w_hidden".to_owned()));

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s_dom);
    world.domains.insert(t_key, t_dom);

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    assert_eq!(pair.pair_revision, 9);

    // 1. Focus-only observation with hidden w_hidden: orphan repartition alone
    // must NOT bump revision!
    let mut hidden = BTreeSet::new();
    hidden.insert(WindowId("w_hidden".to_owned()));
    let observed_noop = vec![obs("s1", &s_desc, false), obs("t1", &t_desc, false)];
    let focus_t1 = WindowId("t1".to_owned());
    let counts = world
        .converge_pair(&mut pair, &observed_noop, Some(&focus_t1), &hidden)
        .expect("noop converges");
    assert_eq!(counts.removed, 0);
    assert_eq!(counts.admitted, 0);
    assert_eq!(counts.flags_adopted, 0);
    assert_eq!(pair.pair_revision, 9);
    assert_eq!(pair.source.accepted_revision, 9);
    assert_eq!(pair.target.accepted_revision, 9);
    assert!(
        pair.source
            .automatic_fixed
            .contains(&WindowId("w_hidden".to_owned()))
            || pair
                .target
                .automatic_fixed
                .contains(&WindowId("w_hidden".to_owned()))
    );

    // 2. Demote t1 to floating on target: uses global retained_float_geometry
    // from source and bumps BOTH domains' accepted_revision once (9 -> 10).
    let observed_float = vec![obs("s1", &s_desc, false), obs("t1", &t_desc, true)];
    let counts = world
        .converge_pair(
            &mut pair,
            &observed_float,
            Some(&WindowId("s1".to_owned())),
            &hidden,
        )
        .expect("float demotion converges");
    assert_eq!(counts.flags_adopted, 1);
    assert_eq!(pair.pair_revision, 10);
    assert_eq!(pair.source.accepted_revision, 10);
    assert_eq!(pair.target.accepted_revision, 10);
    assert_eq!(
        pair.target.exceptions.get(&WindowId("t1".to_owned())),
        Some(&DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: Some(float_rect),
        })
    );
}

#[test]
fn inactive_focus_tail_prunes_without_append_and_preserves_last_active() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world.domains.insert(
        s_key.clone(),
        two_leaf_domain("g-s", ("s1", "l-1"), ("s2", "l-2"), "l-1", 1),
    );
    world.domains.insert(
        t_key,
        two_leaf_domain("g-t", ("t1", "l-1"), ("t2", "l-2"), "l-1", 1),
    );

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    let remapped_t1 = pair.target.leaves[&WindowId("t1".to_owned())].clone();

    // t2 disappears from target; focus is s2 on source (so target is inactive).
    let observed = vec![
        obs("s1", &s_desc, false),
        obs("s2", &s_desc, false),
        obs("t1", &t_desc, false),
    ];
    let focus_s2 = WindowId("s2".to_owned());
    let counts = world
        .converge_pair(&mut pair, &observed, Some(&focus_s2), &BTreeSet::new())
        .expect("converges");
    assert_eq!(counts.removed, 1);

    // Active source: s2 demoted/re-appended on focus_stack, last_active = s2,
    // focus_history not appended.
    assert_eq!(pair.focused_domain.as_ref(), Some(&s_key));
    assert_eq!(pair.focused_leaf, Some(NodeId("l-2".to_owned())));
    assert_eq!(
        pair.source.focus_stack,
        vec![NodeId("l-1".to_owned()), NodeId("l-2".to_owned())]
    );
    assert_eq!(pair.source.last_active, Some(NodeId("l-2".to_owned())));
    assert_eq!(
        pair.source.focus_history,
        vec![NodeId("l-1".to_owned()), NodeId("l-2".to_owned())]
    );

    // Inactive target: t2 pruned from focus_stack and focus_history, original
    // last_active (remapped_t1) preserved.
    assert_eq!(pair.target.focus_stack, vec![remapped_t1.clone()]);
    assert_eq!(pair.target.focus_history, vec![remapped_t1.clone()]);
    assert_eq!(pair.target.last_active, Some(remapped_t1));
}

#[test]
fn revision_limit_atomicity_preserves_pair_and_burns_allocator() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key, single_leaf_domain("s1", "l-1", MAX_REVISION));
    world
        .domains
        .insert(t_key, single_leaf_domain("t1", "l-1", 2));

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    let before_source_tree = pair.source.tree.clone();
    let before_target_tree = pair.target.tree.clone();
    let before_serial = world.group_alloc.next_value();

    let observed = vec![
        obs("s1", &s_desc, false),
        obs("t1", &t_desc, false),
        obs("t_new", &t_desc, false),
    ];
    let err = world
        .converge_pair(&mut pair, &observed, None, &BTreeSet::new())
        .expect_err("must refuse at MAX_REVISION");
    assert_eq!(
        err,
        LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        }
    );

    // Caller pair untouched.
    assert_eq!(pair.source.tree, before_source_tree);
    assert_eq!(pair.target.tree, before_target_tree);
    assert_eq!(pair.pair_revision, MAX_REVISION);
    assert_eq!(pair.admission_seq_next, 0);

    // Shared group allocator burns survive the refused candidate.
    let after_serial = world.group_alloc.next_value();
    assert!(after_serial > before_serial);
}

#[test]
fn retained_homing_gate_refuses_cross_domain_transplant() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key, single_leaf_domain("s1", "l-1", 1));
    world
        .domains
        .insert(t_key, single_leaf_domain("t1", "l-1", 1));

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // Known source window s1 observed on target domain -> CrossDomainMismatch.
    let observed = vec![obs("s1", &t_desc, false), obs("t1", &t_desc, false)];
    let err = world
        .converge_pair(&mut pair, &observed, None, &BTreeSet::new())
        .expect_err("transplant must be refused");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)
    );
    assert!(pair.source.leaves.contains_key(&WindowId("s1".to_owned())));
}

#[test]
fn nonconsuming_install_preserves_canonical_pair_view() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let mut world = WorldLayout::new();
    world
        .domains
        .insert(s_key.clone(), single_leaf_domain("s1", "l-1", 1));
    world
        .domains
        .insert(t_key.clone(), single_leaf_domain("t1", "l-1", 1));

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    let observed = vec![
        obs("s1", &s_desc, false),
        obs("t1", &t_desc, false),
        obs("t2", &t_desc, false),
    ];
    world
        .converge_pair(&mut pair, &observed, None, &BTreeSet::new())
        .expect("pair converges");
    pair.forget_minimized_slot(&WindowId("nonexistent".to_owned()));
    world
        .install_pair(&pair, 0, None)
        .expect("nonconsuming install succeeds");

    // Pair working view still holds canonical pair-target IDs.
    assert!(
        pair.target.leaves[&WindowId("t1".to_owned())]
            .0
            .starts_with("pair-target-")
    );
    // Installed world domain has restored t1's original local ID and admitted t2.
    let installed_t = world.domain(&t_key).expect("target installed");
    assert_eq!(
        installed_t.leaves.get(&WindowId("t1".to_owned())),
        Some(&NodeId("l-1".to_owned()))
    );
    assert!(installed_t.leaves.contains_key(&WindowId("t2".to_owned())));
    assert_eq!(installed_t.accepted_revision, 2);
}

#[test]
fn single_converge_candidate_remembered_leaf_reads_retained_world_not_uninstalled_original() {
    let (s_desc, _) = pair_domains();
    let key = s_desc.key();

    let mut world = WorldLayout::new();
    // Retained world remembers l-1 as last_active.
    world.domains.insert(
        key.clone(),
        two_leaf_domain("g-0", ("w1", "l-1"), ("w2", "l-2"), "l-1", 1),
    );

    // Uninstalled original has no focused_leaf and a different last_active (l-2).
    let mut uninstalled = two_leaf_domain("g-0", ("w1", "l-1"), ("w2", "l-2"), "l-2", 1);
    uninstalled.focused_leaf = None;
    uninstalled.focus_stack = vec![NodeId("l-2".to_owned())];

    let observed = vec![
        obs("w1", &s_desc, false),
        obs("w2", &s_desc, false),
        obs("w3", &s_desc, false),
    ];
    let (candidate, _, _) = world
        .converge_candidate(&key, &uninstalled, &observed, None, &BTreeSet::new())
        .expect("single candidate converges");

    let l1 = NodeId("l-1".to_owned());
    let w3_leaf = &candidate.leaves[&WindowId("w3".to_owned())];
    let split_l1 = match candidate.tree.as_ref().expect("tree") {
        Node::Group { children, .. } => children.iter().any(|child| match child {
            Node::Group { children: sub, .. } => {
                sub.iter().any(|c| c.id() == &l1) && sub.iter().any(|c| c.id() == w3_leaf)
            }
            _ => false,
        }),
        other => panic!("expected group, got {other:?}"),
    };
    assert!(
        split_l1,
        "fresh_admit_anchor must read retained world.remembered_leaf(l-1), not uninstalled original.last_active(l-2)"
    );
}

#[test]
fn pair_capacity_exception_retile_and_absence_clear_capacity_marker() {
    let (s_desc, t_desc) = pair_domains();
    let s_key = s_desc.key();
    let t_key = t_desc.key();

    let float_exc = || DomainException {
        flags: ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        floating_geometry: None,
    };

    let mut s_dom = single_leaf_domain("s1", "l-1", 1);
    s_dom
        .exceptions
        .insert(WindowId("s_cap".to_owned()), float_exc());
    s_dom.capacity_floats.insert(WindowId("s_cap".to_owned()));

    let mut t_dom = single_leaf_domain("t1", "l-1", 1);
    t_dom
        .exceptions
        .insert(WindowId("t_cap_absent".to_owned()), float_exc());
    t_dom
        .capacity_floats
        .insert(WindowId("t_cap_absent".to_owned()));

    let mut world = WorldLayout::new();
    world.domains.insert(s_key, s_dom);
    world.domains.insert(t_key, t_dom);

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");

    // s_cap is observed tiled (retiles into source); t_cap_absent is absent.
    let observed = vec![
        obs("s1", &s_desc, false),
        obs("s_cap", &s_desc, false),
        obs("t1", &t_desc, false),
    ];
    let counts = world
        .converge_pair(&mut pair, &observed, None, &BTreeSet::new())
        .expect("pair converges");
    assert_eq!(counts.removed, 1);
    assert_eq!(counts.flags_adopted, 1);
    assert!(
        pair.source
            .leaves
            .contains_key(&WindowId("s_cap".to_owned()))
    );
    assert!(
        !pair
            .source
            .exceptions
            .contains_key(&WindowId("s_cap".to_owned()))
    );
    assert!(
        pair.source.capacity_floats.is_empty(),
        "retiled s_cap must not be resurrected into source.capacity_floats"
    );
    assert!(
        pair.target.capacity_floats.is_empty(),
        "absent t_cap_absent must not be resurrected into target.capacity_floats"
    );
}

#[test]
fn stale_last_active_falls_through_to_stack_then_overlap() {
    use super::pair_operation::PairOperationContext;
    use crate::seed::EngineWindow;

    // Closing the last-active leaf on target while source is focused leaves
    // target `last_active` stale (write-only, never pruned) while the stack
    // prunes to the survivor. The stale head misses the candidate set, so
    // selection still reaches the surviving edge leaf.
    let (s_desc, t_desc) = pair_domains();
    let source = s_desc.key();
    let target = t_desc.key();

    let mut world = WorldLayout::new();
    world.domains.insert(
        target.clone(),
        two_leaf_domain(
            "g-t",
            ("win-2", "leaf-win-2"),
            ("win-3", "leaf-win-3"),
            "leaf-win-3",
            2,
        ),
    );
    world
        .domains
        .insert(source.clone(), single_leaf_domain("win-1", "leaf-win-1", 1));

    let mut pair = world
        .assemble_directional_pair(&s_desc, &t_desc)
        .expect("pair assembles");
    // Source holds global focus on win-1; win-3 closes on target.
    pair.focused_domain = Some(source.clone());
    pair.focused_leaf = Some(NodeId("leaf-win-1".to_owned()));
    let close_obs = vec![obs("win-1", &s_desc, false), obs("win-2", &t_desc, false)];
    world
        .converge_pair(
            &mut pair,
            &close_obs,
            Some(&WindowId("win-1".to_owned())),
            &BTreeSet::new(),
        )
        .expect("close converges");
    world
        .install_pair(&pair, 0, None)
        .expect("install converged pair");

    let tiled_target: Vec<String> = world.tiled_windows(&target).map(|w| w.0.clone()).collect();
    assert_eq!(tiled_target, vec!["win-2"]);
    let history = world.cross_output_history_newest_first(&target);
    assert_eq!(
        history.len(),
        2,
        "stale head plus pruned stack: {history:?}"
    );
    assert!(
        world.window_for_leaf(&target, &history[0]).is_none(),
        "stale head resolves to no link"
    );
    assert_eq!(
        world.remembered_leaf(&target),
        None,
        "stale slot never poses as remembered"
    );

    let engine_obs = vec![
        EngineWindow {
            window: WindowId("win-1".to_owned()),
            output: s_desc.id.clone(),
            workspace: s_desc.workspace.clone(),
            rect: bounds_200(),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        },
        EngineWindow {
            window: WindowId("win-2".to_owned()),
            output: t_desc.id.clone(),
            workspace: t_desc.workspace.clone(),
            rect: bounds_200(),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: WindowSizeHints::none(),
        },
    ];
    let subject = WindowId("win-1".to_owned());
    let ctx = PairOperationContext {
        source_domain: &s_desc,
        source_outer_gap: 0,
        target_domain: &t_desc,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &engine_obs,
        closed: &[],
    };
    let plan = world
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, false)
        .expect("cross-output focus selects surviving leaf");
    assert_eq!(plan.to_window.0, "win-2");
}
