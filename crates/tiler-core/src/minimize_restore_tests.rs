#![forbid(unsafe_code)]
//! D23 shared-core minimize/restore regressions (REQ-MNZ-01/02/03).
//!
//! Drives [`WorldLayout`] observation convergence and retained reconcile, the
//! paths the adapters use for complete observations: a native minimize arrives
//! as an omission, a native restore as a re-observed window.
//!
//! - MNZ-01: minimizing a middle tile releases its allocation, siblings
//!   reflow, focus falls to the MRU window, and the old slot is stored.
//! - MNZ-02: restore returns the window to its stored old slot and the core
//!   writes no focus (the host owns any restore focus).
//! - MNZ-03: minimizing a workspace's sole window retains the workspace with
//!   focus none (must not fail closed) and a stored restore slot.
//! - Best effort (2026-10-13 selection, option (b)): an intervening focus
//!   change, resize, new window, sibling close (with a survivor), swap,
//!   other-sibling minimize/restore roundtrip, or gaps/resolution change
//!   must not alone invalidate the slot: restore rejoins in the surviving
//!   old group at the old index, or regrouped with a live former neighbor
//!   on the stored axis and side with the stored old share, and never
//!   writes focus.
//! - Fresh ordinary admission only when no live former neighbor remains,
//!   the domain is gone, or the window was sent elsewhere. Restart discards
//!   slots (session-only). Reporting a minimized window's close forgets its
//!   slot via `WorldLayout::forget_minimized_slot` / `closed_windows`; a bare
//!   absence from observation stays a minimize.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ResizeMode;
use crate::directional::{
    Axis, Direction, Node, NodeId, OutputId, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::{
    FreshDomainOutcome, LayoutFailure, LocalOperationContext, RetainedProjectionOutcome,
    RetainedReconcileFailure, WorldLayout,
};

fn domain() -> OutputDomain {
    OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn plain(window: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn converge(layout: &mut WorldLayout, names: &[&str], focused: Option<&str>) {
    let windows: Vec<ObservedWindow> = names.iter().map(|name| plain(name)).collect();
    let focused = focused.map(|name| WindowId(name.to_owned()));
    let key = domain().key();
    if !layout.contains_domain(&key) {
        layout
            .converge_fresh_domain(&domain(), 0, &windows, focused.as_ref())
            .expect("converge fresh");
    } else {
        layout
            .converge_domain(&key, &windows, focused.as_ref(), &BTreeSet::new())
            .expect("converge");
    }
}

fn leaf_of(layout: &WorldLayout, window: &str) -> NodeId {
    layout
        .bindings(&domain().key())
        .find(|(w, _)| w.0 == window)
        .unwrap_or_else(|| panic!("window {window} linked"))
        .1
        .clone()
}

fn focused_window(layout: &WorldLayout) -> Option<String> {
    let key = domain().key();
    let leaf = layout.focus_leaf(&key)?;
    layout.window_for_leaf(&key, leaf).map(|w| w.0)
}

/// Seed H-style A/B/C with MRU history A,C,B (B focused).
fn seed_mru() -> WorldLayout {
    let mut layout = WorldLayout::new();
    converge(&mut layout, &["win-a"], Some("win-a"));
    converge(&mut layout, &["win-a", "win-b"], Some("win-b"));
    converge(&mut layout, &["win-a", "win-b", "win-c"], Some("win-c"));
    let key = domain().key();
    assert!(
        layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert_eq!(focused_window(&layout).as_deref(), Some("win-b"));
    layout
}

fn engine_window(name: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(name.to_owned()),
        output: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        rect: Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

enum ReconcileStep {
    Retained(RetainedProjectionOutcome),
    Fresh(FreshDomainOutcome),
}

fn layout_reconcile(
    layout: &mut WorldLayout,
    key: &DomainKey,
    windows: Vec<EngineWindow>,
    focused: &str,
) -> Result<ReconcileStep, RetainedReconcileFailure> {
    let d = domain();
    let focused_id = WindowId(focused.to_owned());
    if layout.contains_domain(key) {
        layout
            .converge_retained_observation(key, &windows, &focused_id, &[])
            .map_err(RetainedReconcileFailure::Layout)?;
        let out = layout
            .reconcile_retained(key, d.bounds, d.gap, 0, &focused_id, &windows, &[])?
            .expect("retained domain");
        Ok(ReconcileStep::Retained(out))
    } else {
        let out = layout
            .reconcile_fresh(&d, 0, &focused_id, &windows)
            .map_err(|err| match err {
                crate::world_layout::FreshDomainFailure::Layout(l) => {
                    RetainedReconcileFailure::Layout(l)
                }
                _ => panic!("unexpected fresh failure: {err:?}"),
            })?;
        Ok(ReconcileStep::Fresh(out))
    }
}

fn layout_reconcile_closed(
    layout: &mut WorldLayout,
    key: &DomainKey,
    windows: Vec<EngineWindow>,
    closed: &[&str],
    focused: &str,
) -> Result<RetainedProjectionOutcome, RetainedReconcileFailure> {
    let d = domain();
    let focused_id = WindowId(focused.to_owned());
    let closed_ids: Vec<WindowId> = closed.iter().map(|n| WindowId((*n).to_owned())).collect();
    layout
        .converge_retained_observation(key, &windows, &focused_id, &closed_ids)
        .map_err(RetainedReconcileFailure::Layout)?;
    let out = layout
        .reconcile_retained(key, d.bounds, d.gap, 0, &focused_id, &windows, &closed_ids)?
        .expect("retained domain");
    Ok(out)
}

/// Seed a sole window, then drive the sole-minimize reconcile leg: the domain
/// is retained (not retired) with an emptied tiling layer, focus none, and a
/// stored restore slot. Returns the layout plus the pre-minimize tree/leaf.
fn sole_minimized_layout() -> (WorldLayout, DomainKey, Node, NodeId) {
    let mut layout = WorldLayout::new();
    converge(&mut layout, &["win-a"], Some("win-a"));
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    let pre_leaf = leaf_of(&layout, "win-a");

    // Sole minimize arrives as explicit empty evidence with focus none.
    // The retire check must retain the slot (not retire it), reporting an
    // empty projection at the converged revision.
    let pre_revision = layout.accepted_revision(&key);
    match layout_reconcile(&mut layout, &key, vec![], "").expect("sole minimize projects empty") {
        ReconcileStep::Retained(plan) => {
            assert!(plan.geometry.is_empty());
            assert_eq!(plan.focus_leaf, None);
            assert_eq!(plan.base_revision, pre_revision + 1);
        }
        other => panic!(
            "sole minimize must project empty, got {:?}",
            other_kind(&other)
        ),
    }
    assert!(
        layout.contains_domain(&key),
        "sole-minimized domain retained, not retired"
    );
    assert!(layout.tree(&key).is_none(), "tiling layer emptied");
    assert_eq!(layout.focus_leaf(&key), None, "focus none");
    assert!(
        layout.has_minimized_slot(&key, &WindowId("win-a".to_owned())),
        "restore slot survives the retire check"
    );
    (layout, key, pre_tree, pre_leaf)
}

fn other_kind(step: &ReconcileStep) -> &'static str {
    match step {
        ReconcileStep::Retained(_) => "Retained",
        ReconcileStep::Fresh(_) => "Fresh",
    }
}

#[test]
fn mnz_01_minimize_middle_releases_allocation_with_mru_focus_and_stored_slot() {
    let mut layout = seed_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    let pre_b_leaf = leaf_of(&layout, "win-b");

    // Native minimize B arrives as an omission; C is the MRU survivor.
    converge(&mut layout, &["win-a", "win-c"], Some("win-c"));

    let mut leaves: Vec<String> = layout.tiled_windows(&key).map(|w| w.0.clone()).collect();
    leaves.sort();
    assert_eq!(leaves, vec!["win-a".to_owned(), "win-c".to_owned()]);
    assert!(layout.tree(&key).is_some(), "domain retains survivors");
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "focus falls to MRU survivor"
    );
    let slot = layout
        .restore_slot_snapshot(&key, &WindowId("win-b".to_owned()))
        .expect("old slot stored");
    assert_eq!(slot.leaf, pre_b_leaf);
    assert_eq!(slot.tree, pre_tree);
}

#[test]
fn mnz_02_restore_returns_to_old_slot_with_no_focus_write() {
    let mut layout = seed_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    let pre_b_leaf = leaf_of(&layout, "win-b");
    converge(&mut layout, &["win-a", "win-c"], Some("win-c"));

    // Native restore re-observes B; the host natively focuses B, but the
    // core must not adopt that focus write.
    converge(&mut layout, &["win-a", "win-b", "win-c"], Some("win-b"));

    let restored_tree = layout.tree(&key).expect("tree").clone();
    assert_eq!(
        restored_tree, pre_tree,
        "restore returns to the stored old slot"
    );
    assert_eq!(leaf_of(&layout, "win-b"), pre_b_leaf);
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );
}

#[test]
fn mnz_03_sole_minimize_retains_workspace_with_focus_none_and_stored_slot() {
    let mut layout = WorldLayout::new();
    let key = domain().key();
    converge(&mut layout, &["win-a"], Some("win-a"));
    let pre_tree = layout.tree(&key).expect("tree").clone();
    let pre_leaf = leaf_of(&layout, "win-a");

    // Sole-window minimize must not fail closed: the workspace is retained
    // empty with focus none and a stored restore slot.
    converge(&mut layout, &[], None);

    assert_eq!(layout.domain_keys().len(), 1, "workspace retained");
    assert!(layout.tree(&key).is_none(), "tiling layer emptied");
    assert_eq!(layout.focus_leaf(&key), None, "focus none");
    let slot = layout
        .restore_slot_snapshot(&key, &WindowId("win-a".to_owned()))
        .expect("restore slot stored");
    assert_eq!(slot.tree, pre_tree);
    assert_eq!(slot.leaf, pre_leaf);

    // Restore returns the sole window to its old slot, still with no core
    // focus write (the host owns restore focus).
    converge(&mut layout, &["win-a"], Some("win-a"));
    let restored = layout.tree(&key).expect("tree").clone();
    assert_eq!(restored, pre_tree);
    assert_eq!(layout.focus_leaf(&key), None);
}

#[test]
fn mnz_03_sole_minimize_retained_through_engine_reconcile_and_restores_old_slot() {
    let (mut layout, key, pre_tree, pre_leaf) = sole_minimized_layout();

    // A later native restore re-observes the sole window (host-focused, as
    // on the realistic taskbar path). The stored-slot converge must win
    // over the all-new-IDs fresh-admission route: the tree and leaf come
    // back exactly, the core still writes no focus, and the slot is
    // consumed.
    let step = layout_reconcile(&mut layout, &key, vec![engine_window("win-a")], "win-a")
        .expect("sole restore must project old-slot geometry");
    let restored_tree = layout.tree(&key).expect("tree").clone();
    assert_eq!(
        restored_tree, pre_tree,
        "engine-path restore returns to the stored old slot, not fresh admission"
    );
    assert_eq!(leaf_of(&layout, "win-a"), pre_leaf);
    assert_eq!(layout.focus_leaf(&key), None, "core writes no focus");
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-a".to_owned())),
        "slot consumed by restore"
    );
    match step {
        ReconcileStep::Retained(plan) => {
            assert_eq!(plan.geometry.len(), 1);
            assert_eq!(
                plan.geometry[0].window,
                WindowId("win-a".to_owned()),
                "restored tile projected"
            );
            assert_eq!(plan.focus_leaf, None, "no focus fabricated");
            assert_eq!(plan.base_revision, layout.accepted_revision(&key));
        }
        other => panic!(
            "sole restore must project old-slot geometry, got {:?}",
            other_kind(&other)
        ),
    }
}

#[test]
fn mnz_sole_restore_with_unknown_focus_still_reports_mismatch() {
    let (mut layout, key, pre_tree, _) = sole_minimized_layout();

    // The same restore observation, but the observed focus names a window
    // that is not a converged tile: the converged old-slot state still
    // persists, yet the reconcile projection reports the genuine focus mismatch.
    let err = layout_reconcile(&mut layout, &key, vec![engine_window("win-a")], "win-ghost");
    match err {
        Err(RetainedReconcileFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch,
        ))) => {}
        Ok(other) => panic!(
            "unknown-focus restore must report focus-mismatch, got {}",
            other_kind(&other)
        ),
        Err(other) => panic!("unknown-focus restore must report focus-mismatch, got {other:?}"),
    }
    let restored_tree = layout.tree(&key).expect("tree").clone();
    assert_eq!(
        restored_tree, pre_tree,
        "converged old-slot state persists past the refusal"
    );
    assert_eq!(layout.focus_leaf(&key), None);
}

