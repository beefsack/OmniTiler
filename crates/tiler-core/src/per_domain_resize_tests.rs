//! Relocated per-domain keyboard-resize regression tests (COSMIC step schedule,
//! share arithmetic, clamps, refusal atomicity). Moved from the Windows adapter
//! suite with the per-domain route removal; observations via [`crate::per_domain_fixture`].

use crate::contract::ResizeMode;
use crate::directional::{Direction, WindowId};
use crate::geometry::Rect;
use crate::per_domain_fixture::engine_windows_from_floating;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain, RefusalKind};
use crate::size_hints::WindowSizeHints;
use crate::world_layout::{
    LayoutFailure, LocalOperationContext, LocalOperationFailure, ResizeOutcome, WorldLayout,
};

/// Retired adapter parity value (inner gap 8) used by the relocated
/// observation fixtures below.
const INNER_GAP: i32 = 8;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn parse_direction(direction: &str) -> Direction {
    match direction {
        "left" => Direction::Left,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "down" => Direction::Down,
        other => panic!("unexpected direction {other}"),
    }
}

fn parse_mode(mode: &str) -> ResizeMode {
    match mode {
        "outwards" => ResizeMode::Outwards,
        "inwards" => ResizeMode::Inwards,
        other => panic!("unexpected mode {other}"),
    }
}

/// Retained-`WorldLayout` rig for keyboard resize through `run_keyboard_resize`.
/// Rects track the committed plan geometry so consecutive presses observe
/// their own writes, exactly like consecutive live ticks.
struct ResizeRig {
    layout: WorldLayout,
    domain: (OutputDomain, DomainKey),
    rects: Vec<(String, Rect)>,
    hints: std::collections::HashMap<String, WindowSizeHints>,
    focused: String,
}

impl ResizeRig {
    fn seed(names: &[&str], focused: &str) -> Self {
        // Explicit side-by-side seed matching the established
        // pointer-resize seed shape. Domain bounds are pre-inset by the
        // outer gap (bounds `(8,8,1584,884)` for a 1600x900 work area),
        // so the observation already equals the projection and the seed
        // converges without reshaping.
        debug_assert_eq!(names.len(), 2);
        Self::seed_placed(
            rect(8, 8, 1584, 884),
            &[
                (names[0], rect(8, 8, 788, 884)),
                (names[1], rect(804, 8, 788, 884)),
            ],
            focused,
        )
    }

    fn seed_placed(bounds: Rect, placed: &[(&str, Rect)], focused: &str) -> Self {
        let layout = WorldLayout::new();
        let domain = {
            use crate::directional::{OutputId, WorkspaceId};
            let key = DomainKey {
                output: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
            };
            let domain = OutputDomain {
                id: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                bounds,
                gap: 8,
                adjacent: std::collections::BTreeMap::new(),
            };
            (domain, key)
        };
        let rects: Vec<(String, Rect)> = placed
            .iter()
            .map(|(name, r)| ((*name).to_owned(), *r))
            .collect();
        let mut rig = Self {
            layout,
            domain,
            rects,
            hints: std::collections::HashMap::new(),
            focused: focused.to_owned(),
        };
        rig.reconcile();
        rig
    }

    fn set_hint(&mut self, window: &str, hints: WindowSizeHints) {
        self.hints.insert(window.to_owned(), hints);
    }

    fn observed(&self) -> Vec<EngineWindow> {
        let windows: Vec<(WindowId, Rect, WindowSizeHints, bool)> = self
            .rects
            .iter()
            .map(|(w, r)| {
                (
                    WindowId(w.clone()),
                    *r,
                    self.hints
                        .get(w)
                        .copied()
                        .unwrap_or(WindowSizeHints::none()),
                    false,
                )
            })
            .collect();
        engine_windows_from_floating(&self.domain.1, &windows)
    }

    fn reconcile(&mut self) {
        let observed = self.observed();
        let out = self
            .layout
            .reconcile_fresh(
                &self.domain.0,
                8,
                &WindowId(self.focused.clone()),
                &observed,
            )
            .expect("seed must converge");
        self.rects = out
            .projection
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect();
    }

