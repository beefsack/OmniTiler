//! Owned active-border overlay (`cfg(windows)` only).
//!
//! Mechanism: one process-owned `WS_EX_LAYERED | WS_EX_TRANSPARENT |
//! WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` `WS_POPUP` surface with per-pixel
//! alpha via `UpdateLayeredWindow`. No foreign attribute mutation, no foreign
//! ownership/style writes. The surface is click-through, never activates,
//! skips Alt+Tab via `WS_EX_TOOLWINDOW`, and follows the target's topmost
//! band (`HWND_TOPMOST` only while the target is topmost, otherwise a
//! normal-band surface placed directly beneath the target): never
//! permanently pinned over unrelated topmost/shell UI. Hidden on any suppression. Process exit
//! destroys it implicitly (no residue); graceful stop destroys it explicitly.
//!
//! The drawing carrier (create/paint/move/hide/destroy) is reusable for the
//! later group underlay, which is not implemented here.

use tiler_core::geometry::Rect;

use crate::active_border::{ActiveBorderStyle, SystemAccent, effective_color, ring_covers};

#[path = "active_border_sys/carrier.rs"]
mod carrier;

#[cfg(test)]
use carrier::{ZRelation, z_needs_reassert};
use carrier::{
    create_overlay_window, destroy_window, hide_window, is_overlay_window, move_overlay,
    overlay_needs_reassert, paint_and_present, paint_and_present_fill,
    paint_and_present_preview_fill, place_preview_above, preview_needs_reassert,
};
pub(crate) use carrier::{create_overlay_window_in, lowest_in_z, overlay_window_alive};

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

pub const OVERLAY_CLASS: &str = "OmniTilerActiveBorder";
/// Owned group-underlay surface class: distinct from the border class so the
/// read-only inspect commands and residue audits count each carrier exactly.
pub const UNDERLAY_CLASS: &str = "OmniTilerGroupUnderlay";
/// Owned drop-preview surface class: distinct from the border and underlay
/// classes so inspect commands and residue audits count each carrier exactly.
/// Separate Z-plane above windows (KWin overlay-item analogue); never mixed
/// with the border/underlay below-target plane.
pub const PREVIEW_CLASS: &str = "OmniTilerDropPreview";

/// Default drop-preview fill (KDE parity): `#2A82DA` at alpha 64, i.e. ARGB
/// `#402A82DA`. Stored as `(alpha, r, g, b)` like the underlay carrier.
pub const PREVIEW_DEFAULT_ARGB: (u8, u8, u8, u8) = (0x40, 0x2A, 0x82, 0xDA);

/// Default preview fill split into the `paint_fill_argb` argument shape
/// (`(rgb, alpha)`).
#[must_use]
pub const fn preview_default_fill() -> ((u8, u8, u8), u8) {
    (
        (
            PREVIEW_DEFAULT_ARGB.1,
            PREVIEW_DEFAULT_ARGB.2,
            PREVIEW_DEFAULT_ARGB.3,
        ),
        PREVIEW_DEFAULT_ARGB.0,
    )
}

/// Degenerate preview bounds gate: non-positive or absurd surfaces never
/// allocate or present; the caller hides instead.
#[must_use]
pub const fn preview_bounds_valid(outer: Rect) -> bool {
    outer.w > 0 && outer.h > 0 && outer.w <= 16384 && outer.h <= 16384
}

// Stable Win32 broadcast ids (no new dependency): the overlay WndProc only
// flags on these; the loop thread re-queries and repaints.
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;

fn accent_dirty_flag() -> &'static std::sync::atomic::AtomicBool {
    use std::sync::atomic::AtomicBool;
    static DIRTY: AtomicBool = AtomicBool::new(false);
    &DIRTY
}

/// Sticky accent-refresh request from the overlay WndProc. Consumed by
/// `show_at`: sticky, so coalesced broadcasts still refresh exactly once.
fn take_accent_dirty() -> bool {
    use std::sync::atomic::Ordering;
    accent_dirty_flag().swap(false, Ordering::SeqCst)
}

fn mark_accent_dirty() {
    use std::sync::atomic::Ordering;
    accent_dirty_flag().store(true, Ordering::SeqCst);
}

