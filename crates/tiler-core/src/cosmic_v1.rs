//! Explicit portable COSMIC v1 policy API.
//!
//! Versioned thin entry point over the R1-R4 planning policy in
//! [`crate::directional`], plus the named COSMIC v1 lifecycle/share/resize/drop
//! policy owned here. [`crate::session`] orchestrates portable transactions
//! and calls into this module for every COSMIC-specific semantic; no COSMIC
//! constant or choice lives unnamed in the session layer.
//!
//! Source evidence: all COSMIC behavior below is sourced from
//! `pop-os/cosmic-comp` revision `81cd5fdbaa41c3973369ae85bccf829137836e20`.
//! Where the portable ordered N-ary representation requires adaptation of a
//! binary/pixel source behavior, the adaptation is documented on the item.
//! Otherwise comments cite only the source symbol.
//!
//! Frozen scope: R1-R4 movement planning passes through to
//! [`crate::directional`] unchanged. The project-shortcut 1/16 share-step
//! resize (`RESIZE_STEP_DENOMINATOR`, `expected_resize_shares`,
//! `plan_resize_step`) is intentionally NOT re-exported here: COSMIC resize
//! operates on physical pixels through the named constants and helpers below.

/// Cosmic v1 API version (envelope identifier for this explicit surface).
pub const COSMIC_V1_VERSION: u32 = 1;

pub use crate::directional::{
    Capabilities, Capability, MoveIntent, MoveOutcome, MovePlan, Rule, Snapshot,
};

/// Accepted R1-R4 planning surface with full adapter capabilities.
#[must_use]
pub fn plan_move(snapshot: &Snapshot, intent: &MoveIntent) -> MoveOutcome {
    crate::directional::plan_move(snapshot, intent)
}

/// Accepted R1-R4 planning surface gated by adapter-declared capabilities.
#[must_use]
pub fn plan_move_with_capabilities(
    snapshot: &Snapshot,
    intent: &MoveIntent,
    capabilities: &Capabilities,
) -> MoveOutcome {
    crate::directional::plan_move_with_capabilities(snapshot, intent, capabilities)
}

use crate::contract::DragSide;
use crate::directional::{Axis, NodeId};
use crate::geometry::Rect;

// ---- admission (map_to_tree 548-617) ----

/// Admission split axis for a target geometry: width strictly greater than
/// height selects [`Axis::Horizontal`], otherwise [`Axis::Vertical`]
/// (vertical on tie). Source: `map_to_tree` automatic placement splits the
/// last active node with source `Orientation::Vertical` on wide targets
/// (`tiling/mod.rs` `map_to_tree` 584-605, `Data::new_group` 177-191 splits
/// width on `Vertical`); portable [`Axis::Horizontal`] splits width
/// (`geometry.rs`), so wide maps to portable `Horizontal` and tall/tie maps
/// to portable `Vertical`. The no-focus root case derives the axis from the
/// output geometry under the same rule. Frozen R1-R4 direction axes are
/// unchanged.
#[must_use]
pub const fn admission_axis(target_w: i32, target_h: i32) -> Axis {
    if target_w > target_h {
        Axis::Horizontal
    } else {
        Axis::Vertical
    }
}

/// Admission axis from portable rectangles (target or output geometry).
#[must_use]
pub fn admission_axis_for_rect(rect: &Rect) -> Axis {
    admission_axis(rect.w, rect.h)
}

// ---- groups and shares ----

/// New-group shares. Source: `Data::new_group` (177-191) creates an equal
/// pixel-halves binary split; adaptation: the portable ordered N-ary tree
/// represents the halves as integer shares `[1, 1]`.
#[must_use]
pub const fn new_group_shares() -> [u64; 2] {
    [1, 1]
}

/// Greatest common divisor for exact share normalization (Euclidean).
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Exact GCD of a non-empty share slice (0 when empty; callers reject empty).
fn gcd_all(shares: &[u64]) -> u64 {
    let mut out = 0u64;
    for s in shares {
        out = gcd(out, *s);
        if out == 1 {
            break;
        }
    }
    out
}