/// Window names in deterministic tree leaf order for one-domain layouts.
fn leaf_window_order(layout: &WorldLayout) -> Vec<String> {
    let key = domain().key();
    let tree = layout.tree(&key).expect("tree");
    let mut leaves = Vec::new();
    collect_test_leaves(tree, &mut leaves);
    leaves
        .into_iter()
        .map(|leaf| {
            layout
                .window_for_leaf(&key, &leaf)
                .unwrap_or_else(|| panic!("leaf {leaf:?} linked"))
                .0
        })
        .collect()
}

fn collect_test_leaves(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect_test_leaves(child, out);
            }
        }
    }
}

/// Root group shares for one-domain layouts (`None` for a root leaf).
fn root_shares(layout: &WorldLayout) -> Option<Vec<u64>> {
    match layout.tree(&domain().key()).expect("tree") {
        Node::Group { shares, .. } => Some(shares.clone()),
        Node::Leaf { .. } => None,
    }
}

fn complete_engine_obs(layout: &WorldLayout, key: &DomainKey) -> Vec<EngineWindow> {
    let mut windows: Vec<EngineWindow> = layout
        .tiled_windows(key)
        .map(|w| engine_window(&w.0))
        .collect();
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows
}

/// Committed keyboard resize of the focused tiled window (survivor shares
/// change; topology and membership do not).
fn resize_committed(layout: &mut WorldLayout, window: &str, direction: Direction) {
    let d = domain();
    let key = d.key();
    assert!(
        layout.sync_focus_from_window(&key, &WindowId(window.to_owned())),
        "sync focus to resized {window}"
    );
    let obs = complete_engine_obs(layout, &key);
    let subject = WindowId(window.to_owned());
    let ctx = LocalOperationContext {
        domain: &d,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &subject,
        observed: &obs,
        preconverged: true,
    };
    layout
        .run_keyboard_resize(&ctx, &subject, direction, ResizeMode::Outwards, 0)
        .unwrap_or_else(|e| panic!("resize {window:?}: {e:?}"));
}

