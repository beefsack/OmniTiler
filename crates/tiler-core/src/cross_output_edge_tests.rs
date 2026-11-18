#![forbid(unsafe_code)]
//! Cross-output entry-edge focus selection (shared core).
//!
//! Failing-first regressions for the edge-based destination selector:
//! candidates are the destination tiled leaves whose canonical projected
//! allocation touches the entry edge; ranking is most-recently-focused
//! among candidates via the workspace-core MRU history, falling back to
//! largest perpendicular overlap with the source span, then DFS child
//! order. Source/target spans are canonical allocations, never observed
//! native frames.

use crate::directional::{Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind,
};
use crate::world_layout::{
    LayoutFailure, PairAssemblyFailure, PairOperationContext, PairOperationFailure, WorldLayout,
    rank_cross_output_edge_candidates,
};
use std::collections::{BTreeMap, BTreeSet};

fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: crate::size_hints::WindowSizeHints::none(),
    }
}
fn engine_row(w: &str, o: &str, ws: &str, rect: Rect) -> EngineWindow {
    EngineWindow {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect,
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
fn wide_domains() -> (OutputDomain, OutputDomain) {
    let mut o1 = OutputDomain {
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
    };
    let mut o2 = OutputDomain {
        id: OutputId("out-2".to_owned()),
        workspace: WorkspaceId("ws-1".to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        gap: 0,
        adjacent: BTreeMap::new(),
    };
    o1.adjacent
        .insert(Direction::Right, OutputId("out-2".to_owned()));
    o2.adjacent
        .insert(Direction::Left, OutputId("out-1".to_owned()));
    (o1, o2)
}
fn tall_domains() -> (OutputDomain, OutputDomain) {
    let (mut o1, mut o2) = wide_domains();
    for d in [&mut o1, &mut o2] {
        d.bounds = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 600,
        };
    }
    (o1, o2)
}
fn wide_placement() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    }
}
fn tall_placement() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 300,
        h: 400,
    }
}
fn complete_observed_for(
    layout: &WorldLayout,
    k: &DomainKey,
    extra: Vec<ObservedWindow>,
) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(k)
        .map(|w| tiled(&w.0, &k.output.0, &k.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(k) {
        let mut entry = tiled(&w.0, &k.output.0, &k.workspace.0);
        entry.floating = flags.floating;
        entry.fullscreen = flags.fullscreen;
        entry.maximized = flags.maximized;
        entry.sticky = flags.sticky;
        entry.fixed_auto = layout.is_automatic_fixed_float(k, &w);
        windows.push(entry);
    }
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}
fn complete_engine_for_pair(
    layout: &WorldLayout,
    k1: &DomainKey,
    k2: &DomainKey,
) -> Vec<EngineWindow> {
    let mut windows = Vec::new();
    for k in [k1, k2] {
        for w in layout.tiled_windows(k) {
            windows.push(engine_row(
                &w.0,
                &k.output.0,
                &k.workspace.0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
            ));
        }
        for (w, flags) in layout.exception_windows_with_flags(k) {
            let rect = layout
                .retained_float_geometry(k, &w)
                .or_else(|| layout.exception_float_geometry(k, &w))
                .unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                });
            let mut entry = engine_row(&w.0, &k.output.0, &k.workspace.0, rect);
            entry.floating = flags.floating;
            entry.fullscreen = flags.fullscreen;
            entry.maximized = flags.maximized;
            entry.sticky = flags.sticky;
            entry.fixed_auto = layout.is_automatic_fixed_float(k, &w);
            windows.push(entry);
        }
    }
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    windows
}
fn admit(layout: &mut WorldLayout, d: &OutputDomain, w: &str, placement: Rect) {
    admit_with_flags(layout, d, w, placement, ExceptionFlags::none(), None);
}
fn admit_with_flags(
    layout: &mut WorldLayout,
    d: &OutputDomain,
    w: &str,
    placement: Rect,
    exceptions: ExceptionFlags,
    behavior: Option<ExceptionBehavior>,
) {
    let k = d.key();
    let win_id = WindowId(w.to_owned());
    if !layout.contains_domain(&k) {
        let mut row = engine_row(w, &d.id.0, &d.workspace.0, placement);
        row.floating = exceptions.floating;
        row.fullscreen = exceptions.fullscreen;
        row.maximized = exceptions.maximized;
        row.sticky = exceptions.sticky;
        layout
            .reconcile_fresh(d, 0, &win_id, &[row])
            .unwrap_or_else(|e| panic!("fresh admit {w}: {e:?}"));
        return;
    }
    let mut entry = tiled(w, &d.id.0, &d.workspace.0);
    entry.floating = exceptions.floating;
    entry.fullscreen = exceptions.fullscreen;
    entry.maximized = exceptions.maximized;
    entry.sticky = exceptions.sticky;
    let obs = complete_observed_for(layout, &k, vec![entry]);
    let defer_to_float = matches!(behavior, Some(ExceptionBehavior::Defer));
    layout
        .admit_window(
            &k,
            &win_id,
            exceptions,
            behavior,
            placement,
            false,
            defer_to_float,
            &obs,
        )
        .unwrap_or_else(|e| panic!("admit {w}: {e:?}"));
}
fn leaf_window(layout: &WorldLayout, k: &DomainKey, leaf: &NodeId) -> String {
    layout
        .window_for_leaf(k, leaf)
        .unwrap_or_else(|| panic!("no link for leaf {}", leaf.0))
        .0
}
fn dfs_leaves(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                dfs_leaves(child, out);
            }
        }
    }
}
fn dfs_windows(layout: &WorldLayout, domain: &DomainKey) -> Vec<String> {
    let tree = layout.tree(domain).cloned().expect("target tree");
    let mut leaves = Vec::new();
    dfs_leaves(&tree, &mut leaves);
    leaves
        .iter()
        .map(|l| leaf_window(layout, domain, l))
        .collect()
}
fn cross_window(
    layout: &mut WorldLayout,
    source: &OutputDomain,
    target: &OutputDomain,
    window: &str,
    direction: Direction,
    obs: &[EngineWindow],
) -> String {
    let subject = WindowId(window.to_owned());
    let ctx = PairOperationContext {
        source_domain: source,
        target_domain: target,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: obs,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_focus(&ctx, &subject, direction, false)
        .unwrap_or_else(|e| panic!("cross proposes: {e:?}"));
    assert!(outcome.crossed, "expected cross-output focus");
    outcome.to_window.0
}
fn is_nested(node: &Node) -> bool {
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => {
            children.iter().any(|c| matches!(c, Node::Group { .. }))
                || children.iter().any(is_nested)
        }
    }
}

