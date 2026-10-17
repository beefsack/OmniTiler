#![forbid(unsafe_code)]
use super::*;

use crate::ffi::{
    OverlayClass, destroy_overlay, enumerate_windows, hide_overlay, is_visible, overlay_alive,
    place_above, place_below, present_layered, present_preview, window_rect,
};

/// Lowest window in `EnumWindows` top-to-bottom order among `hwnds`: the
/// renderable anchor the underlay sorts directly beneath. `None` when no
/// candidate is enumerated (closed, or the order is unreadable).
pub(crate) fn lowest_in_z(hwnds: &[u64]) -> Option<u64> {
    let order = crate::ffi::enumerate_windows()?;
    order
        .iter()
        .rev()
        .find(|hwnd| hwnds.contains(hwnd))
        .copied()
}

/// True only when the cached overlay needs a same-geometry reassert: the
/// surface is gone, hidden, misplaced, or displaced below the fresh target.
/// Adjacent-below is the achievable correct placement for a background
/// owner's surface under its foreground target, so only true-below (or an
/// unreadable probe, which recovers by reasserting) reasserts for the border.
/// The underlay (`for_underlay`) additionally reasserts when above its anchor:
/// its anchor is the lowest renderable group member, so above-anchor is never
/// an acceptable cached placement. The reassert is a `place_overlay` in the
/// target's own band directly beneath the fresh target HWND, never a blind
/// raise over unrelated topmost or shell UI.
pub(super) fn overlay_needs_reassert(
    overlay_u64: u64,
    outer: Rect,
    target_hwnd: u64,
    for_underlay: bool,
) -> bool {
    if overlay_u64 == 0 {
        return true;
    }
    if !is_overlay_window(overlay_u64) {
        return true;
    }
    if !is_visible(overlay_u64) {
        return true;
    }
    let Some(actual) = window_rect(overlay_u64) else {
        return true;
    };
    let w = actual.right - actual.left;
    let h = actual.bottom - actual.top;
    if actual.left != outer.x || actual.top != outer.y || w != outer.w || h != outer.h {
        return true;
    }
    if target_hwnd == 0 || target_hwnd == overlay_u64 {
        return false;
    }
    // Behind means displaced (a foreign window between, or far below):
    // adjacent-below is the achievable correct placement for a background
    // owner's surface under its foreground target, so only true-below
    // reasserts for the border. The underlay anchor is the lowest renderable
    // member, so an above-anchor cached surface is equally displaced and must
    // also reassert. Unknown order (endpoint vanished mid-probe, or
    // enumeration failed) recovers by reasserting: a needless same-geometry
    // move is harmless, a missed displacement is not.
    z_needs_reassert(z_relation(overlay_u64, target_hwnd), for_underlay)
}

/// Pure z verdict shared by both carriers so the underlay Above repair is
/// unit-covered without a live window. Border preserves its foreground-below
/// assumption (`Above` stays acceptable); the underlay never accepts an
/// above-anchor cached placement.
pub(super) fn z_needs_reassert(relation: Option<ZRelation>, for_underlay: bool) -> bool {
    match relation {
        Some(ZRelation::AdjacentBelow) => false,
        Some(ZRelation::Above) => for_underlay,
        Some(ZRelation::Below) | None => true,
    }
}

pub(super) fn is_overlay_window(hwnd_u64: u64) -> bool {
    overlay_alive(hwnd_u64)
}

/// Creator-thread liveness read for one owned carrier window: the shared
/// readback behind renderer-side own-anchor validation and pre-present
/// ensure. Read-only, never a write.
pub(crate) fn overlay_window_alive(hwnd_u64: u64) -> bool {
    is_overlay_window(hwnd_u64)
}

/// Z-order relation between the owned overlay and the fresh target in
/// `EnumWindows` top-to-bottom order. A background owner cannot place its
/// surface above the foreground window (live probe: steady-state overlay
/// adjacent-below the focused helper despite `HWND_TOP`), so `AdjacentBelow`
/// is the achievable correct placement for an eligible foreground target:
/// directly beneath it, above everything else, ring pixels uncovered.
/// `GetWindow` handle walks are invalid evidence (they skip the toolwindow
/// overlay against normal targets). Read-only enumeration, no writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ZRelation {
    Above,
    AdjacentBelow,
    Below,
}

/// Z-order probe: relation of the overlay to the fresh target, `None` when
/// either endpoint is gone or the order is unreadable.
fn z_relation(overlay_u64: u64, target_hwnd: u64) -> Option<ZRelation> {
    let order = enumerate_windows()?;
    let mut upper_idx: Option<usize> = None;
    let mut lower_idx: Option<usize> = None;
    for (i, hwnd) in order.iter().enumerate() {
        if *hwnd == overlay_u64 && upper_idx.is_none() {
            upper_idx = Some(i);
        }
        if *hwnd == target_hwnd && lower_idx.is_none() {
            lower_idx = Some(i);
        }
        if upper_idx.is_some() && lower_idx.is_some() {
            break;
        }
    }
    match (upper_idx, lower_idx) {
        (Some(u), Some(l)) if u < l => Some(ZRelation::Above),
        (Some(u), Some(l)) if u == l + 1 => Some(ZRelation::AdjacentBelow),
        (Some(_), Some(_)) => Some(ZRelation::Below),
        _ => None,
    }
}

