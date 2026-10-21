#![forbid(unsafe_code)]
//! Scoped send resolvers plus tails (world-owned, adapter projection).
//!
//! Send resolution is core planning now (`SendNumbered` / `SendRelative`
//! commands own the scoped ring, history, and trailing): production routes
//! in [`super::sends`] dispatch the operator intent directly and never call
//! the resolvers below. The helpers that remain are read-only projections
//! over the core's logical rings for log/test compatibility, plus the
//! shared-mode migration guard from the saved workspace mode. No local
//! rings, no mutations, no trailing creation.

use tiler_core::workspace_scope::resolve_relative_send;

/// Refusal outcome when whole-workspace migration runs under shared mode.
pub const MIGRATION_SHARED_REFUSAL: &str = "mode-shared";

/// Resolved scoped send target: a frozen workspace id, or an unknown target
/// carrying the legacy refusal outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScopedSendTarget {
    /// Frozen transfer target on the requesting output.
    Id(String),
    /// No target: the caller refuses with the carried outcome.
    Unknown(&'static str),
}

/// Read-only positional resolve over the core's logical ring for one output
/// (device key via the mechanical `output_tokens` map only). Numbered
/// digits address the ring positionally; `0` reuses the live trailing tail
/// (creation itself is core lifecycle on command dispatch). Relative steps
/// resolve from the mover source through the core helper, wrapping through
/// the trailing empty. Never mutates, never creates.
pub(super) fn resolve_scoped_send_target(
    state: &super::TileLoop,
    output: &str,
    source_ws: &str,
    target: super::SendTarget,
) -> ScopedSendTarget {
    let Some(wire) = state.output_tokens.get(output) else {
        return ScopedSendTarget::Unknown("unknown-output");
    };
    let ring = state.world.engine().logical_ring(wire);
    if ring.is_empty() {
        return ScopedSendTarget::Unknown("unknown-output");
    }
    match target {
        super::SendTarget::Numbered(0) => match ring.last() {
            Some(id) => ScopedSendTarget::Id(id.clone()),
            None => ScopedSendTarget::Unknown("unknown-output"),
        },
        super::SendTarget::Numbered(index) => {
            let position = usize::from(index.saturating_sub(1));
            match ring.get(position) {
                Some(id) => ScopedSendTarget::Id(id.clone()),
                None => ScopedSendTarget::Unknown("unknown-target"),
            }
        }
        super::SendTarget::Relative(delta) => {
            match resolve_relative_send(&ring, source_ws, delta) {
                Some(id) => ScopedSendTarget::Id(id),
                None => ScopedSendTarget::Unknown("unknown-target"),
            }
        }
    }
}

/// Whole-workspace migration scope guard from the saved workspace mode:
/// shared mode refuses with [`MIGRATION_SHARED_REFUSAL`] before any
/// mutation. The core repeats the refusal on dispatch; this pre-check only
/// saves the round trip.
#[must_use]
pub(super) fn migration_scope_refusal(state: &super::TileLoop) -> Option<&'static str> {
    if crate::settings::startup_mode(state.settings_live.as_ref())
        == tiler_core::workspace_scope::WorkspaceMode::Shared
    {
        return Some(MIGRATION_SHARED_REFUSAL);
    }
    None
}