/// Committed directional move of a tiled window (order/shape change with
/// surviving membership).
fn move_committed(layout: &mut WorldLayout, window: &str, direction: Direction) {
    let d = domain();
    let key = d.key();
    assert!(
        layout.sync_focus_from_window(&key, &WindowId(window.to_owned())),
        "sync focus to mover {window}"
    );
    let obs = complete_engine_obs(layout, &key);
    let subject = WindowId(window.to_owned());
    let ctx = LocalOperationContext {
        domain: &d,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &subject,
        observed: &obs,
        preconverged: true,
    };
    layout
        .run_local_move(&ctx, &subject, direction, SameAxisMove::GroupWithNeighbor)
        .unwrap_or_else(|e| panic!("move {window:?}: {e:?}"));
}

/// Committed pointer resize of a tiled window to an absolute work-area
/// boundary (exact pixel shares; focus retained, topology unchanged).
fn pointer_resize_committed(
    layout: &mut WorldLayout,
    window: &str,
    direction: Direction,
    boundary: i32,
) {
    let d = domain();
    let key = d.key();
    let obs = complete_engine_obs(layout, &key);
    let subject = WindowId(window.to_owned());
    let ctx = LocalOperationContext {
        domain: &d,
        outer_gap: 0,
        resize: crate::policy::ScaledResize::unscaled(),
        observed_focus: &subject,
        observed: &obs,
        preconverged: true,
    };
    layout
        .run_pointer_resize(&ctx, &subject, direction, boundary)
        .unwrap_or_else(|e| panic!("pointer resize {window:?}: {e:?}"));
}

