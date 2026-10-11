//! Portable Windows active-border policy, settings, and raster math.
//!
//! KDE parity source: `kwin/native-effect/activeborderconfig.kcfg`
//! (width 3.0, gap 0.0, `UseThemeColor` true, fallback `#2a82da`)
//! and `tiler-core::visual` (`active_border_state`, `active_border_inner_rect`,
//! `active_border_use_theme`). Outline radius defaults to 6.0 per the
//! 2026-10-11 decision. The border is independent of Engine
//! tiling/floating membership: eligibility never consults a tiled token.
//!
//! Theme colour mapping (confirmed in source): KDE selects the Plasma
//! highlight colour - `KColorScheme(QPalette::Active, KColorScheme::Selection,
//! kdeglobals).background()` gated on `isColorSetSupported(..., Selection)`
//! (`kwin/native-effect/activewindowborder.cpp:373-380`) - with the
//! configured `BorderColor` as fallback. The Windows analogue is the system
//! accent from `DwmGetColorizationColor` (0xAARRGGBB): theme wins only when
//! requested (`--active-border-theme` on), available, and positively
//! alpha-quantized per the shared `tiler-core::visual` gate.

use tiler_core::geometry::Rect;
use tiler_core::visual::{VisualRect, active_border_inner_rect, active_border_state};

/// Windows border defaults.
pub const DEFAULT_WIDTH: f64 = 3.0;
pub const DEFAULT_GAP: f64 = 0.0;
pub const DEFAULT_RADIUS: f64 = 6.0;
/// Fallback border colour (`#2a82da`, KDE `BorderColor` parity), used when
/// the system accent is unavailable or theming is off.
pub const DEFAULT_COLOR_HEX: &str = "#2a82da";
pub const DEFAULT_COLOR_RGB: (u8, u8, u8) = (0x2a, 0x82, 0xda);
/// Theme follows the live system accent by default (KDE `UseThemeColor`
/// parity); the configured colour is the fallback.
pub const DEFAULT_USE_THEME: bool = true;

/// KCM spinbox maximums (`unifiedsettings.ui`): width 32, radius 64, gap 64.
/// Minimums are 0 (a zero width paints nothing but stays valid).
pub const MAX_WIDTH: f64 = 32.0;
pub const MAX_GAP: f64 = 64.0;
pub const MAX_RADIUS: f64 = 64.0;

/// Configured border style. All values are logical KDE points; the native
/// layer scales them to physical pixels at the target window's DPI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderStyle {
    pub width: f64,
    pub gap: f64,
    pub radius: f64,
    pub color: (u8, u8, u8),
    pub use_theme: bool,
}

impl Default for ActiveBorderStyle {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            gap: DEFAULT_GAP,
            radius: DEFAULT_RADIUS,
            color: DEFAULT_COLOR_RGB,
            use_theme: DEFAULT_USE_THEME,
        }
    }
}

/// Full border switch: default on, explicit `--no-active-border` off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderOptions {
    pub enabled: bool,
    pub style: ActiveBorderStyle,
}

impl Default for ActiveBorderOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            style: ActiveBorderStyle::default(),
        }
    }
}

