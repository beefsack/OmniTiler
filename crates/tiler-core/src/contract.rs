//! Production transport-independent contract vocabulary.
//!
//! Portable boundary: bounded opaque ids, revision/precondition bounds, and
//! typed divergence/resize/drag semantic enums. No platform, process, IPC,
//! geometry, or native execution imports; only [`crate::directional`] semantic
//! types.

use crate::directional::{Axis, Direction, NodeId, OutputId, WindowId, WorkspaceId};

/// Adapter envelope version.
pub const CONTRACT_VERSION: u32 = 1;
/// Sealed planning policy version carried by plans.
pub const POLICY_VERSION: u32 = 1;
/// Portable layout lifecycle policy version (`cosmic_v1`).
pub const LIFECYCLE_POLICY_VERSION: u32 = 1;
/// Opaque correlation token bound (shared with [`crate::ids`]).
pub use crate::ids::MAX_CORRELATION_LEN;
/// Opaque generation bound (shared with [`crate::ids`]).
pub use crate::ids::MAX_GENERATION_LEN;
/// Opaque owner token bound (shared with [`crate::ids`]).
pub use crate::ids::MAX_OWNER_LEN;
pub use crate::ids::{is_correlation_id, is_generation_id, is_owner_id};
/// Revision bound (inclusive), shared by planner routes.
pub const MAX_REVISION: u64 = 1_000_000;
/// Precondition vector cap (directional plans carry at most a handful).
pub const MAX_PRECONDITIONS: usize = 8;

/// Revision validity.
#[must_use]
pub const fn is_revision(value: u64) -> bool {
    value <= MAX_REVISION
}

/// Typed fail-closed divergence reasons. Fixed redacted messages only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceKind {
    StaleRevision,
    OwnerMismatch,
    GenerationMismatch,
    CorrelationMismatch,
    CapabilityRefused,
    PartialApplication,
    AdapterLost,
    PostconditionUnverified,
    PostconditionMismatch,
    RevisionExhausted,
}

impl DivergenceKind {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaleRevision => "stale-revision",
            Self::OwnerMismatch => "owner-mismatch",
            Self::GenerationMismatch => "generation-mismatch",
            Self::CorrelationMismatch => "correlation-mismatch",
            Self::CapabilityRefused => "capability-refused",
            Self::PartialApplication => "partial-application",
            Self::AdapterLost => "adapter-lost",
            Self::PostconditionUnverified => "postcondition-unverified",
            Self::PostconditionMismatch => "postcondition-mismatch",
            Self::RevisionExhausted => "revision-exhausted",
        }
    }

    /// Fixed redacted message; never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::StaleRevision => "observation revision does not match verified state",
            Self::OwnerMismatch => "owner token does not pin verified state",
            Self::GenerationMismatch => "generation does not match verified state",
            Self::CorrelationMismatch => "correlation does not match the pending plan",
            Self::CapabilityRefused => "adapter cannot realize the required capability",
            Self::PartialApplication => "adapter reported partial application",
            Self::AdapterLost => "adapter contact was lost",
            Self::PostconditionUnverified => "adapter did not verify postconditions",
            Self::PostconditionMismatch => "plan postconditions do not bind the pending plan",
            Self::RevisionExhausted => "revision bound reached",
        }
    }
}

/// Semantic resize direction mode from source COSMIC `ResizeDirection`
/// (`Inwards`/`Outwards`). Combined with the cardinal edge (`Direction` as
/// edge: edge axis selects the nearest matching-axis ancestor, edge determines
/// the neighbor side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeMode {
    Inwards,
    Outwards,
}

impl ResizeMode {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inwards => "inwards",
            Self::Outwards => "outwards",
        }
    }
}