/// Immediate parent group axis/shares of a window's leaf in the one-domain tree.
fn parent_group_of(layout: &WorldLayout, win: &str) -> (Axis, Vec<u64>) {
    let key = domain().key();
    let leaf = layout
        .leaves(&key)
        .and_then(|leaves| leaves.get(&WindowId(win.to_owned())))
        .expect("leaf for window");
    let tree = layout.tree(&key).expect("tree");
    find_parent_test(tree, leaf).expect("parent group")
}

fn find_parent_test(node: &Node, leaf: &NodeId) -> Option<(Axis, Vec<u64>)> {
    match node {
        Node::Leaf { .. } => None,
        Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            if children.iter().any(|child| match child {
                Node::Leaf { id } => id == leaf,
                Node::Group { .. } => false,
            }) {
                return Some((*axis, shares.clone()));
            }
            children
                .iter()
                .find_map(|child| find_parent_test(child, leaf))
        }
    }
}

/// Seed H-style A/B/C with unequal shares and MRU history A,C,B (B focused):
/// the matrix Given for every restore leg.
fn seed_unequal_mru() -> WorldLayout {
    let mut layout = WorldLayout::new();
    converge(&mut layout, &["win-a"], Some("win-a"));
    converge(&mut layout, &["win-a", "win-b"], Some("win-b"));
    converge(&mut layout, &["win-a", "win-b", "win-c"], Some("win-c"));
    resize_committed(&mut layout, "win-c", Direction::Left);
    let key = domain().key();
    assert!(
        layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert!(
        root_shares(&layout).is_some_and(|shares| shares != vec![1, 1]),
        "Given carries unequal shares"
    );
    assert_eq!(focused_window(&layout).as_deref(), Some("win-b"));
    layout
}

/// Minimize B from the unequal seed; the MRU survivor C keeps focus.
fn minimize_b(layout: &mut WorldLayout) {
    converge(layout, &["win-a", "win-c"], Some("win-c"));
    assert!(
        layout.has_minimized_slot(&domain().key(), &WindowId("win-b".to_owned())),
        "B slot stored"
    );
    assert_eq!(
        focused_window(layout).as_deref(),
        Some("win-c"),
        "MRU survivor keeps focus"
    );
}

/// Native restore: re-observe B (host-focused) and expect no core focus
/// write plus a consumed slot.
fn restore_b(layout: &mut WorldLayout) {
    restore_b_with(layout, &["win-a", "win-b", "win-c"]);
}

/// Native restore over an explicit observed set (for legs with extra
/// survivors such as an admitted newcomer).
fn restore_b_with(layout: &mut WorldLayout, names: &[&str]) {
    let focused = Some(WindowId("win-b".to_owned()));
    let windows: Vec<ObservedWindow> = names.iter().map(|name| plain(name)).collect();
    let key = domain().key();
    layout
        .converge_domain(&key, &windows, focused.as_ref(), &BTreeSet::new())
        .expect("converge");
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );
}

