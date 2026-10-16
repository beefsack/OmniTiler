#![forbid(unsafe_code)]

//! Portable macOS active-border policy and point geometry.
//!
//! Shared KDE/Windows semantics live in `tiler-core::visual`
//! (`active_border_state`, `active_border_inner_rect`,
//! `active_border_use_theme`, `active_border_is_maximized`); this module is
//! the thin macOS-local layer over them: defaults (width 3, gap 0, radius 6,
//! fallback `#2a82da`, theme on), the closed hide-reason vocabulary, the
//! carrier frame math in AppKit points, point-to-backing conversion, and the
//! render fence (target visibility, current-Space membership, unlocked
//! session). No AppKit, no window-server touch: the carrier (`ffi`) and the
//! fixture holder (`border_session`) consume these pure helpers.

use tiler_core::visual::{
    VisualRect, active_border_inner_rect, active_border_is_maximized, active_border_state,
    active_border_use_theme,
};

use crate::classifier::{AppFacts, AxFacts, CgFacts, Disposition, WindowFrame, classify_window};

/// Default border width in AppKit points (KDE `BorderWidth` parity).
pub const DEFAULT_WIDTH: f64 = 3.0;
/// Default gap between the target frame and the ring, in points.
pub const DEFAULT_GAP: f64 = 0.0;
/// Default inner corner radius in points (2026-10-11 decision parity).
pub const DEFAULT_RADIUS: f64 = 6.0;
/// Fallback border colour (`#2a82da`, KDE `BorderColor` parity).
pub const DEFAULT_COLOR_HEX: &str = "#2a82da";
/// Same fallback as raw channels.
pub const DEFAULT_COLOR_RGB: (u8, u8, u8) = (0x2a, 0x82, 0xda);
/// Theme follows the live system accent by default (KDE `UseThemeColor`
/// parity); the configured colour is the fallback.
pub const DEFAULT_USE_THEME: bool = true;

/// KCM spinbox maximums (width 32, radius 64, gap 64). Minimums are 0.
pub const MAX_WIDTH: f64 = 32.0;
/// KCM gap maximum.
pub const MAX_GAP: f64 = 64.0;
/// KCM radius maximum.
pub const MAX_RADIUS: f64 = 64.0;

/// Configured border style. All values are AppKit points; the carrier scales
/// them to backing pixels at the target screen's scale when drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderStyle {
    /// Ring thickness in points.
    pub width: f64,
    /// Gap between the target frame and the ring inner edge, in points.
    pub gap: f64,
    /// Inner corner radius in points.
    pub radius: f64,
    /// Configured fallback colour (used when theming is off or unavailable).
    pub color: (u8, u8, u8),
    /// Follow the live system accent when available.
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

/// Full border switch: default on, explicit off. Windows parity
/// (`tiler-windows/src/active_border.rs::ActiveBorderOptions`): the border
/// is independent of Engine tiling/floating membership and on unless
/// refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveBorderOptions {
    /// Master switch (CLI `--no-active-border`, settings `enabled`).
    pub enabled: bool,
    /// Ring geometry plus fallback colour/theme source.
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

/// Parse `#rrggbb` (case-insensitive, `#` required). Alpha forms are
/// refused: the KDE `BorderColor` default is opaque and the macOS accent
/// arrives as a separate theme source (its alpha feeds the theme gate, never
/// the ring).
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

/// Live system accent: theme colour in RGB plus its quantized alpha byte.
/// `None` (query unavailable or failed) always falls back to the configured
/// colour. Mirrors the KDE theme source (Plasma highlight gated on
/// availability): an accent analogue, never a guessed constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemAccent {
    /// Accent RGB channels.
    pub color: (u8, u8, u8),
    /// Quantized alpha byte the core theme gate (`theme_alpha > 0`) consumes.
    pub alpha: u8,
}

