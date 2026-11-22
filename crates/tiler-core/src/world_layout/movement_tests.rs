//! Directional movement + focus integration over `WorldLayout` (portable, headless).
//!
//! Covers rules R1-R4, focus retention, shares/order, domain isolation,
//! refusals, geometry completeness, capacity-float fallback, determinism,
//! adjacency validation, and a bounded property-like loop.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{
    Axis, Capability, Direction, Node, NodeId, OutputId, Rule, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DesiredGeometry, DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{
    AdmissionOutcome, LayoutFailure, LocalFocusOutcome, LocalMoveOutcome, PairMoveOutcome,
    PairOperationFailure,
};
use super::pair::PairAssemblyFailure;
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn domain(output: &str, workspace: &str, w: i32, h: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn domain_with_adjacent(
    output: &str,
    workspace: &str,
    adjacent: Vec<(Direction, &str)>,
) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        },
        gap: 0,
        adjacent: adjacent
            .into_iter()
            .map(|(d, t)| (d, OutputId(t.to_owned())))
            .collect(),
    }
}

fn placement(horiz: bool) -> Rect {
    if horiz {
        Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        }
    } else {
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        }
    }
}

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn tiled(window: &str, output: &str, workspace: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(window),
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

struct MovementRig {
    layout: WorldLayout,
    domains: BTreeMap<DomainKey, OutputDomain>,
}

impl MovementRig {
    fn new(descs: Vec<OutputDomain>) -> Self {
        let mut domains = BTreeMap::new();
        for desc in descs {
            domains.insert(desc.key(), desc);
        }
        Self {
            layout: WorldLayout::new(),
            domains,
        }
    }

    fn single() -> Self {
        Self::new(vec![domain("out-1", "ws-1", 200, 200)])
    }

    fn r4() -> Self {
        Self::new(vec![
            domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
            domain_with_adjacent("out-2", "ws-1", vec![(Direction::Left, "out-1")]),
        ])
    }

    fn stacked() -> Self {
        Self::new(vec![
            domain_with_adjacent("out-top", "ws-1", vec![(Direction::Down, "out-bottom")]),
            domain_with_adjacent("out-bottom", "ws-1", vec![(Direction::Up, "out-top")]),
        ])
    }

    fn domain_observed(&self, k: &DomainKey, extra: Vec<ObservedWindow>) -> Vec<ObservedWindow> {
        let mut windows: Vec<ObservedWindow> = self
            .layout
            .domain(k)
            .map(|dom| {
                dom.leaves
                    .keys()
                    .chain(dom.exceptions.keys())
                    .map(|w| ObservedWindow {
                        window: w.clone(),
                        output: k.output.clone(),
                        workspace: k.workspace.clone(),
                        floating: dom.exceptions.get(w).is_some_and(|e| e.flags.floating),
                        fullscreen: dom.exceptions.get(w).is_some_and(|e| e.flags.fullscreen),
                        maximized: dom.exceptions.get(w).is_some_and(|e| e.flags.maximized),
                        sticky: dom.exceptions.get(w).is_some_and(|e| e.flags.sticky),
                        fixed_auto: dom.automatic_fixed.contains(w),
                        fixed_suppress: dom.fixed_tile_override.contains(w),
                        hints: WindowSizeHints::none(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        windows.extend(extra);
        windows.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        windows
    }

    fn all_engine_windows(&self) -> Vec<EngineWindow> {
        let mut out = Vec::new();
        for (k, dom) in &self.layout.domains {
            for w in dom.leaves.keys() {
                out.push(EngineWindow {
                    window: w.clone(),
                    output: k.output.clone(),
                    workspace: k.workspace.clone(),
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
                    hints: WindowSizeHints::none(),
                });
            }
        }
        out.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        out
    }

    fn admit(
        &mut self,
        window: &str,
        output: &str,
        workspace: &str,
        horiz: bool,
    ) -> AdmissionOutcome {
        let k = key(output, workspace);
        let desc = self.domains[&k].clone();
        self.layout
            .domains
            .entry(k.clone())
            .or_insert_with(|| DomainLayout::new(desc.bounds, desc.gap, 0));
        let obs = self.domain_observed(&k, vec![tiled(window, output, workspace)]);
        self.layout
            .admit_window(&k, &win(window), placement(horiz), false, false, &obs)
            .unwrap_or_else(|e| panic!("admit {window}: {e:?}"))
    }

    fn focused_window(&self, k: &DomainKey) -> WindowId {
        let dom = self.layout.domain(k).expect("domain");
        let focus_leaf = dom.focused_leaf.as_ref().expect("focused leaf");
        dom.leaves
            .iter()
            .find_map(|(w, l)| (l == focus_leaf).then(|| w.clone()))
            .expect("focused window")
    }

    fn focus_commit_focused(&mut self, k: &DomainKey, direction: Direction) -> LocalFocusOutcome {
        let w = self.focused_window(k);
        let obs = self.domain_observed(k, vec![]);
        self.layout
            .local_focus(k, &w, direction, &w, &obs)
            .unwrap_or_else(|e| panic!("focus {direction:?}: {e:?}"))
    }

    fn move_local_focused_with_mode(
        &mut self,
        k: &DomainKey,
        direction: Direction,
        mode: SameAxisMove,
    ) -> LocalMoveOutcome {
        let w = self.focused_window(k);
        let obs = self.domain_observed(k, vec![]);
        self.layout
            .local_move(k, &w, direction, mode, &w, &obs)
            .unwrap_or_else(|e| panic!("move {direction:?}: {e:?}"))
    }

    fn move_local_focused(&mut self, k: &DomainKey, direction: Direction) -> LocalMoveOutcome {
        self.move_local_focused_with_mode(k, direction, SameAxisMove::GroupWithNeighbor)
    }

    fn move_pair_focused(
        &mut self,
        source: &DomainKey,
        target: &DomainKey,
        direction: Direction,
    ) -> PairMoveOutcome {
        let w = self.focused_window(source);
        let src_dom = self.domains[source].clone();
        let dst_dom = self.domains[target].clone();
        let obs = self.all_engine_windows();
        let ctx = PairOperationContext {
            source_domain: &src_dom,
            target_domain: &dst_dom,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &w,
            observed: &obs,
            closed: &[],
        };
        self.layout
            .run_directional_pair_move(&ctx, &w, direction, true, SameAxisMove::GroupWithNeighbor)
            .unwrap_or_else(|e| panic!("pair move {direction:?}: {e:?}"))
    }

    fn leaf(&self, output: &str, workspace: &str, window: &str) -> NodeId {
        self.layout
            .leaves(&key(output, workspace))
            .and_then(|leaves| leaves.get(&win(window)))
            .expect("leaf for window")
            .clone()
    }

    fn leaves(&self, output: &str, workspace: &str) -> Vec<String> {
        let k = key(output, workspace);
        let by_leaf: BTreeMap<NodeId, String> = self
            .layout
            .leaves(&k)
            .map(|leaves| {
                leaves
                    .iter()
                    .map(|(w, l)| (l.clone(), w.0.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut out = Vec::new();
        if let Some(tree) = self.layout.domain(&k).and_then(|d| d.tree.as_ref()) {
            collect_leaves(tree, &by_leaf, &mut out);
        }
        out
    }

    fn tree_of(&self, output: &str, workspace: &str) -> Option<Node> {
        self.layout
            .domain(&key(output, workspace))
            .and_then(|d| d.tree.clone())
    }

    fn assert_geometry_complete(&self, k: &DomainKey, geometry: &[DesiredGeometry]) {
        let dom = self.layout.domain(k).expect("domain");
        assert_eq!(geometry.len(), dom.leaves.len());
        let expected_windows: BTreeSet<&WindowId> = dom.leaves.keys().collect();
        let seen_windows: BTreeSet<&WindowId> = geometry.iter().map(|g| &g.window).collect();
        assert_eq!(seen_windows, expected_windows);
        let bounds = self.domains[k].bounds;
        for g in geometry {
            assert_eq!(g.output, k.output);
            assert_eq!(g.workspace, k.workspace);
            assert!(g.rect.w > 0 && g.rect.h > 0);
            assert!(g.rect.x >= bounds.x && g.rect.y >= bounds.y);
            assert!(g.rect.x + g.rect.w <= bounds.x + bounds.w);
            assert!(g.rect.y + g.rect.h <= bounds.y + bounds.h);
        }
    }

    fn assert_pair_geometry_complete(&self, affected: &[&DomainKey], geometry: &[DesiredGeometry]) {
        let mut expected = 0usize;
        for k in affected {
            let dom = self.layout.domain(k);
            let count = dom.map_or(0, |d| d.leaves.len());
            expected += count;
            let bounds = self.domains[k].bounds;
            for g in geometry
                .iter()
                .filter(|g| g.output == k.output && g.workspace == k.workspace)
            {
                assert!(g.rect.w > 0 && g.rect.h > 0);
                assert!(g.rect.x >= bounds.x && g.rect.y >= bounds.y);
                assert!(g.rect.x + g.rect.w <= bounds.x + bounds.w);
                assert!(g.rect.y + g.rect.h <= bounds.y + bounds.h);
            }
        }
        assert_eq!(geometry.len(), expected);
    }
}

fn collect_leaves(node: &Node, by_leaf: &BTreeMap<NodeId, String>, out: &mut Vec<String>) {
    match node {
        Node::Leaf { id } => out.push(by_leaf.get(id).cloned().unwrap_or_else(|| id.0.clone())),
        Node::Group { children, .. } => {
            for c in children {
                collect_leaves(c, by_leaf, out);
            }
        }
    }
}

fn parent_children_shares(tree: &Node, leaf: &str) -> (Vec<String>, Vec<u64>) {
    fn find(node: &Node, leaf: &str) -> Option<(Vec<String>, Vec<u64>)> {
        match node {
            Node::Leaf { .. } => None,
            Node::Group {
                children, shares, ..
            } => {
                for child in children {
                    if let Node::Leaf { id } = child
                        && id.0 == leaf
                    {
                        return Some((
                            children.iter().map(|c| c.id().0.clone()).collect(),
                            shares.clone(),
                        ));
                    }
                    if let Some(found) = find(child, leaf) {
                        return Some(found);
                    }
                }
                None
            }
        }
    }
    find(tree, leaf).expect("parent group")
}

fn root_children(tree: &Node) -> (Axis, Vec<String>) {
    match tree {
        Node::Leaf { id } => panic!("expected root group, got leaf {}", id.0),
        Node::Group { axis, children, .. } => {
            (*axis, children.iter().map(|c| c.id().0.clone()).collect())
        }
    }
}

fn count_groups(node: &Node) -> usize {
    match node {
        Node::Leaf { .. } => 0,
        Node::Group { children, .. } => 1 + children.iter().map(count_groups).sum::<usize>(),
    }
}

fn check_shares(node: &Node, ids: &mut BTreeSet<String>) {
    assert!(ids.insert(node.id().0.clone()), "duplicate node id");
    match node {
        Node::Leaf { .. } => {}
        Node::Group {
            children, shares, ..
        } => {
            assert!(children.len() >= 2);
            assert_eq!(shares.len(), children.len());
            assert!(shares.iter().all(|s| *s > 0));
            for c in children {
                check_shares(c, ids);
            }
        }
    }
}

#[test]
fn r1_perpendicular_wrap() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let outcome = s.move_local_focused(&k, Direction::Down);
    assert_eq!(outcome.rule, Rule::R1);
    assert_eq!(outcome.focus_leaf, s.leaf("out-1", "ws-1", "win-2"));
    s.assert_geometry_complete(&k, &outcome.geometry);
    assert_eq!(s.leaves("out-1", "ws-1"), vec!["win-1", "win-2"]);
}

#[test]
fn r2a_swap_neighbor() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", false);
    s.admit("win-4", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let outcome = s.move_local_focused(&k, Direction::Left);
    assert_eq!(outcome.rule, Rule::R2a);
    assert_eq!(outcome.focus_leaf, s.leaf("out-1", "ws-1", "win-4"));
    s.assert_geometry_complete(&k, &outcome.geometry);
}

#[test]
fn r2a_uneven_outer_preserved_and_window_share_binding() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    for _ in 0..3 {
        s.focus_commit_focused(&k, Direction::Left);
    }
    let r2b = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b.rule, Rule::R2b);
    let r2c = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2c.rule, Rule::R2c);
    let root = s.tree_of("out-1", "ws-1").expect("tree");
    let (outer_shares, outer_len) = match &root {
        Node::Group {
            shares, children, ..
        } => (shares.clone(), children.len()),
        _ => panic!("root group"),
    };
    assert_eq!(outer_len, 2);
    assert_eq!(outer_shares.iter().sum::<u64>(), 3);
    assert_eq!(outer_shares, vec![2, 1]);

    let r2a = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2a.rule, Rule::R2a);
    let root2 = s.tree_of("out-1", "ws-1").expect("tree");
    match &root2 {
        Node::Group {
            shares, children, ..
        } => {
            assert_eq!(*shares, outer_shares, "uneven outer preserved on R2a");
            assert_eq!(children.len(), 2);
            match &children[0] {
                Node::Group {
                    children, shares, ..
                } => {
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id(), &s.leaf("out-1", "ws-1", "win-2"));
                    assert_eq!(children[1].id(), &s.leaf("out-1", "ws-1", "win-1"));
                }
                other => panic!("wrapper {other:?}"),
            }
        }
        other => panic!("root {other:?}"),
    }
    s.assert_geometry_complete(&k, &r2a.geometry);
}

#[test]
fn r2b_insert_midpoint_after_r1() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let r1 = s.move_local_focused(&k, Direction::Down);
    assert_eq!(r1.rule, Rule::R1);
    s.focus_commit_focused(&k, Direction::Left);
    assert_eq!(s.focused_window(&k).0, "win-1");

    let r2b = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b.rule, Rule::R2b);
    assert_eq!(r2b.capability, Capability::InsertChild);
    let leaf1 = s.leaf("out-1", "ws-1", "win-1");
    let leaf2 = s.leaf("out-1", "ws-1", "win-2");
    let leaf3 = s.leaf("out-1", "ws-1", "win-3");
    assert_eq!(r2b.focus_leaf, leaf1);
    let (children, _) = parent_children_shares(&s.tree_of("out-1", "ws-1").unwrap(), &leaf1.0);
    assert_eq!(children, vec![leaf2.0, leaf1.0, leaf3.0]);
    s.assert_geometry_complete(&k, &r2b.geometry);
}

#[test]
fn r2b_split_odd_target() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    let r1 = s.move_local_focused(&k, Direction::Up);
    assert_eq!(r1.rule, Rule::R1);
    s.focus_commit_focused(&k, Direction::Left);
    assert_eq!(s.focused_window(&k).0, "win-2");
    let r2b_mid = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b_mid.rule, Rule::R2b);
    s.focus_commit_focused(&k, Direction::Left);
    assert_eq!(s.focused_window(&k).0, "win-1");

    let r2b = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b.rule, Rule::R2b);
    assert_eq!(r2b.capability, Capability::SplitGroupChild);
    let leaf1 = s.leaf("out-1", "ws-1", "win-1");
    let (children, _) = parent_children_shares(&s.tree_of("out-1", "ws-1").unwrap(), &leaf1.0);
    assert_eq!(children.len(), 2);
    assert_eq!(children[1], leaf1.0, "Right splits Second (far side)");
    s.assert_geometry_complete(&k, &r2b.geometry);
}

