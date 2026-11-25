//! Keyboard split-share resize integration over `WorldLayout` (portable, headless).
//!
//! Ported from `tests/session_resize.rs`. Covers horizontal/vertical both
//! directions, nested ancestor resolution, N-ary pair-only effects, edge
//! refusal, positivity/normalization/clamp, focus retention, refusal matrix,
//! deterministic replay, bounded property matrix, reversible paired directions,
//! flagged/partial/cross-domain/malformed observations, overflow shares,
//! vertical gap conservation, inwards vs outwards, direct-sum pair thresholds,
//! one-sided keyboard clamp, and distinct pair-minimum refusal.

use std::collections::BTreeMap;

use crate::contract::ResizeMode;
use crate::cosmic_v1;
use crate::directional::{
    Axis, Direction, Node, NodeId, OutputId, ResizePlanError, SameAxisMove, WindowId, WorkspaceId,
    expected_resize_shares, plan_resize_step,
};
use crate::geometry::Rect;
use crate::session::{
    DesiredGeometry, DomainKey, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind,
};
use crate::size_hints::WindowSizeHints;

use super::outcome::{AdmissionOutcome, LayoutFailure, ResizeOutcome};
use super::state::{DomainLayout, WorldLayout};

fn domain(output: &str, workspace: &str, w: i32, h: i32, gap: i32) -> OutputDomain {
    OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds: Rect { x: 0, y: 0, w, h },
        gap,
        adjacent: BTreeMap::new(),
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

struct KeyboardResizeRig {
    layout: WorldLayout,
    domains: BTreeMap<DomainKey, OutputDomain>,
}

impl KeyboardResizeRig {
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
        Self::new(vec![domain("out-1", "ws-1", 800, 600, 0)])
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

    fn admit_commit(&mut self, window: &str, horiz: bool) -> AdmissionOutcome {
        let k = key("out-1", "ws-1");
        let desc = self.domains[&k].clone();
        self.layout
            .domains
            .entry(k.clone())
            .or_insert_with(|| DomainLayout::new(desc.bounds, desc.gap, 0));
        let obs = self.domain_observed(&k, vec![tiled(window, "out-1", "ws-1")]);
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

    fn focus_commit(&mut self, k: &DomainKey, window: &WindowId, direction: Direction) {
        let obs = self.domain_observed(k, vec![]);
        self.layout
            .local_focus(k, window, direction, window, &obs)
            .unwrap_or_else(|e| panic!("focus {direction:?}: {e:?}"));
    }

    fn focus_commit_step(&mut self, k: &DomainKey, direction: Direction) {
        let w = self.focused_window(k);
        self.focus_commit(k, &w, direction);
    }

    fn move_commit_focused(&mut self, k: &DomainKey, direction: Direction) {
        let w = self.focused_window(k);
        let obs = self.domain_observed(k, vec![]);
        self.layout
            .local_move(k, &w, direction, SameAxisMove::GroupWithNeighbor, &w, &obs)
            .unwrap_or_else(|e| panic!("move {direction:?}: {e:?}"));
    }

    fn resize_commit(
        &mut self,
        k: &DomainKey,
        window: &WindowId,
        direction: Direction,
    ) -> ResizeOutcome {
        let obs = self.domain_observed(k, vec![]);
        let base = self.layout.accepted_revision(k);
        let outcome = self
            .layout
            .keyboard_resize(
                k,
                window,
                direction,
                ResizeMode::Outwards,
                0,
                window,
                &obs,
                crate::policy::ScaledResize::unscaled(),
            )
            .unwrap_or_else(|e| panic!("resize {direction:?}: {e:?}"));
        assert_eq!(outcome.base_revision, base);
        assert_eq!(outcome.revision, base + 1);
        assert_eq!(self.layout.accepted_revision(k), base + 1);
        outcome
    }

    fn root_shares(&self, k: &DomainKey) -> Vec<u64> {
        match self.layout.tree(k).cloned().expect("tree") {
            Node::Group { shares, .. } => shares,
            Node::Leaf { .. } => vec![],
        }
    }

    fn assert_resize_geometry(&self, k: &DomainKey, outcome: &ResizeOutcome) {
        assert!(!outcome.geometry.is_empty());
        for g in &outcome.geometry {
            assert!(g.rect.w > 0 && g.rect.h > 0, "positive geometry");
            assert!(!g.window.0.is_empty() && !g.leaf.0.is_empty());
            assert_eq!((&g.output, &g.workspace), (&k.output, &k.workspace));
        }
        let tree = self.layout.tree(k).expect("tree");
        let mut expected = vec![];
        collect_leaves(tree, &mut expected);
        let mut got: Vec<String> = outcome.geometry.iter().map(|g| g.leaf.0.clone()).collect();
        expected.sort();
        got.sort();
        assert_eq!(expected, got, "geometry covers affected domain");
        assert_no_overlap(&outcome.geometry);
    }
}

fn collect_leaves(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Leaf { id } => out.push(id.0.clone()),
        Node::Group { children, .. } => {
            for c in children {
                collect_leaves(c, out);
            }
        }
    }
}

fn assert_no_overlap(geometry: &[DesiredGeometry]) {
    for i in 0..geometry.len() {
        for j in (i + 1)..geometry.len() {
            let (a, b) = (geometry[i].rect, geometry[j].rect);
            assert!(
                !(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h),
                "overlap"
            );
        }
    }
}

#[test]
fn horizontal_both_directions() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    assert_eq!(w2.0, "win-2");
    let outcome = r.resize_commit(&k, &w2, Direction::Left);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![387, 411]);
    let leaf_2 = r
        .layout
        .leaves(&k)
        .and_then(|leaves| leaves.get(&win("win-2")))
        .expect("win-2 leaf")
        .clone();
    assert_eq!(outcome.focus_leaf, leaf_2);
    r.assert_resize_geometry(&k, &outcome);

    r.focus_commit(&k, &win("win-2"), Direction::Left);
    let w1 = r.focused_window(&k);
    assert_eq!(w1.0, "win-1");
    let outcome2 = r.resize_commit(&k, &w1, Direction::Right);
    assert_eq!(outcome2.operation.old_shares, vec![387, 411]);
    assert_eq!(outcome2.operation.new_shares, vec![399, 399]);
    r.assert_resize_geometry(&k, &outcome2);
}

