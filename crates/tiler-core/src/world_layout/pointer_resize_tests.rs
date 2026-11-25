//! Pointer split-share edge and corner resize integration over `WorldLayout`
//! (portable, headless).
//!
//! Ported from `tests/session_pointer_resize.rs`. Covers horizontal and vertical
//! edge derivation, nested ancestor resolution, N-ary adjacent-only share
//! redistribution, minimum clamp and out-of-bounds refusal, exhausted minimum
//! and single-leaf refusal, projectability, determinism, unknown-domain/window
//! and inactive-edge refusals, two-sided pointer clamp, atomic dual-axis
//! corners, inactive-target corner drags, and overlay-active decoupling.

use std::collections::BTreeMap;

use crate::contract::ResizeMode;
use crate::cosmic_v1;
use crate::directional::{Axis, Direction, Node, OutputId, SameAxisMove, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::{DomainKey, ObservedWindow, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;

use super::outcome::{AdmissionOutcome, LayoutFailure, PointerCornerResizeOutcome, ResizeOutcome};
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

struct PointerResizeRig {
    layout: WorldLayout,
    domains: BTreeMap<DomainKey, OutputDomain>,
}

impl PointerResizeRig {
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

    fn pointer_commit(
        &mut self,
        k: &DomainKey,
        subject: &WindowId,
        direction: Direction,
        boundary: i32,
    ) -> ResizeOutcome {
        let obs = self.domain_observed(k, vec![]);
        let active = self.focused_window(k);
        let base = self.layout.accepted_revision(k);
        let outcome = self
            .layout
            .pointer_resize(
                k,
                subject,
                direction,
                boundary,
                &active,
                &obs,
                crate::policy::ScaledResize::unscaled(),
            )
            .unwrap_or_else(|e| panic!("pointer {direction:?}@{boundary}: {e:?}"));
        assert_eq!(outcome.base_revision, base);
        assert_eq!(outcome.revision, base + 1);
        assert_eq!(self.layout.accepted_revision(k), base + 1);
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    fn corner_commit(
        &mut self,
        k: &DomainKey,
        subject: &WindowId,
        direction_h: Direction,
        boundary_h: i32,
        direction_v: Direction,
        boundary_v: i32,
    ) -> PointerCornerResizeOutcome {
        let obs = self.domain_observed(k, vec![]);
        let active = self.focused_window(k);
        let base = self.layout.accepted_revision(k);
        let outcome = self
            .layout
            .pointer_resize_corner(
                k,
                subject,
                direction_h,
                boundary_h,
                direction_v,
                boundary_v,
                &active,
                &obs,
                crate::policy::ScaledResize::unscaled(),
            )
            .unwrap_or_else(|e| {
                panic!("corner {direction_h:?}@{boundary_h} {direction_v:?}@{boundary_v}: {e:?}")
            });
        assert_eq!(outcome.base_revision, base);
        assert_eq!(outcome.revision, base + 1);
        assert_eq!(self.layout.accepted_revision(k), base + 1);
        outcome
    }
}

fn nested_focus_win2() -> (PointerResizeRig, DomainKey, WindowId) {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    r.admit_commit("win-3", false);
    let k = key("out-1", "ws-1");
    r.focus_commit_step(&k, Direction::Up);
    let w = r.focused_window(&k);
    assert_eq!(w.0, "win-2");
    (r, k, w)
}

#[test]
fn horizontal_derivation_grows_focused_and_preserves_focus() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    assert_eq!(w2.0, "win-2");
    let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    // Layout [1,1] over 800px: boundary at 400. Propose 390 with exact
    // pixel-projectable shares [389,409], projecting the pair boundary
    // exactly to 390.
    let outcome = r.pointer_commit(&k, &w2, Direction::Left, 390);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![389, 409]);
    assert_eq!(outcome.operation.focused_index, 1);
    assert_eq!(outcome.operation.neighbor_index, 0);
    assert_eq!(outcome.focus_leaf, before_focus);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
    assert!(!outcome.geometry.is_empty());
    for g in &outcome.geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
    }
    let left_id = if outcome.operation.focused_index < outcome.operation.neighbor_index {
        &outcome.operation.focused_child
    } else {
        &outcome.operation.neighbor_child
    };
    let left_rect = outcome
        .geometry
        .iter()
        .find(|g| &g.leaf == left_id)
        .expect("left geometry");
    assert_eq!(left_rect.rect.x + left_rect.rect.w, 390);
}

