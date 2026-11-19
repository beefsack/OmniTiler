//! Portable shared bounds and pure predicates.
//!
//! Single portable source for wire/core geometry and identity gates that
//! previously drifted across `seed`, `engine`/`active_group`, and the planner
//! protocol layer. No transport, JSON, platform, or process imports; only
//! [`crate::geometry::Rect`] plus `std`. Validation order, admissibility, and
//! error messages stay at the call sites; this only owns the shared numbers
//! and the byte-identical pure checks.

use crate::geometry::Rect;

/// Opaque id bound.
pub const MAX_OPAQUE_ID_LEN: usize = 128;
/// Bounded carried-geometry extent.
pub const GEOMETRY_BOUND: i32 = 16384;
/// Bounded carried-geometry coordinate endpoint (half-open inclusive `-32768..=32768`).
pub const COORDINATE_BOUND: i32 = 32768;
/// Bounded gap extent for carried work-area geometry (inner and outer).
pub const MAX_GAP: i32 = 64;

/// Opaque id gate: non-empty bounded token over `[A-Za-z0-9-_.]`.
#[must_use]
pub fn is_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OPAQUE_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Carried point gate over adapter-normalized integer coordinates.
#[must_use]
pub const fn valid_carried_point(x: i32, y: i32) -> bool {
    x >= -COORDINATE_BOUND
        && x <= COORDINATE_BOUND
        && y >= -COORDINATE_BOUND
        && y <= COORDINATE_BOUND
}

/// Carried rectangle gate over adapter-normalized integer geometry.
/// Half-open rectangles: endpoints inclusive `-32768..=32768`, extents `1..=16384`.
#[must_use]
pub const fn valid_carried_rect(x: i32, y: i32, w: i32, h: i32) -> bool {
    let x = x as i64;
    let y = y as i64;
    let w = w as i64;
    let h = h as i64;
    let coord = COORDINATE_BOUND as i64;
    let extent = GEOMETRY_BOUND as i64;
    w > 0
        && h > 0
        && w <= extent
        && h <= extent
        && x >= -coord
        && y >= -coord
        && x + w <= coord
        && y + h <= coord
}

/// Containment gate with checked `i64` edges.
#[must_use]
pub fn rect_contained(inner: Rect, outer: Rect) -> bool {
    let inner_right = i64::from(inner.x) + i64::from(inner.w);
    let inner_bottom = i64::from(inner.y) + i64::from(inner.h);
    let outer_right = i64::from(outer.x) + i64::from(outer.w);
    let outer_bottom = i64::from(outer.y) + i64::from(outer.h);
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

/// Gap gate: non-negative and within the shared bound.
#[must_use]
pub fn is_gap(value: i32) -> bool {
    (0..=MAX_GAP).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carried_rect_closed_half_open_bounds() {
        let valid = [
            (-16384, -16384, 16384, 16384),
            (16384, 16384, 1, 1),
            (-16384, 16384, 1, 1),
            (16384, 0, 800, 600),
            (16392, 8, 784, 584),
            (-32768, -32768, 16384, 16384),
            (16384, 16384, 16384, 16384),
            (32767, 32767, 1, 1),
        ];
        for (x, y, w, h) in valid {
            assert!(valid_carried_rect(x, y, w, h), "want valid {x} {y} {w} {h}");
        }
        let invalid = [
            (0, 0, 0, 1),
            (0, 0, 1, 0),
            (0, 0, 16385, 1),
            (0, 0, 1, 16385),
            (-32769, 0, 1, 1),
            (0, -32769, 1, 1),
            (32768, 0, 1, 1),
            (0, 32768, 1, 1),
            (16385, 0, 16384, 1),
            (0, 16385, 1, 16384),
            (i32::MAX, 0, 1, 1),
            (i32::MIN, 0, 1, 1),
        ];
        for (x, y, w, h) in invalid {
            assert!(
                !valid_carried_rect(x, y, w, h),
                "want invalid {x} {y} {w} {h}"
            );
        }
    }

    #[test]
    fn carried_point_shares_axis_bound() {
        assert!(valid_carried_point(-32768, 32768));
        assert!(valid_carried_point(32768, -32768));
        assert!(valid_carried_point(-16384, 16384));
        assert!(!valid_carried_point(32769, 0));
        assert!(!valid_carried_point(0, -32769));
    }
}