#[test]
fn mnz_best_effort_focus_change_rejoins_with_no_focus_write() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    minimize_b(&mut layout);

    // Focus A instead of C: focus alone changes no topology.
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-a".to_owned())));
    restore_b(&mut layout);

    assert_eq!(
        layout.tree(&key).expect("tree"),
        &pre_tree,
        "focus alone rejoins the old neighborhood exactly"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_resize_rejoins_beside_former_neighbor() {
    let mut layout = seed_unequal_mru();
    minimize_b(&mut layout);

    // Intervening resize changes survivor shares but removes no neighbor.
    resize_committed(&mut layout, "win-c", Direction::Left);
    let current_shares = root_shares(&layout).expect("root group");
    assert_eq!(current_shares.len(), 2, "survivors share one group");
    restore_b(&mut layout);

    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-a".to_owned(), "win-b".to_owned(), "win-c".to_owned()],
        "B rejoins between its former neighbors, not fresh after C"
    );
    assert_eq!(
        root_shares(&layout).expect("root group"),
        current_shares,
        "regroup keeps the survivor slot shares"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_new_window_rejoins_former_neighborhood() {
    let mut layout = seed_unequal_mru();
    minimize_b(&mut layout);

    // A new window wraps the focused survivor; former neighbors survive.
    converge(&mut layout, &["win-a", "win-c", "win-d"], Some("win-d"));
    restore_b_with(&mut layout, &["win-a", "win-b", "win-c", "win-d"]);

    assert_eq!(
        leaf_window_order(&layout),
        vec![
            "win-a".to_owned(),
            "win-b".to_owned(),
            "win-c".to_owned(),
            "win-d".to_owned()
        ],
        "B rejoins beside former neighbor C, not fresh beside newcomer D"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-d"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_sibling_close_rejoins_by_survivor() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    minimize_b(&mut layout);

    // Close A, retaining C: B must rejoin by its surviving former neighbor
    // on its old side (before C), not fresh after C.
    converge(&mut layout, &["win-c"], Some("win-c"));
    assert!(
        layout.has_minimized_slot(&key, &WindowId("win-a".to_owned())),
        "closed sibling carries its own slot"
    );
    converge(&mut layout, &["win-b", "win-c"], Some("win-b"));
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-b".to_owned(), "win-c".to_owned()],
        "B rejoins before surviving neighbor C (old side)"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, vec![1, 1]),
        "sole-survivor wrap keeps the stored axis with the stored old shares"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_swap_rejoins_neighborhood() {
    let mut layout = seed_unequal_mru();
    minimize_b(&mut layout);

    // Swap the survivors: the order change alone must not invalidate.
    move_committed(&mut layout, "win-c", Direction::Left);
    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-c".to_owned(), "win-a".to_owned()],
        "swap leg precondition"
    );
    restore_b(&mut layout);

    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-b".to_owned(), "win-c".to_owned(), "win-a".to_owned()],
        "B rejoins beside former neighbor C on its old side"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, vec![1, 1]),
        "B regroups with former sibling C carrying the stored old shares"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

/// Seed H-style A/B/C with a skewed old B share (pointer-exact V split,
/// B outweighs C) and MRU history A,C,B (B focused).
fn seed_skewed_mru() -> WorldLayout {
    let mut layout = WorldLayout::new();
    converge(&mut layout, &["win-a"], Some("win-a"));
    converge(&mut layout, &["win-a", "win-b"], Some("win-b"));
    converge(&mut layout, &["win-a", "win-b", "win-c"], Some("win-c"));
    pointer_resize_committed(&mut layout, "win-b", Direction::Down, 360);
    let key = domain().key();
    assert!(
        layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())),
        "sync focus to win-b"
    );
    assert_eq!(focused_window(&layout).as_deref(), Some("win-b"));
    layout
}

/// Stored old parent axis/shares of win-b from its minimize slot.
fn stored_old_group(layout: &WorldLayout) -> (Axis, Vec<u64>) {
    let slot = layout
        .restore_slot_snapshot(&domain().key(), &WindowId("win-b".to_owned()))
        .expect("B slot stored");
    find_parent_test(&slot.tree, &slot.leaf).expect("stored parent group")
}

