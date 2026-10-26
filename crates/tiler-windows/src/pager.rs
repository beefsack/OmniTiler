#![forbid(unsafe_code)]
//! Taskbar pager: per-output workspace strip (portable policy, no Win32).
//!
//! The pager renders every workspace of an output as one numbered cell in a
//! small strip docked inside that output's taskbar, left of the tray clock
//! area: the active cell carries the accent, empty cells stay darker, and each
//! cell carries miniature lighter window rectangles scaled from the real
//! workspace rectangles plus a light 1px border. The focused window's
//! miniature carries its independent pager highlight colour (full-strength
//! border over a dimmer fill of the same); unfocused miniatures stay plain
//! gray. Clicking a cell selects that
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
//!   fails closed for hidden/off-screen taskbars, whose rects move
//!   off-screen when hidden).
//! - The overlay never shows while the taskbar auto-hides
//!   (`SHAppBarMessage(ABM_GETSTATE)` reports `ABS_AUTOHIDE`): the strip is
//!   refused throughout, including while the bar slides into view, because
//!   any parked position would either cover the tray on reveal or float over
//!   applications while hidden.
//! - Only a horizontal bottom-docked taskbar is served; top/left/right
//!   taskbars refuse with a reason instead of guessing an anchor.
//! - Secondary taskbars (`Shell_SecondaryTrayWnd`) serve only with a proven
//!   edge: an exact tray/clock child edge, or the bar's own right edge once
//!   native discovery proves no right-side shell region exists there.
//!   Unproven topologies refuse with `secondary-tray-unproven`, never a
//!   guessed anchor.
//! - `NoRoom` is a geometry-invalid bound, not app-icon crowding: cells are a
//!   fixed physical width per workspace, so ordinary taskbar crowding (many
//!   app icons, many windows per workspace) never changes the layout. When
//!   the workspace count itself cannot fit left of the tray, the whole strip
//!   refuses instead of compressing cells, truncating workspaces, or touching
//!   the taskbar.
//! - Fullscreen/game suppression is fail-closed: any suspend veto, elevated
//!   foreground, or unreadable fact hides every strip.

use std::collections::HashMap;

use tiler_core::geometry::Rect;

use crate::active_border::{DEFAULT_COLOR_RGB, SystemAccent, accent_available};
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

/// Resolved pager active-cell fill: the live Personalization accent whenever
/// it is available (present and positively alpha-quantized, the same core
/// rule the border theme gate uses), independent of the border
/// `--active-border-theme` flag (the pager has no theme setting and always
/// follows an available accent). Unavailable queries fall back to the
/// long-standing pager blue `#2a82da`.
#[must_use]
pub fn pager_active_fill(accent: Option<SystemAccent>) -> (u8, u8, u8) {
    if accent_available(accent) {
        accent.map(|a| a.color).unwrap_or(DEFAULT_COLOR_RGB)
    } else {
        DEFAULT_COLOR_RGB
    }
}

/// Dimmer accent for the focused miniature fill: the opaque-surface analogue
/// of the group-underlay `0x66`-alpha accent fill (see
/// [`crate::group_underlay`]): each accent channel premultiplied at `0x66`,
/// so the focused fill reads accent-tinted but dimmer than the full-accent
/// border beside it. Unfocused miniatures keep their plain gray.
#[must_use]
pub fn pager_dim_accent((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    const ALPHA: u16 = 0x66;
    let dim = |c: u8| ((u16::from(c) * ALPHA + 127) / 255) as u8;
    (dim(r), dim(g), dim(b))
}

/// Whether one pager window token is the reply's tick-time foreground token:
/// empty foreground focuses nothing, never a guess.
#[must_use]
pub fn pager_window_focused(window_token: &str, foreground_token: &str) -> bool {
    !foreground_token.is_empty() && window_token == foreground_token
}

/// Portable mirror of one monitor's geometry for pager layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputGeometry {
    pub device: String,
    pub full: Rect,
    pub work: Rect,
}

/// One discovered taskbar: its rect, the tray-area left edge when proven
/// (`None` when the tray area is unknown), its kind, live visibility, the
/// live auto-hide switch (`SHAppBarMessage(ABM_GETSTATE)` reports
/// `ABS_AUTOHIDE`), and the live taskbar DPI (96 only when the native query
/// fails).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarFacts {
    pub rect: Rect,
    pub tray_left: Option<i32>,
    pub kind: TaskbarKind,
    pub visible: bool,
    pub autohide: bool,
    pub dpi: u32,
}

/// Primary taskbars prove tray exclusion through `TrayNotifyWnd`;
/// secondaries prove it through [`resolve_secondary_anchor`] and refuse
/// otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarKind {
    Primary,
    Secondary,
}

/// Captured facts for one enumerated secondary-taskbar descendant window.
/// Gathering only (native side builds these); the anchor decision lives in
/// [`resolve_secondary_anchor`]. `None` class/rect means that read failed:
/// a window that disappears between enumeration and readback is unknown,
/// never empty, so any unreadable fact refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondaryChildFact {
    pub class: Option<String>,
    pub rect: Option<Rect>,
    pub visible: bool,
}

/// Decided secondary anchor: an exact tray/clock child left edge, the bar's
/// own right edge once the census proves the recognized no-tray hierarchy,
/// or unproven (refuse, never guess).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondaryAnchor {
    TrayChild(i32),
    BarRight(i32),
    Unproven,
}

/// Observed secondary descendant classes for the narrowly recognized
/// no-notification-area hierarchy (native-captured, Win11): exact tray/clock
/// children win first; otherwise only left app-button hosts, a left launcher
/// clear of the edge, an exactly bar-sized composition bridge, and a
/// zero-area input site are recognized. Visible anything-else refuses.
const SECONDARY_TRAY_CLASS: &str = "TrayNotifyWnd";
const SECONDARY_CLOCK_CLASS: &str = "ClockButton";
const SECONDARY_APP_HOST_A: &str = "WorkerW";
const SECONDARY_APP_HOST_B: &str = "MSTaskListWClass";
const SECONDARY_LAUNCHER_CLASS: &str = "Start";
const SECONDARY_BRIDGE_CLASS: &str = "Windows.UI.Composition.DesktopWindowContentBridge";
const SECONDARY_INPUT_CLASS: &str = "Windows.UI.Input.InputSite.WindowClass";