    fn rz(
        &mut self,
        window: &str,
        direction: &str,
        mode: &str,
        press_index: u32,
    ) -> Result<ResizeOutcome, LocalOperationFailure> {
        let observed = self.observed();
        let focused = WindowId(self.focused.clone());
        let ctx = LocalOperationContext {
            domain: &self.domain.0,
            outer_gap: 8,
            observed_focus: &focused,
            observed: &observed,
            preconverged: false,
        };
        let result = self.layout.run_keyboard_resize(
            &ctx,
            &WindowId(window.to_owned()),
            parse_direction(direction),
            parse_mode(mode),
            press_index,
        );
        if let Ok(plan) = &result {
            self.rects = plan
                .geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
        }
        result
    }

    fn toggle_orientation(&mut self, window: &str) {
        let observed = self.observed();
        let focused = WindowId(self.focused.clone());
        let ctx = LocalOperationContext {
            domain: &self.domain.0,
            outer_gap: 8,
            observed_focus: &focused,
            observed: &observed,
            preconverged: false,
        };
        let outcome = self
            .layout
            .run_toggle_orientation(&ctx, &WindowId(window.to_owned()))
            .expect("toggle must commit");
        self.rects = outcome
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect();
    }

    fn geom(&self, window: &str) -> Rect {
        self.rects
            .iter()
            .find(|(w, _)| w == window)
            .unwrap_or_else(|| panic!("rect for {window}"))
            .1
    }

    fn order(&self) -> Vec<String> {
        self.rects.iter().map(|(w, _)| w.clone()).collect()
    }
}

fn resize_plan(result: Result<ResizeOutcome, LocalOperationFailure>, what: &str) -> ResizeOutcome {
    result.unwrap_or_else(|e| panic!("{what} must plan a resize, got {e:?}"))
}

fn rejected_kind(result: &Result<ResizeOutcome, LocalOperationFailure>) -> RefusalKind {
    match result {
        Err(LocalOperationFailure::Layout(LayoutFailure::Semantics(kind))) => *kind,
        other => panic!("expected a semantic refusal, got {other:?}"),
    }
}

#[test]
fn engine_keyboard_resize_grows_focused_pair_by_initial_12px() {
    // H[w1, w2] 50/50, w2 focused: grow w2 leftwards (towards w1) one step.
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    assert_eq!(rig.geom("w1"), rect(8, 8, 788, 884));
    assert_eq!(rig.geom("w2"), rect(804, 8, 788, 884));
    let plan = resize_plan(rig.rz("w2", "left", "outwards", 0), "initial outwards");
    // Only the shared boundary moves, by exactly the initial 12px step; the
    // outer edges never move and heights never change.
    assert_eq!(rig.geom("w1"), rect(8, 8, 776, 884));
    assert_eq!(rig.geom("w2"), rect(792, 8, 800, 884));
    assert_eq!(
        plan.focus_leaf,
        rig.layout.focus_leaf(&rig.domain.1).expect("focus").clone()
    );
    assert_eq!(rig.order(), vec!["w1".to_owned(), "w2".to_owned()]);
    // The two adjacent shares move as a pair; nothing else exists to move.
    assert_eq!(plan.operation.old_shares.len(), 2);
    assert_eq!(plan.operation.new_shares.len(), 2);
    assert_ne!(plan.operation.old_shares, plan.operation.new_shares);
}