/// Proportional N-ary insertion shares preserving survivor ratios.
///
/// Source: `Data::add_window` (219-243) scales every survivor to `n/(n+1)` of
/// existing geometry and the new child takes `1/(n+1)`; adaptation: the
/// portable ordered N-ary tree operates exact positive `u64` shares with
/// `n = existing.len()`, `S = sum(existing)`, survivors `s_i * n` and entrant
/// `S` inserted at `insertion_index`. Ratios among survivors are preserved
/// exactly, even after uneven/pixel-resize shares (e.g. `[389, 409]` admits
/// `[389, 409, 399]` after GCD normalization). Overflow fails closed
/// (`None`); GCD normalization applies only as an exact division (always
/// exact when `GCD > 1`).
#[must_use]
pub fn proportional_insertion_shares(existing: &[u64], insertion_index: usize) -> Option<Vec<u64>> {
    if existing.is_empty() || insertion_index > existing.len() {
        return None;
    }
    if existing.contains(&0) {
        return None;
    }
    let n = existing.len() as u64;
    let mut sum: u64 = 0;
    for s in existing {
        sum = sum.checked_add(*s)?;
    }
    if sum == 0 {
        return None;
    }
    let mut scaled: Vec<u64> = Vec::with_capacity(existing.len() + 1);
    for s in existing {
        scaled.push(s.checked_mul(n)?);
    }
    // Insert entrant S at the ordered position (scaled survivors + entrant).
    let mut out: Vec<u64> = Vec::with_capacity(existing.len() + 1);
    for (i, v) in scaled.iter().enumerate() {
        if i == insertion_index {
            out.push(sum);
        }
        out.push(*v);
    }
    if insertion_index == scaled.len() {
        out.push(sum);
    }
    if out.len() != existing.len() + 1 || out.contains(&0) {
        return None;
    }
    let g = gcd_all(&out);
    if g > 1 {
        for v in out.iter_mut() {
            // Exact by GCD construction.
            *v /= g;
        }
    }
    Some(out)
}

/// Proportional N-ary removal shares preserving survivor ratios.
///
/// Source: `Data::remove_window` (255-283) redistributes the removed extent
/// proportionally with pixel rounding and a last-child correction;
/// adaptation: the portable ordered N-ary tree keeps survivor shares exactly
/// (dropping the removed slot redistributes its extent proportionally under
/// projection) with GCD normalization only as an exact division. E.g.
/// `[778, 818, 798]` minus index 2 normalizes to `[389, 409]`. Returns the
/// surviving shares (length `n-1`); callers collapse single-child groups
/// recursively after this step. `None` on malformed input.
#[must_use]
pub fn proportional_removal_shares(existing: &[u64], removed_index: usize) -> Option<Vec<u64>> {
    if existing.len() < 2 || removed_index >= existing.len() {
        return None;
    }
    if existing.contains(&0) {
        return None;
    }
    let mut out: Vec<u64> = Vec::with_capacity(existing.len() - 1);
    for (i, s) in existing.iter().enumerate() {
        if i != removed_index {
            out.push(*s);
        }
    }
    if out.is_empty() || out.contains(&0) {
        return None;
    }
    let g = gcd_all(&out);
    if g > 1 {
        for v in out.iter_mut() {
            *v /= g;
        }
    }
    Some(out)
}

// ---- keyboard/pointer pixel resize ----

/// Keyboard resize base previous value in physical pixels.
/// Source: `shell/mod.rs` (4558-4564) computes
/// `(previous.unwrap_or(10) + 2).min(20)`: the stored previous defaults to 10
/// and every operation adds 2, so the first operation moves 12px.
pub const COSMIC_KEYBOARD_PREVIOUS_PX: i32 = 10;
/// Keyboard resize per-operation increment in physical pixels.
/// Source: `shell/mod.rs` (4558-4564).
pub const COSMIC_KEYBOARD_STEP_INCREMENT_PX: i32 = 2;
/// Keyboard resize maximum step in physical pixels (cap).
/// Source: `shell/mod.rs` `resize` (4558-4564).
pub const COSMIC_KEYBOARD_MAX_STEP_PX: i32 = 20;
/// Resize pair minimum width in physical pixels: narrower pairs are a no-op.
/// Source: `grabs/resize.rs` pairs under 720 width no-op.
pub const COSMIC_PAIR_MIN_WIDTH_PX: i64 = 720;
/// Resize pair minimum height in physical pixels: shorter pairs are a no-op.
/// Source: `grabs/resize.rs` pairs under 480 height no-op.
pub const COSMIC_PAIR_MIN_HEIGHT_PX: i64 = 480;
/// Resized child minimum width in physical pixels, enforced two-sided on the
/// pointer path only (keyboard clamps one-sided; see below).
/// Source: `grabs/resize.rs` (300-317) child minima 360 width.
pub const COSMIC_CHILD_MIN_WIDTH_PX: i64 = 360;
/// Resized child minimum height in physical pixels, enforced two-sided on the
/// pointer path only (keyboard clamps one-sided; see below).
/// Source: `grabs/resize.rs` (300-317) child minima 240 height.
pub const COSMIC_CHILD_MIN_HEIGHT_PX: i64 = 240;

/// Keyboard step schedule: `(10 + 2 + 2 * press_index).min(20)`, i.e. 12, 14,
/// 16, 18, 20, capped at [`COSMIC_KEYBOARD_MAX_STEP_PX`].
/// Source: `shell/mod.rs` (4558-4564). `press_index` 0 is the initial press
/// (12px first operation).
#[must_use]
pub const fn keyboard_step_px(press_index: u32) -> i32 {
    // Total in u64 so every u32 press_index is representable
    // (max 10 + 2 + 2 * 4294967295 fits); the source `.min(20)` cap
    // applies afterwards. The previous i32 `press_index as i32` multiply
    // overflowed for large indices (wrong step, debug panic).
    let step = COSMIC_KEYBOARD_PREVIOUS_PX as u64
        + COSMIC_KEYBOARD_STEP_INCREMENT_PX as u64
        + COSMIC_KEYBOARD_STEP_INCREMENT_PX as u64 * press_index as u64;
    if step > COSMIC_KEYBOARD_MAX_STEP_PX as u64 {
        COSMIC_KEYBOARD_MAX_STEP_PX
    } else {
        step as i32
    }
}