#[test]
fn vertical_derivation() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", false);
    r.admit_commit("win-2", false);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    // Vertical split over 600px: propose y=290 grows bottom (focused) to 310.
    let outcome = r.pointer_commit(&k, &w2, Direction::Up, 290);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![289, 309]);
    assert_eq!(outcome.operation.direction, Direction::Up);
}

#[test]
fn nested_outer_boundary_with_inner_untouched() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    r.admit_commit("win-3", false);
    let k = key("out-1", "ws-1");
    let cur = r.focused_window(&k);
    assert_eq!(cur.0, "win-3");
    r.focus_commit(&k, &cur, Direction::Up);
    let w = r.focused_window(&k);
    assert_eq!(w.0, "win-2");
    // Direction Left skips inner vertical group, targets outer H [1,1].
    // Exact boundary 390 projects to [389,409].
    let outcome = r.pointer_commit(&k, &w, Direction::Left, 390);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![389, 409]);
    let root = r.layout.tree(&k).cloned().expect("tree");
    match &root {
        Node::Group {
            children,
            shares,
            axis,
            ..
        } => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![389, 409]);
            assert_eq!(children.len(), 2);
            match &children[1] {
                Node::Group { shares, .. } => assert_eq!(*shares, vec![1, 1]),
                other => panic!("inner {other:?}"),
            }
        }
        other => panic!("root {other:?}"),
    }
}