#[test]
fn engine_keyboard_resize_repeat_schedule_caps_at_20px() {
    // Consecutive presses walk 12/14/16/18/20px and stay capped at 20px:
    // the COSMIC `(10 + 2 + 2 * press_index).min(20)` schedule through
    // `WorldLayout`.
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    let mut widths = Vec::new();
    for press_index in 0..=5u32 {
        resize_plan(
            rig.rz("w2", "left", "outwards", press_index),
            &format!("press {press_index}"),
        );
        widths.push(rig.geom("w2").w);
    }
    let deltas: Vec<i32> = widths
        .iter()
        .scan(788, |prev, w| {
            let delta = *w - *prev;
            *prev = *w;
            Some(delta)
        })
        .collect();
    assert_eq!(deltas, vec![12, 14, 16, 18, 20, 20]);
    for press_index in 0..=5u32 {
        assert_eq!(
            crate::cosmic_v1::keyboard_step_px(press_index),
            [12, 14, 16, 18, 20, 20][press_index as usize]
        );
    }
    // The pair sum is conserved through the whole run (no leak, no gap
    // drift): every pixel the focused window gains comes from its neighbor.
    assert_eq!(rig.geom("w1").w + rig.geom("w2").w, 788 + 788);
}

#[test]
fn engine_keyboard_resize_inwards_reverses_outwards_exactly() {
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    resize_plan(rig.rz("w2", "left", "outwards", 0), "grow");
    assert_eq!(rig.geom("w2").w, 800);
    resize_plan(rig.rz("w2", "left", "inwards", 0), "shrink");
    assert_eq!(rig.geom("w1"), rect(8, 8, 788, 884));
    assert_eq!(rig.geom("w2"), rect(804, 8, 788, 884));
}

#[test]
fn engine_keyboard_resize_outer_edge_and_cross_axis_refuse_without_mutation() {
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    // w2 already touches the right work-area edge: growing rightwards has no
    // applicable boundary and refuses `Unchanged` with no plan, exactly like KDE.
    let reply = rig.rz("w2", "right", "outwards", 0);
    assert_eq!(rejected_kind(&reply), RefusalKind::Unchanged);
    // A vertical resize on a horizontal pair likewise refuses without
    // mutating: there is no matching-axis boundary in that direction.
    let reply = rig.rz("w2", "up", "outwards", 0);
    assert_eq!(rejected_kind(&reply), RefusalKind::Unchanged);
    // The refusals mutate nothing and strand nothing: the next valid press
    // still plans the exact initial step.
    assert_eq!(rig.geom("w1"), rect(8, 8, 788, 884));
    assert_eq!(rig.geom("w2"), rect(804, 8, 788, 884));
    resize_plan(rig.rz("w2", "left", "outwards", 0), "post-refusal");
    assert_eq!(rig.geom("w2").w, 800);
}

#[test]
fn engine_keyboard_resize_minimum_clamps_shrink_side_then_exhausts() {
    // One-sided minima clamp: w1 declares min_w 784 (current 788), so the
    // initial 12px shrink clamps to 4px on w1 only; w2 still grows by exactly
    // the clamped transfer and the pair sum is conserved.
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    rig.set_hint(
        "w1",
        crate::size_hints::WindowSizeHints {
            min_w: Some(784),
            min_h: None,
            max_w: None,
            max_h: None,
        },
    );
    resize_plan(rig.rz("w2", "left", "outwards", 0), "clamped grow");
    assert_eq!(rig.geom("w1"), rect(8, 8, 784, 884));
    assert_eq!(rig.geom("w2"), rect(800, 8, 792, 884));
    assert_eq!(rig.geom("w1").w + rig.geom("w2").w, 788 + 788);
    // With w1 exactly at its minimum, continued grows keep planning (the
    // shares still move) but the projected geometry stays frozen at the
    // minimum: the hint is never violated, and production settles these as
    // `resize-noop` with no native write.
    for press_index in 1..=3u32 {
        resize_plan(
            rig.rz("w2", "left", "outwards", press_index),
            &format!("exhausted press {press_index}"),
        );
        assert_eq!(rig.geom("w1"), rect(8, 8, 784, 884));
        assert_eq!(rig.geom("w2"), rect(800, 8, 792, 884));
    }
}