#[test]
fn vertical_both_directions() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", false);
    r.admit_commit("win-2", false);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let outcome = r.resize_commit(&k, &w2, Direction::Up);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![287, 311]);
    r.assert_resize_geometry(&k, &outcome);

    r.focus_commit(&k, &win("win-2"), Direction::Up);
    let w1 = r.focused_window(&k);
    let outcome2 = r.resize_commit(&k, &w1, Direction::Down);
    assert_eq!(outcome2.operation.new_shares, vec![299, 299]);
    r.assert_resize_geometry(&k, &outcome2);
}

#[test]
fn nested_ancestor_resolution_outward() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    r.admit_commit("win-3", false);
    let k = key("out-1", "ws-1");
    r.focus_commit(&k, &win("win-3"), Direction::Up);
    let w = r.focused_window(&k);
    assert_eq!(w.0, "win-2");
    let outcome = r.resize_commit(&k, &w, Direction::Left);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![387, 411]);
    let leaf_2 = r
        .layout
        .leaves(&k)
        .and_then(|leaves| leaves.get(&win("win-2")))
        .expect("win-2 leaf")
        .clone();
    assert_eq!(outcome.focus_leaf, leaf_2);
    let root = r.layout.tree(&k).cloned().expect("tree");
    match &root {
        Node::Group {
            children,
            shares,
            axis,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![387, 411]);
            assert_eq!(children.len(), 2);
            match &children[1] {
                Node::Group { shares, .. } => assert_eq!(*shares, vec![1, 1], "inner untouched"),
                other => panic!("inner {other:?}"),
            }
        }
        other => panic!("root {other:?}"),
    }
    r.assert_resize_geometry(&k, &outcome);
}

