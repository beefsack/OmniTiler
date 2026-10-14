use super::*;

#[cfg(test)]
use super::tiling_sys_pending_seams::{INJECTED_ARRIVAL_PLACEMENT, INJECTED_PLACEMENT_NORMALS};

/// Whether a window's `WINDOWPLACEMENT` coordinates ride workspace
/// coordinates: a top-level window without `WS_EX_TOOLWINDOW` per the
/// `WINDOWPLACEMENT` contract. Everything else rides screen coordinates.
/// Workspace coordinates are screen coordinates minus the per-monitor taskbar
/// inset delta (`WORK.left - FULL.left`, `WORK.top - FULL.top`), so callers
/// convert with the owning monitor's `WORK` plus `FULL` rects; screen
/// coordinates pass through untouched.
#[cfg(not(test))]
pub(super) fn placement_is_workspace_coords(exstyle: u32, top_level: bool) -> bool {
    top_level && exstyle & WS_EX_TOOLWINDOW == 0
}

/// Convert a screen frame to the raw `rcNormalPosition` basis: subtract the
/// taskbar inset delta (`WORK - FULL`) for workspace coordinates, identity
/// for screen (tool windows). The virtual-desktop monitor origin is retained,
/// so a non-primary raw rect keeps its `FULL.left/top` bias. Preserves size;
/// saturating so extreme coordinates never wrap.
pub(super) fn screen_to_placement_raw(
    screen: Rect,
    work: Rect,
    full: Rect,
    workspace_coords: bool,
) -> Rect {
    if workspace_coords {
        let dx = work.x.saturating_sub(full.x);
        let dy = work.y.saturating_sub(full.y);
        Rect {
            x: screen.x.saturating_sub(dx),
            y: screen.y.saturating_sub(dy),
            w: screen.w,
            h: screen.h,
        }
    } else {
        screen
    }
}

/// Convert a raw `rcNormalPosition` rect back to screen coordinates: add the
/// taskbar inset delta (`WORK - FULL`) for workspace coordinates, identity
/// for screen (tool windows). Preserves size.
pub(super) fn placement_raw_to_screen(
    raw: Rect,
    work: Rect,
    full: Rect,
    workspace_coords: bool,
) -> Rect {
    if workspace_coords {
        let dx = work.x.saturating_sub(full.x);
        let dy = work.y.saturating_sub(full.y);
        Rect {
            x: raw.x.saturating_add(dx),
            y: raw.y.saturating_add(dy),
            w: raw.w,
            h: raw.h,
        }
    } else {
        raw
    }
}

/// Translate a source raw normal rect to the target raw normal rect: raw to
/// source screen via the source inset delta, FULL-origin delta to the target
/// screen, screen to target raw via the target inset delta. Preserves the
/// source restore size; the caller preserves `showCmd`.
pub(super) fn translate_placement_normal_to_target(
    source_raw: Rect,
    source_work: Rect,
    target_work: Rect,
    source_full: Rect,
    target_full: Rect,
    workspace_coords: bool,
) -> Rect {
    let source_screen =
        placement_raw_to_screen(source_raw, source_work, source_full, workspace_coords);
    let target_screen = Rect {
        x: source_screen
            .x
            .saturating_sub(source_full.x)
            .saturating_add(target_full.x),
        y: source_screen
            .y
            .saturating_sub(source_full.y)
            .saturating_add(target_full.y),
        w: source_screen.w,
        h: source_screen.h,
    };
    screen_to_placement_raw(target_screen, target_work, target_full, workspace_coords)
}

/// Live placement normal rect plus its coordinate basis (test seam only):
/// the injected normal boundary distinctly from pre-state. Production uses
/// the single-snapshot remap below, so this helper is test-only.
#[cfg(test)]
pub(super) fn live_placement_normal(hwnd_u64: u64) -> Option<(Rect, bool)> {
    INJECTED_PLACEMENT_NORMALS.with(|map| map.borrow().get(&hwnd_u64).copied())
}

/// Live placement coordinate basis for one window (no geometry read): whether
/// its `rcNormalPosition` rides workspace coordinates. Production reads the
/// live extended style plus parent; test reads the injected normal flag when
/// scripted and defaults to workspace (the common non-tool top-level case)
/// so existing arrival-only fixtures keep proving without new scripting.
pub(super) fn live_placement_workspace_coords(hwnd_u64: u64) -> bool {
    #[cfg(test)]
    {
        INJECTED_PLACEMENT_NORMALS
            .with(|map| map.borrow().get(&hwnd_u64).map(|(_, basis)| *basis))
            .unwrap_or(true)
    }
    #[cfg(not(test))]
    {
        let hwnd = hwnd_u64 as isize as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return true;
        }
        let exstyle = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        let top_level = unsafe { GetParent(hwnd) }.is_null();
        placement_is_workspace_coords(exstyle, top_level)
    }
}

/// Fresh post-setter concealed placement readback (causal, never translation
/// math): the live `WINDOWPLACEMENT` normal rect after the actual setter,
/// converted back to screen coordinates for the physical output proof.
/// `work` is the target monitor's `WORK` rect and `workspace_coords` is the
/// live placement basis (non-tool top-level rides workspace coordinates per
/// the `WINDOWPLACEMENT` contract, tool windows ride screen). Under test
/// reads the injected screen placement boundary distinctly from pre-state
/// (`None` when unscripted). In production reads `GetWindowPlacement`
/// without touching show-state and converts workspace to screen.
pub(super) fn fresh_arrival_placement(
    hwnd: u64,
    work: Rect,
    full: Rect,
    workspace_coords: bool,
) -> Option<Rect> {
    #[cfg(test)]
    {
        let _ = (work, full, workspace_coords);
        INJECTED_ARRIVAL_PLACEMENT.with(|map| map.borrow().get(&hwnd).copied())
    }
    #[cfg(not(test))]
    {
        let hwnd_native = hwnd as isize as HWND;
        if unsafe { IsWindow(hwnd_native) } == 0 {
            return None;
        }
        let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if unsafe { GetWindowPlacement(hwnd_native, &mut placement) } == 0 {
            return None;
        }
        let w = placement
            .rcNormalPosition
            .right
            .saturating_sub(placement.rcNormalPosition.left);
        let h = placement
            .rcNormalPosition
            .bottom
            .saturating_sub(placement.rcNormalPosition.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let raw = Rect {
            x: placement.rcNormalPosition.left,
            y: placement.rcNormalPosition.top,
            w,
            h,
        };
        Some(placement_raw_to_screen(raw, work, full, workspace_coords))
    }
}