/// Effective border colour with the live system accent. The accent wins only
/// when requested (`use_theme`), present, and positively alpha-quantized per
/// the shared core gate; otherwise the configured fallback wins.
#[must_use]
pub fn effective_color(style: &ActiveBorderStyle, accent: Option<SystemAccent>) -> (u8, u8, u8) {
    if let Some(theme) = accent
        && active_border_use_theme(
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

/// One observed foreground target in AppKit points. Eligible floating AND
/// tiled windows map here identically: tiling membership never participates
/// (KDE/Windows parity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderTarget {
    /// A live target window exists.
    pub has_window: bool,
    /// Target frame in AppKit points (global coordinates).
    pub frame: VisualRect,
    /// Target is deleted/closed.
    pub deleted: bool,
    /// Target is minimized.
    pub minimized: bool,
    /// Target is in native fullscreen.
    pub fullscreen: bool,
    /// Target is zoomed/maximized horizontally (macOS zoom is one axis, like
    /// the Windows zoom mapping).
    pub maximized_h: bool,
    /// Target is zoomed/maximized vertically.
    pub maximized_v: bool,
    /// Target is excluded (utility/sheet transient, hidden-Space member, or
    /// otherwise out of scope).
    pub excluded: bool,
}

/// Closed hide-reason vocabulary. Reasons derive AFTER the core verdict, never
/// pre-returned ahead of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderHideReason {
    /// Border switch off.
    Disabled,
    /// No live target.
    NoTarget,
    /// Target minimized.
    Minimized,
    /// Target maximized/zoomed.
    Maximized,
    /// Target in native fullscreen.
    Fullscreen,
    /// Target deleted or excluded.
    Excluded,
}

impl BorderHideReason {
    /// Stable machine string for reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoTarget => "no-target",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Excluded => "excluded",
        }
    }
}

/// Border eligibility for one target, governed by
/// `tiler-core::visual::active_border_state`. The core verdict is
/// authoritative: reasons derive after the verdict.
pub fn border_decision(
    enabled: bool,
    target: &BorderTarget,
    gap: f64,
) -> Result<VisualRect, BorderHideReason> {
    if !enabled {
        return Err(BorderHideReason::Disabled);
    }
    if !target.has_window {
        return Err(BorderHideReason::NoTarget);
    }
    let maximized = active_border_is_maximized(target.maximized_h, target.maximized_v);
    let state = active_border_state(
        target.has_window,
        target.frame,
        target.deleted,
        target.minimized,
        target.fullscreen,
        maximized,
        target.excluded,
    );
    if state.visible {
        return Ok(active_border_inner_rect(target.frame, gap));
    }
    if target.minimized {
        return Err(BorderHideReason::Minimized);
    }
    if target.fullscreen {
        return Err(BorderHideReason::Fullscreen);
    }
    if maximized {
        return Err(BorderHideReason::Maximized);
    }
    if target.deleted || target.excluded {
        return Err(BorderHideReason::Excluded);
    }
    Err(BorderHideReason::NoTarget)
}

/// Carrier window frame in AppKit points: the inner rectangle expanded by
/// the ring `width` on every side, so the ring occupies `[inner, outer)`.
/// `None` on non-finite/negative inputs or non-positive/overflowing output.
#[must_use]
pub fn border_outer_rect(inner: VisualRect, width: f64) -> Option<VisualRect> {
    if !width.is_finite() || width < 0.0 {
        return None;
    }
    for v in [inner.x, inner.y, inner.w, inner.h] {
        if !v.is_finite() {
            return None;
        }
    }
    if inner.w <= 0.0 || inner.h <= 0.0 {
        return None;
    }
    let outer = VisualRect {
        x: inner.x - width,
        y: inner.y - width,
        w: inner.w + 2.0 * width,
        h: inner.h + 2.0 * width,
    };
    if !outer.w.is_finite() || !outer.h.is_finite() || outer.w <= 0.0 || outer.h <= 0.0 {
        return None;
    }
    // Keep coordinates inside a sane window-server range (rejects overflow
    // wraps without inventing a clamp).
    const BOUND: f64 = 1_000_000.0;
    if outer.x.abs() > BOUND || outer.y.abs() > BOUND || outer.w > BOUND || outer.h > BOUND {
        return None;
    }
    Some(outer)
}

/// Backing-pixel rectangle for a point frame at `scale` (points-to-pixels).
/// Plain rounding (KWin `round(thickness * scale)` parity); `None` on
/// non-positive/non-finite scale or overflow. Callers pass the target
/// screen's `backingScaleFactor`; global-point vs backing confusion fails
/// closed here instead of misplacing the ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackingRect {
    /// Backing x in pixels.
    pub x: i64,
    /// Backing y in pixels.
    pub y: i64,
    /// Backing width in pixels.
    pub w: i64,
    /// Backing height in pixels.
    pub h: i64,
}