#[test]
fn nary_pair_only_with_unaffected_subtree_and_order() {
    let mut r = KeyboardResizeRig::new(vec![domain("out-1", "ws-1", 1600, 600, 0)]);
    for i in 1..=4 {
        r.admit_commit(&format!("win-{i}"), true);
    }
    let k = key("out-1", "ws-1");
    r.move_commit_focused(&k, Direction::Right);
    r.focus_commit_step(&k, Direction::Left);
    r.focus_commit_step(&k, Direction::Left);
    r.move_commit_focused(&k, Direction::Left);
    r.focus_commit_step(&k, Direction::Right);
    r.move_commit_focused(&k, Direction::Left);
    r.focus_commit_step(&k, Direction::Right);

    let w4 = r.focused_window(&k);
    assert_eq!(w4.0, "win-4");
    let before_leaves = {
        let mut out = vec![];
        collect_leaves(r.layout.tree(&k).expect("tree"), &mut out);
        out
    };
    let outcome = r.resize_commit(&k, &w4, Direction::Left);
    assert_eq!(outcome.operation.old_shares, vec![1, 1, 1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![399, 399, 387, 411]);
    let mut after = vec![];
    collect_leaves(r.layout.tree(&k).expect("tree"), &mut after);
    assert_eq!(before_leaves, after);
    assert_eq!(r.root_shares(&k), vec![399, 399, 387, 411]);
    r.assert_resize_geometry(&k, &outcome);
}

#[test]
fn edge_refusals_are_unchanged() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    let k = key("out-1", "ws-1");
    let w1 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    for dir in [
        Direction::Left,
        Direction::Right,
        Direction::Up,
        Direction::Down,
    ] {
        assert_eq!(
            r.layout.keyboard_resize(
                &k,
                &w1,
                dir,
                ResizeMode::Outwards,
                0,
                &w1,
                &obs,
                crate::policy::ScaledResize::unscaled()
            ),
            Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
        );
        assert_eq!(r.layout.accepted_revision(&k), rev_before);
    }

    r.admit_commit("win-2", true);
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    let before_tree = r.layout.tree(&k).cloned();
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Right,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);
    assert_eq!(r.layout.tree(&k).cloned(), before_tree);
}

#[test]
fn normalization_and_clamp_to_exhaustion() {
    assert_eq!(cosmic_v1::keyboard_step_px(0), 12);
    assert_eq!(cosmic_v1::keyboard_step_px(1), 14);
    assert_eq!(cosmic_v1::keyboard_step_px(4), 20);
    assert!(!cosmic_v1::pair_admits_resize(719, 600, Axis::Horizontal));
    assert!(cosmic_v1::pair_admits_resize(720, 600, Axis::Horizontal));
    assert!(!cosmic_v1::pair_admits_resize(800, 479, Axis::Vertical));
    assert!(cosmic_v1::pair_admits_resize(800, 480, Axis::Vertical));
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 10, Axis::Horizontal),
        Some((360, 440))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 790, Axis::Horizontal),
        Some((440, 360))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(719, 360, Axis::Horizontal),
        None
    );

    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let mut step_n = 0;
    loop {
        let w = r.focused_window(&k);
        assert_eq!(w.0, "win-2");
        let obs = r.domain_observed(&k, vec![]);
        step_n += 1;
        match r.layout.keyboard_resize(
            &k,
            &w,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ) {
            Ok(outcome) => {
                for share in outcome.operation.new_shares.iter() {
                    assert!(*share > 0);
                }
            }
            Err(LayoutFailure::Semantics(RefusalKind::Unchanged)) => break,
            Err(other) => panic!("unexpected {other:?}"),
        }
        if step_n > 30 {
            panic!("donor never exhausted");
        }
    }
    assert!(step_n > 1, "at least one step must plan");
    let shares = r.root_shares(&k);
    assert!(shares[0] < shares[1], "left child shrank: {shares:?}");
}

#[test]
fn focus_retained_and_outcome_carries_semantics() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let base = r.layout.accepted_revision(&k);
    let outcome = r
        .layout
        .keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("outcome");
    assert_eq!(outcome.focus_leaf, before_focus);
    assert_eq!(outcome.base_revision, base);
    assert_eq!(outcome.revision, base + 1);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![387, 411]);
    assert_eq!(outcome.operation.direction, Direction::Left);
    assert_eq!(outcome.operation.focused_window, w2);
    assert_eq!(outcome.operation.mode, ResizeMode::Outwards);
    r.assert_resize_geometry(&k, &outcome);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
}

