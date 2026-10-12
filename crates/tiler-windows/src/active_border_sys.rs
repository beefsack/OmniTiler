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

/// Lowest window in `EnumWindows` top-to-bottom order among `hwnds`: the
/// renderable anchor the underlay sorts directly beneath. `None` when no
/// candidate is enumerated (closed, or the order is unreadable).
pub(crate) fn lowest_in_z(hwnds: &[u64]) -> Option<u64> {
    let order = top_to_bottom_hwnds()?;
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
fn overlay_needs_reassert(
    overlay_u64: u64,
    outer: Rect,
    target_hwnd: u64,
    for_underlay: bool,
) -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};
    if overlay_u64 == 0 {
        return true;
    }
    let overlay = overlay_u64 as usize as HWND;
    if !is_overlay_window(overlay_u64) {
        return true;
    }
    if unsafe { IsWindowVisible(overlay) } == 0 {
        return true;
    }
    let mut actual: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(overlay, &mut actual) } == 0 {
        return true;
    }
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
fn z_needs_reassert(relation: Option<ZRelation>, for_underlay: bool) -> bool {
    match relation {
        Some(ZRelation::AdjacentBelow) => false,
        Some(ZRelation::Above) => for_underlay,
        Some(ZRelation::Below) | None => true,
    }
}

fn is_overlay_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    if hwnd_u64 == 0 {
        return false;
    }
    unsafe { IsWindow(hwnd_u64 as usize as HWND) != 0 }
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
enum ZRelation {
    Above,
    AdjacentBelow,
    Below,
}

/// Z-order probe: relation of the overlay to the fresh target, `None` when
/// either endpoint is gone or the order is unreadable.
fn z_relation(overlay_u64: u64, target_hwnd: u64) -> Option<ZRelation> {
    let order = top_to_bottom_hwnds()?;
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

/// Shared `EnumWindows` top-to-bottom snapshot for z-order evidence.
fn top_to_bottom_hwnds() -> Option<Vec<u64>> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
    unsafe extern "system" fn enum_proc(hwnd: HWND, state: LPARAM) -> i32 {
        let out = unsafe { &mut *(state as *mut Vec<u64>) };
        out.push(hwnd as usize as u64);
        1
    }
    let mut out: Vec<u64> = Vec::new();
    let ok = unsafe { EnumWindows(Some(enum_proc), &mut out as *mut Vec<u64> as LPARAM) };
    if ok == 0 { None } else { Some(out) }
}

fn truncate_error(message: &str) -> String {
    const MAX: usize = 160;
    let mut out: String = message.chars().take(MAX).collect();
    if message.chars().count() > MAX {
        out.push_str("...");
    }
    out
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn create_overlay_window() -> Result<u64, DynError> {
    create_overlay_window_in(OVERLAY_CLASS)
}

static BORDER_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static UNDERLAY_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static PREVIEW_CLASS_REGISTERED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn overlay_class_registered(class: &str) -> bool {
    use std::sync::atomic::Ordering;
    if class == UNDERLAY_CLASS {
        UNDERLAY_CLASS_REGISTERED.load(Ordering::SeqCst)
    } else if class == PREVIEW_CLASS {
        PREVIEW_CLASS_REGISTERED.load(Ordering::SeqCst)
    } else {
        BORDER_CLASS_REGISTERED.load(Ordering::SeqCst)
    }
}

fn mark_overlay_class_registered(class: &str) {
    use std::sync::atomic::Ordering;
    if class == UNDERLAY_CLASS {
        UNDERLAY_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    } else if class == PREVIEW_CLASS {
        PREVIEW_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    } else {
        BORDER_CLASS_REGISTERED.store(true, Ordering::SeqCst);
    }
}

/// Create one owned carrier window in `class`. Registration is attempted on
/// every creation and `ERROR_CLASS_ALREADY_EXISTS` (1410) is tolerated, so
/// the border and underlay classes each register exactly once via the two
/// per-class `AtomicBool`s above (`BORDER_CLASS_REGISTERED` /
/// `UNDERLAY_CLASS_REGISTERED`). Creation is rare (at most once per carrier
/// per process).
pub(crate) fn create_overlay_window_in(class: &str) -> Result<u64, DynError> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_LAYERED,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    };
    if !overlay_class_registered(class) {
        let class_w = wide(class);
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.style = CS_HREDRAW | CS_VREDRAW;
        cls.lpfnWndProc = Some(overlay_wnd_proc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 {
            // Tolerate a lost race / prior registration in this process:
            // ERROR_CLASS_ALREADY_EXISTS (1410) means the class is usable.
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if code != 1410 {
                return Err(err("error: overlay RegisterClass failed"));
            }
        }
        mark_overlay_class_registered(class);
    }
    let class_w = wide(class);
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_w.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(err("error: active-border CreateWindow failed"));
    }
    Ok(hwnd as usize as u64)
}

