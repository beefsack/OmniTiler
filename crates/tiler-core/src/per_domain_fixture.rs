//! Test-only per-domain observation row scaffolding.
//!
//! Previously built [`CoreEvent`](crate::boundary::CoreEvent) observations for
//! the retired per-domain `Engine` dispatcher. In C1, `CoreEvent` and `Engine`
//! are deleted in favor of typed synchronous [`WorldLayout`](crate::world_layout::WorldLayout)
//! operations. Remaining unmigrated callers (such as `per_domain_resize_tests`)
//! should migrate from `CoreEvent` to `WorldLayout` + `LocalOperationContext`
//! using [`engine_windows_from_floating`] / [`engine_windows_from`].

use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;

/// Deterministic FNV-1a 64-bit fingerprint over sorted `(token, rect)` pairs.
#[must_use]
pub(crate) fn fingerprint(windows: &[(String, Rect)]) -> u64 {
    let mut sorted: Vec<(&String, &Rect)> = windows.iter().map(|(t, r)| (t, r)).collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut hash: u64 = 0xcbf29ce484222325;
    for (token, rect) in sorted {
        for byte in token.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for word in [rect.x, rect.y, rect.w, rect.h] {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

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

/// Tiled-only variant of [`engine_windows_from_floating`]: every row rides
/// `floating=false`.
#[must_use]
pub(crate) fn engine_windows_from(
    domain_key: &DomainKey,
    windows: &[(WindowId, Rect, WindowSizeHints)],
) -> Vec<EngineWindow> {
    let floating: Vec<(WindowId, Rect, WindowSizeHints, bool)> = windows
        .iter()
        .map(|(window, rect, hints)| (window.clone(), *rect, *hints, false))
        .collect();
    engine_windows_from_floating(domain_key, &floating)
}
