//! Cross-output directional focus/movement with distinct active workspaces.
//!
//! Typed `WorldLayout` tests for directional focus and movement across
//! adjacent outputs with distinct workspaces: `Left/Right` cross horizontally
//! adjacent outputs (and `Up/Down` vertically adjacent outputs) only after
//! local focus/move is exhausted, targeting the adjacent output's currently
//! selected logical workspace.

use std::collections::BTreeMap;

use crate::directional::{
    Axis, Capability, Direction, Node, OutputId, Rule, SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{
    DesiredGeometry, DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind,
};
use crate::size_hints::WindowSizeHints;

use super::outcome::{LayoutFailure, PairMoveOutcome, PairOperationFailure};
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn domain_ws(
    output: &str,
    workspace: &str,
    adjacent: Vec<(Direction, &str)>,
    gap: i32,
) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
        gap,
        adjacent: adjacent
            .into_iter()
            .map(|(d, t)| (d, OutputId(t.to_owned())))
            .collect(),
    }
}

struct TwoOutputRig {
    layout: WorldLayout,
    d1: OutputDomain,
    d2: OutputDomain,
}

impl TwoOutputRig {
    fn new(d1: OutputDomain, d2: OutputDomain) -> Self {
        Self {
            layout: WorldLayout::new(),
            d1,
            d2,
        }
    }

    fn domain_desc(&self, k: &DomainKey) -> &OutputDomain {
        if self.d1.key() == *k {
            &self.d1
        } else if self.d2.key() == *k {
            &self.d2
        } else {
            panic!("unknown domain {k:?}")
        }
    }

    fn other_desc(&self, k: &DomainKey) -> &OutputDomain {
        if self.d1.key() == *k {
            &self.d2
        } else if self.d2.key() == *k {
            &self.d1
        } else {
            panic!("unknown domain {k:?}")
        }
    }