#[test]
fn r2b_near_edge_parallel_target() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    s.focus_commit_focused(&k, Direction::Left);
    s.focus_commit_focused(&k, Direction::Left);
    assert_eq!(s.focused_window(&k).0, "win-1");

    let outcome = s.move_local_focused(&k, Direction::Right);
    assert_eq!(outcome.rule, Rule::R2b);
    assert_eq!(outcome.capability, Capability::InsertChild);
    let leaf1 = s.leaf("out-1", "ws-1", "win-1");
    let (children, _) = parent_children_shares(&s.tree_of("out-1", "ws-1").unwrap(), &leaf1.0);
    assert_eq!(children[0], leaf1.0, "NearEdge inserts at 0");
    s.assert_geometry_complete(&k, &outcome.geometry);
}

#[test]
fn r2c_wrap_neighbor_nary() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    for _ in 0..3 {
        s.focus_commit_focused(&k, Direction::Left);
    }
    assert_eq!(s.focused_window(&k).0, "win-1");
    s.move_local_focused(&k, Direction::Right);
    assert_eq!(s.focused_window(&k).0, "win-1");
    let outcome = s.move_local_focused(&k, Direction::Right);
    assert_eq!(outcome.rule, Rule::R2c);
    let leaf1 = s.leaf("out-1", "ws-1", "win-1");
    let leaf2 = s.leaf("out-1", "ws-1", "win-2");
    assert_eq!(outcome.focus_leaf, leaf1);
    s.assert_geometry_complete(&k, &outcome.geometry);
    let tree = s.tree_of("out-1", "ws-1").expect("tree");
    match tree {
        Node::Group {
            children, shares, ..
        } => {
            assert_eq!(children.len(), 2);
            assert_eq!(shares.iter().sum::<u64>(), 3);
            assert_eq!(shares, vec![2, 1]);
            match &children[0] {
                Node::Group {
                    children, shares, ..
                } => {
                    assert_eq!(children.len(), 2);
                    assert_eq!(*shares, vec![1, 1]);
                    assert_eq!(children[0].id(), &leaf1);
                    assert_eq!(children[1].id(), &leaf2);
                }
                other => panic!("expected wrapper, got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
}

#[test]
fn flat_swap_nary_end_to_end_with_traveling_shares() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    for _ in 0..3 {
        s.focus_commit_focused(&k, Direction::Left);
    }
    assert_eq!(s.focused_window(&k).0, "win-1");
    s.move_local_focused(&k, Direction::Right);
    assert_eq!(s.focused_window(&k).0, "win-1");

    let focus_leaf_id = s.leaf("out-1", "ws-1", "win-1");
    let focus_leaf = focus_leaf_id.0.as_str();
    let pre_tree = s.tree_of("out-1", "ws-1").expect("tree");
    let (pre_children, pre_shares) = parent_children_shares(&pre_tree, focus_leaf);
    let pos = pre_children
        .iter()
        .position(|c| c == focus_leaf)
        .expect("focused child");
    assert!(pos + 1 < pre_children.len());

    let outcome =
        s.move_local_focused_with_mode(&k, Direction::Right, SameAxisMove::SwapWithNeighbor);
    assert_eq!(outcome.rule, Rule::R2c);
    assert_eq!(outcome.capability, Capability::SwapNeighbor);
    assert_eq!(outcome.focus_leaf.0, focus_leaf);

    let post_tree = s.tree_of("out-1", "ws-1").expect("tree");
    let (post_children, post_shares) = parent_children_shares(&post_tree, focus_leaf);
    assert_eq!(post_children.len(), pre_children.len());
    let mut expected_children = pre_children.clone();
    expected_children.swap(pos, pos + 1);
    assert_eq!(post_children, expected_children);
    let mut expected_shares = pre_shares.clone();
    expected_shares.swap(pos, pos + 1);
    assert_eq!(post_shares, expected_shares);
    s.assert_geometry_complete(&k, &outcome.geometry);

    let back = s.move_local_focused_with_mode(&k, Direction::Left, SameAxisMove::SwapWithNeighbor);
    assert_eq!(back.rule, Rule::R2c);
    assert_eq!(back.capability, Capability::SwapNeighbor);
    assert_eq!(back.focus_leaf.0, focus_leaf);
    let restored_tree = s.tree_of("out-1", "ws-1").expect("tree");
    let (restored_children, restored_shares) = parent_children_shares(&restored_tree, focus_leaf);
    assert_eq!(restored_children, pre_children);
    assert_eq!(restored_shares, pre_shares);
    s.assert_geometry_complete(&k, &back.geometry);
}

#[test]
fn flat_swap_group_end_escape_unchanged() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    for _ in 0..3 {
        s.focus_commit_focused(&k, Direction::Left);
    }
    let r2b = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b.rule, Rule::R2b);
    let r2c = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2c.rule, Rule::R2c);
    let r3 = s.move_local_focused_with_mode(&k, Direction::Left, SameAxisMove::SwapWithNeighbor);
    assert_eq!(r3.rule, Rule::R3);
    assert_eq!(r3.capability, Capability::ReparentLeaf);
    s.assert_geometry_complete(&k, &r3.geometry);
}