// ---- Pure selector ranking: empty-history overlap, DFS ties, edges. ----

fn leaf(id: &str) -> NodeId {
    NodeId(id.to_owned())
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

#[test]
fn rank_empty_history_picks_largest_overlap_over_dfs_first() {
    // V-stacked pair, no history: the bottom-weighted source span favors
    // the second DFS leaf.
    let rects = vec![
        (leaf("a"), rect(0, 0, 400, 300)),
        (leaf("b"), rect(0, 300, 400, 300)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[], (300, 600)),
        Some(leaf("b"))
    );
}

#[test]
fn rank_overlap_tie_keeps_dfs_first() {
    // Equal overlaps with a full-height source span: DFS child order wins.
    let rects = vec![
        (leaf("a"), rect(0, 0, 400, 300)),
        (leaf("b"), rect(0, 300, 400, 300)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[], (0, 600)),
        Some(leaf("a"))
    );
}

#[test]
fn rank_history_beats_larger_overlap() {
    // Newest history wins even when the source span favors the other
    // candidate.
    let rects = vec![
        (leaf("a"), rect(0, 0, 400, 300)),
        (leaf("b"), rect(0, 300, 400, 300)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[leaf("b")], (0, 300)),
        Some(leaf("b"))
    );
}

#[test]
fn rank_stale_history_falls_through_to_overlap() {
    // History entries outside the candidate set never win.
    let rects = vec![
        (leaf("a"), rect(0, 0, 400, 300)),
        (leaf("b"), rect(0, 300, 400, 300)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(
            &rects,
            Direction::Right,
            &[leaf("gone"), leaf("a")],
            (300, 600)
        ),
        // "gone" misses, then "a" is the newest real candidate even
        // though the overlap favors "b": history first is total.
        Some(leaf("a"))
    );
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[leaf("gone")], (300, 600)),
        Some(leaf("b"))
    );
}

