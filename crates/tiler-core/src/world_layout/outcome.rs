//! Compact typed outcomes and failures for direct-endpoint layout operations.

use crate::contract::ResizeOperation;
use crate::directional::{Capability, Direction, NodeId, OutputId, Rule, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::FitDeclineReason;
use crate::session::{DesiredGeometry, DomainKey, DragHoverPrior, RefusalKind};

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

/// Typed failure for a composed local operation (retained, relocated, or
/// legacy tiled-seed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocalOperationFailure {
    Layout(LayoutFailure),
    AmbiguousPlacement,
    SeedFailed,
}

impl From<LayoutFailure> for LocalOperationFailure {
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

/// Typed result of an accepted intentional float or unfloat toggle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FloatOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Complete hinted geometry for the affected domain in child order.
    /// Empty when floating the last tiled window, on a capacity no-space
    /// re-float, or when only a tolerated fullscreen-only subject remains.
    pub(crate) geometry: Vec<DesiredGeometry>,
    /// Retained active focus leaf in the domain after the operation.
    pub(crate) focus_leaf: Option<NodeId>,
    /// Native float target: `Some(subject)` for tiled-to-float (with
    /// `float_rect: Some(...)`) and for unfloat capacity no-space re-float
    /// (with `float_rect: None`); `None` for unfloat-to-tile.
    pub(crate) float_window: Option<WindowId>,
    /// Effective float placement for tiled-to-float; `None` on unfloat or
    /// capacity no-space re-float.
    pub(crate) float_rect: Option<Rect>,
}

/// Typed result of an accepted single-domain directional move (R1-R3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalMoveOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Planned movement rule (R1, R2a, R2b, R2c or R3; never R4).
    pub(crate) rule: Rule,
    /// Required planner capability for the applied rule.
    pub(crate) capability: Capability,
    /// Requested movement direction.
    pub(crate) direction: Direction,
    /// Subject window (equal to the focused tiled window).
    pub(crate) window: WindowId,
    /// Focused leaf retained in the domain after the move.
    pub(crate) focus_leaf: NodeId,
    /// Complete hinted geometry reprojected from the moved tree, sorted by
    /// `(output, workspace, leaf)`.
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Typed result of an accepted single-domain directional focus change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalFocusOutcome {
    /// Accepted revision before the operation.
    pub(crate) base_revision: u64,
    /// Accepted revision after the operation (`base_revision + 1`).
    pub(crate) revision: u64,
    /// Requested focus direction.
    pub(crate) direction: Direction,
    /// Previously focused tiled window.
    pub(crate) from_window: WindowId,
    /// Previously focused leaf.
    pub(crate) from_leaf: NodeId,
    /// Newly focused target tiled window.
    pub(crate) to_window: WindowId,
    /// Newly focused target leaf.
    pub(crate) to_leaf: NodeId,
    /// Deterministic group-to-leaf descent route from the planner.
    pub(crate) route: Vec<NodeId>,
    /// Complete hinted geometry for the unmodified domain, sorted by
    /// `(output, workspace, leaf)`.
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Typed result of an accepted whole-workspace migration from a source domain
/// to an unoccupied target domain on another output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceMigrationOutcome {
    /// Relocated target member identities (tiled links plus carried exceptions),
    /// sorted by window id. Empty when the source was absent/retained-empty or
    /// when every observed member stayed sticky on the source.
    pub(crate) members: Vec<WindowId>,
    /// Number of retained exceptions carried into the relocated target domain.
    pub(crate) floats: usize,
}

/// Typed failure for whole-workspace migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceMigrationFailure {
    /// Semantic or revision failure from layout validation or convergence.
    Layout(LayoutFailure),
    /// Source and target share the same output (`cross-domain-mismatch` with
    /// the command-target message).
    SameOutput,
    /// Observation contains an unexplained `fit_excluded` window.
    OverlayPresent,
    /// Source outer gap is out of range or source bounds/gap/outer gap do
    /// not match the retained domain.
    DomainMismatch,
}

impl From<LayoutFailure> for WorkspaceMigrationFailure {
    fn from(value: LayoutFailure) -> Self {
        Self::Layout(value)
    }
}

/// Typed result of an accepted global-unique two-workspace swap across outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceSwapOutcome {
    pub(crate) moved_a: bool,
    pub(crate) moved_b: bool,
    pub(crate) stay_a: bool,
    pub(crate) stay_b: bool,
}

