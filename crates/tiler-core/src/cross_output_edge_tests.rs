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

use crate::contract::{
    AckOutcome, AdapterAck, FocusCapabilities, FocusPostObservation, LifecycleCapabilities,
    LifecyclePostObservation, Observation,
};
use crate::directional::{Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::session::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, OutputDomain, ProposeError,
    RefusalKind, Session, SessionCommand, SessionFocusPlan, SessionObservation,
};
use std::collections::BTreeMap;

fn owner() -> OwnerId {
    OwnerId::parse("owner-1").expect("valid")
}
fn generation() -> GenerationId {
    GenerationId::parse("gen-1").expect("valid")
}
fn corr(v: &str) -> CorrelationId {
    CorrelationId::parse(v).expect("valid")
}
fn key(o: &str, w: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
    }
}
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
fn wide_domains() -> Vec<OutputDomain> {
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
    vec![o1, o2]
}
fn tall_domains() -> Vec<OutputDomain> {
    let mut domains = wide_domains();
    for d in &mut domains {
        d.bounds = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 600,
        };
    }
    domains
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
fn complete_observation(session: &Session, extra: Vec<ObservedWindow>) -> SessionObservation {
    let mut windows: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| tiled(&link.window.0, &link.output.0, &link.workspace.0))
        .collect();
    windows.extend(session.exception_observed());
    windows.extend(extra);
    windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    windows.dedup_by(|a, b| a.window == b.window);
    SessionObservation {
        observation: Observation::new(
            owner(),
            generation(),
            session.accepted_revision(),
            100 + session.accepted_revision(),
        ),
        windows,
    }
}
fn admit(s: &mut Session, w: &str, o: &str, ws: &str, placement: Rect, c: &str) {
    admit_with_flags(
        s,
        w,
        o,
        ws,
        placement,
        ExceptionFlags::none(),
        None,
        tiled(w, o, ws),
        c,
    );
}
#[allow(clippy::too_many_arguments)]
fn admit_with_flags(
    s: &mut Session,
    w: &str,
    o: &str,
    ws: &str,
    placement: Rect,
    exceptions: ExceptionFlags,
    behavior: Option<ExceptionBehavior>,
    entry: ObservedWindow,
    c: &str,
) {
    let obs = complete_observation(s, vec![entry]);
    let base = s.accepted_revision();
    let cmd = SessionCommand::Admit {
        window: WindowId(w.to_owned()),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        exceptions,
        exception_behavior: behavior,
        placement_bounds: placement,
        suppress_fixed_float: false,
    };
    let plan = s
        .propose(&cmd, &obs, &corr(c), &LifecycleCapabilities::full())
        .unwrap_or_else(|e| panic!("admit {w} proposes: {e:?}"));
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_lifecycle(&LifecyclePostObservation::new(
        Observation::new(owner(), generation(), base, 200 + base),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("commit");
}
fn commit_focus(s: &mut Session, plan: &SessionFocusPlan, c: &str) {
    let base = s.accepted_revision();
    s.acknowledge(&AdapterAck::new(
        corr(c),
        owner(),
        generation(),
        base,
        AckOutcome::Accepted,
    ))
    .expect("ack");
    s.verify_focus(&FocusPostObservation::new(
        Observation::new(owner(), generation(), base, 950 + base),
        corr(c),
        true,
        plan.dispatch.preconditions.clone(),
        plan.dispatch.operation.clone(),
    ))
    .expect("verify focus");
}
fn leaf_window(s: &Session, leaf: &NodeId) -> String {
    s.snapshot()
        .windows
        .iter()
        .find(|l| &l.leaf == leaf)
        .unwrap_or_else(|| panic!("no link for leaf {}", leaf.0))
        .window
        .0
        .clone()
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
fn dfs_windows(s: &Session, domain: &DomainKey) -> Vec<String> {
    let tree = s
        .snapshot()
        .domains
        .iter()
        .find(|d| d.output == domain.output && d.workspace == domain.workspace)
        .and_then(|d| d.tree.clone())
        .expect("target tree");
    let mut leaves = Vec::new();
    dfs_leaves(&tree, &mut leaves);
    leaves.iter().map(|l| leaf_window(s, l)).collect()
}
fn cross_window(
    s: &mut Session,
    source: &DomainKey,
    window: &str,
    direction: Direction,
    obs: &SessionObservation,
    c: &str,
) -> String {
    let plan = s
        .propose_cross_output_focus(
            source,
            &WindowId(window.to_owned()),
            direction,
            obs,
            &corr(c),
            &FocusCapabilities::full(),
        )
        .unwrap_or_else(|e| panic!("cross proposes: {e:?}"));
    leaf_window(s, &plan.desired_focus_leaf)
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

use crate::session::rank_cross_output_edge_candidates;

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
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let target = key("out-2", "ws-1");
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        "win-2"
    );
}

#[test]
fn left_crossing_prefers_edge_candidate_over_mru() {
    // Mirror: O2 sits left of O1, O2 = H[W2 W3] with W2 most recently
    // focused: the Left entry edge (right) touches only W3.
    let mut domains = wide_domains();
    domains[0].adjacent.clear();
    domains[1].adjacent.clear();
    domains[0]
        .adjacent
        .insert(Direction::Left, OutputId("out-2".to_owned()));
    domains[1]
        .adjacent
        .insert(Direction::Right, OutputId("out-1".to_owned()));
    let mut s = Session::new(owner(), generation(), 0, 7, domains).expect("session");
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    // Refocus W2 locally so the MRU leaf is the non-edge window.
    let target = key("out-2", "ws-1");
    let obs = complete_observation(&s, Vec::new());
    let plan = s
        .propose_focus(
            &target,
            &WindowId("win-3".to_owned()),
            Direction::Left,
            &obs,
            &corr("c-f1"),
            &FocusCapabilities::full(),
        )
        .expect("local focus plans");
    commit_focus(&mut s, &plan, "c-f1");
    assert_eq!(leaf_window(&s, &s.focus().1.expect("leaf")), "win-2");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Left, &obs, "c-x1"),
        "win-3"
    );
}