#[test]
fn nary_only_adjacent_pair_redistributed() {
    // Four-wide needs COSMIC-scale pairs: 1600px so the 400px children form
    // an 800px pair above the 720px pair minimum.
    let mut r = PointerResizeRig::new(vec![domain("out-1", "ws-1", 1600, 600, 0)]);
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
    let mut before = vec![];
    collect_leaves(r.layout.tree(&k).expect("tree"), &mut before);
    // 4x400px strips; pair [win-3,win-4] starts at 800, avail 800.
    // Propose 1190: left=390 -> exact ratio-preserving normalization.
    let outcome = r.pointer_commit(&k, &w4, Direction::Left, 1190);
    assert_eq!(outcome.operation.old_shares, vec![1, 1, 1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![399, 399, 389, 409]);
    assert_eq!(outcome.operation.focused_index, 3);
    assert_eq!(outcome.operation.neighbor_index, 2);
    let mut after = vec![];
    collect_leaves(r.layout.tree(&k).expect("tree"), &mut after);
    assert_eq!(before, after, "order preserved");
    assert_eq!(outcome.operation.new_shares[0], 399);
    assert_eq!(outcome.operation.new_shares[1], 399);
    assert!(outcome.operation.new_shares.iter().all(|v| *v > 0));
}

#[test]
fn clamp_plans_at_minimum_and_outside_domain_is_malformed() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    // Far-left edge clamps to the COSMIC child minimum (360) with exact
    // shares [359,439], projecting the pair boundary exactly to 360.
    let outcome = r.pointer_commit(&k, &w2, Direction::Left, 0);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![359, 439]);
    let left_id = if outcome.operation.focused_index < outcome.operation.neighbor_index {
        &outcome.operation.focused_child
    } else {
        &outcome.operation.neighbor_child
    };
    let left_rect = outcome
        .geometry
        .iter()
        .find(|g| &g.leaf == left_id)
        .expect("left geometry");
    assert_eq!(left_rect.rect.x + left_rect.rect.w, 360);

    // Exact boundary 390 still plans on a fresh rig.
    let mut r2 = PointerResizeRig::single();
    r2.admit_commit("win-1", true);
    r2.admit_commit("win-2", true);
    let k2 = key("out-1", "ws-1");
    let w2b = r2.focused_window(&k2);
    let outcome2 = r2.pointer_commit(&k2, &w2b, Direction::Left, 390);
    assert_eq!(outcome2.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome2.operation.new_shares, vec![389, 409]);

    // Fresh rig: outside the domain work area is malformed.
    let mut t = PointerResizeRig::single();
    t.admit_commit("win-1", true);
    t.admit_commit("win-2", true);
    let tk = key("out-1", "ws-1");
    let tw = t.focused_window(&tk);
    let tobs = t.domain_observed(&tk, vec![]);
    let rev_before = t.layout.accepted_revision(&tk);
    assert_eq!(
        t.layout.pointer_resize(
            &tk,
            &tw,
            Direction::Left,
            900,
            &tw,
            &tobs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );
    assert_eq!(t.layout.accepted_revision(&tk), rev_before);
    // Far outside the work area is also malformed (no project bound).
    assert_eq!(
        t.layout.pointer_resize(
            &tk,
            &tw,
            Direction::Left,
            20000,
            &tw,
            &tobs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );
    assert_eq!(t.layout.accepted_revision(&tk), rev_before);
}

#[test]
fn minimum_exhausted_and_single_leaf_refuse_unchanged() {
    // Sub-minimum pair region: 40px wide under the 720px COSMIC pair
    // minimum -> no feasible split.
    let mut r = PointerResizeRig::new(vec![domain("out-1", "ws-1", 40, 200, 0)]);
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    assert_eq!(
        r.layout.pointer_resize(
            &k,
            &w2,
            Direction::Left,
            20,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);

    // Single root leaf has no boundary.
    let mut t = PointerResizeRig::single();
    t.admit_commit("win-1", true);
    let tk = key("out-1", "ws-1");
    let tw = t.focused_window(&tk);
    let tobs = t.domain_observed(&tk, vec![]);
    let trev_before = t.layout.accepted_revision(&tk);
    assert_eq!(
        t.layout.pointer_resize(
            &tk,
            &tw,
            Direction::Left,
            50,
            &tw,
            &tobs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(t.layout.accepted_revision(&tk), trev_before);
}

#[test]
fn projectability_geometry_covers_and_spans_minimum() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    // 390 is exactly projectable ([389,409]); the pair boundary lands on 390.
    let outcome = r.pointer_commit(&k, &w2, Direction::Left, 390);
    assert_eq!(outcome.geometry.len(), 2);
    for g in &outcome.geometry {
        assert!(g.rect.w > 0 && g.rect.h > 0);
        // Direct leaves keep the COSMIC axis child minimum along the axis.
        assert!(
            i64::from(g.rect.w) >= cosmic_v1::child_min_for_axis(Axis::Horizontal),
            "span {g:?}"
        );
    }
    let min_x = outcome.geometry.iter().map(|g| g.rect.x).min().unwrap();
    let max_e = outcome
        .geometry
        .iter()
        .map(|g| g.rect.x + g.rect.w)
        .max()
        .unwrap();
    assert_eq!(min_x, 0);
    assert_eq!(max_e, 800);
}

#[test]
fn repeated_outcomes_are_deterministic() {
    fn build() -> (PointerResizeRig, DomainKey, WindowId) {
        let mut r = PointerResizeRig::single();
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
        .pointer_resize(
            &ka,
            &wa,
            Direction::Left,
            390,
            &wa,
            &obsa,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("a");
    let pb = b
        .layout
        .pointer_resize(
            &kb,
            &wb,
            Direction::Left,
            390,
            &wb,
            &obsb,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("b");
    assert_eq!(pa, pb);
    assert_eq!(a.layout.tree(&ka), b.layout.tree(&kb));
    assert_eq!(
        a.layout.accepted_revision(&ka),
        b.layout.accepted_revision(&kb)
    );
    let (mut c, kc, wc) = build();
    let obsc = c.domain_observed(&kc, vec![]);
    let pc = c
        .layout
        .pointer_resize(
            &kc,
            &wc,
            Direction::Left,
            390,
            &wc,
            &obsc,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("c");
    assert_eq!(pa.operation.new_shares, pc.operation.new_shares);
}

#[test]
fn unknown_domain_window_and_inactive_edge_refusals() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);

    assert_eq!(
        r.layout.pointer_resize(
            &key("out-9", "ws-1"),
            &w2,
            Direction::Left,
            391,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownDomain))
    );
    assert_eq!(
        r.layout.pointer_resize(
            &k,
            &win("win-9"),
            Direction::Left,
            391,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::UnknownWindow))
    );
    // Pointer-only decoupling: an inactive tiled window does not refuse as
    // focus-mismatch. win-1 is the left leaf while win-2 is active; Left has
    // no neighbor for the leftmost leaf, so it refuses as unchanged.
    assert_eq!(
        r.layout.pointer_resize(
            &k,
            &win("win-1"),
            Direction::Left,
            391,
            &w2,
            &obs,
            crate::policy::ScaledResize::unscaled()
        ),
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);
}

#[test]
fn ordinary_boundary_plans_and_projects_exactly() {
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w2 = r.focused_window(&k);
    let before = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    let outcome = r.pointer_commit(&k, &w2, Direction::Left, 390);
    assert_eq!(outcome.operation.old_shares, vec![1, 1]);
    assert_eq!(outcome.operation.new_shares, vec![389, 409]);
    assert_eq!(outcome.focus_leaf, before);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before));

    let left_id = if outcome.operation.focused_index < outcome.operation.neighbor_index {
        &outcome.operation.focused_child
    } else {
        &outcome.operation.neighbor_child
    };
    let right_id = if outcome.operation.focused_index < outcome.operation.neighbor_index {
        &outcome.operation.neighbor_child
    } else {
        &outcome.operation.focused_child
    };
    let left_rect = outcome
        .geometry
        .iter()
        .find(|g| &g.leaf == left_id)
        .expect("left geometry");
    let right_rect = outcome
        .geometry
        .iter()
        .find(|g| &g.leaf == right_id)
        .expect("right geometry");
    assert_eq!(left_rect.rect.x + left_rect.rect.w, 390);
    assert_eq!(right_rect.rect.x, 390);

    // Proposal 391 still plans exactly with [390,408] on a fresh rig.
    let mut u = PointerResizeRig::single();
    u.admit_commit("win-1", true);
    u.admit_commit("win-2", true);
    let uk = key("out-1", "ws-1");
    let uw = u.focused_window(&uk);
    let outcome81 = u.pointer_commit(&uk, &uw, Direction::Left, 391);
    assert_eq!(outcome81.operation.new_shares, vec![390, 408]);
}

#[test]
fn pointer_two_sided_correction_preserved() {
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 10, Axis::Horizontal),
        Some((360, 440))
    );
    assert_eq!(
        cosmic_v1::clamp_pair_split(800, 790, Axis::Horizontal),
        Some((440, 360))
    );
    assert_eq!(
        cosmic_v1::clamp_keyboard_shrink_pair(500, 220, 12, Axis::Horizontal),
        Some((488, 232))
    );
    let mut r = PointerResizeRig::single();
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    let k = key("out-1", "ws-1");
    let w = r.focused_window(&k);
    let obs = r.domain_observed(&k, vec![]);
    let outcome = r
        .layout
        .pointer_resize(
            &k,
            &w,
            Direction::Left,
            390,
            &w,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("pointer plans");
    assert!(!outcome.operation.new_shares.contains(&0));
    assert_ne!(outcome.operation.new_shares, outcome.operation.old_shares);
}