fn hide_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};
    unsafe {
        ShowWindow(hwnd_u64 as usize as HWND, SW_HIDE);
    }
    window_hidden(hwnd_u64)
}

/// True when the owned window reads hidden (or is already gone): the
/// creator-side readback behind every verified hide acknowledgement.
fn window_hidden(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    if !is_overlay_window(hwnd_u64) {
        return true;
    }
    unsafe { IsWindowVisible(hwnd_u64 as usize as HWND) == 0 }
}

fn destroy_window(hwnd_u64: u64) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
    unsafe {
        DestroyWindow(hwnd_u64 as usize as HWND);
    }
    // Verified destroyed readback: a surviving HWND reads live so the
    // caller preserves the handle for a bounded retry instead of claiming
    // success over a live window.
    !is_overlay_window(hwnd_u64)
}

/// Z-order anchors: ordinary targets stay in the normal band and sit directly
/// beneath the target; topmost targets use the topmost band the same way.
/// Live probes proved `HWND_TOP` a silent no-op for z-order here (a direct
/// `SetWindowPos(TOP)` from the background owner succeeds yet never
/// reorders), while passing the fresh target HWND as `hWndInsertAfter`
/// places the overlay immediately below it. The band still follows the
/// target every paint/move, never permanently over unrelated topmost/shell.
fn insert_after(topmost: bool) -> windows_sys::Win32::Foundation::HWND {
    if topmost {
        windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST as usize
            as windows_sys::Win32::Foundation::HWND
    } else {
        windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOP as usize
            as windows_sys::Win32::Foundation::HWND
    }
}