#[test]
fn stacked_history_winner_among_edge_candidates() {
    // O2 = V[W2 W3]: both leaves touch the Right entry edge, so the
    // most recently focused (W3) wins.
    let mut s = Session::new(owner(), generation(), 0, 7, tall_domains()).expect("session");
    admit(&mut s, "win-2", "out-2", "ws-1", tall_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", tall_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", tall_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let target = key("out-2", "ws-1");
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        "win-3"
    );
}

#[test]
fn nested_target_selects_edge_candidate_over_mru() {
    // Three windows force a nested group; only the leftmost leaf touches
    // the Right entry edge, so it wins over the MRU leaf.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-4", "out-2", "ws-1", wide_placement(), "c-a4");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let target = key("out-2", "ws-1");
    let tree = s
        .snapshot()
        .domains
        .iter()
        .find(|d| d.output == target.output && d.workspace == target.workspace)
        .and_then(|d| d.tree.clone())
        .expect("target tree");
    assert!(is_nested(&tree), "three windows nest: {tree:?}");
    let order = dfs_windows(&s, &target);
    assert_eq!(order.len(), 3);
    let source = key("out-1", "ws-1");
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        order[0]
    );
}

#[test]
fn seeded_order_breaks_overlap_tie_by_recency() {
    // Converge seeds freshly admitted leaves into the MRU stack in serial
    // order (D8), so O2 carries history [win-2, win-3] even though nothing
    // was ever focused there. Source W1 spans the full height, so both
    // V-stacked candidates overlap equally and recency (win-3) beats DFS
    // order (win-2). Pre-selector code refused here (no remembered leaf).
    let mut s = Session::new(owner(), generation(), 0, 7, tall_domains()).expect("session");
    admit(&mut s, "win-1", "out-1", "ws-1", tall_placement(), "c-a1");
    let target = key("out-2", "ws-1");
    let obs = complete_observation(
        &s,
        vec![
            tiled("win-2", "out-2", "ws-1"),
            tiled("win-3", "out-2", "ws-1"),
        ],
    );
    let counts = s
        .converge_observation(&obs, Some(&WindowId("win-1".to_owned())))
        .expect("converge admits target pair");
    assert_eq!(counts.admitted, 2);
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    let source = key("out-1", "ws-1");
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        "win-3"
    );
}

