#![forbid(unsafe_code)]
use super::*;

/// Shipped KDE parity: inner gap 8, outer gap 8 (`kwin/src/domain-gap.ts`).
pub const INNER_GAP: i32 = 8;
/// Shipped KDE parity: outer gap 8.
pub const OUTER_GAP: i32 = 8;
/// Single-display tiling-only output identity.
pub const OUTPUT_ID: &str = "display-0";
/// Implicit current workspace for the tiling-only slice.
pub const WORKSPACE_ID: &str = "workspace-0";
/// Stable Engine owner token for this adapter.
pub const OWNER_ID: &str = "tiler-windows";

/// Adapter-side domain bounds: the protocol layer owns the outer inset
/// (`tiler-protocol` applies `inset_bounds(carried, outer_gap)`), and this
/// adapter constructs `CoreEvent` directly, so inset here. `None` when the
/// work area cannot carry the margin: the caller skips the tick with retained
/// Engine state rather than tiling an un-inset domain.
#[must_use]
pub fn tiling_domain_bounds(work: Rect) -> Option<Rect> {
    tiling_domain_bounds_with(work, OUTER_GAP)
}

/// Outer-gap-parameterized domain inset for live settings: the protocol
/// layer owns the outer inset and this adapter constructs `CoreEvent`
/// directly, so inset here. `None` when the work area cannot carry the
/// margin: the caller skips the tick with retained Engine state rather than
/// tiling an un-inset domain.
#[must_use]
pub fn tiling_domain_bounds_with(work: Rect, outer_gap: i32) -> Option<Rect> {
    if outer_gap < 0 {
        return None;
    }
    tiler_core::geometry::inset_bounds(work, outer_gap).ok()
}

/// Measured per-app frame: outer `GetWindowRect` minus visible
/// `DWMWA_EXTENDED_FRAME_BOUNDS`, in physical pixels. All fields non-negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl FrameInsets {
    /// Measure from one outer/visible pair; negative deltas saturate to zero
    /// (DWM rounding) rather than failing the tick.
    #[must_use]
    pub fn measure(outer: Rect, visible: Rect) -> Self {
        Self {
            left: visible.x.saturating_sub(outer.x).max(0),
            top: visible.y.saturating_sub(outer.y).max(0),
            right: (outer.x as i64 + outer.w as i64 - visible.x as i64 - visible.w as i64)
                .clamp(0, i64::from(i32::MAX)) as i32,
            bottom: (outer.y as i64 + outer.h as i64 - visible.y as i64 - visible.h as i64)
                .clamp(0, i64::from(i32::MAX)) as i32,
        }
    }

    /// Map an Engine visible rectangle back to the outer rectangle for
    /// `SetWindowPos`. `None` on arithmetic overflow: the caller skips the
    /// write for this tick instead of clamping geometry.
    #[must_use]
    pub fn visible_to_outer(self, visible: Rect) -> Option<Rect> {
        let x = visible.x.checked_sub(self.left)?;
        let y = visible.y.checked_sub(self.top)?;
        let w = visible.w.checked_add(self.left)?.checked_add(self.right)?;
        let h = visible.h.checked_add(self.top)?.checked_add(self.bottom)?;
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(Rect { x, y, w, h })
    }
}

/// Validate one raw outer bound (`ptMinTrackSize`/`ptMaxTrackSize` axis) in
/// physical pixels, independently per axis (R-SPC-04 D1, G-05):
/// - `None` when the app declares nothing usable there: negative,
///   unbounded-sentinel (`i32::MAX`), or absurd out-of-bound values all map
///   to unknown, never to a floor or cap;
/// - `Some(0)` for an explicit zero: unset per axis, transported (not
///   dropped) so the shared predicate distinguishes partial-zero vectors
///   such as min=max=(640,0) from missing bounds;
/// - otherwise the positive in-bound size.
///
/// Minimum projection behavior is unchanged: the shared
/// [`tiler_core::size_hints`] `meaningful` filter treats `Some(0)` exactly
/// like absent everywhere except the fixed-size predicate.
#[must_use]
pub fn normalize_outer_bound(value: i32) -> Option<i32> {
    match value {
        v if v < 0 => None,
        0 => Some(0),
        v if (1..=tiler_core::bounds::GEOMETRY_BOUND).contains(&v) => Some(v),
        _ => None,
    }
}

/// Validate one raw outer minimum-track extent (`ptMinTrackSize`) in physical
/// pixels, independently per axis. Each axis maps through
/// [`normalize_outer_bound`]: usable sizes and explicit zeros survive per
/// axis, unknown axes map to `None` without discarding the usable sibling.
#[must_use]
pub fn normalize_min_track(outer_w: i32, outer_h: i32) -> (Option<i32>, Option<i32>) {
    (
        normalize_outer_bound(outer_w),
        normalize_outer_bound(outer_h),
    )
}