pub(super) fn create_overlay_window() -> Result<u64, DynError> {
    create_overlay_window_in(OVERLAY_CLASS)
}

/// Create one owned carrier window in `class` (registration tolerates a
/// previous registration; creation is rare, at most once per carrier per
/// process). Only the three owned carrier classes map; anything else fails
/// closed instead of instantiating a foreign window procedure.
pub(crate) fn create_overlay_window_in(class: &str) -> Result<u64, DynError> {
    let kind = if class == OVERLAY_CLASS {
        OverlayClass::Border
    } else if class == UNDERLAY_CLASS {
        OverlayClass::Underlay
    } else if class == PREVIEW_CLASS {
        OverlayClass::Preview
    } else {
        return Err(err("error: overlay unknown class"));
    };
    crate::ffi::create_overlay_window(kind).map_err(err)
}

pub(super) fn hide_window(hwnd_u64: u64) -> bool {
    hide_overlay(hwnd_u64)
}

pub(super) fn destroy_window(hwnd_u64: u64) -> bool {
    destroy_overlay(hwnd_u64)
}

pub(super) fn move_overlay(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
) -> Result<(), DynError> {
    place_below(hwnd_u64, outer, topmost, target_hwnd).map_err(err)
}

/// Paint the ring into a fresh 32bpp top-down DIB and present it with
/// `UpdateLayeredWindow` (`ULW_ALPHA`). Returns an FNV-1a checksum over the
/// owned DIB bytes (proof support without screen capture), computed in the
/// single paint pass (no second full-buffer hash walk).
pub(super) fn paint_and_present(
    hwnd_u64: u64,
    outer: Rect,
    width_px: i32,
    radius_px: i32,
    color: (u8, u8, u8),
    topmost: bool,
    target_hwnd: u64,
) -> Result<u64, DynError> {
    if width_px < 0 || radius_px < 0 {
        return Err(err("error: active-border bad paint geometry"));
    }
    if width_px == 0 {
        hide_window(hwnd_u64);
        return Ok(0);
    }
    present_layered(
        hwnd_u64,
        outer,
        topmost,
        target_hwnd,
        "active-border",
        |pixels| paint_ring(pixels, outer.w, outer.h, width_px, radius_px, color),
    )
    .map_err(err)
}

/// Paint the premultiplied-alpha group fill into a fresh 32bpp top-down DIB
/// and present it on the shared carrier pipeline. Same checksum/placement
/// contract as the ring path; only the painter differs.
pub(super) fn paint_and_present_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
    topmost: bool,
    anchor_hwnd: u64,
) -> Result<u64, DynError> {
    present_layered(
        hwnd_u64,
        outer,
        topmost,
        anchor_hwnd,
        "group-underlay",
        |pixels| crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha),
    )
    .map_err(err)
}

/// Position the drop-preview surface ABOVE windows in the topmost band
/// (KWin overlay-item analogue). Unlike [`move_overlay`] there is no
/// below-target step: the filled target slot must stay above normal app
/// windows while shown. `SWP_NOACTIVATE` keeps it nonactivating; the
/// `WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` creation
/// flags keep it click-through and out of Alt+Tab. Callers hide immediately
/// on cancel/finish/suspend/teardown so the topmost band is never pinned
/// over unrelated shell UI.
pub(super) fn place_preview_above(hwnd_u64: u64, outer: Rect) -> Result<(), DynError> {
    place_above(hwnd_u64, outer).map_err(err)
}

/// Paint the premultiplied-alpha drop-preview fill and present it ABOVE
/// windows on the shared DIB pipeline, then assert the topmost band.
/// Same checksum contract as the underlay path; only the placement differs.
pub(super) fn paint_and_present_preview_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
) -> Result<u64, DynError> {
    present_preview(hwnd_u64, outer, "drop-preview", |pixels| {
        crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha)
    })
    .map_err(err)
}

/// True only when the cached preview needs a same-geometry reassert: the
/// surface is gone, hidden, or misplaced. Z-order against the mover is not
/// probed here (the preview presents topmost by construction); a hidden or
/// misplaced surface re-presents, anything else stays as-is.
pub(super) fn preview_needs_reassert(preview_u64: u64, outer: Rect) -> bool {
    if preview_u64 == 0 || !is_overlay_window(preview_u64) {
        return true;
    }
    if !is_visible(preview_u64) {
        return true;
    }
    let Some(actual) = window_rect(preview_u64) else {
        return true;
    };
    let w = actual.right - actual.left;
    let h = actual.bottom - actual.top;
    actual.left != outer.x || actual.top != outer.y || w != outer.w || h != outer.h
}