/// Pair minimum along `axis` in physical pixels.
#[must_use]
pub const fn pair_min_for_axis(axis: Axis) -> i64 {
    match axis {
        Axis::Horizontal => COSMIC_PAIR_MIN_WIDTH_PX,
        Axis::Vertical => COSMIC_PAIR_MIN_HEIGHT_PX,
    }
}

/// Child minimum along `axis` in physical pixels.
#[must_use]
pub const fn child_min_for_axis(axis: Axis) -> i64 {
    match axis {
        Axis::Horizontal => COSMIC_CHILD_MIN_WIDTH_PX,
        Axis::Vertical => COSMIC_CHILD_MIN_HEIGHT_PX,
    }
}

/// Whether a resize pair with direct pair extents (`pair_w`, `pair_h` as
/// `sizes[i] + sizes[i + 1]`, never union geometry including gap) admits a
/// resize along `axis`. Pairs under the axis minimum are a source no-op on
/// both paths.
/// Source: `grabs/resize.rs` (291-298) and `tiling/mod.rs` (2582-2589)
/// threshold the direct pair sum (720 width / 480 height).
#[must_use]
pub const fn pair_admits_resize(pair_w: i64, pair_h: i64, axis: Axis) -> bool {
    match axis {
        Axis::Horizontal => pair_w >= COSMIC_PAIR_MIN_WIDTH_PX,
        Axis::Vertical => pair_h >= COSMIC_PAIR_MIN_HEIGHT_PX,
    }
}

/// Pointer two-sided clamped split of a `pair_total` physical extent
/// requesting `desired_first` for the first child along `axis`. Both children
/// keep at least the axis child minimum, with overflow on the second side
/// corrected back into the first; infeasible totals (`pair_total` below twice
/// the minimum) return `None` so callers refuse without planning.
/// Source: `grabs/resize.rs` (300-317) two-sided minimum correction. This is
/// the pointer path only; it must not be used to claim keyboard parity (the
/// keyboard path clamps one-sided, see [`clamp_keyboard_shrink_pair`]).
#[must_use]
pub fn clamp_pair_split(pair_total: i64, desired_first: i64, axis: Axis) -> Option<(i64, i64)> {
    clamp_pair_split_with(pair_total, desired_first, child_min_for_axis(axis))
}

/// Logical [`clamp_pair_split`] arithmetic parameterized by the physical
/// child minimum, so per-output scaled metrics
/// ([`crate::policy::ScaledResize`]) reuse the one source rule instead of
/// restating it. Same two-sided correction and infeasibility contract.
#[must_use]
pub fn clamp_pair_split_with(
    pair_total: i64,
    desired_first: i64,
    child_min: i64,
) -> Option<(i64, i64)> {
    if pair_total < child_min * 2 {
        return None;
    }
    let first = desired_first.clamp(child_min, pair_total - child_min);
    let second = pair_total - first;
    if first < child_min || second < child_min {
        return None;
    }
    Some((first, second))
}

/// Keyboard one-sided shrink step on a (`shrink`, `grow`) physical pair along
/// `axis` with a positive `amount` (see [`keyboard_step_px`]). Only the shrink
/// side is clamped to the axis child minimum; the grow side takes exactly the
/// amount actually removed (`grow + (shrink - new_shrink)`), even if it stays
/// below the minimum. Infeasible totals (`shrink + grow` below the axis pair
/// minimum) return `None` so callers refuse without planning.
/// Source: `tiling/mod.rs` (2576-2600) one-sided shrink clamp
/// (`(old - amount).max(min)` on the shrink side, grow takes the diff). This
/// is the keyboard path only; do not derive it from the pointer two-sided
/// correction above.
#[must_use]
pub fn clamp_keyboard_shrink_pair(
    shrink: i64,
    grow: i64,
    amount: i64,
    axis: Axis,
) -> Option<(i64, i64)> {
    clamp_keyboard_shrink_pair_with(
        shrink,
        grow,
        amount,
        pair_min_for_axis(axis),
        child_min_for_axis(axis),
    )
}

