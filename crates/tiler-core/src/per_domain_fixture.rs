//! Test-only per-domain observation row scaffolding.
//!
//! Shared window observations for typed per-domain layout operations.

use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;

/// Build [`EngineWindow`] observation rows homed to `domain_key` from
/// `(window, rect, hints, floating)` tuples; all other flags ride `false`.
#[must_use]
pub(crate) fn engine_windows_from_floating(
    domain_key: &DomainKey,
    windows: &[(WindowId, Rect, WindowSizeHints, bool)],
) -> Vec<EngineWindow> {
    windows
        .iter()
        .map(|(window, rect, hints, floating)| EngineWindow {
            window: window.clone(),
            output: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            rect: *rect,
            floating: *floating,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: *hints,
        })
        .collect()
}
