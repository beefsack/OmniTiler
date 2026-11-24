//! Portable layout-policy seam over the COSMIC v1 behavior.
//!
//! [`LayoutPolicy`] groups every COSMIC-specific semantic consumed by
//! [`crate::session::Session`]: movement planning, admission axis and share
//! arithmetic, keyboard/pointer pixel resize, and drag/drop classification.
//! [`CosmicV1Policy`] is the only implementation and delegates each operation
//! to [`crate::cosmic_v1`]. Sessions and the engine hold the selected policy
//! and route every policy decision through it, preserving exact behavior.
//!
//! Dependency-free: `std` plus intra-crate portable modules only, so the
//! existing portable gate is unaffected.

use std::fmt::Debug;
use std::sync::Arc;

use crate::contract::DragSide;
use crate::cosmic_v1;
use crate::directional::{Axis, Capabilities, MoveIntent, MoveOutcome, NodeId, Snapshot};
use crate::geometry::Rect;

pub use crate::cosmic_v1::PriorGroupEdge;

/// Per-output physical resize metrics: every logical resize constant scaled
/// once by that output's [`crate::world_report::ScreenReport::scale`].
///
/// [`LayoutPolicy`] keeps the logical constants as the single source; this
/// block is the pre-scaled physical view one output's operations consume, so
/// no resize or gesture path re-rounds mid-operation. Integer-only (and
/// `Eq`) for the same reason the staged gap pair is: a scale change never
/// rescales a retained operation.
///
/// Selection rule: local resize uses the source output's metrics (the
/// staged domain the subject lives on); pair resize constraints and
/// group-edge classification use the target output's metrics (the domain
/// the pointer/placement lands in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScaledResize {
    /// Logical keyboard schedule 12/14/16/18/20, each scaled individually.
    pub keyboard_steps: [i32; 5],
    /// Pair minimum by axis index (`[Horizontal, Vertical]`): 720/480.
    pub pair_min: [i64; 2],
    /// Child minimum by axis index (`[Horizontal, Vertical]`): 360/240.
    pub child_min: [i64; 2],
    /// Group edge zone depth for non-sticky edges: logical 32.
    pub group_edge_inactive: i32,
    /// Group edge zone depth for the sticky prior group+edge: logical 80.
    pub group_edge_active: i32,
}

impl ScaledResize {
    /// Logical metrics at scale 1.0: the identity block fixtures and
    /// scale-free callers use. Never a production fallback for a missing
    /// output scale (staged domains always carry one).
    #[must_use]
    pub const fn unscaled() -> Self {
        Self {
            keyboard_steps: [
                cosmic_v1::keyboard_step_px(0),
                cosmic_v1::keyboard_step_px(1),
                cosmic_v1::keyboard_step_px(2),
                cosmic_v1::keyboard_step_px(3),
                cosmic_v1::keyboard_step_px(4),
            ],
            pair_min: [
                cosmic_v1::COSMIC_PAIR_MIN_WIDTH_PX,
                cosmic_v1::COSMIC_PAIR_MIN_HEIGHT_PX,
            ],
            child_min: [
                cosmic_v1::COSMIC_CHILD_MIN_WIDTH_PX,
                cosmic_v1::COSMIC_CHILD_MIN_HEIGHT_PX,
            ],
            group_edge_inactive: cosmic_v1::COSMIC_GROUP_EDGE_INACTIVE_PX,
            group_edge_active: cosmic_v1::COSMIC_GROUP_EDGE_ACTIVE_PX,
        }
    }

    /// All-zero metrics: the placeholder an overflowed output stages. The
    /// owning domain is unreadable in the same pass, so these authorize
    /// nothing; zero (rather than a clamped or unscaled value) keeps a
    /// silently rescaled physics path impossible.
    #[must_use]
    pub const fn zeroed() -> Self {
        Self {
            keyboard_steps: [0; 5],
            pair_min: [0; 2],
            child_min: [0; 2],
            group_edge_inactive: 0,
            group_edge_active: 0,
        }
    }