/// Convert one normalized outer bound into visible physical pixels by
/// subtracting the frame extent on its axis. Unknown stays unknown; an
/// explicit zero stays unset (`Some(0)`, no subtraction is meaningful); a
/// positive bound whose visible remainder vanishes inside its own frame maps
/// to unknown (`None`), never to a zero floor.
fn visible_bound(outer: Option<i32>, frame: i32) -> Option<i32> {
    match outer {
        None => None,
        Some(0) => Some(0),
        Some(v) => {
            let remainder = v.checked_sub(frame)?;
            if remainder >= 1 {
                Some(remainder)
            } else {
                None
            }
        }
    }
}

/// Convert a raw outer minimum-track size into visible physical pixels by
/// subtracting the currently measured frame insets, independently per axis.
/// An axis whose remainder vanishes inside its own frame carries no usable
/// constraint on that axis; the usable sibling axis still converts.
#[must_use]
pub fn visible_min_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> (Option<i32>, Option<i32>) {
    let frame_w = insets.left.checked_add(insets.right).unwrap_or(i32::MAX);
    let frame_h = insets.top.checked_add(insets.bottom).unwrap_or(i32::MAX);
    let (outer_w, outer_h) = normalize_min_track(outer_w, outer_h);
    (
        visible_bound(outer_w, frame_w),
        visible_bound(outer_h, frame_h),
    )
}

/// Portable minimum-size hint for one window from its fresh outer
/// minimum-track size and currently measured frame insets. Timeout, failure,
/// or invalid data already maps to unknown at the native query, which lands
/// here as no hint: unknown means no hint, never a learned or persistent
/// floor. Only minimum bounds are ever set; maximums stay absent. Kept for
/// the minimum-only call sites; [`hints_from_outer`] carries both sides for
/// fixed-size admission.
#[must_use]
pub fn min_hints_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> tiler_core::size_hints::WindowSizeHints {
    hints_from_outer(Some((outer_w, outer_h)), None, insets)
}

/// Validate one raw outer maximum-track extent (`ptMaxTrackSize`) in physical
/// pixels. Same per-axis rule as [`normalize_min_track`]. A seeded but
/// app-untouched maximum never reaches here: the native query strips values
/// identical to the seed before conversion, so a system default is never
/// claimed as app evidence and no style/resizeable inference is involved.
#[must_use]
pub fn normalize_max_track(outer_w: i32, outer_h: i32) -> (Option<i32>, Option<i32>) {
    (
        normalize_outer_bound(outer_w),
        normalize_outer_bound(outer_h),
    )
}

/// Convert a raw outer maximum-track size into visible physical pixels by
/// subtracting the currently measured frame insets, independently per axis.
/// Same rule as [`visible_min_from_outer`].
#[must_use]
pub fn visible_max_from_outer(
    outer_w: i32,
    outer_h: i32,
    insets: FrameInsets,
) -> (Option<i32>, Option<i32>) {
    let frame_w = insets.left.checked_add(insets.right).unwrap_or(i32::MAX);
    let frame_h = insets.top.checked_add(insets.bottom).unwrap_or(i32::MAX);
    let (outer_w, outer_h) = normalize_max_track(outer_w, outer_h);
    (
        visible_bound(outer_w, frame_w),
        visible_bound(outer_h, frame_h),
    )
}

/// Portable size hints for one window from its fresh outer minimum/maximum
/// track sizes and currently measured frame insets. Each side converts
/// independently with the same fresh DPI/frame-inset conversion, and each
/// axis converts independently within its side: a usable bound sets its
/// hint, an explicit zero rides as unset (`Some(0)`), and an unknown axis
/// stays unknown without discarding its usable sibling. Either side may be
/// unknown (`None`, timeout, failure, budget skip, or invalid remainders).
/// Unknown maximums never classify: the shared
/// [`tiler_core::size_hints::is_fixed_size_with`] predicate requires both
/// sides present, usable, and equal (nonzero per axis under G-05).
#[must_use]
pub fn hints_from_outer(
    min_outer: Option<(i32, i32)>,
    max_outer: Option<(i32, i32)>,
    insets: FrameInsets,
) -> tiler_core::size_hints::WindowSizeHints {
    let (min_w, min_h) =
        min_outer.map_or((None, None), |(w, h)| visible_min_from_outer(w, h, insets));
    let (max_w, max_h) =
        max_outer.map_or((None, None), |(w, h)| visible_max_from_outer(w, h, insets));
    tiler_core::size_hints::WindowSizeHints {
        min_w,
        min_h,
        max_w,
        max_h,
    }
}

/// Effective native target for an overconstrained tile: planned origin with
/// each extent raised to the known native minimum (`WM_GETMINMAXINFO` track
/// minus frame insets; unknown minimums keep the planned rect). Explicit so
/// readback matches the OS-enforced size instead of fighting every tick.
#[must_use]
pub fn overconstrained_effective(
    planned: Rect,
    hints: tiler_core::size_hints::WindowSizeHints,
) -> Rect {
    let w = hints
        .meaningful_min_w()
        .map_or(planned.w, |min| planned.w.max(min));
    let h = hints
        .meaningful_min_h()
        .map_or(planned.h, |min| planned.h.max(min));
    Rect {
        x: planned.x,
        y: planned.y,
        w,
        h,
    }
}