/// Typed failure for a global-unique two-workspace swap. Any failure leaves
/// `WorldLayout` untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceSwapFailure {
    /// Swap keys do not form a valid two-output, two-workspace exchange.
    InvalidKeys,
    /// Either target domain slot is already occupied.
    TargetOccupied,
    /// Either target bounds, inner gap, or outer gap is invalid.
    InvalidGeometry,
    /// A tiled or minimized window in a source domain is sticky.
    StickyTiledOrMinimized,
    /// Source or relocated candidate topology failed validation.
    MalformedTopology,
    /// Deriving a same-output sticky stay domain failed.
    StayFailed,
}

use super::pair::PairAssemblyFailure;

/// Typed failure for a composed two-domain pair operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PairOperationFailure {
    /// Canonical pair assembly, split, or install failed.
    Assembly(PairAssemblyFailure),
    /// Semantic or revision failure from layout validation, convergence, or command.
    Layout(LayoutFailure),
    /// First-time source seed construction or non-revision convergence failed.
    SeedFailed,
}

impl From<PairAssemblyFailure> for PairOperationFailure {
    fn from(value: PairAssemblyFailure) -> Self {
        Self::Assembly(value)
    }
}

impl From<LayoutFailure> for PairOperationFailure {
    fn from(value: LayoutFailure) -> Self {
        Self::Layout(value)
    }
}

/// Typed result of an accepted directional pair focus change (local or cross-output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairFocusOutcome {
    pub(crate) base_revision: u64,
    pub(crate) revision: u64,
    pub(crate) direction: Direction,
    pub(crate) from_window: WindowId,
    pub(crate) from_leaf: Option<NodeId>,
    pub(crate) to_window: WindowId,
    pub(crate) to_leaf: NodeId,
    pub(crate) crossed: bool,
    pub(crate) cross_source_output: Option<OutputId>,
    pub(crate) cross_source_workspace: Option<WorkspaceId>,
    pub(crate) target_output: OutputId,
    pub(crate) target_workspace: WorkspaceId,
    pub(crate) route: Vec<NodeId>,
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Cross-output transfer metadata for an accepted R4 directional pair move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairMoveCross {
    pub(crate) intent_window: WindowId,
    pub(crate) source_output: OutputId,
    pub(crate) source_workspace: WorkspaceId,
    pub(crate) target_output: OutputId,
    pub(crate) target_workspace: WorkspaceId,
}

/// Typed result of an accepted directional pair move (R1-R4 or R4 capacity-float fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PairMoveOutcome {
    Moved {
        base_revision: u64,
        revision: u64,
        rule: Rule,
        capability: Capability,
        direction: Direction,
        window: WindowId,
        focus_leaf: NodeId,
        geometry: Vec<DesiredGeometry>,
        cross: Option<PairMoveCross>,
    },
    CapacityFloat {
        base_revision: u64,
        revision: u64,
        window: WindowId,
        leaf: NodeId,
        source_output: OutputId,
        source_workspace: WorkspaceId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        follow: bool,
        geometry: Vec<DesiredGeometry>,
    },
}

/// Which transfer path executed an accepted explicit two-domain send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairSendKind {
    Tiled,
    CapacityFloat,
    Deferred,
}

/// Typed result of an accepted explicit two-domain send (workspace or output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PairSendOutcome {
    pub(crate) base_revision: u64,
    pub(crate) revision: u64,
    pub(crate) kind: PairSendKind,
    pub(crate) window: WindowId,
    pub(crate) source_output: OutputId,
    pub(crate) source_workspace: WorkspaceId,
    pub(crate) target_output: OutputId,
    pub(crate) target_workspace: WorkspaceId,
    pub(crate) follow: bool,
    pub(crate) focus_leaf: Option<NodeId>,
    pub(crate) stay_focus_window: Option<WindowId>,
    pub(crate) geometry: Vec<DesiredGeometry>,
}

/// Typed result of a read-only local or cross-output drag preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DragPreviewOutcome {
    pub(crate) base_revision: u64,
    pub(crate) domain: DomainKey,
    pub(crate) source_domain: DomainKey,
    pub(crate) target_window: WindowId,
    pub(crate) proposed_rect: Rect,
    pub(crate) hover: DragHoverPrior,
}

/// Typed result of an accepted local or cross-output drag drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DragDropOutcome {
    pub(crate) base_revision: u64,
    pub(crate) revision: u64,
}