#[test]
fn r3_escape_with_r1_continuation() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let r1 = s.move_local_focused(&k, Direction::Down);
    assert_eq!(r1.rule, Rule::R1);
    let r3 = s.move_local_focused(&k, Direction::Down);
    assert_eq!(r3.rule, Rule::R3);
    assert_eq!(
        s.tree_of("out-1", "ws-1").unwrap().axis(),
        Some(Axis::Vertical)
    );
    s.assert_geometry_complete(&k, &r3.geometry);
}

#[test]
fn r3_escape_same_axis() {
    let mut s = MovementRig::single();
    for i in 1..=4 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", true);
    }
    let k = key("out-1", "ws-1");
    for _ in 0..3 {
        s.focus_commit_focused(&k, Direction::Left);
    }
    let r2b = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2b.rule, Rule::R2b);
    let r2c = s.move_local_focused(&k, Direction::Right);
    assert_eq!(r2c.rule, Rule::R2c);
    let r3 = s.move_local_focused(&k, Direction::Left);
    assert_eq!(r3.rule, Rule::R3);
    assert_eq!(r3.capability, Capability::ReparentLeaf);
    s.assert_geometry_complete(&k, &r3.geometry);
}

#[test]
fn r4_occupied_target() {
    let mut s = MovementRig::r4();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-2", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let outcome = s.move_pair_focused(&k1, &k2, Direction::Right);
    let PairMoveOutcome::Moved {
        rule,
        focus_leaf,
        geometry,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-2");
    assert_eq!(cross.target_workspace.0, "ws-1");
    let leaf3 = s.leaf("out-2", "ws-1", "win-3");
    assert_eq!(focus_leaf, leaf3);
    assert_eq!(s.layout.domain(&k2).unwrap().focused_leaf, Some(leaf3));
    assert_eq!(s.leaves("out-1", "ws-1"), vec!["win-1".to_string()]);
    assert_eq!(s.leaves("out-2", "ws-1").len(), 2);
    s.assert_pair_geometry_complete(&[&k1, &k2], &geometry);
}

#[test]
fn r4_empty_target() {
    let mut s = MovementRig::r4();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let outcome = s.move_pair_focused(&k1, &k2, Direction::Right);
    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-2");
    assert_eq!(cross.target_workspace.0, "ws-1");
    assert_eq!(s.leaves("out-2", "ws-1"), vec!["win-2".to_string()]);
    s.assert_pair_geometry_complete(&[&k1, &k2], &geometry);
}

#[test]
fn r4_edge_direction_domain_isolation() {
    let mut solo = MovementRig::single();
    solo.admit("a", "out-1", "ws-1", true);
    solo.admit("b", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let w = solo.focused_window(&k);
    let obs = solo.domain_observed(&k, vec![]);
    assert_eq!(
        solo.layout.local_move(
            &k,
            &w,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &w,
            &obs
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PlannerNoop))
    );
    let left = solo.move_local_focused(&k, Direction::Left);
    assert_ne!(left.rule, Rule::R4);

    let mut two_ws = MovementRig::new(vec![
        domain("out-1", "ws-1", 200, 200),
        domain("out-2", "ws-2", 200, 200),
    ]);
    two_ws.admit("w1", "out-1", "ws-1", true);
    two_ws.admit("w2", "out-1", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let w = two_ws.focused_window(&k1);
    let obs = two_ws.domain_observed(&k1, vec![]);
    assert_eq!(
        two_ws.layout.local_move(
            &k1,
            &w,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &w,
            &obs
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PlannerNoop))
    );

    let mut deep = MovementRig::single();
    deep.admit("w1", "out-1", "ws-1", true);
    deep.admit("w2", "out-1", "ws-1", true);
    deep.admit("w3", "out-1", "ws-1", false);
    deep.admit("w4", "out-1", "ws-1", false);
    let kd = key("out-1", "ws-1");
    let plan = deep.move_local_focused(&kd, Direction::Down);
    assert_eq!(plan.rule, Rule::R3);
}

#[test]
fn exact_opaque_scoping_move_and_focus() {
    let mut s = MovementRig::new(vec![
        domain("out-1", "ws-1", 200, 200),
        domain("out-2", "ws-1", 200, 200),
    ]);
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-2", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let obs1 = s.domain_observed(&k1, vec![]);
    assert_eq!(
        s.layout.local_move(
            &k1,
            &win("win-2"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("win-1"),
            &obs1
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );
    let obs2 = s.domain_observed(&k2, vec![]);
    assert_eq!(
        s.layout.local_move(
            &k2,
            &win("nope"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("win-2"),
            &obs2
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );

    let mut f = MovementRig::single();
    f.admit("a", "out-1", "ws-1", true);
    f.admit("b", "out-1", "ws-1", true);
    let kf = key("out-1", "ws-1");
    let obsf = f.domain_observed(&kf, vec![]);
    assert_eq!(
        f.layout
            .local_focus(&kf, &win("a"), Direction::Left, &win("b"), &obsf),
        Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
    assert_eq!(
        f.layout
            .local_focus(&kf, &win("nope"), Direction::Left, &win("b"), &obsf),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );
}

#[test]
fn focus_commit_and_refusals() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let before_tree = s.tree_of("out-1", "ws-1");
    let outcome = s.focus_commit_focused(&k, Direction::Left);
    let leaf2 = s.leaf("out-1", "ws-1", "win-2");
    assert_eq!(outcome.to_leaf, leaf2);
    assert_eq!(s.layout.domain(&k).unwrap().focused_leaf, Some(leaf2));
    assert_eq!(s.tree_of("out-1", "ws-1"), before_tree);
    s.assert_geometry_complete(&k, &outcome.geometry);

    let mut e = MovementRig::single();
    e.admit("solo", "out-1", "ws-1", true);
    let ke = key("out-1", "ws-1");
    let we = e.focused_window(&ke);
    let obse = e.domain_observed(&ke, vec![]);
    assert_eq!(
        e.layout.local_focus(&ke, &we, Direction::Left, &we, &obse),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );

    let mut pi = MovementRig::single();
    pi.admit("a", "out-1", "ws-1", true);
    pi.admit("b", "out-1", "ws-1", true);
    let kpi = key("out-1", "ws-1");
    let wpi = pi.focused_window(&kpi);
    assert_eq!(
        pi.layout
            .local_focus(&kpi, &wpi, Direction::Left, &wpi, &[]),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
}

#[test]
fn move_geometry_projection_fallback() {
    let mut s = MovementRig::new(vec![
        domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
        OutputDomain {
            id: OutputId("out-2".into()),
            workspace: WorkspaceId("ws-1".into()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            gap: 0,
            adjacent: [(Direction::Left, OutputId("out-1".into()))]
                .into_iter()
                .collect(),
        },
    ]);
    s.admit("a", "out-1", "ws-1", true);
    s.admit("b", "out-2", "ws-1", true);
    s.admit("c", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let target = key("out-2", "ws-1");
    let dest_tree_before = s.tree_of("out-2", "ws-1").expect("target tree");
    let w = s.focused_window(&k);
    assert_eq!(w.0, "c");

    let outcome = s.move_pair_focused(&k, &target, Direction::Right);
    let PairMoveOutcome::CapacityFloat {
        window,
        target_output,
        target_workspace,
        geometry,
        ..
    } = outcome
    else {
        panic!("expected capacity float fallback");
    };
    assert_eq!(window, w);
    assert_eq!(target_output.0, "out-2");
    assert_eq!(target_workspace.0, "ws-1");
    assert!(geometry.iter().all(|g| g.window != w));
    let target_dom = s.layout.domain(&target).expect("target domain");
    assert!(target_dom.capacity_floats.contains(&w));
    assert_eq!(
        target_dom
            .exceptions
            .get(&w)
            .and_then(|e| e.floating_geometry),
        None
    );
    assert_eq!(s.tree_of("out-2", "ws-1"), Some(dest_tree_before));
}

#[test]
fn deep_nary_route_preserves_order_and_shares() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-1", "ws-1", true);
    s.admit("win-3", "out-1", "ws-1", false);
    s.admit("win-4", "out-1", "ws-1", false);
    s.admit("win-5", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let p1 = s.move_local_focused(&k, Direction::Up);
    s.assert_geometry_complete(&k, &p1.geometry);
    assert_eq!(
        s.layout.domain(&k).unwrap().focused_leaf,
        Some(p1.focus_leaf)
    );
    let p2 = s.move_local_focused(&k, Direction::Down);
    s.assert_geometry_complete(&k, &p2.geometry);
    let tree = s.tree_of("out-1", "ws-1").expect("tree");
    let mut ids = BTreeSet::new();
    check_shares(&tree, &mut ids);
    assert_eq!(ids.len(), 5 + count_groups(&tree));
}

#[test]
fn navigation_deterministic_focus_commit() {
    fn run() -> (LocalFocusOutcome, NodeId) {
        let mut s = MovementRig::single();
        s.admit("win-1", "out-1", "ws-1", true);
        s.admit("win-2", "out-1", "ws-1", true);
        s.admit("win-3", "out-1", "ws-1", true);
        let k = key("out-1", "ws-1");
        let outcome = s.focus_commit_focused(&k, Direction::Left);
        (outcome, s.leaf("out-1", "ws-1", "win-2"))
    }
    let a = run();
    let b = run();
    assert_eq!(a.0, b.0);
    assert_eq!(a.0.to_leaf, a.1);
}

#[test]
fn domain_isolation_outside_r4() {
    let mut s = MovementRig::new(vec![
        domain("out-1", "ws-1", 200, 200),
        domain("out-2", "ws-2", 200, 200),
    ]);
    s.admit("win-1", "out-1", "ws-1", true);
    s.admit("win-2", "out-2", "ws-2", true);
    s.admit("win-3", "out-1", "ws-1", true);
    let before_other = s.leaves("out-2", "ws-2");
    let k = key("out-1", "ws-1");
    let outcome = s.move_local_focused(&k, Direction::Left);
    assert_eq!(outcome.rule, Rule::R2a);
    assert_eq!(s.leaves("out-2", "ws-2"), before_other);
    assert!(
        outcome
            .geometry
            .iter()
            .all(|g| g.output.0 == "out-1" && g.workspace.0 == "ws-1")
    );
}

#[test]
fn refusals_unknown_focus_incomplete_noop() {
    let mut s = MovementRig::single();
    s.admit("win-1", "out-1", "ws-1", true);
    let k = key("out-1", "ws-1");
    let w1 = s.focused_window(&k);
    let obs = s.domain_observed(&k, vec![]);
    assert_eq!(
        s.layout.local_move(
            &key("nope", "ws-1"),
            &w1,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &w1,
            &obs
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain))
    );

    s.admit("win-2", "out-1", "ws-1", true);
    let before_dom = s.layout.domain(&k).unwrap().clone();
    let obs2 = s.domain_observed(&k, vec![]);
    assert_eq!(
        s.layout.local_move(
            &k,
            &win("win-1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("win-1"),
            &[]
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    assert_eq!(
        s.layout.local_move(
            &k,
            &win("win-1"),
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &win("win-2"),
            &obs2
        ),
        Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
    let after_dom = s.layout.domain(&k).unwrap();
    assert_eq!(after_dom.accepted_revision, before_dom.accepted_revision);
    assert_eq!(after_dom.tree, before_dom.tree);
    assert_eq!(after_dom.focused_leaf, before_dom.focused_leaf);

    let mut solo = MovementRig::single();
    solo.admit("only", "out-1", "ws-1", true);
    let ks = key("out-1", "ws-1");
    let before_solo = solo.layout.domain(&ks).unwrap().clone();
    let wsolo = solo.focused_window(&ks);
    let obs_solo = solo.domain_observed(&ks, vec![]);
    assert_eq!(
        solo.layout.local_move(
            &ks,
            &wsolo,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &wsolo,
            &obs_solo
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PlannerNoop))
    );
    let after_solo = solo.layout.domain(&ks).unwrap();
    assert_eq!(after_solo.accepted_revision, before_solo.accepted_revision);
    assert_eq!(after_solo.tree, before_solo.tree);
    assert_eq!(after_solo.focused_leaf, before_solo.focused_leaf);
}

#[test]
fn invalid_geometry_admits_capacity_float() {
    let mut tiny = MovementRig::new(vec![domain("tiny", "ws-1", 1, 1)]);
    tiny.admit("a", "tiny", "ws-1", true);
    let k = key("tiny", "ws-1");
    let obs = tiny.domain_observed(&k, vec![tiled("b", "tiny", "ws-1")]);
    let outcome = tiny
        .layout
        .admit_window(&k, &win("b"), placement(true), false, false, &obs)
        .expect("exhaustion floats instead of refusing");
    assert!(outcome.admitted_leaf.is_none());
    assert!(outcome.geometry.is_empty());
    let dom = tiny.layout.domain(&k).unwrap();
    assert!(dom.exceptions.contains_key(&win("b")));
    assert!(dom.capacity_floats.contains(&win("b")));
    assert!(!dom.automatic_fixed.contains(&win("b")));
    assert_eq!(dom.leaves.len(), 1);
}

#[test]
fn deterministic_replay_of_moves() {
    fn run() -> (Option<Node>, Option<NodeId>, Vec<LocalMoveOutcome>) {
        let mut s = MovementRig::single();
        s.admit("win-1", "out-1", "ws-1", true);
        s.admit("win-2", "out-1", "ws-1", true);
        s.admit("win-3", "out-1", "ws-1", true);
        let k = key("out-1", "ws-1");
        let p1 = s.move_local_focused(&k, Direction::Left);
        let p2 = s.move_local_focused(&k, Direction::Down);
        let dom = s.layout.domain(&k).unwrap();
        (dom.tree.clone(), dom.focused_leaf.clone(), vec![p1, p2])
    }
    assert_eq!(run(), run());

    fn run_focus() -> Option<NodeId> {
        let mut s = MovementRig::single();
        s.admit("win-1", "out-1", "ws-1", true);
        s.admit("win-2", "out-1", "ws-1", true);
        s.admit("win-3", "out-1", "ws-1", true);
        let k = key("out-1", "ws-1");
        s.focus_commit_focused(&k, Direction::Left);
        s.layout.domain(&k).unwrap().focused_leaf.clone()
    }
    assert_eq!(run_focus(), run_focus());
}

#[test]
fn adjacency_validation_strict() {
    let mut one_way = MovementRig::new(vec![
        domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
        domain("out-2", "ws-1", 200, 200),
    ]);
    one_way.admit("win-1", "out-1", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let w1 = one_way.focused_window(&k1);
    let obs1 = one_way.all_engine_windows();
    let ctx1 = PairOperationContext {
        source_domain: &one_way.domains[&k1],
        target_domain: &one_way.domains[&k2],
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w1,
        observed: &obs1,
        closed: &[],
    };
    assert!(matches!(
        one_way.layout.run_directional_pair_move(
            &ctx1,
            &w1,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor
        ),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::DomainMismatch
        ))
    ));

    let mut unknown = MovementRig::new(vec![
        domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "nope")]),
        domain("out-2", "ws-1", 200, 200),
    ]);
    unknown.admit("win-1", "out-1", "ws-1", true);
    let w_unk = unknown.focused_window(&k1);
    let obs_unk = unknown.all_engine_windows();
    let ctx_unk = PairOperationContext {
        source_domain: &unknown.domains[&k1],
        target_domain: &unknown.domains[&k2],
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w_unk,
        observed: &obs_unk,
        closed: &[],
    };
    assert!(matches!(
        unknown.layout.run_directional_pair_move(
            &ctx_unk,
            &w_unk,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor
        ),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::DomainMismatch
        ))
    ));

    let mut self_adj = MovementRig::new(vec![domain_with_adjacent(
        "out-1",
        "ws-1",
        vec![(Direction::Right, "out-1")],
    )]);
    self_adj.admit("win-1", "out-1", "ws-1", true);
    let w_self = self_adj.focused_window(&k1);
    let obs_self = self_adj.all_engine_windows();
    let ctx_self = PairOperationContext {
        source_domain: &self_adj.domains[&k1],
        target_domain: &self_adj.domains[&k1],
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w_self,
        observed: &obs_self,
        closed: &[],
    };
    assert!(matches!(
        self_adj.layout.run_directional_pair_move(
            &ctx_self,
            &w_self,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor
        ),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::DomainMismatch
        ))
    ));

    let mut cross_ws = MovementRig::new(vec![
        domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
        domain_with_adjacent("out-2", "ws-2", vec![(Direction::Left, "out-1")]),
    ]);
    cross_ws.admit("win-1", "out-1", "ws-1", true);
    let k2_ws2 = key("out-2", "ws-2");
    let w_cross = cross_ws.focused_window(&k1);
    let obs_cross = cross_ws.all_engine_windows();
    let ctx_cross = PairOperationContext {
        source_domain: &cross_ws.domains[&k1],
        target_domain: &cross_ws.domains[&k2_ws2],
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w_cross,
        observed: &obs_cross,
        closed: &[],
    };
    assert!(
        cross_ws
            .layout
            .run_directional_pair_move(
                &ctx_cross,
                &w_cross,
                Direction::Right,
                true,
                SameAxisMove::GroupWithNeighbor
            )
            .is_ok()
    );
}

#[test]
fn bounded_property_loop_over_accepted_moves() {
    let mut s = MovementRig::single();
    for i in 1..=5 {
        s.admit(&format!("win-{i}"), "out-1", "ws-1", i % 2 == 1);
    }
    let k = key("out-1", "ws-1");
    let dirs = [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ];
    for i in 0..24 {
        let dir = dirs[i % dirs.len()];
        let obs = s.domain_observed(&k, vec![]);
        let before_dom = s.layout.domain(&k).unwrap().clone();
        let w = s.focused_window(&k);
        match s
            .layout
            .local_move(&k, &w, dir, SameAxisMove::GroupWithNeighbor, &w, &obs)
        {
            Ok(outcome) => {
                assert_eq!(outcome.base_revision, before_dom.accepted_revision);
                assert_eq!(outcome.revision, before_dom.accepted_revision + 1);
                s.assert_geometry_complete(&k, &outcome.geometry);
                let after_dom = s.layout.domain(&k).unwrap();
                assert_eq!(
                    after_dom.accepted_revision,
                    before_dom.accepted_revision + 1
                );
                assert_eq!(after_dom.focused_leaf, Some(outcome.focus_leaf));
            }
            Err(LayoutFailure::Semantics(_)) => {
                let after_dom = s.layout.domain(&k).unwrap();
                assert_eq!(after_dom.tree, before_dom.tree);
                assert_eq!(after_dom.focused_leaf, before_dom.focused_leaf);
                assert_eq!(after_dom.accepted_revision, before_dom.accepted_revision);
            }
            Err(other) => panic!("loop must not fail with {other:?}"),
        }
    }
}

#[test]
fn portable_session_movement_prohibits_platform_imports() {
    const SESSION: &str = include_str!("../session.rs");
    const BANNED: &[&str] = &[
        "zbus",
        "rustix",
        "dbus",
        "tokio",
        "async-std",
        "smol",
        "wayland",
        "x11",
        "kwin",
        "libc",
        "nix",
        "socket",
        "ipc",
        "process",
        "platform",
        "std::process",
        "std::net",
        "std::os",
        "std::fs",
        "std::env",
        "std::thread",
        "ffi",
    ];
    for raw_line in SESSION.lines() {
        let line = raw_line.trim().to_ascii_lowercase();
        if !(line.starts_with("use ")
            || line.starts_with("extern crate")
            || line.starts_with("mod "))
        {
            continue;
        }
        for token in BANNED {
            assert!(
                !line.contains(token),
                "import {raw_line:?} contains {token:?}"
            );
        }
    }
}

#[test]
fn r4_vertical_cross_occupied_landing_nearest_source() {
    let mut s = MovementRig::stacked();
    s.admit("win-t", "out-bottom", "ws-1", true);
    s.admit("win-1", "out-top", "ws-1", false);
    s.admit("win-2", "out-top", "ws-1", false);
    let k_top = key("out-top", "ws-1");
    let k_bot = key("out-bottom", "ws-1");
    assert_eq!(s.focused_window(&k_top).0, "win-2");

    let outcome = s.move_pair_focused(&k_top, &k_bot, Direction::Down);
    let PairMoveOutcome::Moved {
        rule,
        focus_leaf,
        geometry,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-bottom");
    assert_eq!(cross.target_workspace.0, "ws-1");
    let leaf2 = s.leaf("out-bottom", "ws-1", "win-2");
    let leaft = s.leaf("out-bottom", "ws-1", "win-t");
    assert_eq!(focus_leaf, leaf2);
    let target = s.tree_of("out-bottom", "ws-1").expect("target tree");
    let (axis, children) = root_children(&target);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], leaf2.0, "mover nearest the source above");
    assert_eq!(children[1], leaft.0);
    assert_eq!(s.leaves("out-top", "ws-1"), vec!["win-1".to_string()]);
    s.assert_pair_geometry_complete(&[&k_top, &k_bot], &geometry);
}

#[test]
fn r4_vertical_cross_up_landing_nearest_source() {
    let mut s = MovementRig::stacked();
    s.admit("win-t", "out-top", "ws-1", true);
    s.admit("win-1", "out-bottom", "ws-1", false);
    s.admit("win-2", "out-bottom", "ws-1", false);
    let k_bot = key("out-bottom", "ws-1");
    let k_top = key("out-top", "ws-1");
    assert_eq!(s.focused_window(&k_bot).0, "win-2");
    s.focus_commit_focused(&k_bot, Direction::Up);
    assert_eq!(s.focused_window(&k_bot).0, "win-1");

    let outcome = s.move_pair_focused(&k_bot, &k_top, Direction::Up);
    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-top");
    assert_eq!(cross.target_workspace.0, "ws-1");
    let target = s.tree_of("out-top", "ws-1").expect("target tree");
    let (axis, children) = root_children(&target);
    assert_eq!(axis, Axis::Vertical);
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], s.leaf("out-top", "ws-1", "win-t").0);
    assert_eq!(
        children[1],
        s.leaf("out-top", "ws-1", "win-1").0,
        "mover nearest the source below"
    );
    assert_eq!(s.leaves("out-bottom", "ws-1"), vec!["win-2".to_string()]);
    s.assert_pair_geometry_complete(&[&k_bot, &k_top], &geometry);
}