/// Overlay window procedure: accent broadcasts only flag a refresh, every
/// other message keeps default handling. Never paints, hides, or destroys:
/// no feedback into the loop-owned overlay state.
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW;
    if msg == WM_DWMCOLORIZATIONCOLORCHANGED || msg == WM_SETTINGCHANGE {
        mark_accent_dirty();
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Read-only system accent query: the official DWM colorization colour
/// (`DwmGetColorizationColor`, `0xAARRGGBB`), the Windows analogue of the KDE
/// Plasma highlight the KWin effect reads from `KColorScheme`. `None` when
/// the API fails: callers fall back to the configured colour. No writes, no
/// settings changes, no parameters consumed.
fn query_system_accent() -> Option<SystemAccent> {
    use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;
    let mut dword: u32 = 0;
    let mut opaque: i32 = 0;
    let hr = unsafe { DwmGetColorizationColor(&mut dword, &mut opaque) };
    if hr == 0 {
        Some(SystemAccent::from_colorization_dword(dword))
    } else {
        None
    }
}

/// Outcome of one overlay update, for bounded logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayOutcome {
    Hidden,
    Shown,
    Moved,
    Redrew,
    Unchanged,
}

/// Process-owned overlay lifecycle plus owned-pixel inspection.
#[derive(Default)]
pub struct BorderOverlay {
    hwnd: Option<u64>,
    outer: Option<Rect>,
    style_px: Option<(i32, i32, i32)>,
    color: Option<(u8, u8, u8)>,
    /// Last queried system accent (`None` until the first show or when the
    /// query fails): refreshed only on the WndProc dirty flag, compared
    /// through `effective_color` so an accent change repaints via the
    /// existing colour comparison with no extra paint path.
    accent: Option<SystemAccent>,
    target_token: Option<String>,
    topmost: bool,
    dib_checksum: u64,
    redraws: u64,
    moves: u64,
    failures: u64,
    last_error: Option<String>,
}

impl BorderOverlay {
    #[must_use]
    pub fn hwnd(&self) -> Option<u64> {
        self.hwnd
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.outer.is_some()
    }

    /// Last resolved ring colour (accent-or-fallback actually painted).
    #[must_use]
    pub fn current_color(&self) -> Option<(u8, u8, u8)> {
        self.color
    }

    /// Opaque target token currently shown, if any. Lets the renderer admit
    /// the own border as an underlay anchor only for its own group.
    #[must_use]
    pub fn target_token(&self) -> Option<String> {
        self.target_token.clone()
    }

    /// Checksum over the owned DIB bytes actually presented (0 when hidden).
    /// Renderer-ack evidence: the owner logs acknowledged checksums, never
    /// desired state masquerading as rendered pixels.
    #[must_use]
    pub fn dib_checksum(&self) -> u64 {
        self.dib_checksum
    }