#[test]
fn rank_entry_edges_cover_all_four_directions() {
    let wide = vec![
        (leaf("a"), rect(0, 0, 400, 600)),
        (leaf("b"), rect(400, 0, 400, 600)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&wide, Direction::Right, &[], (0, 600)),
        Some(leaf("a")),
        "right enters the left edge"
    );
    assert_eq!(
        rank_cross_output_edge_candidates(&wide, Direction::Left, &[], (0, 600)),
        Some(leaf("b")),
        "left enters the right edge"
    );
    let tall = vec![
        (leaf("a"), rect(0, 0, 800, 300)),
        (leaf("b"), rect(0, 300, 800, 300)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&tall, Direction::Down, &[], (0, 800)),
        Some(leaf("a")),
        "down enters the top edge"
    );
    assert_eq!(
        rank_cross_output_edge_candidates(&tall, Direction::Up, &[], (0, 800)),
        Some(leaf("b")),
        "up enters the bottom edge"
    );
}

#[test]
fn rank_nested_group_keeps_dfs_order_on_tie() {
    // Nested left column: DFS visitation order decides the tie.
    let rects = vec![
        (leaf("a"), rect(0, 0, 400, 300)),
        (leaf("b"), rect(0, 300, 400, 300)),
        (leaf("c"), rect(400, 0, 400, 600)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[], (0, 600)),
        Some(leaf("a"))
    );
}

#[test]
fn rank_empty_set_selects_none() {
    assert_eq!(
        rank_cross_output_edge_candidates(&[], Direction::Right, &[leaf("a")], (0, 600)),
        None
    );
}

#[test]
fn right_crossing_prefers_edge_candidate_over_mru() {
    // O1 = [W1], O2 = H[W2 W3] with W3 most recently focused: the
    // destination entry edge (left) touches only W2, so W2 wins even
    // though W3 is the remembered/MRU leaf.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        "win-2"
    );
}

#[test]
fn left_crossing_prefers_edge_candidate_over_mru() {
    // Mirror: O2 sits left of O1, O2 = H[W2 W3] with W2 most recently
    // focused: the Left entry edge (right) touches only W3.
    let (mut o1, mut o2) = wide_domains();
    o1.adjacent.clear();
    o2.adjacent.clear();
    o1.adjacent
        .insert(Direction::Left, OutputId("out-2".to_owned()));
    o2.adjacent
        .insert(Direction::Right, OutputId("out-1".to_owned()));
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    // Refocus W2 locally so the MRU leaf is the non-edge window.
    let target = o2.key();
    let obs = complete_observed_for(&layout, &target, Vec::new());
    layout
        .local_focus(
            &target,
            &WindowId("win-3".to_owned()),
            Direction::Left,
            &WindowId("win-3".to_owned()),
            &obs,
        )
        .expect("local focus plans");
    assert_eq!(
        leaf_window(&layout, &target, layout.focus_leaf(&target).expect("leaf")),
        "win-2"
    );
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Left, &obs),
        "win-3"
    );
}

#[test]
fn stacked_history_winner_among_edge_candidates() {
    // O2 = V[W2 W3]: both leaves touch the Right entry edge, so the
    // most recently focused (W3) wins.
    let (o1, o2) = tall_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o2, "win-2", tall_placement());
    admit(&mut layout, &o2, "win-3", tall_placement());
    admit(&mut layout, &o1, "win-1", tall_placement());
    let source = o1.key();
    let target = o2.key();
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        "win-3"
    );
}