#[test]
fn seeded_order_and_overlap_agree_against_dfs_first() {
    // Source W1 is the bottom half of a tall O1; O2 = V[W2 top, W3
    // bottom] with D8-seeded history [win-2, win-3]. Both recency and the
    // larger source overlap favor W3 over DFS-first W2. Pre-selector code
    // refused here (no remembered leaf).
    let mut s = Session::new(owner(), generation(), 0, 7, tall_domains()).expect("session");
    admit(&mut s, "win-0", "out-1", "ws-1", tall_placement(), "c-a0");
    admit(&mut s, "win-1", "out-1", "ws-1", tall_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    assert_eq!(dfs_windows(&s, &source), vec!["win-0", "win-1"]);
    let target = key("out-2", "ws-1");
    let obs = complete_observation(
        &s,
        vec![
            tiled("win-2", "out-2", "ws-1"),
            tiled("win-3", "out-2", "ws-1"),
        ],
    );
    let counts = s
        .converge_observation(&obs, Some(&WindowId("win-1".to_owned())))
        .expect("converge admits target pair");
    assert_eq!(counts.admitted, 2);
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        "win-3"
    );
}

#[test]
fn fullscreen_target_ranks_by_retained_allocation() {
    // The edge candidate carries a fullscreen overlay flag; selection
    // still ranks its retained tiled allocation over the MRU leaf.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let mut overrides = BTreeMap::new();
    let mut flagged = tiled("win-2", "out-2", "ws-1");
    flagged.fullscreen = true;
    overrides.insert("win-2".to_owned(), flagged);
    let obs = complete_observation(&s, Vec::new());
    let mut wins = obs.windows.clone();
    for entry in &mut wins {
        if let Some(replacement) = overrides.get(&entry.window.0) {
            *entry = replacement.clone();
        }
    }
    let obs = SessionObservation {
        observation: obs.observation,
        windows: wins,
    };
    let picked = cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1");
    assert_eq!(picked, "win-2");
}

#[test]
fn maximized_target_ranks_by_retained_allocation() {
    // Same retained-allocation ranking for a maximized (non-fullscreen)
    // edge candidate; the maximized fence gates subjects, not targets.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let obs = complete_observation(&s, Vec::new());
    let mut wins = obs.windows.clone();
    for entry in &mut wins {
        if entry.window.0 == "win-2" {
            entry.maximized = true;
        }
    }
    let obs = SessionObservation {
        observation: obs.observation,
        windows: wins,
    };
    let picked = cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1");
    assert_eq!(picked, "win-2");
}

#[test]
fn empty_target_is_exact_noop() {
    // Empty destination: Unchanged with no pending, no desired state,
    // and untouched focus/snapshot.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let before_snapshot = s.snapshot();
    let before_focus = s.focus();
    let obs = complete_observation(&s, Vec::new());
    let err = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-1".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x1"),
            &FocusCapabilities::full(),
        )
        .expect_err("empty target must refuse");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "no pending staged");
    assert!(!s.has_pending_desired(), "no desired state staged");
    assert_eq!(s.focus(), before_focus, "focus retained");
    assert_eq!(s.snapshot(), before_snapshot, "no state cleared");
}

#[test]
fn floated_target_is_noop_without_state_clearing() {
    // The sole destination window floats away: no tiled edge candidate
    // remains, so crossing refuses with state intact.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    let mut float_obs = complete_observation(&s, Vec::new());
    for entry in &mut float_obs.windows {
        if entry.window.0 == "win-2" {
            entry.floating = true;
        }
    }
    s.converge_observation(&float_obs, Some(&WindowId("win-1".to_owned())))
        .expect("float transition converges");
    let source = key("out-1", "ws-1");
    let before_snapshot = s.snapshot();
    let obs = complete_observation(&s, Vec::new());
    let err = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-1".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x1"),
            &FocusCapabilities::full(),
        )
        .expect_err("floated target must refuse");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    assert!(!s.has_pending(), "no pending staged");
    assert!(!s.has_pending_desired(), "no desired state staged");
    assert_eq!(s.snapshot(), before_snapshot, "no state cleared");
}