/// Position the owned overlay directly beneath its fresh target without
/// leaving its band. Ordinary targets: a `NOTOPMOST` demote-if-needed (no-op
/// when already normal) followed by placement with the target HWND as
/// `hWndInsertAfter` (immediately below the target). Topmost targets: a
/// `TOPMOST` promote-if-needed followed by the same below-target placement
/// inside the topmost band. A zero `target_hwnd` falls back to the band
/// anchor (best effort, no adjacency). Read-only band input, owned-window
/// writes only, never over unrelated topmost or shell UI.
fn place_overlay(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
) -> Result<(), DynError> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    };
    let band = |after: HWND| unsafe {
        SetWindowPos(
            hwnd_u64 as usize as HWND,
            after,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    if topmost {
        if band(insert_after(true)) == 0 {
            return Err(err("error: active-border position failed"));
        }
    } else if band(HWND_NOTOPMOST as usize as HWND) == 0 {
        return Err(err("error: active-border position failed"));
    }
    if target_hwnd != 0 && target_hwnd != hwnd_u64 {
        let below_target = unsafe {
            SetWindowPos(
                hwnd_u64 as usize as HWND,
                target_hwnd as usize as HWND,
                outer.x,
                outer.y,
                outer.w,
                outer.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
        if below_target == 0 {
            return Err(err("error: active-border position failed"));
        }
        return Ok(());
    }
    if band(insert_after(topmost)) == 0 {
        return Err(err("error: active-border position failed"));
    }
    Ok(())
}

fn move_overlay(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
) -> Result<(), DynError> {
    place_overlay(hwnd_u64, outer, topmost, target_hwnd)
}

/// Paint the ring into a fresh 32bpp top-down DIB and present it with
/// `UpdateLayeredWindow` (`ULW_ALPHA`). Returns an FNV-1a checksum over the
/// owned DIB bytes (proof support without screen capture), computed in the
/// single paint pass (no second full-buffer hash walk).
fn paint_and_present(
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
    present_pixels(
        hwnd_u64,
        outer,
        topmost,
        target_hwnd,
        "active-border",
        |pixels| paint_ring(pixels, outer.w, outer.h, width_px, radius_px, color),
    )
}

/// Paint the premultiplied-alpha group fill into a fresh 32bpp top-down DIB
/// and present it on the shared carrier pipeline. Same checksum/placement
/// contract as the ring path; only the painter differs.
fn paint_and_present_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
    topmost: bool,
    anchor_hwnd: u64,
) -> Result<u64, DynError> {
    present_pixels(
        hwnd_u64,
        outer,
        topmost,
        anchor_hwnd,
        "group-underlay",
        |pixels| crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha),
    )
}

/// Position the drop-preview surface ABOVE windows in the topmost band
/// (KWin overlay-item analogue). Unlike [`place_overlay`] there is no
/// below-target step: the filled target slot must stay above normal app
/// windows while shown. `SWP_NOACTIVATE` keeps it nonactivating; the
/// `WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` creation
/// flags keep it click-through and out of Alt+Tab. Callers hide immediately
/// on cancel/finish/suspend/teardown so the topmost band is never pinned
/// over unrelated shell UI.
fn place_preview_above(hwnd_u64: u64, outer: Rect) -> Result<(), DynError> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetWindowPos,
    };
    let ok = unsafe {
        SetWindowPos(
            hwnd_u64 as usize as HWND,
            HWND_TOPMOST as usize as HWND,
            outer.x,
            outer.y,
            outer.w,
            outer.h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    if ok == 0 {
        return Err(err("error: drop-preview position failed"));
    }
    Ok(())
}

/// Paint the premultiplied-alpha drop-preview fill and present it ABOVE
/// windows on the shared DIB pipeline, then assert the topmost band.
/// Same checksum contract as the underlay path; only the placement differs.
fn paint_and_present_preview_fill(
    hwnd_u64: u64,
    outer: Rect,
    color: (u8, u8, u8),
    alpha: u8,
) -> Result<u64, DynError> {
    use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    if outer.w <= 0 || outer.h <= 0 {
        return Err(err("error: drop-preview bad paint geometry"));
    }
    if outer.w > 16384 || outer.h > 16384 {
        return Err(err("error: drop-preview surface too large"));
    }
    let memdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if memdc.is_null() {
        return Err(err("error: drop-preview DC failed"));
    }
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = outer.w;
    bmi.bmiHeader.biHeight = -outer.h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = unsafe {
        CreateDIBSection(
            memdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if hbmp.is_null() || bits.is_null() {
        unsafe {
            DeleteDC(memdc);
        }
        return Err(err("error: drop-preview DIB failed"));
    }
    let stride = outer.w as usize * 4;
    let total = stride * outer.h as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, total) };
    let checksum = crate::group_underlay::paint_fill_argb(pixels, outer.w, outer.h, color, alpha);
    let old = unsafe { SelectObject(memdc, hbmp) };
    let dst = POINT {
        x: outer.x,
        y: outer.y,
    };
    let size = SIZE {
        cx: outer.w,
        cy: outer.h,
    };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let presented = unsafe {
        UpdateLayeredWindow(
            hwnd_u64 as usize as HWND,
            std::ptr::null_mut(),
            &dst,
            &size,
            memdc,
            &src,
            0,
            &blend,
            ULW_ALPHA,
        )
    };
    unsafe {
        SelectObject(memdc, old);
        DeleteObject(hbmp);
        DeleteDC(memdc);
    }
    if presented == 0 {
        return Err(err("error: drop-preview present failed"));
    }
    place_preview_above(hwnd_u64, outer)?;
    Ok(checksum)
}

/// True only when the cached preview needs a same-geometry reassert: the
/// surface is gone, hidden, or misplaced. Z-order against the mover is not
/// probed here (the preview presents topmost by construction); a hidden or
/// misplaced surface re-presents, anything else stays as-is.
fn preview_needs_reassert(preview_u64: u64, outer: Rect) -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};
    if preview_u64 == 0 || !is_overlay_window(preview_u64) {
        return true;
    }
    if unsafe { IsWindowVisible(preview_u64 as usize as HWND) } == 0 {
        return true;
    }
    let mut actual: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(preview_u64 as usize as HWND, &mut actual) } == 0 {
        return true;
    }
    let w = actual.right - actual.left;
    let h = actual.bottom - actual.top;
    actual.left != outer.x || actual.top != outer.y || w != outer.w || h != outer.h
}