/// Convert a point rect to backing pixels at `scale`.
#[must_use]
pub fn points_to_backing(frame: VisualRect, scale: f64) -> Option<BackingRect> {
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    for v in [frame.x, frame.y, frame.w, frame.h] {
        if !v.is_finite() {
            return None;
        }
    }
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return None;
    }
    let conv = |v: f64| {
        let scaled = v * scale;
        if !scaled.is_finite() {
            return None;
        }
        let rounded = scaled.round();
        if rounded < -9_000_000_000.0 || rounded > 9_000_000_000.0 {
            return None;
        }
        Some(rounded as i64)
    };
    Some(BackingRect {
        x: conv(frame.x)?,
        y: conv(frame.y)?,
        w: conv(frame.w)?,
        h: conv(frame.h)?,
    })
}

/// Render fence over plain data (no OS touch): the carrier may show only
/// when the target policy says visible AND the target is on the current
/// Space AND the session is unlocked. Fullscreen, Space departure, and lock
/// each hide independently; restoration re-evaluates all three.
#[must_use]
pub const fn render_fence(
    target_visible: bool,
    on_active_space: bool,
    session_unlocked: bool,
) -> bool {
    target_visible && on_active_space && session_unlocked
}

/// One joined observation for the active-border mapper, in AppKit points.
///
/// The AX/CG/app facts reuse the existing classifier vocabulary
/// ([`crate::classifier`]): no second role/subrole/layer/policy taxonomy
/// lives here. `frame` is the joined target frame in AppKit points (`None`
/// when no live target window exists); `focused` is the AX focused-window
/// join verdict for exactly this window (the ring follows the focused
/// window only); `maximized_h/v` is the macOS zoom-state readback (one axis
/// each, like the Windows zoom mapping); `deleted` marks a closed target.
#[derive(Debug, Clone)]
pub struct BorderObservation {
    /// Joined target frame in AppKit points (`None` = no live target).
    pub frame: Option<VisualRect>,
    /// Typed AX facts for the target (role/subrole/fullscreen/minimized).
    pub ax: AxFacts,
    /// Typed CG facts for the target (layer/onscreen).
    pub cg: CgFacts,
    /// Typed owner-app facts (hidden/policy/own-bundle).
    pub app: AppFacts,
    /// Target is zoomed/maximized horizontally.
    pub maximized_h: bool,
    /// Target is zoomed/maximized vertically.
    pub maximized_v: bool,
    /// AX focused-window join says this window is focused.
    pub focused: bool,
    /// Target is deleted/closed.
    pub deleted: bool,
}

