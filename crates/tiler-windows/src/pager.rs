#![forbid(unsafe_code)]
//! Taskbar pager: per-output workspace strip (portable policy, no Win32).
//!
//! The pager renders every workspace of an output as one numbered cell in a
//! small strip docked inside that output's taskbar, left of the tray clock
//! area: the active cell carries the accent, empty cells stay darker, and each
//! cell carries miniature lighter window rectangles scaled from the real
//! workspace rectangles plus a light 1px border. Clicking a cell selects that
//! exact workspace through the existing `workspace_do_select` resolver.
//!
//! This module holds only portable geometry, safety policy, and the
//! output-scoped select fence. Native effects (overlay windows, Explorer
//! rediscovery, `TaskbarCreated` retry, z-order reassert, painting, click
//! capture) live in `crate::pager_sys` (`cfg(windows)` only); the loop host
//! (`crate::tiling_sys::pager_host`) wires the two. Miniature icons arrive as
//! worker-owned opaque pixel buffers (see [`crate::pager_icons`]): each
//! miniature carries its full window identity plus optional artwork, and the
//! repaint hash covers both, so artwork changes repaint without touching
//! geometry, labels, selection, or safety gates.
//!
//! Safety contract (see `pager_suppression` and `strip_for_taskbar`):
//! - The overlay may overlap applications but NEVER the tray: the strip is
//!   refused unless it fits fully left of the tray area with a 2px gap, fully
//!   inside the taskbar rect, and fully inside the monitor rect (this also
//!   fails closed for auto-hidden/off-screen taskbars, whose rects move
//!   off-screen when hidden).
//! - Only a horizontal bottom-docked taskbar is served; top/left/right
//!   taskbars refuse with a reason instead of guessing an anchor.
//! - Secondary taskbars (`Shell_SecondaryTrayWnd`) refuse with
//!   `secondary-tray-unproven`: their clock area has no stable child to
//!   anchor against, so tray exclusion cannot be proven.
//! - `NoRoom` is a geometry-invalid bound, not app-icon crowding: cells are a
//!   fixed physical width per workspace, so ordinary taskbar crowding (many
//!   app icons, many windows per workspace) never changes the layout. When
//!   the workspace count itself cannot fit left of the tray, the whole strip
//!   refuses instead of compressing cells, truncating workspaces, or touching
//!   the taskbar.
//! - Fullscreen/game suppression is fail-closed: any suspend veto, elevated
//!   foreground, or unreadable fact hides every strip.

use std::collections::{BTreeMap, HashMap};

use tiler_core::geometry::Rect;

use crate::tray_pager::SelectScope;

const fn right(rect: Rect) -> i32 {
    rect.x.saturating_add(rect.w)
}

const fn bottom(rect: Rect) -> i32 {
    rect.y.saturating_add(rect.h)
}

/// Fixed cell width at 96 DPI (probe-validated 3-cell 216px strip: 3 x 72).
/// Physical cell width scales with the taskbar DPI (see `dpi_cell_w`).
pub const PAGER_CELL_W_96: i32 = 72;
/// Base DPI for cell geometry. The native side reads the live taskbar DPI
/// per pump; 96 is only the fallback when that query fails, never an
/// assumption about the monitor.
pub const PAGER_BASE_DPI: u32 = 96;
/// Gap between the strip's right edge and the tray area (physical px): keeps
/// overlay pixels off the tray clock area with a visible margin.
pub const PAGER_TRAY_GAP: i32 = 2;
/// Minimum strip height at 96 DPI that still paints numbers legibly.
pub const PAGER_MIN_STRIP_H_96: i32 = 24;
/// Maximum strip height at 96 DPI (taller taskbars keep the strip compact).
pub const PAGER_MAX_STRIP_H_96: i32 = 48;

/// Scale one 96-DPI layout px to physical px at `dpi` (rounded). Gaps and
/// 1px borders stay physical by construction (never passed through here).
/// The 1..16384 clamp rejects zero/overflow geometry from corrupt DPI reads.
#[must_use]
pub fn dpi_scale(px_96: i32, dpi: u32) -> i32 {
    let dpi = dpi.max(1) as i64;
    ((px_96 as i64 * dpi + (PAGER_BASE_DPI as i64 / 2)) / PAGER_BASE_DPI as i64).clamp(1, 16384)
        as i32
}

/// Physical cell width at `dpi`.
#[must_use]
pub fn dpi_cell_w(dpi: u32) -> i32 {
    dpi_scale(PAGER_CELL_W_96, dpi)
}

