//! Ports of engine-level update-gaps behaviors onto the retained
//! [`WorldLayout`] endpoint, constructing a [`DomainLayout`] directly.
//!
//! - accepted client-clamp marking at new bounds, without topology churn;
//! - atomic invalid-gap rollback, exception-only adoption and empty retire;
//! - D7 repair focus settle and stale-provenance pruning.

use crate::directional::{Axis, Node, NodeId, OutputId, WorkspaceId};
use crate::session::{DesiredGeometry, ExceptionFlags};

use super::super::state::DomainException;
use super::*;

fn key() -> DomainKey {
    DomainKey {
        output: OutputId("out".to_owned()),
        workspace: WorkspaceId("ws".to_owned()),
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn window(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn leaf(id: &str) -> NodeId {
    NodeId(id.to_owned())
}

fn tiled_observed(window_id: &str, frame: Rect, hints: WindowSizeHints) -> EngineWindow {
    let k = key();
    EngineWindow {
        window: window(window_id),
        output: k.output,
        workspace: k.workspace,
        rect: frame,
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints,
    }
}

/// Port of `ghostty_like_short_frame_accepts_client_clamp_without_reassert`:
/// a real carried maximum with the client rendering exactly there is a
/// `client_clamped` flag on the projected desired rect, not a topology change.
#[test]
fn update_gaps_marks_accepted_client_clamp_without_changing_topology() {
    let k = key();
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(rect(0, 0, 1200, 800), 0, 0);
    domain.tree = Some(Node::Group {
        id: leaf("root"),
        axis: Axis::Horizontal,
        children: vec![Node::Leaf { id: leaf("a") }, Node::Leaf { id: leaf("b") }],
        shares: vec![1, 1],
    });
    domain.leaves.insert(window("win-a"), leaf("a"));
    domain.leaves.insert(window("win-b"), leaf("b"));
    domain.focused_leaf = Some(leaf("a"));
    domain.accepted_revision = 5;
    let tree_before = domain.tree.clone();
    layout.domains.insert(k.clone(), domain);

    let win_b_hints = WindowSizeHints {
        max_h: Some(744),
        ..WindowSizeHints::none()
    };
    let observed = vec![
        tiled_observed("win-a", rect(0, 0, 600, 800), WindowSizeHints::none()),
        tiled_observed("win-b", rect(600, 0, 600, 744), win_b_hints),
    ];
    let out = layout
        .update_domain_gaps(&k, rect(0, 0, 1200, 800), 0, 0, &window("win-a"), &observed)
        .expect("projects");

    assert_eq!(out.base_revision, 5, "gap adoption is revision-neutral");
    assert_eq!(out.focus_leaf, Some(leaf("a")));
    assert_eq!(out.geometry.len(), 2);
    let by_window: BTreeMap<&str, &DesiredGeometry> = out
        .geometry
        .iter()
        .map(|g| (g.window.0.as_str(), g))
        .collect();
    assert_eq!(
        by_window["win-b"].rect,
        rect(600, 0, 600, 800),
        "desired truth retained"
    );
    assert!(by_window["win-b"].client_clamped);
    assert!(!by_window["win-b"].overconstrained);
    assert!(!by_window["win-a"].client_clamped);
    assert_eq!(layout.tree(&k), tree_before.as_ref(), "topology untouched");
}

/// Port of the engine `retained_reconcile_empty_retires_slot` and invalid-gap
/// refusal behaviors onto update-gaps: an out-of-range gap refuses without
/// touching state, a valid gap adopts on a no-tree exception-only slot, and a
/// fully empty slot retires while reporting its accepted revision.
#[test]
fn update_gaps_invalid_gap_rolls_back_and_exception_only_adopts_or_retires() {
    let k = key();
    let mut layout = WorldLayout::new();
    let mut domain = DomainLayout::new(rect(0, 0, 800, 600), 8, 2);
    domain.exceptions.insert(
        window("float"),
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
    domain.accepted_revision = 7;
    layout.domains.insert(k.clone(), domain);

    let err = layout
        .update_domain_gaps(
            &k,
            rect(0, 0, 900, 700),
            65,
            9,
            &WindowId(String::new()),
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err,
        LayoutFailure::Semantics(RefusalKind::MalformedTopology)
    );
    let retained = layout.domain(&k).expect("domain survives refusal");
    assert_eq!(retained.bounds, rect(0, 0, 800, 600));
    assert_eq!(retained.gap, 8);
    assert_eq!(retained.outer_gap, 2);
    assert_eq!(retained.accepted_revision, 7);
    assert!(retained.exceptions.contains_key(&window("float")));

    let out = layout
        .update_domain_gaps(
            &k,
            rect(0, 0, 900, 700),
            4,
            9,
            &WindowId(String::new()),
            &[],
        )
        .expect("exception-only slot adopts");
    assert_eq!(out.base_revision, 7);
    assert!(out.geometry.is_empty());
    assert_eq!(out.focus_leaf, None);
    let retained = layout.domain(&k).expect("domain retained");
    assert_eq!(retained.bounds, rect(0, 0, 900, 700));
    assert_eq!(retained.gap, 4);
    assert_eq!(retained.outer_gap, 9);
    assert_eq!(retained.accepted_revision, 7);

    let mut empty = DomainLayout::new(rect(0, 0, 800, 600), 0, 0);
    empty.accepted_revision = 3;
    layout.domains.insert(k.clone(), empty);
    let out = layout
        .update_domain_gaps(
            &k,
            rect(0, 0, 800, 600),
            0,
            0,
            &WindowId(String::new()),
            &[],
        )
        .expect("fully empty slot retires");
    assert_eq!(out.base_revision, 3, "retire reports the retained revision");
    assert!(out.geometry.is_empty());
    assert!(!layout.domains.contains_key(&k));
}

/// D7 repair after a projection failure evicts the newest-first victim,
/// settles focus to the newest surviving MRU, prunes stale capacity/admission
/// provenance, and stays revision-neutral. A five-child 100px-wide group at
/// gap 25 is unprojectable (gap budget leaves no room) and projects again with
/// one victim removed; `ghost` is provenance with no live leaf or exception.
#[test]
fn update_gaps_repair_settles_focus_and_prunes_stale_provenance() {
    let k = key();
    let mut layout = WorldLayout::new();
    let bounds = rect(0, 0, 100, 100);
    let mut domain = DomainLayout::new(bounds, 1, 0);
    domain.tree = Some(Node::Group {
        id: leaf("root"),
        axis: Axis::Horizontal,
        children: (0..5)
            .map(|i| Node::Leaf {
                id: leaf(&format!("l{i}")),
            })
            .collect(),
        shares: vec![1, 1, 1, 1, 1],
    });
    for i in 0..5 {
        domain
            .leaves
            .insert(window(&format!("w{i}")), leaf(&format!("l{i}")));
        domain
            .admission_order
            .insert(window(&format!("w{i}")), i + 1);
    }
    domain.focused_leaf = Some(leaf("l4"));
    domain.focus_stack = (0..5).map(|i| leaf(&format!("l{i}"))).collect();
    domain.capacity_floats.insert(window("ghost"));
    domain.admission_order.insert(window("ghost"), 99);
    domain.accepted_revision = 9;
    layout.domains.insert(k.clone(), domain);

    let observed: Vec<EngineWindow> = (0..5)
        .map(|i| {
            tiled_observed(
                &format!("w{i}"),
                rect(0, 0, 20, 100),
                WindowSizeHints::none(),
            )
        })
        .collect();
    let out = layout
        .update_domain_gaps(&k, bounds, 25, 0, &window("w4"), &observed)
        .expect("D7 repair succeeds");

    assert_eq!(out.base_revision, 9, "repair is revision-neutral");
    assert_eq!(
        out.focus_leaf,
        Some(leaf("l3")),
        "focus settles to newest survivor"
    );
    assert_eq!(out.geometry.len(), 4, "only the four survivors project");

    let retained = layout.domain(&k).expect("domain retained");
    assert_eq!(retained.accepted_revision, 9);
    assert!(
        !retained.leaves.contains_key(&window("w4")),
        "newest victim evicted"
    );
    assert!(retained.exceptions.contains_key(&window("w4")));
    assert_eq!(retained.focused_leaf.as_ref(), Some(&leaf("l3")));
    assert!(retained.capacity_floats.contains(&window("w4")));
    assert!(
        !retained.capacity_floats.contains(&window("ghost")),
        "stale capacity pruned"
    );
    assert!(
        !retained.admission_order.contains_key(&window("ghost")),
        "stale order pruned"
    );
}
