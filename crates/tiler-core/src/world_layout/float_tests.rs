use std::collections::BTreeMap;

use crate::contract::MAX_REVISION;
use crate::directional::{Axis, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ExceptionFlags, ObservedWindow, RefusalKind, centered_float_rect};
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

fn obs(window: &str, floating: bool) -> ObservedWindow {
    let k = key();
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: k.output,
        workspace: k.workspace,
        floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn two_tile_layout() -> WorldLayout {
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
        children: vec![leaf("a"), leaf("b")],
        shares: vec![500, 500],
    });
    domain
        .leaves
        .insert(WindowId("w1".to_owned()), NodeId("a".to_owned()));
    domain
        .leaves
        .insert(WindowId("w2".to_owned()), NodeId("b".to_owned()));
    domain.focused_leaf = Some(NodeId("a".to_owned()));
    domain.focus_stack = vec![NodeId("b".to_owned()), NodeId("a".to_owned())];
    domain.focus_history = vec![NodeId("b".to_owned()), NodeId("a".to_owned())];
    domain.last_active = Some(NodeId("a".to_owned()));
    domain.admission_order.insert(WindowId("w1".to_owned()), 1);
    domain.admission_order.insert(WindowId("w2".to_owned()), 2);
    layout.admission_seq_next = 3;
    layout.domains.insert(key(), domain);
    layout
}

#[test]
fn explicit_durable_and_default_float_geometry_round_trip() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());

    // Default centered 60% when no explicit or retained rect exists.
    let outcome = layout
        .toggle_float(&k, &w1, None, &[obs("w1", false), obs("w2", false)])
        .expect("float with default rect");
    let expected_default = centered_float_rect(Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    });
    assert_eq!(outcome.base_revision, 0);
    assert_eq!(outcome.revision, 1);
    assert_eq!(outcome.float_window, Some(w1.clone()));
    assert_eq!(outcome.float_rect, Some(expected_default));
    assert_eq!(
        layout.domain(&k).unwrap().retained_float_geometry.get(&w1),
        Some(&expected_default)
    );

    // Unfloat with live moved frame stores durable retained geometry.
    let moved = Rect {
        x: 15,
        y: 12,
        w: 44,
        h: 36,
    };
    let unfloat = layout
        .toggle_float(&k, &w1, Some(moved), &[obs("w1", true), obs("w2", false)])
        .expect("unfloat with moved rect");
    assert_eq!(unfloat.base_revision, 1);
    assert_eq!(unfloat.revision, 2);
    assert_eq!(unfloat.float_window, None);
    assert_eq!(unfloat.float_rect, None);
    assert_eq!(
        layout.domain(&k).unwrap().retained_float_geometry.get(&w1),
        Some(&moved)
    );

    // Refloat without rect reuses durable retained geometry, not default 60%.
    let refloat = layout
        .toggle_float(&k, &w1, None, &[obs("w1", false), obs("w2", false)])
        .expect("refloat reuses retained");
    assert_eq!(refloat.float_rect, Some(moved));
    assert_eq!(
        layout
            .domain(&k)
            .unwrap()
            .exceptions
            .get(&w1)
            .and_then(|e| e.floating_geometry),
        Some(moved)
    );

    // Unfloat without rect keeps the retained geometry; explicit rect on next float overrides.
    layout
        .toggle_float(&k, &w1, None, &[obs("w1", true), obs("w2", false)])
        .expect("unfloat without rect");
    let explicit = Rect {
        x: 4,
        y: 6,
        w: 30,
        h: 20,
    };
    let override_float = layout
        .toggle_float(
            &k,
            &w1,
            Some(explicit),
            &[obs("w1", false), obs("w2", false)],
        )
        .expect("float with explicit rect");
    assert_eq!(override_float.float_rect, Some(explicit));
    assert_eq!(
        layout.domain(&k).unwrap().retained_float_geometry.get(&w1),
        Some(&explicit)
    );
}

#[test]
fn fullscreen_toggle_float_changes_membership_without_subject_writes() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());
    let mut o1 = obs("w1", false);
    o1.fullscreen = true;

    let outcome = layout
        .toggle_float(&k, &w1, None, &[o1, obs("w2", false)])
        .expect("fullscreen float");
    assert!(outcome.geometry.iter().all(|g| g.window != w1));
    assert!(!outcome.geometry.is_empty());

    let domain = layout.domain(&k).unwrap();
    assert!(!domain.leaves.contains_key(&w1));
    let record = domain.exceptions.get(&w1).expect("float exception");
    assert_eq!(
        record.flags,
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        }
    );
    let expected = centered_float_rect(Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    });
    assert_eq!(record.floating_geometry, Some(expected));
    assert_eq!(domain.retained_float_geometry.get(&w1), Some(&expected));
}

