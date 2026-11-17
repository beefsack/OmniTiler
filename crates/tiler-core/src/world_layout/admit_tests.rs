use crate::directional::{Node, OutputId, WorkspaceId};
use crate::session::{ExceptionBehavior, ExceptionFlags, RefusalKind};
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
            ExceptionFlags::none(),
            None,
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
        ExceptionFlags::none(),
        None,
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
fn explicit_defer_maximize_only_preserves_flags_slotless() {
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
            max_only,
            Some(ExceptionBehavior::Defer),
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
        .expect("maximize-only defer admits");
    assert!(outcome.admitted_leaf.is_none());
    assert!(outcome.geometry.is_empty());
    let domain = layout.domain(&k).expect("domain");
    assert!(domain.leaves.is_empty());
    assert_eq!(domain.exceptions.get(&win).map(|e| e.flags), Some(max_only));
}

#[test]
fn explicit_defer_floating_maximized_preserves_flags_slotless() {
    let k = key();
    let mut layout = empty_domain_layout();
    let combo = ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: true,
        sticky: false,
    };
    let win = WindowId("win-float-max".to_owned());
    let outcome = layout
        .admit_window(
            &k,
            &win,
            combo,
            Some(ExceptionBehavior::Defer),
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            false,
            false,
            &[flagged_observed("win-float-max", combo)],
        )
        .expect("floating+max defer admits");
    assert!(outcome.admitted_leaf.is_none());
    assert!(outcome.geometry.is_empty());
    let domain = layout.domain(&k).expect("domain");
    assert!(domain.leaves.is_empty());
    assert_eq!(domain.exceptions.get(&win).map(|e| e.flags), Some(combo));
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
    let max_only = ExceptionFlags {
        floating: false,
        fullscreen: false,
        maximized: true,
        sticky: false,
    };
    let float_only = ExceptionFlags {
        floating: true,
        fullscreen: false,
        maximized: false,
        sticky: false,
    };

    // Maximized-only observed with floating exception selected -> PartialObservation.
    let err = layout
        .admit_window(
            &k,
            &WindowId("win-a".to_owned()),
            float_only,
            Some(ExceptionBehavior::Defer),
            bounds,
            false,
            false,
            &[flagged_observed("win-a", max_only)],
        )
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );

    // Floating observed with no exceptions selected -> PartialObservation.
    let err = layout
        .admit_window(
            &k,
            &WindowId("win-b".to_owned()),
            ExceptionFlags::none(),
            None,
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

    // Maximized-only observed with maximized exception and no behavior -> ExceptionBehaviorUnselected.
    let err = layout
        .admit_window(
            &k,
            &WindowId("win-c".to_owned()),
            max_only,
            None,
            bounds,
            false,
            false,
            &[flagged_observed("win-c", max_only)],
        )
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::ExceptionBehaviorUnselected)
    );
}