/// Map one joined observation to a [`BorderTarget`]: `None` when no live
/// target exists or the window is not the focused one (the ring follows the
/// active window only). Classification reuses
/// [`crate::classifier::classify_window`]; only `Tile` and plain
/// `Fullscreen` shapes map to a non-excluded target — sheets, dialogs,
/// modals, popups (`AXUnknown`), utility/nonordinary-layer windows, and
/// every other excluded shape (accessory/prohibited policy, own bundle,
/// hidden app, minimized) set `excluded`, so the ring never draws on
/// transients. Fullscreen/minimized/maximized ride as their own flags and
/// the shared core verdict suppresses them. Tiling membership never
/// participates: eligible floating AND tiled windows map identically
/// (KDE/Windows parity). Space membership and session lock are NOT folded
/// here — the caller evaluates them through [`render_fence`].
#[must_use]
pub fn map_observation_to_target(obs: &BorderObservation) -> Option<BorderTarget> {
    let frame = obs.frame?;
    if !obs.focused {
        return None;
    }
    // Classifier geometry is an i32 join key only; the returned target keeps
    // the exact point frame (no rounding through integers).
    let classified = classify_window(
        WindowFrame {
            wid: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        },
        &obs.ax,
        &obs.cg,
        &obs.app,
    );
    let excluded = !matches!(
        classified.disposition,
        Disposition::Tile | Disposition::Fullscreen
    );
    Some(BorderTarget {
        has_window: true,
        frame,
        deleted: obs.deleted,
        minimized: obs.ax.minimized,
        fullscreen: obs.ax.fullscreen,
        maximized_h: obs.maximized_h,
        maximized_v: obs.maximized_v,
        excluded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> VisualRect {
        VisualRect { x, y, w, h }
    }

    fn live_target() -> BorderTarget {
        BorderTarget {
            has_window: true,
            frame: rect(10.0, 20.0, 320.0, 200.0),
            deleted: false,
            minimized: false,
            fullscreen: false,
            maximized_h: false,
            maximized_v: false,
            excluded: false,
        }
    }

    #[test]
    fn defaults_match_shared_parity() {
        let style = ActiveBorderStyle::default();
        assert_eq!(style.width, 3.0);
        assert_eq!(style.gap, 0.0);
        assert_eq!(style.radius, 6.0);
        assert_eq!(style.color, (0x2a, 0x82, 0xda));
        assert!(style.use_theme);
        assert_eq!(render_color(style.color), "#2a82da");
        assert_eq!(DEFAULT_COLOR_HEX, "#2a82da");
        // Full switch defaults on (Windows parity).
        let options = ActiveBorderOptions::default();
        assert!(options.enabled);
        assert_eq!(options.style, style);
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
        assert_eq!(parse_color("#2a82da"), Ok((0x2a, 0x82, 0xda)));
        assert_eq!(parse_color("#2A82DA"), Ok((0x2a, 0x82, 0xda)));
        assert!(parse_color("2a82da").is_err());
        assert!(parse_color("#fff").is_err());
        assert!(parse_color("#402a82da").is_err());
    }

    #[test]
    fn decision_agrees_with_core_verdict() {
        // The core `active_border_state` verdict is authoritative: for every
        // flag combination the local decision matches core visibility, and
        // the inner rect is the gap-expanded frame.
        let frame = rect(10.0, 20.0, 320.0, 200.0);
        for bits in 0..64u32 {
            let target = BorderTarget {
                has_window: bits & 1 != 0,
                frame,
                deleted: bits & 2 != 0,
                minimized: bits & 4 != 0,
                fullscreen: bits & 8 != 0,
                maximized_h: bits & 16 != 0,
                maximized_v: bits & 32 != 0,
                excluded: false,
            };
            let maximized = active_border_is_maximized(target.maximized_h, target.maximized_v);
            let core = active_border_state(
                target.has_window,
                frame,
                target.deleted,
                target.minimized,
                target.fullscreen,
                maximized,
                target.excluded,
            );
            let decision = border_decision(true, &target, 0.0);
            assert_eq!(decision.is_ok(), core.visible, "bits={bits:06b}");
            if core.visible {
                assert_eq!(decision.unwrap(), active_border_inner_rect(frame, 0.0));
            }
        }
        // Gap expands the inner rect on the visible path.
        assert_eq!(
            border_decision(true, &live_target(), 5.0).unwrap(),
            rect(5.0, 15.0, 330.0, 210.0)
        );
    }

    #[test]
    fn decision_suppression_matrix() {
        let frame_target = live_target();
        assert!(border_decision(true, &frame_target, 0.0).is_ok());
        assert_eq!(
            border_decision(false, &frame_target, 0.0),
            Err(BorderHideReason::Disabled)
        );
        assert_eq!(
            border_decision(
                true,
                &BorderTarget {
                    has_window: false,
                    ..frame_target
                },
                0.0
            ),
            Err(BorderHideReason::NoTarget)
        );
        for (target, want) in [
            (
                BorderTarget {
                    minimized: true,
                    ..frame_target
                },
                BorderHideReason::Minimized,
            ),
            (
                BorderTarget {
                    fullscreen: true,
                    ..frame_target
                },
                BorderHideReason::Fullscreen,
            ),
            (
                BorderTarget {
                    maximized_h: true,
                    ..frame_target
                },
                BorderHideReason::Maximized,
            ),
            (
                BorderTarget {
                    maximized_v: true,
                    ..frame_target
                },
                BorderHideReason::Maximized,
            ),
            (
                BorderTarget {
                    excluded: true,
                    ..frame_target
                },
                BorderHideReason::Excluded,
            ),
            (
                BorderTarget {
                    deleted: true,
                    ..frame_target
                },
                BorderHideReason::Excluded,
            ),
        ] {
            assert_eq!(border_decision(true, &target, 0.0), Err(want));
        }
    }

    #[test]
    fn outer_rect_expands_gap_then_width() {
        // Target 100x80 at (10,20), gap 2, width 4:
        // inner (8,18,104,84), outer (4,14,112,92).
        let target = BorderTarget {
            frame: rect(10.0, 20.0, 100.0, 80.0),
            ..live_target()
        };
        let inner = border_decision(true, &target, 2.0).unwrap();
        assert_eq!(inner, rect(8.0, 18.0, 104.0, 84.0));
        assert_eq!(
            border_outer_rect(inner, 4.0),
            Some(rect(4.0, 14.0, 112.0, 92.0))
        );
        // Zero width is the identity.
        assert_eq!(border_outer_rect(inner, 0.0), Some(inner));
        // Degenerate inputs fail closed, never clamped.
        assert!(border_outer_rect(inner, -1.0).is_none());
        assert!(border_outer_rect(inner, f64::NAN).is_none());
        assert!(border_outer_rect(rect(0.0, 0.0, 0.0, 10.0), 3.0).is_none());
        assert!(border_outer_rect(rect(f64::INFINITY, 0.0, 10.0, 10.0), 3.0).is_none());
    }

    #[test]
    fn backing_conversion_rounds_at_scale() {
        // 2x Retina: 3pt -> 6px; fractional points round.
        let frame = rect(10.0, 20.0, 100.0, 80.0);
        assert_eq!(
            points_to_backing(frame, 2.0),
            Some(BackingRect {
                x: 20,
                y: 40,
                w: 200,
                h: 160
            })
        );
        assert_eq!(points_to_backing(frame, 1.0).unwrap().w, 100);
        // 0.5pt at 1x rounds to 1px (0.5 rounds away from zero).
        assert_eq!(
            points_to_backing(rect(0.5, 0.5, 0.5, 0.5), 1.0).map(|r| (r.x, r.w)),
            Some((1, 1))
        );
        assert!(points_to_backing(frame, 0.0).is_none());
        assert!(points_to_backing(frame, -2.0).is_none());
        assert!(points_to_backing(frame, f64::NAN).is_none());
        assert!(points_to_backing(rect(0.0, 0.0, 0.0, 10.0), 2.0).is_none());
    }

    #[test]
    fn fence_needs_all_three_legs() {
        assert!(render_fence(true, true, true));
        assert!(!render_fence(false, true, true));
        assert!(!render_fence(true, false, true));
        assert!(!render_fence(true, true, false));
        assert!(!render_fence(false, false, false));
    }

    #[test]
    fn theme_gate_selects_accent_or_configured_fallback() {
        let themed = ActiveBorderStyle::default();
        let explicit = ActiveBorderStyle {
            use_theme: false,
            color: (0x11, 0x22, 0x33),
            ..ActiveBorderStyle::default()
        };
        let accent = SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        };
        assert_eq!(effective_color(&themed, Some(accent)), (0xaa, 0xbb, 0xcc));
        assert_eq!(
            effective_color(
                &themed,
                Some(SystemAccent {
                    color: (0xaa, 0xbb, 0xcc),
                    alpha: 0
                })
            ),
            DEFAULT_COLOR_RGB
        );
        assert_eq!(effective_color(&themed, None), DEFAULT_COLOR_RGB);
        assert_eq!(effective_color(&explicit, Some(accent)), (0x11, 0x22, 0x33));
    }

    #[test]
    fn hide_reasons_render_stable_strings() {
        assert_eq!(BorderHideReason::Disabled.as_str(), "disabled");
        assert_eq!(BorderHideReason::NoTarget.as_str(), "no-target");
        assert_eq!(BorderHideReason::Minimized.as_str(), "minimized");
        assert_eq!(BorderHideReason::Maximized.as_str(), "maximized");
        assert_eq!(BorderHideReason::Fullscreen.as_str(), "fullscreen");
        assert_eq!(BorderHideReason::Excluded.as_str(), "excluded");
    }

    fn observed(ax: AxFacts, cg: CgFacts, app: AppFacts) -> BorderObservation {
        BorderObservation {
            frame: Some(rect(10.0, 20.0, 320.0, 200.0)),
            ax,
            cg,
            app,
            maximized_h: false,
            maximized_v: false,
            focused: true,
            deleted: false,
        }
    }

    fn standard_facts() -> (AxFacts, CgFacts, AppFacts) {
        (AxFacts::standard(), CgFacts::normal(), AppFacts::regular())
    }

    #[test]
    fn mapper_needs_a_live_focused_window() {
        let (ax, cg, app) = standard_facts();
        // Focused standard tile maps to a visible target.
        let target =
            map_observation_to_target(&observed(ax.clone(), cg, app)).expect("focused tile maps");
        assert!(border_decision(true, &target, 0.0).is_ok());
        // Unfocused: no active target, never a ring.
        let mut obs = observed(ax.clone(), cg, app);
        obs.focused = false;
        assert!(map_observation_to_target(&obs).is_none());
        // Absent window: no target either.
        let mut obs = observed(ax, cg, app);
        obs.frame = None;
        assert!(map_observation_to_target(&obs).is_none());
    }

    #[test]
    fn mapper_suppresses_transients_and_policy_exclusions() {
        use crate::classifier::{
            ActivationPolicy, ROLE_SHEET, SUBROLE_DIALOG, SUBROLE_FLOATING, SUBROLE_UNKNOWN,
        };
        let (ax, cg, app) = standard_facts();
        // Popup (unknown subrole), sheet role, utility floating panel.
        let mut popup = ax.clone();
        popup.subrole = Some(SUBROLE_UNKNOWN.to_owned());
        let mut sheet = ax.clone();
        sheet.role = Some(ROLE_SHEET.to_owned());
        let mut utility = ax.clone();
        utility.subrole = Some(SUBROLE_FLOATING.to_owned());
        let mut utility_cg = cg;
        utility_cg.layer = 3;
        // Modal window, accessory-policy app, own bundle, minimized app-hide.
        let mut modal = ax.clone();
        modal.modal = true;
        let mut accessory = app;
        accessory.activation_policy = ActivationPolicy::Accessory;
        let mut own = AppFacts::regular();
        own.is_own_bundle = true;
        for (ax, cg, app) in [
            (popup, cg, AppFacts::regular()),
            (sheet, cg, AppFacts::regular()),
            (utility, utility_cg, AppFacts::regular()),
            (modal, cg, AppFacts::regular()),
            (ax.clone(), cg, accessory),
            (ax.clone(), cg, own),
        ] {
            let target = map_observation_to_target(&observed(ax, cg, app)).expect("maps");
            assert!(target.excluded, "transient/excluded shape must hide");
            assert_eq!(
                border_decision(true, &target, 0.0),
                Err(BorderHideReason::Excluded)
            );
        }
        // A dialog reporting fullscreen keeps the transient exclusion (never
        // re-typed to a plain fullscreen hold for border purposes).
        let mut dialog_fs = AxFacts::standard();
        dialog_fs.subrole = Some(SUBROLE_DIALOG.to_owned());
        dialog_fs.fullscreen = true;
        let target =
            map_observation_to_target(&observed(dialog_fs, cg, AppFacts::regular())).expect("maps");
        assert!(target.excluded && target.fullscreen);
        assert_eq!(
            border_decision(true, &target, 0.0),
            Err(BorderHideReason::Fullscreen)
        );
    }

    #[test]
    fn mapper_carries_suppression_flags_for_core_verdict() {
        let (ax, cg, app) = standard_facts();
        // Native fullscreen without a float origin: not excluded, but the
        // core verdict suppresses it.
        let mut fs = ax.clone();
        fs.fullscreen = true;
        let target = map_observation_to_target(&observed(fs, cg, app)).expect("maps");
        assert!(!target.excluded && target.fullscreen);
        assert_eq!(
            border_decision(true, &target, 0.0),
            Err(BorderHideReason::Fullscreen)
        );
        // Zoomed (either axis), minimized, deleted each suppress by reason.
        for (mutate, want) in [
            (
                Box::new(|o: &mut BorderObservation| o.maximized_h = true)
                    as Box<dyn Fn(&mut BorderObservation)>,
                BorderHideReason::Maximized,
            ),
            (
                Box::new(|o: &mut BorderObservation| o.maximized_v = true),
                BorderHideReason::Maximized,
            ),
            (
                Box::new(|o: &mut BorderObservation| o.ax.minimized = true),
                BorderHideReason::Minimized,
            ),
            (
                Box::new(|o: &mut BorderObservation| o.deleted = true),
                BorderHideReason::Excluded,
            ),
        ] {
            let mut obs = observed(ax.clone(), cg, app);
            mutate(&mut obs);
            let target = map_observation_to_target(&obs).expect("maps");
            assert_eq!(border_decision(true, &target, 0.0), Err(want));
        }
    }

    #[test]
    fn mapped_target_still_needs_space_and_session_fence() {
        // A visible mapped target shows only on the active Space with an
        // unlocked session; departure and lock each hide independently.
        let (ax, cg, app) = standard_facts();
        let target = map_observation_to_target(&observed(ax, cg, app)).expect("focused tile maps");
        let visible = border_decision(true, &target, 0.0).is_ok();
        assert!(visible);
        assert!(render_fence(visible, true, true));
        assert!(!render_fence(visible, false, true));
        assert!(!render_fence(visible, true, false));
    }
}
