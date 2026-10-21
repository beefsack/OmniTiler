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

/// Core revision for log correlation (the per-domain Engine revision no
/// longer exists; commands carry no revision binding).
pub(super) fn revision_for(state: &TileLoop, _output: &str, _workspace: &str) -> u64 {
    state.core_revision()
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
/// FULL-rectangle outputs for the shared window selector.
pub(super) fn directional_outputs_from_areas(
    areas: &[MonitorArea],
) -> Vec<crate::native_guards::DirectionalOutput> {
    areas
        .iter()
        .map(|area| crate::native_guards::DirectionalOutput {
            id: area.device.clone(),
            full: area.full,
        })
        .collect()
}

/// Resolve the cross-output target for one directional intent.
///
/// Returns `None` for the local path: no candidate, floating target,
/// vertical focus which never crosses, or unreadable/unknown destination
/// topology. Local-first semantics require this: an unreadable destination
/// must never refuse an otherwise valid local command; the exhausted edge
/// then reports the honest local noop.
pub(super) fn resolve_directional_target(
    state: &TileLoop,
    areas: &[MonitorArea],
    source_output: &str,
    direction: tiler_core::directional::Direction,
    mover_rect: &Rect,
    is_focus: bool,
) -> Option<(String, String)> {
    if is_focus && !crate::native_guards::directional_focus_crosses(direction) {
        return None;
    }
    let outputs = directional_outputs_from_areas(areas);
    let selected = crate::native_guards::select_directional_target(
        direction,
        source_output,
        mover_rect,
        &outputs,
    )
    .ok()??;
    if selected == source_output {
        return None;
    }
    let target_workspace = state.active_view(&selected)?;
    // Modes beyond tiled stay out of scope: fall back to the local path so
    // an exhausted edge reports the honest local noop instead of crossing
    // into a floating layout.
    if !state.is_tiled_view(&selected, &target_workspace) {
        return None;
    }
    Some((selected, target_workspace))
}

/// Snapshot the directional fences at pair-build time, including the live
/// FULL rectangles, inset domain bounds, and owner binding so later tails
/// detect topology or binding drift.
#[allow(clippy::too_many_arguments)]
pub(super) fn directional_snapshot(
    state: &TileLoop,
    areas: &[MonitorArea],
    source_output: &str,
    source_workspace: &str,
    target_output: &str,
    target_workspace: &str,
    direction: tiler_core::directional::Direction,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> Option<crate::native_guards::DirectionalFenceSnapshot> {
    let source_area = areas.iter().find(|a| a.device == source_output)?;
    let target_area = areas.iter().find(|a| a.device == target_output)?;
    let source_bounds = tiling_domain_bounds_with(source_area.work, state.outer_gap)?;
    let target_bounds = tiling_domain_bounds_with(target_area.work, state.outer_gap)?;
    Some(crate::native_guards::DirectionalFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: source_output.to_owned(),
        source_workspace: source_workspace.to_owned(),
        target_output: target_output.to_owned(),
        target_workspace: target_workspace.to_owned(),
        source_tiled: state.is_tiled_view(source_output, source_workspace),
        target_tiled: state.is_tiled_view(target_output, target_workspace),
        direction,
        overlay_maximized,
        overlay_fullscreen,
        source_full: source_area.full,
        target_full: target_area.full,
        source_bounds,
        target_bounds,
        owner: state.owner.as_str().to_owned(),
        generation: state.generation.as_str().to_owned(),
    })
}

/// Read the live directional fences with fresh native state at a meaningful
/// boundary (before Engine dispatch, before actuation). Scope, lifetime,
/// overlay, views, gaps, modes, topology, and owner binding are all
/// re-read here; callers pass only `membership_ok`, whose semantics differ
/// before and after the project membership moves.
pub(super) fn directional_live(
    state: &TileLoop,
    areas: &[MonitorArea],
    snapshot: &crate::native_guards::DirectionalFenceSnapshot,
    mover_key: &crate::native_guards::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::native_guards::DirectionalFenceLive {
    // The direction selects the candidate only; fence evaluation is
    // direction-free, so the drag-drop snapshot carries the shared
    // computation (single mapping in `drag_snapshot_for_directional`).
    fence_live_impl(
        state,
        areas,
        &crate::native_guards::drag_snapshot_for_directional(snapshot),
        mover_key,
        membership_ok,
        overlay_maximized,
        overlay_fullscreen,
    )
}

/// Read the live drag-drop fences: same shared computation as
/// [`directional_live`] over the pointer-resolved (non-adjacency) snapshot.
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

/// Verify the cross-output arrival after a world-routed apply: the mover's
/// desired core allocation must read back on the destination monitor.
/// Uses the just-published managed observation (`state.stable`, refreshed by
/// the apply's internal readback), never the setter result.
pub(super) fn directional_arrival_verified(
    state: &TileLoop,
    areas: &[MonitorArea],
    reply: &tiler_core::world_report::WorldReply,
    mover_token: &str,
    mover_hwnd: u64,
    target_output: &str,
) -> bool {
    let Some(desired) = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == mover_token)
        .and_then(|w| w.allocation)
    else {
        return false;
    };
    let Some(visible) = state.stable.get(&mover_hwnd).copied() else {
        return false;
    };
    if visible != desired {
        return false;
    }
    output_for_rect(areas, &visible) == target_output
}