#[test]
fn engine_keyboard_resize_vertical_pair_moves_adjacent_heights_only() {
    // Flip the H pair to V through `run_toggle_orientation`, then
    // grow the focused lower window upwards: heights move by the step while
    // widths and the outer frame stay byte-identical.
    let mut rig = ResizeRig::seed(&["w1", "w2"], "w2");
    rig.toggle_orientation("w2");
    let top = rig.geom("w1");
    let bottom = rig.geom("w2");
    assert_eq!((top.w, bottom.w), (1584, 1584));
    assert_eq!((top.x, bottom.x), (8, 8));
    assert!(
        bottom.y > top.y,
        "w2 stacks below w1: {top:?} vs {bottom:?}"
    );
    let focus_before = plan_focus_leaf(&rig);
    let plan = resize_plan(rig.rz("w2", "up", "outwards", 0), "vertical grow");
    assert_eq!(rig.geom("w1").w, 1584);
    assert_eq!(rig.geom("w2").w, 1584);
    assert_eq!(top.h - rig.geom("w1").h, 12);
    assert_eq!(rig.geom("w2").h - bottom.h, 12);
    assert_eq!(rig.geom("w1").x, 8);
    assert_eq!(plan.focus_leaf, focus_before);
    assert_eq!(rig.order(), vec!["w1".to_owned(), "w2".to_owned()]);
}

#[test]
fn engine_keyboard_resize_middle_window_leaves_far_sibling_untouched() {
    // Three-up H row with the middle window focused: growing the middle
    // leftwards moves only the adjacent shares; the far share is
    // byte-identical, the shared boundary moves exactly the step, and
    // order/focus never shuffle. Widths are read off the converged seed;
    // all asserts below are relative to that converged baseline.
    let mut rig = ResizeRig::seed_placed(
        rect(8, 8, 1576, 884),
        &[
            ("w1", rect(8, 8, 520, 884)),
            ("w2", rect(536, 8, 520, 884)),
            ("w3", rect(1064, 8, 520, 884)),
        ],
        "w2",
    );
    let (w1, w2, w3) = (rig.geom("w1"), rig.geom("w2"), rig.geom("w3"));
    assert_eq!((w1.y, w2.y, w3.y), (8, 8, 8));
    assert_eq!((w1.h, w2.h, w3.h), (884, 884, 884));
    assert!(w1.x < w2.x && w2.x < w3.x, "H row order");
    assert_eq!(
        (w1.x + w1.w + INNER_GAP, w2.x + w2.w + INNER_GAP),
        (w2.x, w3.x)
    );
    let focus_before = plan_focus_leaf(&rig);
    let order_before = rig.order();
    let plan = resize_plan(rig.rz("w2", "left", "outwards", 0), "middle grow");
    // Adjacency-only is exact in share space: only the two adjacent shares
    // move (by exactly the 12px step); the far share is byte-identical.
    assert_eq!(plan.operation.old_shares, vec![520, 520, 520]);
    assert_eq!(plan.operation.new_shares, vec![508, 532, 520]);
    assert_eq!(
        (plan.operation.focused_index, plan.operation.neighbor_index),
        (1, 0)
    );
    // In pixel space the shared boundary moves exactly 12px; the far
    // sibling only absorbs projector rounding (<=1px), never a share
    // transfer, and the row total is conserved.
    assert_eq!(rig.geom("w1"), Rect { w: w1.w - 12, ..w1 });
    assert_eq!(rig.geom("w2").x, w2.x - 12);
    assert_eq!(rig.geom("w2").w, w2.w + 11);
    assert!((rig.geom("w3").x - w3.x).abs() <= 1);
    assert!((rig.geom("w3").w - w3.w).abs() <= 1);
    assert_eq!(
        rig.geom("w1").w + INNER_GAP + rig.geom("w2").w + INNER_GAP + rig.geom("w3").w,
        w1.w + INNER_GAP + w2.w + INNER_GAP + w3.w
    );
    assert_eq!(plan.focus_leaf, focus_before);
    assert_eq!(rig.order(), order_before);
}

fn plan_focus_leaf(rig: &ResizeRig) -> crate::directional::NodeId {
    rig.layout
        .focus_leaf(&rig.domain.1)
        .expect("focus leaf")
        .clone()
}