#[test]
fn fullscreen_double_toggle_readmits_invisibly() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());

    let mut o1_tiled = obs("w1", false);
    o1_tiled.fullscreen = true;
    layout
        .toggle_float(&k, &w1, None, &[o1_tiled, obs("w2", false)])
        .expect("fullscreen float");
    let retained = layout
        .domain(&k)
        .unwrap()
        .retained_float_geometry
        .get(&w1)
        .copied()
        .expect("retained");

    let mut o1_float = obs("w1", true);
    o1_float.fullscreen = true;
    let outcome = layout
        .toggle_float(&k, &w1, None, &[o1_float, obs("w2", false)])
        .expect("fullscreen unfloat");
    assert!(outcome.geometry.iter().all(|g| g.window != w1));
    assert_eq!(outcome.float_window, None);
    assert_eq!(outcome.float_rect, None);

    let domain = layout.domain(&k).unwrap();
    assert!(domain.leaves.contains_key(&w1));
    assert!(!domain.exceptions.contains_key(&w1));
    assert_eq!(domain.retained_float_geometry.get(&w1), Some(&retained));
}

#[test]
fn fullscreen_toggle_float_keeps_explicit_retained_placement() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());
    let explicit = Rect {
        x: 8,
        y: 8,
        w: 40,
        h: 30,
    };

    let mut o1_tiled = obs("w1", false);
    o1_tiled.fullscreen = true;
    let outcome = layout
        .toggle_float(
            &k,
            &w1,
            Some(explicit),
            &[o1_tiled.clone(), obs("w2", false)],
        )
        .expect("fullscreen explicit float");
    assert!(outcome.geometry.iter().all(|g| g.window != w1));
    assert_eq!(outcome.float_rect, Some(explicit));

    let mut o1_float = obs("w1", true);
    o1_float.fullscreen = true;
    layout
        .toggle_float(&k, &w1, None, &[o1_float, obs("w2", false)])
        .expect("fullscreen unfloat");
    assert_eq!(
        layout.domain(&k).unwrap().retained_float_geometry.get(&w1),
        Some(&explicit)
    );

    layout
        .toggle_float(&k, &w1, None, &[o1_tiled, obs("w2", false)])
        .expect("fullscreen refloat");
    assert_eq!(
        layout
            .domain(&k)
            .unwrap()
            .exceptions
            .get(&w1)
            .and_then(|e| e.floating_geometry),
        Some(explicit)
    );
}

#[test]
fn maximized_toggle_float_refuses_both_directions() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());

    let mut o1_max = obs("w1", false);
    o1_max.maximized = true;
    let err = layout
        .toggle_float(&k, &w1, None, &[o1_max, obs("w2", false)])
        .unwrap_err();
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::NotTiled));

    layout
        .toggle_float(&k, &w1, None, &[obs("w1", false), obs("w2", false)])
        .expect("ordinary float");

    let mut o1_float_max = obs("w1", true);
    o1_float_max.maximized = true;
    let err = layout
        .toggle_float(&k, &w1, None, &[o1_float_max, obs("w2", false)])
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}

#[test]
fn no_space_retry_keeps_float_with_no_override_or_frame_authority() {
    let k = key();
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(
        Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        0,
        0,
    );
    domain.tree = Some(leaf("a"));
    let w1 = WindowId("w1".to_owned());
    let w2 = WindowId("w2".to_owned());
    domain.leaves.insert(w1.clone(), NodeId("a".to_owned()));
    domain.focused_leaf = Some(NodeId("a".to_owned()));
    domain.focus_stack = vec![NodeId("a".to_owned())];
    domain.admission_order.insert(w1.clone(), 1);
    let saved_rect = Rect {
        x: 10,
        y: 10,
        w: 40,
        h: 30,
    };
    domain.exceptions.insert(
        w2.clone(),
        DomainException {
            flags: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            floating_geometry: Some(saved_rect),
        },
    );
    domain
        .retained_float_geometry
        .insert(w2.clone(), saved_rect);
    layout.admission_seq_next = 2;
    layout.domains.insert(k.clone(), domain);

    let outcome = layout
        .toggle_float(&k, &w2, None, &[obs("w1", false), obs("w2", true)])
        .expect("capacity re-float commits");
    assert_eq!(outcome.base_revision, 0);
    assert_eq!(outcome.revision, 1);
    assert!(outcome.geometry.is_empty());
    assert_eq!(outcome.float_window, Some(w2.clone()));
    assert_eq!(outcome.float_rect, None);
    assert_eq!(layout.admission_seq_next, 2);

    let domain = layout.domain(&k).unwrap();
    assert!(!domain.leaves.contains_key(&w2));
    assert!(domain.capacity_floats.contains(&w2));
    assert!(!domain.fixed_tile_override.contains(&w2));
    assert_eq!(
        domain.exceptions.get(&w2).and_then(|e| e.floating_geometry),
        None
    );
    assert_eq!(domain.retained_float_geometry.get(&w2), Some(&saved_rect));
}