/// Shared carrier presentation: allocate one 32bpp top-down DIB, run the
/// caller painter over the owned bytes (it returns the FNV-1a checksum),
/// present with `UpdateLayeredWindow` (`ULW_ALPHA`), then anchor below the
/// target in its band.
fn present_pixels(
    hwnd_u64: u64,
    outer: Rect,
    topmost: bool,
    target_hwnd: u64,
    what: &str,
    paint: impl FnOnce(&mut [u8]) -> u64,
) -> Result<u64, DynError> {
    use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    if outer.w <= 0 || outer.h <= 0 {
        return Err(err(format!("error: {what} bad paint geometry")));
    }
    // Bound the surface: a full-monitor overlay plus ring is the largest
    // legitimate case; anything absurd fails closed instead of allocating.
    if outer.w > 16384 || outer.h > 16384 {
        return Err(err(format!("error: {what} surface too large")));
    }
    let memdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
    if memdc.is_null() {
        return Err(err(format!("error: {what} DC failed")));
    }
    let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = outer.w;
    bmi.bmiHeader.biHeight = -outer.h;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = unsafe {
        CreateDIBSection(
            memdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if hbmp.is_null() || bits.is_null() {
        unsafe {
            DeleteDC(memdc);
        }
        return Err(err(format!("error: {what} DIB failed")));
    }
    let stride = outer.w as usize * 4;
    let total = stride * outer.h as usize;
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, total) };
    let checksum = paint(pixels);
    let old = unsafe { SelectObject(memdc, hbmp) };
    let dst = POINT {
        x: outer.x,
        y: outer.y,
    };
    let size = SIZE {
        cx: outer.w,
        cy: outer.h,
    };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let presented = unsafe {
        UpdateLayeredWindow(
            hwnd_u64 as usize as HWND,
            std::ptr::null_mut(),
            &dst,
            &size,
            memdc,
            &src,
            0,
            &blend,
            ULW_ALPHA,
        )
    };
    unsafe {
        SelectObject(memdc, old);
        DeleteObject(hbmp);
        DeleteDC(memdc);
    }
    if presented == 0 {
        return Err(err(format!("error: {what} present failed")));
    }
    // Target-relative anchor (not blindly permanent TOPMOST): above the
    // ordinary frame, topmost only while the target is topmost. Task
    // View/Alt+Tab interplay needs live evidence.
    place_overlay(hwnd_u64, outer, topmost, target_hwnd)?;
    Ok(checksum)
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
mod tests {
    use super::*;

    #[test]
    fn accent_dirty_flag_is_sticky_until_consumed() {
        // WndProc broadcast path without a live window: marks coalesce into
        // one refresh, and consumption resets exactly once.
        assert!(!take_accent_dirty());
        mark_accent_dirty();
        mark_accent_dirty();
        assert!(take_accent_dirty());
        assert!(!take_accent_dirty());
    }

    #[test]
    fn z_verdict_repairs_underlay_above_but_preserves_border() {
        use ZRelation::{Above, AdjacentBelow, Below};
        // Adjacent-below is the only acceptable cached placement for both.
        assert!(!z_needs_reassert(Some(AdjacentBelow), false));
        assert!(!z_needs_reassert(Some(AdjacentBelow), true));
        // Far-below and unreadable probes reassert for both carriers.
        assert!(z_needs_reassert(Some(Below), false));
        assert!(z_needs_reassert(Some(Below), true));
        assert!(z_needs_reassert(None, false));
        assert!(z_needs_reassert(None, true));
        // Above-anchor: border preserves its foreground-below assumption,
        // the underlay (lowest-member anchor) must never accept it.
        assert!(!z_needs_reassert(Some(Above), false));
        assert!(z_needs_reassert(Some(Above), true));
    }

    #[test]
    fn preview_default_matches_kde_argb402a82da() {
        // KDE parity: `#2A82DA` at alpha 64, i.e. ARGB `#402A82DA`.
        assert_eq!(PREVIEW_DEFAULT_ARGB, (0x40, 0x2A, 0x82, 0xDA));
        assert_eq!(preview_default_fill(), ((0x2A, 0x82, 0xDA), 0x40));
        assert_eq!(
            crate::group_underlay::render_color_argb(((0x2A, 0x82, 0xDA), 0x40)),
            "#402a82da"
        );
        // Distinct carrier class: inspect and residue audits count each
        // surface exactly; the preview never shares the border/underlay
        // below-target plane.
        assert_eq!(PREVIEW_CLASS, "OmniTilerDropPreview");
        assert_ne!(PREVIEW_CLASS, OVERLAY_CLASS);
        assert_ne!(PREVIEW_CLASS, UNDERLAY_CLASS);
    }

    #[test]
    fn preview_fill_raster_is_premultiplied_kde_blue() {
        // Straight-to-premultiplied with rounding: (c * 0x40 + 127) / 255.
        // 0x2A*0x40/255 is 0x0B, 0x82*0x40/255 is 0x21, 0xDA*0x40/255 is
        // 0x37, over BGRA order with the alpha byte itself.
        let ((r, g, b), alpha) = preview_default_fill();
        let mut pixels = vec![0u8; 2 * 2 * 4];
        let checksum = crate::group_underlay::paint_fill_argb(&mut pixels, 2, 2, (r, g, b), alpha);
        assert_eq!(
            &pixels[0..4],
            &[0x37, 0x21, 0x0B, 0x40],
            "BGRA premultiplied"
        );
        assert_eq!(pixels.len(), 16);
        let mut other = vec![0u8; 2 * 2 * 4];
        assert_eq!(
            crate::group_underlay::paint_fill_argb(&mut other, 2, 2, (r, g, b), alpha),
            checksum
        );
    }

    #[test]
    fn preview_degenerate_bounds_gate() {
        // Non-positive or absurd surfaces never present: the caller hides.
        let rect = |x: i32, y: i32, w: i32, h: i32| Rect { x, y, w, h };
        assert!(preview_bounds_valid(rect(0, 0, 100, 80)));
        assert!(!preview_bounds_valid(rect(0, 0, 0, 80)));
        assert!(!preview_bounds_valid(rect(0, 0, 100, 0)));
        assert!(!preview_bounds_valid(rect(0, 0, -5, 80)));
        assert!(!preview_bounds_valid(rect(0, 0, 16385, 80)));
        assert!(!preview_bounds_valid(rect(0, 0, 100, 16385)));
        // A fresh overlay hides closed on a degenerate show: no surface,
        // no panic, exactly one counted failure.
        let mut overlay = PreviewOverlay::default();
        assert_eq!(
            overlay.show_fill_above(rect(0, 0, 0, 80), PREVIEW_DEFAULT_ARGB, "w1", 7),
            OverlayOutcome::Hidden
        );
        assert!(!overlay.is_visible());
        assert!(overlay.hwnd().is_none());
    }

    /// Verbatim pre-optimization exhaustive painter: evaluates `ring_covers`
    /// and mixes four bytes per pixel. Test oracle only (no Win32 calls);
    /// `paint_ring` must stay byte- and checksum-identical to this.
    fn paint_ring_oracle(
        pixels: &mut [u8],
        w: i32,
        h: i32,
        width_px: i32,
        radius_px: i32,
        color: (u8, u8, u8),
    ) -> u64 {
        let (r, g, b) = color;
        let mut hash: u64 = 0xcbf29ce484222325;
        let mut mix = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        };
        for y in 0..h {
            for x in 0..w {
                let idx = (y as usize * w as usize + x as usize) * 4;
                let (bb, gg, rr, aa) = if ring_covers(x, y, w, h, width_px, radius_px) {
                    (b, g, r, 255)
                } else {
                    (0, 0, 0, 0)
                };
                pixels[idx] = bb;
                pixels[idx + 1] = gg;
                pixels[idx + 2] = rr;
                pixels[idx + 3] = aa;
                mix(bb);
                mix(gg);
                mix(rr);
                mix(aa);
            }
        }
        hash
    }

    fn check_paint_ring_matches_oracle(
        w: i32,
        h: i32,
        width_px: i32,
        radius_px: i32,
        color: (u8, u8, u8),
    ) {
        // Dirty nonzero start: a missed clear shows up as a byte mismatch.
        let total = w.max(0) as usize * h.max(0) as usize * 4;
        let mut got = vec![0xA5u8; total + 16];
        let mut want = vec![0xA5u8; total + 16];
        let got_sum = paint_ring(&mut got, w, h, width_px, radius_px, color);
        let want_sum = paint_ring_oracle(&mut want, w, h, width_px, radius_px, color);
        assert_eq!(
            got_sum, want_sum,
            "checksum {w}x{h} bw{width_px} r{radius_px}"
        );
        assert_eq!(got, want, "bytes {w}x{h} bw{width_px} r{radius_px}");
    }

    #[test]
    fn paint_ring_matches_exhaustive_oracle_small_matrix() {
        // Broad small matrix: degenerate dims, border width 0 / negative /
        // thick (no inner hole), radius 0 / negative / oversized (clamped),
        // narrow surfaces, and color variations.
        let dims = [-1, 0, 1, 2, 3, 4, 7, 8, 11, 16];
        let borders = [-1, 0, 1, 2, 3, 5, 8, 20];
        let radii = [-2, 0, 1, 6, 40];
        let colors = [(0u8, 0u8, 0u8), (42, 130, 218), (255, 255, 255)];
        for w in dims {
            for h in dims {
                for width_px in borders {
                    for radius_px in radii {
                        for color in colors {
                            check_paint_ring_matches_oracle(w, h, width_px, radius_px, color);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn paint_ring_matches_oracle_at_typical_sizes() {
        // Sensible larger surfaces incl. defaults (width 3, radius 6).
        for (w, h, width_px, radius_px) in [
            (320, 200, 3, 6),
            (320, 200, 1, 0),
            (800, 600, 3, 6),
            (800, 600, 8, 12),
            (1920, 1080, 3, 6),
            (1920, 1080, 1, 0),
            (320, 200, 200, 6),
        ] {
            check_paint_ring_matches_oracle(w, h, width_px, radius_px, (42, 130, 218));
        }
    }

    /// Offline microbenchmark: old exhaustive vs new banded painter over
    /// identical dirty buffers. Ignored by default; run explicitly in dev
    /// and release with `-- --ignored --nocapture`. No Win32 calls, no
    /// asserted timing thresholds; prints wall time only.
    #[test]
    #[ignore]
    fn paint_ring_bench_old_vs_new() {
        use std::hint::black_box;
        use std::time::Instant;
        const REPS: u32 = 3;
        let color = (42u8, 130u8, 218u8);
        for (label, w, h, width_px, radius_px) in
            [("medium", 800, 600, 3, 6), ("large", 1920, 1080, 3, 6)]
        {
            let total = w as usize * h as usize * 4;
            let dirt = vec![0xA5u8; total];
            let mut new_elapsed = std::time::Duration::ZERO;
            let mut old_elapsed = std::time::Duration::ZERO;
            for _ in 0..REPS {
                let mut buf = dirt.clone();
                let start = Instant::now();
                let new_sum = paint_ring(
                    black_box(buf.as_mut_slice()),
                    w,
                    h,
                    width_px,
                    radius_px,
                    color,
                );
                new_elapsed += start.elapsed();
                black_box(new_sum);
                let mut expect = dirt.clone();
                let start = Instant::now();
                let old_sum = paint_ring_oracle(
                    black_box(expect.as_mut_slice()),
                    w,
                    h,
                    width_px,
                    radius_px,
                    color,
                );
                old_elapsed += start.elapsed();
                black_box(old_sum);
                assert_eq!(new_sum, old_sum, "bench checksum {label}");
                assert_eq!(buf, expect, "bench bytes {label}");
            }
            println!(
                "paint_ring {label} {w}x{h} bw{width_px} r{radius_px}: new {:?}/rep old {:?}/rep ({REPS} reps)",
                new_elapsed / REPS,
                old_elapsed / REPS,
            );
        }
    }
}