#[test]
fn corner_commits_both_axes_atomically() {
    let (mut r, k, w) = nested_focus_win2();
    // Accepted nested layout: root H [win-1 | V[win-2, win-3]] over
    // 800x600, so the shared horizontal edge sits at 400 and the shared
    // vertical edge at 300. One corner request moves both.
    let base = r.layout.accepted_revision(&k);
    let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    let outcome = r.corner_commit(&k, &w, Direction::Left, 390, Direction::Down, 310);
    assert_eq!(r.layout.accepted_revision(&k), base + 1);
    assert_eq!(outcome.operation_h.direction, Direction::Left);
    assert_eq!(outcome.operation_v.direction, Direction::Down);

    let win2 = outcome
        .geometry
        .iter()
        .find(|g| g.window.0 == "win-2")
        .expect("win-2 geometry");
    assert_eq!(win2.rect.x, 390);
    assert_eq!(win2.rect.x + win2.rect.w, 800);
    assert_eq!(win2.rect.y + win2.rect.h, 310);
    let win1 = outcome
        .geometry
        .iter()
        .find(|g| g.window.0 == "win-1")
        .expect("win-1 geometry");
    assert_eq!(win1.rect.x + win1.rect.w, 390);
    let win3 = outcome
        .geometry
        .iter()
        .find(|g| g.window.0 == "win-3")
        .expect("win-3 geometry");
    assert_eq!(win3.rect.x, 390);
    assert_eq!(win3.rect.y, 310);
    assert_eq!(outcome.focus_leaf, before_focus);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
}