#[test]
fn revision_limit_refusal_keeps_state_and_burns_allocator_on_unfloat() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());

    // Float w1 first, then push domain to MAX_REVISION.
    layout
        .toggle_float(&k, &w1, None, &[obs("w1", false), obs("w2", false)])
        .expect("float");
    layout.domains.get_mut(&k).unwrap().accepted_revision = MAX_REVISION;
    let seq_before = layout.admission_seq_next;
    let next_group_before = layout.group_alloc.next_value();

    let err = layout
        .toggle_float(&k, &w1, None, &[obs("w1", true), obs("w2", false)])
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::RevisionLimit {
            base: MAX_REVISION,
            limit: MAX_REVISION,
        }
    );
    assert_eq!(layout.admission_seq_next, seq_before);
    assert!(
        layout.group_alloc.next_value() > next_group_before,
        "unfloat trial burns shared group serial even on revision refusal"
    );
    let domain = layout.domain(&k).unwrap();
    assert_eq!(domain.accepted_revision, MAX_REVISION);
    assert!(domain.exceptions.contains_key(&w1));
    assert!(!domain.leaves.contains_key(&w1));
}

#[test]
fn float_focused_leaf_falls_back_clears_fixed_marks_and_preserves_other_domain() {
    let k = key();
    let ok = other_key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());
    layout
        .domains
        .get_mut(&k)
        .unwrap()
        .automatic_fixed
        .insert(w1.clone());
    layout
        .domains
        .get_mut(&k)
        .unwrap()
        .fixed_tile_override
        .insert(w1.clone());

    let mut other = DomainLayout::new(
        Rect {
            x: 200,
            y: 0,
            w: 100,
            h: 80,
        },
        0,
        0,
    );
    other.tree = Some(leaf("x"));
    other
        .leaves
        .insert(WindowId("wx".to_owned()), NodeId("x".to_owned()));
    other.accepted_revision = 9;
    layout.domains.insert(ok.clone(), other);

    let outcome = layout
        .toggle_float(&k, &w1, None, &[obs("w1", false), obs("w2", false)])
        .expect("float focused leaf");
    assert_eq!(outcome.focus_leaf, Some(NodeId("b".to_owned())));
    let domain = layout.domain(&k).unwrap();
    assert_eq!(domain.focused_leaf, Some(NodeId("b".to_owned())));
    assert_eq!(domain.last_active, Some(NodeId("b".to_owned())));
    assert_eq!(domain.focus_stack, vec![NodeId("b".to_owned())]);
    assert_eq!(domain.focus_history, vec![NodeId("b".to_owned())]);
    assert!(!domain.automatic_fixed.contains(&w1));
    assert!(!domain.fixed_tile_override.contains(&w1));

    // Foreign domain untouched.
    assert_eq!(layout.domain(&ok).unwrap().accepted_revision, 9);
    assert!(
        layout
            .domain(&ok)
            .unwrap()
            .leaves
            .contains_key(&WindowId("wx".to_owned()))
    );
}

#[test]
fn validation_order_preserves_topology_shape_scope_priority() {
    let k = key();
    let mut layout = two_tile_layout();
    let w1 = WindowId("w1".to_owned());
    let bad_rect = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 10,
    };

    // Malformed topology wins over malformed command rect.
    layout.domains.get_mut(&k).unwrap().leaves = BTreeMap::new();
    let err = layout
        .toggle_float(&k, &w1, Some(bad_rect), &[obs("w1", false)])
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::MalformedTopology)
    );

    // Malformed command rect wins over cross-domain observed entry.
    let mut layout = two_tile_layout();
    let mut foreign = obs("w1", false);
    foreign.output = OutputId("other".to_owned());
    let err = layout
        .toggle_float(
            &k,
            &w1,
            Some(bad_rect),
            &[foreign.clone(), obs("w2", false)],
        )
        .unwrap_err();
    assert_eq!(err, LayoutFailure::Semantics(RefusalKind::MalformedInput));

    // Unknown domain with non-empty observation -> CrossDomainMismatch.
    let unknown = DomainKey {
        output: OutputId("missing".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
    };
    let mut missing_obs = obs("w1", false);
    missing_obs.output = unknown.output.clone();
    let err = layout
        .toggle_float(&unknown, &w1, None, &[missing_obs])
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch)
    );

    // Unknown domain with empty observation -> PartialObservation.
    let err = layout.toggle_float(&unknown, &w1, None, &[]).unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::PartialObservation)
    );
}