    /// Scale every logical resize constant once for one output.
    ///
    /// Each keyboard step is scaled from its computed logical value (12, 14,
    /// 16, 18, 20), not from a rounded base+increment formula: at 1.25 the
    /// schedule is 15/18/20/23/25. Same nearest, halves-upward checked rule
    /// as [`crate::world_engine::WorldEngine::scale_once`]. `None` for a
    /// non-positive or non-finite scale, or any overflow: callers stage the
    /// output unreadable with one deduplicated diagnostic and never clamp.
    #[must_use]
    pub fn from_scale(scale: f64) -> Option<Self> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let scale_i32 = |logical: i32| crate::world_engine::WorldEngine::scale_once(logical, scale);
        let scale_i64 =
            |logical: i64| crate::world_engine::WorldEngine::scale_once_i64(logical, scale);
        Some(Self {
            keyboard_steps: [
                scale_i32(cosmic_v1::keyboard_step_px(0))?,
                scale_i32(cosmic_v1::keyboard_step_px(1))?,
                scale_i32(cosmic_v1::keyboard_step_px(2))?,
                scale_i32(cosmic_v1::keyboard_step_px(3))?,
                scale_i32(cosmic_v1::keyboard_step_px(4))?,
            ],
            pair_min: [
                scale_i64(cosmic_v1::COSMIC_PAIR_MIN_WIDTH_PX)?,
                scale_i64(cosmic_v1::COSMIC_PAIR_MIN_HEIGHT_PX)?,
            ],
            child_min: [
                scale_i64(cosmic_v1::COSMIC_CHILD_MIN_WIDTH_PX)?,
                scale_i64(cosmic_v1::COSMIC_CHILD_MIN_HEIGHT_PX)?,
            ],
            group_edge_inactive: scale_i32(cosmic_v1::COSMIC_GROUP_EDGE_INACTIVE_PX)?,
            group_edge_active: scale_i32(cosmic_v1::COSMIC_GROUP_EDGE_ACTIVE_PX)?,
        })
    }

    const fn axis_index(axis: Axis) -> usize {
        match axis {
            Axis::Horizontal => 0,
            Axis::Vertical => 1,
        }
    }

    /// Physical keyboard step for `press_index` (capped at the last entry).
    #[must_use]
    pub fn keyboard_step_px(&self, press_index: u32) -> i32 {
        self.keyboard_steps[press_index.min(4) as usize]
    }

    /// Physical resize pair minimum along `axis`.
    #[must_use]
    pub fn pair_min_for_axis(&self, axis: Axis) -> i64 {
        self.pair_min[Self::axis_index(axis)]
    }

    /// Physical resized child minimum along `axis`.
    #[must_use]
    pub fn child_min_for_axis(&self, axis: Axis) -> i64 {
        self.child_min[Self::axis_index(axis)]
    }

    /// Whether a direct pair extent admits a resize along `axis`.
    #[must_use]
    pub fn pair_admits_resize(&self, pair_w: i64, pair_h: i64, axis: Axis) -> bool {
        let min = self.pair_min_for_axis(axis);
        match axis {
            Axis::Horizontal => pair_w >= min,
            Axis::Vertical => pair_h >= min,
        }
    }

    /// Pointer two-sided clamped split using the physical child minimum.
    #[must_use]
    pub fn clamp_pair_split(
        &self,
        pair_total: i64,
        desired_first: i64,
        axis: Axis,
    ) -> Option<(i64, i64)> {
        cosmic_v1::clamp_pair_split_with(pair_total, desired_first, self.child_min_for_axis(axis))
    }

    /// Keyboard one-sided shrink step using the physical pair/child minima.
    #[must_use]
    pub fn clamp_keyboard_shrink_pair(
        &self,
        shrink: i64,
        grow: i64,
        amount: i64,
        axis: Axis,
    ) -> Option<(i64, i64)> {
        cosmic_v1::clamp_keyboard_shrink_pair_with(
            shrink,
            grow,
            amount,
            self.pair_min_for_axis(axis),
            self.child_min_for_axis(axis),
        )
    }

    /// Physical group-edge depth for this group/edge and prior hover.
    #[must_use]
    pub fn group_edge_depth_for(
        &self,
        group: &NodeId,
        edge: DragSide,
        prior: Option<&PriorGroupEdge>,
    ) -> i32 {
        cosmic_v1::group_edge_depth_with(
            group,
            edge,
            prior,
            self.group_edge_inactive,
            self.group_edge_active,
        )
    }

    /// Group-edge classification using this output's physical edge depths.
    #[must_use]
    pub fn classify_group_point(
        &self,
        rect: &Rect,
        group: &NodeId,
        x: i32,
        y: i32,
        prior: Option<&PriorGroupEdge>,
    ) -> Option<DragSide> {
        cosmic_v1::classify_group_point_with(
            rect,
            group,
            x,
            y,
            prior,
            self.group_edge_inactive,
            self.group_edge_active,
        )
    }
}

/// Portable layout policy: the COSMIC-specific decisions a session needs.
///
/// Grouped by the actual session call sites (planning, admission/shares,
/// resize, drag/drop). Implementations must preserve the exact COSMIC v1
/// semantics documented on each [`crate::cosmic_v1`] item.
pub trait LayoutPolicy: Debug + Send + Sync {
    /// Accepted movement planning for one focused move intent.
    fn plan_move(
        &self,
        snapshot: &Snapshot,
        intent: &MoveIntent,
        capabilities: &Capabilities,
    ) -> MoveOutcome;

