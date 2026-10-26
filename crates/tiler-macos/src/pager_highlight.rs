#![forbid(unsafe_code)]

//! Independent pager focused-window highlight style (macOS host styling).
//!
//! Two independent colours, never ring-coupled (see the generic rule in
//! `world-adapter-guide` section 8 and the macOS port notes): the
//! active-window ring keeps its own [`crate::active_border::ActiveBorderStyle`]
//! colour/theme tokens, and the pager's focused miniature keeps this one.
//! Both default to the live system accent with the `#2a82da` fallback; an
//! explicit override on either side never touches the other.
//!
//! Resolution mirrors [`crate::active_border::effective_color`] exactly
//! (available accent plus the theme switch, otherwise the configured
//! `#rrggbb` fallback), but over this struct, never over the border style:
//! the miniature border and the ring cannot diverge through a shared input,
//! they only coincide when both settings resolve the same way.
//!
//! The focused miniature paints a full-strength accent border over a dimmer
//! accent fill of the same RGB; unfocused miniatures stay plain. The dimmer
//! fill is the opaque-surface analogue of the group-underlay `0x66`-alpha
//! accent fill: each accent channel premultiplied at `0x66`, so the focused
//! fill reads accent-tinted but dimmer than the full-accent border beside
//! it. The pager active workspace cell keeps the system accent with no theme
//! switch (see [`crate::pager`]).

use tiler_core::visual::active_border_use_theme;

use crate::active_border::{DEFAULT_COLOR_RGB as BORDER_DEFAULT_RGB, SystemAccent};

/// Fallback highlight colour (`#2a82da`, border parity), used when the
/// system accent is unavailable or theming is off.
pub const DEFAULT_COLOR_HEX: &str = "#2a82da";
pub const DEFAULT_COLOR_RGB: (u8, u8, u8) = BORDER_DEFAULT_RGB;
/// Highlight follows the live system accent by default; the configured
/// colour is the fallback.
pub const DEFAULT_USE_THEME: bool = true;

/// Configured pager highlight style: explicit `#rrggbb` plus the
/// system-accent switch. No geometry, no enable flag; the pager owns
/// visibility. Startup-only: a running session resolves the live accent
/// per paint, never a frozen boot snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagerHighlightStyle {
    pub color: (u8, u8, u8),
    pub use_theme: bool,
}

impl Default for PagerHighlightStyle {
    fn default() -> Self {
        Self {
            color: DEFAULT_COLOR_RGB,
            use_theme: DEFAULT_USE_THEME,
        }
    }
}

/// Whether the highlight resolves from the live accent. Mirrors the border
/// condition exactly (availability plus the theme switch), so source
/// reporting can never diverge from the painted colour.
#[must_use]
pub fn effective_uses_accent(style: &PagerHighlightStyle, accent: Option<SystemAccent>) -> bool {
    accent
        .is_some_and(|theme| active_border_use_theme(style.use_theme, true, i32::from(theme.alpha)))
}

/// Effective highlight colour with the live system accent. The accent wins
/// only when requested (`use_theme`), present, and positively
/// alpha-quantized; otherwise the configured fallback wins.
#[must_use]
pub fn effective_color(style: &PagerHighlightStyle, accent: Option<SystemAccent>) -> (u8, u8, u8) {
    if effective_uses_accent(style, accent) {
        accent.map(|theme| theme.color).unwrap_or(style.color)
    } else {
        style.color
    }
}

/// Dimmer accent for the focused miniature fill: the opaque-surface analogue
/// of the group-underlay `0x66`-alpha accent fill. Each accent channel
/// premultiplied at `0x66`, so the focused fill reads accent-tinted but
/// dimmer than the full-strength border beside it.
#[must_use]
pub fn dim_accent((r, g, b): (u8, u8, u8)) -> (u8, u8, u8) {
    const ALPHA: u16 = 0x66;
    let dim = |c: u8| ((u16::from(c) * ALPHA + 127) / 255) as u8;
    (dim(r), dim(g), dim(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_system_accent_with_blue_fallback() {
        let style = PagerHighlightStyle::default();
        assert_eq!(style.color, (0x2a, 0x82, 0xda));
        assert!(style.use_theme);
        assert_eq!(DEFAULT_COLOR_HEX, "#2a82da");
    }

    #[test]
    fn theme_gate_selects_accent_or_configured_fallback() {
        let themed = PagerHighlightStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: true,
        };
        let explicit = PagerHighlightStyle {
            color: (0x11, 0x22, 0x33),
            use_theme: false,
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
            (0x11, 0x22, 0x33)
        );
        assert_eq!(effective_color(&themed, None), (0x11, 0x22, 0x33));
        assert_eq!(effective_color(&explicit, Some(accent)), (0x11, 0x22, 0x33));
    }

    #[test]
    fn effective_source_agrees_with_effective_color() {
        let styles = [
            PagerHighlightStyle {
                color: (0x11, 0x22, 0x33),
                use_theme: true,
            },
            PagerHighlightStyle {
                color: (0x11, 0x22, 0x33),
                use_theme: false,
            },
        ];
        let accents = [
            None,
            Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0xC4,
            }),
            Some(SystemAccent {
                color: (0xaa, 0xbb, 0xcc),
                alpha: 0,
            }),
        ];
        for style in styles {
            for accent in accents {
                let uses = effective_uses_accent(&style, accent);
                let painted = effective_color(&style, accent);
                let available = accent.is_some_and(|theme| {
                    active_border_use_theme(true, true, i32::from(theme.alpha))
                });
                assert_eq!(uses, available && style.use_theme);
                if uses {
                    assert_eq!(painted, accent.map(|a| a.color).expect("used accent"));
                } else {
                    assert_eq!(painted, style.color);
                }
            }
        }
    }

    #[test]
    fn dimmer_fill_shares_rgb_and_stays_dimmer() {
        // Full-strength border beside a dimmer fill of the same RGB: every
        // channel of the fill is the `0x66` premultiply of the border.
        assert_eq!(dim_accent((0x2a, 0x82, 0xda)), (0x11, 0x34, 0x57));
        assert_eq!(dim_accent((0xff, 0xff, 0xff)), (0x66, 0x66, 0x66));
        assert_eq!(dim_accent((0x00, 0x00, 0x00)), (0x00, 0x00, 0x00));
        let accent = (0xaa, 0xbb, 0xcc);
        let fill = dim_accent(accent);
        assert!(fill.0 <= accent.0 && fill.1 <= accent.1 && fill.2 <= accent.2);
        assert_ne!(fill, accent);
    }
}