#[test]
fn r4_sole_leaf_cross_empties_source_all_directions() {
    for (direction, target_output) in [
        (Direction::Left, "out-w"),
        (Direction::Right, "out-e"),
        (Direction::Up, "out-n"),
        (Direction::Down, "out-s"),
    ] {
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let mut s = MovementRig::new(vec![
            domain_with_adjacent("out-c", "ws-1", vec![(direction, target_output)]),
            domain_with_adjacent(target_output, "ws-1", vec![(opposite, "out-c")]),
        ]);
        s.admit("solo", "out-c", "ws-1", true);
        let kc = key("out-c", "ws-1");
        let kt = key(target_output, "ws-1");
        let outcome = s.move_pair_focused(&kc, &kt, direction);
        let PairMoveOutcome::Moved {
            rule,
            focus_leaf,
            geometry,
            cross: Some(cross),
            ..
        } = outcome
        else {
            panic!("expected cross move");
        };
        assert_eq!(rule, Rule::R4, "{direction:?}");
        assert_eq!(cross.target_output.0, target_output);
        assert_eq!(cross.target_workspace.0, "ws-1");
        assert_eq!(focus_leaf, s.leaf(target_output, "ws-1", "solo"));
        assert_eq!(s.tree_of("out-c", "ws-1"), None, "{direction:?}");
        assert_eq!(
            s.leaves(target_output, "ws-1"),
            vec!["solo".to_string()],
            "{direction:?}"
        );
        s.assert_pair_geometry_complete(&[&kc, &kt], &geometry);
    }
}

