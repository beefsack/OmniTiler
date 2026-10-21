#![forbid(unsafe_code)]
use super::*;

/// Stable output key for a rectangle center: the monitor containing the
/// center wins; otherwise the largest overlap; otherwise the primary.
/// Rectangles decide placement, device names decide session identity.
pub(super) fn output_for_rect(areas: &[MonitorArea], rect: &Rect) -> String {
    let cx = rect.x + rect.w / 2;
    let cy = rect.y + rect.h / 2;
    for area in areas {
        let r = area.full;
        if cx >= r.x && cx < r.x + r.w && cy >= r.y && cy < r.y + r.h {
            return area.device.clone();
        }
    }
    let mut best: Option<(&MonitorArea, i64)> = None;
    for area in areas {
        let r = area.full;
        let ox = (rect.x.min(r.x + r.w) - rect.x.max(r.x)).max(0) as i64;
        let oy = (rect.y.min(r.y + r.h) - rect.y.max(r.y)).max(0) as i64;
        let overlap = ox * oy;
        if best.as_ref().is_none_or(|(_, d)| overlap > *d) {
            best = Some((area, overlap));
        }
    }
    best.map(|(a, _)| a.device.clone())
        .or_else(|| areas.first().map(|a| a.device.clone()))
        .unwrap_or_else(|| "display-?".to_owned())
}

pub(super) fn output_for_point(areas: &[MonitorArea], x: i32, y: i32) -> Option<String> {
    let outputs: Vec<crate::native_guards::DirectionalOutput> = areas
        .iter()
        .map(|area| crate::native_guards::DirectionalOutput {
            id: area.device.clone(),
            full: area.full,
        })
        .collect();
    crate::native_guards::output_at_point(&outputs, x, y)
}

fn foreground_output(areas: &[MonitorArea]) -> Option<String> {
    let foreground = crate::ffi::foreground_window();
    if foreground == 0 {
        return None;
    }
    let raw = crate::ffi::window_rect(foreground)?;
    rect_from_win(raw).map(|r| output_for_rect(areas, &r))
}

/// Exact output context for a digit chord: the genuine foreground monitor
/// when known, else the pointer monitor, else the cached last-switch
/// output when still known, else output ordering. The cached value is
/// deliberately last: a stale last-switch cache must never beat live
/// focus (tray pager parity). Never invents a key.
pub(super) fn chord_output(state: &TileLoop, areas: &[MonitorArea]) -> Option<String> {
    let cached = if state.active_output.is_empty() {
        None
    } else {
        Some(state.active_output.as_str())
    };
    let foreground = foreground_output(areas);
    let pointer = cursor_pos().and_then(|(x, y)| output_for_point(areas, x, y));
    crate::native_guards::output_context_keys(
        &state.output_devices(),
        cached,
        foreground.as_deref(),
        pointer.as_deref(),
    )
}

