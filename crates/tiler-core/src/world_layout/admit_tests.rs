use crate::directional::{Node, OutputId, WorkspaceId};
use crate::session::{ExceptionFlags, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::super::state::DomainLayout;
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
        hints: WindowSizeHints::none(),
    }
}

fn tiled_seed() -> DomainLayout {
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
    domain
}

#[test]
fn builder_leaves_retained_state_untouched_until_install() {
    let key = key();
    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 7;
    let seed = tiled_seed();
    let observed = vec![observed_window("w1", false), observed_window("w2", false)];
    let counter = 42;

    let (candidate, new_counter, outcome) = layout
        .build_admission_candidate(
            &key,
            Some(&seed),
            counter,
            &WindowId("w2".to_owned()),
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            false,
            false,
            &observed,
            false,
            FixedSizePredicate::default(),
        )
        .expect("tiled candidate builds");

    assert!(!layout.domains.contains_key(&key));
    assert_eq!(layout.admission_seq_next, 7);
    assert_eq!(outcome.base_revision, 0);
    assert_eq!(outcome.revision, 1);
    assert_eq!(candidate.accepted_revision, 1);
    assert!(outcome.admitted_leaf.is_some());
    assert_eq!(outcome.admitted_leaf, candidate.focused_leaf);
    assert_eq!(new_counter, counter + 1);
    assert_eq!(
        candidate.admission_order.get(&WindowId("w2".to_owned())),
        Some(&counter)
    );
    assert!(candidate.leaves.contains_key(&WindowId("w2".to_owned())));
}

#[test]
fn builder_revision_refusal_keeps_state_but_burns_allocator() {
    let key = key();
    let mut layout = WorldLayout::new();
    layout.admission_seq_next = 7;
    let mut seed = tiled_seed();
    seed.accepted_revision = MAX_REVISION;
    let observed = vec![observed_window("w1", false), observed_window("w2", false)];

    let result = layout.build_admission_candidate(
        &key,
        Some(&seed),
        42,
        &WindowId("w2".to_owned()),
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        },
        false,
        false,
        &observed,
        false,
        FixedSizePredicate::default(),
    );

    assert_eq!(
        result.err(),
        Some(LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        })
    );
    assert!(!layout.domains.contains_key(&key));
    assert_eq!(layout.admission_seq_next, 7);
    assert!(layout.group_alloc.next_value() >= 1);
    assert_eq!(seed.accepted_revision, MAX_REVISION);
    assert_eq!(seed.leaves.len(), 1);
    assert!(!seed.leaves.contains_key(&WindowId("w2".to_owned())));
}

fn empty_domain_layout() -> WorldLayout {
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        key(),
        DomainLayout::new(
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            0,
            0,
        ),
    );
    layout
}

fn flagged_observed(id: &str, flags: ExceptionFlags) -> ObservedWindow {
    let key = key();
    ObservedWindow {
        window: WindowId(id.to_owned()),
        output: key.output,
        workspace: key.workspace,
        floating: flags.floating,
        fullscreen: flags.fullscreen,
        maximized: flags.maximized,
        sticky: flags.sticky,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

#[test]
fn normal_maximized_only_carrier_admits_tiled_with_alloc() {
    let k = key();
    let mut layout = empty_domain_layout();
    let max_only = ExceptionFlags {
        floating: false,
        fullscreen: false,
        maximized: true,
        sticky: false,
    };
    let win = WindowId("win-max".to_owned());
    let outcome = layout
        .admit_window(
            &k,
            &win,
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            false,
            false,
            &[flagged_observed("win-max", max_only)],
        )
        .expect("max-only carrier admits tiled");
    assert!(outcome.admitted_leaf.is_some());
    assert!(!outcome.geometry.is_empty());
    let domain = layout.domain(&k).expect("domain");
    assert_eq!(domain.leaves.len(), 1);
    assert!(domain.exceptions.is_empty());
}

#[test]
fn admit_binding_mismatch_stays_refused() {
    let k = key();
    let mut layout = empty_domain_layout();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 50,
    };
    let float_only = ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
    };

    // Floating observed on ordinary route -> PartialObservation.
    let err = layout
        .admit_window(
            &k,
            &WindowId("win-b".to_owned()),
            bounds,
            false,
            false,
            &[flagged_observed("win-b", float_only)],
        )
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}

fn collect_group_ids(node: &Node, out: &mut std::collections::BTreeSet<NodeId>) {
    if let Node::Group { id, children, .. } = node {
        out.insert(id.clone());
        for child in children {
            collect_group_ids(child, out);
        }
    }
}

fn domain_group_ids(layout: &WorldLayout, k: &DomainKey) -> std::collections::BTreeSet<NodeId> {
    let mut ids = std::collections::BTreeSet::new();
    if let Some(tree) = layout.tree(k) {
        collect_group_ids(tree, &mut ids);
    }
    ids
}