#[test]
fn nested_target_selects_edge_candidate_over_mru() {
    // Three windows force a nested group; only the leftmost leaf touches
    // the Right entry edge, so it wins over the MRU leaf.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o2, "win-4", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let target = o2.key();
    let tree = layout.tree(&target).cloned().expect("target tree");
    assert!(is_nested(&tree), "three windows nest: {tree:?}");
    let order = dfs_windows(&layout, &target);
    assert_eq!(order.len(), 3);
    let source = o1.key();
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        order[0]
    );
}

#[test]
fn seeded_order_breaks_overlap_tie_by_recency() {
    // Converge seeds freshly admitted leaves into the MRU stack in serial
    // order (D8), so O2 carries history [win-2, win-3] even though nothing
    // was ever focused there. Source W1 spans the full height, so both
    // V-stacked candidates overlap equally and recency (win-3) beats DFS
    // order (win-2).
    let (o1, o2) = tall_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-1", tall_placement());
    let source = o1.key();
    let target = o2.key();
    let seed_rows = vec![
        tiled("win-2", "out-2", "ws-1"),
        tiled("win-3", "out-2", "ws-1"),
    ];
    layout
        .converge_fresh_domain(&o2, 0, &seed_rows, None)
        .expect("fresh seed admits target pair");
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        "win-3"
    );
}

#[test]
fn seeded_order_and_overlap_agree_against_dfs_first() {
    // Source W1 is the bottom half of a tall O1; O2 = V[W2 top, W3
    // bottom] with D8-seeded history [win-2, win-3]. Both recency and the
    // larger source overlap favor W3 over DFS-first W2.
    let (o1, o2) = tall_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-0", tall_placement());
    admit(&mut layout, &o1, "win-1", tall_placement());
    let source = o1.key();
    assert_eq!(dfs_windows(&layout, &source), vec!["win-0", "win-1"]);
    let target = o2.key();
    let seed_rows = vec![
        tiled("win-2", "out-2", "ws-1"),
        tiled("win-3", "out-2", "ws-1"),
    ];
    layout
        .converge_fresh_domain(&o2, 0, &seed_rows, None)
        .expect("fresh seed admits target pair");
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        "win-3"
    );
}

#[test]
fn fullscreen_target_ranks_by_retained_allocation() {
    // The edge candidate carries a fullscreen overlay flag; selection
    // still ranks its retained tiled allocation over the MRU leaf.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    let mut obs = complete_engine_for_pair(&layout, &source, &target);
    for entry in &mut obs {
        if entry.window.0 == "win-2" {
            entry.fullscreen = true;
        }
    }
    let picked = cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs);
    assert_eq!(picked, "win-2");
}

#[test]
fn maximized_target_ranks_by_retained_allocation() {
    // Same retained-allocation ranking for a maximized (non-fullscreen)
    // edge candidate; the maximized fence gates subjects, not targets.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    let mut obs = complete_engine_for_pair(&layout, &source, &target);
    for entry in &mut obs {
        if entry.window.0 == "win-2" {
            entry.maximized = true;
        }
    }
    let picked = cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs);
    assert_eq!(picked, "win-2");
}

#[test]
fn empty_target_is_exact_noop() {
    // Empty destination refuses with untouched focus/tree.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    let before_tree = layout.tree(&source).cloned();
    let before_focus = layout.focus_leaf(&source).cloned();
    let obs = complete_engine_for_pair(&layout, &source, &target);
    let subject = WindowId("win-1".to_owned());
    let ctx = PairOperationContext {
        source_domain: &o1,
        target_domain: &o2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &obs,
        closed: &[],
    };
    let err = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, false)
        .expect_err("empty target must refuse");
    assert!(
        matches!(
            err,
            PairOperationFailure::Assembly(
                PairAssemblyFailure::TargetUnavailable | PairAssemblyFailure::TargetUnusable
            ) | PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
        ),
        "unexpected refusal: {err:?}"
    );
    assert_eq!(
        layout.focus_leaf(&source).cloned(),
        before_focus,
        "focus retained"
    );
    assert_eq!(
        layout.tree(&source),
        before_tree.as_ref(),
        "no state cleared"
    );
}