/// Windows multi-output directional foundation (REQ-OUT-01..03, REQ-MOV-08).
///
/// Exhausted directional tiled moves cross all four directions into the
/// selected adjacent output's current workspace; exhausted horizontal focus
/// crosses Left/Right only (vertical stays local). Selection reuses the
/// shared window-based FULL-rectangle rule; domain bounds use per-output work
/// areas with the live outer-gap inset; topology uses FULL rectangles. Core
/// rects are consumed in DPI-aware physical coordinates with no rescaling.
///
/// Fences (lifetime/owner/scope/recovery/geometry/visibility/current-view)
/// are complete: snapshot at pair build, re-verified live before any
/// membership change or native effect. Failed half-applied operations stop
/// narrowly with no replay and no permanent disabling; the retained Engine
/// plan stands and the next observation converges.
/// Read the live drag-drop fences over the pointer-resolved (non-adjacency)
/// snapshot.
pub(super) fn drag_drop_live(
    state: &TileLoop,
    areas: &[MonitorArea],
    snapshot: &crate::native_guards::DragDropFenceSnapshot,
    mover_key: &crate::native_guards::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::native_guards::DirectionalFenceLive {
    fence_live_impl(
        state,
        areas,
        snapshot,
        mover_key,
        membership_ok,
        overlay_maximized,
        overlay_fullscreen,
    )
}

/// Shared live-fence reads for directional and pointer-drop pairs.
fn fence_live_impl(
    state: &TileLoop,
    areas: &[MonitorArea],
    snapshot: &crate::native_guards::DragDropFenceSnapshot,
    mover_key: &crate::native_guards::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::native_guards::DirectionalFenceLive {
    let source_active = state.active_view(&snapshot.source_output);
    let target_active = state.active_view(&snapshot.target_output);
    let lifetime_ok = state.member_tags.get(mover_key).is_some_and(|stored| {
        crate::native_guards::visible_lifetime_ok(
            stored,
            live_member_tag(mover_key.hwnd).as_deref(),
        )
    });
    let scope_ok = state.member_identity.get(mover_key).is_some_and(|stored| {
        crate::tiling::scope_allows(&state.scope, &stored.exe_path)
            && hosted_gate_allows(
                &stored.exe_path,
                mover_key.hwnd,
                stored.pid,
                &state.scope_hosts,
            )
    });
    let topology_ok = areas
        .iter()
        .find(|a| a.device == snapshot.source_output)
        .is_some_and(|area| {
            area.full == snapshot.source_full
                && tiling_domain_bounds_with(area.work, state.outer_gap)
                    .is_some_and(|bounds| bounds == snapshot.source_bounds)
        })
        && areas
            .iter()
            .find(|a| a.device == snapshot.target_output)
            .is_some_and(|area| {
                area.full == snapshot.target_full
                    && tiling_domain_bounds_with(area.work, state.outer_gap)
                        .is_some_and(|bounds| bounds == snapshot.target_bounds)
            });
    let owner_ok = state.owner.as_str() == snapshot.owner.as_str()
        && state.generation.as_str() == snapshot.generation.as_str();
    crate::native_guards::DirectionalFenceLive {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_tiled: state.is_tiled_view(&snapshot.source_output, &snapshot.source_workspace),
        target_tiled: state.is_tiled_view(&snapshot.target_output, &snapshot.target_workspace),
        source_is_active: source_active.as_deref() == Some(snapshot.source_workspace.as_str()),
        target_is_active: target_active.as_deref() == Some(snapshot.target_workspace.as_str()),
        topology_ok,
        scope_ok,
        lifetime_ok,
        membership_ok,
        overlay_maximized,
        overlay_fullscreen,
        owner_ok,
    }
}

/// Verify post-apply arrival for one mover row with fresh native proof,
/// never the setter result and never core desire.
///
/// Tiled rows under Controlled frame authority prove through the exact
/// published frame target on the row's device: the live frame must equal
/// `effective_frame` (the only write target; `allocation` is the canonical
/// slot, never a write target and never arrival proof) with screen affinity
/// on the same device.
///
/// Every other row (membership-only redirect lanes with no published frame,
/// fullscreen rows without a frame target, floats) proves through the
/// proven applied-homing receipt the tick merged with independent fresh
/// revalidation: non-floating rows must additionally still sit on the row's
/// device, while floating rows preserve their physical frame by design and
/// settle on the receipt alone. Anything unproven fails closed so the
/// caller holds instead of claiming arrival.
#[allow(clippy::too_many_arguments)]
pub(super) fn drop_arrival_verified(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    reply: &tiler_core::world_report::WorldReply,
    mover_token: &str,
    mover_hwnd: u64,
    expect_device: Option<&str>,
) -> bool {
    let Some(row) = reply.windows.iter().find(|w| w.lifetime.0 == mover_token) else {
        return false;
    };
    let Some(device) = state.device_of(&row.output.0) else {
        return false;
    };
    if expect_device.is_some_and(|expected| expected != device) {
        return false;
    }
    if !row.floating && row.frame_authority == tiler_core::world_report::ScopeAuthority::Controlled
    {
        let Some(target) = row.effective_frame else {
            return false;
        };
        let Some(live) = state.readback_frame(me, fulls, mover_hwnd) else {
            return false;
        };
        return live == target && output_for_rect(areas, &live) == device;
    }
    let Some((wire, workspace)) = state.applied_homing.get(mover_token) else {
        return false;
    };
    if wire != &row.output.0 || workspace != &row.workspace {
        return false;
    }
    if row.floating {
        return true;
    }
    let Some(live) = state.readback_frame(me, fulls, mover_hwnd) else {
        return false;
    };
    output_for_rect(areas, &live) == device
}