#[test]
fn mnz_best_effort_skewed_share_regroups_stored_fraction() {
    let mut layout = seed_skewed_mru();
    minimize_b(&mut layout);
    let (old_axis, old_shares) = stored_old_group(&layout);
    assert_eq!(old_axis, Axis::Vertical);
    assert!(
        old_shares[0] > old_shares[1],
        "skewed Given carries B outweighing C: {old_shares:?}"
    );

    // Intervening root resize breaks the exact fast path only.
    resize_committed(&mut layout, "win-c", Direction::Left);
    let current_shares = root_shares(&layout).expect("root group");
    restore_b(&mut layout);

    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-a".to_owned(), "win-b".to_owned(), "win-c".to_owned()],
        "B rejoins between its former neighbors"
    );
    assert_eq!(
        root_shares(&layout).expect("root group"),
        current_shares,
        "regroup keeps the resized survivor slot shares"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, old_shares),
        "regroup carries the stored old shares verbatim, never an average"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_skewed_wrap_preserves_old_ratio() {
    let mut layout = seed_skewed_mru();
    let key = domain().key();
    minimize_b(&mut layout);
    let (old_axis, old_shares) = stored_old_group(&layout);
    assert_eq!(old_axis, Axis::Vertical);
    assert!(
        old_shares[0] > old_shares[1],
        "skewed Given carries B outweighing C: {old_shares:?}"
    );

    // Close A: the sole survivor is the root leaf, so restore wraps it on
    // the stored axis with the stored ratio, not equal halves.
    converge(&mut layout, &["win-c"], Some("win-c"));
    converge(&mut layout, &["win-b", "win-c"], Some("win-b"));
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-b".to_owned(), "win-c".to_owned()],
        "B rejoins before surviving neighbor C (old side)"
    );
    assert_eq!(
        parent_group_of(&layout, "win-b"),
        (Axis::Vertical, old_shares),
        "wrap preserves the stored old ratio, never equal halves"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_surviving_group_inserts_at_old_index() {
    let mut layout = seed_skewed_mru();
    // Grow the old parent to three children: the R1 wrap plus the R2b
    // same-axis append keep the original group id.
    move_committed(&mut layout, "win-a", Direction::Down);
    move_committed(&mut layout, "win-a", Direction::Up);
    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-b".to_owned(), "win-c".to_owned(), "win-a".to_owned()],
        "three-child old group precondition"
    );
    // A newcomer nests inside the old group around C; the group id survives.
    let key = domain().key();
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-c".to_owned())));
    converge(
        &mut layout,
        &["win-a", "win-b", "win-c", "win-d"],
        Some("win-d"),
    );
    assert!(layout.sync_focus_from_window(&key, &WindowId("win-b".to_owned())));
    converge(&mut layout, &["win-a", "win-c", "win-d"], Some("win-c"));
    let (stored_axis, stored_shares) = stored_old_group(&layout);
    assert_eq!(stored_axis, Axis::Vertical);
    assert_eq!(
        stored_shares.len(),
        3,
        "stored old group carries three children: {stored_shares:?}"
    );
    // An intervening root resize breaks the exact fast path while the old
    // group id, axis, and survivors stay live (keyboard resize preserves
    // the pair total, so the comparable entrant equals the stored share).
    resize_committed(&mut layout, "win-a", Direction::Up);
    let current_shares = root_shares(&layout).expect("root group");
    assert_ne!(
        current_shares,
        stored_shares[1..].to_vec(),
        "exact fast path is broken by the resize"
    );

    converge(
        &mut layout,
        &["win-a", "win-b", "win-c", "win-d"],
        Some("win-b"),
    );
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "slot consumed by restore"
    );

    assert_eq!(
        leaf_window_order(&layout),
        vec![
            "win-b".to_owned(),
            "win-c".to_owned(),
            "win-d".to_owned(),
            "win-a".to_owned()
        ],
        "B rejoins at its old index in the surviving old group"
    );
    // The comparable entrant equals the stored old share here (the keyboard
    // resize preserves the pair total); a proportional average would admit
    // 269 instead of 359.
    let mut expected = vec![stored_shares[0]];
    expected.extend_from_slice(&current_shares);
    assert_eq!(
        root_shares(&layout).expect("root group"),
        expected,
        "surviving-group insert keeps the stored old share with live survivors"
    );
    let leaf_b = layout
        .leaves(&key)
        .and_then(|leaves| leaves.get(&WindowId("win-b".to_owned())))
        .expect("win-b leaf");
    match layout.tree(&key).expect("tree") {
        Node::Group { children, .. } => assert!(
            children[0].id() == leaf_b,
            "B is a direct child at the old index, not nested"
        ),
        Node::Leaf { .. } => panic!("root group expected"),
    }
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_other_minimize_restore_roundtrip_rejoins() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    minimize_b(&mut layout);

    // An interleaved sibling minimize/restore roundtrip leaves B's
    // neighborhood intact.
    converge(&mut layout, &["win-a"], Some("win-a"));
    converge(&mut layout, &["win-a", "win-c"], Some("win-c"));
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-c".to_owned())),
        "sibling slot consumed by its own restore"
    );
    restore_b(&mut layout);

    assert_eq!(
        layout.tree(&key).expect("tree"),
        &pre_tree,
        "interleaved sibling roundtrip rejoins the old neighborhood"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-a"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_best_effort_gaps_change_rejoins_with_current_projection() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    minimize_b(&mut layout);

    // Gaps/resolution changes reproject; topology is untouched.
    let obs = complete_engine_obs(&layout, &key);
    layout
        .update_domain_gaps(
            &key,
            Rect {
                x: 0,
                y: 0,
                w: 1024,
                h: 768,
            },
            8,
            0,
            &WindowId("win-c".to_owned()),
            &obs,
        )
        .expect("gap update applies");
    restore_b(&mut layout);

    assert_eq!(
        layout.tree(&key).expect("tree"),
        &pre_tree,
        "gaps change rejoins the old neighborhood"
    );
    assert_eq!(
        focused_window(&layout).as_deref(),
        Some("win-c"),
        "core writes no focus on restore"
    );
}

#[test]
fn mnz_session_restart_discards_slots_into_fresh_admission() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    minimize_b(&mut layout);
    assert_eq!(layout.minimized_count(&key), 1);

    // Restart: a fresh WorldLayout starts with no restore context.
    let mut restarted = WorldLayout::new();
    assert_eq!(restarted.minimized_count(&key), 0);
    converge(&mut restarted, &["win-a", "win-c"], Some("win-c"));
    converge(&mut restarted, &["win-a", "win-b", "win-c"], Some("win-b"));

    assert!(
        restarted.is_tiled_window(&key, &WindowId("win-b".to_owned())),
        "win-b admitted fresh after restart"
    );
    assert_eq!(
        restarted.minimized_count(&key),
        0,
        "no slot survives a restart"
    );
    let _ = layout;
}