/// Physical strip-height bounds at `dpi`.
#[must_use]
pub fn dpi_strip_h_range(dpi: u32) -> (i32, i32) {
    (
        dpi_scale(PAGER_MIN_STRIP_H_96, dpi),
        dpi_scale(PAGER_MAX_STRIP_H_96, dpi),
    )
}

/// Portable mirror of one monitor's geometry for pager layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputGeometry {
    pub device: String,
    pub full: Rect,
    pub work: Rect,
}

/// One discovered taskbar: its rect, the tray-area left edge when proven
/// (`None` when the tray area is unknown), its kind, live visibility, and
/// the live taskbar DPI (96 only when the native query fails).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarFacts {
    pub rect: Rect,
    pub tray_left: Option<i32>,
    pub kind: TaskbarKind,
    pub visible: bool,
    pub dpi: u32,
}

/// Primary taskbars prove tray exclusion through `TrayNotifyWnd`;
/// secondaries cannot and always refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarKind {
    Primary,
    Secondary,
}

/// Why one output got no strip (bounded log vocabulary, no HWNDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerRefusal {
    TaskbarHidden,
    SecondaryTrayUnproven,
    NotBottomDocked,
    TrayUnknown,
    TrayOutsideBar,
    BarTooShort,
    NoRoom,
    OffScreen,
}

impl PagerRefusal {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskbarHidden => "taskbar-hidden",
            Self::SecondaryTrayUnproven => "secondary-tray-unproven",
            Self::NotBottomDocked => "not-bottom-docked",
            Self::TrayUnknown => "tray-unknown",
            Self::TrayOutsideBar => "tray-outside-bar",
            Self::BarTooShort => "bar-too-short",
            Self::NoRoom => "no-room",
            Self::OffScreen => "off-screen",
        }
    }
}

/// One painted cell: its rect, active flag, 1-based ordinal, the window
/// miniatures already scaled into cell coordinates, and per-miniature icon
/// state parallel to `minis`: the full window identity plus optional owned
/// artwork. Missing artwork is `None` and leaves the rectangle unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerCell {
    pub rect: Rect,
    pub active: bool,
    pub ordinal: u32,
    pub minis: Vec<Rect>,
    pub mini_keys: Vec<crate::workspace::WindowKey>,
    pub mini_icons: Vec<Option<std::sync::Arc<crate::pager_icons::IconImage>>>,
}

/// One output's strip: the strip rect plus its cells in workspace order
/// (trailing empty included, like the tray menu pager).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPager {
    pub output: String,
    pub strip: Rect,
    /// Workspace ids in cell order (parallel to `cells`).
    pub ids: Vec<String>,
    pub cells: Vec<PagerCell>,
    pub active_index: usize,
    /// Taskbar DPI the frame was laid out for (font + geometry).
    pub dpi: u32,
}

/// Strip layout for one output on a bottom-docked horizontal taskbar.
/// `tray_left` is the proven tray-area left edge; the strip ends
/// `PAGER_TRAY_GAP` physical px left of it. Cell width and strip height scale
/// with the taskbar DPI. Returns the refusal instead of guessing when the
/// taskbar is hidden, not bottom-docked, the tray edge is unknown or outside
/// the bar, the bar is too short for a legible strip, the cells do not fit
/// left of the tray, or the strip would leave the monitor or the bar.
pub fn strip_for_taskbar(
    monitor_full: Rect,
    taskbar: &TaskbarFacts,
    cell_count: usize,
) -> Result<Rect, PagerRefusal> {
    if !taskbar.visible {
        return Err(PagerRefusal::TaskbarHidden);
    }
    if taskbar.kind == TaskbarKind::Secondary {
        return Err(PagerRefusal::SecondaryTrayUnproven);
    }
    let bar = taskbar.rect;
    let horizontal = bar.w >= bar.h.saturating_mul(2);
    let bottom_docked = bottom(bar) == bottom(monitor_full) && bar.w <= monitor_full.w;
    if !horizontal || !bottom_docked {
        return Err(PagerRefusal::NotBottomDocked);
    }
    let Some(tray_left) = taskbar.tray_left else {
        return Err(PagerRefusal::TrayUnknown);
    };
    // The tray child must live inside its own bar: an expanded or detached
    // tray rect (stale facts, auto-hide drift) refuses instead of anchoring
    // the overlay over the tray.
    if tray_left < bar.x || tray_left > right(bar) {
        return Err(PagerRefusal::TrayOutsideBar);
    }
    let cell_w = dpi_cell_w(taskbar.dpi);
    let strip_w = cell_count as i32 * cell_w;
    if strip_w <= 0 {
        return Err(PagerRefusal::NoRoom);
    }
    let (min_h, max_h) = dpi_strip_h_range(taskbar.dpi);
    // A bar shorter than one legible strip cannot host the overlay: clamping
    // would push pixels outside the bar, so refuse instead.
    if bar.h < min_h {
        return Err(PagerRefusal::BarTooShort);
    }
    let strip_h = bar.h.clamp(min_h, max_h);
    let right_edge = tray_left - PAGER_TRAY_GAP;
    let left = right_edge - strip_w;
    if left < bar.x {
        return Err(PagerRefusal::NoRoom);
    }
    let top = bar.y + (bar.h - strip_h) / 2;
    let strip = Rect {
        x: left,
        y: top,
        w: strip_w,
        h: strip_h,
    };
    // Never overlap the tray, never leave the bar vertically, and never
    // leave the monitor (auto-hidden taskbars report off-screen rects, which
    // refuse here).
    if strip.x < monitor_full.x
        || strip.y < monitor_full.y
        || right(strip) > right(monitor_full)
        || bottom(strip) > bottom(monitor_full)
        || strip.y < bar.y
        || bottom(strip) > bottom(bar)
    {
        return Err(PagerRefusal::OffScreen);
    }
    Ok(strip)
}

