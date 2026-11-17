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