#[test]
fn mnz_close_while_minimized_forgets_slot_into_fresh_admission() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    let pre_tree = layout.tree(&key).expect("tree").clone();
    minimize_b(&mut layout);

    // Explicit close (not minimize): the stored slot is forgotten.
    assert!(
        layout.forget_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "close forgets the minimized slot"
    );
    assert!(!layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())));
    assert!(
        !layout.forget_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "repeat forget is false"
    );

    // A later same-id observation is a brand-new window: fresh ordinary
    // admission beside the focused survivor, never the old slot.
    converge(&mut layout, &["win-a", "win-b", "win-c"], Some("win-b"));
    assert_ne!(
        layout.tree(&key).expect("tree"),
        &pre_tree,
        "forgotten slot falls back to fresh admission"
    );
    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-a".to_owned(), "win-c".to_owned(), "win-b".to_owned()],
        "fresh admission wraps the focused survivor"
    );
}

#[test]
fn mnz_close_sole_minimized_allows_session_retire() {
    let mut layout = WorldLayout::new();
    let key = domain().key();
    converge(&mut layout, &["win-a"], Some("win-a"));
    converge(&mut layout, &[], None);
    assert_eq!(
        layout.minimized_count(&key),
        1,
        "minimized slot retains the empty domain"
    );

    assert!(
        layout.forget_minimized_slot(&key, &WindowId("win-a".to_owned())),
        "close forgets the sole slot"
    );
    assert_eq!(
        layout.minimized_count(&key),
        0,
        "no live slot remains, the empty domain may retire"
    );
}

#[test]
fn mnz_fresh_fallback_absent_neighbors() {
    let mut layout = seed_unequal_mru();
    let key = domain().key();
    minimize_b(&mut layout);

    // Close every former neighbor: no survivor remains, so restore falls
    // back to fresh ordinary admission on the emptied domain.
    converge(&mut layout, &["win-c"], Some("win-c"));
    converge(&mut layout, &[], None);
    converge(&mut layout, &["win-b"], Some("win-b"));

    assert!(
        layout.is_tiled_window(&key, &WindowId("win-b".to_owned())),
        "win-b admitted"
    );
    assert_eq!(
        leaf_window_order(&layout),
        vec!["win-b".to_owned()],
        "sole survivor on the emptied domain"
    );
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "stale slot consumed"
    );
}

#[test]
fn mnz_fresh_fallback_domain_gone() {
    let (mut layout, key, _, _) = sole_minimized_layout();

    // The domain is gone: drop removes the retained minimized domain, so
    // the next observation seeds a fresh domain with no restore context.
    layout.drop_domain(&key);
    assert!(!layout.contains_domain(&key), "domain released");
    let step = layout_reconcile(&mut layout, &key, vec![engine_window("win-a")], "win-a")
        .expect("released domain must seed fresh");
    match step {
        ReconcileStep::Fresh(out) => {
            assert_eq!(
                out.projection.geometry.len(),
                1,
                "fresh admit projects one tile"
            )
        }
        other => panic!(
            "released domain must seed fresh, got {}",
            other_kind(&other)
        ),
    }
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-a".to_owned())),
        "no slot survives domain release"
    );
}