/// Split one strip into per-workspace cells in order.
#[must_use]
pub fn cells_for_strip(strip: Rect, cell_count: usize, cell_w: i32) -> Vec<Rect> {
    (0..cell_count)
        .map(|index| Rect {
            x: strip.x + index as i32 * cell_w,
            y: strip.y,
            w: cell_w,
            h: strip.h,
        })
        .collect()
}

/// Hit-test one click point against the cells: the 0-based cell index.
#[must_use]
pub fn cell_at(cells: &[Rect], x: i32, y: i32) -> Option<usize> {
    cells
        .iter()
        .position(|cell| x >= cell.x && x < right(*cell) && y >= cell.y && y < bottom(*cell))
}

/// Scale one real workspace rectangle (monitor-full coordinates) into a cell
/// miniature. The cell inset (scaled with the taskbar DPI) keeps the light
/// border and the number readable; degenerate results clamp to a 2px physical
/// minimum inside the inset.
#[must_use]
pub fn miniature(cell: Rect, monitor_full: Rect, window: Rect, dpi: u32) -> Rect {
    let pad = dpi_scale(4, dpi);
    let inset = Rect {
        x: cell.x + pad,
        y: cell.y + pad,
        w: (cell.w - pad.saturating_mul(2)).max(1),
        h: (cell.h - pad.saturating_mul(3)).max(1),
    };
    if monitor_full.w <= 0 || monitor_full.h <= 0 || window.w <= 0 || window.h <= 0 {
        return Rect {
            x: inset.x,
            y: inset.y,
            w: 2,
            h: 2,
        };
    }
    let scale = (inset.w as f64 / monitor_full.w as f64)
        .min(inset.h as f64 / monitor_full.h as f64)
        .clamp(0.0, 1.0);
    let w = ((window.w as f64 * scale).round() as i32).clamp(2, inset.w);
    let h = ((window.h as f64 * scale).round() as i32).clamp(2, inset.h);
    let x = (inset.x as f64 + (window.x - monitor_full.x) as f64 * scale).round() as i32;
    let y = (inset.y as f64 + (window.y - monitor_full.y) as f64 * scale).round() as i32;
    Rect {
        x: x.clamp(inset.x, right(inset) - w),
        y: y.clamp(inset.y, bottom(inset) - h),
        w,
        h,
    }
}

/// Match each discovered bar to the monitor it overlaps most. Returns
/// `(geo_index, bar_index)` pairs; bars overlapping no monitor match nothing
/// and are ignored.
#[must_use]
pub fn match_bars_to_outputs(
    geos: &[OutputGeometry],
    bars: &[TaskbarFacts],
) -> Vec<(usize, usize)> {
    let mut matched = Vec::new();
    for (bar_index, bar) in bars.iter().enumerate() {
        let mut best: Option<(usize, i64)> = None;
        for (geo_index, geo) in geos.iter().enumerate() {
            let overlap = overlap_area(geo.full, bar.rect);
            if overlap > 0 && best.is_none_or(|(_, area)| overlap > area) {
                best = Some((geo_index, overlap));
            }
        }
        if let Some((geo_index, _)) = best {
            matched.push((geo_index, bar_index));
        }
    }
    matched
}

fn overlap_area(a: Rect, b: Rect) -> i64 {
    let w = (right(a).min(right(b)) - a.x.max(b.x)).max(0) as i64;
    let h = (bottom(a).min(bottom(b)) - a.y.max(b.y)).max(0) as i64;
    w * h
}

