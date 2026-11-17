//! Compact typed outcomes and failures for direct-endpoint layout operations.

use crate::directional::{NodeId, OutputId, WindowId, WorkspaceId};
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

/// Typed result of an atomic single-domain retained gap update. The domain is
/// the caller's input key and is not echoed back. `base_revision` is the
/// accepted revision observed before and after the update: gap adoption,
/// outer-gap adoption and D7 repair are all revision-neutral. `geometry` is
/// the complete hinted, clamp-marked projection at the requested bounds/gap;
/// it is empty for a retired empty domain, an exception-only/float-only slot,
/// or a repair that emptied the tiled set with floats remaining. `focus_leaf`
/// is the retained tiled focus binding projected with the update (only ever
/// `Some` when geometry is non-empty); an ordinary-float observed focus is
/// allowed as no focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedProjectionOutcome {
    /// Accepted revision before and after the update (never advanced here).
    pub(crate) base_revision: u64,
    /// Complete hinted geometry at the requested bounds/gap, ordered by
    /// `(output, workspace, leaf)`.
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