#[test]
fn refusal_matrix_unknown_domain_window_and_focus_mismatch() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let before_rev = r.layout.accepted_revision(&k);
    let before_tree = r.layout.tree(&k).cloned();
    let before_focus = r.layout.focus_leaf(&k).cloned();

    let bad_domain = key("out-9", "ws-1");
    assert_eq!(
        r.layout.keyboard_resize(
            &bad_domain,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain))
    );

    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &win("win-9"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );

    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &win("win-1"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );

    assert_eq!(r.layout.accepted_revision(&k), before_rev);
    assert_eq!(r.layout.tree(&k).cloned(), before_tree);
    assert_eq!(r.layout.focus_leaf(&k).cloned(), before_focus);
}

#[test]
fn deterministic_replay() {
    fn build() -> (KeyboardResizeRig, DomainKey, WindowId) {
        let mut r = KeyboardResizeRig::single();
        r.admit_commit("win-1", true);
        r.admit_commit("win-2", true);
        let k = key("out-1", "ws-1");
        let w = r.focused_window(&k);
        (r, k, w)
    }
    let (mut a, ka, wa) = build();
    let (mut b, kb, wb) = build();
    let obsa = a.domain_observed(&ka, vec![]);
    let obsb = b.domain_observed(&kb, vec![]);
    let pa = a
        .layout
        .keyboard_resize(
            &ka,
            &wa,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &wa,
            &obsa,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("a");
    let pb = b
        .layout
        .keyboard_resize(
            &kb,
            &wb,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &wb,
            &obsb,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("b");
    assert_eq!(pa, pb);
    assert_eq!(pa.geometry, pb.geometry);
    assert_eq!(a.layout.tree(&ka), b.layout.tree(&kb));
    assert_eq!(
        a.layout.accepted_revision(&ka),
        b.layout.accepted_revision(&kb)
    );
    assert_eq!(a.layout.focus_leaf(&ka), b.layout.focus_leaf(&kb));
}

#[test]
fn bounded_property_matrix() {
    let bounds_cases: Vec<(i32, i32, i32)> = vec![
        (800, 600, 0),
        (1440, 900, 2),
        (1600, 600, 4),
        (800, 900, 8),
        (200, 200, 0),
    ];
    let dir_cases: Vec<(bool, Vec<Direction>)> = vec![
        (true, vec![Direction::Left, Direction::Right]),
        (false, vec![Direction::Up, Direction::Down]),
    ];
    for (w, h, gap) in bounds_cases {
        for (horiz, dirs) in &dir_cases {
            let mut r = KeyboardResizeRig::new(vec![domain("out-1", "ws-1", w, h, gap)]);
            r.admit_commit("win-1", *horiz);
            r.admit_commit("win-2", *horiz);
            r.admit_commit("win-3", *horiz);
            let k = key("out-1", "ws-1");
            for &dir in dirs {
                let cur = r.focused_window(&k);
                if cur.0 == "win-3" {
                    let back = match dir {
                        Direction::Left | Direction::Right => Direction::Left,
                        Direction::Up | Direction::Down => Direction::Up,
                    };
                    let obs = r.domain_observed(&k, vec![]);
                    let _ = r.layout.local_focus(&k, &cur, back, &cur, &obs);
                }
                let fw = r.focused_window(&k);
                let obs = r.domain_observed(&k, vec![]);
                let outcome = match r.layout.keyboard_resize(
                    &k,
                    &fw,
                    dir,
                    ResizeMode::Outwards,
                    0,
                    &fw,
                    &obs,
                    crate::policy::ScaledResize::unscaled(),
                ) {
                    Ok(o) => o,
                    Err(LayoutFailure::Semantics(RefusalKind::Unchanged)) => continue,
                    Err(LayoutFailure::Semantics(RefusalKind::PairBelowMinimum)) => continue,
                    Err(e) => panic!("matrix {w}x{h} gap {gap} {dir:?}: {e:?}"),
                };
                assert!(!outcome.operation.new_shares.contains(&0));
                let total: u64 = outcome.operation.new_shares.iter().sum();
                assert!(total > 0);
                for g in &outcome.geometry {
                    assert!(g.rect.w > 0 && g.rect.h > 0, "no zero dims {w}x{h} {dir:?}");
                }
                let dom = &r.domains[&k];
                let is_horiz = matches!(
                    outcome.operation.direction,
                    Direction::Left | Direction::Right
                );
                if is_horiz {
                    let min_x = outcome.geometry.iter().map(|g| g.rect.x).min().unwrap();
                    let max_e = outcome
                        .geometry
                        .iter()
                        .map(|g| g.rect.x + g.rect.w)
                        .max()
                        .unwrap();
                    assert_eq!(min_x, dom.bounds.x);
                    assert_eq!(max_e, dom.bounds.x + dom.bounds.w);
                    let mut xs: Vec<_> = outcome
                        .geometry
                        .iter()
                        .map(|g| (g.rect.x, g.rect.w))
                        .collect();
                    xs.sort();
                    for pair in xs.windows(2) {
                        assert_eq!(pair[1].0, pair[0].0 + pair[0].1 + dom.gap, "gap conserved");
                    }
                } else {
                    let min_y = outcome.geometry.iter().map(|g| g.rect.y).min().unwrap();
                    let max_e = outcome
                        .geometry
                        .iter()
                        .map(|g| g.rect.y + g.rect.h)
                        .max()
                        .unwrap();
                    assert_eq!(min_y, dom.bounds.y);
                    assert_eq!(max_e, dom.bounds.y + dom.bounds.h);
                }
                let old = outcome.operation.old_shares.clone();
                let new = outcome.operation.new_shares.clone();
                let fi = outcome.operation.focused_index;
                let ni = outcome.operation.neighbor_index;
                let old_pair = old[fi] + old[ni];
                let new_pair = new[fi] + new[ni];
                assert!(new_pair > 0 && old_pair > 0);
                let n = old.len();
                let has_non_pair = (0..n).any(|i| i != fi && i != ni);
                if has_non_pair {
                    let witness = (0..n).find(|i| *i != fi && *i != ni).expect("witness");
                    assert!(new[witness] % old[witness] == 0);
                    let scale = new[witness] / old[witness];
                    assert!(scale >= 1);
                    assert_eq!(new_pair, old_pair * scale);
                } else {
                    assert!(new_pair % old_pair == 0);
                }
            }
        }
    }
}

#[test]
fn reversible_paired_directions_without_clamp() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    r.resize_commit(&k, &w2, Direction::Left);
    r.focus_commit(&k, &win("win-2"), Direction::Left);
    let w1 = r.focused_window(&k);
    assert_eq!(w1.0, "win-1");
    r.resize_commit(&k, &w1, Direction::Right);
    assert_eq!(r.root_shares(&k), vec![399, 399]);

    r.focus_commit(&k, &win("win-1"), Direction::Right);
    let w2b = r.focused_window(&k);
    assert_eq!(w2b.0, "win-2");
    let fwd = r.resize_commit(&k, &w2b, Direction::Left);
    assert_eq!(fwd.operation.old_shares, vec![399, 399]);
    assert_eq!(fwd.operation.new_shares, vec![387, 411]);
    r.focus_commit(&k, &win("win-2"), Direction::Left);
    let w1b = r.focused_window(&k);
    let back = r.resize_commit(&k, &w1b, Direction::Right);
    assert_eq!(back.operation.old_shares, vec![387, 411]);
    assert_eq!(back.operation.new_shares, vec![399, 399]);
    assert_eq!(r.root_shares(&k), vec![399, 399]);
}

#[test]
fn flagged_observation_refuses_fail_closed() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let mut obs = r.domain_observed(&k, vec![]);
    for entry in obs.iter_mut() {
        if entry.window == w2 {
            entry.floating = true;
        }
    }
    let rev_before = r.layout.accepted_revision(&k);
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);

    let mut t = KeyboardResizeRig::single();
    t.admit_commit("win-1", true);
    let tk = key("out-1", "ws-1");
    t.layout.seed_exception_for_test(
        &tk,
        &win("win-9"),
        ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
    );
    let tkobs = t.domain_observed(&tk, vec![]);
    let t_rev_before = t.layout.accepted_revision(&tk);
    assert_eq!(
        t.layout.keyboard_resize(
            &tk,
            &win("win-9"),
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &win("win-1"),
            &tkobs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::NotTiled))
    );
    assert_eq!(t.layout.accepted_revision(&tk), t_rev_before);
}