/// Resolve one member's pager rectangle by exact member key, newest first:
/// 1. the live identity-verified frame from the latest observation (visible
///    tiled members, floats, kept maximized/fullscreen overlays, and
///    slotless born-fullscreen holds: all observed with fresh frames);
/// 2. the live float frame (floats between observations);
/// 3. the saved tile slot / hide-time snapshot (hidden members, minimized
///    members, and slotless holds that are currently hidden: born holds seed
///    a hidden snapshot at admission for exactly this).
///
/// Members with no rectangle anywhere contribute no miniature, never a
/// guess. This reads only owned managed identities and never mutates the
/// tiling `member_rects`/`float_rects` tables.
#[must_use]
pub fn resolve_member_rect(
    key: &crate::workspace::WindowKey,
    live: &HashMap<crate::workspace::WindowKey, Rect>,
    member_tokens: &BTreeMap<crate::workspace::WindowKey, String>,
    member_rects: &HashMap<String, Rect>,
    float_rects: &HashMap<String, Rect>,
) -> Option<Rect> {
    if let Some(rect) = live.get(key).copied() {
        return Some(rect);
    }
    let token = member_tokens.get(key)?;
    float_rects
        .get(token)
        .or_else(|| member_rects.get(token))
        .copied()
}

/// Build one output's pager frame: the strip plus one cell per workspace in
/// order with one miniature per actual managed window (no cap: every member
/// renders; ordinary crowding only adds miniatures, never changes cells).
/// `icons` maps full member identities to worker-owned artwork; members
/// without artwork render the plain rectangle. Returns the refusal when the
/// strip cannot anchor safely.
#[allow(clippy::too_many_arguments)]
pub fn build_output_pager(
    spaces: &crate::workspace::ManagedWorkspaces,
    live: &HashMap<crate::workspace::WindowKey, Rect>,
    member_tokens: &BTreeMap<crate::workspace::WindowKey, String>,
    member_rects: &HashMap<String, Rect>,
    float_rects: &HashMap<String, Rect>,
    icons: &HashMap<crate::workspace::WindowKey, std::sync::Arc<crate::pager_icons::IconImage>>,
    output: &str,
    monitor_full: Rect,
    taskbar: &TaskbarFacts,
) -> Result<OutputPager, PagerRefusal> {
    let ids = spaces.workspace_ids(output);
    let active_id = spaces.active_id(output);
    let strip = strip_for_taskbar(monitor_full, taskbar, ids.len())?;
    let cell_rects = cells_for_strip(strip, ids.len(), dpi_cell_w(taskbar.dpi));
    let active_index = active_id
        .as_deref()
        .and_then(|active| ids.iter().position(|id| id == active))
        .unwrap_or(0);
    let mut cells = Vec::with_capacity(ids.len());
    for (index, cell_rect) in cell_rects.into_iter().enumerate() {
        let workspace = &ids[index];
        let mut minis = Vec::new();
        let mut mini_keys = Vec::new();
        let mut mini_icons = Vec::new();
        for key in spaces.workspace_members(output, workspace) {
            if let Some(rect) =
                resolve_member_rect(&key, live, member_tokens, member_rects, float_rects)
            {
                minis.push(miniature(cell_rect, monitor_full, rect, taskbar.dpi));
                mini_icons.push(icons.get(&key).cloned());
                mini_keys.push(key);
            }
        }
        cells.push(PagerCell {
            rect: cell_rect,
            active: index == active_index,
            ordinal: index as u32 + 1,
            minis,
            mini_keys,
            mini_icons,
        });
    }
    Ok(OutputPager {
        output: output.to_owned(),
        strip,
        ids,
        cells,
        active_index,
        dpi: taskbar.dpi,
    })
}