/// Resolve one secondary bar's tray edge from its flat descendant census
/// (pure; native only gathers [`SecondaryChildFact`]s). The census is
/// already complete including nested descendants: the native enumeration
/// contract covers every descendant, so no hierarchy is reconstructed here.
/// Decision, narrowly:
/// - empty census refuses: disappearance is unknown, never empty;
/// - any unreadable class/rect refuses, even hidden (a fact that cannot be
///   read cannot prove anything, and disappearance must never read as
///   absence);
/// - an exact `TrayNotifyWnd`/`ClockButton` edge wins first: the minimum left
///   edge among all readable nondegenerate exact regions (enumeration order
///   never decides), regardless of visibility or siblings. A degenerate
///   exact node (zero-width placeholder) refuses outright instead of falling
///   back to bar-right: its tray edge is unproven and bar-right could cover
///   it;
/// - else the bar's own right edge, but only for the recognized hierarchy:
///   every visible child must be a known class in a known shape
///   (left-anchored app-button host, left launcher clear of the edge, an
///   exactly bar-sized composition bridge, or a zero-area input site), and
///   at least one visible left app-button host must actually be present: an
///   empty or all-hidden census proves nothing and refuses.
///
/// Anything else (unknown visible class, bridge impostor, right-touching
/// content, missing positive evidence) refuses. This deliberately covers
/// only the observed hierarchy; it is not universal shell compatibility and
/// says nothing about HWND-less composition pixels: only HWND-descendant
/// facts decide here.
#[must_use]
pub fn resolve_secondary_anchor(bar: Rect, children: &[SecondaryChildFact]) -> SecondaryAnchor {
    if children.is_empty() {
        return SecondaryAnchor::Unproven;
    }
    // Readability first: any unreadable class/rect refuses, even hidden. A
    // fact that cannot be read cannot prove anything, and a window that
    // disappears between enumeration and readback is unknown, never absent.
    // No later exact edge rescues unknown facts.
    for child in children {
        if child.class.is_none() || child.rect.is_none() {
            return SecondaryAnchor::Unproven;
        }
    }
    // Exact tray/clock edges win next, like the primary path: the minimum
    // left edge among all readable nondegenerate exact regions, so
    // enumeration order never decides (a first-enumerated clock must not
    // cover tray region left of it). A degenerate exact node refuses
    // outright instead of falling back to bar-right: a zero-width
    // placeholder proves no edge and bar-right could cover it.
    let mut exact: Option<i32> = None;
    for child in children {
        // Safe: the pass above refused every unreadable fact.
        let class = child.class.as_deref().unwrap_or_default();
        let rect = child.rect.unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
        if class == SECONDARY_TRAY_CLASS || class == SECONDARY_CLOCK_CLASS {
            if rect.w <= 0 || rect.h <= 0 {
                return SecondaryAnchor::Unproven;
            }
            exact = Some(exact.map_or(rect.x, |min: i32| min.min(rect.x)));
        }
    }
    if let Some(left) = exact {
        return SecondaryAnchor::TrayChild(left);
    }
    // Bar-right fallback for the recognized hierarchy only. Every fact is
    // readable here (the pass above refused otherwise); hidden children hold
    // no pixels and are ignored beyond that.
    let bar_right = bar.x.saturating_add(bar.w);
    let bar_centre = bar.x.saturating_add(bar.w / 2);
    let mut positive = false;
    for child in children {
        // Safe: the first pass refused every unreadable fact.
        let class = child.class.as_deref().unwrap_or_default();
        let rect = child.rect.unwrap_or(Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
        if !child.visible {
            continue;
        }
        let degenerate = rect.w <= 0 || rect.h <= 0;
        let right = rect.x.saturating_add(rect.w);
        if class == SECONDARY_APP_HOST_A || class == SECONDARY_APP_HOST_B {
            // Left app-button host: must start left of centre, stay clear of
            // the right edge, and hold real pixels to count as positive
            // topology evidence.
            if degenerate || rect.x >= bar_centre || right >= bar_right {
                return SecondaryAnchor::Unproven;
            }
            positive = true;
        } else if class == SECONDARY_LAUNCHER_CLASS {
            // Left launcher: must stay clear of the right edge.
            if right >= bar_right {
                return SecondaryAnchor::Unproven;
            }
        } else if class == SECONDARY_BRIDGE_CLASS {
            // Composition host: exempt only with exact 2-D bar bounds. This
            // says nothing about pixels composited without HWNDs; only that
            // no HWND descendant claims the right edge (nested descendants
            // arrive as their own flat census entries and face these rules).
            if rect != bar {
                return SecondaryAnchor::Unproven;
            }
        } else if class == SECONDARY_INPUT_CLASS {
            // Zero-area input site only; any real pixels refuse.
            if !degenerate {
                return SecondaryAnchor::Unproven;
            }
        } else {
            return SecondaryAnchor::Unproven;
        }
    }
    if !positive {
        return SecondaryAnchor::Unproven;
    }
    SecondaryAnchor::BarRight(bar_right)
}

/// Why one output got no strip (bounded log vocabulary, no HWNDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerRefusal {
    TaskbarHidden,
    TaskbarAutohide,
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
            Self::TaskbarAutohide => "taskbar-autohide",
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
/// miniatures already scaled into cell coordinates, per-miniature focus
/// (parallel to `minis`: true only for the reply's foreground window), and
/// per-miniature icon state parallel to `minis`: the full window identity
/// plus optional owned artwork. Missing artwork is `None` and leaves the
/// rectangle unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerCell {
    pub rect: Rect,
    pub active: bool,
    pub ordinal: u32,
    pub minis: Vec<Rect>,
    pub mini_focused: Vec<bool>,
    pub mini_keys: Vec<crate::native_guards::WindowKey>,
    /// Raw miniature source rectangles in monitor-full coordinates, parallel
    /// to `minis`/`mini_keys`: the exact content rect each miniature was
    /// projected from. Evidence only (the painter uses `minis`); the trace
    /// publishes these so an exact frame proof survives miniature rounding.
    pub mini_sources: Vec<Rect>,
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
    /// Independent highlight colour the focused miniatures paint
    /// (loop's pager-highlight style plus the shared accent snapshot):
    /// full-strength on the miniature border, dimmed for the miniature
    /// fill. Cells keep the pager accent.
    pub focused_highlight: (u8, u8, u8),
}