#[test]
fn floated_target_is_noop_without_state_clearing() {
    // The sole destination window floats away: no tiled edge candidate
    // remains, so crossing refuses with state intact.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-1", wide_placement());
    admit(&mut layout, &o2, "win-2", wide_placement());
    let source = o1.key();
    let target = o2.key();
    let mut float_entry = tiled("win-2", "out-2", "ws-1");
    float_entry.floating = true;
    layout
        .converge_domain(&target, &[float_entry], None, &BTreeSet::new())
        .expect("float transition converges");
    let before_source_tree = layout.tree(&source).cloned();
    let before_target_tree = layout.tree(&target).cloned();
    let obs = complete_engine_for_pair(&layout, &source, &target);
    let subject = WindowId("win-1".to_owned());
    let ctx = PairOperationContext {
        source_domain: &o1,
        target_domain: &o2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &obs,
        closed: &[],
    };
    let err = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, false)
        .expect_err("floated target must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(
        layout.tree(&source),
        before_source_tree.as_ref(),
        "no state cleared"
    );
    assert_eq!(
        layout.tree(&target),
        before_target_tree.as_ref(),
        "no state cleared"
    );
}

#[test]
fn float_origin_keeps_remembered_selection() {
    // Float-origin crossing is unchanged: the remembered tiled target
    // wins even though it does not touch the entry edge.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-1", wide_placement());
    admit_with_flags(
        &mut layout,
        &o1,
        "win-f",
        wide_placement(),
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        Some(ExceptionBehavior::Defer),
    );
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    let source = o1.key();
    let target = o2.key();
    let obs = complete_engine_for_pair(&layout, &source, &target);
    let subject = WindowId("win-f".to_owned());
    let ctx = PairOperationContext {
        source_domain: &o1,
        target_domain: &o2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &obs,
        closed: &[],
    };
    let outcome = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, true)
        .expect("float cross proposes");
    assert_eq!(outcome.to_window.0, "win-3");
}

#[test]
fn rank_globally_offset_outputs_overlap_in_shared_frame() {
    // Canonical allocations staged in the global frame (out-b at x=800
    // with a y offset): both V-stacked leaves touch the Right entry edge,
    // and the lower-weighted global source span favors the second DFS
    // leaf. Entry extremes are set-relative and overlap is span
    // differences, so the shared frame ranks correctly.
    let rects = vec![
        (leaf("a"), rect(800, 600, 400, 280)),
        (leaf("b"), rect(800, 880, 400, 280)),
    ];
    assert_eq!(
        rank_cross_output_edge_candidates(&rects, Direction::Right, &[], (880, 1160)),
        Some(leaf("b"))
    );
}

// ---- Seeded target without remembered leaf: float refuses, tiled ranks. ----

/// Converge-admit one window per output without ever focusing the target
/// domain: the target carries D8-seeded stack history but no remembered
/// (`last_active`) leaf, via production APIs only.
fn converged_no_remembered_target() -> (WorldLayout, OutputDomain, OutputDomain) {
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    admit(&mut layout, &o1, "win-1", wide_placement());
    let target = o2.key();
    let seed_rows = vec![
        tiled("win-2", "out-2", "ws-1"),
        tiled("win-3", "out-2", "ws-1"),
    ];
    layout
        .converge_fresh_domain(&o2, 0, &seed_rows, None)
        .expect("fresh seed admits target pair");
    assert_eq!(dfs_windows(&layout, &target), vec!["win-2", "win-3"]);
    assert_eq!(
        layout.remembered_leaf(&target),
        None,
        "converge-admission seeds the stack but never the remembered slot"
    );
    (layout, o1, o2)
}