/// FNV-1a hash of one snapshot for the change-gated repaint fence: steady
/// state paints nothing.
#[must_use]
pub fn snapshot_hash(pagers: &[OutputPager]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |word: u64| {
        hash ^= word;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    for pager in pagers {
        for byte in pager.output.bytes() {
            mix(u64::from(byte));
        }
        for id in &pager.ids {
            for byte in id.bytes() {
                mix(u64::from(byte));
            }
            mix(0xfe);
        }
        mix(pager.strip.x as u64);
        mix(pager.strip.y as u64);
        mix(pager.strip.w as u64);
        mix(pager.strip.h as u64);
        mix(pager.dpi as u64);
        mix(pager.active_index as u64);
        for cell in &pager.cells {
            mix(cell.rect.x as u64);
            mix(cell.rect.y as u64);
            mix(cell.ordinal as u64);
            mix(cell.active as u64);
            for mini in &cell.minis {
                mix(mini.x as u64);
                mix(mini.y as u64);
                mix(mini.w as u64);
                mix(mini.h as u64);
            }
            // Window identity per miniature (full key, never HWND alone) plus
            // artwork presence: icon arrival/refresh repaints without moving
            // any pixel of geometry.
            for (key, icon) in cell.mini_keys.iter().zip(cell.mini_icons.iter()) {
                crate::pager_icons::mix_mini_icon(&mut mix, key, icon.as_deref());
            }
        }
        mix(0xff);
    }
    hash
}

/// Live gate inputs for pager visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagerGate {
    /// Settings opt-out (`core.taskbar_pager`, default on).
    pub enabled: bool,
    /// Fullscreen/game suspend veto (managed-overlay exception included).
    pub suspended: bool,
    /// Elevated foreground or unreadable foreground facts.
    pub elevated: bool,
    /// Any other unreadable fact (no monitors, no taskbar read).
    pub unknown: bool,
}

/// Suppression reason when the strips must hide, or `None` when they may
/// show. Fail-closed: any veto hides every strip.
#[must_use]
pub const fn pager_suppression(gate: PagerGate) -> Option<&'static str> {
    if !gate.enabled {
        Some("disabled-setting")
    } else if gate.suspended {
        Some("suspended-fullscreen")
    } else if gate.elevated {
        Some("elevated-foreground")
    } else if gate.unknown {
        Some("unknown-facts")
    } else {
        None
    }
}

/// Fullscreen covering for the pager gate: stricter than tiling suspension.
/// Even a managed fullscreen overlay (which keeps tiling alive) covers the
/// pager, so a borderless game admitted as a member can never be painted over.
/// Shared by the visibility sync and the click-dispatch poll so both enforce
/// exactly the same covering.
#[must_use]
pub const fn pager_covering(blocked: bool, managed_overlay: bool) -> bool {
    blocked || managed_overlay
}

/// Verify one drained pager click: true only when the rendered output is
/// still known and the rendered workspace is still live on it. Unlike the
/// tray menu fence this never requires the output to be focused: overlay
/// clicks on a secondary output must work without focusing it first. The
/// 1-based ordinal is logging only and never retargets: an unknown id
/// refuses, never falls back to a positional guess. The already-active
/// workspace is NOT stale: selecting it is an honest no-op at dispatch.
#[must_use]
pub fn verify_output_scoped_select(
    rendered: &SelectScope,
    spaces: &crate::workspace::ManagedWorkspaces,
) -> bool {
    spaces
        .output_keys()
        .iter()
        .any(|key| key == &rendered.output)
        && spaces
            .workspace_ids(&rendered.output)
            .iter()
            .any(|id| id == &rendered.workspace)
}

