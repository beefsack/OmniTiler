//! Cursor-first output targeting (2026-10-20 decision).
//!
//! Shared policy for workspace-switch targeting and fresh-window placement
//! over adapter-normalized integer geometry. The adapter reports the cursor
//! position with each complete [`crate::world_report::WorldReport`]; core
//! resolves every target from that plus retained focused facts, never from
//! an adapter-preselected output.
//!
//! - [`SwitchTarget`]: which output numbered/relative/previous/trailing
//!   selections act on. Cursor output by default (COSMIC), focused output
//!   as the i3/sway option.
//! - [`FreshPlacement`]: which output a truly new lifetime admits into and
//!   which anchor it splits. COSMIC (default: cursor output, split that
//!   output's last-focused window), Hyprland (cursor output, split the
//!   cursor-closest node per `DwindleAlgorithm::getClosestNode` at pin
//!   `19fb395d`), or i3/sway (focused output, split its last-focused
//!   window). Sends keep their explicit contracts and are unaffected.
//! - Focus-follows-cursor and cursor-warp are the retained behavior
//!   settings behind the stage-4b coupling policy in
//!   [`crate::world_cursor`]; the enums here carry targeting only.

use crate::directional::{NodeId, OutputId};
use crate::geometry::Rect;

/// Workspace-switch target: cursor output (default, COSMIC) or the focused
/// window's output (i3/sway option).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SwitchTarget {
    /// Resolve selections on the output under the cursor (default).
    #[default]
    Cursor,
    /// Resolve selections on the focused window's output.
    Focused,
}

impl SwitchTarget {
    /// Wire token for this value (`cursor-output` / `focused-output`).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Cursor => "cursor-output",
            Self::Focused => "focused-output",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers refuse fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "cursor-output" => Some(Self::Cursor),
            "focused-output" => Some(Self::Focused),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

/// Fresh-window placement: which output a truly new lifetime admits into
/// and which anchor it splits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FreshPlacement {
    /// Cursor output, split that output's last-focused window (default,
    /// COSMIC).
    #[default]
    CursorOutputMru,
    /// Cursor output, split the cursor-closest node (Hyprland).
    CursorOutputCursorClosest,
    /// Focused output, split its last-focused window (i3/sway).
    FocusedOutputMru,
}

impl FreshPlacement {
    /// Wire token for this value.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::CursorOutputMru => "cursor-output-last-focused",
            Self::CursorOutputCursorClosest => "cursor-output-cursor-closest",
            Self::FocusedOutputMru => "focused-output-last-focused",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers refuse fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "cursor-output-last-focused" => Some(Self::CursorOutputMru),
            "cursor-output-cursor-closest" => Some(Self::CursorOutputCursorClosest),
            "focused-output-last-focused" => Some(Self::FocusedOutputMru),
            _ => None,
        }
    }

    /// Normalized mode for an optional raw string: missing (`None`) or any
    /// invalid token falls back to the default.
    #[must_use]
    pub fn normalize_wire(value: Option<&str>) -> Self {
        value.and_then(Self::parse_wire).unwrap_or_default()
    }
}

/// Output whose FULL rectangle contains `cursor` (half-open `[start, end)`
/// on both axes, matching the shared centre-containment rule). First match
/// in slice order wins; callers pass screens primary-first so overlap ties
/// stay deterministic.
#[must_use]
pub fn output_at_cursor(screens: &[(OutputId, Rect)], cursor: (i32, i32)) -> Option<OutputId> {
    let (x, y) = (i64::from(cursor.0), i64::from(cursor.1));
    screens
        .iter()
        .find(|(_, rect)| {
            let end_x = i64::from(rect.x) + i64::from(rect.w);
            let end_y = i64::from(rect.y) + i64::from(rect.h);
            x >= i64::from(rect.x) && x < end_x && y >= i64::from(rect.y) && y < end_y
        })
        .map(|(output, _)| output.clone())
}