    /// Admission split axis for a placement target geometry.
    fn admission_axis_for_rect(&self, bounds: &Rect) -> Axis;
    /// New-group shares for an equal-halves admission split.
    fn new_group_shares(&self) -> [u64; 2];
    /// Proportional N-ary insertion shares preserving survivor ratios.
    fn proportional_insertion_shares(
        &self,
        existing: &[u64],
        insertion_index: usize,
    ) -> Option<Vec<u64>>;
    /// Proportional N-ary removal shares preserving survivor ratios.
    fn proportional_removal_shares(
        &self,
        existing: &[u64],
        removed_index: usize,
    ) -> Option<Vec<u64>>;

    /// Keyboard resize step schedule in physical pixels.
    fn keyboard_step_px(&self, press_index: u32) -> i32;
    /// Whether a resize pair with direct pair extents admits a resize.
    fn pair_admits_resize(&self, pair_w: i64, pair_h: i64, axis: Axis) -> bool;
    /// Pointer two-sided clamped split of a physical pair extent.
    fn clamp_pair_split(
        &self,
        pair_total: i64,
        desired_first: i64,
        axis: Axis,
    ) -> Option<(i64, i64)>;
    /// Keyboard one-sided shrink step on a physical pair.
    fn clamp_keyboard_shrink_pair(
        &self,
        shrink: i64,
        grow: i64,
        amount: i64,
        axis: Axis,
    ) -> Option<(i64, i64)>;
    /// Resize pair minimum along an axis in physical pixels.
    fn pair_min_for_axis(&self, axis: Axis) -> i64;
    /// Resized child minimum along an axis in physical pixels.
    fn child_min_for_axis(&self, axis: Axis) -> i64;

    /// Window-drop classification of a point inside a projected window rect.
    fn classify_window_point(&self, rect: &Rect, x: i32, y: i32) -> Option<DragSide>;
    /// Group-edge classification of a point inside a projected group rect.
    fn classify_group_point(
        &self,
        rect: &Rect,
        group: &NodeId,
        x: i32,
        y: i32,
        prior: Option<&PriorGroupEdge>,
    ) -> Option<DragSide>;
    /// Predecessor index for a group-interior drop at an absolute offset.
    fn insertion_index_for_offset(&self, child_starts: &[i64], offset: i64) -> usize;
}

/// COSMIC v1 policy: the only selected implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CosmicV1Policy;

impl LayoutPolicy for CosmicV1Policy {
    fn plan_move(
        &self,
        snapshot: &Snapshot,
        intent: &MoveIntent,
        capabilities: &Capabilities,
    ) -> MoveOutcome {
        cosmic_v1::plan_move_with_capabilities(snapshot, intent, capabilities)
    }

    fn admission_axis_for_rect(&self, bounds: &Rect) -> Axis {
        cosmic_v1::admission_axis_for_rect(bounds)
    }

    fn new_group_shares(&self) -> [u64; 2] {
        cosmic_v1::new_group_shares()
    }

    fn proportional_insertion_shares(
        &self,
        existing: &[u64],
        insertion_index: usize,
    ) -> Option<Vec<u64>> {
        cosmic_v1::proportional_insertion_shares(existing, insertion_index)
    }

    fn proportional_removal_shares(
        &self,
        existing: &[u64],
        removed_index: usize,
    ) -> Option<Vec<u64>> {
        cosmic_v1::proportional_removal_shares(existing, removed_index)
    }

    fn keyboard_step_px(&self, press_index: u32) -> i32 {
        cosmic_v1::keyboard_step_px(press_index)
    }

    fn pair_admits_resize(&self, pair_w: i64, pair_h: i64, axis: Axis) -> bool {
        cosmic_v1::pair_admits_resize(pair_w, pair_h, axis)
    }

    fn clamp_pair_split(
        &self,
        pair_total: i64,
        desired_first: i64,
        axis: Axis,
    ) -> Option<(i64, i64)> {
        cosmic_v1::clamp_pair_split(pair_total, desired_first, axis)
    }

    fn clamp_keyboard_shrink_pair(
        &self,
        shrink: i64,
        grow: i64,
        amount: i64,
        axis: Axis,
    ) -> Option<(i64, i64)> {
        cosmic_v1::clamp_keyboard_shrink_pair(shrink, grow, amount, axis)
    }