/// Live settings accessor for the default-on opt-out.
#[must_use]
pub fn pager_enabled(settings: &crate::settings::Settings) -> bool {
    settings.core.taskbar_pager
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }
    }

    fn bottom_bar() -> TaskbarFacts {
        TaskbarFacts {
            rect: Rect {
                x: 0,
                y: 1040,
                w: 1920,
                h: 40,
            },
            tray_left: Some(1700),
            kind: TaskbarKind::Primary,
            visible: true,
            dpi: PAGER_BASE_DPI,
        }
    }

    #[test]
    fn strip_anchors_left_of_tray_with_gap() {
        let strip = strip_for_taskbar(full(), &bottom_bar(), 3).expect("fits");
        assert_eq!(strip.w, 3 * dpi_cell_w(PAGER_BASE_DPI));
        assert_eq!(right(strip), 1700 - PAGER_TRAY_GAP);
        assert!(strip.y >= 1040 && bottom(strip) <= 1080);
    }

    #[test]
    fn strip_refuses_instead_of_guessing() {
        // Hidden taskbar.
        let mut bar = bottom_bar();
        bar.visible = false;
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TaskbarHidden)
        );
        // Secondary tray area is unproven.
        let mut bar = bottom_bar();
        bar.kind = TaskbarKind::Secondary;
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::SecondaryTrayUnproven)
        );
        // Vertical taskbar is not served.
        let mut bar = bottom_bar();
        bar.rect = Rect {
            x: 0,
            y: 0,
            w: 60,
            h: 1080,
        };
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::NotBottomDocked)
        );
        // Unknown tray edge refuses.
        let mut bar = bottom_bar();
        bar.tray_left = None;
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TrayUnknown)
        );
        // Crowding refuses instead of compressing.
        assert_eq!(
            strip_for_taskbar(full(), &bottom_bar(), 30),
            Err(PagerRefusal::NoRoom)
        );
    }

    #[test]
    fn strip_validates_tray_bar_and_vertical_bounds() {
        // Off-screen strip (bar extends past the monitor edge while the
        // tray edge stays inside the bar): refuses instead of painting
        // outside the monitor.
        let mut bar = bottom_bar();
        bar.rect = Rect {
            x: -100,
            y: 1040,
            w: 1920,
            h: 40,
        };
        bar.tray_left = Some(100);
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::OffScreen)
        );
        // Tray edge outside its own bar (expanded/detached tray facts):
        // refuses instead of anchoring over the tray.
        let mut bar = bottom_bar();
        bar.tray_left = Some(1950);
        bar.rect = Rect {
            x: 0,
            y: 1040,
            w: 1900,
            h: 40,
        };
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TrayOutsideBar)
        );
        let mut bar = bottom_bar();
        bar.tray_left = Some(-50);
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TrayOutsideBar)
        );
        // Bar shorter than one legible strip: clamping would push pixels
        // outside the bar, so refuse instead.
        let mut bar = bottom_bar();
        bar.rect = Rect {
            x: 0,
            y: 1060,
            w: 1920,
            h: 20,
        };
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::BarTooShort)
        );
        // Tray edge exactly at the bar edges still anchors safely.
        let mut bar = bottom_bar();
        bar.tray_left = Some(1920);
        assert!(strip_for_taskbar(full(), &bar, 2).is_ok());
    }

    #[test]
    fn cells_split_in_order_and_hit_test() {
        let strip = strip_for_taskbar(full(), &bottom_bar(), 3).expect("fits");
        let cells = cells_for_strip(strip, 3, dpi_cell_w(PAGER_BASE_DPI));
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[1].x, cells[0].x + dpi_cell_w(PAGER_BASE_DPI));
        assert_eq!(cell_at(&cells, cells[0].x + 5, cells[0].y + 5), Some(0));
        assert_eq!(cell_at(&cells, cells[2].x + 71, cells[2].y + 5), Some(2));
        assert_eq!(cell_at(&cells, 10, 10), None);
    }

    #[test]
    fn miniatures_scale_from_real_rects() {
        let strip = strip_for_taskbar(full(), &bottom_bar(), 1).expect("fits");
        let cell = cells_for_strip(strip, 1, dpi_cell_w(PAGER_BASE_DPI))[0];
        // Full-monitor window fills the inset.
        let mini = miniature(
            cell,
            full(),
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            PAGER_BASE_DPI,
        );
        assert!(mini.w >= 2 && mini.h >= 2);
        assert!(mini.x >= cell.x && right(mini) <= right(cell));
        // Quarter window scales down proportionally.
        let small = miniature(
            cell,
            full(),
            Rect {
                x: 0,
                y: 0,
                w: 480,
                h: 270,
            },
            PAGER_BASE_DPI,
        );
        assert!(small.w <= mini.w && small.h <= mini.h);
        // Degenerate input clamps instead of vanishing.
        let flat = miniature(
            cell,
            full(),
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            PAGER_BASE_DPI,
        );
        assert_eq!((flat.w, flat.h), (2, 2));
    }

    #[test]
    fn bars_match_by_overlap() {
        let geos = vec![
            OutputGeometry {
                device: "a".to_owned(),
                full: full(),
                work: full(),
            },
            OutputGeometry {
                device: "b".to_owned(),
                full: Rect {
                    x: 1920,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
                work: Rect {
                    x: 1920,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
            },
        ];
        let bars = vec![bottom_bar()];
        assert_eq!(match_bars_to_outputs(&geos, &bars), vec![(0, 0)]);
        assert!(match_bars_to_outputs(&geos, &[]).is_empty());
    }

    #[test]
    fn suppression_is_fail_closed() {
        let open = PagerGate {
            enabled: true,
            suspended: false,
            elevated: false,
            unknown: false,
        };
        assert_eq!(pager_suppression(open), None);
        assert_eq!(
            pager_suppression(PagerGate {
                enabled: false,
                ..open
            }),
            Some("disabled-setting")
        );
        assert_eq!(
            pager_suppression(PagerGate {
                suspended: true,
                ..open
            }),
            Some("suspended-fullscreen")
        );
        assert_eq!(
            pager_suppression(PagerGate {
                elevated: true,
                ..open
            }),
            Some("elevated-foreground")
        );
        assert_eq!(
            pager_suppression(PagerGate {
                unknown: true,
                ..open
            }),
            Some("unknown-facts")
        );
    }

    #[test]
    fn output_scoped_select_needs_no_focus() {
        use crate::workspace::ManagedWorkspaces;
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        spaces.ensure_output("mon-b");
        let second = spaces.workspace_ids("mon-b")[1].clone();
        // Live on its own output verifies without that output focused.
        assert!(verify_output_scoped_select(
            &SelectScope::new("mon-b", &second, 2),
            &spaces
        ));
        // Unknown workspace or output refuses with no ordinal fallback.
        assert!(!verify_output_scoped_select(
            &SelectScope::new("mon-b", "ws-gone", 2),
            &spaces
        ));
        assert!(!verify_output_scoped_select(
            &SelectScope::new("mon-gone", &second, 2),
            &spaces
        ));
    }

    #[test]
    fn snapshot_hash_is_stable_and_change_gated() {
        use crate::workspace::ManagedWorkspaces;
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        let live = HashMap::new();
        let tokens = BTreeMap::new();
        let rects = HashMap::new();
        let first = build_output_pager(
            &spaces,
            &live,
            &tokens,
            &rects,
            &rects,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        let second = build_output_pager(
            &spaces,
            &live,
            &tokens,
            &rects,
            &rects,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        assert_eq!(
            snapshot_hash(std::slice::from_ref(&first)),
            snapshot_hash(&[second])
        );
        spaces.select("mon-a", 2);
        let moved = build_output_pager(
            &spaces,
            &live,
            &tokens,
            &rects,
            &rects,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        assert_ne!(snapshot_hash(&[first]), snapshot_hash(&[moved]));
    }

    #[test]
    fn dpi_scales_cells_and_hash() {
        // 96/144/192 DPI dimensions scale linearly; geometry (hence the
        // repaint hash) changes with DPI.
        assert_eq!(dpi_cell_w(96), 72);
        assert_eq!(dpi_cell_w(144), 108);
        assert_eq!(dpi_cell_w(192), 144);
        assert_eq!(dpi_strip_h_range(96), (24, 48));
        assert_eq!(dpi_strip_h_range(144), (36, 72));
        assert_eq!(dpi_strip_h_range(192), (48, 96));
        let frame = |dpi: u32| {
            let mut bar = bottom_bar();
            bar.dpi = dpi;
            // Taller physical bars at higher DPI (same 40 CSS px).
            bar.rect.h = dpi_scale(40, dpi);
            bar.rect.y = 1080 - bar.rect.h;
            build_output_pager(
                &spaces_with_output(),
                &HashMap::new(),
                &BTreeMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                "mon-a",
                full(),
                &bar,
            )
            .expect("fits")
        };
        let low = frame(96);
        let mid = frame(144);
        let high = frame(192);
        assert_eq!(low.cells[0].rect.w, 72);
        assert_eq!(mid.cells[0].rect.w, 108);
        assert_eq!(high.cells[0].rect.w, 144);
        assert_eq!(low.dpi, 96);
        assert_eq!(mid.dpi, 144);
        assert_ne!(
            snapshot_hash(&[low]),
            snapshot_hash(std::slice::from_ref(&mid))
        );
        assert_ne!(snapshot_hash(&[mid]), snapshot_hash(&[high]));
    }

    fn spaces_with_output() -> crate::workspace::ManagedWorkspaces {
        let mut spaces = crate::workspace::ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        spaces
    }

    #[test]
    fn member_rects_prefer_live_then_float_then_slot() {
        use crate::workspace::WindowKey;
        let mut spaces = spaces_with_output();
        let active = spaces.active_id("mon-a").expect("active");
        let key = WindowKey {
            hwnd: 11,
            pid: 7,
            creation: "c1".to_owned(),
        };
        assert!(spaces.assign(key.clone(), "mon-a", &active, false));
        let slot = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let float_frame = Rect {
            x: 100,
            y: 100,
            w: 800,
            h: 600,
        };
        let live_frame = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let mut tokens = BTreeMap::new();
        tokens.insert(key.clone(), "tok-1".to_owned());
        let mut slots = HashMap::new();
        slots.insert("tok-1".to_owned(), slot);
        let mut floats = HashMap::new();
        floats.insert("tok-1".to_owned(), float_frame);
        let mut live = HashMap::new();
        // Slot alone resolves.
        assert_eq!(
            resolve_member_rect(&key, &live, &tokens, &slots, &HashMap::new()),
            Some(slot)
        );
        // Float beats the slot.
        assert_eq!(
            resolve_member_rect(&key, &live, &tokens, &slots, &floats),
            Some(float_frame)
        );
        // Live identity-verified frame beats both (kept maximized overlays
        // and slotless born holds carry no usable slot).
        live.insert(key.clone(), live_frame);
        assert_eq!(
            resolve_member_rect(&key, &live, &tokens, &slots, &floats),
            Some(live_frame)
        );
        // Unknown members resolve to nothing, never a guess.
        let gone = WindowKey {
            hwnd: 99,
            pid: 9,
            creation: "gone".to_owned(),
        };
        assert_eq!(
            resolve_member_rect(&gone, &live, &tokens, &slots, &floats),
            None
        );
    }

    #[test]
    fn every_managed_window_renders_with_no_cap() {
        use crate::workspace::WindowKey;
        let mut spaces = spaces_with_output();
        let active = spaces.active_id("mon-a").expect("active");
        let mut tokens = BTreeMap::new();
        let mut slots = HashMap::new();
        for n in 0..24u64 {
            let key = WindowKey {
                hwnd: 100 + n,
                pid: 7,
                creation: format!("c{n}"),
            };
            assert!(spaces.assign(key.clone(), "mon-a", &active, false));
            tokens.insert(key, format!("tok-{n}"));
            slots.insert(
                format!("tok-{n}"),
                Rect {
                    x: 0,
                    y: 0,
                    w: 480,
                    h: 270,
                },
            );
        }
        let pager = build_output_pager(
            &spaces,
            &HashMap::new(),
            &tokens,
            &slots,
            &HashMap::new(),
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        // All 24 members render miniatures: ordinary crowding only adds
        // miniatures, never changes the cell layout.
        let rendered: usize = pager.cells.iter().map(|cell| cell.minis.len()).sum();
        assert_eq!(rendered, 24);
        assert_eq!(pager.cells.len(), spaces.workspace_ids("mon-a").len());
        // Icon lanes stay parallel to the miniatures (no artwork staged).
        for cell in &pager.cells {
            assert_eq!(cell.mini_keys.len(), cell.minis.len());
            assert_eq!(cell.mini_icons.len(), cell.minis.len());
            assert!(cell.mini_icons.iter().all(Option::is_none));
        }
    }

    #[test]
    fn icons_attach_by_identity_and_repaint() {
        use crate::pager_icons::{IconImage, mix_mini_icon};
        use crate::workspace::WindowKey;
        use std::sync::Arc;
        let mut spaces = spaces_with_output();
        let active = spaces.active_id("mon-a").expect("active");
        let key = WindowKey {
            hwnd: 21,
            pid: 7,
            creation: "icon-1".to_owned(),
        };
        assert!(spaces.assign(key.clone(), "mon-a", &active, false));
        let mut tokens = BTreeMap::new();
        tokens.insert(key.clone(), "tok-icon".to_owned());
        let mut slots = HashMap::new();
        slots.insert(
            "tok-icon".to_owned(),
            Rect {
                x: 0,
                y: 0,
                w: 960,
                h: 540,
            },
        );
        let plain = build_output_pager(
            &spaces,
            &HashMap::new(),
            &tokens,
            &slots,
            &HashMap::new(),
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        let mut artwork = HashMap::new();
        artwork.insert(
            key.clone(),
            Arc::new(IconImage::new(32, 32, vec![9; 32 * 32 * 4])),
        );
        let iconic = build_output_pager(
            &spaces,
            &HashMap::new(),
            &tokens,
            &slots,
            &HashMap::new(),
            &artwork,
            "mon-a",
            full(),
            &bottom_bar(),
        )
        .expect("fits");
        // Geometry identical; identity attached; artwork arrival repaints.
        assert_eq!(
            plain.cells[0].minis, iconic.cells[0].minis,
            "artwork never moves geometry"
        );
        assert_eq!(iconic.cells[0].mini_keys, vec![key.clone()]);
        assert_ne!(
            snapshot_hash(&[plain]),
            snapshot_hash(&[iconic]),
            "artwork arrival must repaint"
        );
        // A recycled HWND (same number, new creation) is a different mini.
        let mut mixed = Vec::new();
        mix_mini_icon(
            &mut |w| mixed.push(w),
            &WindowKey {
                hwnd: 21,
                pid: 7,
                creation: "icon-2".to_owned(),
            },
            None,
        );
        let mut original = Vec::new();
        mix_mini_icon(&mut |w| original.push(w), &key, None);
        assert_ne!(mixed, original);
    }

    #[test]
    fn covering_treats_managed_overlay_like_block() {
        assert!(!pager_covering(false, false));
        assert!(pager_covering(true, false));
        assert!(pager_covering(false, true));
        assert!(pager_covering(true, true));
    }
}
