//! Compact typed outcomes and failures for direct-endpoint layout operations.

use crate::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use crate::session::{DesiredGeometry, RefusalKind};

/// Typed result of an accepted parent-axis orientation flip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OrientationOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Subject window (equal to the focused tiled window).
    pub(crate) window: WindowId,
    /// Focused leaf whose immediate parent group was flipped. Focus is
    /// preserved: same window, same leaf, same tree child order and shares.
    pub(crate) leaf: NodeId,
    /// Immediate parent group whose split axis was flipped.
    pub(crate) group: NodeId,
    pub(crate) output: OutputId,
    pub(crate) workspace: WorkspaceId,
    /// Complete hinted geometry reprojected from the flipped tree.
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Typed failure for a layout operation: a semantic refusal with a stable
/// `RefusalKind`, or the per-domain accepted revision bound reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrientationFailure {
    Semantics(RefusalKind),
    /// The base revision has reached the bound. The outer operation boundary
    /// must map this to `DivergenceKind::RevisionExhausted`'s existing kind
    /// ("revision-exhausted") and message ("revision bound reached"), as the
    /// old `ProposeError::Diverged` mapping did, without minting a new wire
    /// outcome or retaining divergence state here.
    RevisionLimit {
        base: u64,
        limit: u64,
    },
}