#[test]
fn partial_cross_domain_malformed_observations_refuse() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let rev_before = r.layout.accepted_revision(&k);

    let mut partial = r.domain_observed(&k, vec![]);
    partial.pop();
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &partial,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );

    let mut cross = r.domain_observed(&k, vec![]);
    cross[0].output = OutputId("out-9".to_owned());
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &cross,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::CrossDomainMismatch))
    );

    let mut malformed = r.domain_observed(&k, vec![]);
    malformed[0].window = WindowId(String::new());
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &malformed,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );

    let mut dup = r.domain_observed(&k, vec![]);
    dup.push(dup[0].clone());
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &dup,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );

    assert_eq!(r.layout.accepted_revision(&k), rev_before);
}

#[test]
fn overflow_shares_are_unrepresentable_malformed() {
    let tree = Node::Group {
        id: NodeId("root".to_owned()),
        axis: Axis::Horizontal,
        children: vec![
            Node::Leaf {
                id: NodeId("A".to_owned()),
            },
            Node::Leaf {
                id: NodeId("B".to_owned()),
            },
        ],
        shares: vec![u64::MAX - 100, 1],
    };
    assert_eq!(
        plan_resize_step(&tree, &NodeId("A".to_owned()), Direction::Right),
        Err(ResizePlanError::Malformed)
    );
    assert_eq!(expected_resize_shares(&[u64::MAX - 100, 1], 0, 1), None);
}