/// Parse a logical width in `0..=32` (KCM maximum).
pub fn parse_width(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-width needs 0..=32";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_WIDTH {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse a logical gap in `0..=64` (KCM maximum).
pub fn parse_gap(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-gap needs 0..=64";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_GAP {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse a logical radius in `0..=64` (KCM maximum).
pub fn parse_radius(value: &str) -> Result<f64, String> {
    let usage = "refuse: --active-border-radius needs 0..=64";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > MAX_RADIUS {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Parse `#rrggbb` (case-insensitive, `#` required). Alpha forms are refused:
/// the KDE `BorderColor` default is opaque and the Windows accent arrives as
/// a separate `SystemAccent` (its alpha feeds the theme gate, never the ring).
pub fn parse_color(value: &str) -> Result<(u8, u8, u8), String> {
    let usage = "refuse: --active-border-color needs #rrggbb";
    let hex = value.strip_prefix('#').ok_or_else(|| usage.to_owned())?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(usage.to_owned());
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| usage.to_owned())
    };
    Ok((channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// Render a colour back to `#rrggbb`.
#[must_use]
pub fn render_color((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Scale one logical KDE point value to physical pixels at `dpi`.
/// Matches KWin `itemrenderer_opengl.cpp` (`round(outline.thickness *
/// scale)`): plain rounding, so a positive logical value below half a
/// physical pixel rounds to 0 (paints nothing, valid). Zero stays zero.
/// `None` on non-positive DPI or overflow.
#[must_use]
pub fn scale_to_physical(logical: f64, dpi: u32) -> Option<i32> {
    if dpi == 0 || !logical.is_finite() || logical < 0.0 {
        return None;
    }
    if logical == 0.0 {
        return Some(0);
    }
    let scaled = logical * f64::from(dpi) / 96.0;
    if !scaled.is_finite() {
        return None;
    }
    if scaled <= 0.0 {
        return None;
    }
    let rounded = scaled.round() as i64;
    if !(0..=i64::from(i32::MAX)).contains(&rounded) {
        return None;
    }
    Some(rounded as i32)
}

/// Scale the full style to physical pixels. `None` when any channel fails.
#[must_use]
pub fn scale_style(style: &ActiveBorderStyle, dpi: u32) -> Option<(i32, i32, i32)> {
    Some((
        scale_to_physical(style.width, dpi)?,
        scale_to_physical(style.gap, dpi)?,
        scale_to_physical(style.radius, dpi)?,
    ))
}

/// Live Windows system accent: the `DwmGetColorizationColor` theme colour in
/// RGB plus its quantized alpha byte. `None` (query unavailable or failed)
/// always falls back to the configured colour. Mirrors the KDE theme source
/// (`KColorScheme` Selection background, `activewindowborder.cpp:373-380`):
/// an accent analogue, not a button-face or highlight-selection metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemAccent {
    pub color: (u8, u8, u8),
    pub alpha: u8,
}

impl SystemAccent {
    /// Split a `DwmGetColorizationColor` `0xAARRGGBB` value into RGB plus the
    /// quantized alpha byte the core theme gate (`theme_alpha > 0`) consumes.
    #[must_use]
    pub const fn from_colorization_dword(dword: u32) -> Self {
        Self {
            color: ((dword >> 16) as u8, (dword >> 8) as u8, dword as u8),
            alpha: (dword >> 24) as u8,
        }
    }
}

/// Effective border colour with the live system accent. Mirrors KDE
/// `activeBorderColor` (`activeborderlogic.h:23-32`): the accent wins only
/// when requested (`use_theme`, i.e. `--active-border-theme`), present,
/// and positively alpha-quantized per `active_border_use_theme`; otherwise
/// the configured fallback (`--active-border-color`, default `#2a82da`) wins.
#[must_use]
pub fn effective_color(style: &ActiveBorderStyle, accent: Option<SystemAccent>) -> (u8, u8, u8) {
    if let Some(theme) = accent
        && tiler_core::visual::active_border_use_theme(
            style.use_theme,
            /* theme_valid = */ true,
            i32::from(theme.alpha),
        )
    {
        theme.color
    } else {
        style.color
    }
}

fn visual_of(rect: Rect) -> VisualRect {
    VisualRect {
        x: rect.x as f64,
        y: rect.y as f64,
        w: rect.w as f64,
        h: rect.h as f64,
    }
}

/// Inner rectangle: the visible frame expanded by the physical gap.
/// Governed by `tiler-core::visual::active_border_inner_rect`.
#[must_use]
pub fn border_inner_rect(visible: Rect, gap_px: i32) -> Rect {
    let inner = active_border_inner_rect(visual_of(visible), f64::from(gap_px));
    Rect {
        x: inner.x.round() as i32,
        y: inner.y.round() as i32,
        w: inner.w.round() as i32,
        h: inner.h.round() as i32,
    }
}

/// Outer rectangle: the inner rectangle expanded by the physical border
/// width on every side. The KDE `OutlinedBorderItem` paints its `BorderOutline`
/// thickness outside `innerRect` (`updateBorder` sets the inner rect and the
/// outline separately), so the ring occupies `[inner, outer)`.
#[must_use]
pub fn border_outer_rect(visible: Rect, gap_px: i32, width_px: i32) -> Option<Rect> {
    let inner = border_inner_rect(visible, gap_px);
    let w = i64::from(width_px);
    let x = i64::from(inner.x).checked_sub(w)?;
    let y = i64::from(inner.y).checked_sub(w)?;
    let w_out = i64::from(inner.w).checked_add(2 * w)?;
    let h_out = i64::from(inner.h).checked_add(2 * w)?;
    if w_out <= 0 || h_out <= 0 || w_out > i64::from(i32::MAX) || h_out > i64::from(i32::MAX) {
        return None;
    }
    if x < i64::from(i32::MIN)
        || x > i64::from(i32::MAX)
        || y < i64::from(i32::MIN)
        || y > i64::from(i32::MAX)
    {
        return None;
    }
    Some(Rect {
        x: x as i32,
        y: y as i32,
        w: w_out as i32,
        h: h_out as i32,
    })
}

/// Closed hide-reason vocabulary for the border (no tiling skip reuse).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderHideReason {
    Disabled,
    NoTarget,
    Minimized,
    Maximized,
    Fullscreen,
    Cloaked,
    HiddenWorkspace,
    Shell,
}

impl BorderHideReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoTarget => "no-target",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Cloaked => "cloaked",
            Self::HiddenWorkspace => "hidden-workspace",
            Self::Shell => "shell",
        }
    }
}

/// Border eligibility for one foreground target, governed by
/// `tiler-core::visual::active_border_state`. Independent of Engine
/// tiling/floating membership: no tiled token participates. `maximized` is
/// the single Windows zoom axis (mapped to the horizontal maximize input).
/// `excluded` folds the Windows analogues of the KDE applet-popup exclusion:
/// cloaked surfaces, hidden-workspace members, and shell surfaces.
/// The core verdict is authoritative: reasons are derived after the verdict,
/// never pre-returned ahead of it.
#[allow(clippy::too_many_arguments)]
pub fn border_eligible(
    enabled: bool,
    has_window: bool,
    frame: Rect,
    minimized: bool,
    fullscreen: bool,
    maximized: bool,
    cloaked: bool,
    hidden_workspace: bool,
    shell: bool,
) -> Result<(), BorderHideReason> {
    if !enabled {
        return Err(BorderHideReason::Disabled);
    }
    if !has_window {
        return Err(BorderHideReason::NoTarget);
    }
    let maximized_core = tiler_core::visual::active_border_is_maximized(maximized, false);
    let state = active_border_state(
        has_window,
        visual_of(frame),
        /* deleted = */ false,
        minimized,
        fullscreen,
        maximized_core,
        cloaked || hidden_workspace || shell,
    );
    if state.visible {
        return Ok(());
    }
    if minimized {
        return Err(BorderHideReason::Minimized);
    }
    if fullscreen {
        return Err(BorderHideReason::Fullscreen);
    }
    if maximized {
        return Err(BorderHideReason::Maximized);
    }
    if cloaked {
        return Err(BorderHideReason::Cloaked);
    }
    if hidden_workspace {
        return Err(BorderHideReason::HiddenWorkspace);
    }
    if shell {
        return Err(BorderHideReason::Shell);
    }
    Err(BorderHideReason::NoTarget)
}

/// True when the point lies inside the rounded rectangle `(x, y, w, h)` with
/// corner radius `radius` (clamped to half the smallest side).
#[must_use]
pub fn point_in_rounded_rect(
    px: i32,
    py: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    if px < x || py < y || px >= x + w || py >= y + h {
        return false;
    }
    let r = radius.max(0).min(w / 2).min(h / 2);
    if r == 0 {
        return true;
    }
    // Edge bands (pixel centres): a pixel column/row inside the straight
    // section is covered regardless of the other axis.
    if px >= x + r && px < x + w - r {
        return true;
    }
    if py >= y + r && py < y + h - r {
        return true;
    }
    // Corner discs in doubled coordinates (pixel centres at odd coordinates,
    // corner centres at even ones) so the boundary is exact without floats.
    let cx2 = if px < x + r {
        2 * i64::from(x) + 2 * i64::from(r)
    } else {
        2 * i64::from(x) + 2 * i64::from(w) - 2 * i64::from(r)
    };
    let cy2 = if py < y + r {
        2 * i64::from(y) + 2 * i64::from(r)
    } else {
        2 * i64::from(y) + 2 * i64::from(h) - 2 * i64::from(r)
    };
    let dx = 2 * i64::from(px) + 1 - cx2;
    let dy = 2 * i64::from(py) + 1 - cy2;
    let rr = 2 * i64::from(r);
    dx * dx + dy * dy <= rr * rr
}

/// True when the overlay-local pixel `(px, py)` paints: inside the outer
/// rounded rect but outside the inner rounded rect. The overlay origin is the
/// outer rect origin. Matches KWin `outlinedborderitem.cpp` +
/// `itemrenderer_opengl.cpp`: `radius` is the INNER radius (the `Border`
/// shader `box` is the inner rect), the outer corner extent is
/// `radius + width`, and the inner hole at `(width, width)` keeps `radius`.
#[must_use]
pub fn ring_covers(
    px: i32,
    py: i32,
    outer_w: i32,
    outer_h: i32,
    width_px: i32,
    radius_px: i32,
) -> bool {
    if width_px <= 0 {
        return false;
    }
    // Square inner corners stay square (KWin emits no corner quads when
    // radius is 0); rounded inner corners extend the outer corner by the
    // ring width (KWin corner extent thickness + radius).
    let outer_r = if radius_px > 0 {
        radius_px.saturating_add(width_px)
    } else {
        0
    };
    if !point_in_rounded_rect(px, py, 0, 0, outer_w, outer_h, outer_r) {
        return false;
    }
    !point_in_rounded_rect(
        px,
        py,
        width_px,
        width_px,
        outer_w - 2 * width_px,
        outer_h - 2 * width_px,
        radius_px.max(0),
    )
}

/// Echo of parsed border flags for argv-consistency verification.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BorderArgvEcho {
    pub no_active_border: bool,
    pub width: Option<String>,
    pub gap: Option<String>,
    pub radius: Option<String>,
    pub color: Option<String>,
    pub no_theme: bool,
    pub theme: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn defaults_match_kde_parity() {
        let options = ActiveBorderOptions::default();
        assert!(options.enabled);
        assert_eq!(options.style.width, 3.0);
        assert_eq!(options.style.gap, 0.0);
        assert_eq!(options.style.radius, 6.0);
        assert_eq!(options.style.color, (0x2a, 0x82, 0xda));
        assert!(options.style.use_theme);
        assert_eq!(render_color(options.style.color), "#2a82da");
    }

    #[test]
    fn setting_ranges_follow_kcm_maximums() {
        assert!(parse_width("0").is_ok());
        assert!(parse_width("3").is_ok());
        assert!(parse_width("32").is_ok());
        assert!(parse_width("32.1").is_err());
        assert!(parse_width("-1").is_err());
        assert!(parse_width("nan").is_err());
        assert!(parse_gap("64").is_ok());
        assert!(parse_gap("64.1").is_err());
        assert!(parse_radius("64").is_ok());
        assert!(parse_radius("65").is_err());
    }

    #[test]
    fn color_requires_rrggbb() {
        assert_eq!(parse_color("#2a82da"), Ok((0x2a, 0x82, 0xda)));
        assert_eq!(parse_color("#2A82DA"), Ok((0x2a, 0x82, 0xda)));
        assert!(parse_color("2a82da").is_err());
        assert!(parse_color("#fff").is_err());
        assert!(parse_color("#402a82da").is_err());
        assert!(parse_color("#zzzzzz").is_err());
    }

    #[test]
    fn dpi_scaling_rounds_physical_pixels() {
        // 125% (120 DPI): 3.0 logical -> 4 physical (3.75 rounds to 4).
        assert_eq!(scale_to_physical(3.0, 120), Some(4));
        assert_eq!(scale_to_physical(3.0, 96), Some(3));
        assert_eq!(scale_to_physical(0.0, 120), Some(0));
        assert_eq!(scale_to_physical(3.0, 0), None);
        assert_eq!(scale_to_physical(-1.0, 96), None);
    }

    #[test]
    fn fractional_width_below_half_pixel_rounds_to_zero_kwin_parity() {
        // KWin `round(thickness * scale)`: sub-half-pixel widths round to 0
        // (valid, paints nothing) rather than clamping to 1.
        assert_eq!(scale_to_physical(0.4, 96), Some(0));
        assert_eq!(scale_to_physical(0.1, 96), Some(0));
        assert_eq!(scale_to_physical(0.4, 120), Some(1));
        assert_eq!(scale_to_physical(0.49, 96), Some(0));
        assert_eq!(scale_to_physical(0.5, 96), Some(1));
        assert_eq!(
            scale_style(
                &ActiveBorderStyle {
                    width: 0.4,
                    ..ActiveBorderStyle::default()
                },
                96
            )
            .map(|s| s.0),
            Some(0)
        );
    }

    #[test]
    fn eligibility_agrees_with_core_verdict() {
        // Core `active_border_state` is authoritative: for every flag
        // combination the portable verdict matches the core visibility, and
        // hidden reasons derive after the verdict.
        let frame = rect(10, 20, 320, 200);
        for bits in 0..64u32 {
            let minimized = bits & 1 != 0;
            let fullscreen = bits & 2 != 0;
            let maximized = bits & 4 != 0;
            let cloaked = bits & 8 != 0;
            let hidden = bits & 16 != 0;
            let shell = bits & 32 != 0;
            let maximized_core = tiler_core::visual::active_border_is_maximized(maximized, false);
            let core = tiler_core::visual::active_border_state(
                true,
                tiler_core::visual::VisualRect {
                    x: 10.0,
                    y: 20.0,
                    w: 320.0,
                    h: 200.0,
                },
                false,
                minimized,
                fullscreen,
                maximized_core,
                cloaked || hidden || shell,
            );
            let verdict = border_eligible(
                true, true, frame, minimized, fullscreen, maximized, cloaked, hidden, shell,
            );
            assert_eq!(verdict.is_ok(), core.visible, "bits={bits:06b}");
        }
    }

    #[test]
    fn ring_expands_gap_then_width() {
        // Visible 100x80 at (10,20), gap 2, width 4:
        // inner (8,18,104,84), outer (4,14,112,92).
        let visible = rect(10, 20, 100, 80);
        assert_eq!(border_inner_rect(visible, 2), rect(8, 18, 104, 84));
        assert_eq!(border_outer_rect(visible, 2, 4), Some(rect(4, 14, 112, 92)));
        // Zero gap/width is the identity.
        assert_eq!(border_outer_rect(visible, 0, 3), Some(rect(7, 17, 106, 86)));
    }

    #[test]
    fn eligibility_suppression_matrix() {
        let frame = rect(10, 20, 320, 200);
        assert!(
            border_eligible(true, true, frame, false, false, false, false, false, false).is_ok()
        );
        assert_eq!(
            border_eligible(false, true, frame, false, false, false, false, false, false),
            Err(BorderHideReason::Disabled)
        );
        assert_eq!(
            border_eligible(true, false, frame, false, false, false, false, false, false),
            Err(BorderHideReason::NoTarget)
        );
        for (min, fs, max, cloak, hidden, shell, want) in [
            (
                true,
                false,
                false,
                false,
                false,
                false,
                BorderHideReason::Minimized,
            ),
            (
                false,
                true,
                false,
                false,
                false,
                false,
                BorderHideReason::Fullscreen,
            ),
            (
                false,
                false,
                true,
                false,
                false,
                false,
                BorderHideReason::Maximized,
            ),
            (
                false,
                false,
                false,
                true,
                false,
                false,
                BorderHideReason::Cloaked,
            ),
            (
                false,
                false,
                false,
                false,
                true,
                false,
                BorderHideReason::HiddenWorkspace,
            ),
            (
                false,
                false,
                false,
                false,
                false,
                true,
                BorderHideReason::Shell,
            ),
        ] {
            assert_eq!(
                border_eligible(true, true, frame, min, fs, max, cloak, hidden, shell),
                Err(want)
            );
        }
    }

    #[test]
    fn accent_dword_splits_argb_channels() {
        // `DwmGetColorizationColor` format is 0xAARRGGBB.
        let accent = SystemAccent::from_colorization_dword(0xC4_2A82DA);
        assert_eq!(accent.color, (0x2a, 0x82, 0xda));
        assert_eq!(accent.alpha, 0xC4);
        assert_eq!(render_color(accent.color), "#2a82da");
        let opaque_red = SystemAccent::from_colorization_dword(0xFF_FF0000);
        assert_eq!(opaque_red.color, (0xff, 0x00, 0x00));
        assert_eq!(opaque_red.alpha, 0xff);
        let transparent = SystemAccent::from_colorization_dword(0x00_112233);
        assert_eq!(transparent.color, (0x11, 0x22, 0x33));
        assert_eq!(transparent.alpha, 0);
    }

    #[test]
    fn theme_gate_selects_accent_or_configured_fallback() {
        let themed = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: true,
            ..ActiveBorderStyle::default()
        };
        let explicit = ActiveBorderStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
            ..ActiveBorderStyle::default()
        };
        let accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        // Requested, present, positive alpha: accent wins (KDE highlight parity).
        assert_eq!(effective_color(&themed, Some(accent)), (0xaa, 0xbb, 0xcc));
        // Zero alpha fails the core gate: fallback wins.
        assert_eq!(
            effective_color(
                &themed,
                Some(SystemAccent {
                    color: (0xaa, 0xbb, 0xcc),
                    alpha: 0
                })
            ),
            (0x11, 0x22, 0x33)
        );
        // Unavailable query: fallback wins.
        assert_eq!(effective_color(&themed, None), (0x11, 0x22, 0x33));
        // `--no-active-border-theme` keeps the explicit configured colour even
        // when a valid accent is present.
        assert_eq!(effective_color(&explicit, Some(accent)), (0x11, 0x22, 0x33));
        assert_eq!(effective_color(&explicit, None), (0x11, 0x22, 0x33));
    }

    #[test]
    fn ring_raster_is_hollow() {
        // 12x12 outer, 3px ring, square corners (inner radius 0).
        assert!(ring_covers(0, 0, 12, 12, 3, 0));
        assert!(ring_covers(11, 11, 12, 12, 3, 0));
        assert!(!ring_covers(6, 6, 12, 12, 3, 0));
        assert!(!ring_covers(12, 6, 12, 12, 3, 0));
        // KWin inner-radius parity: radius param is the INNER radius.
        // Outer corner extent is radius + width, so with inner radius 3 and
        // width 3 the outer corner (extent 6) rounds the extreme corner
        // away while the top-edge centre still paints.
        assert!(!ring_covers(0, 0, 12, 12, 3, 3));
        assert!(ring_covers(6, 0, 12, 12, 3, 3));
        // Symmetric case from the old outer-radius reading would have
        // painted (0,0) here; inner-radius must not.
        assert!(!ring_covers(0, 0, 14, 14, 2, 5));
        assert!(ring_covers(7, 0, 14, 14, 2, 5));
        // Zero width never paints (width 0 hides the border).
        assert!(!ring_covers(0, 0, 12, 12, 0, 0));
    }
}