#[test]
fn float_origin_keeps_remembered_selection() {
    // Float-origin crossing is unchanged: the remembered tiled target
    // wins even though it does not touch the entry edge.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let mut float_entry = tiled("win-f", "out-1", "ws-1");
    float_entry.floating = true;
    admit_with_flags(
        &mut s,
        "win-f",
        "out-1",
        "ws-1",
        wide_placement(),
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        Some(ExceptionBehavior::Defer),
        float_entry,
        "c-af",
    );
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    let source = key("out-1", "ws-1");
    let obs = complete_observation(&s, Vec::new());
    let plan = s
        .propose_float_cross_output_focus(
            &source,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x1"),
            &FocusCapabilities::full(),
        )
        .expect("float cross proposes");
    assert_eq!(leaf_window(&s, &plan.desired_focus_leaf), "win-3");
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

// ---- Combined-refusal precedence: float target-first, tiled capability-first. ----

/// Converge-admit one window per output without ever focusing the target
/// domain: the target carries D8-seeded stack history but no remembered
/// (`last_active`) leaf, via production APIs only.
fn converged_no_remembered_target() -> (Session, DomainKey, DomainKey) {
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let target = key("out-2", "ws-1");
    let obs = complete_observation(
        &s,
        vec![
            tiled("win-2", "out-2", "ws-1"),
            tiled("win-3", "out-2", "ws-1"),
        ],
    );
    let counts = s
        .converge_observation(&obs, Some(&WindowId("win-1".to_owned())))
        .expect("converge admits target pair");
    assert_eq!(counts.admitted, 2);
    assert_eq!(dfs_windows(&s, &target), vec!["win-2", "win-3"]);
    assert_eq!(
        s.remembered_leaf(&target),
        None,
        "converge-admission seeds the stack but never the remembered slot"
    );
    (s, key("out-1", "ws-1"), target)
}

#[test]
fn float_missing_remembered_beats_unsupported_capability() {
    // Float-origin keeps the exact original refusal precedence: no
    // remembered target refuses `Unchanged` even when the adapter also
    // lacks directional focus (which alone would refuse
    // `UnsupportedCapability`).
    let (mut s, source, target) = converged_no_remembered_target();
    let mut float_entry = tiled("win-f", "out-1", "ws-1");
    float_entry.floating = true;
    admit_with_flags(
        &mut s,
        "win-f",
        "out-1",
        "ws-1",
        wide_placement(),
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        Some(ExceptionBehavior::Defer),
        float_entry,
        "c-af",
    );
    assert_eq!(s.remembered_leaf(&target), None);
    let obs = complete_observation(&s, Vec::new());
    let err = s
        .propose_float_cross_output_focus(
            &source,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x1"),
            &FocusCapabilities::none(),
        )
        .expect_err("missing remembered target must refuse first");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
    let err = s
        .propose_float_cross_output_focus(
            &source,
            &WindowId("win-f".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x2"),
            &FocusCapabilities::full(),
        )
        .expect_err("missing remembered target must refuse");
    assert_eq!(err, ProposeError::Refused(RefusalKind::Unchanged));
}

#[test]
fn tiled_unsupported_capability_precedes_rank() {
    // Tiled origins necessarily project (capability-gated) before ranking
    // consumes the geometry, so a missing capability refuses
    // `UnsupportedCapability` even though the edge rank would succeed.
    // This documents the principled split from the float-origin order.
    let (mut s, source, _) = converged_no_remembered_target();
    let obs = complete_observation(&s, Vec::new());
    let err = s
        .propose_cross_output_focus(
            &source,
            &WindowId("win-1".to_owned()),
            Direction::Right,
            &obs,
            &corr("c-x1"),
            &FocusCapabilities::none(),
        )
        .expect_err("missing capability must refuse first");
    assert_eq!(
        err,
        ProposeError::Refused(RefusalKind::UnsupportedCapability)
    );
    // With capability the seeded-stack rank succeeds on the edge leaf.
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x2"),
        "win-2"
    );
}

#[test]
fn stale_last_active_falls_through_to_stack_then_overlap() {
    // Closing the last-active leaf leaves `last_active` stale (the slot is
    // write-only, never pruned) while the stack prunes to the survivor.
    // The stale head misses the candidate set, so selection still reaches
    // the surviving edge leaf. Uses the production close path only.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    let source = key("out-1", "ws-1");
    let target = key("out-2", "ws-1");
    let mut wins: Vec<ObservedWindow> = s
        .snapshot()
        .windows
        .iter()
        .map(|link| tiled(&link.window.0, &link.output.0, &link.workspace.0))
        .collect();
    wins.extend(s.exception_observed());
    wins.retain(|entry| entry.window.0 != "win-3");
    wins.sort_by(|a, b| a.window.0.cmp(&b.window.0));
    let close = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), 500),
        windows: wins,
    };
    s.converge_observation(&close, Some(&WindowId("win-1".to_owned())))
        .expect("close converges");
    assert_eq!(dfs_windows(&s, &target), vec!["win-2"]);
    let history = s.cross_output_history_newest_first(&target);
    assert_eq!(
        history.len(),
        2,
        "stale head plus pruned stack: {history:?}"
    );
    assert!(
        s.focused_window_for(&history[0], &target).is_none(),
        "stale head resolves to no link"
    );
    assert_eq!(
        s.remembered_leaf(&target),
        None,
        "stale slot never poses as remembered"
    );
    let obs = complete_observation(&s, Vec::new());
    assert_eq!(
        cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1"),
        "win-2"
    );
}