#[test]
fn vertical_gap_conservation() {
    let mut r = KeyboardResizeRig::new(vec![domain("out-1", "ws-1", 800, 600, 4)]);
    r.admit_commit("win-1", false);
    r.admit_commit("win-2", false);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let outcome = r.resize_commit(&k, &w2, Direction::Up);
    assert_eq!(outcome.operation.direction, Direction::Up);
    let dom = &r.domains[&k];
    let min_y = outcome.geometry.iter().map(|g| g.rect.y).min().unwrap();
    let max_e = outcome
        .geometry
        .iter()
        .map(|g| g.rect.y + g.rect.h)
        .max()
        .unwrap();
    assert_eq!(min_y, dom.bounds.y);
    assert_eq!(max_e, dom.bounds.y + dom.bounds.h);
    let mut ys: Vec<(i32, i32)> = outcome
        .geometry
        .iter()
        .map(|g| (g.rect.y, g.rect.h))
        .collect();
    ys.sort();
    assert_eq!(ys.len(), 2);
    assert_eq!(
        ys[1].0,
        ys[0].0 + ys[0].1 + dom.gap,
        "vertical gap conserved"
    );
    for g in &outcome.geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
    }
}

#[test]
fn divisible_step_inverts_exactly_via_primitive() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", false);
    r.admit_commit("win-2", false);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let fwd = r.resize_commit(&k, &w2, Direction::Up);
    assert_eq!(fwd.operation.old_shares, vec![1, 1]);
    assert_eq!(fwd.operation.new_shares, vec![287, 311]);
    r.focus_commit(&k, &w2, Direction::Up);
    let w1 = r.focused_window(&k);
    let back = r.resize_commit(&k, &w1, Direction::Down);
    assert_eq!(back.operation.old_shares, vec![287, 311]);
    assert_eq!(back.operation.new_shares, vec![299, 299]);
    assert_eq!(r.root_shares(&k), vec![299, 299]);

    r.focus_commit(&k, &w1, Direction::Down);
    let w2b = r.focused_window(&k);
    let fwd2 = r.resize_commit(&k, &w2b, Direction::Up);
    assert_eq!(fwd2.operation.new_shares, vec![287, 311]);
}

