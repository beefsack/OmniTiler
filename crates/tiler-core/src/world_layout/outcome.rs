//! Compact typed outcomes and failures for direct-endpoint layout operations.

use crate::contract::ResizeOperation;
use crate::directional::{Direction, NodeId, OutputId, WindowId, WorkspaceId};
use crate::seed::FitDeclineReason;
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

/// Typed result of an accepted single-domain window admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmissionOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Bound leaf for a successful tiled admission; `None` for a deferred
    /// exception, automatic fixed membership float or capacity float.
    pub(crate) admitted_leaf: Option<NodeId>,
    /// Complete hinted geometry in child order; empty for every exception
    /// branch. A tolerated fullscreen-only subject is stripped from this
    /// vector before it is returned.
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Typed reason a fresh fitted admission did not install. A fit-stage decline
/// carries the fitter's reason; a semantic, validation or commit failure after
/// the fit succeeded collapses to `CommitFailed` (the old fit route fell back
/// on any failed commit). No correlation, report or topology text is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FittedAdmissionReason {
    Fit(FitDeclineReason),
    CommitFailed,
}

impl FittedAdmissionReason {
    /// Stable diagnostic token, matching the fitter's tokens for fit declines
    /// and `commit_failed` for the collapsed commit-failure branch.
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fit(reason) => reason.as_str(),
            Self::CommitFailed => "commit_failed",
        }
    }
}

/// Typed decline for [`WorldLayout::try_fitted_admission`](super::WorldLayout::try_fitted_admission).
/// `centre_splits` is the fitter's committed centre-split count for a centre
/// decline and zero otherwise; the generation-shared allocator burns survive
/// every decline regardless of the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FittedAdmissionDecline {
    pub(crate) reason: FittedAdmissionReason,
    pub(crate) centre_splits: usize,
}

/// Single-domain geometry and focus outcome. The domain is the caller's input
/// key. Retained reconciliation and gap updates preserve the accepted revision
/// and return sorted, clamp-marked geometry. Fresh adoption retains its route's
/// admission or convergence revision and geometry semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedProjectionOutcome {
    /// Accepted layout base revision of the producing operation.
    pub(crate) base_revision: u64,
    /// Hinted geometry in the producing operation's order.
    pub(crate) geometry: Vec<DesiredGeometry>,
    /// Retained focus leaf carried with the successful projection.
    pub(crate) focus_leaf: Option<NodeId>,
}

/// Typed failure for retained reconciliation. Relocation prechecks and the
/// fresh-routing absence signal are not failures (an absent target with no
/// eligible relocation returns `Ok(None)`); every other refusal is typed here.
/// The gap mismatches map to the outer layer's exact `domain-mismatch` kind
/// and messages, which stay outside this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RetainedReconcileFailure {
    /// A semantic or revision failure from the layout operation.
    Layout(LayoutFailure),
    /// Retained inner gap does not match the requested inner gap.
    InnerGapMismatch,
    /// Retained outer gap does not match the requested outer gap.
    OuterGapMismatch,
}

impl From<LayoutFailure> for RetainedReconcileFailure {
    fn from(value: LayoutFailure) -> Self {
        Self::Layout(value)
    }
}

/// Typed result of a fresh-domain adoption reconcile on an initially absent
/// target. Mixed and serial branches use retained clamp-marked projection;
/// fit and legacy admission keep their plan's child-order geometry without
/// client-clamp marks. Each keeps its operation's base revision.
/// `native_capacity_float`
/// is set only by the serial capacity-float branch, whose native opening frame
/// the adapter owns (no float-rect authority here). `kind` is compact typed
/// route evidence for the outer seam, never a report, correlation string,
/// identity or retained diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FreshDomainOutcome {
    pub(crate) projection: RetainedProjectionOutcome,
    pub(crate) native_capacity_float: Option<WindowId>,
    pub(crate) kind: FreshOutcomeKind,
}

/// Which fresh adoption route produced an outcome. Typed evidence only: the
/// outer layer decides whether any of this is surfaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FreshOutcomeKind {
    /// An empty, truly fresh observation: no retained slot, revision zero.
    Empty,
    /// Clean recursive-cut fit committed at revision one.
    Fit,
    /// Floating-aware carried observation converged in one commit.
    Converged,
    /// Serial spatial seed converged on an empty local domain.
    SerialSeed,
    /// Legacy seed or unique-source relocation plus the final anchor admit.
    Legacy,
}

/// Typed failure for a fresh-domain adoption reconcile. `Layout` carries every
/// ordinary semantic or revision refusal; `SeedFailed` carries legacy seed
/// construction and serial-seed non-revision convergence failures; the outer
/// boundary maps `AmbiguousPlacement` to the legacy ambiguous seed-order kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FreshDomainFailure {
    Layout(LayoutFailure),
    SeedFailed,
    AmbiguousPlacement,
}

impl From<LayoutFailure> for FreshDomainFailure {
    fn from(value: LayoutFailure) -> Self {
        Self::Layout(value)
    }
}

/// Typed failure for a layout operation: a semantic refusal with a stable
/// `RefusalKind`, or the per-domain accepted revision bound reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutFailure {
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

/// Typed result of an accepted single-axis keyboard or pointer edge resize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResizeOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Semantic resize operation (target group, adjacent pair, old/new shares).
    pub(crate) operation: ResizeOperation,
    /// Complete hinted geometry reprojected from the resized tree, sorted by
    /// `(output, workspace, leaf)`.
    pub(crate) geometry: Vec<DesiredGeometry>,
    /// Retained active focus leaf in the domain after the resize.
    pub(crate) focus_leaf: NodeId,
}

/// Typed result of an accepted atomic dual-axis (corner) pointer resize.
/// Axes are normalized horizontal-first, vertical-second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PointerCornerResizeOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Normalized horizontal direction and requested boundary.
    pub(crate) direction_h: Direction,
    pub(crate) boundary_h: i32,
    /// Normalized horizontal resize operation.
    pub(crate) operation_h: ResizeOperation,
    /// Normalized vertical direction and requested boundary.
    pub(crate) direction_v: Direction,
    pub(crate) boundary_v: i32,
    /// Normalized vertical resize operation.
    pub(crate) operation_v: ResizeOperation,
    /// Complete hinted geometry reprojected from the combined tree, sorted by
    /// `(output, workspace, leaf)`.
    pub(crate) geometry: Vec<DesiredGeometry>,
    /// Retained active focus leaf in the domain after the resize.
    pub(crate) focus_leaf: NodeId,
}
