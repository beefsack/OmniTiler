#![forbid(unsafe_code)]
//! Scoped send resolvers plus tails (item 12 stage B).
//!
//! Numbered, relative, and trailing FOLLOW/STAY sends honor the adopted
//! [`tiler_core::workspace_scope::WorkspaceMode`]; the local path delegates
//! straight back to [`crate::workspace::ManagedWorkspaces`] (same resolvers,
//! same order, same mutations), so local native behavior is unchanged.
//! Global-unique resolves positionally over the requesting output's scoped
//! ring; shared resolves over the single logical shared ring/history
//! ([`crate::workspace_scope_owner::WorkspaceScopeOwner::scoped_ring`]).
//! There is deliberately no global send to an id outside the requesting
//! output's numbered scope: global sends stay same-output.
//!
//! Transfer design:
//! - Same-output sends preserve the window's output-bound domain: membership
//!   moves from the source workspace to the target workspace on the
//!   requesting output only. In shared mode the requesting output converges
//!   the logical target entry first (`ensure_workspace_id`: empty entry,
//!   fresh token, no activation), so `assign` cannot refuse a live shared id
//!   homed only on another output. Per-output Engine layout domains are
//!   never relocated for sends; the tiled Engine route plans the explicit
//!   same-output domain pair exactly like the local path.
//! - Follow runs the common [`super::workspace_do_select`] dispatcher (thin
//!   call site, untouched here), so shared follow converges every live output
//!   onto the logical id and records one shared history entry, while global
//!   follow swaps visible-elsewhere views with history on both outputs. Only
//!   verified follow selects record; the recording itself stays inside the
//!   select tail (plus the scope-effects swap/shared tails).
//! - Stay never selects and never records: the mover hides, the source view
//!   stays selected with its existing focus, and ordinary trailing lifecycle
//!   supplies the next spare. In shared mode the trailing spare converges
//!   through the one logical ring (same scope contract as selects); the
//!   scoped cleanup unit owns convergence and shared creation.
//! - Shared trailing creation mints one logical id across all output domains
//!   (no refusal, no deferral): a send to an occupied shared trailing empty
//!   selects the just-minted id like the local append path.
//! - Floating/fullscreen/maximized retention, exact membership-transfer
//!   proof fences, and overlay gates are untouched: this module only resolves
//!   targets, converges the requesting entry, and scopes trailing/history.
//! - Whole-workspace migration refuses in shared mode (`mode-shared`, per the
//!   established core contract) with no mutation. In global-unique mode a
//!   completed migration validates the source scope's previous entry when it
//!   names the moved id (out-of-scope invalidation, mirroring the local
//!   removal rule); local mode needs nothing here.
//! - Cross-output directional sends (`run_output_send`) are out of scope and
//!   unchanged: they transfer through explicit `(output, workspace)` domain
//!   keys on a candidate workspace plus explicit Engine domains, never
//!   through the id-keyed `try_relocate_for_target` steal arm (that arm only
//!   runs in hotplug displacement/reconnect, owned by stage C, plus the
//!   already-claimed shared-select tail). No ambiguous relocation was found
//!   on the send paths. `sync_monitor_outputs`/hotplug behavior is
//!   preserved untouched.

use tiler_core::workspace_scope::{WorkspaceMode, resolve_relative_send};

/// Refusal outcome when whole-workspace migration runs under shared mode.
/// Matches the KDE `mode-shared` vocabulary from the established contract.
pub const MIGRATION_SHARED_REFUSAL: &str = "mode-shared";

/// Resolved scoped send target: a frozen workspace id, or an unknown target
/// carrying the exact legacy refusal outcome (`unknown-output` for trailing
/// with no ring, `unknown-target` otherwise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScopedSendTarget {
    /// Frozen transfer target on the requesting output.
    Id(String),
    /// No target: the caller refuses with the carried outcome.
    Unknown(&'static str),
}