#[test]
fn keyboard_inwards_shrinks_focused_outwards_grows() {
    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    assert_eq!(w2.0, "win-2");
    let obs = r.domain_observed(&k, vec![]);
    let out_outcome = r
        .layout
        .keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("outwards plans");
    assert_eq!(out_outcome.operation.new_shares, vec![387, 411]);
    assert_eq!(out_outcome.operation.mode, ResizeMode::Outwards);

    let in_outcome = {
        let mut tmp = KeyboardResizeRig::single();
        tmp.admit_commit("win-1", true);
        tmp.admit_commit("win-2", true);
        let tk = key("out-1", "ws-1");
        let tw = tmp.focused_window(&tk);
        let tobs = tmp.domain_observed(&tk, vec![]);
        tmp.layout
            .keyboard_resize(
                &tk,
                &tw,
                Direction::Left,
                ResizeMode::Inwards,
                0,
                &tw,
                &tobs,
                crate::policy::ScaledResize::unscaled(),
            )
            .expect("inwards plans")
    };
    assert_eq!(in_outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(in_outcome.operation.mode, ResizeMode::Inwards);
    let out_left = out_outcome.operation.new_shares[0];
    let in_left = in_outcome.operation.new_shares[0];
    assert!(
        out_left < 399 && in_left > 399,
        "out {out_left} in {in_left}"
    );
}

#[test]
fn pair_threshold_uses_direct_sum_not_union_with_gap() {
    fn rig_with(w: i32, h: i32, gap: i32) -> KeyboardResizeRig {
        KeyboardResizeRig::new(vec![domain("out-1", "ws-1", w, h, gap)])
    }

    let mut s = rig_with(727, 600, 8);
    s.admit_commit("win-1", true);
    s.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w = s.focused_window(&k);
    let obs = s.domain_observed(&k, vec![]);
    assert_eq!(
        s.layout.keyboard_resize(
            &k,
            &w,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PairBelowMinimum))
    );

    let mut s2 = rig_with(800, 600, 8);
    s2.admit_commit("win-1", true);
    s2.admit_commit("win-2", true);
    let k2 = key("out-1", "ws-1");
    let w2 = s2.focused_window(&k2);
    let obs2 = s2.domain_observed(&k2, vec![]);
    assert!(
        s2.layout
            .keyboard_resize(
                &k2,
                &w2,
                Direction::Left,
                ResizeMode::Outwards,
                0,
                &w2,
                &obs2,
                crate::policy::ScaledResize::unscaled(),
            )
            .is_ok()
    );

    let mut v = rig_with(800, 487, 8);
    v.admit_commit("win-1", false);
    v.admit_commit("win-2", false);
    let vk = key("out-1", "ws-1");
    let vw = v.focused_window(&vk);
    let vobs = v.domain_observed(&vk, vec![]);
    assert_eq!(
        v.layout.keyboard_resize(
            &vk,
            &vw,
            Direction::Up,
            ResizeMode::Outwards,
            0,
            &vw,
            &vobs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PairBelowMinimum))
    );

    let mut v2 = rig_with(800, 600, 8);
    v2.admit_commit("win-1", false);
    v2.admit_commit("win-2", false);
    let vk2 = key("out-1", "ws-1");
    let vw2 = v2.focused_window(&vk2);
    let vobs2 = v2.domain_observed(&vk2, vec![]);
    assert!(
        v2.layout
            .keyboard_resize(
                &vk2,
                &vw2,
                Direction::Up,
                ResizeMode::Outwards,
                0,
                &vw2,
                &vobs2,
                crate::policy::ScaledResize::unscaled(),
            )
            .is_ok()
    );
}