    /// Hide the surface (no-op when already hidden). Keeps the window alive
    /// for the next target; failures count boundedly.
    pub fn hide(&mut self) {
        if self.outer.is_none() {
            return;
        }
        self.outer = None;
        self.target_token = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Creator-side verified hide: drops state and reads back hidden. True
    /// only when no owned surface reads visible afterwards; the renderer
    /// acknowledgement behind every hide wait.
    pub fn hide_verified(&mut self) -> bool {
        if !self.hwnd.is_none_or(hide_window) {
            return false;
        }
        self.outer = None;
        self.target_token = None;
        true
    }

    /// Creator-side ensure: create the hidden carrier window when absent so
    /// readiness never signals before the carriers exist. Never shows:
    /// visibility still starts on the first show. An externally destroyed
    /// surface drops its stale handle and recreates here.
    pub fn ensure_created(&mut self) -> Result<u64, DynError> {
        if let Some(hwnd) = self.hwnd
            && is_overlay_window(hwnd)
        {
            return Ok(hwnd);
        }
        self.hwnd = None;
        self.outer = None;
        match create_overlay_window() {
            Ok(hwnd) => {
                self.hwnd = Some(hwnd);
                self.last_error = None;
                Ok(hwnd)
            }
            Err(e) => {
                self.failures += 1;
                self.last_error = Some(truncate_error(&e.to_string()));
                Err(err(self.last_error.clone().unwrap_or_default()))
            }
        }
    }

    /// Destroy the surface explicitly (graceful stop). Process exit also
    /// destroys it implicitly. Verified: true only when the HWND reads
    /// destroyed afterwards; a surviving window keeps its live handle so a
    /// bounded creator-side retry can re-drive it instead of dropping a
    /// live window and claiming success.
    pub fn destroy(&mut self) -> bool {
        let Some(hwnd) = self.hwnd else {
            self.outer = None;
            self.style_px = None;
            self.color = None;
            self.accent = None;
            self.target_token = None;
            self.topmost = false;
            return true;
        };
        if !destroy_window(hwnd) {
            self.failures += 1;
            self.last_error = Some(truncate_error("error: active-border destroy unverified"));
            self.outer = None;
            self.target_token = None;
            return false;
        }
        self.hwnd = None;
        self.outer = None;
        self.style_px = None;
        self.color = None;
        self.accent = None;
        self.target_token = None;
        self.topmost = false;
        true
    }

    /// Show or move the ring for one eligible target. Position-only changes
    /// move without repainting; size/style/colour changes repaint. `topmost`
    /// mirrors the target's `WS_EX_TOPMOST`: a topmost target needs a topmost
    /// ring, an ordinary target must demote a previously topmost ring.
    /// Paint/move failures hide any stale ring and clear state, returning
    /// `Hidden` for bounded caller logging with `last_error`.
    ///
    /// Cached equality never trusts the cache alone: when geometry, style,
    /// colour, token, and band all match, the actual overlay is still probed
    /// (visibility, `GetWindowRect`, z-order against the fresh `target_hwnd`)
    /// and only a truly current surface returns `Unchanged`. A hidden,
    /// misplaced, or displaced surface falls through to a `place_overlay`
    /// reassert in the target's own band, directly beneath the target:
    /// never over unrelated topmost or shell UI. No foreign attribute
    /// writes; `target_hwnd` is the refresh's fresh foreground HWND, never
    /// a stale cache.
    #[allow(clippy::too_many_arguments)]
    pub fn show_at(
        &mut self,
        outer: Rect,
        width_px: i32,
        gap_px: i32,
        radius_px: i32,
        style: &ActiveBorderStyle,
        target_token: &str,
        topmost: bool,
        target_hwnd: u64,
    ) -> OverlayOutcome {
        // Accent first, before the equality check: a theme change must miss
        // neither the repaint (colour comparison below) nor the first show.
        // Sticky flag plus first-show query means no broadcast is missed; the
        // comparison makes an unchanged accent a no-op without repaint.
        if self.accent.is_none() || take_accent_dirty() {
            self.accent = query_system_accent();
        }
        let color = effective_color(style, self.accent);
        let style_px = (width_px, gap_px, radius_px);
        if self.outer == Some(outer)
            && self.style_px == Some(style_px)
            && self.color == Some(color)
            && self.target_token.as_deref() == Some(target_token)
            && self.topmost == topmost
        {
            match self.hwnd {
                Some(hwnd) if !overlay_needs_reassert(hwnd, outer, target_hwnd, false) => {
                    return OverlayOutcome::Unchanged;
                }
                Some(dead) if !is_overlay_window(dead) => {
                    // Externally destroyed surface: drop the stale handle so
                    // the path below recreates instead of moving a dead HWND.
                    self.hwnd = None;
                    self.outer = None;
                }
                _ => {}
            }
        }
        let hwnd = match self.hwnd {
            Some(hwnd) => hwnd,
            None => match create_overlay_window() {
                Ok(hwnd) => {
                    self.hwnd = Some(hwnd);
                    self.last_error = None;
                    hwnd
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            },
        };
        let first_show = self.outer.is_none();
        let size_changed = first_show
            || self.outer.is_none_or(|o| o.w != outer.w || o.h != outer.h)
            || self.style_px != Some(style_px)
            || self.color != Some(color);
        if size_changed {
            match paint_and_present(
                hwnd,
                outer,
                width_px,
                radius_px,
                color,
                topmost,
                target_hwnd,
            ) {
                Ok(checksum) => {
                    self.dib_checksum = checksum;
                    self.redraws += 1;
                    self.last_error = None;
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            }
        } else if let Err(e) = move_overlay(hwnd, outer, topmost, target_hwnd) {
            self.failures += 1;
            self.last_error = Some(truncate_error(&e.to_string()));
            self.fail_hide();
            return OverlayOutcome::Hidden;
        } else {
            self.moves += 1;
        }
        self.outer = Some(outer);
        self.style_px = Some(style_px);
        self.color = Some(color);
        self.target_token = Some(target_token.to_owned());
        self.topmost = topmost;
        if first_show {
            OverlayOutcome::Shown
        } else if size_changed {
            OverlayOutcome::Redrew
        } else {
            OverlayOutcome::Moved
        }
    }

    fn fail_hide(&mut self) {
        self.outer = None;
        self.target_token = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Read-only owned-surface snapshot for proof support: geometry, style,
    /// opaque target token, and a checksum over our own DIB pixels only.
    /// No screen capture, no titles, no content, no foreign IDs.
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "visible": self.is_visible(),
            "outer": self.outer.map(|r| [r.x, r.y, r.w, r.h]),
            "style_px": self.style_px.map(|(w, g, r)| [w, g, r]),
            "color": self.color.map(crate::active_border::render_color),
            "accent": self.accent.map(|a| crate::active_border::render_color(a.color)),
            "target": self.target_token.clone(),
            "dib_checksum": format!("{:016x}", self.dib_checksum),
            "redraws": self.redraws,
            "moves": self.moves,
            "failures": self.failures,
            "last_error": self.last_error.clone(),
        })
    }
}

/// Process-owned group-underlay lifecycle on the shared carrier: one
/// `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
/// popup in its own window class, filled with premultiplied alpha and placed
/// directly beneath the lowest renderable group member (KWin z=-2 analogue,
/// below the active border at z=-1). Creation, placement, hide, and teardown
/// reuse the border carrier paths; only the fill painter differs.
#[derive(Default)]
pub struct UnderlayOverlay {
    hwnd: Option<u64>,
    outer: Option<Rect>,
    color: Option<(u8, u8, u8, u8)>,
    anchor_token: Option<String>,
    anchor_hwnd: Option<u64>,
    topmost: bool,
    dib_checksum: u64,
    redraws: u64,
    moves: u64,
    failures: u64,
    last_error: Option<String>,
}

impl UnderlayOverlay {
    #[must_use]
    pub fn hwnd(&self) -> Option<u64> {
        self.hwnd
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Checksum over the owned DIB bytes actually presented (0 when hidden).
    /// Renderer-ack evidence, mirroring the border carrier.
    #[must_use]
    pub fn dib_checksum(&self) -> u64 {
        self.dib_checksum
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.outer.is_some()
    }

    /// Currently presented fill colour (actually painted bytes), if shown.
    #[must_use]
    pub fn current_fill(&self) -> Option<(u8, u8, u8, u8)> {
        self.color
    }

    /// Opaque anchor token currently shown, if any.
    #[must_use]
    pub fn anchor_token(&self) -> Option<String> {
        self.anchor_token.clone()
    }

    /// Hide the surface (no-op when already hidden). Keeps the window alive
    /// for the next group; failures count boundedly.
    pub fn hide(&mut self) {
        if self.outer.is_none() {
            return;
        }
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Creator-side verified hide, mirroring the border carrier.
    pub fn hide_verified(&mut self) -> bool {
        if !self.hwnd.is_none_or(hide_window) {
            return false;
        }
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        true
    }

    /// Creator-side ensure for the underlay class, mirroring the border.
    pub fn ensure_created(&mut self) -> Result<u64, DynError> {
        if let Some(hwnd) = self.hwnd
            && is_overlay_window(hwnd)
        {
            return Ok(hwnd);
        }
        self.hwnd = None;
        self.outer = None;
        match create_overlay_window_in(UNDERLAY_CLASS) {
            Ok(hwnd) => {
                self.hwnd = Some(hwnd);
                self.last_error = None;
                Ok(hwnd)
            }
            Err(e) => {
                self.failures += 1;
                self.last_error = Some(truncate_error(&e.to_string()));
                Err(err(self.last_error.clone().unwrap_or_default()))
            }
        }
    }

    /// Destroy the surface explicitly (graceful stop). Process exit also
    /// destroys it implicitly. Verified like the border carrier: a surviving
    /// window keeps its live handle for a bounded retry.
    pub fn destroy(&mut self) -> bool {
        let Some(hwnd) = self.hwnd else {
            self.outer = None;
            self.color = None;
            self.anchor_token = None;
            self.anchor_hwnd = None;
            self.topmost = false;
            return true;
        };
        if !destroy_window(hwnd) {
            self.failures += 1;
            self.last_error = Some(truncate_error("error: group-underlay destroy unverified"));
            self.outer = None;
            self.anchor_token = None;
            self.anchor_hwnd = None;
            return false;
        }
        self.hwnd = None;
        self.outer = None;
        self.color = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        self.topmost = false;
        true
    }

    /// Show or move the fill for one resolved group. `anchor_hwnd` is the
    /// lowest renderable member (or the border surface, whichever sorts
    /// lower): the underlay lands directly beneath it. Cached equality still
    /// probes the actual surface (visibility, rect, z against the fresh
    /// anchor) and reasserts a hidden, misplaced, or displaced surface.
    /// Paint/move failures hide any stale fill and return `Hidden` with
    /// `last_error` for bounded caller logging. `color` is `(alpha, r, g, b)`.
    pub fn show_fill_at(
        &mut self,
        outer: Rect,
        color: (u8, u8, u8, u8),
        anchor_token: &str,
        topmost: bool,
        anchor_hwnd: u64,
    ) -> OverlayOutcome {
        if self.outer == Some(outer)
            && self.color == Some(color)
            && self.anchor_token.as_deref() == Some(anchor_token)
            && self.anchor_hwnd == Some(anchor_hwnd)
            && self.topmost == topmost
        {
            match self.hwnd {
                Some(hwnd) if !overlay_needs_reassert(hwnd, outer, anchor_hwnd, true) => {
                    return OverlayOutcome::Unchanged;
                }
                Some(dead) if !is_overlay_window(dead) => {
                    self.hwnd = None;
                    self.outer = None;
                }
                _ => {}
            }
        }
        let hwnd = match self.hwnd {
            Some(hwnd) => hwnd,
            None => match create_overlay_window_in(UNDERLAY_CLASS) {
                Ok(hwnd) => {
                    self.hwnd = Some(hwnd);
                    self.last_error = None;
                    hwnd
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            },
        };
        let first_show = self.outer.is_none();
        let size_changed = first_show
            || self.outer.is_none_or(|o| o.w != outer.w || o.h != outer.h)
            || self.color != Some(color);
        if size_changed {
            match paint_and_present_fill(
                hwnd,
                outer,
                (color.1, color.2, color.3),
                color.0,
                topmost,
                anchor_hwnd,
            ) {
                Ok(checksum) => {
                    self.dib_checksum = checksum;
                    self.redraws += 1;
                    self.last_error = None;
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            }
        } else if let Err(e) = move_overlay(hwnd, outer, topmost, anchor_hwnd) {
            self.failures += 1;
            self.last_error = Some(truncate_error(&e.to_string()));
            self.fail_hide();
            return OverlayOutcome::Hidden;
        } else {
            self.moves += 1;
        }
        self.outer = Some(outer);
        self.color = Some(color);
        self.anchor_token = Some(anchor_token.to_owned());
        self.anchor_hwnd = Some(anchor_hwnd);
        self.topmost = topmost;
        if first_show {
            OverlayOutcome::Shown
        } else if size_changed {
            OverlayOutcome::Redrew
        } else {
            OverlayOutcome::Moved
        }
    }

    fn fail_hide(&mut self) {
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Read-only owned-surface snapshot for proof support: geometry, colour,
    /// opaque anchor token, and a checksum over our own DIB pixels only.
    /// No screen capture, no titles, no content, no raw window identifiers.
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "visible": self.is_visible(),
            "outer": self.outer.map(|r| [r.x, r.y, r.w, r.h]),
            "color": self.color.map(|(a, r, g, b)| crate::group_underlay::render_color_argb(((r, g, b), a))),
            "target": self.anchor_token.clone(),
            "dib_checksum": format!("{:016x}", self.dib_checksum),
            "redraws": self.redraws,
            "moves": self.moves,
            "failures": self.failures,
            "last_error": self.last_error.clone(),
        })
    }
}

/// Process-owned drop-preview lifecycle on the shared carrier: one
/// `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
/// popup in its own window class, filled with premultiplied alpha and placed
/// ABOVE windows (KWin overlay-item analogue, above the active border).
/// Creation, presentation, hide, and teardown reuse the border/underlay
/// carrier paths; only the fill colour (default KDE `#402A82DA`) and the
/// above-target placement differ. Never focuses, never takes geometry
/// writes, never mixes with the border/underlay below-target plane.
#[derive(Default)]
pub struct PreviewOverlay {
    hwnd: Option<u64>,
    outer: Option<Rect>,
    color: Option<(u8, u8, u8, u8)>,
    anchor_token: Option<String>,
    anchor_hwnd: Option<u64>,
    dib_checksum: u64,
    redraws: u64,
    moves: u64,
    failures: u64,
    last_error: Option<String>,
}

impl PreviewOverlay {
    #[must_use]
    pub fn hwnd(&self) -> Option<u64> {
        self.hwnd
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Checksum over the owned DIB bytes actually presented (0 when hidden).
    /// Renderer-ack evidence, mirroring the border carrier.
    #[must_use]
    pub fn dib_checksum(&self) -> u64 {
        self.dib_checksum
    }

    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.outer.is_some()
    }

    /// Currently presented fill colour (actually painted bytes), if shown.
    #[must_use]
    pub fn current_fill(&self) -> Option<(u8, u8, u8, u8)> {
        self.color
    }

    /// Opaque anchor token currently shown, if any.
    #[must_use]
    pub fn anchor_token(&self) -> Option<String> {
        self.anchor_token.clone()
    }

    /// Hide the surface (no-op when already hidden). Keeps the window alive
    /// for the next target; failures count boundedly.
    pub fn hide(&mut self) {
        if self.outer.is_none() {
            return;
        }
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Creator-side verified hide, mirroring the border carrier.
    pub fn hide_verified(&mut self) -> bool {
        if !self.hwnd.is_none_or(hide_window) {
            return false;
        }
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        true
    }

    /// Creator-side ensure for the preview class, mirroring the border.
    pub fn ensure_created(&mut self) -> Result<u64, DynError> {
        if let Some(hwnd) = self.hwnd
            && is_overlay_window(hwnd)
        {
            return Ok(hwnd);
        }
        self.hwnd = None;
        self.outer = None;
        match create_overlay_window_in(PREVIEW_CLASS) {
            Ok(hwnd) => {
                self.hwnd = Some(hwnd);
                self.last_error = None;
                Ok(hwnd)
            }
            Err(e) => {
                self.failures += 1;
                self.last_error = Some(truncate_error(&e.to_string()));
                Err(err(self.last_error.clone().unwrap_or_default()))
            }
        }
    }

    /// Destroy the surface explicitly (graceful stop). Process exit also
    /// destroys it implicitly. Verified like the border carrier.
    pub fn destroy(&mut self) -> bool {
        let Some(hwnd) = self.hwnd else {
            self.outer = None;
            self.color = None;
            self.anchor_token = None;
            self.anchor_hwnd = None;
            return true;
        };
        if !destroy_window(hwnd) {
            self.failures += 1;
            self.last_error = Some(truncate_error("error: drop-preview destroy unverified"));
            self.outer = None;
            self.anchor_token = None;
            self.anchor_hwnd = None;
            return false;
        }
        self.hwnd = None;
        self.outer = None;
        self.color = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        true
    }

    /// Show or move the filled target-slot rectangle above windows.
    /// `anchor_hwnd` is the gesture mover (identity fence only, never
    /// repositioned below it: the preview always presents in the topmost
    /// band so it stays above normal app windows). Degenerate bounds fail
    /// closed to `Hidden`. Cached equality still probes the actual surface
    /// (visibility plus rect) and re-presents a hidden or misplaced
    /// surface. Paint/move failures hide any stale fill and return
    /// `Hidden` with `last_error`. `color` is `(alpha, r, g, b)`.
    pub fn show_fill_above(
        &mut self,
        outer: Rect,
        color: (u8, u8, u8, u8),
        anchor_token: &str,
        anchor_hwnd: u64,
    ) -> OverlayOutcome {
        if !preview_bounds_valid(outer) {
            self.failures += 1;
            self.last_error = Some(truncate_error("error: drop-preview bad paint geometry"));
            self.fail_hide();
            return OverlayOutcome::Hidden;
        }
        if self.outer == Some(outer)
            && self.color == Some(color)
            && self.anchor_token.as_deref() == Some(anchor_token)
            && self.anchor_hwnd == Some(anchor_hwnd)
        {
            match self.hwnd {
                Some(hwnd) if !preview_needs_reassert(hwnd, outer) => {
                    return OverlayOutcome::Unchanged;
                }
                Some(dead) if !is_overlay_window(dead) => {
                    self.hwnd = None;
                    self.outer = None;
                }
                _ => {}
            }
        }
        let hwnd = match self.hwnd {
            Some(hwnd) => hwnd,
            None => match create_overlay_window_in(PREVIEW_CLASS) {
                Ok(hwnd) => {
                    self.hwnd = Some(hwnd);
                    self.last_error = None;
                    hwnd
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            },
        };
        let first_show = self.outer.is_none();
        let size_changed = first_show
            || self.outer.is_none_or(|o| o.w != outer.w || o.h != outer.h)
            || self.color != Some(color);
        if size_changed {
            match paint_and_present_preview_fill(hwnd, outer, (color.1, color.2, color.3), color.0)
            {
                Ok(checksum) => {
                    self.dib_checksum = checksum;
                    self.redraws += 1;
                    self.last_error = None;
                }
                Err(e) => {
                    self.failures += 1;
                    self.last_error = Some(truncate_error(&e.to_string()));
                    self.fail_hide();
                    return OverlayOutcome::Hidden;
                }
            }
        } else if let Err(e) = place_preview_above(hwnd, outer) {
            self.failures += 1;
            self.last_error = Some(truncate_error(&e.to_string()));
            self.fail_hide();
            return OverlayOutcome::Hidden;
        } else {
            self.moves += 1;
        }
        self.outer = Some(outer);
        self.color = Some(color);
        self.anchor_token = Some(anchor_token.to_owned());
        self.anchor_hwnd = Some(anchor_hwnd);
        if first_show {
            OverlayOutcome::Shown
        } else if size_changed {
            OverlayOutcome::Redrew
        } else {
            OverlayOutcome::Moved
        }
    }

    fn fail_hide(&mut self) {
        self.outer = None;
        self.anchor_token = None;
        self.anchor_hwnd = None;
        if let Some(hwnd) = self.hwnd {
            hide_window(hwnd);
        }
    }

    /// Read-only owned-surface snapshot for proof support: geometry, colour,
    /// opaque anchor token, and a checksum over our own DIB pixels only.
    /// No screen capture, no titles, no content, no raw window identifiers.
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "visible": self.is_visible(),
            "outer": self.outer.map(|r| [r.x, r.y, r.w, r.h]),
            "color": self.color.map(|(a, r, g, b)| crate::group_underlay::render_color_argb(((r, g, b), a))),
            "target": self.anchor_token.clone(),
            "dib_checksum": format!("{:016x}", self.dib_checksum),
            "redraws": self.redraws,
            "moves": self.moves,
            "failures": self.failures,
            "last_error": self.last_error.clone(),
        })
    }
}

fn truncate_error(message: &str) -> String {
    const MAX: usize = 160;
    let mut out: String = message.chars().take(MAX).collect();
    if message.chars().count() > MAX {
        out.push_str("...");
    }
    out
}

/// `PRIME^exp mod 2^64` by wrapping binary exponentiation. One FNV-1a step
/// over a zero byte is `hash = hash * PRIME` (xor with 0 is the identity),
/// so a run of `exp` zero bytes collapses to a single multiply by this.
fn fnv_prime_pow(mut exp: usize) -> u64 {
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut base = FNV_PRIME;
    let mut acc = 1u64;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = acc.wrapping_mul(base);
        }
        base = base.wrapping_mul(base);
        exp >>= 1;
    }
    acc
}