    fn domain_observed(&self, k: &DomainKey) -> Vec<ObservedWindow> {
        let Some(dom) = self.layout.domain(k) else {
            return Vec::new();
        };
        let mut windows: Vec<ObservedWindow> = dom
            .leaves
            .keys()
            .map(|w| ObservedWindow {
                window: w.clone(),
                output: k.output.clone(),
                workspace: k.workspace.clone(),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: WindowSizeHints::none(),
            })
            .collect();
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

    fn admit(&mut self, window: &str, output: &str, workspace: &str) {
        self.admit_with_placement(
            window,
            output,
            workspace,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
        );
    }

    fn admit_with_placement(
        &mut self,
        window: &str,
        output: &str,
        workspace: &str,
        placement_bounds: Rect,
    ) {
        let k = key(output, workspace);
        let desc = self.domain_desc(&k).clone();
        self.layout
            .domains
            .entry(k.clone())
            .or_insert_with(|| DomainLayout::new(desc.bounds, desc.gap, 0));
        let mut observed = self.domain_observed(&k);
        observed.push(ObservedWindow {
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
        });
        observed.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        self.layout
            .admit_window(
                &k,
                &win(window),
                ExceptionFlags::none(),
                None,
                placement_bounds,
                false,
                false,
                &observed,
            )
            .expect("admit");
    }

    fn focused_window(&self, k: &DomainKey) -> WindowId {
        let dom = self.layout.domain(k).expect("domain");
        let focus_leaf = dom.focused_leaf.as_ref().expect("focused leaf");
        dom.leaves
            .iter()
            .find_map(|(w, l)| (l == focus_leaf).then(|| w.clone()))
            .expect("focused window")
    }

    fn window_in(&self, k: &DomainKey, window: &str) -> WindowId {
        let dom = self.layout.domain(k).expect("domain");
        assert!(dom.leaves.contains_key(&win(window)), "missing {window}");
        win(window)
    }

    fn focus_local(&mut self, k: &DomainKey, window: &WindowId, direction: Direction) {
        let obs = self.domain_observed(k);
        self.layout
            .local_focus(k, window, direction, window, &obs)
            .unwrap_or_else(|e| panic!("local focus {direction:?}: {e:?}"));
    }

    fn move_pair(&mut self, source: &DomainKey, direction: Direction) -> PairMoveOutcome {
        let subject = self.focused_window(source);
        let src_dom = self.domain_desc(source).clone();
        let dst_dom = self.other_desc(source).clone();
        let obs = self.all_engine_windows();
        let ctx = PairOperationContext {
            source_domain: &src_dom,
            target_domain: &dst_dom,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: &subject,
            observed: &obs,
            closed: &[],
        };
        self.layout
            .run_directional_pair_move(
                &ctx,
                &subject,
                direction,
                true,
                SameAxisMove::GroupWithNeighbor,
            )
            .expect("pair move")
    }

    fn focus_pair(
        &mut self,
        source: &DomainKey,
        window: &WindowId,
        direction: Direction,
    ) -> super::outcome::PairFocusOutcome {
        let src_dom = self.domain_desc(source).clone();
        let dst_dom = self.other_desc(source).clone();
        let obs = self.all_engine_windows();
        let ctx = PairOperationContext {
            source_domain: &src_dom,
            target_domain: &dst_dom,
            source_outer_gap: 0,
            target_outer_gap: Some(0),
            observed_focus: window,
            observed: &obs,
            closed: &[],
        };
        self.layout
            .run_directional_pair_focus(&ctx, window, direction, false)
            .expect("pair focus")
    }

    fn tree_of(&self, output: &str, workspace: &str) -> Option<Node> {
        self.layout
            .domain(&key(output, workspace))
            .and_then(|d| d.tree.clone())
    }

    fn root_axis(&self, output: &str, workspace: &str) -> Option<Axis> {
        match self.tree_of(output, workspace)? {
            Node::Leaf { .. } => None,
            Node::Group { axis, .. } => Some(axis),
        }
    }

    fn root_id(&self, output: &str, workspace: &str) -> Option<String> {
        self.tree_of(output, workspace).map(|t| match t {
            Node::Leaf { id } => id.0,
            Node::Group { id, .. } => id.0,
        })
    }

    fn parent_of(
        &self,
        output: &str,
        workspace: &str,
        leaf: &str,
    ) -> Option<(String, Axis, Vec<String>)> {
        fn find(node: &Node, leaf: &str) -> Option<(String, Axis, Vec<String>)> {
            match node {
                Node::Leaf { .. } => None,
                Node::Group {
                    id, axis, children, ..
                } => {
                    for child in children {
                        if let Node::Leaf { id: cid } = child
                            && cid.0 == leaf
                        {
                            return Some((
                                id.0.clone(),
                                *axis,
                                children
                                    .iter()
                                    .map(|c| match c {
                                        Node::Leaf { id } => id.0.clone(),
                                        Node::Group { id, .. } => id.0.clone(),
                                    })
                                    .collect(),
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
        find(&self.tree_of(output, workspace)?, leaf)
    }

    fn leaves(&self, output: &str, workspace: &str) -> Vec<String> {
        match self.tree_of(output, workspace) {
            Some(tree) => {
                fn collect(t: &Node, out: &mut Vec<String>) {
                    match t {
                        Node::Leaf { id } => out.push(id.0.clone()),
                        Node::Group { children, .. } => {
                            for c in children {
                                collect(c, out);
                            }
                        }
                    }
                }
                let mut out = Vec::new();
                collect(&tree, &mut out);
                out
            }
            None => Vec::new(),
        }
    }

    fn assert_geometry_complete_for(
        &self,
        geometry: &[DesiredGeometry],
        affected: &[(&str, &str)],
    ) {
        assert!(!geometry.is_empty());
        let mut expected = 0usize;
        for (output, workspace) in affected {
            let k = key(output, workspace);
            expected += self.layout.domain(&k).map_or(0, |d| d.leaves.len());
            let domain = self.domain_desc(&k);
            for g in geometry
                .iter()
                .filter(|g| g.output.0 == *output && g.workspace.0 == *workspace)
            {
                assert!(
                    g.rect.w > 0 && g.rect.h > 0,
                    "positive rect for {}",
                    g.window.0
                );
                assert!(
                    g.rect.x >= domain.bounds.x
                        && g.rect.y >= domain.bounds.y
                        && g.rect.x + g.rect.w <= domain.bounds.x + domain.bounds.w
                        && g.rect.y + g.rect.h <= domain.bounds.y + domain.bounds.h,
                    "rect of {} contained in {output}/{workspace} bounds",
                    g.window.0,
                );
            }
        }
        assert_eq!(
            geometry.len(),
            expected,
            "geometry covers exactly the affected tiled windows"
        );
    }
}

fn two_output_rig() -> TwoOutputRig {
    TwoOutputRig::new(
        domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")], 4),
        domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")], 8),
    )
}

fn defect_two_output_rig() -> TwoOutputRig {
    TwoOutputRig::new(
        OutputDomain {
            id: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-a".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 2048,
                h: 1152,
            },
            gap: 0,
            adjacent: [(Direction::Right, OutputId("out-2".to_owned()))]
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
        },
        OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-b".to_owned()),
            bounds: Rect {
                x: 2048,
                y: 72,
                w: 1920,
                h: 1080,
            },
            gap: 0,
            adjacent: [(Direction::Left, OutputId("out-1".to_owned()))]
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
        },
    )
}

fn stacked_two_output_rig() -> TwoOutputRig {
    TwoOutputRig::new(
        domain_ws("out-top", "ws-a", vec![(Direction::Down, "out-bottom")], 0),
        domain_ws("out-bottom", "ws-b", vec![(Direction::Up, "out-top")], 0),
    )
}

#[test]
fn cross_workspace_s20_occupied_single_leaf() {
    let mut s = two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-a", "out-2", "ws-b");
    s.admit("win-b", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let current = s.focused_window(&k2);
    if current.0 != "win-a" {
        s.focus_local(&k2, &current, Direction::Left);
    }
    assert_eq!(s.focused_window(&k2).0, "win-a");

    let outcome = s.move_pair(&k2, Direction::Left);
    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross-output move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-b".to_string()]);
    let target_leaves = s.leaves("out-1", "ws-a");
    assert_eq!(target_leaves.len(), 2);
    assert!(target_leaves.contains(&"leaf-win-a".to_string()));
    assert!(!geometry.is_empty());
    for g in &geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
    }
}

#[test]
fn cross_workspace_s21_perpendicular_no_cross() {
    let mut s = two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-a", "out-2", "ws-b");
    s.admit_with_placement(
        "win-b",
        "out-2",
        "ws-b",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k2 = key("out-2", "ws-b");
    assert_eq!(s.root_axis("out-2", "ws-b"), Some(Axis::Vertical));
    assert_eq!(
        s.leaves("out-2", "ws-b"),
        vec!["leaf-win-a".to_string(), "leaf-win-b".to_string()]
    );
    let focused_b = s.focused_window(&k2);
    assert_eq!(focused_b.0, "win-b");
    s.focus_local(&k2, &focused_b, Direction::Up);
    let window_a = s.focused_window(&k2);
    assert_eq!(window_a.0, "win-a");

    let target_tree_before = s.tree_of("out-1", "ws-a");
    let out1_leaves_before = s
        .layout
        .domain(&key("out-1", "ws-a"))
        .unwrap()
        .leaves
        .clone();
    let out2_leaves_before = s.layout.domain(&k2).unwrap().leaves.clone();

    let outcome = s.move_pair(&k2, Direction::Left);
    let PairMoveOutcome::Moved {
        rule,
        capability,
        focus_leaf,
        geometry,
        cross,
        ..
    } = outcome
    else {
        panic!("expected local moved outcome");
    };
    assert_eq!(rule, Rule::R1);
    assert_eq!(capability, Capability::WrapPerpendicular);
    assert!(cross.is_none());
    assert_eq!(s.root_axis("out-2", "ws-b"), Some(Axis::Horizontal));
    assert_eq!(
        s.leaves("out-2", "ws-b"),
        vec!["leaf-win-a".to_string(), "leaf-win-b".to_string()]
    );
    assert_eq!(s.tree_of("out-1", "ws-a"), target_tree_before);
    assert_eq!(focus_leaf.0, "leaf-win-a");
    assert_eq!(
        s.layout.domain(&key("out-1", "ws-a")).unwrap().leaves,
        out1_leaves_before
    );
    assert_eq!(s.layout.domain(&k2).unwrap().leaves, out2_leaves_before);
    assert!(
        geometry
            .iter()
            .all(|g| g.output.0 == "out-2" && g.workspace.0 == "ws-b")
    );
}

#[test]
fn cross_workspace_s22_empty_target() {
    let mut s = two_output_rig();
    s.admit("win-a", "out-2", "ws-b");
    s.admit("win-b", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let current = s.focused_window(&k2);
    if current.0 != "win-a" {
        s.focus_local(&k2, &current, Direction::Left);
    }
    assert_eq!(s.leaves("out-1", "ws-a"), Vec::<String>::new());

    let outcome = s.move_pair(&k2, Direction::Left);
    let PairMoveOutcome::Moved {
        rule,
        cross: Some(cross),
        ..
    } = outcome
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    assert_eq!(s.leaves("out-1", "ws-a"), vec!["leaf-win-a".to_string()]);
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-b".to_string()]);
}

#[test]
fn cross_workspace_occupied_multiwindow_wraps_whole_target() {
    let mut s = two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-y", "out-1", "ws-a");
    s.admit_with_placement(
        "win-z",
        "out-1",
        "ws-a",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let k1 = key("out-1", "ws-a");
    let target_root_before = s.root_id("out-1", "ws-a").expect("target root");
    let z_parent_before = s
        .parent_of("out-1", "ws-a", "leaf-win-z")
        .expect("Z parent");
    assert_ne!(
        z_parent_before.0, target_root_before,
        "Z starts nested, not at root"
    );

    s.admit("win-a", "out-2", "ws-b");
    s.admit("win-b", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let focused_b = s.focused_window(&k2);
    assert_eq!(focused_b.0, "win-b");
    s.focus_local(&k2, &focused_b, Direction::Left);
    assert_eq!(s.focused_window(&k2).0, "win-a");

    let outcome = s.move_pair(&k2, Direction::Left);
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
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-b".to_string()]);

    let mover_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-a")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-1", "ws-a").expect("new root"),
        "mover sits at the new target root"
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-a".to_string()],
        "mover lands last (nearest the eastern source)"
    );
    let z_parent_after = s
        .parent_of("out-1", "ws-a", "leaf-win-z")
        .expect("Z parent");
    assert_eq!(z_parent_after.0, z_parent_before.0);
    assert_eq!(z_parent_after.1, Axis::Vertical);
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-x".to_string(),
            "leaf-win-y".to_string(),
            "leaf-win-z".to_string(),
            "leaf-win-a".to_string(),
        ]
    );
    assert_eq!(focus_leaf.0, "leaf-win-a");
    assert!(
        s.layout
            .domain(&k1)
            .unwrap()
            .leaves
            .contains_key(&win("win-a"))
    );
    assert_eq!(
        s.layout
            .domain(&k1)
            .unwrap()
            .focused_leaf
            .as_ref()
            .map(|l| l.0.as_str()),
        Some("leaf-win-a")
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn cross_output_focus_selects_target_last_focus() {
    let mut s = two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-a", "out-2", "ws-b");
    s.admit("win-b", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let current = s.focused_window(&k2);
    if current.0 != "win-a" {
        s.focus_local(&k2, &current, Direction::Left);
    }
    let window_a = s.focused_window(&k2);
    assert_eq!(window_a.0, "win-a");

    let local_obs = s.domain_observed(&k2);
    assert_eq!(
        s.layout
            .local_focus(&k2, &window_a, Direction::Left, &window_a, &local_obs),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );

    let before_d1 = s.layout.domain(&key("out-1", "ws-a")).unwrap().tree.clone();
    let before_d2 = s.layout.domain(&k2).unwrap().tree.clone();
    let src_dom = s.domain_desc(&k2).clone();
    let dst_dom = s.other_desc(&k2).clone();
    let obs = s.all_engine_windows();
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &window_a,
        observed: &obs,
        closed: &[],
    };
    assert_eq!(
        s.layout
            .run_directional_pair_focus(&ctx, &window_a, Direction::Up, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::Unchanged
        )))
    );

    let outcome = s
        .layout
        .run_directional_pair_focus(&ctx, &window_a, Direction::Left, false)
        .expect("cross focus plans");
    assert!(outcome.crossed);
    assert_eq!(outcome.target_output.0, "out-1");
    assert_eq!(outcome.target_workspace.0, "ws-a");
    assert_eq!(outcome.to_window.0, "win-x");
    assert_eq!(
        outcome
            .geometry
            .iter()
            .filter(|g| g.output.0 == "out-1")
            .count(),
        1
    );
    assert_eq!(
        outcome
            .geometry
            .iter()
            .filter(|g| g.output.0 == "out-2")
            .count(),
        2
    );
    assert_eq!(
        s.layout.domain(&key("out-1", "ws-a")).unwrap().tree,
        before_d1
    );
    assert_eq!(s.layout.domain(&k2).unwrap().tree, before_d2);
}

#[test]
fn cross_output_focus_empty_target_refuses() {
    let mut s = TwoOutputRig::new(
        domain_ws("out-1", "ws-a", vec![(Direction::Right, "out-2")], 0),
        domain_ws("out-2", "ws-b", vec![(Direction::Left, "out-1")], 0),
    );
    s.admit("win-a", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w = s.focused_window(&k2);
    let src_dom = s.domain_desc(&k2).clone();
    let dst_dom = s.other_desc(&k2).clone();
    let obs = s.all_engine_windows();
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &w,
        observed: &obs,
        closed: &[],
    };
    assert_eq!(
        s.layout
            .run_directional_pair_focus(&ctx, &w, Direction::Left, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::Unchanged
        )))
    );
}

#[test]
fn cross_output_changed_target_observation_refuses() {
    let mut s = two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-a", "out-2", "ws-b");
    s.admit("win-b", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let current = s.focused_window(&k2);
    if current.0 != "win-a" {
        s.focus_local(&k2, &current, Direction::Left);
    }
    let window_a = s.focused_window(&k2);
    assert_eq!(window_a.0, "win-a");

    // Local focus refuses PartialObservation when a known window is missing.
    let before_k2 = s.layout.domain(&k2).unwrap().clone();
    let mut local_missing = s.domain_observed(&k2);
    local_missing.retain(|w| w.window.0 != "win-b");
    assert_eq!(
        s.layout
            .local_focus(&k2, &window_a, Direction::Left, &window_a, &local_missing),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    let after_k2 = s.layout.domain(&k2).unwrap();
    assert_eq!(after_k2.accepted_revision, before_k2.accepted_revision);
    assert_eq!(after_k2.tree, before_k2.tree);
    assert_eq!(after_k2.focused_leaf, before_k2.focused_leaf);

    // Changed target visibility (window reported on an unknown workspace):
    // cross-domain mismatch on both pair focus and pair move.
    let k1 = key("out-1", "ws-a");
    let before_k1 = s.layout.domain(&k1).unwrap().clone();
    let mut changed = s.all_engine_windows();
    for entry in &mut changed {
        if entry.window.0 == "win-x" {
            entry.workspace = WorkspaceId("ws-ghost".to_owned());
        }
    }
    let src_dom = s.domain_desc(&k2).clone();
    let dst_dom = s.other_desc(&k2).clone();
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &window_a,
        observed: &changed,
        closed: &[],
    };
    assert_eq!(
        s.layout
            .run_directional_pair_focus(&ctx, &window_a, Direction::Left, false),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        )))
    );
    assert_eq!(
        s.layout.run_directional_pair_move(
            &ctx,
            &window_a,
            Direction::Left,
            true,
            SameAxisMove::GroupWithNeighbor
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        )))
    );
    let after_k1 = s.layout.domain(&k1).unwrap();
    let after_k2 = s.layout.domain(&k2).unwrap();
    assert_eq!(after_k1.accepted_revision, before_k1.accepted_revision);
    assert_eq!(after_k1.tree, before_k1.tree);
    assert_eq!(after_k1.focused_leaf, before_k1.focused_leaf);
    assert_eq!(after_k2.accepted_revision, before_k2.accepted_revision);
    assert_eq!(after_k2.tree, before_k2.tree);
    assert_eq!(after_k2.focused_leaf, before_k2.focused_leaf);
}

