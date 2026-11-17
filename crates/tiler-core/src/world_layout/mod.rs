//! Owned layout storage for the sole world engine.
//!
//! [`WorldLayout`] is the single retained home for per-domain layout data,
//! the one layout policy, the generation-shared group allocator, the single
//! admission high-water and the layout configuration. It deliberately owns
//! no owner, generation, fingerprint, reconciler, pending-operation or
//! native-settlement state: those belong to the world transition or are
//! obsolete. Members remain the sole homing/lifetime authority in
//! `WorldEngine`; a layout binding stores only a leaf association, and
//! public vocabulary snapshots derive their home from the owning map key.

mod admission;
mod admit;
mod adoption;
mod bootstrap;
mod capacity;
mod convergence;
mod convergence_phases;
mod fit;
mod float;
mod focus;
mod gesture;
mod gesture_resolve;
mod group_alloc;
mod local_focus;
mod local_move;
mod local_operation;
mod observation;
mod orientation;
mod outcome;
mod pair;
mod pair_convergence;
mod pair_focus;
mod pair_move;
mod pair_operation;
mod pair_send;
mod pair_transfer;
mod projection;
mod queries;
mod relocation;
mod resize;
mod restore;
mod retained;
mod state;
mod topology;
mod tree_edit;
mod unchanged;
mod workspace_migration;
mod workspace_swap;

#[cfg(test)]
mod cross_output_tests;
#[cfg(test)]
mod deferred_send_tests;
#[cfg(test)]
mod fixed_send_tests;
#[cfg(test)]
mod float_focus_tests;
#[cfg(test)]
mod float_tests;
#[cfg(test)]
mod gesture_tests;
#[cfg(test)]
mod keyboard_resize_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod local_directional_tests;
#[cfg(test)]
mod local_operation_tests;
#[cfg(test)]
mod movement_tests;
#[cfg(test)]
mod orientation_tests;
#[cfg(test)]
mod overlay_send_tests;
#[cfg(test)]
mod pair_convergence_tests;
#[cfg(test)]
mod pair_directional_tests;
#[cfg(test)]
mod pair_send_tests;
#[cfg(test)]
mod pair_tests;
#[cfg(test)]
mod query_focus_tests;
#[cfg(test)]
mod resize_tests;
#[cfg(test)]
mod unchanged_tests;
#[cfg(test)]
mod workspace_migration_tests;
#[cfg(test)]
mod workspace_send_tests;
#[cfg(test)]
mod workspace_swap_tests;

pub(crate) use float::centered_float_rect;
pub(crate) use group_alloc::GroupAllocator;
pub(crate) use local_operation::LocalOperationContext;
pub(crate) use outcome::{
    AdmissionOutcome, DragDropOutcome, DragPreviewOutcome, FittedAdmissionDecline,
    FittedAdmissionReason, FloatOutcome, FreshDomainFailure, FreshDomainOutcome, FreshOutcomeKind,
    LayoutFailure, LocalFocusOutcome, LocalMoveOutcome, LocalOperationFailure, OrientationOutcome,
    PairFocusOutcome, PairMoveCross, PairMoveOutcome, PairOperationFailure, PairSendKind,
    PairSendOutcome, PointerCornerResizeOutcome, ResizeOutcome, RetainedProjectionOutcome,
    RetainedReconcileFailure, WorkspaceMigrationFailure, WorkspaceMigrationOutcome,
    WorkspaceSwapFailure, WorkspaceSwapOutcome,
};
pub(crate) use pair::PairAssemblyFailure;
pub(crate) use pair_operation::PairOperationContext;
pub(crate) use projection::ProjectionFailure;
pub(crate) use queries::rank_cross_output_edge_candidates;
pub(crate) use state::WorldLayout;
pub(crate) use workspace_swap::WorkspaceSwapSide;