/// Resolve a numbered/relative/trailing send target once before transfer.
///
/// Local mode delegates to the [`crate::workspace::ManagedWorkspaces`]
/// resolvers (byte-for-byte equivalent, including trailing creation);
/// global-unique and shared resolve positionally over the scoped ring.
/// Relative steps resolve from the mover's source workspace (frozen once,
/// wrapping through the trailing empty and ordinals beyond 9, never MRU,
/// never creating). Shared trailing creation mints one logical id across
/// all output domains when the tail is occupied.
pub(super) fn resolve_scoped_send_target(
    state: &mut super::TileLoop,
    output: &str,
    source_ws: &str,
    target: super::SendTarget,
) -> ScopedSendTarget {
    match target {
        super::SendTarget::Numbered(0) => {
            let sticky: std::collections::BTreeSet<crate::workspace::WindowKey> =
                state.sticky.keys().cloned().collect();
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            match owner.resolve_trailing_select(spaces, output, &sticky) {
                crate::workspace_scope_owner::TrailingResolve::Reuse(id) => {
                    ScopedSendTarget::Id(id)
                }
                crate::workspace_scope_owner::TrailingResolve::Unknown => {
                    ScopedSendTarget::Unknown("unknown-output")
                }
            }
        }
        super::SendTarget::Numbered(index) => {
            match state
                .scope_owner
                .resolve_numbered(&state.workspaces, output, index)
            {
                Some(id) => ScopedSendTarget::Id(id),
                None => ScopedSendTarget::Unknown("unknown-target"),
            }
        }
        super::SendTarget::Relative(delta) => {
            if state.scope_owner.adopted() == WorkspaceMode::PerOutputLocal {
                match state
                    .workspaces
                    .resolve_relative_from(output, source_ws, delta)
                {
                    Some(id) => ScopedSendTarget::Id(id),
                    None => ScopedSendTarget::Unknown("unknown-target"),
                }
            } else {
                let ring = state.scope_owner.scoped_ring(&state.workspaces, output);
                match resolve_relative_send(&ring, source_ws, delta) {
                    Some(id) => ScopedSendTarget::Id(id),
                    None => ScopedSendTarget::Unknown("unknown-target"),
                }
            }
        }
    }
}

/// Converge the requesting output's entry for a shared send target before
/// assignment: outputs lacking the logical id gain an empty entry (fresh
/// token, no activation) so the transfer preserves the window's output-bound
/// domain. No-op for local/global-unique modes and for already-present ids.
pub(super) fn ensure_shared_send_entry(state: &mut super::TileLoop, output: &str, target_id: &str) {
    if state.scope_owner.adopted() != WorkspaceMode::Shared {
        return;
    }
    state.workspaces.ensure_workspace_id(output, target_id);
}

/// Scoped trailing spare after a stay send: local runs the ordinary
/// per-output lifecycle for the next spare; global-unique and shared
/// converge through the shared scope contract (per-output scoped plan, one
/// logical ring). Never records history in any mode.
pub(super) fn trailing_spare_scoped(state: &mut super::TileLoop, output: &str) {
    super::scope_trailing::converge_spare(state, output);
}

/// Whole-workspace migration scope guard: shared mode refuses with
/// [`MIGRATION_SHARED_REFUSAL`] before any mutation. Local/global-unique
/// migrate (capability is unchanged); `None` means proceed.
#[must_use]
pub(super) fn migration_scope_refusal(state: &super::TileLoop) -> Option<&'static str> {
    if state.scope_owner.adopted() == WorkspaceMode::Shared {
        return Some(MIGRATION_SHARED_REFUSAL);
    }
    None
}

/// Validate the source scope's previous entry after a completed migration.
///
/// Global-unique only, and only when the recorded previous names the moved
/// id (now out of the source scope): the stale entry clears like removal,
/// mirroring the local invalidation rule. Any other entry is untouched, and
/// no new history records here. Local mode needs nothing (the workspace
/// primitive already invalidates); shared never reaches here (refused).
pub(super) fn note_migration_history(
    state: &mut super::TileLoop,
    source_output: &str,
    moved_id: &str,
) {
    if state.scope_owner.adopted() != WorkspaceMode::GlobalUnique {
        return;
    }
    let stale = state
        .scope_owner
        .history()
        .previous(source_output)
        .as_deref()
        == Some(moved_id);
    if !stale {
        return;
    }
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    let _ = owner.resolve_previous(spaces, source_output);
}