    fn pair_min_for_axis(&self, axis: Axis) -> i64 {
        cosmic_v1::pair_min_for_axis(axis)
    }

    fn child_min_for_axis(&self, axis: Axis) -> i64 {
        cosmic_v1::child_min_for_axis(axis)
    }

    fn classify_window_point(&self, rect: &Rect, x: i32, y: i32) -> Option<DragSide> {
        cosmic_v1::classify_window_point(rect, x, y)
    }

    fn classify_group_point(
        &self,
        rect: &Rect,
        group: &NodeId,
        x: i32,
        y: i32,
        prior: Option<&PriorGroupEdge>,
    ) -> Option<DragSide> {
        cosmic_v1::classify_group_point(rect, group, x, y, prior)
    }

    fn insertion_index_for_offset(&self, child_starts: &[i64], offset: i64) -> usize {
        cosmic_v1::insertion_index_for_offset(child_starts, offset)
    }
}

/// Default selected policy handle (COSMIC v1, the only implementation).
#[must_use]
pub fn default_policy() -> Arc<dyn LayoutPolicy> {
    Arc::new(CosmicV1Policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unscaled_matches_logical_resize_constants() {
        let unscaled = ScaledResize::unscaled();
        assert_eq!(unscaled.keyboard_steps, [12, 14, 16, 18, 20]);
        assert_eq!(unscaled.pair_min, [720, 480]);
        assert_eq!(unscaled.child_min, [360, 240]);
        assert_eq!(unscaled.group_edge_inactive, 32);
        assert_eq!(unscaled.group_edge_active, 80);
        assert_eq!(unscaled, ScaledResize::from_scale(1.0).expect("1.0 scales"));
        // The scaled accessors agree with the logical policy at 1.0.
        for press in 0..6 {
            assert_eq!(
                unscaled.keyboard_step_px(press),
                cosmic_v1::keyboard_step_px(press)
            );
        }
        for axis in [Axis::Horizontal, Axis::Vertical] {
            assert_eq!(
                unscaled.pair_min_for_axis(axis),
                cosmic_v1::pair_min_for_axis(axis)
            );
            assert_eq!(
                unscaled.child_min_for_axis(axis),
                cosmic_v1::child_min_for_axis(axis)
            );
        }
    }

    #[test]
    fn keyboard_schedule_scales_each_computed_step() {
        // Each logical step is scaled individually: 1.25 is 15/18/20/23/25,
        // not a rounded base+increment schedule.
        for (scale, expected) in [
            (1.0, [12, 14, 16, 18, 20]),
            (1.25, [15, 18, 20, 23, 25]),
            (1.5, [18, 21, 24, 27, 30]),
            (2.0, [24, 28, 32, 36, 40]),
        ] {
            let resize = ScaledResize::from_scale(scale).expect("scale is finite");
            assert_eq!(resize.keyboard_steps, expected, "scale {scale}");
            for (press, step) in expected.iter().enumerate() {
                assert_eq!(
                    resize.keyboard_step_px(press as u32),
                    *step,
                    "scale {scale} press {press}"
                );
            }
            // The schedule caps at the last entry for every later press.
            assert_eq!(resize.keyboard_step_px(4), expected[4]);
            assert_eq!(resize.keyboard_step_px(100), expected[4]);
            assert_eq!(resize.keyboard_step_px(u32::MAX), expected[4]);
        }
    }

    #[test]
    fn minima_and_group_depths_scale_once_per_output() {
        for (scale, pair, child, inactive, active) in [
            (1.0, [720, 480], [360, 240], 32, 80),
            (1.25, [900, 600], [450, 300], 40, 100),
            (1.5, [1080, 720], [540, 360], 48, 120),
            (2.0, [1440, 960], [720, 480], 64, 160),
        ] {
            let resize = ScaledResize::from_scale(scale).expect("scale is finite");
            assert_eq!(resize.pair_min, pair, "scale {scale}");
            assert_eq!(resize.child_min, child, "scale {scale}");
            assert_eq!(resize.group_edge_inactive, inactive, "scale {scale}");
            assert_eq!(resize.group_edge_active, active, "scale {scale}");
            assert_eq!(resize.pair_min_for_axis(Axis::Horizontal), pair[0]);
            assert_eq!(resize.pair_min_for_axis(Axis::Vertical), pair[1]);
            assert_eq!(resize.child_min_for_axis(Axis::Horizontal), child[0]);
            assert_eq!(resize.child_min_for_axis(Axis::Vertical), child[1]);
        }
    }

    #[test]
    fn scaled_pair_clamps_use_physical_minima() {
        let one = ScaledResize::from_scale(1.0).expect("scale");
        let one_and_quarter = ScaledResize::from_scale(1.25).expect("scale");
        // 1000px pair, desired 0: the child minimum clamps the first side.
        assert_eq!(
            one.clamp_pair_split(1000, 0, Axis::Horizontal),
            Some((360, 640))
        );
        assert_eq!(
            one_and_quarter.clamp_pair_split(1000, 0, Axis::Horizontal),
            Some((450, 550))
        );
        // Vertical scales independently (240 -> 300).
        assert_eq!(
            one_and_quarter.clamp_pair_split(1000, 0, Axis::Vertical),
            Some((300, 700))
        );
        // A pair below twice the scaled minimum is infeasible.
        assert_eq!(
            one_and_quarter.clamp_pair_split(800, 400, Axis::Horizontal),
            None
        );
        // Pair admission uses the scaled pair minimum.
        assert!(!one_and_quarter.pair_admits_resize(800, 800, Axis::Horizontal));
        assert!(one_and_quarter.pair_admits_resize(900, 100, Axis::Horizontal));
        // Keyboard shrink clamps one-sided at the scaled child minimum and
        // refuses a pair below the scaled pair minimum.
        assert_eq!(
            one_and_quarter.clamp_keyboard_shrink_pair(470, 500, 15, Axis::Horizontal),
            Some((455, 515))
        );
        assert_eq!(
            one_and_quarter.clamp_keyboard_shrink_pair(460, 500, 15, Axis::Horizontal),
            Some((450, 510))
        );
        assert_eq!(
            one_and_quarter.clamp_keyboard_shrink_pair(400, 400, 15, Axis::Horizontal),
            None
        );
    }

    #[test]
    fn scaled_group_edge_depths_classify_with_physical_zones() {
        let group = NodeId("g1".to_owned());
        let rect = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let one = ScaledResize::from_scale(1.0).expect("scale");
        let two = ScaledResize::from_scale(2.0).expect("scale");
        // 40px from the left edge: outside the 32px logical zone, inside
        // the 64px scaled zone.
        assert_eq!(one.classify_group_point(&rect, &group, 40, 200, None), None);
        assert_eq!(
            two.classify_group_point(&rect, &group, 40, 200, None),
            Some(DragSide::Left)
        );
        // Sticky expansion is scaled too: 80 -> 160.
        let prior = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Left,
        };
        assert_eq!(
            one.group_edge_depth_for(&group, DragSide::Left, Some(&prior)),
            80
        );
        assert_eq!(
            two.group_edge_depth_for(&group, DragSide::Left, Some(&prior)),
            160
        );
        assert_eq!(
            two.classify_group_point(&rect, &group, 140, 200, Some(&prior)),
            Some(DragSide::Left)
        );
        assert_eq!(
            one.classify_group_point(&rect, &group, 140, 200, Some(&prior)),
            None
        );
    }

    #[test]
    fn overflow_and_malformed_scales_are_refused_without_clamping() {
        // Non-finite, zero, negative, and overflowing scales scale nothing.
        for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0, 1e308] {
            assert_eq!(
                ScaledResize::from_scale(scale),
                None,
                "scale {scale} must not clamp"
            );
        }
        // Tiny scales scale down rather than fail (checked nearest rule).
        let tiny = ScaledResize::from_scale(0.001).expect("finite positive scale");
        assert_eq!(tiny.keyboard_steps, [0, 0, 0, 0, 0]);
        assert_eq!(tiny.pair_min, [1, 0]);
        assert_eq!(tiny.child_min, [0, 0]);
        assert_eq!(tiny.group_edge_inactive, 0);
        assert_eq!(tiny.group_edge_active, 0);
        // The zeroed placeholder authorizes nothing and is not the identity.
        assert_eq!(ScaledResize::zeroed().keyboard_steps, [0; 5]);
        assert_ne!(ScaledResize::zeroed(), ScaledResize::unscaled());
    }

    #[test]
    fn i64_scale_helper_ties_and_overflow() {
        use crate::world_engine::WorldEngine;
        assert_eq!(WorldEngine::scale_once_i64(8, 1.0), Some(8));
        assert_eq!(WorldEngine::scale_once_i64(8, 1.25), Some(10));
        assert_eq!(WorldEngine::scale_once_i64(8, 1.5), Some(12));
        assert_eq!(WorldEngine::scale_once_i64(1, 1.5), Some(2));
        assert_eq!(WorldEngine::scale_once_i64(0, 1.5), Some(0));
        assert_eq!(WorldEngine::scale_once_i64(i64::MAX, 2.0), None);
        assert_eq!(WorldEngine::scale_once_i64(8, 1e308), None);
    }
}
