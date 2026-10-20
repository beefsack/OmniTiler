//! Minimal platform-extension entry points.
//!
//! The portable core covers geometry, hints, modes, decorations and
//! description. Host-specific creation options live here as small,
//! explicitly gated helpers. The pattern for a future Windows or macOS action
//! is: add one `action_*` function plus one capability flag, report
//! unsupported everywhere else, no plugin framework.
//!
//! Shipped example: creation-only X11 `_NET_WM_WINDOW_TYPE`
//! normal/dialog/utility. It exercises tiler classification and automatic
//! float paths without scripts or access to foreign windows. Runtime changes
//! to this creation-only value are reported as unsupported, never applied by
//! recreating the window.
//!
//! The same creation-only pattern covers X11 `override_redirect`: unmanaged
//! windows for negative-path tests. Runtime changes are reported as
//! unsupported everywhere.

use crate::protocol::X11WindowType;

/// Extension action names accepted by [`action_supported`].
pub const EXTENSION_ACTIONS: &[&str] = &["x11_window_type", "x11_override_redirect"];

/// Actions supported on a live backend, derived from [`EXTENSION_ACTIONS`].
/// Future Windows/macOS actions extend the catalog, not the call pattern.
pub fn supported_actions(backend: &str) -> Vec<&'static str> {
    EXTENSION_ACTIONS
        .iter()
        .copied()
        .filter(|action| action_supported(action, backend))
        .collect()
}

/// Whether a named extension action applies to a live backend. Unknown
/// actions are unsupported by definition, which keeps future Windows/macOS
/// additions additive.
pub fn action_supported(action: &str, backend: &str) -> bool {
    match action {
        "x11_window_type" => backend == "x11",
        "x11_override_redirect" => backend == "x11",
        _ => false,
    }
}

/// Map the portable creation type to the winit X11 window type.
///
/// Only `Normal`, `Dialog` and `Utility` are exposed: they are the useful
/// classification/automatic-float witnesses. Everything else stays out of
/// scope for this dev helper.
#[cfg(target_os = "linux")]
pub fn winit_x11_types(kind: X11WindowType) -> Vec<winit::platform::x11::WindowType> {
    use winit::platform::x11::WindowType;
    vec![match kind {
        X11WindowType::Normal => WindowType::Normal,
        X11WindowType::Dialog => WindowType::Dialog,
        X11WindowType::Utility => WindowType::Utility,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_type_is_gated_to_x11() {
        assert!(action_supported("x11_window_type", "x11"));
        assert!(!action_supported("x11_window_type", "wayland"));
        assert!(!action_supported("x11_window_type", "windows"));
        assert!(action_supported("x11_override_redirect", "x11"));
        assert!(!action_supported("x11_override_redirect", "wayland"));
        assert!(!action_supported("future_action", "x11"));
    }

    #[test]
    fn catalog_matches_gating() {
        assert_eq!(
            supported_actions("x11"),
            vec!["x11_window_type", "x11_override_redirect"]
        );
        assert!(supported_actions("wayland").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mapping_covers_all_creation_types() {
        use winit::platform::x11::WindowType;
        assert_eq!(
            winit_x11_types(X11WindowType::Normal),
            vec![WindowType::Normal]
        );
        assert_eq!(
            winit_x11_types(X11WindowType::Dialog),
            vec![WindowType::Dialog]
        );
        assert_eq!(
            winit_x11_types(X11WindowType::Utility),
            vec![WindowType::Utility]
        );
    }
}
