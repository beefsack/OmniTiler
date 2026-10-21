//! Test-only per-domain [`CoreEvent`](crate::boundary::CoreEvent) scaffolding.
//!
//! The relocated per-domain regression suites below construct their
//! [`CoreEvent`](crate::boundary::CoreEvent) observations through these
//! helpers. They mirror the retired adapter-side reconcile builders exactly
//! (complete observation rows plus focus and gap carriage, `Reconcile`
//! command); production adapters build world reports instead.

use crate::boundary::{CoreCommand, CoreEvent};
use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::size_hints::WindowSizeHints;

/// Deterministic FNV-1a 64-bit fingerprint over sorted `(token, rect)` pairs.
/// Same algorithm the retired adapter builder used to fill the carried
/// fingerprint; kept stable so relocated replay tests keep their exact
/// convergence behavior.
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

/// Build a `Reconcile` event for one explicit `(output, workspace)` domain.
/// `(window, rect, hints, floating)` rows become complete observation
/// carriers homed to `domain_key`; every other flag rides `false`, exactly
/// like the retired builder. Hidden retained membership is preserved by
/// inclusion: callers pass the full carried row set.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn reconcile_event_for_floating(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fp: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, WindowSizeHints, bool)],
    focused: Option<&WindowId>,
) -> CoreEvent {
    CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation: correlation.clone(),
        revision,
        fingerprint: fp,
        domain: domain.clone(),
        domain_key: domain_key.clone(),
        outer_gap,
        focused_window: focused.cloned().unwrap_or(WindowId(String::new())),
        windows: windows
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
            .collect(),
        directional: None,
        directional_target_outer_gap: None,
        target_domain: None,
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::Reconcile,
    }
}

/// Tiled-only variant of [`reconcile_event_for_floating`]: every row rides
/// `floating=false`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn reconcile_event_for(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fp: u64,
    domain: &OutputDomain,
    domain_key: &DomainKey,
    outer_gap: i32,
    windows: &[(WindowId, Rect, WindowSizeHints)],
    focused: Option<&WindowId>,
) -> CoreEvent {
    let floating: Vec<(WindowId, Rect, WindowSizeHints, bool)> = windows
        .iter()
        .map(|(window, rect, hints)| (window.clone(), *rect, *hints, false))
        .collect();
    reconcile_event_for_floating(
        owner,
        generation,
        correlation,
        revision,
        fp,
        domain,
        domain_key,
        outer_gap,
        &floating,
        focused,
    )
}