#[test]
fn keyboard_one_sided_clamp_and_inwards_shrink() {
    assert_eq!(
        cosmic_v1::clamp_keyboard_shrink_pair(500, 220, 12, Axis::Horizontal),
        Some((488, 232))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(720, 488, Axis::Horizontal),
        Some((360, 360))
    );

    let mut r = KeyboardResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let in_outcome = r
        .layout
        .keyboard_resize(
            &k,
            &w2,
            Direction::Left,
            ResizeMode::Inwards,
            0,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("keyboard inwards plans");
    assert_eq!(in_outcome.operation.mode, ResizeMode::Inwards);

    let focused_leaf = in_outcome.operation.focused_child.clone();
    let mut t = KeyboardResizeRig::single();
    t.admit_commit("win-1", true);
    t.admit_commit("win-2", true);
    let tk = key("out-1", "ws-1");
    let tw = t.focused_window(&tk);
    let tobs = t.domain_observed(&tk, vec![]);
    let out_outcome = t
        .layout
        .keyboard_resize(
            &tk,
            &tw,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &tw,
            &tobs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("outwards plans");
    let inw_focused_w = in_outcome
        .geometry
        .iter()
        .find(|g| g.leaf == focused_leaf)
        .expect("inwards geometry")
        .rect
        .w;
    let out_focused_w = out_outcome
        .geometry
        .iter()
        .find(|g| g.leaf == out_outcome.operation.focused_child)
        .expect("outwards geometry")
        .rect
        .w;
    assert!(
        inw_focused_w < out_focused_w,
        "inwards {inw_focused_w} must shrink vs outwards {out_focused_w}"
    );
}

#[test]
fn pair_minimum_refuses_distinct_kind() {
    let mut r = KeyboardResizeRig::new(vec![domain("out-1", "ws-1", 727, 600, 8)]);
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &w,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &w,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::PairBelowMinimum))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);

    let mut e = KeyboardResizeRig::single();
    e.admit_commit("win-1", true);
    let ek = key("out-1", "ws-1");
    let ew = e.focused_window(&ek);
    let eobs = e.domain_observed(&ek, vec![]);
    let erev_before = e.layout.accepted_revision(&ek);
    assert_eq!(
        e.layout.keyboard_resize(
            &ek,
            &ew,
            Direction::Left,
            ResizeMode::Outwards,
            0,
            &ew,
            &eobs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(e.layout.accepted_revision(&ek), erev_before);
}

#[test]
fn sixty_five_share_keyboard_resize_succeeds_without_count_cap() {
    let mut r = KeyboardResizeRig::new(vec![domain("out-1", "ws-1", 65 * 400, 600, 0)]);
    let k = key("out-1", "ws-1");
    let mut children = Vec::with_capacity(65);
    let mut leaves = BTreeMap::new();
    let mut focus_stack = Vec::with_capacity(65);
    let mut observed = Vec::with_capacity(65);
    for i in 0..65 {
        let w = win(&format!("win-{i}"));
        let l = NodeId(format!("leaf-{i}"));
        children.push(Node::Leaf { id: l.clone() });
        leaves.insert(w.clone(), l.clone());
        focus_stack.push(l);
        observed.push(tiled(&format!("win-{i}"), "out-1", "ws-1"));
    }
    let focused_window = win("win-0");
    let focused_leaf = leaves.get(&focused_window).cloned().expect("win-0 leaf");
    r.layout.domains.insert(
        k.clone(),
        DomainLayout {
            tree: Some(Node::Group {
                id: NodeId("root".to_owned()),
                axis: Axis::Horizontal,
                children,
                shares: vec![1; 65],
            }),
            leaves,
            bounds: Rect {
                x: 0,
                y: 0,
                w: 65 * 400,
                h: 600,
            },
            gap: 0,
            outer_gap: 0,
            exceptions: BTreeMap::new(),
            focused_leaf: Some(focused_leaf),
            focus_stack,
            focus_history: Vec::new(),
            last_active: None,
            retained_float_geometry: BTreeMap::new(),
            automatic_fixed: std::collections::BTreeSet::new(),
            fixed_tile_override: std::collections::BTreeSet::new(),
            capacity_floats: std::collections::BTreeSet::new(),
            admission_order: BTreeMap::new(),
            minimized_slots: BTreeMap::new(),
            accepted_revision: 1,
        },
    );

    let outcome = r
        .layout
        .keyboard_resize(
            &k,
            &focused_window,
            Direction::Right,
            ResizeMode::Outwards,
            0,
            &focused_window,
            &observed,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("65-share keyboard resize succeeds without count cap");
    assert_eq!(outcome.base_revision, 1);
    assert_eq!(outcome.revision, 2);
    assert_eq!(outcome.operation.old_shares.len(), 65);
    assert_eq!(outcome.operation.new_shares.len(), 65);
    assert_eq!(outcome.geometry.len(), 65);
    assert!(outcome.operation.new_shares[0] > outcome.operation.new_shares[1]);
}