/// Logical [`clamp_keyboard_shrink_pair`] arithmetic parameterized by the
/// physical pair and child minima, so per-output scaled metrics
/// ([`crate::policy::ScaledResize`]) reuse the one source rule.
#[must_use]
pub fn clamp_keyboard_shrink_pair_with(
    shrink: i64,
    grow: i64,
    amount: i64,
    pair_min: i64,
    child_min: i64,
) -> Option<(i64, i64)> {
    let pair_total = shrink.checked_add(grow)?;
    if pair_total < pair_min {
        return None;
    }
    let new_shrink = (shrink - amount).max(child_min);
    let removed = shrink - new_shrink;
    let new_grow = grow.checked_add(removed)?;
    Some((new_shrink, new_grow))
}

// ---- drag zones (update_pointer_position 3373-3883, drop_window 2666-2824) ----

/// Group edge zone depth in physical pixels for edges other than the exact
/// prior-hover edge. Source: `update_pointer_position` (3512-3520, 3534-3543,
/// 3559-3570, 3587-3598) builds every non-matching edge at 32px.
pub const COSMIC_GROUP_EDGE_INACTIVE_PX: i32 = 32;
/// Group edge zone depth in physical pixels for exactly the prior-hover
/// group+edge (sticky expansion). Source: `update_pointer_position`
/// (3500-3511, 3522-3533, 3544-3558, 3572-3586) builds the edge matching
/// `last_overview_hover == GroupEdge(res_id, direction)` at 80px.
pub const COSMIC_GROUP_EDGE_ACTIVE_PX: i32 = 80;

/// Smallest portable prior-hover identity for sticky group edges: the portable
/// group id plus the edge direction of the previous `GroupEdge` hover. No
/// native event, pointer, or trigger state is carried; callers pass the last
/// emitted group-edge identity (or `None` when the last hover was not a group
/// edge). A `Center` edge never matches a group edge zone, so it behaves as
/// non-sticky for every edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorGroupEdge {
    pub group: NodeId,
    pub edge: DragSide,
}

/// Per-edge zone depth for `group`/`edge` given the portable prior hover.
/// Exactly the same group id plus the same edge direction yields the sticky
/// 80px depth; every other edge (different group, different edge, no prior,
/// or `Center` prior) yields the normal 32px depth.
/// Source: `update_pointer_position` (3500-3599) matches each edge
/// independently against `last_overview_hover`.
#[must_use]
pub fn group_edge_depth_for(group: &NodeId, edge: DragSide, prior: Option<&PriorGroupEdge>) -> i32 {
    group_edge_depth_with(
        group,
        edge,
        prior,
        COSMIC_GROUP_EDGE_INACTIVE_PX,
        COSMIC_GROUP_EDGE_ACTIVE_PX,
    )
}

/// Logical [`group_edge_depth_for`] arithmetic parameterized by the physical
/// inactive/active depths, so per-output scaled metrics
/// ([`crate::policy::ScaledResize`]) reuse the one source rule.
#[must_use]
pub fn group_edge_depth_with(
    group: &NodeId,
    edge: DragSide,
    prior: Option<&PriorGroupEdge>,
    inactive: i32,
    active: i32,
) -> i32 {
    match prior {
        Some(p) if p.group == *group && p.edge == edge => active,
        _ => inactive,
    }
}

/// Group-edge classification of `(x, y)` inside a projected group `rect` for
/// portable group `group` given the portable prior hover `prior`.
///
/// Each edge depth resolves independently through [`group_edge_depth_for`]:
/// exactly the prior group+edge is sticky 80px, every other edge is normal
/// 32px. Source geometry (`update_pointer_position` 3500-3609): left is built
/// then cropped, top is built from the remainder then cropped, right is built
/// from the remainder then cropped, bottom comes from the remaining
/// rectangle. Hit checking order is left, right, top, bottom, so corners
/// overlap asymmetrically (e.g. a top-right point in the top strip is `Top`,
/// not `Right`, because the right zone starts below the top strip). Points
/// outside every zone are group interior (`None`: the caller resolves the
/// predecessor index via [`insertion_index_for_offset`]). Integer math only;
/// zero/negative extents return `None`. Upstream `Direction::Up`/`Down` map to
/// portable [`DragSide::Top`]/[`DragSide::Bottom`].
#[must_use]
pub fn classify_group_point(
    rect: &Rect,
    group: &NodeId,
    x: i32,
    y: i32,
    prior: Option<&PriorGroupEdge>,
) -> Option<DragSide> {
    classify_group_point_with(
        rect,
        group,
        x,
        y,
        prior,
        COSMIC_GROUP_EDGE_INACTIVE_PX,
        COSMIC_GROUP_EDGE_ACTIVE_PX,
    )
}