/// Resolve one logical output from cursor/focused facts plus a primary-first
/// output fallback: the cursor output when `use_cursor` and known, else the
/// focused output, else the first fallback (primary, then first readable).
/// `None` only when every source is unknown: callers refuse without
/// inventing focus.
fn resolve_output(
    use_cursor: bool,
    cursor_output: Option<&OutputId>,
    focused_output: Option<&OutputId>,
    primary_first: &[OutputId],
) -> Option<OutputId> {
    if use_cursor && let Some(cursor) = cursor_output {
        return Some(cursor.clone());
    }
    if let Some(focused) = focused_output {
        return Some(focused.clone());
    }
    primary_first.first().cloned()
}

/// Resolve the migration/control action output from neutral adapter facts:
/// the genuine foreground output when known, else the pointer (cursor)
/// output when known, else the adapter-cached output when known, else the
/// first ordered registry output. Every leg filters to the ordered known
/// registry (which retains mapped stale outputs, exactly like the retired
/// platform selector); a stale mapped leg selects and refuses downstream
/// instead of silently retargeting. `None` only when every source is
/// unknown: callers refuse without inventing focus. This is the sole
/// policy owner for migration/control source selection; adapters capture
/// the neutral facts in the report and never preselect.
#[must_use]
pub fn resolve_action_output(
    foreground: Option<&OutputId>,
    cursor: Option<&OutputId>,
    cached: Option<&OutputId>,
    ordered: &[OutputId],
) -> Option<OutputId> {
    if let Some(output) = foreground
        && ordered.iter().any(|live| live == output)
    {
        return Some(output.clone());
    }
    if let Some(output) = cursor
        && ordered.iter().any(|live| live == output)
    {
        return Some(output.clone());
    }
    if let Some(output) = cached
        && ordered.iter().any(|live| live == output)
    {
        return Some(output.clone());
    }
    ordered.first().cloned()
}

/// Resolve the switch target output for numbered/relative/previous/trailing
/// selections: cursor output under [`SwitchTarget::Cursor`], focused output
/// under [`SwitchTarget::Focused`], then primary/first fallback.
#[must_use]
pub fn resolve_switch_output(
    target: SwitchTarget,
    cursor_output: Option<&OutputId>,
    focused_output: Option<&OutputId>,
    primary_first: &[OutputId],
) -> Option<OutputId> {
    resolve_output(
        matches!(target, SwitchTarget::Cursor),
        cursor_output,
        focused_output,
        primary_first,
    )
}

/// Resolve the fresh-placement output for a truly new lifetime: cursor
/// output under the cursor-first placements, focused output under the
/// i3/sway placement, then primary/first fallback. Never invents focus.
#[must_use]
pub fn resolve_fresh_output(
    placement: FreshPlacement,
    cursor_output: Option<&OutputId>,
    focused_output: Option<&OutputId>,
    primary_first: &[OutputId],
) -> Option<OutputId> {
    resolve_output(
        !matches!(placement, FreshPlacement::FocusedOutputMru),
        cursor_output,
        focused_output,
        primary_first,
    )
}

/// Squared point-to-rect distance following the pinned Hyprland
/// `vecToRectDistanceSquared` (`src/helpers/MiscFunctions.cpp` at
/// `19fb395d`): zero when the point is inside (edges inclusive there),
/// growing with the outside axis gaps. `i128` keeps the full `i32`
/// coordinate range exact: axis gaps reach ~2^32 and their squares would
/// overflow `i64`.
fn point_rect_distance_squared(cursor: (i32, i32), rect: &Rect) -> i128 {
    let (x, y) = (i128::from(cursor.0), i128::from(cursor.1));
    let end_x = i128::from(rect.x) + i128::from(rect.w);
    let end_y = i128::from(rect.y) + i128::from(rect.h);
    let dx = (i128::from(rect.x) - x).max(0).max(x - end_x);
    let dy = (i128::from(rect.y) - y).max(0).max(y - end_y);
    dx * dx + dy * dy
}

