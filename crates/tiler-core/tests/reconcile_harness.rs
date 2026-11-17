#![forbid(unsafe_code)]
//! Retired C1 contract test file (`Reconciler` handshake harness).
//!
//! All 16 tests in this file previously drove `tiler_core::reconcile::Reconciler`,
//! `AdapterAck`, `Observation`, and `PostObservation` (propose/acknowledge/verify
//! state machine, `DivergenceKind`, `StateKind`). In C1, `Reconciler` and the
//! synthetic ack/post-observation handshake were physically deleted in favor of
//! synchronous `WorldLayout` operations owned by `WorldEngine`.
//!
//! Exact count rationale: 16 -> 0 tests here (15 `Reconciler` handshake/divergence
//! tests retired; the 3 typed-ID parse checks in `malformed_ack_and_verify_shapes_classify_typed`
//! are covered by `reconcile_invariants::session_ids_are_opaque_bounded_and_redacted`).