#[test]
fn corner_refuses_unchanged_when_either_axis_is_a_noop() {
    let (mut r, k, w) = nested_focus_win2();
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    let tree_before = r.layout.tree(&k).cloned();

    // Horizontal edge already at 400: the corner is a no-op on that axis.
    let refused = r.layout.pointer_resize_corner(
        &k,
        &w,
        Direction::Left,
        400,
        Direction::Down,
        310,
        &w,
        &obs,
        crate::policy::ScaledResize::unscaled(),
    );
    assert_eq!(
        refused,
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);
    assert_eq!(r.layout.tree(&k).cloned(), tree_before);

    // Vertical edge already at 300: likewise refused as a whole corner.
    let refused = r.layout.pointer_resize_corner(
        &k,
        &w,
        Direction::Left,
        390,
        Direction::Down,
        300,
        &w,
        &obs,
        crate::policy::ScaledResize::unscaled(),
    );
    assert_eq!(
        refused,
        Err(LayoutFailure::Semantics(RefusalKind::Unchanged))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);
    assert_eq!(r.layout.tree(&k).cloned(), tree_before);
}

#[test]
fn corner_refuses_same_axis_pair_as_malformed() {
    let (mut r, k, w) = nested_focus_win2();
    let obs = r.domain_observed(&k, vec![]);
    let rev_before = r.layout.accepted_revision(&k);
    let refused = r.layout.pointer_resize_corner(
        &k,
        &w,
        Direction::Left,
        390,
        Direction::Right,
        700,
        &w,
        &obs,
        crate::policy::ScaledResize::unscaled(),
    );
    assert_eq!(
        refused,
        Err(LayoutFailure::Semantics(RefusalKind::MalformedInput))
    );
    assert_eq!(r.layout.accepted_revision(&k), rev_before);
}

#[test]
fn drag23_inactive_corner_resize_preserves_focus() {
    // drag-23: 1528x1016 domain, root H [win-1 | V[win-2 (top, active) |
    // win-3 (bottom, dragged)]]. One corner resize on the inactive bottom leaf
    // moves left+up from (680,538,848,478) to (397,315,1131,701); active focus
    // is preserved and keyboard resize of the inactive window still refuses.
    let mut r = PointerResizeRig::new(vec![domain("out-1", "ws-1", 1528, 1016, 0)]);
    r.admit_commit("win-1", true);
    r.admit_commit("win-2", true);
    r.admit_commit("win-3", false);
    let k = key("out-1", "ws-1");
    r.focus_commit_step(&k, Direction::Up);
    let active = r.focused_window(&k);
    assert_eq!(active.0, "win-2");
    let dragged = win("win-3");

    // Stage the start geometry (680,538,848,478) for the dragged leaf.
    let stage = r.corner_commit(&k, &dragged, Direction::Left, 680, Direction::Up, 538);
    let stage_geom = |window: &str| {
        stage
            .geometry
            .iter()
            .find(|g| g.window.0 == window)
            .unwrap_or_else(|| panic!("stage geometry for {window}"))
            .rect
    };
    assert_eq!(
        stage_geom("win-3"),
        Rect {
            x: 680,
            y: 538,
            w: 848,
            h: 478
        }
    );
    assert_eq!(r.focused_window(&k).0, "win-2");
    let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    let base = r.layout.accepted_revision(&k);

    let outcome = r.corner_commit(&k, &dragged, Direction::Left, 397, Direction::Up, 315);
    assert_eq!(r.layout.accepted_revision(&k), base + 1);
    let geom = |window: &str| {
        outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == window)
            .unwrap_or_else(|| panic!("geometry for {window}"))
            .rect
    };
    assert_eq!(
        geom("win-3"),
        Rect {
            x: 397,
            y: 315,
            w: 1131,
            h: 701
        }
    );
    assert_eq!(
        geom("win-2"),
        Rect {
            x: 397,
            y: 0,
            w: 1131,
            h: 315
        }
    );
    assert_eq!(
        geom("win-1"),
        Rect {
            x: 0,
            y: 0,
            w: 397,
            h: 1016
        }
    );
    assert_eq!(outcome.focus_leaf, before_focus);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
    assert_eq!(r.focused_window(&k).0, "win-2");
    assert_eq!(outcome.base_revision, base);

    // Keyboard resize of the inactive dragged window still refuses.
    let obs = r.domain_observed(&k, vec![]);
    assert_eq!(
        r.layout.keyboard_resize(
            &k,
            &dragged,
            Direction::Left,
            ResizeMode::Inwards,
            0,
            &active,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        ),
        Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
    );
}