/// Cursor-closest leaf following the pinned Hyprland `getClosestNode`:
/// minimum squared point-to-rect distance over the given leaves in slice
/// order (callers pass child-order projection, so ties resolve to the
/// first layout leaf), strictly-less wins. `None` for an empty set.
#[must_use]
pub fn cursor_closest_leaf(leaves: &[(NodeId, Rect)], cursor: (i32, i32)) -> Option<NodeId> {
    let mut best: Option<(&NodeId, i128)> = None;
    for (leaf, rect) in leaves {
        let dist = point_rect_distance_squared(cursor, rect);
        if best.is_none_or(|(_, known)| dist < known) {
            best = Some((leaf, dist));
        }
    }
    best.map(|(leaf, _)| leaf.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(id: &str) -> OutputId {
        OutputId(id.to_owned())
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn switch_wire_roundtrips_with_cursor_default() {
        assert_eq!(SwitchTarget::default(), SwitchTarget::Cursor);
        assert_eq!(SwitchTarget::Cursor.as_wire_str(), "cursor-output");
        assert_eq!(SwitchTarget::Focused.as_wire_str(), "focused-output");
        assert_eq!(
            SwitchTarget::parse_wire("cursor-output"),
            Some(SwitchTarget::Cursor)
        );
        assert_eq!(
            SwitchTarget::parse_wire("focused-output"),
            Some(SwitchTarget::Focused)
        );
        for invalid in ["", "cursor", "focused", "cursor_output", "CURSOR-OUTPUT"] {
            assert_eq!(SwitchTarget::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(SwitchTarget::normalize_wire(None), SwitchTarget::Cursor);
        assert_eq!(
            SwitchTarget::normalize_wire(Some("bogus")),
            SwitchTarget::Cursor
        );
    }

    #[test]
    fn placement_wire_roundtrips_with_cosmic_default() {
        assert_eq!(FreshPlacement::default(), FreshPlacement::CursorOutputMru);
        assert_eq!(
            FreshPlacement::CursorOutputMru.as_wire_str(),
            "cursor-output-last-focused"
        );
        assert_eq!(
            FreshPlacement::CursorOutputCursorClosest.as_wire_str(),
            "cursor-output-cursor-closest"
        );
        assert_eq!(
            FreshPlacement::FocusedOutputMru.as_wire_str(),
            "focused-output-last-focused"
        );
        assert_eq!(
            FreshPlacement::parse_wire("cursor-output-last-focused"),
            Some(FreshPlacement::CursorOutputMru)
        );
        assert_eq!(
            FreshPlacement::parse_wire("cursor-output-cursor-closest"),
            Some(FreshPlacement::CursorOutputCursorClosest)
        );
        assert_eq!(
            FreshPlacement::parse_wire("focused-output-last-focused"),
            Some(FreshPlacement::FocusedOutputMru)
        );
        for invalid in ["", "cursor", "mru", "hyprland", "cosmic", "i3-sway"] {
            assert_eq!(FreshPlacement::parse_wire(invalid), None, "{invalid:?}");
        }
        assert_eq!(
            FreshPlacement::normalize_wire(None),
            FreshPlacement::CursorOutputMru
        );
    }

    #[test]
    fn cursor_lookup_uses_half_open_full_bounds() {
        let screens = vec![
            (output("left"), rect(0, 0, 800, 600)),
            (output("right"), rect(800, 0, 800, 600)),
        ];
        assert_eq!(output_at_cursor(&screens, (0, 0)), Some(output("left")));
        assert_eq!(output_at_cursor(&screens, (799, 599)), Some(output("left")));
        // Shared edge belongs to the right screen (half-open).
        assert_eq!(
            output_at_cursor(&screens, (800, 300)),
            Some(output("right"))
        );
        assert_eq!(output_at_cursor(&screens, (1600, 300)), None);
        assert_eq!(output_at_cursor(&screens, (-1, 300)), None);
        assert_eq!(output_at_cursor(&screens, (400, 600)), None);
    }

    #[test]
    fn cursor_lookup_first_match_wins_overlaps() {
        let screens = vec![
            (output("first"), rect(0, 0, 800, 600)),
            (output("second"), rect(0, 0, 800, 600)),
        ];
        assert_eq!(
            output_at_cursor(&screens, (100, 100)),
            Some(output("first"))
        );
    }

    #[test]
    fn switch_resolution_orders_cursor_focused_fallback() {
        let cursor = output("cursor-out");
        let focused = output("focused-out");
        let primary_first = vec![output("primary"), output("other")];
        // Cursor mode: cursor wins over focused.
        assert_eq!(
            resolve_switch_output(
                SwitchTarget::Cursor,
                Some(&cursor),
                Some(&focused),
                &primary_first
            ),
            Some(cursor.clone())
        );
        // Cursor mode without cursor: focused fallback.
        assert_eq!(
            resolve_switch_output(SwitchTarget::Cursor, None, Some(&focused), &primary_first),
            Some(focused.clone())
        );
        // Focused mode ignores the cursor.
        assert_eq!(
            resolve_switch_output(
                SwitchTarget::Focused,
                Some(&cursor),
                Some(&focused),
                &primary_first
            ),
            Some(focused.clone())
        );
        // No facts: primary fallback, then first.
        assert_eq!(
            resolve_switch_output(SwitchTarget::Cursor, None, None, &primary_first),
            Some(output("primary"))
        );
        assert_eq!(
            resolve_switch_output(SwitchTarget::Focused, None, None, &[]),
            None
        );
    }

    #[test]
    fn fresh_resolution_matches_placement_policy() {
        let cursor = output("cursor-out");
        let focused = output("focused-out");
        let primary_first = vec![output("primary")];
        for placement in [
            FreshPlacement::CursorOutputMru,
            FreshPlacement::CursorOutputCursorClosest,
        ] {
            assert_eq!(
                resolve_fresh_output(placement, Some(&cursor), Some(&focused), &primary_first),
                Some(cursor.clone())
            );
            assert_eq!(
                resolve_fresh_output(placement, None, Some(&focused), &primary_first),
                Some(focused.clone())
            );
        }
        // i3/sway placement never consults the cursor.
        assert_eq!(
            resolve_fresh_output(
                FreshPlacement::FocusedOutputMru,
                Some(&cursor),
                Some(&focused),
                &primary_first
            ),
            Some(focused.clone())
        );
        assert_eq!(
            resolve_fresh_output(
                FreshPlacement::FocusedOutputMru,
                Some(&cursor),
                None,
                &primary_first
            ),
            Some(output("primary"))
        );
    }

    #[test]
    fn action_resolution_preserves_foreground_pointer_cache_order() {
        let foreground = output("fore");
        let pointer = output("point");
        let cached = output("cache");
        let ordered = vec![output("fore"), output("point"), output("cache")];
        // Foreground beats live pointer and live cache.
        assert_eq!(
            resolve_action_output(Some(&foreground), Some(&pointer), Some(&cached), &ordered),
            Some(foreground.clone())
        );
        // Pointer disagreement: foreground wins; without it the pointer wins.
        assert_eq!(
            resolve_action_output(Some(&foreground), Some(&pointer), None, &ordered),
            Some(foreground.clone())
        );
        assert_eq!(
            resolve_action_output(None, Some(&pointer), Some(&cached), &ordered),
            Some(pointer.clone())
        );
        // Absent foreground with unknown pointer falls to the cache.
        assert_eq!(
            resolve_action_output(None, None, Some(&cached), &ordered),
            Some(cached.clone())
        );
        // Unknown foreground/pointer never invent: they fall through.
        let unknown = output("gone");
        assert_eq!(
            resolve_action_output(Some(&unknown), Some(&unknown), Some(&cached), &ordered),
            Some(cached.clone())
        );
        assert_eq!(
            resolve_action_output(Some(&unknown), Some(&unknown), None, &ordered),
            Some(output("fore"))
        );
        // A cached output outside the known registry never selects: it
        // falls through to output order (the adapter sends `None` for an
        // unmapped cache; only mapped-but-removed entries can arrive here
        // via the retained registry, and those still select below).
        let stale = output("stale");
        let registry = vec![output("fore"), output("point"), stale.clone()];
        assert_eq!(
            resolve_action_output(None, None, Some(&stale), &registry),
            Some(stale.clone())
        );
        assert_eq!(
            resolve_action_output(None, None, Some(&stale), &ordered),
            Some(output("fore"))
        );
        // No facts: output order, then nothing.
        assert_eq!(
            resolve_action_output(None, None, None, &ordered),
            Some(output("fore"))
        );
        assert_eq!(resolve_action_output(None, None, None, &[]), None);
    }

    fn leaf(id: &str) -> NodeId {
        NodeId::from(id)
    }

    #[test]
    fn cursor_closest_inside_wins_with_zero_distance() {
        let leaves = vec![
            (leaf("a"), rect(0, 0, 100, 100)),
            (leaf("b"), rect(200, 0, 100, 100)),
        ];
        assert_eq!(cursor_closest_leaf(&leaves, (250, 50)), Some(leaf("b")));
        assert_eq!(cursor_closest_leaf(&leaves, (50, 50)), Some(leaf("a")));
    }

    #[test]
    fn cursor_closest_measures_outside_gaps() {
        // Gap midpoint between two tiles: nearer rect wins.
        let leaves = vec![
            (leaf("a"), rect(0, 0, 100, 100)),
            (leaf("b"), rect(300, 0, 100, 100)),
        ];
        assert_eq!(cursor_closest_leaf(&leaves, (190, 50)), Some(leaf("a")));
        assert_eq!(cursor_closest_leaf(&leaves, (210, 50)), Some(leaf("b")));
        assert!(cursor_closest_leaf(&[], (0, 0)).is_none());
    }

    #[test]
    fn cursor_closest_first_wins_exact_ties() {
        let leaves = vec![
            (leaf("a"), rect(0, 0, 100, 100)),
            (leaf("b"), rect(0, 0, 100, 100)),
        ];
        assert_eq!(cursor_closest_leaf(&leaves, (500, 500)), Some(leaf("a")));
    }

    #[test]
    fn cursor_closest_stays_exact_at_i32_extremes() {
        // Axis gaps near 2^32 would overflow i64 squares; i128 stays exact.
        let leaves = vec![
            (
                leaf("a"),
                Rect {
                    x: i32::MIN,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            ),
            (
                leaf("b"),
                Rect {
                    x: i32::MAX - 100,
                    y: 0,
                    w: 100,
                    h: 100,
                },
            ),
        ];
        assert_eq!(
            cursor_closest_leaf(&leaves, (i32::MAX, 50)),
            Some(leaf("b"))
        );
        assert_eq!(
            cursor_closest_leaf(&leaves, (i32::MIN + 50, 50)),
            Some(leaf("a"))
        );
    }

    #[test]
    fn cursor_closest_includes_rect_end_edges() {
        // Pinned Hyprland `vecToRectDistanceSquared` is inclusive on both
        // ends (zero when p1 <= vec <= p2). Abutting rects therefore tie
        // with zero distance on their shared edge; slice order wins. This
        // is intentionally distinct from `output_at_cursor`, which stays
        // half-open so the shared screen edge belongs to the right screen.
        let leaves = vec![
            (leaf("a"), rect(0, 0, 800, 600)),
            (leaf("b"), rect(800, 0, 800, 600)),
        ];
        assert_eq!(cursor_closest_leaf(&leaves, (800, 300)), Some(leaf("a")));
        assert_eq!(cursor_closest_leaf(&leaves, (799, 300)), Some(leaf("a")));
        assert_eq!(cursor_closest_leaf(&leaves, (801, 300)), Some(leaf("b")));
        assert_eq!(cursor_closest_leaf(&leaves, (800, 600)), Some(leaf("a")));
    }
}