#[test]
fn float_missing_remembered_refuses_unchanged() {
    // Float-origin refuses `Unchanged` when the target domain has no
    // remembered (`last_active`) leaf. (The deleted adapter-capability
    // handshake precedence is retired.)
    let (mut layout, o1, o2) = converged_no_remembered_target();
    admit_with_flags(
        &mut layout,
        &o1,
        "win-f",
        wide_placement(),
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        Some(ExceptionBehavior::Defer),
    );
    let source = o1.key();
    let target = o2.key();
    assert_eq!(layout.remembered_leaf(&target), None);
    let obs = complete_engine_for_pair(&layout, &source, &target);
    let subject = WindowId("win-f".to_owned());
    let ctx = PairOperationContext {
        source_domain: &o1,
        target_domain: &o2,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &subject,
        observed: &obs,
        closed: &[],
    };
    let err = layout
        .run_directional_pair_focus(&ctx, &subject, Direction::Right, true)
        .expect_err("missing remembered target must refuse");
    assert_eq!(
        err,
        PairOperationFailure::Layout(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
}

#[test]
fn tiled_seeded_stack_ranks_without_remembered_target() {
    // Tiled origins rank the seeded-stack edge leaf even when the target
    // has no remembered (`last_active`) leaf. (The deleted adapter
    // capability handshake precedence is retired.)
    let (mut layout, o1, o2) = converged_no_remembered_target();
    let source = o1.key();
    let target = o2.key();
    let obs = complete_engine_for_pair(&layout, &source, &target);
    assert_eq!(
        cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs),
        "win-2"
    );
}

// ---- Retained overlay allocation: converge holds the tiled link, ranking uses it. ----

fn converge_flagged_hold(
    layout: &mut WorldLayout,
    target: &DomainKey,
    window: &str,
    fullscreen: bool,
) {
    // Converge one observation carrying the overlay flag on a known tiled
    // window through the production convergence API: the link must stay
    // tiled (overlays never evict), so the propose-time observation below
    // still carries the flag as the frame discriminator over the retained
    // allocation.
    let mut wins = complete_observed_for(layout, target, Vec::new());
    let mut seen = false;
    for entry in &mut wins {
        if entry.window.0 == window {
            if fullscreen {
                entry.fullscreen = true;
            } else {
                entry.maximized = true;
            }
            seen = true;
        }
    }
    assert!(seen, "flagged window present");
    layout
        .converge_domain(target, &wins, None, &BTreeSet::new())
        .expect("overlay hold converges");
    assert!(
        layout.is_tiled_window(target, &WindowId(window.to_owned())),
        "overlay hold retains the tiled link"
    );
}

#[test]
fn fullscreen_hold_ranks_by_retained_allocation() {
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    converge_flagged_hold(&mut layout, &target, "win-2", true);
    let mut obs = complete_engine_for_pair(&layout, &source, &target);
    for entry in &mut obs {
        if entry.window.0 == "win-2" {
            entry.fullscreen = true;
        }
    }
    let picked = cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs);
    assert_eq!(picked, "win-2");
}

#[test]
fn maximized_hold_ranks_by_retained_allocation() {
    // Same retained-allocation ranking for a maximized hold; the maximized
    // fence gates subjects, not targets.
    let (o1, o2) = wide_domains();
    let mut layout = WorldLayout::new();
    layout.set_fixed_size_admission(true);
    admit(&mut layout, &o2, "win-2", wide_placement());
    admit(&mut layout, &o2, "win-3", wide_placement());
    admit(&mut layout, &o1, "win-1", wide_placement());
    let source = o1.key();
    let target = o2.key();
    converge_flagged_hold(&mut layout, &target, "win-2", false);
    let mut obs = complete_engine_for_pair(&layout, &source, &target);
    for entry in &mut obs {
        if entry.window.0 == "win-2" {
            entry.maximized = true;
        }
    }
    let picked = cross_window(&mut layout, &o1, &o2, "win-1", Direction::Right, &obs);
    assert_eq!(picked, "win-2");
}