#[test]
fn r4_left_far_side_remembered_lands_at_root_edge() {
    let mut s = defect_two_output_rig();
    s.admit("win-1", "out-1", "ws-a");
    s.admit("win-2", "out-1", "ws-a");
    let k1 = key("out-1", "ws-a");
    let w2 = s.window_in(&k1, "win-2");
    s.focus_local(&k1, &w2, Direction::Left);
    assert_eq!(s.focused_window(&k1).0, "win-1");
    let target_root_before = s.root_id("out-1", "ws-a").expect("target root");

    s.admit("win-3", "out-2", "ws-b");
    s.admit("win-4", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w4 = s.focused_window(&k2);
    assert_eq!(w4.0, "win-4");
    s.focus_local(&k2, &w4, Direction::Left);
    assert_eq!(s.focused_window(&k2).0, "win-3");

    let outcome = s.move_pair(&k2, Direction::Left);
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
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    assert_eq!(focus_leaf.0, "leaf-win-3");
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    let mover_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-1", "ws-a").expect("new root"),
        "mover sits at the target root, not nested beside far-side W1"
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()],
        "mover lands last (nearest the eastern source)"
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_right_far_side_remembered_lands_at_root_edge() {
    let mut s = defect_two_output_rig();
    s.admit("win-1", "out-2", "ws-b");
    s.admit("win-2", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    assert_eq!(s.focused_window(&k2).0, "win-2");
    let target_root_before = s.root_id("out-2", "ws-b").expect("target root");

    s.admit("win-3", "out-1", "ws-a");
    s.admit("win-4", "out-1", "ws-a");
    let k1 = key("out-1", "ws-a");
    assert_eq!(s.focused_window(&k1).0, "win-4");

    let outcome = s.move_pair(&k1, Direction::Right);
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
    assert_eq!(cross.target_workspace.0, "ws-b");
    assert_eq!(focus_leaf.0, "leaf-win-4");
    assert_eq!(s.leaves("out-1", "ws-a"), vec!["leaf-win-3".to_string()]);
    assert_eq!(
        s.leaves("out-2", "ws-b"),
        vec![
            "leaf-win-4".to_string(),
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
        ]
    );
    let mover_parent = s
        .parent_of("out-2", "ws-b", "leaf-win-4")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-2", "ws-b").expect("new root"),
        "mover sits at the target root, not nested beside far-side W2"
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-4".to_string(), target_root_before],
        "mover lands first (nearest the western source)"
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_right_then_left_journey_returns_to_root_edge() {
    let mut s = defect_two_output_rig();
    s.admit("win-1", "out-1", "ws-a");
    s.admit("win-2", "out-1", "ws-a");
    s.admit("win-3", "out-1", "ws-a");
    s.admit("win-4", "out-2", "ws-b");
    let k1 = key("out-1", "ws-a");
    let k2 = key("out-2", "ws-b");
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-4".to_string()]);

    let w4 = s.focused_window(&k2);
    assert_eq!(w4.0, "win-4");
    let focus_outcome = s.focus_pair(&k2, &w4, Direction::Left);
    assert_eq!(focus_outcome.to_window.0, "win-3");
    assert_eq!(s.focused_window(&k1).0, "win-3");

    let PairMoveOutcome::Moved {
        rule: flat_rule, ..
    } = s.move_pair(&k1, Direction::Right)
    else {
        panic!("expected local R3 escape");
    };
    assert_eq!(flat_rule, Rule::R3);
    assert_eq!(s.focused_window(&k1).0, "win-3");
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );

    let PairMoveOutcome::Moved {
        rule: right_rule,
        focus_leaf: right_leaf,
        geometry: right_geom,
        cross: Some(right_cross),
        ..
    } = s.move_pair(&k1, Direction::Right)
    else {
        panic!("expected cross move right");
    };
    assert_eq!(right_rule, Rule::R4);
    assert_eq!(right_cross.target_output.0, "out-2");
    assert_eq!(right_cross.target_workspace.0, "ws-b");
    assert_eq!(right_leaf.0, "leaf-win-3");
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec!["leaf-win-1".to_string(), "leaf-win-2".to_string()]
    );
    assert_eq!(
        s.leaves("out-2", "ws-b"),
        vec!["leaf-win-3".to_string(), "leaf-win-4".to_string()]
    );
    let r_parent = s
        .parent_of("out-2", "ws-b", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(r_parent.1, Axis::Horizontal);
    assert_eq!(r_parent.0, s.root_id("out-2", "ws-b").expect("root"));
    assert_eq!(
        r_parent.2,
        vec!["leaf-win-3".to_string(), "leaf-win-4".to_string()]
    );
    s.assert_geometry_complete_for(&right_geom, &[("out-1", "ws-a"), ("out-2", "ws-b")]);

    assert_eq!(s.focused_window(&k2).0, "win-3");
    let l_root_mid = s.root_id("out-1", "ws-a").expect("mid root");
    let PairMoveOutcome::Moved {
        rule: left_rule,
        focus_leaf: left_leaf,
        geometry: left_geom,
        cross: Some(left_cross),
        ..
    } = s.move_pair(&k2, Direction::Left)
    else {
        panic!("expected cross move left");
    };
    assert_eq!(left_rule, Rule::R4);
    assert_eq!(left_cross.target_output.0, "out-1");
    assert_eq!(left_cross.target_workspace.0, "ws-a");
    assert_eq!(left_leaf.0, "leaf-win-3");
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-1".to_string(),
            "leaf-win-2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    let l_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(l_parent.1, Axis::Horizontal);
    assert_eq!(l_parent.0, s.root_id("out-1", "ws-a").expect("new root"));
    assert_eq!(l_parent.2, vec![l_root_mid, "leaf-win-3".to_string()]);
    s.assert_geometry_complete_for(&left_geom, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_left_near_side_remembered_wraps_whole_target() {
    let mut s = defect_two_output_rig();
    s.admit("win-1", "out-1", "ws-a");
    s.admit("win-2", "out-1", "ws-a");
    let k1 = key("out-1", "ws-a");
    assert_eq!(s.focused_window(&k1).0, "win-2");
    let target_root_before = s.root_id("out-1", "ws-a").expect("target root");

    s.admit("win-3", "out-2", "ws-b");
    s.admit("win-4", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w4 = s.focused_window(&k2);
    assert_eq!(w4.0, "win-4");
    s.focus_local(&k2, &w4, Direction::Left);
    assert_eq!(s.focused_window(&k2).0, "win-3");

    let PairMoveOutcome::Moved {
        rule,
        focus_leaf,
        geometry,
        cross: Some(cross),
        ..
    } = s.move_pair(&k2, Direction::Left)
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    assert_eq!(focus_leaf.0, "leaf-win-3");
    assert_eq!(s.leaves("out-2", "ws-b"), vec!["leaf-win-4".to_string()]);
    let mover_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_right_near_side_remembered_wraps_whole_target() {
    let mut s = defect_two_output_rig();
    s.admit("win-1", "out-2", "ws-b");
    s.admit("win-2", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w2 = s.window_in(&k2, "win-2");
    s.focus_local(&k2, &w2, Direction::Left);
    assert_eq!(s.focused_window(&k2).0, "win-1");
    let target_root_before = s.root_id("out-2", "ws-b").expect("target root");

    s.admit("win-3", "out-1", "ws-a");
    s.admit("win-4", "out-1", "ws-a");
    let k1 = key("out-1", "ws-a");
    assert_eq!(s.focused_window(&k1).0, "win-4");

    let PairMoveOutcome::Moved {
        rule,
        focus_leaf,
        geometry,
        cross: Some(cross),
        ..
    } = s.move_pair(&k1, Direction::Right)
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-2");
    assert_eq!(cross.target_workspace.0, "ws-b");
    assert_eq!(focus_leaf.0, "leaf-win-4");
    assert_eq!(s.leaves("out-1", "ws-a"), vec!["leaf-win-3".to_string()]);
    let mover_parent = s
        .parent_of("out-2", "ws-b", "leaf-win-4")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-2", "ws-b").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-4".to_string(), target_root_before]
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_left_nested_target_wraps_whole_and_preserves_nesting() {
    let mut s = defect_two_output_rig();
    s.admit("win-x", "out-1", "ws-a");
    s.admit("win-y", "out-1", "ws-a");
    s.admit_with_placement(
        "win-z",
        "out-1",
        "ws-a",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    let target_root_before = s.root_id("out-1", "ws-a").expect("target root");
    let target_tree_before = s.tree_of("out-1", "ws-a").expect("target tree");
    let z_parent_before = s
        .parent_of("out-1", "ws-a", "leaf-win-z")
        .expect("Z parent");
    assert_ne!(z_parent_before.0, target_root_before);

    s.admit("win-3", "out-2", "ws-b");
    s.admit("win-4", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w4 = s.focused_window(&k2);
    s.focus_local(&k2, &w4, Direction::Left);

    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = s.move_pair(&k2, Direction::Left)
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    let mover_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    let z_parent_after = s
        .parent_of("out-1", "ws-a", "leaf-win-z")
        .expect("Z parent");
    assert_eq!(z_parent_after.0, z_parent_before.0);
    assert_eq!(z_parent_after.1, Axis::Vertical);
    assert_eq!(
        z_parent_after.2,
        vec!["leaf-win-y".to_string(), "leaf-win-z".to_string()]
    );
    match s.tree_of("out-1", "ws-a").expect("new target tree") {
        Node::Group { children, .. } => {
            assert_eq!(children.len(), 2);
            assert_eq!(children[0], target_tree_before);
        }
        other => panic!("expected new root group, got {other:?}"),
    }
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_left_perpendicular_target_wraps_whole() {
    let mut s = defect_two_output_rig();
    s.admit("win-x1", "out-1", "ws-a");
    s.admit_with_placement(
        "win-x2",
        "out-1",
        "ws-a",
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        },
    );
    assert_eq!(s.root_axis("out-1", "ws-a"), Some(Axis::Vertical));
    let target_root_before = s.root_id("out-1", "ws-a").expect("target root");

    s.admit("win-3", "out-2", "ws-b");
    s.admit("win-4", "out-2", "ws-b");
    let k2 = key("out-2", "ws-b");
    let w4 = s.focused_window(&k2);
    s.focus_local(&k2, &w4, Direction::Left);

    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = s.move_pair(&k2, Direction::Left)
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-1");
    assert_eq!(cross.target_workspace.0, "ws-a");
    let mover_parent = s
        .parent_of("out-1", "ws-a", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Horizontal);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-1", "ws-a").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec![target_root_before, "leaf-win-3".to_string()]
    );
    assert_eq!(
        s.leaves("out-1", "ws-a"),
        vec![
            "leaf-win-x1".to_string(),
            "leaf-win-x2".to_string(),
            "leaf-win-3".to_string(),
        ]
    );
    s.assert_geometry_complete_for(&geometry, &[("out-1", "ws-a"), ("out-2", "ws-b")]);
}

#[test]
fn r4_up_remembered_leaf_variants_wrap_whole_target() {
    for remembered in ["near", "far"] {
        let mut s = stacked_two_output_rig();
        s.admit("win-1", "out-top", "ws-a");
        s.admit_with_placement(
            "win-2",
            "out-top",
            "ws-a",
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120,
            },
        );
        let k_top = key("out-top", "ws-a");
        assert_eq!(s.root_axis("out-top", "ws-a"), Some(Axis::Vertical));
        if remembered == "far" {
            let w2 = s.window_in(&k_top, "win-2");
            s.focus_local(&k_top, &w2, Direction::Up);
            assert_eq!(s.focused_window(&k_top).0, "win-1");
        } else {
            assert_eq!(s.focused_window(&k_top).0, "win-2");
        }
        let target_root_before = s.root_id("out-top", "ws-a").expect("target root");
        let target_tree_before = s.tree_of("out-top", "ws-a").expect("target tree");
        s.admit("win-3", "out-bottom", "ws-b");
        let k_bottom = key("out-bottom", "ws-b");
        let PairMoveOutcome::Moved {
            rule,
            focus_leaf,
            geometry,
            cross: Some(cross),
            ..
        } = s.move_pair(&k_bottom, Direction::Up)
        else {
            panic!("expected cross move");
        };
        assert_eq!(rule, Rule::R4);
        assert_eq!(cross.target_output.0, "out-top");
        assert_eq!(cross.target_workspace.0, "ws-a");
        assert_eq!(focus_leaf.0, "leaf-win-3");
        assert_eq!(s.leaves("out-bottom", "ws-b"), Vec::<String>::new());
        let mover_parent = s
            .parent_of("out-top", "ws-a", "leaf-win-3")
            .expect("mover parent");
        assert_eq!(mover_parent.1, Axis::Vertical);
        assert_eq!(
            mover_parent.0,
            s.root_id("out-top", "ws-a").expect("new root")
        );
        assert_eq!(
            mover_parent.2,
            vec![target_root_before, "leaf-win-3".to_string()],
            "mover lands last (nearest the southern source), remembered {remembered}"
        );
        match s.tree_of("out-top", "ws-a").expect("new target tree") {
            Node::Group { children, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0], target_tree_before);
            }
            other => panic!("expected new root group, got {other:?}"),
        }
        s.assert_geometry_complete_for(&geometry, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
    }
}

#[test]
fn r4_down_remembered_leaf_variants_wrap_whole_target() {
    for remembered in ["near", "far"] {
        let mut s = stacked_two_output_rig();
        s.admit("win-1", "out-bottom", "ws-b");
        s.admit_with_placement(
            "win-2",
            "out-bottom",
            "ws-b",
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120,
            },
        );
        let k_bottom = key("out-bottom", "ws-b");
        assert_eq!(s.root_axis("out-bottom", "ws-b"), Some(Axis::Vertical));
        if remembered == "near" {
            let w2 = s.window_in(&k_bottom, "win-2");
            s.focus_local(&k_bottom, &w2, Direction::Up);
            assert_eq!(s.focused_window(&k_bottom).0, "win-1");
        } else {
            assert_eq!(s.focused_window(&k_bottom).0, "win-2");
        }
        let target_root_before = s.root_id("out-bottom", "ws-b").expect("target root");
        s.admit("win-3", "out-top", "ws-a");
        let k_top = key("out-top", "ws-a");
        let PairMoveOutcome::Moved {
            rule,
            focus_leaf,
            geometry,
            cross: Some(cross),
            ..
        } = s.move_pair(&k_top, Direction::Down)
        else {
            panic!("expected cross move");
        };
        assert_eq!(rule, Rule::R4);
        assert_eq!(cross.target_output.0, "out-bottom");
        assert_eq!(cross.target_workspace.0, "ws-b");
        assert_eq!(focus_leaf.0, "leaf-win-3");
        assert_eq!(s.leaves("out-top", "ws-a"), Vec::<String>::new());
        let mover_parent = s
            .parent_of("out-bottom", "ws-b", "leaf-win-3")
            .expect("mover parent");
        assert_eq!(mover_parent.1, Axis::Vertical);
        assert_eq!(
            mover_parent.0,
            s.root_id("out-bottom", "ws-b").expect("new root")
        );
        assert_eq!(
            mover_parent.2,
            vec!["leaf-win-3".to_string(), target_root_before],
            "mover lands first (nearest the northern source), remembered {remembered}"
        );
        s.assert_geometry_complete_for(&geometry, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
    }
}

#[test]
fn r4_down_perpendicular_target_wraps_whole() {
    let mut s = stacked_two_output_rig();
    s.admit("win-x1", "out-bottom", "ws-b");
    s.admit("win-x2", "out-bottom", "ws-b");
    assert_eq!(s.root_axis("out-bottom", "ws-b"), Some(Axis::Horizontal));
    let target_root_before = s.root_id("out-bottom", "ws-b").expect("target root");
    s.admit("win-3", "out-top", "ws-a");
    let k_top = key("out-top", "ws-a");
    let PairMoveOutcome::Moved {
        rule,
        geometry,
        cross: Some(cross),
        ..
    } = s.move_pair(&k_top, Direction::Down)
    else {
        panic!("expected cross move");
    };
    assert_eq!(rule, Rule::R4);
    assert_eq!(cross.target_output.0, "out-bottom");
    assert_eq!(cross.target_workspace.0, "ws-b");
    let mover_parent = s
        .parent_of("out-bottom", "ws-b", "leaf-win-3")
        .expect("mover parent");
    assert_eq!(mover_parent.1, Axis::Vertical);
    assert_eq!(
        mover_parent.0,
        s.root_id("out-bottom", "ws-b").expect("new root")
    );
    assert_eq!(
        mover_parent.2,
        vec!["leaf-win-3".to_string(), target_root_before]
    );
    s.assert_geometry_complete_for(&geometry, &[("out-top", "ws-a"), ("out-bottom", "ws-b")]);
}