#[test]
fn r4_ambiguous_adjacency_refuses_end_to_end() {
    let mut bad = MovementRig::new(vec![
        domain_with_adjacent("out-1", "ws-1", vec![(Direction::Right, "out-2")]),
        domain("out-2", "ws-1", 200, 200),
    ]);
    bad.admit("win-1", "out-1", "ws-1", true);
    let k1 = key("out-1", "ws-1");
    let k2 = key("out-2", "ws-1");
    let w = bad.focused_window(&k1);
    let obs = bad.all_engine_windows();
    let ctx = PairOperationContext {
        source_domain: &bad.domains[&k1],
        target_domain: &bad.domains[&k2],
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w,
        observed: &obs,
        closed: &[],
    };
    assert_eq!(
        bad.layout.run_directional_pair_move(
            &ctx,
            &w,
            Direction::Right,
            true,
            SameAxisMove::GroupWithNeighbor
        ),
        Err(PairOperationFailure::Assembly(
            PairAssemblyFailure::DomainMismatch
        ))
    );

    let mut s = MovementRig::stacked();
    s.admit("win-1", "out-top", "ws-1", true);
    s.admit("win-2", "out-top", "ws-1", true);
    let k = key("out-top", "ws-1");
    let before = s.layout.domain(&k).unwrap().clone();
    let w = s.focused_window(&k);
    let obs = s.domain_observed(&k, vec![]);
    assert_eq!(
        s.layout.local_move(
            &k,
            &w,
            Direction::Right,
            SameAxisMove::GroupWithNeighbor,
            &w,
            &obs
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PlannerNoop))
    );
    assert_eq!(s.layout.domain(&k).unwrap().tree, before.tree);
    assert_eq!(
        s.layout.domain(&k).unwrap().accepted_revision,
        before.accepted_revision
    );
}