#[test]
fn pointer_resize_inactive_target_with_overlay_active_plans() {
    // Active window observed fullscreen/floating/sticky must not reject a
    // valid inactive tiled drag; the dragged window's own flags still
    // refuse, focus is preserved, and keyboard is unchanged.
    for flag in ["fullscreen", "floating", "sticky"] {
        let mut r = PointerResizeRig::single();
        r.admit_commit("win-1", true);
        r.admit_commit("win-2", true);
        let k = key("out-1", "ws-1");
        let active = r.focused_window(&k);
        assert_eq!(active.0, "win-2");
        let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
        let mut obs = r.domain_observed(&k, vec![]);
        for w in obs.iter_mut() {
            if w.window.0 == "win-2" {
                match flag {
                    "fullscreen" => w.fullscreen = true,
                    "floating" => w.floating = true,
                    _ => w.sticky = true,
                }
            }
        }
        // Layout [1,1] over 800px: shared edge at 400. Inactive win-1 grows
        // right to 410, mirroring the focused Left@390 derivation.
        let base = r.layout.accepted_revision(&k);
        let outcome = r
            .layout
            .pointer_resize(
                &k,
                &win("win-1"),
                Direction::Right,
                410,
                &active,
                &obs,
                crate::policy::ScaledResize::unscaled(),
            )
            .unwrap_or_else(|e| panic!("inactive drag with {flag} active: {e:?}"));
        assert_eq!(outcome.operation.new_shares, vec![409, 389]);
        let left = outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == "win-1")
            .expect("win-1 geometry");
        assert_eq!(left.rect.x + left.rect.w, 410);
        assert_eq!(outcome.focus_leaf, before_focus);
        assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
        assert_eq!(r.focused_window(&k).0, "win-2");
        assert_eq!(outcome.base_revision, base);
        assert_eq!(outcome.revision, base + 1);
        assert_eq!(r.layout.accepted_revision(&k), base + 1);

        // The dragged window itself flagged still refuses.
        let mut flagged = r.domain_observed(&k, vec![]);
        for w in flagged.iter_mut() {
            if w.window.0 == "win-1" {
                w.fullscreen = true;
            }
        }
        assert_eq!(
            r.layout.pointer_resize(
                &k,
                &win("win-1"),
                Direction::Right,
                410,
                &active,
                &flagged,
                crate::policy::ScaledResize::unscaled(),
            ),
            Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
        );
        // Keyboard resize of the inactive window still refuses focus binding.
        let clean = r.domain_observed(&k, vec![]);
        assert_eq!(
            r.layout.keyboard_resize(
                &k,
                &win("win-1"),
                Direction::Right,
                ResizeMode::Outwards,
                0,
                &active,
                &clean,
                crate::policy::ScaledResize::unscaled(),
            ),
            Err(LayoutFailure::Semantics(RefusalKind::FocusMismatch))
        );
    }
}

#[test]
fn pointer_corner_inactive_target_with_overlay_active_plans() {
    // Corner variant of the overlay-active decoupling: root H [win-1 |
    // V[win-2 (active, observed fullscreen) | win-3 (dragged)]]. One corner
    // resize on the inactive bottom leaf moves its left edge to 390 and its
    // top edge to 290 with active focus preserved.
    let (mut r, k, active) = nested_focus_win2();
    assert_eq!(active.0, "win-2");
    let before_focus = r.layout.focus_leaf(&k).cloned().expect("focus leaf");
    let mut obs = r.domain_observed(&k, vec![]);
    for w in obs.iter_mut() {
        if w.window.0 == "win-2" {
            w.fullscreen = true;
        }
    }
    let outcome = r
        .layout
        .pointer_resize_corner(
            &k,
            &win("win-3"),
            Direction::Left,
            390,
            Direction::Up,
            290,
            &active,
            &obs,
            crate::policy::ScaledResize::unscaled(),
        )
        .expect("inactive corner drag with overlay active");
    let geom = |window: &str| {
        outcome
            .geometry
            .iter()
            .find(|g| g.window.0 == window)
            .unwrap_or_else(|| panic!("geometry for {window}"))
            .rect
    };
    assert_eq!(geom("win-3").x, 390);
    assert_eq!(geom("win-3").y, 290);
    assert_eq!(outcome.focus_leaf, before_focus);
    assert_eq!(r.layout.focus_leaf(&k), Some(&before_focus));
    assert_eq!(r.focused_window(&k).0, "win-2");
}
