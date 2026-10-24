#![forbid(unsafe_code)]
//! Independent pager active-window highlight style (Windows host styling).
//!
//! Two independent colours: the active-window ring keeps its own
//! `border.color` + `border.use-theme` setting, and the pager's focused
//! miniature keeps this one. Both default to the live system accent
//! (`use_theme = true` with the `#2a82da` fallback); an explicit override on
//! either side never touches the other. Resolution mirrors
//! [`crate::active_border::effective_color`] exactly (available accent plus
//! the theme switch, otherwise the configured `#rrggbb` fallback), but over
//! this struct, never over the border style: the miniature border and the
//! ring cannot diverge through a shared input, they only coincide when both
//! settings resolve the same way.

use crate::active_border::{
    DEFAULT_COLOR_RGB as BORDER_DEFAULT_RGB, SystemAccent, accent_available,
};

/// Fallback highlight colour (`#2a82da`, border parity), used when the
/// system accent is unavailable or theming is off.
pub const DEFAULT_COLOR_HEX: &str = "#2a82da";
pub const DEFAULT_COLOR_RGB: (u8, u8, u8) = BORDER_DEFAULT_RGB;
/// Highlight follows the live system accent by default; the configured
/// colour is the fallback.
pub const DEFAULT_USE_THEME: bool = true;

/// Configured pager highlight style: explicit `#rrggbb` plus the
/// system-accent switch. No geometry, no enable flag; the pager owns
/// visibility.
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
    accent_available(accent) && style.use_theme
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
                assert_eq!(uses, accent_available(accent) && style.use_theme);
                if uses {
                    assert_eq!(painted, accent.map(|a| a.color).expect("used accent"));
                } else {
                    assert_eq!(painted, style.color);
                }
            }
        }
    }
}