#[test]
fn r4_sole_leaf_cross_occupied_landing_nearest_source_all_directions() {
    for (direction, target_output, mover_first) in [
        (Direction::Left, "out-w", false),
        (Direction::Right, "out-e", true),
        (Direction::Up, "out-n", false),
        (Direction::Down, "out-s", true),
    ] {
        let opposite = match direction {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        };
        let mut s = MovementRig::new(vec![
            domain_with_adjacent("out-c", "ws-1", vec![(direction, target_output)]),
            domain_with_adjacent(target_output, "ws-1", vec![(opposite, "out-c")]),
        ]);
        s.admit("stay", target_output, "ws-1", true);
        s.admit("solo", "out-c", "ws-1", true);
        let kc = key("out-c", "ws-1");
        let kt = key(target_output, "ws-1");
        let outcome = s.move_pair_focused(&kc, &kt, direction);
        let PairMoveOutcome::Moved {
            rule,
            focus_leaf,
            geometry,
            cross: Some(cross),
            ..
        } = outcome
        else {
            panic!("expected cross move");
        };
        assert_eq!(rule, Rule::R4, "{direction:?}");
        assert_eq!(cross.target_output.0, target_output);
        assert_eq!(cross.target_workspace.0, "ws-1");
        let tree = s.tree_of(target_output, "ws-1").expect("target tree");
        let (axis, children) = root_children(&tree);
        assert_eq!(axis, Axis::for_direction(direction), "{direction:?}");
        assert_eq!(children.len(), 2, "{direction:?}");
        let leaf_solo = s.leaf(target_output, "ws-1", "solo");
        let leaf_stay = s.leaf(target_output, "ws-1", "stay");
        let expected = if mover_first {
            vec![leaf_solo.0.clone(), leaf_stay.0]
        } else {
            vec![leaf_stay.0, leaf_solo.0.clone()]
        };
        assert_eq!(children, expected, "mover nearest source {direction:?}");
        assert_eq!(focus_leaf, leaf_solo);
        assert_eq!(s.tree_of("out-c", "ws-1"), None, "{direction:?}");
        s.assert_pair_geometry_complete(&[&kc, &kt], &geometry);
    }
}

trait NodeAxisExt {
    fn axis(&self) -> Option<Axis>;
}

impl NodeAxisExt for Node {
    fn axis(&self) -> Option<Axis> {
        match self {
            Node::Leaf { .. } => None,
            Node::Group { axis, .. } => Some(*axis),
        }
    }
}