fn homed(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

#[test]
fn mnz_fresh_fallback_sent_elsewhere() {
    let ws1 = domain();
    let ws2 = OutputDomain {
        id: OutputId("out-1".to_owned()),
        workspace: WorkspaceId("ws-2".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    let ws1_key = ws1.key();
    let ws2_key = ws2.key();
    let mut layout = WorldLayout::new();
    layout
        .converge_fresh_domain(
            &ws1,
            0,
            &[
                homed("win-a", "out-1", "ws-1"),
                homed("win-b", "out-1", "ws-1"),
                homed("win-c", "out-1", "ws-1"),
            ],
            Some(&WindowId("win-b".to_owned())),
        )
        .expect("seed ws-1");
    layout
        .converge_fresh_domain(&ws2, 0, &[homed("win-d", "out-1", "ws-2")], None)
        .expect("seed ws-2");

    // Minimize B on ws-1; ws-2 stays complete.
    layout
        .converge_domain(
            &ws1_key,
            &[
                homed("win-a", "out-1", "ws-1"),
                homed("win-c", "out-1", "ws-1"),
            ],
            Some(&WindowId("win-c".to_owned())),
            &BTreeSet::new(),
        )
        .expect("minimize B on ws-1");
    assert!(
        layout.has_minimized_slot(&ws1_key, &WindowId("win-b".to_owned())),
        "B slot stored on ws-1"
    );

    // B reappears on ws-2 (sent elsewhere): the ws-1 slot is forgotten and
    // B takes fresh ordinary admission on ws-2.
    layout.forget_minimized_slot(&ws1_key, &WindowId("win-b".to_owned()));
    layout
        .converge_domain(
            &ws2_key,
            &[
                homed("win-b", "out-1", "ws-2"),
                homed("win-d", "out-1", "ws-2"),
            ],
            Some(&WindowId("win-b".to_owned())),
            &BTreeSet::new(),
        )
        .expect("admit B on ws-2");
    assert!(
        layout.is_tiled_window(&ws2_key, &WindowId("win-b".to_owned())),
        "win-b linked on ws-2"
    );
    assert!(
        !layout.has_minimized_slot(&ws1_key, &WindowId("win-b".to_owned())),
        "stale slot consumed"
    );
    assert_eq!(layout.tiled_count(&ws1_key), 2, "ws-1 keeps its survivors");
}

/// Seed H-style A/B/C through retained reconcile legs (B focused).
fn triple_layout() -> (WorldLayout, DomainKey) {
    let key = domain().key();
    let mut layout = WorldLayout::new();
    for (names, focused) in [
        (vec!["win-a"], "win-a"),
        (vec!["win-a", "win-b"], "win-b"),
        (vec!["win-a", "win-b", "win-c"], "win-b"),
    ] {
        let windows = names.iter().map(|name| engine_window(name)).collect();
        layout_reconcile(&mut layout, &key, windows, focused).expect("seed leg must plan");
    }
    assert!(layout.contains_domain(&key), "triple retained");
    (layout, key)
}

fn layout_windows(layout: &WorldLayout, key: &DomainKey) -> Vec<String> {
    let mut names: Vec<String> = layout.tiled_windows(key).map(|w| w.0.clone()).collect();
    names.sort();
    names
}

#[test]
fn mnz_close_minimized_via_engine_forgets_slot_without_readmit() {
    let (mut layout, key) = triple_layout();

    // Minimize B as a bare omission: the old slot is stored.
    layout_reconcile(
        &mut layout,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        "win-c",
    )
    .expect("minimize leg must plan");
    assert!(
        layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "minimize stores the old slot"
    );

    // The minimized window closes: same omission plus the explicit close
    // report forgets the slot without readmitting the window.
    layout_reconcile_closed(
        &mut layout,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        &["win-b"],
        "win-c",
    )
    .expect("close leg must plan");
    assert!(layout.contains_domain(&key), "survivors retain the domain");
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "explicit close forgets the minimized slot"
    );
    assert_eq!(
        layout_windows(&layout, &key),
        vec!["win-a".to_owned(), "win-c".to_owned()],
        "closed window is not readmitted"
    );
}

#[test]
fn mnz_ordinary_close_mints_no_slot() {
    let (mut layout, key) = triple_layout();

    // A visible tiled window closes in the same turn it disappears: the
    // omission plus the close report must never mint a slot for it.
    layout_reconcile_closed(
        &mut layout,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        &["win-b"],
        "win-c",
    )
    .expect("ordinary close must plan");
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "ordinary close mints no slot"
    );
    assert_eq!(
        layout_windows(&layout, &key),
        vec!["win-a".to_owned(), "win-c".to_owned()],
        "survivors keep membership"
    );

    // A later same-id window is a brand-new admission, never an old-slot
    // restore.
    layout_reconcile(
        &mut layout,
        &key,
        vec![
            engine_window("win-a"),
            engine_window("win-b"),
            engine_window("win-c"),
        ],
        "win-b",
    )
    .expect("re-admission must plan");
    assert!(
        !layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "no slot lingers after fresh re-admission"
    );
}

#[test]
fn mnz_close_sole_minimized_retires_session_via_engine() {
    let (mut layout, key, _, _) = sole_minimized_layout();
    assert!(
        layout.contains_domain(&key),
        "sole-minimized domain retained"
    );

    // The sole minimized window closes: the slot is forgotten and the
    // emptied domain retires on the same turn instead of lingering.
    let d = domain();
    let closed = vec![WindowId("win-a".to_owned())];
    layout
        .converge_retained_observation(&key, &[], &WindowId(String::new()), &closed)
        .expect("converge sole close");
    layout
        .reconcile_retained(
            &key,
            d.bounds,
            d.gap,
            0,
            &WindowId(String::new()),
            &[],
            &closed,
        )
        .expect("sole close must plan");
    assert!(
        !layout.contains_domain(&key),
        "slot-only domain retires once its sole slot is forgotten"
    );
}

#[test]
fn mnz_close_cleanup_fenced_on_diverged_observation() {
    let (mut layout, key) = triple_layout();
    layout_reconcile(
        &mut layout,
        &key,
        vec![engine_window("win-a"), engine_window("win-c")],
        "win-c",
    )
    .expect("minimize leg must plan");
    assert!(
        layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "minimize stores the old slot"
    );

    // A close report on a failing observation (e.g. duplicate window id)
    // must not forget the slot: the caller has to retry with the close retained.
    let failed = layout.converge_retained_observation(
        &key,
        &[engine_window("win-a"), engine_window("win-a")],
        &WindowId("win-c".to_owned()),
        &[WindowId("win-b".to_owned())],
    );
    assert!(failed.is_err(), "invalid observation fails, got {failed:?}");
    assert!(
        layout.has_minimized_slot(&key, &WindowId("win-b".to_owned())),
        "failed close report retains the slot for retry"
    );
}
