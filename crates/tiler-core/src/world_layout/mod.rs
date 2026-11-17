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
mod bootstrap;
mod capacity;
mod convergence;
mod focus;
mod observation;
mod orientation;
mod outcome;
mod projection;
mod queries;
mod relocation;
mod restore;
mod retained;
mod state;
mod topology;
mod tree_edit;

#[cfg(test)]
mod query_focus_tests;

pub(crate) use outcome::{
    AdmissionOutcome, LayoutFailure, OrientationOutcome, RetainedProjectionOutcome,
    RetainedReconcileFailure,
};
pub(crate) use projection::ProjectionFailure;
pub(crate) use state::WorldLayout;
