//! Ports of engine-level update-gaps behaviors onto the retained
//! [`WorldLayout`] endpoint, constructing a [`DomainLayout`] directly.
//!
//! - accepted client-clamp marking at new bounds, without topology churn;
//! - atomic invalid-gap rollback, exception-only adoption and empty retire;
//! - D7 repair focus settle and stale-provenance pruning.

use crate::directional::{Axis, Node, NodeId, OutputId, WorkspaceId};
use crate::session::{DesiredGeometry, ExceptionFlags, RefusalKind};
use crate::world_layout::RetainedReconcileFailure;

use super::super::state::{DomainException, DomainRestoreSlot};
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

fn key_for(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn observed_window(window_id: &str, output: &str, workspace: &str, floating: bool) -> EngineWindow {
    EngineWindow {
        window: window(window_id),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        rect: rect(0, 0, 100, 100),
        floating,
        fit_excluded: floating,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn seeded_domain(bounds: Rect, gap: i32, outer_gap: i32, ids: &[&str]) -> DomainLayout {
    let mut domain = DomainLayout::new(bounds, gap, outer_gap);
    let mut nodes = Vec::new();
    for id in ids {
        let node = leaf(&format!("leaf-{id}"));
        domain.leaves.insert(window(id), node.clone());
        domain.focus_stack.push(node.clone());
        domain.focus_history.push(node.clone());
        nodes.push(Node::Leaf { id: node });
    }
    domain.tree = Some(if nodes.len() == 1 {
        nodes.remove(0)
    } else {
        Node::Group {
            id: leaf("root"),
            axis: Axis::Horizontal,
            children: nodes,
            shares: vec![1; ids.len()],
        }
    });
    domain.focused_leaf = Some(leaf(&format!("leaf-{}", ids[0])));
    domain.last_active = Some(leaf(&format!("leaf-{}", ids[0])));
    domain
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

/// Port of `relocate_moves_seeded_topology_with_outer_gap`: a unique same-workspace
/// source is relocated into the absent target at the requested outer gap,
/// preserving revision and focus history. Every home derives from the new map
/// key: the projected geometry and exception snapshot both name the target.
#[test]
fn reconcile_relocates_source_preserving_revision_history_and_key_derived_home() {
    let source_key = key_for("out", "ws");
    let target_key = key_for("out-2", "ws");
    let bounds = rect(0, 0, 800, 600);
    let mut layout = WorldLayout::new();
    let mut source = seeded_domain(rect(0, 0, 400, 300), 0, 3, &["win-1"]);
    source.accepted_revision = 11;
    source.exceptions.insert(
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
    let history = source.focus_history.clone();
    layout.domains.insert(source_key.clone(), source);

    let observed = vec![
        observed_window("win-1", "out-2", "ws", false),
        observed_window("float", "out-2", "ws", true),
    ];
    let out = layout
        .reconcile_retained(&target_key, bounds, 0, 3, &window("win-1"), &observed, &[])
        .expect("relocation succeeds")
        .expect("relocation, not fresh routing");

    assert_eq!(out.base_revision, 11);
    assert_eq!(out.focus_leaf, Some(leaf("leaf-win-1")));
    assert_eq!(out.geometry.len(), 1, "only the tiled member projects");
    assert_eq!(out.geometry[0].output, OutputId("out-2".to_owned()));
    assert_eq!(out.geometry[0].workspace, WorkspaceId("ws".to_owned()));
    assert!(layout.domain(&source_key).is_none(), "source removed");
    let moved = layout.domain(&target_key).expect("target present");
    assert_eq!(moved.accepted_revision, 11);
    assert_eq!(moved.focus_history, history, "history preserved");
    assert_eq!(moved.outer_gap, 3, "requested outer gap adopted");
    assert_eq!(
        layout
            .exception_record(&target_key, &window("float"))
            .expect("exception homed on target")
            .output,
        OutputId("out-2".to_owned())
    );
}

/// Port of `relocated_target_skew_rejects_partial_observation_with_rollback`:
/// the relocated target missed convergence, so an observation that is not the
/// exact tiled-plus-exception set is refused and the whole source returns.
#[test]
fn reconcile_relocation_exact_set_skew_rolls_back_source() {
    let source_key = key_for("out", "ws");
    let target_key = key_for("out-2", "ws");
    let bounds = rect(0, 0, 800, 600);
    let mut layout = WorldLayout::new();
    let source = seeded_domain(rect(0, 0, 400, 300), 0, 3, &["win-1", "win-2"]);
    layout.domains.insert(source_key.clone(), source);
    let revision = layout.domain(&source_key).unwrap().accepted_revision;

    let observed = vec![observed_window("win-1", "out-2", "ws", false)];
    let err = layout
        .reconcile_retained(&target_key, bounds, 0, 3, &window("win-1"), &observed, &[])
        .unwrap_err();
    assert_eq!(
        err,
        RetainedReconcileFailure::from(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );

    assert!(layout.domain(&target_key).is_none(), "target stays absent");
    let restored = layout.domain(&source_key).expect("source restored");
    assert_eq!(restored.accepted_revision, revision);
    assert!(restored.leaves.contains_key(&window("win-1")));
    assert!(restored.leaves.contains_key(&window("win-2")));
    assert_eq!(restored.outer_gap, 3);
}

/// Outer-layer `domain-mismatch` precedence: a retained target reports the
/// inner-gap mismatch first, then the outer-gap mismatch, leaving state
/// untouched on both refusals.
#[test]
fn reconcile_retained_gap_mismatch_precedes_outer_gap_mismatch() {
    let k = key();
    let mut layout = WorldLayout::new();
    let mut domain = seeded_domain(rect(0, 0, 800, 600), 8, 2, &["win-1"]);
    domain.accepted_revision = 4;
    layout.domains.insert(k.clone(), domain);

    let err = layout
        .reconcile_retained(&k, rect(0, 0, 800, 600), 4, 9, &window("win-1"), &[], &[])
        .unwrap_err();
    assert_eq!(err, RetainedReconcileFailure::InnerGapMismatch);

    let err = layout
        .reconcile_retained(&k, rect(0, 0, 800, 600), 8, 9, &window("win-1"), &[], &[])
        .unwrap_err();
    assert_eq!(err, RetainedReconcileFailure::OuterGapMismatch);

    let retained = layout.domain(&k).expect("domain untouched");
    assert_eq!(retained.gap, 8);
    assert_eq!(retained.outer_gap, 2);
    assert_eq!(retained.accepted_revision, 4);
}

/// Direct exact-source relocation: the can-probe leaves the layout untouched,
/// a commit preserves revision/focus/history and rehomes a minimize slot by map
/// key, and a collision refusal keeps both domains unchanged.
#[test]
fn relocate_domain_probe_nonmutating_commit_preserves_state_collision_keeps_both() {
    let source_key = key_for("out", "ws");
    let target_key = key_for("out-2", "other-ws");
    let bounds = rect(0, 0, 800, 600);
    let mut layout = WorldLayout::new();
    let mut source = seeded_domain(rect(0, 0, 400, 300), 0, 2, &["win-1"]);
    source.accepted_revision = 13;
    source.minimized_slots.insert(
        window("min-1"),
        DomainRestoreSlot {
            leaf: leaf("min-leaf"),
            tree: Node::Leaf {
                id: leaf("min-leaf"),
            },
        },
    );
    let history = source.focus_history.clone();
    let focus = source.focused_leaf.clone();
    layout.domains.insert(source_key.clone(), source);

    // Probe is read-only.
    assert!(layout.can_relocate_domain(&source_key, &target_key, bounds, 0));
    assert!(layout.domain(&source_key).is_some());
    assert!(layout.domain(&target_key).is_none());

    // Commit preserves accepted state and homes the slot by the new key.
    assert!(layout.relocate_domain(&source_key, &target_key, bounds, 0, 2));
    assert!(layout.domain(&source_key).is_none());
    let moved = layout.domain(&target_key).expect("target present");
    assert_eq!(moved.accepted_revision, 13);
    assert_eq!(moved.focused_leaf, focus);
    assert_eq!(moved.focus_history, history);
    assert_eq!(moved.outer_gap, 2);
    assert_eq!(
        layout
            .restore_slot_snapshot(&target_key, &window("min-1"))
            .expect("slot homed on target")
            .workspace,
        WorkspaceId("other-ws".to_owned())
    );

    // A collision refusal keeps both domains exactly as they were.
    let occupied = key_for("out-3", "ws");
    layout.domains.insert(
        occupied.clone(),
        DomainLayout::new(rect(0, 0, 100, 100), 0, 0),
    );
    let before = layout.domain(&target_key).unwrap().clone();
    assert!(!layout.relocate_domain(&target_key, &occupied, bounds, 0, 0));
    assert_eq!(
        layout.domain(&target_key).unwrap().accepted_revision,
        before.accepted_revision
    );
    assert!(layout.domain(&occupied).is_some());
}