/// Logical [`classify_group_point`] arithmetic parameterized by the physical
/// edge depths, so per-output scaled metrics
/// ([`crate::policy::ScaledResize`]) reuse the one source rule.
#[must_use]
pub fn classify_group_point_with(
    rect: &Rect,
    group: &NodeId,
    x: i32,
    y: i32,
    prior: Option<&PriorGroupEdge>,
    inactive: i32,
    active: i32,
) -> Option<DragSide> {
    if rect.w <= 0 || rect.h <= 0 {
        return None;
    }
    let depth = |edge: DragSide| group_edge_depth_with(group, edge, prior, inactive, active);
    let dx = x.checked_sub(rect.x)?;
    let dy = y.checked_sub(rect.y)?;
    if dx < 0 || dy < 0 || dx >= rect.w || dy >= rect.h {
        return None;
    }
    let left_depth = depth(DragSide::Left).min(rect.w);
    let top_depth = depth(DragSide::Top).min(rect.h);
    // Build order: left full-height strip, then top of the remainder, then
    // right of the remainder, then bottom of the remainder.
    if dx < left_depth {
        return Some(DragSide::Left);
    }
    let w1 = rect.w - left_depth;
    if w1 <= 0 {
        return None;
    }
    // Right zone lives in R2 (remainder after left+top): y from top_depth.
    let h2 = rect.h - top_depth;
    if h2 > 0 && w1 > 0 {
        let right_depth = depth(DragSide::Right).min(w1);
        let right_start = rect.w - right_depth;
        if dx >= right_start && dy >= top_depth {
            return Some(DragSide::Right);
        }
    }
    // Top zone: remainder after left, top strip.
    if dx >= left_depth && dy < top_depth {
        return Some(DragSide::Top);
    }
    // Bottom zone: remainder after left+top+right, bottom strip.
    if h2 <= 0 {
        return None;
    }
    let right_depth = depth(DragSide::Right).min(w1);
    let w3 = w1 - right_depth;
    if w3 <= 0 {
        return None;
    }
    let bottom_depth = depth(DragSide::Bottom).min(h2);
    let bottom_start = rect.h - bottom_depth;
    if dx >= left_depth && dx < rect.w - right_depth && dy >= bottom_start {
        return Some(DragSide::Bottom);
    }
    None
}

/// Window-drop classification of `(x, y)` inside a projected window `rect`.
/// The middle third on both axes (rounded thirds: `(extent / 3.0).round()`,
/// integer `(extent + 1) / 3`) is the COSMIC center stack source fact
/// (`DragSide::Center`); any other location selects an edge by minimum
/// normalized half-distance with horizontal winning only on strict less
/// (`h_min * h < v_min * w`, ties choose vertical). Source:
/// `tiling/mod.rs` 3646-3689 uses rounded thirds, stack rectangle extremes,
/// then horizontal only if strictly less. Integer-friendly via
/// cross-multiplication. Points outside the rect return `None`.
#[must_use]
pub fn classify_window_point(rect: &Rect, x: i32, y: i32) -> Option<DragSide> {
    if rect.w <= 0 || rect.h <= 0 {
        return None;
    }
    let dx = x.checked_sub(rect.x)?;
    let dy = y.checked_sub(rect.y)?;
    if dx < 0 || dy < 0 || dx >= rect.w || dy >= rect.h {
        return None;
    }
    // Rounded thirds: round(w / 3.0) == (w + 1) / 3 for w >= 0.
    let third_w = (rect.w + 1) / 3;
    let third_h = (rect.h + 1) / 3;
    let in_middle_x = dx >= third_w && dx < rect.w - third_w;
    let in_middle_y = dy >= third_h && dy < rect.h - third_h;
    if in_middle_x && in_middle_y {
        return Some(DragSide::Center);
    }
    let dx64 = i64::from(dx);
    let dy64 = i64::from(dy);
    let w = i64::from(rect.w);
    let h = i64::from(rect.h);
    let left_d = dx64;
    let right_d = w - dx64;
    let top_d = dy64;
    let bottom_d = h - dy64;
    let h_min = left_d.min(right_d);
    let v_min = top_d.min(bottom_d);
    if h_min * h < v_min * w {
        if left_d <= right_d {
            Some(DragSide::Left)
        } else {
            Some(DragSide::Right)
        }
    } else if top_d <= bottom_d {
        Some(DragSide::Top)
    } else {
        Some(DragSide::Bottom)
    }
}