/// Structural resize operation with fully resolved portable identities.
///
/// Names the exact logical domain, the focused leaf/window, the intentional
/// edge (`direction` as cardinal edge) plus source mode, the stable target
/// split (`target_group`) plus the selected adjacent pair
/// (`focused_child`/`neighbor_child` with their group indices), and the full
/// selected-group share vectors before (`old_shares`) and after
/// (`new_shares`). Only the two selected shares change ratios (plus an exact
/// ratio-preserving whole-group integer scaling when pixel precision requires
/// it); topology/order/descendants are unchanged. No geometry or native
/// handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeOperation {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub focused_leaf: NodeId,
    pub focused_window: WindowId,
    pub direction: Direction,
    pub mode: ResizeMode,
    pub target_group: NodeId,
    pub focused_child: NodeId,
    pub neighbor_child: NodeId,
    pub focused_index: usize,
    pub neighbor_index: usize,
    pub old_shares: Vec<u64>,
    pub new_shares: Vec<u64>,
}

/// Semantic drag drop side (COSMIC `DropTarget`: Left/Right/Top/Bottom edge
/// splits, or Center stack). `Center` is a semantic source fact: callers must
/// branch on `Center` first and only edges reach split placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragSide {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

impl DragSide {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Center => "center",
        }
    }

    /// Split axis ordered by this edge (`None` for the center stack fact).
    #[must_use]
    pub const fn axis(self) -> Option<Axis> {
        match self {
            Self::Left | Self::Right => Some(Axis::Horizontal),
            Self::Top | Self::Bottom => Some(Axis::Vertical),
            Self::Center => None,
        }
    }

    /// Whether the source orders before the target (`Left`/`Top`; `None` for
    /// the center stack fact).
    #[must_use]
    pub const fn before(self) -> Option<bool> {
        match self {
            Self::Left | Self::Top => Some(true),
            Self::Right | Self::Bottom => Some(false),
            Self::Center => None,
        }
    }

    /// Whether this side is the COSMIC center stack source fact.
    #[must_use]
    pub const fn is_stack(self) -> bool {
        matches!(self, Self::Center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_bounds_reject_without_echo() {
        assert!(is_owner_id("owner-1"));
        assert!(!is_owner_id(""));
        assert!(!is_owner_id("bad id"));
        assert!(!is_owner_id(&"x".repeat(MAX_OWNER_LEN + 1)));
        assert!(is_generation_id("gen-1"));
        assert!(!is_generation_id("GEN-1"));
        assert!(!is_generation_id(""));
        assert!(is_correlation_id("corr-1"));
        assert!(!is_correlation_id(""));
        assert!(is_revision(MAX_REVISION));
        assert!(!is_revision(MAX_REVISION + 1));
    }

    #[test]
    fn divergence_messages_are_fixed() {
        assert_eq!(
            DivergenceKind::StaleRevision.message(),
            "observation revision does not match verified state"
        );
        assert!(!DivergenceKind::OwnerMismatch.message().contains("owner-1"));
    }

    #[test]
    fn drag_side_maps_to_axis_and_order() {
        assert_eq!(DragSide::Left.axis(), Some(Axis::Horizontal));
        assert_eq!(DragSide::Right.axis(), Some(Axis::Horizontal));
        assert_eq!(DragSide::Top.axis(), Some(Axis::Vertical));
        assert_eq!(DragSide::Bottom.axis(), Some(Axis::Vertical));
        assert_eq!(DragSide::Center.axis(), None);
        assert_eq!(DragSide::Left.before(), Some(true));
        assert_eq!(DragSide::Top.before(), Some(true));
        assert_eq!(DragSide::Right.before(), Some(false));
        assert_eq!(DragSide::Bottom.before(), Some(false));
        assert_eq!(DragSide::Center.before(), None);
        assert_eq!(DragSide::Left.as_str(), "left");
        assert_eq!(DragSide::Center.as_str(), "center");
        assert!(DragSide::Center.is_stack());
        assert!(!DragSide::Left.is_stack());
        assert_eq!(ResizeMode::Inwards.as_str(), "inwards");
        assert_eq!(ResizeMode::Outwards.as_str(), "outwards");
    }
}