/// Strip layout for one output on a bottom-docked horizontal taskbar.
/// `tray_left` is the proven tray-area left edge; the strip ends
/// `PAGER_TRAY_GAP` physical px left of it. Cell width and strip height scale
/// with the taskbar DPI. Returns the refusal instead of guessing when the
/// taskbar auto-hides, is hidden, is not bottom-docked, the tray edge is
/// unknown or outside the bar, the bar is too short for a legible strip,
/// the cells do not fit left of the tray, or the strip would leave the
/// monitor or the bar.
pub fn strip_for_taskbar(
    monitor_full: Rect,
    taskbar: &TaskbarFacts,
    cell_count: usize,
) -> Result<Rect, PagerRefusal> {
    // Auto-hide refuses first and throughout: the bar slides off-screen
    // while hidden (so geometry alone would refuse) but slides into view on
    // hover (so geometry alone would accept and park pixels over the reveal
    // path). Refusing on the switch itself keeps one honest reason in both
    // poses with no slide-in/out tracking.
    if taskbar.autohide {
        return Err(PagerRefusal::TaskbarAutohide);
    }
    if !taskbar.visible {
        return Err(PagerRefusal::TaskbarHidden);
    }
    if taskbar.kind == TaskbarKind::Secondary && taskbar.tray_left.is_none() {
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

/// One workspace's pre-ordered pager content: the core content decision for
/// one ring slot — the stable id, its 1-based core ordinal, whether it is
/// the core current view, plus its windows in paint order (bottom first,
/// topmost last) with resolved miniature source rectangles and the
/// reply-foreground focus bit each. The core owns
/// order, ordinal, and currency; this only lays out and scales, never sorts,
/// renumbers, or filters content.
pub struct PagerContentWorkspace {
    pub id: String,
    pub ordinal: u32,
    pub current: bool,
    pub windows: Vec<PagerContentMiniature>,
}

/// One content miniature: the member key, its resolved source rectangle,
/// and whether it is the reply's foreground (focused) window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerContentMiniature {
    pub key: crate::native_guards::WindowKey,
    pub rect: Rect,
    pub focused: bool,
}

/// Build one output's pager frame from core-ordered content: the core logical
/// ring in order with its current view marked, plus the per-workspace paint
/// lists. Ordinals and the active cell ride the core values verbatim; the
/// renderer may format a provided ordinal numerically as host styling.
/// `focused_highlight` is the resolved independent highlight colour (loop's
/// pager-highlight style plus the shared accent snapshot); it rides the
/// frame so a highlight-colour change repaints even when every cell is
/// identical.
/// Presentation only; the core remains the sole authority.
pub fn build_output_pager_from_content(
    content: &[PagerContentWorkspace],
    icons: &HashMap<crate::native_guards::WindowKey, std::sync::Arc<crate::pager_icons::IconImage>>,
    output: &str,
    monitor_full: Rect,
    taskbar: &TaskbarFacts,
    focused_highlight: (u8, u8, u8),
) -> Result<OutputPager, PagerRefusal> {
    let ids: Vec<String> = content.iter().map(|row| row.id.clone()).collect();
    let strip = strip_for_taskbar(monitor_full, taskbar, ids.len())?;
    let cell_rects = cells_for_strip(strip, ids.len(), dpi_cell_w(taskbar.dpi));
    // The core marks exactly one current row per non-empty content (falling
    // back to the ring head when no view is retained); the guard below only
    // covers empty content, which already refused above.
    let active_index = content.iter().position(|row| row.current).unwrap_or(0);
    let mut cells = Vec::with_capacity(ids.len());
    for (index, cell_rect) in cell_rects.into_iter().enumerate() {
        let mut minis = Vec::new();
        let mut mini_focused = Vec::new();
        let mut mini_keys = Vec::new();
        let mut mini_sources = Vec::new();
        let mut mini_icons = Vec::new();
        for entry in &content[index].windows {
            minis.push(miniature(cell_rect, monitor_full, entry.rect, taskbar.dpi));
            mini_focused.push(entry.focused);
            mini_sources.push(entry.rect);
            mini_icons.push(icons.get(&entry.key).cloned());
            mini_keys.push(entry.key.clone());
        }
        cells.push(PagerCell {
            rect: cell_rect,
            active: content[index].current,
            ordinal: content[index].ordinal,
            minis,
            mini_focused,
            mini_keys,
            mini_sources,
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
        focused_highlight,
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
        // A highlight-colour change with identical geometry still repaints:
        // the focused miniature carries the highlight colour.
        mix(u64::from(pager.focused_highlight.0));
        mix(u64::from(pager.focused_highlight.1));
        mix(u64::from(pager.focused_highlight.2));
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
            // A focus move with identical geometry still repaints: the
            // focused miniature carries accent styling.
            for focused in &cell.mini_focused {
                mix(*focused as u64);
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

/// Verify one drained pager click over plain keys: true only when the
/// rendered output is still known and the rendered workspace is still live
/// on it.
#[must_use]
pub fn verify_output_scoped_select_keys(
    rendered: &SelectScope,
    outputs: &[String],
    ring: &[String],
) -> bool {
    outputs.iter().any(|key| key == &rendered.output)
        && ring.iter().any(|id| id == &rendered.workspace)
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
            autohide: false,
            dpi: PAGER_BASE_DPI,
        }
    }

    /// Independent highlight colour for frame tests: the dogfood purple
    /// `#bd93f9`, deliberately different from every pager accent in these
    /// tests so a fill/border mix-up fails loudly.
    fn focused_highlight() -> (u8, u8, u8) {
        (0xbd, 0x93, 0xf9)
    }

    #[test]
    fn active_fill_follows_accent_regardless_of_border_theme() {
        use crate::active_border::{ActiveBorderStyle, SystemAccent, effective_color};
        let accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        // The pager always follows an available accent, even when the
        // border theme is off: there is no pager theme setting to consult.
        assert_eq!(pager_active_fill(Some(accent)), (0xaa, 0xbb, 0xcc));
        let plain = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
            ..ActiveBorderStyle::default()
        };
        assert_eq!(effective_color(&plain, Some(accent)), (0x11, 0x22, 0x33));
        assert_eq!(pager_active_fill(Some(accent)), (0xaa, 0xbb, 0xcc));
        // Unavailable queries fall back to the long-standing pager blue.
        assert_eq!(pager_active_fill(None), (0x2a, 0x82, 0xda));
        // Zero-alpha accents are unavailable everywhere (same core rule the
        // border gate uses): the pager falls back instead of painting a
        // colour the border would refuse. The pager ignores the theme
        // switch only, not availability.
        let ghost = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0,
        };
        let themed = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: true,
            ..ActiveBorderStyle::default()
        };
        assert_eq!(effective_color(&themed, Some(ghost)), (0x11, 0x22, 0x33));
        assert_eq!(pager_active_fill(Some(ghost)), (0x2a, 0x82, 0xda));
    }

    #[test]
    fn strip_anchors_left_of_tray_with_gap() {
        let strip = strip_for_taskbar(full(), &bottom_bar(), 3).expect("fits");
        assert_eq!(strip.w, 3 * dpi_cell_w(PAGER_BASE_DPI));
        assert_eq!(right(strip), 1700 - PAGER_TRAY_GAP);
        assert!(strip.y >= 1040 && bottom(strip) <= 1080);
    }

    #[test]
    fn primary_tray_churn_moves_strip_with_the_boundary() {
        // Shell icon loading moves the primary tray edge left while the bar
        // stays put: the strip must follow the fresh facts pump-for-pump
        // (same width and row, shifted by exactly the boundary delta), so a
        // fresh owner read is never painted at a stale carrier position.
        let before = strip_for_taskbar(full(), &bottom_bar(), 2).expect("fits");
        let mut grown = bottom_bar();
        grown.tray_left = Some(1650);
        let after = strip_for_taskbar(full(), &grown, 2).expect("fits");
        assert_eq!(after.w, before.w);
        assert_eq!(after.y, before.y);
        assert_eq!(after.h, before.h);
        assert_eq!(before.x - after.x, 50);
        assert_eq!(right(after), 1650 - PAGER_TRAY_GAP);
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
        // Unproven secondary tray area refuses narrowly (native discovery
        // could not prove an exact child edge or an empty right edge).
        let mut bar = bottom_bar();
        bar.kind = TaskbarKind::Secondary;
        bar.tray_left = None;
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::SecondaryTrayUnproven)
        );
        // A proven secondary edge serves exactly like the primary path.
        bar.tray_left = Some(1700);
        assert!(strip_for_taskbar(full(), &bar, 2).is_ok());
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
    fn autohide_refuses_throughout_and_reanchors_when_off() {
        // Auto-hide enabled refuses even on a visible bottom-docked bar
        // (the slide-into-view pose): no parked pixels over the reveal path.
        let mut bar = bottom_bar();
        bar.autohide = true;
        assert!(bar.visible);
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TaskbarAutohide)
        );
        // Hidden pose under auto-hide keeps the same honest reason (the
        // switch, not the transient visibility).
        bar.visible = false;
        assert_eq!(
            strip_for_taskbar(full(), &bar, 2),
            Err(PagerRefusal::TaskbarAutohide)
        );
        // Disabling re-anchors the same geometry: enabled/disabled flips
        // only this refusal, never the tray edge or the menu path.
        bar.autohide = false;
        bar.visible = true;
        let strip = strip_for_taskbar(full(), &bar, 2).expect("re-anchors");
        assert_eq!(right(strip), 1700 - PAGER_TRAY_GAP);
        // Non-bottom and unproven-secondary refusals survive alongside the
        // new one.
        let mut side = bottom_bar();
        side.autohide = true;
        side.kind = TaskbarKind::Secondary;
        side.tray_left = None;
        assert_eq!(
            strip_for_taskbar(full(), &side, 2),
            Err(PagerRefusal::TaskbarAutohide)
        );
        side.autohide = false;
        assert_eq!(
            strip_for_taskbar(full(), &side, 2),
            Err(PagerRefusal::SecondaryTrayUnproven)
        );
    }

    #[test]
    fn autohide_disable_reanchors_fresh_geometry_and_dpi() {
        // Stale pre-autohide frame under the old resolution/scale.
        let stale = strip_for_taskbar(full(), &bottom_bar(), 2).expect("old fits");
        // Resolution + scale change while auto-hide hides everything: the
        // switch still refuses against the new facts, never a stale strip.
        let new_full = Rect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1440,
        };
        let mut bar = bottom_bar();
        bar.autohide = true;
        assert_eq!(
            strip_for_taskbar(new_full, &bar, 2),
            Err(PagerRefusal::TaskbarAutohide)
        );
        // Disable with all-new facts (taller bar, moved tray edge, 144 DPI):
        // the fresh frame follows the new facts, never the stale ones.
        bar.autohide = false;
        bar.dpi = 144;
        bar.rect = Rect {
            x: 0,
            y: 1440 - 60,
            w: 2560,
            h: 60,
        };
        bar.tray_left = Some(2300);
        let fresh = strip_for_taskbar(new_full, &bar, 2).expect("re-anchors fresh");
        assert_eq!(fresh.w, 2 * dpi_cell_w(144));
        assert_eq!(right(fresh), 2300 - PAGER_TRAY_GAP);
        assert!(fresh.y >= 1440 - 60 && bottom(fresh) <= 1440);
        // No stale coordinates survive: position and scale both differ from
        // the old frame, so laying out from remembered facts would misplace.
        assert_ne!(fresh, stale);
        assert_ne!(fresh.w, stale.w);
        let frame = build_output_pager_from_content(
            &content_rows(&ring_ids(), "ws-1"),
            &HashMap::new(),
            "mon-a",
            new_full,
            &bar,
            focused_highlight(),
        )
        .expect("frames fresh");
        assert_eq!(frame.dpi, 144);
        assert_eq!(frame.cells[0].rect.w, dpi_cell_w(144));
        assert_eq!(right(frame.strip), 2300 - PAGER_TRAY_GAP);
    }

    #[test]
    fn autohide_refusal_reaches_frames_with_tray_edge_intact() {
        // The frame refuses (never a guessed strip) while the proven tray
        // edge stays staged, so the tray number/menu path is untouched.
        let mut bar = bottom_bar();
        bar.autohide = true;
        assert_eq!(
            build_output_pager_from_content(
                &content_rows(&ring_ids(), "ws-1"),
                &HashMap::new(),
                "mon-a",
                full(),
                &bar,
                focused_highlight(),
            )
            .expect_err("autohide never frames"),
            PagerRefusal::TaskbarAutohide
        );
        assert_eq!(bar.tray_left, Some(1700), "tray proof retained");
    }

    fn secondary_full() -> Rect {
        // Captured native topology (Win11, two-display host): DISPLAY2 at
        // (2560,355) 1920x1080 with a bottom-docked 48px secondary bar.
        Rect {
            x: 2560,
            y: 355,
            w: 1920,
            h: 1080,
        }
    }

    fn secondary_bar_right_anchored() -> TaskbarFacts {
        // Native child set on this build: full-span composition bridge,
        // zero-area input site, hidden Start, left-anchored WorkerW /
        // MSTaskListWClass app-button hosts ending ~1513px left of the bar
        // edge; no TrayNotifyWnd/ClockButton anywhere. Discovery proves the
        // empty right edge and anchors to the bar's own right side.
        TaskbarFacts {
            rect: Rect {
                x: 2560,
                y: 1387,
                w: 1920,
                h: 48,
            },
            tray_left: Some(4480),
            kind: TaskbarKind::Secondary,
            visible: true,
            autohide: false,
            dpi: PAGER_BASE_DPI,
        }
    }

    #[test]
    fn secondary_proven_right_edge_anchors() {
        let bar = secondary_bar_right_anchored();
        let strip = strip_for_taskbar(secondary_full(), &bar, 2).expect("serves");
        assert_eq!(strip.w, 2 * dpi_cell_w(PAGER_BASE_DPI));
        assert_eq!(right(strip), 4480 - PAGER_TRAY_GAP);
        assert!(strip.y >= 1387 && bottom(strip) <= 1435);
        // The app-button span (native 2615..2967) sits far left of the
        // right-anchored strip: ordinary counts never touch it.
        assert!(strip.x > 2967);
    }

    #[test]
    fn proven_secondary_churn_moves_strip_with_the_edge() {
        // Healthy proven-secondary churn (tray child edge moves as shell
        // content loads while the bar stays put): the strip follows the
        // fresh facts pump-for-pump, same width and row, shifted by exactly
        // the edge delta. Same follow contract as the primary path above.
        let before = strip_for_taskbar(secondary_full(), &secondary_bar_right_anchored(), 2)
            .expect("serves");
        let mut churned = secondary_bar_right_anchored();
        churned.tray_left = Some(4440);
        let after = strip_for_taskbar(secondary_full(), &churned, 2).expect("serves");
        assert_eq!(after.w, before.w);
        assert_eq!(after.y, before.y);
        assert_eq!(after.h, before.h);
        assert_eq!(before.x - after.x, 40);
        assert_eq!(right(after), 4440 - PAGER_TRAY_GAP);
    }

    #[test]
    fn secondary_tray_child_anchors_exact() {
        // A build hosting a real secondary tray proves its exact child edge
        // (same anchor shape as the primary path).
        let mut bar = secondary_bar_right_anchored();
        bar.tray_left = Some(4300);
        let strip = strip_for_taskbar(secondary_full(), &bar, 3).expect("serves");
        assert_eq!(right(strip), 4300 - PAGER_TRAY_GAP);
        assert_eq!(strip.w, 3 * dpi_cell_w(PAGER_BASE_DPI));
    }

    #[test]
    fn secondary_fences_match_primary() {
        // Hidden, auto-hide, non-bottom and unknown-edge secondaries refuse
        // with the same honest reasons as the primary path.
        let mut bar = secondary_bar_right_anchored();
        bar.visible = false;
        assert_eq!(
            strip_for_taskbar(secondary_full(), &bar, 2),
            Err(PagerRefusal::TaskbarHidden)
        );
        bar.visible = true;
        bar.autohide = true;
        assert_eq!(
            strip_for_taskbar(secondary_full(), &bar, 2),
            Err(PagerRefusal::TaskbarAutohide)
        );
        bar.autohide = false;
        bar.rect = Rect {
            x: 2560,
            y: 355,
            w: 48,
            h: 1080,
        };
        assert_eq!(
            strip_for_taskbar(secondary_full(), &bar, 2),
            Err(PagerRefusal::NotBottomDocked)
        );
        // Over-wide strips refuse instead of covering the app buttons.
        let bar = secondary_bar_right_anchored();
        assert_eq!(
            strip_for_taskbar(secondary_full(), &bar, 27),
            Err(PagerRefusal::NoRoom)
        );
        // An edge outside its own bar refuses instead of parking over it.
        let mut outside = secondary_bar_right_anchored();
        outside.tray_left = Some(4481);
        assert_eq!(
            strip_for_taskbar(secondary_full(), &outside, 2),
            Err(PagerRefusal::TrayOutsideBar)
        );
    }

    #[test]
    fn secondary_frames_one_strip_per_output() {
        let frame_for = |bar: &TaskbarFacts| {
            build_output_pager_from_content(
                &content_rows(&ring_ids(), "ws-1"),
                &HashMap::new(),
                "mon-b",
                secondary_full(),
                bar,
                focused_highlight(),
            )
        };
        let pager = frame_for(&secondary_bar_right_anchored()).expect("secondary frames");
        assert_eq!(pager.output, "mon-b");
        assert_eq!(right(pager.strip), 4480 - PAGER_TRAY_GAP);
        // The unproven topology still refuses at frame level, never guessed.
        let mut unproven = secondary_bar_right_anchored();
        unproven.tray_left = None;
        assert_eq!(
            frame_for(&unproven).expect_err("unproven never frames"),
            PagerRefusal::SecondaryTrayUnproven
        );
    }

    fn sec_fact(class: &str, rect: Rect, visible: bool) -> SecondaryChildFact {
        SecondaryChildFact {
            class: Some(class.to_owned()),
            rect: Some(rect),
            visible,
        }
    }

    fn sec_bar_rect() -> Rect {
        Rect {
            x: 2560,
            y: 1387,
            w: 1920,
            h: 48,
        }
    }

    fn captured_secondary_census() -> Vec<SecondaryChildFact> {
        // Native flat descendant census (Win11 build 26300, secondary bar
        // 2560,1387 1920x48): the composition bridge spans exactly the bar,
        // its zero-area input site nests inside it (flat entries both: the
        // contract enumerates every descendant), Start is hidden at the
        // left, and visible WorkerW/MSTaskListWClass hosts sit left at
        // 2615..2967. No tray/clock descendant anywhere.
        vec![
            sec_fact(
                "Windows.UI.Composition.DesktopWindowContentBridge",
                sec_bar_rect(),
                true,
            ),
            sec_fact(
                "Windows.UI.Input.InputSite.WindowClass",
                Rect {
                    x: 2560,
                    y: 1387,
                    w: 0,
                    h: 0,
                },
                true,
            ),
            sec_fact(
                "Start",
                Rect {
                    x: 2560,
                    y: 1387,
                    w: 55,
                    h: 48,
                },
                false,
            ),
            sec_fact(
                "WorkerW",
                Rect {
                    x: 2615,
                    y: 1387,
                    w: 352,
                    h: 48,
                },
                true,
            ),
            sec_fact(
                "MSTaskListWClass",
                Rect {
                    x: 2615,
                    y: 1387,
                    w: 352,
                    h: 48,
                },
                true,
            ),
        ]
    }

    #[test]
    fn secondary_anchor_accepts_captured_hierarchy() {
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &captured_secondary_census()),
            SecondaryAnchor::BarRight(4480)
        );
    }

    #[test]
    fn secondary_anchor_wires_into_strip_geometry() {
        // The pure decision (not an injected edge) drives the strip: the
        // captured census resolves to the bar-right anchor and frames.
        let anchor = resolve_secondary_anchor(sec_bar_rect(), &captured_secondary_census());
        let tray_left = match anchor {
            SecondaryAnchor::TrayChild(left) | SecondaryAnchor::BarRight(left) => Some(left),
            SecondaryAnchor::Unproven => None,
        };
        let facts = TaskbarFacts {
            rect: sec_bar_rect(),
            tray_left,
            kind: TaskbarKind::Secondary,
            visible: true,
            autohide: false,
            dpi: PAGER_BASE_DPI,
        };
        let strip = strip_for_taskbar(secondary_full(), &facts, 2).expect("serves");
        assert_eq!(right(strip), 4480 - PAGER_TRAY_GAP);
    }

    #[test]
    fn secondary_anchor_prefers_exact_tray_and_clock_children() {
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::TrayChild(4300)
        );
        // A hidden tray still anchors exactly (primary-symmetric): hiding the
        // region never moves pixels over it.
        let mut hidden = captured_secondary_census();
        hidden.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            false,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &hidden),
            SecondaryAnchor::TrayChild(4300)
        );
        let mut clock = captured_secondary_census();
        clock.push(sec_fact(
            "ClockButton",
            Rect {
                x: 4350,
                y: 1387,
                w: 130,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &clock),
            SecondaryAnchor::TrayChild(4350)
        );
    }

    #[test]
    fn secondary_anchor_exact_edge_prefers_minimum_x_regardless_of_order() {
        // If ClockButton enumerates before an encompassing TrayNotifyWnd,
        // anchoring to the clock alone could cover tray region left of it:
        // the minimum left edge wins in either enumeration order.
        let tray = sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            true,
        );
        let clock = sec_fact(
            "ClockButton",
            Rect {
                x: 4350,
                y: 1387,
                w: 130,
                h: 48,
            },
            true,
        );
        let mut clock_first = captured_secondary_census();
        clock_first.push(clock.clone());
        clock_first.push(tray.clone());
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &clock_first),
            SecondaryAnchor::TrayChild(4300)
        );
        let mut tray_first = captured_secondary_census();
        tray_first.push(tray);
        tray_first.push(clock);
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &tray_first),
            SecondaryAnchor::TrayChild(4300)
        );
        // Minimum wins even when the clock sits left of the tray.
        let mut swapped = captured_secondary_census();
        swapped.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4350,
                y: 1387,
                w: 130,
                h: 48,
            },
            true,
        ));
        swapped.push(sec_fact(
            "ClockButton",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &swapped),
            SecondaryAnchor::TrayChild(4300)
        );
    }

    #[test]
    fn secondary_anchor_refuses_degenerate_exact_node() {
        // A zero-width tray placeholder proves no edge: refuse outright
        // instead of falling back to bar-right.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 0,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // A degenerate exact node poisons even a valid exact sibling: the
        // census disagrees about the tray region, so no edge is proven.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "ClockButton",
            Rect {
                x: 4350,
                y: 1387,
                w: 130,
                h: 0,
            },
            true,
        ));
        census.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
    }

    #[test]
    fn secondary_anchor_refuses_empty_and_unreadable() {
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &[]),
            SecondaryAnchor::Unproven
        );
        // Unreadable class on a visible child.
        let mut census = captured_secondary_census();
        census[3].class = None;
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Unreadable rect on a visible child (disappeared mid-read).
        let mut census = captured_secondary_census();
        census[3].rect = None;
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Strictest legs: unreadable facts refuse even when hidden.
        let mut census = captured_secondary_census();
        census[2].rect = None;
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        let mut census = captured_secondary_census();
        census[2].class = None;
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Unknown facts are never rescued by an exact edge elsewhere.
        let mut census = captured_secondary_census();
        census[0].rect = None;
        census.push(sec_fact(
            "TrayNotifyWnd",
            Rect {
                x: 4300,
                y: 1387,
                w: 180,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
    }

    #[test]
    fn secondary_anchor_refuses_vacuous_censuses() {
        // Known classes but nothing visible: no positive evidence.
        let census = vec![
            sec_fact(
                "Windows.UI.Composition.DesktopWindowContentBridge",
                sec_bar_rect(),
                false,
            ),
            sec_fact(
                "Start",
                Rect {
                    x: 2560,
                    y: 1387,
                    w: 55,
                    h: 48,
                },
                false,
            ),
        ];
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Bridge and input alone, however visible, prove no app buttons.
        let census = vec![
            sec_fact(
                "Windows.UI.Composition.DesktopWindowContentBridge",
                sec_bar_rect(),
                true,
            ),
            sec_fact(
                "Windows.UI.Input.InputSite.WindowClass",
                Rect {
                    x: 2560,
                    y: 1387,
                    w: 0,
                    h: 0,
                },
                true,
            ),
        ];
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // App-button hosts hidden: vacuous again.
        let mut census = captured_secondary_census();
        census[3].visible = false;
        census[4].visible = false;
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
    }

    #[test]
    fn secondary_anchor_refuses_unknown_and_right_side_content() {
        // Unknown visible class, even parked far left.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "Shell_Flyout",
            Rect {
                x: 3000,
                y: 1387,
                w: 100,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Nested-descendant shape: the flat census already contains depth-2
        // windows as plain entries, so a partially overlapping unknown under
        // the bridge still refuses by class.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "XamlClockIsland",
            Rect {
                x: 4000,
                y: 1400,
                w: 200,
                h: 20,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Known left host stretched onto the right edge.
        let mut census = captured_secondary_census();
        census[3].rect = Some(Rect {
            x: 2615,
            y: 1387,
            w: 1865,
            h: 48,
        });
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Right-touching unknown content.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "TrayClockWClass",
            Rect {
                x: 4400,
                y: 1387,
                w: 80,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
    }

    #[test]
    fn secondary_anchor_refuses_bridge_impostors_and_pixel_input() {
        // Bridge one pixel short of the bar: not the exempt host.
        let mut census = captured_secondary_census();
        census[0].rect = Some(Rect {
            x: 2560,
            y: 1387,
            w: 1920,
            h: 47,
        });
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Bridge shifted right: the edge still touches, the bounds differ.
        let mut census = captured_secondary_census();
        census[0].rect = Some(Rect {
            x: 2561,
            y: 1387,
            w: 1919,
            h: 48,
        });
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Input site with real pixels is not the known zero-area child.
        let mut census = captured_secondary_census();
        census[1].rect = Some(Rect {
            x: 4400,
            y: 1387,
            w: 80,
            h: 48,
        });
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
        );
        // Visible launcher reaching the edge.
        let mut census = captured_secondary_census();
        census.push(sec_fact(
            "Start",
            Rect {
                x: 2560,
                y: 1387,
                w: 1920,
                h: 48,
            },
            true,
        ));
        assert_eq!(
            resolve_secondary_anchor(sec_bar_rect(), &census),
            SecondaryAnchor::Unproven
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
        let ring = ring_ids();
        let outputs = vec!["mon-a".to_owned(), "mon-b".to_owned()];
        let second = ring[1].clone();
        // Live on its own output verifies without that output focused.
        assert!(verify_output_scoped_select_keys(
            &SelectScope::new("mon-b", &second, 2),
            &outputs,
            &ring
        ));
        // Unknown workspace or output refuses with no ordinal fallback.
        assert!(!verify_output_scoped_select_keys(
            &SelectScope::new("mon-b", "ws-gone", 2),
            &outputs,
            &ring
        ));
        assert!(!verify_output_scoped_select_keys(
            &SelectScope::new("mon-gone", &second, 2),
            &outputs,
            &ring
        ));
    }

    #[test]
    fn snapshot_hash_is_stable_and_change_gated() {
        let frame_for = |active: &str| {
            build_output_pager_from_content(
                &content_rows(&ring_ids(), active),
                &HashMap::new(),
                "mon-a",
                full(),
                &bottom_bar(),
                focused_highlight(),
            )
            .expect("fits")
        };
        let first = frame_for("ws-1");
        let second = frame_for("ws-1");
        assert_eq!(
            snapshot_hash(std::slice::from_ref(&first)),
            snapshot_hash(&[second])
        );
        let moved = frame_for("ws-2");
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
            build_output_pager_from_content(
                &content_rows(&ring_ids(), "ws-1"),
                &HashMap::new(),
                "mon-a",
                full(),
                &bar,
                focused_highlight(),
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

    fn ring_ids() -> Vec<String> {
        vec!["ws-1".to_owned(), "ws-2".to_owned()]
    }

    fn content_rows(ids: &[String], active: &str) -> Vec<PagerContentWorkspace> {
        ids.iter()
            .enumerate()
            .map(|(index, id)| PagerContentWorkspace {
                id: id.clone(),
                ordinal: index as u32 + 1,
                current: id == active,
                windows: Vec::new(),
            })
            .collect()
    }

    fn content_key(hwnd: u64, tag: &str) -> crate::native_guards::WindowKey {
        crate::native_guards::WindowKey {
            hwnd,
            pid: 7,
            creation: tag.to_owned(),
        }
    }

    #[test]
    fn every_managed_window_renders_with_no_cap() {
        // Core-ordered paint list with 24 resolved source rects: ordinary
        // crowding only adds miniatures, never changes the cell layout.
        let mut windows = Vec::new();
        for n in 0..24u64 {
            windows.push(PagerContentMiniature {
                key: content_key(100 + n, &format!("c{n}")),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 480,
                    h: 270,
                },
                focused: false,
            });
        }
        let mut content = content_rows(&ring_ids(), "ws-1");
        content[0].windows = windows;
        let pager = build_output_pager_from_content(
            &content,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
            focused_highlight(),
        )
        .expect("fits");
        // All 24 members render miniatures: ordinary crowding only adds
        // miniatures, never changes the cell layout.
        let rendered: usize = pager.cells.iter().map(|cell| cell.minis.len()).sum();
        assert_eq!(rendered, 24);
        assert_eq!(pager.cells.len(), ring_ids().len());
        // Icon lanes stay parallel to the miniatures (no artwork staged).
        // Raw source rects stay parallel too: the exact evidence miniature.
        for cell in &pager.cells {
            assert_eq!(cell.mini_keys.len(), cell.minis.len());
            assert_eq!(cell.mini_sources.len(), cell.minis.len());
            assert_eq!(cell.mini_icons.len(), cell.minis.len());
            assert_eq!(cell.mini_focused.len(), cell.minis.len());
            assert!(cell.mini_icons.iter().all(Option::is_none));
        }
    }

    #[test]
    fn icons_attach_by_identity_and_repaint() {
        use crate::pager_icons::{IconImage, mix_mini_icon};
        use std::sync::Arc;
        let key = content_key(21, "icon-1");
        let slot = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let content_of = |artwork: &HashMap<crate::native_guards::WindowKey, Arc<IconImage>>| {
            let mut content = content_rows(&ring_ids(), "ws-1");
            content[0].windows = vec![PagerContentMiniature {
                key: key.clone(),
                rect: slot,
                focused: false,
            }];
            build_output_pager_from_content(
                &content,
                artwork,
                "mon-a",
                full(),
                &bottom_bar(),
                focused_highlight(),
            )
            .expect("fits")
        };
        let plain = content_of(&HashMap::new());
        let mut artwork = HashMap::new();
        artwork.insert(
            key.clone(),
            Arc::new(IconImage::new(32, 32, vec![9; 32 * 32 * 4])),
        );
        let iconic = content_of(&artwork);
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
        mix_mini_icon(&mut |w| mixed.push(w), &content_key(21, "icon-2"), None);
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

    fn stack_key(hwnd: u64, tag: &str) -> crate::native_guards::WindowKey {
        crate::native_guards::WindowKey {
            hwnd,
            pid: 7,
            creation: tag.to_owned(),
        }
    }

    fn stack_frame(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            w: 960,
            h: 540,
        }
    }

    #[test]
    fn content_paint_order_is_preserved_verbatim() {
        // The frame lays out core-ordered paint lists verbatim: HWND numeric
        // order never re-sorts. The topmost window arrives last (bottom
        // first) and paints last so it occludes.
        let top = content_key(10, "stack-top");
        let middle = content_key(20, "stack-middle");
        let bottom = content_key(30, "stack-bottom");
        let mut content = content_rows(&ring_ids(), "ws-1");
        content[0].windows = vec![
            PagerContentMiniature {
                key: bottom.clone(),
                rect: stack_frame(0, 540),
                focused: false,
            },
            PagerContentMiniature {
                key: middle.clone(),
                rect: stack_frame(960, 0),
                focused: false,
            },
            PagerContentMiniature {
                key: top.clone(),
                rect: stack_frame(0, 0),
                focused: true,
            },
        ];
        let pager = build_output_pager_from_content(
            &content,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
            focused_highlight(),
        )
        .expect("fits");
        assert_eq!(
            pager.cells[0].mini_keys,
            vec![bottom.clone(), middle.clone(), top.clone()]
        );
        let cell = pager.cells[0].rect;
        assert_eq!(
            pager.cells[0].minis,
            vec![
                miniature(cell, full(), stack_frame(0, 540), PAGER_BASE_DPI),
                miniature(cell, full(), stack_frame(960, 0), PAGER_BASE_DPI),
                miniature(cell, full(), stack_frame(0, 0), PAGER_BASE_DPI),
            ]
        );
    }

    #[test]
    fn content_interleaves_overlay_order_verbatim() {
        // A kept maximized/fullscreen overlay arrives interleaved with
        // ordinary windows in native sequence; the frame keeps that order,
        // never grouping overlays apart.
        let plain = stack_key(40, "overlay-plain");
        let overlay = stack_key(50, "overlay-kept");
        let tiled = stack_key(60, "overlay-tiled");
        let mut content = content_rows(&ring_ids(), "ws-1");
        content[0].windows = vec![
            PagerContentMiniature {
                key: plain.clone(),
                rect: stack_frame(0, 0),
                focused: false,
            },
            PagerContentMiniature {
                key: tiled.clone(),
                rect: stack_frame(960, 540),
                focused: false,
            },
            PagerContentMiniature {
                key: overlay.clone(),
                rect: full(),
                focused: false,
            },
        ];
        let pager = build_output_pager_from_content(
            &content,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
            focused_highlight(),
        )
        .expect("fits");
        assert_eq!(pager.cells[0].mini_keys, vec![plain, tiled, overlay]);
    }

    #[test]
    fn content_ordinal_and_currency_ride_verbatim() {
        // Core content decisions are honored, never recomputed: ordinals
        // need not match positions and the current cell need not be first.
        // The renderer may format a provided ordinal numerically as host
        // styling, like the positional path always has.
        let rows = vec![
            PagerContentWorkspace {
                id: "ws-a".to_owned(),
                ordinal: 7,
                current: false,
                windows: Vec::new(),
            },
            PagerContentWorkspace {
                id: "ws-b".to_owned(),
                ordinal: 3,
                current: true,
                windows: Vec::new(),
            },
        ];
        let pager = build_output_pager_from_content(
            &rows,
            &HashMap::new(),
            "mon-a",
            full(),
            &bottom_bar(),
            focused_highlight(),
        )
        .expect("fits");
        assert_eq!(pager.ids, vec!["ws-a".to_owned(), "ws-b".to_owned()]);
        assert_eq!(pager.cells[0].ordinal, 7);
        assert_eq!(pager.cells[1].ordinal, 3);
        assert!(!pager.cells[0].active);
        assert!(pager.cells[1].active);
        assert_eq!(pager.active_index, 1);
    }

    #[test]
    fn dim_accent_is_premultiplied_at_underlay_alpha() {
        // Opaque-surface analogue of the `#66`-alpha underlay fill: the
        // long-standing `#2a82da` fallback dims to the same premultiplied
        // channels the underlay raster proves (`[0x57, 0x34, 0x11, 0x66]`
        // BGRA in `group_underlay`).
        assert_eq!(pager_dim_accent((0x2a, 0x82, 0xda)), (0x11, 0x34, 0x57));
        assert_eq!(pager_dim_accent((0xff, 0xff, 0xff)), (0x66, 0x66, 0x66));
        assert_eq!(pager_dim_accent((0x00, 0x00, 0x00)), (0x00, 0x00, 0x00));
    }

    #[test]
    fn foreground_token_marks_exactly_one_focus() {
        assert!(pager_window_focused("tok-a", "tok-a"));
        assert!(!pager_window_focused("tok-b", "tok-a"));
        // Empty foreground focuses nothing: an unmanaged or departed
        // foreground never guesses a miniature.
        assert!(!pager_window_focused("tok-a", ""));
        assert!(!pager_window_focused("", ""));
    }

    #[test]
    fn focused_miniature_follows_highlight_resolution_not_pager_fill() {
        use crate::active_border::SystemAccent;
        use crate::pager_highlight::{PagerHighlightStyle, effective_color};
        // Dogfood settings: configured purple highlight, theme off, live
        // system accent different. The highlight resolves to the configured
        // colour while the pager cell keeps the system accent: the miniature
        // must carry the highlight colour, never the cell fill, and never
        // the ring colour (independent setting).
        let highlight = PagerHighlightStyle {
            color: (0xbd, 0x93, 0xf9),
            use_theme: false,
        };
        let ring = crate::active_border::ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
            ..crate::active_border::ActiveBorderStyle::default()
        };
        let accent = SystemAccent {
            color: (0x4c, 0x4a, 0x48),
            alpha: 0xE3,
        };
        assert_eq!(
            effective_color(&highlight, Some(accent)),
            (0xbd, 0x93, 0xf9)
        );
        assert_eq!(
            crate::active_border::effective_color(&ring, Some(accent)),
            (0x11, 0x22, 0x33),
            "ring stays independent of the highlight"
        );
        assert_eq!(
            pager_active_fill(Some(accent)),
            (0x4c, 0x4a, 0x48),
            "cell fill stays on the system accent"
        );
        assert_eq!(pager_dim_accent((0xbd, 0x93, 0xf9)), (76, 59, 100));
        // Themed highlight resolves to the accent the cell also uses: the
        // helper still takes its own switch, it just coincides.
        let themed = PagerHighlightStyle {
            use_theme: true,
            ..highlight
        };
        assert_eq!(effective_color(&themed, Some(accent)), (0x4c, 0x4a, 0x48));
        // The frame carries the highlight colour, and a highlight-colour
        // change with identical geometry still repaints.
        let frame_of = |highlight: (u8, u8, u8)| {
            build_output_pager_from_content(
                &content_rows(&ring_ids(), "ws-1"),
                &HashMap::new(),
                "mon-a",
                full(),
                &bottom_bar(),
                highlight,
            )
            .expect("fits")
        };
        let purple = frame_of((0xbd, 0x93, 0xf9));
        assert_eq!(purple.focused_highlight, (0xbd, 0x93, 0xf9));
        let accented = frame_of((0x4c, 0x4a, 0x48));
        assert_ne!(
            snapshot_hash(&[purple]),
            snapshot_hash(&[accented]),
            "highlight-colour change must repaint"
        );
    }

    #[test]
    fn focused_flag_rides_into_cells_and_repaints() {
        let key = || content_key(21, "focus-1");
        let slot = Rect {
            x: 0,
            y: 0,
            w: 960,
            h: 540,
        };
        let frame_of = |focused: bool| {
            let mut content = content_rows(&ring_ids(), "ws-1");
            content[0].windows = vec![PagerContentMiniature {
                key: key(),
                rect: slot,
                focused,
            }];
            build_output_pager_from_content(
                &content,
                &HashMap::new(),
                "mon-a",
                full(),
                &bottom_bar(),
                focused_highlight(),
            )
            .expect("fits")
        };
        let plain = frame_of(false);
        assert_eq!(plain.cells[0].mini_focused, vec![false]);
        let focused = frame_of(true);
        assert_eq!(focused.cells[0].mini_focused, vec![true]);
        // Identical geometry with a moved focus still repaints.
        assert_eq!(plain.cells[0].minis, focused.cells[0].minis);
        assert_ne!(
            snapshot_hash(&[plain]),
            snapshot_hash(&[focused]),
            "focus move must repaint"
        );
    }
}