/// One FNV-1a mix step over an owned byte.
#[inline]
fn fnv_mix(hash: &mut u64, byte: u8) {
    *hash ^= u64::from(byte);
    *hash = hash.wrapping_mul(0x100000001b3);
}

/// Paint one already-evaluated pixel: ring bytes plus four FNV-1a mixes when
/// covered, otherwise a single `PRIME^4` advance over the four zero bytes
/// left by the bulk clear. Byte- and checksum-identical to the old
/// evaluate-and-write-everything walk.
#[inline]
fn paint_ring_pixel(
    pixels: &mut [u8],
    hash: &mut u64,
    zero4: u64,
    idx: usize,
    covered: bool,
    color: (u8, u8, u8),
) {
    if covered {
        let (r, g, b) = color;
        pixels[idx] = b;
        pixels[idx + 1] = g;
        pixels[idx + 2] = r;
        pixels[idx + 3] = 255;
        for byte in [b, g, r, 255] {
            fnv_mix(hash, byte);
        }
    } else {
        *hash = hash.wrapping_mul(zero4);
    }
}

/// Fill a BGRA buffer with the ring: colour with full alpha on the ring,
/// fully transparent elsewhere. Returns the FNV-1a checksum over the owned
/// bytes without a separate hash walk.
///
/// Cost: the backing store is cleared in bulk and only the ring bands are
/// evaluated per pixel. Middle rows skip their known-transparent center hole
/// (`x` in `[width, w - width)`) with one hash multiply per row, so large
/// default surfaces do O(perimeter) coverage/hash work (the bulk clear
/// itself stays one O(area) memset).
fn paint_ring(
    pixels: &mut [u8],
    w: i32,
    h: i32,
    width_px: i32,
    radius_px: i32,
    color: (u8, u8, u8),
) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    if w <= 0 || h <= 0 {
        // Matches the old walk: no pixel is visited, the buffer is untouched.
        return hash;
    }
    let total = w as usize * h as usize * 4;
    if width_px <= 0 {
        // ring_covers is false everywhere: bulk clear, one hash multiply.
        pixels[..total].fill(0);
        return hash.wrapping_mul(fnv_prime_pow(total));
    }
    pixels[..total].fill(0);
    let zero4 = fnv_prime_pow(4);
    let inner_w = w - 2 * width_px;
    let inner_h = h - 2 * width_px;
    if inner_w <= 0 || inner_h <= 0 {
        // Thick border with no inner hole: the center may paint, so every
        // pixel is still evaluated (same bytes and hash order as before).
        for y in 0..h {
            let row = y as usize * w as usize * 4;
            for x in 0..w {
                let covered = ring_covers(x, y, w, h, width_px, radius_px);
                paint_ring_pixel(
                    pixels,
                    &mut hash,
                    zero4,
                    row + x as usize * 4,
                    covered,
                    color,
                );
            }
        }
        return hash;
    }
    // Clamped inner radius, mirroring point_in_rounded_rect's
    // `radius.max(0).min(inner_w / 2).min(inner_h / 2)`.
    let inner_r = radius_px.max(0).min(inner_w / 2).min(inner_h / 2);
    // One exponentiation shared by every middle row: the skipped hole below
    // is `inner_w` pixels (four zero bytes each) wide on each such row.
    let gap_pow = fnv_prime_pow(inner_w as usize * 4);
    // Middle rows whose full center span lies inside the inner hole and is
    // therefore transparent whatever the outer corner rounding does: for
    // `y` in this band and `x` in `[width, w - width)`, the inner
    // point-in-rect hits either its horizontal straight section or (in the
    // side columns) its vertical straight section, so `ring_covers` is false
    // and the inner `!...` term decides without consulting the outer rect.
    // Top/bottom rows evaluate the full width.
    let mid_start = width_px + inner_r;
    let mid_end = h - width_px - inner_r;
    for y in 0..h {
        let row = y as usize * w as usize * 4;
        if y >= mid_start && y < mid_end {
            for x in 0..width_px {
                let covered = ring_covers(x, y, w, h, width_px, radius_px);
                paint_ring_pixel(
                    pixels,
                    &mut hash,
                    zero4,
                    row + x as usize * 4,
                    covered,
                    color,
                );
            }
            hash = hash.wrapping_mul(gap_pow);
            for x in w - width_px..w {
                let covered = ring_covers(x, y, w, h, width_px, radius_px);
                paint_ring_pixel(
                    pixels,
                    &mut hash,
                    zero4,
                    row + x as usize * 4,
                    covered,
                    color,
                );
            }
        } else {
            for x in 0..w {
                let covered = ring_covers(x, y, w, h, width_px, radius_px);
                paint_ring_pixel(
                    pixels,
                    &mut hash,
                    zero4,
                    row + x as usize * 4,
                    covered,
                    color,
                );
            }
        }
    }
    hash
}

#[cfg(test)]
#[path = "active_border_sys/tests.rs"]
mod tests;
