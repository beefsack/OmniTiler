#![forbid(unsafe_code)]
//! Scoped send guard (world-owned, adapter projection).
//!
//! Send resolution is core planning now (`SendNumbered` / `SendRelative`
//! commands own the scoped ring, history, and trailing): production routes
//! in [`super::sends`] dispatch the operator intent directly. The helper
//! that remains is the shared-mode migration guard from the saved workspace
//! mode. No local rings, no mutations, no trailing creation.

/// Refusal outcome when whole-workspace migration runs under shared mode.
pub const MIGRATION_SHARED_REFUSAL: &str = "mode-shared";

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
