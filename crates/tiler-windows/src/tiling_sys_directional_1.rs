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
    let outputs: Vec<crate::workspace_owner::DirectionalOutput> = areas
        .iter()
        .map(|area| crate::workspace_owner::DirectionalOutput {
            id: area.device.clone(),
            full: area.full,
        })
        .collect();
    crate::workspace_owner::output_at_point(&outputs, x, y)
}

fn foreground_output(areas: &[MonitorArea]) -> Option<String> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return None;
    }
    let mut raw: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(foreground, &mut raw) } == 0 {
        return None;
    }
    rect_from_win(raw).map(|r| output_for_rect(areas, &r))
}

/// Exact output context for a digit chord: cached active output when known,
/// else the foreground monitor, else the pointer monitor, else ordering.
pub(super) fn chord_output(state: &TileLoop, areas: &[MonitorArea]) -> Option<String> {
    let cached = if state.active_output.is_empty() {
        None
    } else {
        Some(state.active_output.as_str())
    };
    let foreground = foreground_output(areas);
    let pointer = cursor_pos().and_then(|(x, y)| output_for_point(areas, x, y));
    crate::workspace_owner::output_context(
        &state.workspaces,
        cached,
        foreground.as_deref(),
        pointer.as_deref(),
    )
}

/// Engine revision for one `(output, workspace)` domain.
pub(super) fn revision_for(state: &TileLoop, output: &str, workspace: &str) -> u64 {
    let key = tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
    };
    state
        .engine
        .session(&key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0)
}

/// Portable domain value for one workspace on its monitor's work area.
/// Gaps ride the owner's live settings (inner gap in the domain, outer gap
/// in both the inset bounds and the carried event value).
pub(super) fn workspace_domain_for(
    output: &str,
    workspace: &str,
    areas: &[MonitorArea],
    inner_gap: i32,
    outer_gap: i32,
) -> Option<(
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
)> {
    let area = areas.iter().find(|a| a.device == output)?;
    let bounds = tiling_domain_bounds_with(area.work, outer_gap)?;
    Some(crate::workspace_owner::workspace_domain(
        output, workspace, bounds, inner_gap,
    ))
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
) -> Vec<crate::workspace_owner::DirectionalOutput> {
    areas
        .iter()
        .map(|area| crate::workspace_owner::DirectionalOutput {
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
    if is_focus && !crate::workspace_owner::directional_focus_crosses(direction) {
        return None;
    }
    let outputs = directional_outputs_from_areas(areas);
    let selected = crate::workspace_owner::select_directional_target(
        direction,
        source_output,
        mover_rect,
        &outputs,
    )
    .ok()??;
    if selected == source_output {
        return None;
    }
    let target_workspace = state.workspaces.active_id(&selected)?;
    // Modes beyond tiled stay out of scope: fall back to the local path so
    // an exhausted edge reports the honest local noop instead of crossing
    // into a floating layout.
    if !state.workspaces.is_tiled(&selected, &target_workspace) {
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
) -> Option<crate::workspace_owner::DirectionalFenceSnapshot> {
    let source_area = areas.iter().find(|a| a.device == source_output)?;
    let target_area = areas.iter().find(|a| a.device == target_output)?;
    let source_bounds = tiling_domain_bounds_with(source_area.work, state.outer_gap)?;
    let target_bounds = tiling_domain_bounds_with(target_area.work, state.outer_gap)?;
    Some(crate::workspace_owner::DirectionalFenceSnapshot {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_output: source_output.to_owned(),
        source_workspace: source_workspace.to_owned(),
        target_output: target_output.to_owned(),
        target_workspace: target_workspace.to_owned(),
        source_tiled: state.workspaces.is_tiled(source_output, source_workspace),
        target_tiled: state.workspaces.is_tiled(target_output, target_workspace),
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
    snapshot: &crate::workspace_owner::DirectionalFenceSnapshot,
    mover_key: &crate::workspace::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::workspace_owner::DirectionalFenceLive {
    // The direction selects the candidate only; fence evaluation is
    // direction-free, so the drag-drop snapshot carries the shared
    // computation (single mapping in `drag_snapshot_for_directional`).
    fence_live_impl(
        state,
        areas,
        &crate::workspace_owner::drag_snapshot_for_directional(snapshot),
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
    snapshot: &crate::workspace_owner::DragDropFenceSnapshot,
    mover_key: &crate::workspace::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::workspace_owner::DirectionalFenceLive {
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
    snapshot: &crate::workspace_owner::DragDropFenceSnapshot,
    mover_key: &crate::workspace::WindowKey,
    membership_ok: bool,
    overlay_maximized: bool,
    overlay_fullscreen: bool,
) -> crate::workspace_owner::DirectionalFenceLive {
    let source_active = state.workspaces.active_id(&snapshot.source_output);
    let target_active = state.workspaces.active_id(&snapshot.target_output);
    let lifetime_ok = state.member_tags.get(mover_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(
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
    let owner_ok = state
        .engine
        .owner()
        .is_some_and(|owner| owner.as_str() == snapshot.owner.as_str())
        && state
            .engine
            .generation()
            .is_some_and(|generation| generation.as_str() == snapshot.generation.as_str());
    crate::workspace_owner::DirectionalFenceLive {
        inner_gap: state.inner_gap,
        outer_gap: state.outer_gap,
        source_tiled: state
            .workspaces
            .is_tiled(&snapshot.source_output, &snapshot.source_workspace),
        target_tiled: state
            .workspaces
            .is_tiled(&snapshot.target_output, &snapshot.target_workspace),
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

/// Verify the cross-output arrival after a combined geometry apply: the
/// mover's desired Engine rect must read back on the destination monitor.
/// Uses the just-published managed observation (`state.stable`, refreshed by
/// the apply's internal readback), never the setter result. Physical
/// coordinates throughout; no rescaling.
pub(super) fn directional_arrival_verified(
    state: &TileLoop,
    areas: &[MonitorArea],
    reply: &CoreReply,
    mover_token: &str,
    mover_hwnd: u64,
    target_output: &str,
) -> bool {
    let Some(writes) = crate::workspace_owner::planned_writes(reply) else {
        return false;
    };
    let Some(desired) = writes.iter().find(|w| w.window.0 == mover_token) else {
        return false;
    };
    let effective = if desired.overconstrained {
        let hints = state
            .hint_logged
            .get(mover_token)
            .copied()
            .unwrap_or(WindowSizeHints::none());
        crate::tiling::overconstrained_effective(desired.rect, hints)
    } else {
        desired.rect
    };
    let Some(visible) = state.stable.get(&mover_hwnd).copied() else {
        return false;
    };
    if visible != effective {
        return false;
    }
    output_for_rect(areas, &visible) == target_output
}