// ---- Retained overlay allocation: converge holds the tiled link, ranking uses it. ----

fn converge_flagged_hold(s: &mut Session, window: &str, fullscreen: bool, tag: u64) {
    // Converge one observation carrying the overlay flag on a known tiled
    // window through the production convergence API: the link must stay
    // tiled (overlays never evict), so the propose-time observation below
    // still carries the flag as the frame discriminator over the retained
    // allocation.
    let mut wins = complete_observation(s, Vec::new()).windows;
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
    let hold = SessionObservation {
        observation: Observation::new(owner(), generation(), s.accepted_revision(), tag),
        windows: wins,
    };
    s.converge_observation(&hold, Some(&WindowId("win-1".to_owned())))
        .expect("overlay hold converges");
    assert!(
        s.snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == window),
        "overlay hold retains the tiled link"
    );
}

#[test]
fn fullscreen_hold_ranks_by_retained_allocation() {
    // Session-level proof is adequate here: convergence retains the
    // overlay window as a tiled link by design (world facts converge to
    // the same retained link), and the propose-time observation is the
    // Session API's flag carrier, so ranking the retained left-edge
    // allocation over the MRU right leaf is covered end to end.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    converge_flagged_hold(&mut s, "win-2", true, 600);
    let source = key("out-1", "ws-1");
    let mut wins = complete_observation(&s, Vec::new()).windows;
    for entry in &mut wins {
        if entry.window.0 == "win-2" {
            entry.fullscreen = true;
        }
    }
    let obs = SessionObservation {
        observation: complete_observation(&s, Vec::new()).observation,
        windows: wins,
    };
    let picked = cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1");
    assert_eq!(picked, "win-2");
}

#[test]
fn maximized_hold_ranks_by_retained_allocation() {
    // Same retained-allocation ranking for a maximized hold; the maximized
    // fence gates subjects, not targets.
    let mut s = Session::new(owner(), generation(), 0, 7, wide_domains()).expect("session");
    s.set_fixed_size_admission(true);
    admit(&mut s, "win-2", "out-2", "ws-1", wide_placement(), "c-a2");
    admit(&mut s, "win-3", "out-2", "ws-1", wide_placement(), "c-a3");
    admit(&mut s, "win-1", "out-1", "ws-1", wide_placement(), "c-a1");
    converge_flagged_hold(&mut s, "win-2", false, 600);
    let source = key("out-1", "ws-1");
    let mut wins = complete_observation(&s, Vec::new()).windows;
    for entry in &mut wins {
        if entry.window.0 == "win-2" {
            entry.maximized = true;
        }
    }
    let obs = SessionObservation {
        observation: complete_observation(&s, Vec::new()).observation,
        windows: wins,
    };
    let picked = cross_window(&mut s, &source, "win-1", Direction::Right, &obs, "c-x1");
    assert_eq!(picked, "win-2");
}