#[test]
fn alloc_ordinary_remove_never_reissues() {
    let k = key();
    let mut layout = empty_domain_layout();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    };
    let w1 = WindowId("win-1".to_owned());
    let w2 = WindowId("win-2".to_owned());
    let w3 = WindowId("win-3".to_owned());

    layout
        .admit_window(
            &k,
            &w1,
            bounds,
            false,
            false,
            &[observed_window("win-1", false)],
        )
        .expect("admit win-1");
    layout
        .admit_window(
            &k,
            &w2,
            bounds,
            false,
            false,
            &[
                observed_window("win-1", false),
                observed_window("win-2", false),
            ],
        )
        .expect("admit win-2");
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-0".to_owned())])
    );
    assert_eq!(layout.group_alloc.next_value(), 1);

    // Removing win-2 collapses the wrapper; the serial is gone.
    layout
        .converge_domain(
            &k,
            &[observed_window("win-1", false)],
            Some(&w1),
            &std::collections::BTreeSet::new(),
        )
        .expect("remove win-2");
    assert!(domain_group_ids(&layout, &k).is_empty());

    // Readmission mints the next serial, never the removed id.
    layout
        .admit_window(
            &k,
            &w3,
            bounds,
            false,
            false,
            &[
                observed_window("win-1", false),
                observed_window("win-3", false),
            ],
        )
        .expect("admit win-3");
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-1".to_owned())])
    );
    assert_eq!(layout.group_alloc.next_value(), 2);
}

#[test]
fn alloc_cap_fallback_burns_tried_serials() {
    let k = DomainKey {
        output: OutputId("o".to_owned()),
        workspace: WorkspaceId("w".to_owned()),
    };
    let mut layout = WorldLayout::new();
    layout.domains.insert(
        k.clone(),
        DomainLayout::new(
            Rect {
                x: 0,
                y: 0,
                w: 12,
                h: 12,
            },
            4,
            0,
        ),
    );
    let placed = Rect {
        x: 0,
        y: 0,
        w: 12,
        h: 4,
    };
    let obs = |name: &str| ObservedWindow {
        window: WindowId(name.to_owned()),
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

    layout
        .admit_window(
            &k,
            &WindowId("a".to_owned()),
            placed,
            false,
            false,
            &[obs("a")],
        )
        .expect("admit a");
    layout
        .admit_window(
            &k,
            &WindowId("b".to_owned()),
            placed,
            false,
            false,
            &[obs("a"), obs("b")],
        )
        .expect("admit b");
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-0".to_owned())])
    );

    layout
        .admit_window(
            &k,
            &WindowId("c".to_owned()),
            placed,
            false,
            false,
            &[obs("a"), obs("b"), obs("c")],
        )
        .expect("admit c via fallback");
    // Failed ordinary wrap burned grp-1; fallback split minted grp-2.
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-0".to_owned()), NodeId("grp-2".to_owned()),])
    );
    assert_eq!(layout.group_alloc.next_value(), 3);
    assert!(layout.is_tiled_window(&k, &WindowId("c".to_owned())));

    // Remove c and admit again: repeat burns ordinary/fallback pair and never reissues grp-2.
    layout
        .converge_domain(
            &k,
            &[obs("a"), obs("b")],
            Some(&WindowId("b".to_owned())),
            &std::collections::BTreeSet::new(),
        )
        .expect("remove c");
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-0".to_owned())])
    );

    layout
        .admit_window(
            &k,
            &WindowId("c".to_owned()),
            placed,
            false,
            false,
            &[obs("a"), obs("b"), obs("c")],
        )
        .expect("readmit c via fallback");
    assert_eq!(
        domain_group_ids(&layout, &k),
        std::collections::BTreeSet::from([NodeId("grp-0".to_owned()), NodeId("grp-4".to_owned()),])
    );
    assert_eq!(layout.group_alloc.next_value(), 5);
}

#[test]
fn alloc_exhausted_ordinary_refuses_instead_of_floating() {
    let k = key();
    let mut layout = empty_domain_layout();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 50,
    };
    let w1 = WindowId("win-1".to_owned());
    let w2 = WindowId("win-2".to_owned());

    layout
        .admit_window(
            &k,
            &w1,
            bounds,
            false,
            false,
            &[observed_window("win-1", false)],
        )
        .expect("admit win-1");
    layout.group_alloc = super::super::group_alloc::GroupAllocator::test_with_next(u64::MAX);

    let err = layout
        .admit_window(
            &k,
            &w2,
            bounds,
            false,
            false,
            &[
                observed_window("win-1", false),
                observed_window("win-2", false),
            ],
        )
        .expect_err("exhausted admission refuses");
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::MalformedTopology),
        "fail-closed refusal, got {err:?}"
    );
    assert!(
        !layout.is_capacity_float(&k, &w2),
        "exhausted window never silently floats"
    );
    assert!(!layout.is_exception(&k, &w2));
    assert_eq!(layout.tiled_count(&k), 1);
}