/// Predecessor index for a group-interior drop at `offset` (absolute
/// coordinate along the group axis) given ordered child start edges
/// `child_starts` (absolute, ascending, one per child).
///
/// Source: `tiling/mod.rs` (3610-3624) takes the position of the first child
/// whose start exceeds the pointer (`location.x < geo.loc.x` on
/// `Orientation::Vertical`, `location.y < geo.loc.y` on `Horizontal`), minus
/// one, defaulting to 0 when the position is 0 or absent. The drop path then
/// inserts at `min(len, idx + 1)` (`drop_window` 2725-2730), so this portable
/// predecessor plus the caller's `+1` bound reproduces the source slot.
/// Adaptation: the portable ordered N-ary tree generalizes the source binary
/// predecessor to N-ary child order.
#[must_use]
pub fn insertion_index_for_offset(child_starts: &[i64], offset: i64) -> usize {
    child_starts
        .iter()
        .position(|start| offset < *start)
        .and_then(|pos| pos.checked_sub(1))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::{
        Direction, Node, NodeId, Output, OutputId, WindowId, WindowLink, WorkspaceId,
    };
    use std::collections::BTreeMap;

    #[test]
    fn cosmic_v1_version_is_current() {
        assert_eq!(COSMIC_V1_VERSION, 1);
    }

    #[test]
    fn r2a_conformance_plan_through_v1() {
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Horizontal,
            children: vec![
                Node::Leaf {
                    id: NodeId::from("A"),
                },
                Node::Leaf {
                    id: NodeId::from("B"),
                },
            ],
            shares: vec![1, 1],
        };
        let snapshot = Snapshot {
            outputs: vec![Output {
                id: OutputId::from("source"),
                workspace: WorkspaceId::from("workspace-1"),
                tree: Some(tree),
                adjacent: BTreeMap::new(),
            }],
            windows: vec![
                WindowLink {
                    window: WindowId::from("w-A"),
                    leaf: NodeId::from("A"),
                    output: OutputId::from("source"),
                    workspace: WorkspaceId::from("workspace-1"),
                },
                WindowLink {
                    window: WindowId::from("w-B"),
                    leaf: NodeId::from("B"),
                    output: OutputId::from("source"),
                    workspace: WorkspaceId::from("workspace-1"),
                },
            ],
        };
        let intent = MoveIntent {
            source_output: OutputId::from("source"),
            focused_leaf: NodeId::from("A"),
            focused_window: WindowId::from("w-A"),
            direction: Direction::Right,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
        };
        match plan_move(&snapshot, &intent) {
            MoveOutcome::Planned(plan) => {
                assert_eq!(plan.rule, Rule::R2a);
            }
            other => panic!("expected R2a plan through cosmic_v1, got {other:?}"),
        }
    }

    #[test]
    fn admission_axis_maps_wide_to_horizontal() {
        // map_to_tree wide => source Vertical splits width => portable
        // Horizontal splits width (geometry.rs); tall/tie => portable Vertical.
        assert_eq!(admission_axis(100, 50), Axis::Horizontal);
        assert_eq!(admission_axis(50, 100), Axis::Vertical);
        assert_eq!(admission_axis(100, 100), Axis::Vertical);
        assert_eq!(
            admission_axis_for_rect(&Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            }),
            Axis::Horizontal
        );
        assert_eq!(
            admission_axis_for_rect(&Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120
            }),
            Axis::Vertical
        );
    }

    #[test]
    fn new_group_and_proportional_shares_are_named() {
        assert_eq!(new_group_shares(), [1, 1]);
        // N-ary proportional insertion: survivors * n, entrant S, GCD exact.
        assert_eq!(
            proportional_insertion_shares(&[1, 1], 2),
            Some(vec![1, 1, 1])
        );
        assert_eq!(
            proportional_insertion_shares(&[389, 409], 2),
            Some(vec![389, 409, 399])
        );
        assert_eq!(
            proportional_insertion_shares(&[389, 409], 0),
            Some(vec![399, 389, 409])
        );
        // Uneven ratios preserved: [3, 1] (n=2, S=4) => [6, 2, 4] => GCD 2 => [3, 1, 2].
        assert_eq!(
            proportional_insertion_shares(&[3, 1], 2),
            Some(vec![3, 1, 2])
        );
        assert_eq!(proportional_insertion_shares(&[], 0), None);
        assert_eq!(proportional_insertion_shares(&[1, 0], 1), None);
        assert_eq!(proportional_insertion_shares(&[u64::MAX, 1], 2), None);
        // Removal: drop slot, GCD exact.
        assert_eq!(
            proportional_removal_shares(&[389, 409, 399], 2),
            Some(vec![389, 409])
        );
        assert_eq!(
            proportional_removal_shares(&[778, 818, 798], 2),
            Some(vec![389, 409])
        );
        assert_eq!(proportional_removal_shares(&[1, 1], 0), Some(vec![1]));
        assert_eq!(proportional_removal_shares(&[1], 0), None);
        assert_eq!(proportional_removal_shares(&[1, 1], 2), None);
    }

    #[test]
    fn keyboard_step_schedule_first_is_12_then_plus_2_capped_20() {
        assert_eq!(keyboard_step_px(0), 12);
        assert_eq!(keyboard_step_px(1), 14);
        assert_eq!(keyboard_step_px(2), 16);
        assert_eq!(keyboard_step_px(3), 18);
        assert_eq!(keyboard_step_px(4), 20);
        assert_eq!(keyboard_step_px(5), 20);
        assert_eq!(keyboard_step_px(100), 20);
        assert_eq!(COSMIC_KEYBOARD_PREVIOUS_PX, 10);
        assert_eq!(COSMIC_KEYBOARD_STEP_INCREMENT_PX, 2);
        assert_eq!(COSMIC_KEYBOARD_MAX_STEP_PX, 20);
    }

    #[test]
    fn keyboard_step_px_caps_for_all_u32_without_overflow() {
        // The `(10 + 2 + 2 * press_index).min(20)` schedule must hold
        // for every u32 without overflow.
        assert_eq!(keyboard_step_px(1_073_741_823), 20);
        assert_eq!(keyboard_step_px(1_073_741_824), 20);
        assert_eq!(keyboard_step_px(2_147_483_647), 20);
        assert_eq!(keyboard_step_px(2_147_483_648), 20);
        assert_eq!(keyboard_step_px(u32::MAX), 20);
    }

    #[test]
    fn pair_minima_refuse_small_pairs() {
        assert!(!pair_admits_resize(719, 800, Axis::Horizontal));
        assert!(pair_admits_resize(720, 100, Axis::Horizontal));
        assert!(!pair_admits_resize(800, 479, Axis::Vertical));
        assert!(pair_admits_resize(100, 480, Axis::Vertical));
        assert_eq!(pair_min_for_axis(Axis::Horizontal), 720);
        assert_eq!(pair_min_for_axis(Axis::Vertical), 480);
        assert_eq!(child_min_for_axis(Axis::Horizontal), 360);
        assert_eq!(child_min_for_axis(Axis::Vertical), 240);
    }

    #[test]
    fn clamp_enforces_two_sided_minima_pointer_only() {
        // Pointer path: resize.rs 300-317 corrects both sides.
        assert_eq!(clamp_pair_split(719, 360, Axis::Horizontal), None);
        assert_eq!(
            clamp_pair_split(720, 10, Axis::Horizontal),
            Some((360, 360))
        );
        assert_eq!(
            clamp_pair_split(720, 710, Axis::Horizontal),
            Some((360, 360))
        );
        assert_eq!(
            clamp_pair_split(800, 500, Axis::Horizontal),
            Some((440, 360))
        );
        assert_eq!(
            clamp_pair_split(800, 400, Axis::Horizontal),
            Some((400, 400))
        );
        assert_eq!(clamp_pair_split(479, 240, Axis::Vertical), None);
        assert_eq!(clamp_pair_split(480, 0, Axis::Vertical), Some((240, 240)));
    }

    #[test]
    fn keyboard_shrink_clamps_one_sided() {
        // Keyboard path: tiling/mod.rs 2591-2599 clamps only the shrink side.
        assert_eq!(
            clamp_keyboard_shrink_pair(500, 500, 12, Axis::Horizontal),
            Some((488, 512))
        );
        // Shrink clamped at 360; grow takes only the 5px actually removed.
        assert_eq!(
            clamp_keyboard_shrink_pair(365, 500, 12, Axis::Horizontal),
            Some((360, 505))
        );
        // Shrink already at minimum: no movement.
        assert_eq!(
            clamp_keyboard_shrink_pair(360, 500, 12, Axis::Horizontal),
            Some((360, 500))
        );
        // One-sided distinction: grow stays below minimum (232) instead of
        // being corrected two-sided (pointer would force 360/360 here).
        assert_eq!(
            clamp_keyboard_shrink_pair(500, 220, 12, Axis::Horizontal),
            Some((488, 232))
        );
        // Infeasible pair total refuses.
        assert_eq!(
            clamp_keyboard_shrink_pair(360, 359, 12, Axis::Horizontal),
            None
        );
        assert_eq!(
            clamp_keyboard_shrink_pair(240, 239, 12, Axis::Vertical),
            None
        );
        assert_eq!(
            clamp_keyboard_shrink_pair(300, 300, 12, Axis::Vertical),
            Some((288, 312))
        );
    }

    #[test]
    fn group_zones_follow_exact_build_and_priority() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let group = NodeId::from("g");
        // No prior: every edge is the normal 32px.
        assert_eq!(group_edge_depth_for(&group, DragSide::Left, None), 32);
        assert_eq!(group_edge_depth_for(&group, DragSide::Right, None), 32);
        assert_eq!(
            classify_group_point(&rect, &group, 5, 200, None),
            Some(DragSide::Left)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 395, 200, None),
            Some(DragSide::Right)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 200, 5, None),
            Some(DragSide::Top)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 200, 395, None),
            Some(DragSide::Bottom)
        );
        assert_eq!(classify_group_point(&rect, &group, 200, 200, None), None);
        // Left wins top-left; top-right collision is Top (right starts below
        // the top strip).
        assert_eq!(
            classify_group_point(&rect, &group, 5, 5, None),
            Some(DragSide::Left)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 395, 5, None),
            Some(DragSide::Top)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 395, 40, None),
            Some(DragSide::Right)
        );
        assert_eq!(classify_group_point(&rect, &group, 500, 200, None), None);
    }

    #[test]
    fn group_sticky_applies_only_to_same_group_edge() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let group = NodeId::from("g");
        let other = NodeId::from("other");
        let prior_left = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Left,
        };
        // Same group+edge is sticky 80; all other edges stay 32.
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Left, Some(&prior_left)),
            80
        );
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Right, Some(&prior_left)),
            32
        );
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Top, Some(&prior_left)),
            32
        );
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Bottom, Some(&prior_left)),
            32
        );
        // Sticky reach: 79px inside the left strip hits only with the match.
        assert_eq!(
            classify_group_point(&rect, &group, 79, 200, Some(&prior_left)),
            Some(DragSide::Left)
        );
        assert_eq!(classify_group_point(&rect, &group, 79, 200, None), None);
        // Stale edge on the same group is non-sticky.
        let prior_right = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Right,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 50, 200, Some(&prior_right)),
            None
        );
        // Same edge on a different group is non-sticky.
        let prior_other = PriorGroupEdge {
            group: other.clone(),
            edge: DragSide::Left,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 50, 200, Some(&prior_other)),
            None
        );
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Left, Some(&prior_other)),
            32
        );
        // Center prior never sticks any edge.
        let prior_center = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Center,
        };
        assert_eq!(
            group_edge_depth_for(&group, DragSide::Left, Some(&prior_center)),
            32
        );
        assert_eq!(
            classify_group_point(&rect, &group, 50, 200, Some(&prior_center)),
            None
        );
    }

    #[test]
    fn group_sticky_transitions_and_corners() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 400,
        };
        let group = NodeId::from("g");
        // Hover can leave the sticky edge: with prior Left, a point in the
        // normal right zone still reports Right.
        let prior_left = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Left,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 395, 200, Some(&prior_left)),
            Some(DragSide::Right)
        );
        // Sticky top (80) still loses top-left to Left and wins top-right
        // over the normal right zone via hit order left, right, top, bottom.
        let prior_top = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Top,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 5, 5, Some(&prior_top)),
            Some(DragSide::Left)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 390, 10, Some(&prior_top)),
            Some(DragSide::Top)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 390, 200, Some(&prior_top)),
            Some(DragSide::Right)
        );
        // Sticky right does not pull the top strip: (390, 10) stays Top.
        let prior_right = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Right,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 390, 10, Some(&prior_right)),
            Some(DragSide::Top)
        );
        assert_eq!(
            classify_group_point(&rect, &group, 390, 40, Some(&prior_right)),
            Some(DragSide::Right)
        );
        // Sticky bottom reach with corners retained.
        let prior_bottom = PriorGroupEdge {
            group: group.clone(),
            edge: DragSide::Bottom,
        };
        assert_eq!(
            classify_group_point(&rect, &group, 200, 330, Some(&prior_bottom)),
            Some(DragSide::Bottom)
        );
        assert_eq!(classify_group_point(&rect, &group, 200, 330, None), None);
    }

    #[test]
    fn window_zones_use_rounded_thirds_and_strict_tie_vertical() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        assert_eq!(classify_window_point(&rect, 60, 45), Some(DragSide::Center));
        assert_eq!(classify_window_point(&rect, 5, 45), Some(DragSide::Left));
        assert_eq!(classify_window_point(&rect, 115, 45), Some(DragSide::Right));
        assert_eq!(classify_window_point(&rect, 60, 4), Some(DragSide::Top));
        assert_eq!(classify_window_point(&rect, 60, 86), Some(DragSide::Bottom));
        assert_eq!(classify_window_point(&rect, 200, 45), None);
        // Rounded thirds: w=122 => third 41, middle x [41, 81).
        let wide = Rect {
            x: 0,
            y: 0,
            w: 122,
            h: 90,
        };
        assert_eq!(classify_window_point(&wide, 40, 45), Some(DragSide::Left));
        // Axis tie chooses vertical: 120x60 point (10,5) has
        // h_min*h == v_min*w (10*60 == 5*120) => Top.
        let tie = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 60,
        };
        assert_eq!(classify_window_point(&tie, 10, 5), Some(DragSide::Top));
        // Corner tie in 120x60 at (10,50): h_min=10, v_min=10 => 600 vs 1200?
        // 10*60=600 < 10*120=1200 => horizontal Left.
        assert_eq!(classify_window_point(&tie, 10, 50), Some(DragSide::Left));
    }

    #[test]
    fn insertion_index_matches_source_predecessor() {
        // Source tiling/mod.rs 3610-3623: position of first start exceeding
        // the offset, minus one, else 0. Callers bound with min(len, idx + 1)
        // (drop_window 2725-2730).
        assert_eq!(insertion_index_for_offset(&[0, 40, 80], -5), 0);
        assert_eq!(insertion_index_for_offset(&[0, 40, 80], 10), 0);
        assert_eq!(insertion_index_for_offset(&[0, 40, 80], 50), 1);
        assert_eq!(insertion_index_for_offset(&[0, 40, 80], 90), 0);
        assert_eq!(insertion_index_for_offset(&[], 10), 0);
        // Drop-slot bound: predecessor + 1 clamped to len.
        let idx = insertion_index_for_offset(&[0, 40, 80], 50);
        assert_eq!(3.min(idx + 1), 2);
    }
}
