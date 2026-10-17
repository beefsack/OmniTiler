//! Durable platform-neutral authoritative session/domain and lifecycle foundation.
//!
//! transport-independent authoritative lifecycle over the ordered N-ary
//! [`crate::directional`] topology and the [`crate::geometry`] projector.
//! No platform, process, IPC, or native execution imports; only directional
//! semantic types, the portable projector, and the [`crate::contract`] /
//! [`crate::reconcile`] identity and acknowledgement model.
//!
//! Transaction model (immutable/transactional):
//! - `Session` owns logical `(OutputId, WorkspaceId)` domains, tiled topology
//!   and shares, the focused leaf, exception observation state, and the accepted
//!   revision (mirroring the internal [`crate::reconcile::Reconciler`]).
//! - `propose`: `&mut Session` + command/observation -> `SessionPlan` or
//!   refusal. Refusals leave state unchanged (except terminal divergence,
//!   which clears pending like the reconciler). At most one pending plan.
//! - `acknowledge`: delegates to the reconciler acknowledgement binding;
//!   terminal divergence clears the pending desired state like
//!   `note_adapter_loss`.
//! - `verify_lifecycle`: delegates to the reconciler lifecycle verifier; on
//!   commit the pending desired topology/focus/exceptions are applied and the
//!   accepted revision advances by exactly one. Terminal divergence clears the
//!   pending desired state.
//! - Acknowledgement/verification reuse the existing reconciler state machine;
//!   no second acknowledgement state machine exists here.
//!
//! Placement policy (explicit `cosmic_v1` lifecycle policy, deterministic,
//! source-evidenced where COSMIC establishes behavior):
//! - Versioned [`crate::contract::LIFECYCLE_POLICY_VERSION`] (`cosmic_v1`);
//!   plans/dispatch carry the binding and the reconciler rejects mismatches.
//!   Frozen R1-R4 movement APIs/fixtures are unaffected.
//! - Admission axis is owned by [`crate::cosmic_v1::admission_axis`]:
//!   target geometry width strictly greater than height selects portable
//!   [`Axis::Horizontal`] (splits width, source `Orientation::Vertical`),
//!   otherwise [`Axis::Vertical`] (vertical on tie).
//!   The first tiled admitted window in an empty logical domain becomes a root
//!   leaf; automatic admission always wraps the focused eligible leaf in an
//!   ordered binary group old/new carrying the admission axis and
//!   [`crate::cosmic_v1::new_group_shares`] (source `TilingLayout::new_group`
//!   2898-2936 via `map_to_tree` 548-617). Same-axis N-ary parent append never
//!   applies to admission; N-ary groups remain for movement/drag only. The new
//!   leaf is focused (deterministic project fallback; COSMIC does not
//!   establish admission focus).
//! - Input placement orientation is carried explicitly as logical `bounds`
//!   (target geometry for the split, output geometry for the no-focus root
//!   case under the same rule).
//! - When no eligible focus exists in the target domain but a tree exists,
//!   the entire root is wrapped old/new under the same admission axis/shares;
//!   focus fallback only, not source COSMIC parity.
//! - Removal retains empty logical domains (trees become
//!   `None`), removes empty groups, and collapses single-child groups
//!   recursively (survivor promotion mirrors `unmap_internal` structurally).
//!   Shares use [`crate::cosmic_v1::proportional_removal_shares`] before
//!   collapse (portable N-ary adaptation of `remove_window` 255-283).
//!   Unaffected subtree identity/order and shares are preserved
//!   (a collapsed child inherits its collapsed parent slot). A focused removal
//!   falls back through the source domain's MRU tiled focus stack; an unfocused
//!   removal preserves focus. Floating/fullscreen fallback is not represented
//!   by this tiled-only focus type.
//! - Floating/fullscreen/maximized/sticky exception flags are explicit. An
//!   admission carrying any set flag without an explicit
//!   [`ExceptionBehavior`] fails closed instead of silently tiling. An
//!   admission carrying `Some` behavior with no set flag is malformed and
//!   refused.
//!
//! Domain model: logical outputs and workspaces are separate domains keyed by
//! the distinct `(OutputId, WorkspaceId)` pair with exact opaque ids preserved
//! and deterministic construction order. Trees, focus, and projected geometry
//! are domain-scoped. The portable [`SessionSnapshot`] carries ordered N-ary
//! trees and [`WindowLink`]s and may repeat an `OutputId` across workspaces;
//! the frozen [`crate::directional::Snapshot`] cannot represent duplicate
//! output ids and is therefore exposed only per single logical domain via
//! [`Session::domain_snapshot`], never as a global abused snapshot.
//!
//! Plans carry full portable identity binding (owner/generation/correlation/
//! base revision via [`crate::contract::LifecycleDispatch`]), the semantic
//! lifecycle intent, affected ids (operation plus desired snapshot), required
//! capabilities/preconditions, desired topology/focus, and complete desired
//! geometry for affected tiled windows. No native commands exist here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::contract::{
    Dispatch, DivergenceKind, DragCapabilities, DragDispatch, DragIntent, DragOperation, DragPlan,
    DragPostObservation, DragSide, FocusCapabilities, FocusDispatch, FocusIntent, FocusOperation,
    FocusPlanContract, FocusPostObservation, LifecycleCapabilities, LifecycleDispatch,
    LifecycleIntent, LifecycleOperation, LifecyclePlan, LifecyclePostObservation, PostObservation,
    ResizeCapabilities, ResizeDispatch, ResizeIntent, ResizeMode, ResizeOperation, ResizePlan,
    ResizePostObservation, is_revision,
};
use crate::directional::{
    Axis, Capabilities, Direction, FocusPlan, MoveOperation, MovePlan, Node, NodeId, OutputId,
    Snapshot, WindowId, WindowLink, WorkspaceId,
};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
#[cfg(test)]
use crate::policy::CosmicV1Policy;
use crate::policy::{LayoutPolicy, default_policy};
use crate::reconcile::{AckApplied, AckError, Commit, Reconciler, StatusView, VerifyError};

mod fit;
mod ops;
mod world;

pub mod capacity;

pub use world::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ExceptionRecord, MinimizedSlot,
    ObservationConvergence, ObservedWindow, OutputDomain, SessionDomainView, SessionObservation,
    SessionSnapshot,
};

/// Closed stage vocabulary for the read-only drag-preview diagnostic.
/// Shape tokens only: no payload, id, rect, or domain labels.
pub(crate) const DRAG_PREVIEW_STAGE_CONVERGE: &str = "converge";
pub(crate) const DRAG_PREVIEW_STAGE_BEGIN: &str = "begin";
pub(crate) const DRAG_PREVIEW_STAGE_TOPOLOGY: &str = "topology";
pub(crate) const DRAG_PREVIEW_STAGE_STALE_CAPTURE: &str = "stale-capture";
pub(crate) const DRAG_PREVIEW_STAGE_SCOPE: &str = "scope";
pub(crate) const DRAG_PREVIEW_STAGE_RESOLVE_OUTSIDE: &str = "resolve-outside";
pub(crate) const DRAG_PREVIEW_STAGE_RESOLVE_TOPOLOGY: &str = "resolve-topology";
pub(crate) const DRAG_PREVIEW_STAGE_PROJECTION: &str = "projection";

/// Lifecycle command against the accepted session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    Admit {
        window: WindowId,
        output: OutputId,
        workspace: WorkspaceId,
        exceptions: ExceptionFlags,
        exception_behavior: Option<ExceptionBehavior>,
        placement_bounds: Rect,
        /// Explicit user-tile origin for this admission (unfloat path):
        /// the fixed-size classifier must not float this window, and the
        /// override stages transactionally with the plan. False for every
        /// other admission (fresh seeds consult the retained override
        /// set instead). Never affects topology, geometry, or focus.
        suppress_fixed_float: bool,
    },
    Remove {
        window: WindowId,
    },
    MoveToWorkspace {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// True follows the mover into the target domain; false stays on the
        /// source domain with the existing focused-removal MRU focus.
        follow: bool,
    },
    /// Explicit output send (REQ-OUT-04, item 5.3/5.4): same ordinary
    /// transfer as [`SessionCommand::MoveToWorkspace`] (remembered leaf,
    /// destination focus history, root fallback; explicit follow/stay) but
    /// the target is the destination output's current workspace, resolved
    /// adapter-side. Unlike `MoveToWorkspace`, cross-output targets are
    /// admitted; same-output targets refuse as `CrossDomainMismatch` so the
    /// two ops stay distinct.
    MoveToOutput {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// True follows the mover into the target domain; false stays on the
        /// source domain with the existing focused-removal MRU focus.
        follow: bool,
    },
    /// Explicit, stateful intentional-float transition. A tiled target becomes
    /// a non-tree floating exception; a tracked floating target is freshly
    /// admitted back into the tree.
    ToggleFloat {
        window: WindowId,
        float_geometry: Option<Rect>,
    },
    /// Toggle the focused tiled window's immediate parent group split axis
    /// (R-LAY-01, COSMIC `update_orientation` with no explicit orientation).
    /// The supplied window must equal the authoritative focused tiled window;
    /// child order, shares, and focus are preserved and geometry reprojects
    /// through the existing hints-aware projector. A lone root leaf (no
    /// parent group) refuses as [`RefusalKind::Unchanged`].
    ToggleOrientation {
        window: WindowId,
    },
}

/// Non-divergent session refusal reasons. Fixed redacted messages only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalKind {
    DuplicateWindow,
    UnknownWindow,
    UnknownDomain,
    FocusMismatch,
    NotTiled,
    PlannerNoop,
    PlannerRejected,
    UnsupportedCapability,
    CrossDomainMismatch,
    MalformedInput,
    MalformedTopology,
    PartialObservation,
    ExceptionBehaviorUnselected,
    Unchanged,
    PairBelowMinimum,
}

impl RefusalKind {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateWindow => "duplicate-window",
            Self::UnknownWindow => "unknown-window",
            Self::UnknownDomain => "unknown-domain",
            Self::FocusMismatch => "focus-mismatch",
            Self::NotTiled => "not-tiled",
            Self::PlannerNoop => "planner-noop",
            Self::PlannerRejected => "planner-rejected",
            Self::UnsupportedCapability => "unsupported-capability",
            Self::CrossDomainMismatch => "cross-domain-mismatch",
            Self::MalformedInput => "malformed-input",
            Self::MalformedTopology => "malformed-topology",
            Self::PartialObservation => "partial-observation",
            Self::ExceptionBehaviorUnselected => "exception-behavior-unselected",
            Self::Unchanged => "unchanged",
            Self::PairBelowMinimum => "pair-below-minimum",
        }
    }

    /// Fixed redacted message; never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::DuplicateWindow => "window is already known to the session",
            Self::UnknownWindow => "window is not known to the session",
            Self::UnknownDomain => "domain is not known to the session",
            Self::FocusMismatch => "focus does not resolve to the selected domain",
            Self::NotTiled => "focused window is not a tiled window",
            Self::PlannerNoop => "planner reports no movement",
            Self::PlannerRejected => "planner rejects the movement",
            Self::UnsupportedCapability => "adapter does not declare the required capability",
            Self::CrossDomainMismatch => {
                "input output or workspace does not match a logical domain"
            }
            Self::MalformedInput => "command or observation input is malformed",
            Self::MalformedTopology => "topology or shares are malformed or not projectable",
            Self::PartialObservation => "observation does not cover the known window set",
            Self::ExceptionBehaviorUnselected => {
                "exception window requires an explicit behavior selection"
            }
            Self::Unchanged => "command would not change session state",
            Self::PairBelowMinimum => "resize pair is below the minimum size",
        }
    }
}

/// Session proposal failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposeError {
    /// At most one pending plan.
    PendingExists,
    /// Fail-closed divergence; terminal like the reconciler.
    Diverged(DivergenceKind),
    /// Non-divergent refusal; state unchanged and still usable.
    Refused(RefusalKind),
}

impl ProposeError {
    /// Stable kind string, never echoes input.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::PendingExists => "pending-exists",
            Self::Diverged(reason) => reason.as_str(),
            Self::Refused(reason) => reason.as_str(),
        }
    }

    /// Fixed redacted message, never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::PendingExists => "complete the pending plan before proposing",
            Self::Diverged(reason) => reason.message(),
            Self::Refused(reason) => reason.message(),
        }
    }
}

/// Construction failure (pre-state, never divergence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionNewError {
    InvalidOwner,
    InvalidGeneration,
    RevisionOutOfBounds,
    InvalidDomain,
}

impl SessionNewError {
    /// Stable kind string.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::InvalidOwner => "owner-invalid",
            Self::InvalidGeneration => "generation-invalid",
            Self::RevisionOutOfBounds => "revision-out-of-bounds",
            Self::InvalidDomain => "domain-invalid",
        }
    }
}

/// Canonical pair assembly/split failure (pre-state, never divergence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalPairError {
    MismatchedIdentity,
    UnusableInput,
    DomainMismatch,
    DuplicateState,
}

impl CanonicalPairError {
    /// Stable kind string.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::MismatchedIdentity => "owner-generation-mismatch",
            Self::UnusableInput => "unusable-input",
            Self::DomainMismatch => "domain-mismatch",
            Self::DuplicateState => "duplicate-state",
        }
    }

    /// Fixed redacted message; never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::MismatchedIdentity => "canonical sessions do not share owner and generation",
            Self::UnusableInput => "canonical session is pending, dragging, or diverged",
            Self::DomainMismatch => "canonical domain keys do not match the pair domains",
            Self::DuplicateState => "canonical sessions share window or node identity",
        }
    }
}

/// Complete desired geometry for one affected tiled window.
/// Domain-scoped: names the exact `(output, workspace)` domain plus leaf.
///
/// AR12 diagnostics ride here so every workflow (admit/remove/move/
/// focus/resize/drag plus the reconcile/update-gaps projections) exposes
/// them uniformly:
/// - `overconstrained`: the window's minimums are unsatisfiable in this
///   allocation; the rectangle is the proportional fallback and must never
///   be reasserted. Set by hints-aware projection wherever hints are
///   available, else false.
/// - `client_clamped`: only the reconcile/update-gaps projection path sets
///   this, when the observed rectangle accepts as a client clamp of this
///   desired rectangle. The adapter must not rewrite the window nor count
///   it toward drift/park. Every other path leaves it false.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredGeometry {
    pub window: WindowId,
    pub leaf: NodeId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub rect: Rect,
    pub overconstrained: bool,
    pub client_clamped: bool,
}

/// Authoritative session plan: reconciler identity-bound lifecycle dispatch
/// plus desired topology/focus and complete desired geometry for affected
/// tiled windows. No native commands. The dispatch carries the
/// `cosmic_v1` lifecycle policy binding
/// ([`crate::contract::LIFECYCLE_POLICY_VERSION`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    pub dispatch: LifecycleDispatch,
    pub desired_snapshot: SessionSnapshot,
    pub desired_focus_domain: Option<DomainKey>,
    pub desired_focus_leaf: Option<NodeId>,
    pub desired_geometry: Vec<DesiredGeometry>,
}

/// Authoritative movement plan: reconciler identity-bound directional
/// [`Dispatch`] (operation/preconditions/rule/capability/intent) plus desired
/// topology/snapshot, desired focus, and complete desired rectangles for every
/// tiled window in affected domains. No KWin execution fields. Affected
/// domains are the source domain for R1-R3 and source plus adjacent target
/// for R4. The mover remains focused (for R4 the focus domain changes with
/// it). Immutable and complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMovePlan {
    pub dispatch: Dispatch,
    pub desired_snapshot: SessionSnapshot,
    pub desired_focus_domain: DomainKey,
    pub desired_focus_leaf: NodeId,
    pub desired_geometry: Vec<DesiredGeometry>,
}

/// Directional move outcome: either the exact planned tiled move
/// ([`SessionMovePlan`], R1-R4 exact wrap) or the R4 capacity-float
/// fallback ([`SessionPlan`] carrying a `MoveToFloat` operation under the
/// explicit output-send intent, follow-only). The fallback stages only
/// when the exact R4 wrap first plans successfully and then proves
/// geometric exhaustion under the float no-space result: the mover
/// becomes a slotless floating exception homed at the R4-chosen
/// output/workspace with its exact current frame (no D2 redirect, no
/// guessed geometry) instead of refusing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMoveOutcome {
    Moved(SessionMovePlan),
    CapacityFloat(SessionPlan),
}

impl SessionMoveOutcome {
    /// The exact tiled move plan, if the outcome moved without floating.
    #[must_use]
    pub fn moved(self) -> Option<SessionMovePlan> {
        match self {
            Self::Moved(plan) => Some(plan),
            Self::CapacityFloat(_) => None,
        }
    }

    /// The capacity-float fallback plan, if the R4 wrap exhausted.
    #[must_use]
    pub fn capacity_float(self) -> Option<SessionPlan> {
        match self {
            Self::CapacityFloat(plan) => Some(plan),
            Self::Moved(_) => None,
        }
    }

    /// Test-only accessor: the exact tiled move plan, panicking when the
    /// outcome is the capacity-float fallback.
    #[must_use]
    pub fn expect_moved(self, message: &str) -> SessionMovePlan {
        match self {
            Self::Moved(plan) => plan,
            Self::CapacityFloat(_) => panic!("expected tiled move: {message}"),
        }
    }
}

/// Authoritative focus plan: reconciler identity-bound portable
/// [`FocusDispatch`] (semantic focus operation/preconditions/capability/intent
/// plus owner/generation/correlation/base revision) plus the deterministic
/// directional [`FocusPlan`], desired topology/snapshot (unmodified),
/// desired focus, and complete desired rectangles for the selected domain.
/// No native commands. The focus mover commits only via
/// acknowledge-then-[`Session::verify_focus`]; exhausted edges refuse as
/// [`RefusalKind::Unchanged`] with no plan and no pending. Immutable and
/// complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFocusPlan {
    pub dispatch: FocusDispatch,
    pub focus_plan: FocusPlan,
    pub desired_snapshot: SessionSnapshot,
    pub desired_focus_domain: DomainKey,
    pub desired_focus_leaf: NodeId,
    pub desired_geometry: Vec<DesiredGeometry>,
}

/// Authoritative resize plan: reconciler identity-bound portable
/// [`ResizeDispatch`] (semantic resize operation/preconditions/
/// capability/intent plus owner/generation/correlation/base revision) plus the
/// deterministic contract [`ResizePlan`], desired topology/snapshot (shares
/// only), unchanged desired focus, and complete desired rectangles for every
/// tiled window in the affected domain. No native commands. The resizer
/// commits only via acknowledge-then-[`Session::verify_resize`]; missing
/// boundaries refuse as [`RefusalKind::Unchanged`] with no plan and no
/// pending. Focus is retained exactly. Immutable and complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResizePlan {
    pub dispatch: ResizeDispatch,
    pub resize_plan: ResizePlan,
    /// Second-axis plan for an atomic corner (dual-axis) pointer resize.
    /// `None` for every single-axis plan; `Some` only when one request
    /// carries both a horizontal and a vertical boundary. The desired
    /// trees/snapshot/geometry already combine both axes; this carries the
    /// vertical plan for the bound pending operation and the reply detail.
    pub secondary_plan: Option<ResizePlan>,
    pub desired_snapshot: SessionSnapshot,
    pub desired_focus_domain: DomainKey,
    pub desired_focus_leaf: NodeId,
    pub desired_geometry: Vec<DesiredGeometry>,
}

/// Authoritative drag plan: reconciler identity-bound portable
/// [`DragDispatch`] (semantic drag operation/preconditions/capability/intent
/// plus owner/generation/correlation/base revision) plus the deterministic
/// contract [`DragPlan`], desired topology/snapshot (edge drops: source
/// removed, collapsed, then inserted/wrapped; center never plans
/// topology rendered natively by the compositor), preserved focus on the moved
/// window, and complete desired rectangles for every tiled window in the
/// affected domain. No native
/// commands. The drag commits only via acknowledge-then-[`Session::verify_drag`];
/// self/invalid releases snap back with no plan and no pending. Focus is
/// preserved on the moved window. Immutable and complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDragPlan {
    pub dispatch: DragDispatch,
    pub drag_plan: DragPlan,
    pub desired_snapshot: SessionSnapshot,
    pub desired_focus_domain: DomainKey,
    pub desired_focus_leaf: NodeId,
    pub desired_geometry: Vec<DesiredGeometry>,
}

/// Portable drag capture returned by [`Session::begin_drag`]: the source domain
/// and tiled leaf/window, the accepted revision bound to the capture, and the
/// projected source rectangle plus the domain work area. Carries no topology
/// mutation and no native commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragCapture {
    pub domain: DomainKey,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub revision: u64,
    pub source_rect: Rect,
    pub work_area: Rect,
}

/// Pure non-mutating drag preview from logical pointer coordinates. Names the
/// source and target portable ids with their projected rectangles plus the
/// normalized `side`/`axis`/`before` ordering and the resolved placement form
/// (`wrap` with `target_group`/`insertion_index`). `proposed_rect` is the
/// deterministic projected desired source rectangle after applying the
/// placement (subregion of the work area for the moved source leaf), not the
/// target current rectangle alone. `revision` binds the preview to the exact
/// accepted revision of its capture; `prior` carries the stored next hover
/// (Some only for a resolved GroupEdge) so single-shot clones can forward it.
/// `domain` is the resolution domain (the pointer domain); `source_domain` is
/// the capture domain the mover leaves (equal for same-domain previews).
/// No native rendering or effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPreview {
    pub domain: DomainKey,
    pub source_domain: DomainKey,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub revision: u64,
    pub source_rect: Rect,
    pub target_leaf: NodeId,
    pub target_window: WindowId,
    pub target_rect: Rect,
    pub proposed_rect: Rect,
    pub side: DragSide,
    pub axis: Axis,
    pub before: bool,
    pub wrap: bool,
    pub target_group: NodeId,
    pub insertion_index: usize,
    pub prior: Option<crate::policy::PriorGroupEdge>,
}

impl DragPreview {
    /// Portable hover carry for the next preview or the final drop: the exact
    /// capture identity (source domain/leaf/window plus revision) and the
    /// stored next hover, scoped to the resolution domain so a cross-output
    /// hover never applies to another output's tree.
    /// [`Session::carry_drag_prior`] validates it against the live capture and
    /// ignores mismatches advisory-style (no refusal, no divergence).
    #[must_use]
    pub fn hover_prior(&self) -> DragHoverPrior {
        DragHoverPrior {
            domain: self.source_domain.clone(),
            source_leaf: self.source_leaf.clone(),
            source_window: self.source_window.clone(),
            revision: self.revision,
            prior: self.prior.clone(),
            prior_domain: self.domain.clone(),
        }
    }
}

/// Portable sticky group-edge hover carry: the exact drag capture identity
/// (capture domain/leaf/window plus revision), the stored next hover (`None`
/// when the last hover was not a group edge), and the resolution domain the
/// hover was classified in (`prior_domain`, equal to the capture domain for
/// same-domain hovers). Applied via [`Session::carry_drag_prior`] before the
/// next preview or the final drop; mismatched carries are ignored. The
/// resolution scope keeps a cross-output 80px sticky edge from ever applying
/// to another domain's tree (which degrades to the normal 32px edge depth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragHoverPrior {
    pub domain: DomainKey,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub revision: u64,
    pub prior: Option<crate::policy::PriorGroupEdge>,
    pub prior_domain: DomainKey,
}

/// Portable no-structure snap-back intent: the accepted source rectangle the
/// adapter must restore visually. Carries no topology and no geometry beyond
/// the accepted source rect. Returned by [`Session::cancel_drag`] and by
/// invalid [`Session::drop_drag`] releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragSnapBack {
    pub domain: DomainKey,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub source_rect: Rect,
}

/// Drag release outcome: either a complete structural/focus/geometry
/// [`SessionDragPlan`] staged through the shared reconciler slot, or a
/// no-structure [`DragSnapBack`] with no reconciler use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DragRelease {
    Planned(Box<SessionDragPlan>),
    SnapBack(DragSnapBack),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingDesired {
    trees: BTreeMap<DomainKey, Option<Node>>,
    windows: BTreeMap<WindowId, WindowLink>,
    focused_domain: Option<DomainKey>,
    focused_leaf: Option<NodeId>,
    last_active: BTreeMap<DomainKey, NodeId>,
    exceptions: BTreeMap<WindowId, ExceptionRecord>,
    retained_float_geometry: BTreeMap<WindowId, Rect>,
    /// Desired fixed-size bookkeeping staged transactionally with the
    /// topology above: applied atomically at verify, discarded on any
    /// refusal, acknowledgement failure, cancellation, or divergence, so
    /// failed proposals never leak markers into future classifications.
    automatic_fixed: BTreeSet<WindowId>,
    fixed_tile_override: BTreeSet<WindowId>,
    /// Desired capacity-float origin markers (diagnostic only, distinct
    /// from `automatic_fixed`): same transactional staging as above.
    capacity_floats: BTreeSet<WindowId>,
    /// Desired first-successful-tiled-admission order (D7 victim order):
    /// same transactional staging as above.
    admission_order: BTreeMap<WindowId, u64>,
    admission_counter: u64,
    /// Desired overlap-accepted domains: same transactional staging.
    overlap_accepted: BTreeSet<DomainKey>,
}

/// Transient drag capture: source identity plus the accepted
/// revision/generation, the full topology/membership/focus at capture time,
/// and the projected source/work-area geometry preconditions. Holds no
/// reconciler pending slot; cleared by drop, cancel, commit-divergence paths,
/// or acknowledgement-divergence. The smallest portable prior hover
/// (`cosmic_v1::PriorGroupEdge`: last emitted group-edge identity, or `None`
/// when the last hover was not a group edge) lives here only and drives the
/// sticky 80px/32px group-edge depths; it updates after each resolved
/// preview/drop target and never participates in stale-capture equality.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DragState {
    domain: DomainKey,
    source_leaf: NodeId,
    source_window: WindowId,
    revision: u64,
    generation: GenerationId,
    trees: BTreeMap<DomainKey, Option<Node>>,
    windows: BTreeMap<WindowId, WindowLink>,
    exceptions: BTreeMap<WindowId, ExceptionRecord>,
    focused_domain: Option<DomainKey>,
    focused_leaf: Option<NodeId>,
    source_rect: Rect,
    work_area: Rect,
    prior: Option<crate::policy::PriorGroupEdge>,
    /// Resolution domain of the stored hover (`prior`): the capture domain
    /// for same-domain hovers, the pointer domain for cross-output hovers.
    /// Resolution gates the stored hover against it, so a hover classified
    /// in one domain's tree never steers another domain's tree.
    prior_domain: DomainKey,
}

/// Trace-only send-to-workspace placement diagnostic: which anchor branch
/// the `map_to_tree` (`direction=None`) resolution selected plus its opaque
/// leaf, the admission axis, that projected rectangle, and the target leaf
/// count. Set on successful `propose_move_to_workspace` only; the Engine
/// copies it into its own per-op report. Opaque tokens only, never native
/// window or platform identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendPlacementTrace {
    pub anchor_kind: &'static str,
    pub anchor: Option<NodeId>,
    pub axis: Axis,
    pub projected: Rect,
    pub target_leaves: usize,
}

/// Durable authoritative session. See module docs for the transaction model.
#[derive(Debug, Clone)]
pub struct Session {
    policy: Arc<dyn LayoutPolicy>,
    owner: OwnerId,
    generation: GenerationId,
    accepted_fingerprint: u64,
    domains: Vec<OutputDomain>,
    trees: BTreeMap<DomainKey, Option<Node>>,
    windows: BTreeMap<WindowId, WindowLink>,
    focused_domain: Option<DomainKey>,
    focused_leaf: Option<NodeId>,
    focus_stack: BTreeMap<DomainKey, Vec<NodeId>>,
    last_active: BTreeMap<DomainKey, NodeId>,
    exceptions: BTreeMap<WindowId, ExceptionRecord>,
    retained_float_geometry: BTreeMap<WindowId, Rect>,
    /// Opt-in Q2 fixed-size float admission (D1-D8). Off by default so
    /// Windows behavior is unchanged; the Linux planner route enables it.
    /// Cloned verbatim; owner/generation rebinding clears the world map
    /// (Engine) so no tile-override persists across restart (D7).
    fixed_admission: bool,
    /// R-SPC-04 D1 fixed-size admission predicate. Both-axes default
    /// (current delivered behavior); either-axis floats one-axis-fixed
    /// windows. Admission-only: changing it never reclassifies retained
    /// windows, only subsequent admissions.
    fixed_predicate: crate::size_hints::FixedSizePredicate,
    /// Opt-in G-06 maximized directional focus fence (REQ-MAX-08). Off by
    /// default so Windows carriers keep exact current behavior; the Linux
    /// planner route enables it. When on, directional focus with a
    /// maximized (non-fullscreen) focused subject refuses as `Unchanged`
    /// with no plan and no pending. Cloned verbatim like `fixed_admission`.
    maximized_focus_fence: bool,
    /// Opt-in deferred send for automatic fixed-size floats (Windows
    /// workspace sends). Off by default so KDE/macOS carriers keep exact
    /// current behavior; the Windows route enables it. When on, the
    /// automatic-origin signals (`fixed_auto` observation flag and the
    /// retained `automatic_fixed` marker) no longer refuse the slotless
    /// deferred transfer: automatic floats send like intentional floats
    /// with flags and floating size preserved verbatim and no geometry
    /// writes. Sticky, dual overlays, exited movers, and explicit tile
    /// wins (`fixed_suppress`) still refuse. Cloned verbatim like
    /// `fixed_admission`.
    fixed_send_admission: bool,
    /// Classifier-created automatic fixed floats (membership only, D8).
    /// Intentional floats (toggle-float, sticky) never land here (D6).
    automatic_fixed: BTreeSet<WindowId>,
    /// Explicit user tile wins (unfloat, sticky-off-to-tile) for the same
    /// live client (D3). Cleared when the window leaves membership, so a
    /// removed/replaced native reference with a reused id classifies again.
    fixed_tile_override: BTreeSet<WindowId>,
    /// Best-effort admission policy (D2/D3, selected 2026-10-17): anchor
    /// fallback and no-space result for subsequent capacity decisions only.
    /// Changing either never reflows, retries, or reclassifies retained
    /// state (D10).
    anchor_fallback: crate::admission::AnchorFallback,
    no_space: crate::admission::NoSpaceResult,
    /// Diagnostic capacity-float origin (D4): windows floating because no
    /// split was available. Functionally ordinary floats with no
    /// capacity-specific behavior; distinct from `automatic_fixed`.
    /// Cleared when the window leaves membership.
    capacity_floats: BTreeSet<WindowId>,
    /// First-successful-tiled-admission sequence (D7 victim order):
    /// window lifetime to its admission sequence number. A clean-fit
    /// roster shares one number (identity breaks those ties). Preserved
    /// across moves and re-tiles for the same lifetime; cleared on close.
    admission_order: BTreeMap<WindowId, u64>,
    admission_counter: u64,
    /// Domains whose retained tree was accepted via overlap projection
    /// (D9). Such allocations stay projectable until a real capacity
    /// decision replaces them, even after switching the no-space setting
    /// back to float.
    overlap_accepted: BTreeSet<DomainKey>,
    /// D23 minimize restore slots (REQ-MNZ-01/02/03, option (b)): one stored
    /// old slot per minimized tiled window, keyed by window id. Session-runtime
    /// only: seeded empty, never persisted, dropped on owner restart with the
    /// world map. A slot holds the pre-minimize domain tree plus the removed
    /// leaf so restore can return the window to its old neighborhood: an
    /// exact fast path restores the whole stored tree when nothing
    /// intervened, else a best-effort rejoin in the surviving old group at
    /// the old index, or regrouped with a live former neighbor on the
    /// stored axis and side, with an entrant share preserving the stored
    /// old fraction. Resizes, admissions, sibling closes with a survivor,
    /// swaps, other minimize/restore roundtrips, and gaps/resolution
    /// changes therefore do not alone invalidate. Fresh ordinary admission
    /// applies only with no live former neighbor, a gone domain, or a
    /// window sent elsewhere; explicit close forgets the slot via
    /// [`Session::forget_minimized_slot`].
    minimized_slots: BTreeMap<WindowId, MinimizedSlot>,
    // Only a transient canonical pair uses this. It restores target-local node
    // identities after pair-wide planning, whose snapshot requires uniqueness.
    canonical_pair_target_restore: BTreeMap<NodeId, NodeId>,
    reconciler: Reconciler,
    pending_desired: Option<PendingDesired>,
    drag: Option<DragState>,
    last_send_placement: Option<SendPlacementTrace>,
    /// Read-only drag-preview diagnostic stage for the current call.
    /// Set only by the preview fns on ambiguous refusal; cleared at each
    /// preview entry. Never touches topology, pending, or reconciler state.
    preview_stage: Option<&'static str>,
}

impl Session {
    /// Create an empty session over explicit logical domains.
    pub fn new(
        owner: OwnerId,
        generation: GenerationId,
        initial_revision: u64,
        initial_fingerprint: u64,
        domains: Vec<OutputDomain>,
    ) -> Result<Self, SessionNewError> {
        Self::new_with_policy(
            owner,
            generation,
            initial_revision,
            initial_fingerprint,
            domains,
            default_policy(),
        )
    }

    /// Create an empty session carrying an explicit layout policy.
    ///
    /// The policy is stateless transactionally: it never affects owner,
    /// generation, revision, divergence, pending, or drag state, only which
    /// layout semantics admission/planning/resize/drag decisions use. Clones,
    /// backups, and canonical pair/split outputs preserve it.
    pub fn new_with_policy(
        owner: OwnerId,
        generation: GenerationId,
        initial_revision: u64,
        initial_fingerprint: u64,
        domains: Vec<OutputDomain>,
        policy: Arc<dyn LayoutPolicy>,
    ) -> Result<Self, SessionNewError> {
        if !crate::contract::is_owner_id(owner.as_str()) {
            return Err(SessionNewError::InvalidOwner);
        }
        if !crate::contract::is_generation_id(generation.as_str()) {
            return Err(SessionNewError::InvalidGeneration);
        }
        if !is_revision(initial_revision) {
            return Err(SessionNewError::RevisionOutOfBounds);
        }
        if domains.is_empty() || !domains.iter().all(|d| d.validate()) {
            return Err(SessionNewError::InvalidDomain);
        }
        let mut seen = BTreeSet::new();
        for domain in &domains {
            if !seen.insert((domain.id.clone(), domain.workspace.clone())) {
                return Err(SessionNewError::InvalidDomain);
            }
        }
        if !validate_adjacency(&domains) {
            return Err(SessionNewError::InvalidDomain);
        }
        let reconciler = Reconciler::new(
            owner.clone(),
            generation.clone(),
            initial_revision,
            initial_fingerprint,
        )
        .map_err(|_| SessionNewError::RevisionOutOfBounds)?;
        let trees = domains.iter().map(|d| (d.key(), None)).collect();
        Ok(Self {
            policy,
            owner,
            generation,
            accepted_fingerprint: initial_fingerprint,
            domains,
            trees,
            windows: BTreeMap::new(),
            focused_domain: None,
            focused_leaf: None,
            focus_stack: BTreeMap::new(),
            last_active: BTreeMap::new(),
            exceptions: BTreeMap::new(),
            retained_float_geometry: BTreeMap::new(),
            fixed_admission: false,
            fixed_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            maximized_focus_fence: false,
            fixed_send_admission: false,
            automatic_fixed: BTreeSet::new(),
            fixed_tile_override: BTreeSet::new(),
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space: crate::admission::NoSpaceResult::default(),
            capacity_floats: BTreeSet::new(),
            admission_order: BTreeMap::new(),
            admission_counter: 0,
            overlap_accepted: BTreeSet::new(),
            minimized_slots: BTreeMap::new(),
            canonical_pair_target_restore: BTreeMap::new(),
            reconciler,
            pending_desired: None,
            drag: None,
            last_send_placement: None,
            preview_stage: None,
        })
    }

    /// Accepted revision (mirrors the reconciler verified revision).
    #[must_use]
    pub fn accepted_revision(&self) -> u64 {
        self.reconciler.verified_revision()
    }

    /// Trace-only send placement diagnostic from the last successful
    /// `propose_move_to_workspace` on this session, if any.
    #[must_use]
    pub fn last_send_placement(&self) -> Option<&SendPlacementTrace> {
        self.last_send_placement.as_ref()
    }

    /// Read-only drag-preview diagnostic stage recorded by the last preview
    /// call on this session, if it refused with an ambiguous kind.
    #[must_use]
    pub fn preview_stage(&self) -> Option<&'static str> {
        self.preview_stage
    }

    /// Accepted fingerprint from the last commit (or initial seed).
    #[must_use]
    pub const fn accepted_fingerprint(&self) -> u64 {
        self.accepted_fingerprint
    }

    /// Session owner binding (mirrors the reconciler binding).
    #[must_use]
    pub fn owner(&self) -> &OwnerId {
        &self.owner
    }

    /// Session generation binding (mirrors the reconciler binding).
    #[must_use]
    pub fn generation(&self) -> &GenerationId {
        &self.generation
    }

    /// Selected layout policy driving admission/planning/resize/drag
    /// decisions. Stateless transactionally; cloning preserves it.
    #[must_use]
    pub fn policy(&self) -> &dyn LayoutPolicy {
        &*self.policy
    }

    /// Carry a selected policy into this session without touching owner,
    /// generation, revision, divergence, pending, or drag state. The engine
    /// uses this when retaining sessions so every stored session carries the
    /// selected policy.
    pub fn set_policy(&mut self, policy: Arc<dyn LayoutPolicy>) {
        self.policy = policy;
    }

    /// Recorded divergence, if fail-closed.
    #[must_use]
    pub fn divergence(&self) -> Option<DivergenceKind> {
        self.reconciler.divergence()
    }

    /// Redacted status view (delegates to the reconciler).
    #[must_use]
    pub fn status(&self) -> StatusView {
        self.reconciler.status()
    }

    /// Whether a plan is pending acknowledgement/verification.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.reconciler.status().pending
    }

    /// Whether pending desired session state is staged.
    #[must_use]
    pub fn has_pending_desired(&self) -> bool {
        self.pending_desired.is_some()
    }

    /// Build a temporary two-domain Session from one or two canonical
    /// single-domain Sessions without reseeding.
    ///
    /// `source` maps to `pair_domains[0]` and `target` (when present) maps to
    /// `pair_domains[1]` by exact `(output, workspace)` key match. Component
    /// domains must be single, valid, non-adjacent, usable
    /// (no divergence/pending/drag, valid topology) with matching
    /// owner/generation. `pair_domains` must be exactly two valid domains with
    /// distinct keys: cross-output pairs keep the reciprocal-adjacency fence
    /// (each side names the other output), while same-output
    /// distinct-workspace pairs carry no cross-output adjacency requirement.
    /// Trees/shares, `WindowLink`
    /// membership, focus stacks, last-active, exceptions (including
    /// `floating_geometry`), retained float geometry, owner/generation, and
    /// the unified accepted revision/fingerprint are transplanted verbatim; no
    /// admission reseeds.
    ///
    /// Revision unification: the pair carries a single reconciler, so it seeds
    /// with the maximum accepted revision of the inputs (and that input's
    /// fingerprint; source wins ties). A target whose revision is lower
    /// advances to the pair revision without topology change; split
    /// propagates the pair's current revision/fingerprint to both outputs.
    /// Pair focus is the source focus when present, else the target focus; the
    /// other domain's focus survives in its focus-stack top and is restored by
    /// [`Session::split_canonical_pair`]. Duplicate window/exception/retained
    /// entries across inputs fail closed. Node ids are domain-scoped and may
    /// therefore repeat across independently retained component trees. Never mutates
    /// the inputs and never diverges.
    pub fn paired_from_canonical(
        source: &Session,
        target: Option<&Session>,
        pair_domains: Vec<OutputDomain>,
    ) -> Result<Session, CanonicalPairError> {
        Self::paired_from_canonical_impl(source, target, pair_domains, true)
    }

    /// Canonical pair assembly for explicit sends (workspace send and
    /// explicit output send, REQ-WS-01/REQ-OUT-04): identical to
    /// [`Session::paired_from_canonical`] except the cross-output reciprocal-
    /// adjacency fence is skipped. An explicit send names both endpoints
    /// (the destination output's current workspace is resolved adapter-side),
    /// so no adjacency-derived candidate needs disambiguation; every other
    /// fence (distinct keys, usable components, identity, duplicate state)
    /// still applies. Directional R4 keeps the fenced constructor: there the
    /// target IS the adjacency-derived candidate.
    pub fn paired_from_canonical_for_send(
        source: &Session,
        target: Option<&Session>,
        pair_domains: Vec<OutputDomain>,
    ) -> Result<Session, CanonicalPairError> {
        Self::paired_from_canonical_impl(source, target, pair_domains, false)
    }

    fn paired_from_canonical_impl(
        source: &Session,
        target: Option<&Session>,
        pair_domains: Vec<OutputDomain>,
        require_adjacency: bool,
    ) -> Result<Session, CanonicalPairError> {
        if pair_domains.len() != 2
            || !pair_domains.iter().all(|d| d.validate())
            || pair_domains[0].key() == pair_domains[1].key()
            || !validate_adjacency(&pair_domains)
        {
            return Err(CanonicalPairError::DomainMismatch);
        }
        // Cross-output pairs keep the reciprocal-adjacency fence, except for
        // explicit sends whose endpoints are both named (see
        // `paired_from_canonical_for_send`); same-output distinct-workspace
        // pairs need no cross-output adjacency.
        if require_adjacency
            && pair_domains[0].id != pair_domains[1].id
            && (!pair_domains[0]
                .adjacent
                .values()
                .any(|v| v == &pair_domains[1].id)
                || !pair_domains[1]
                    .adjacent
                    .values()
                    .any(|v| v == &pair_domains[0].id))
        {
            return Err(CanonicalPairError::DomainMismatch);
        }
        if !canonical_component_usable(source) || source.domains.len() != 1 {
            return Err(canonical_component_error(source));
        }
        if !source.domains[0].adjacent.is_empty() {
            return Err(CanonicalPairError::DomainMismatch);
        }
        if source.domains[0].key() != pair_domains[0].key() {
            return Err(CanonicalPairError::DomainMismatch);
        }
        if let Some(target) = target {
            if !canonical_component_usable(target) || target.domains.len() != 1 {
                return Err(canonical_component_error(target));
            }
            if !target.domains[0].adjacent.is_empty() {
                return Err(CanonicalPairError::DomainMismatch);
            }
            if target.domains[0].key() != pair_domains[1].key() {
                return Err(CanonicalPairError::DomainMismatch);
            }
            if target.owner != source.owner || target.generation != source.generation {
                return Err(CanonicalPairError::MismatchedIdentity);
            }
        }
        if let Some(target) = target {
            let mut seen_windows: BTreeSet<&WindowId> = BTreeSet::new();
            for id in source.windows.keys().chain(source.exceptions.keys()) {
                seen_windows.insert(id);
            }
            for id in target.windows.keys().chain(target.exceptions.keys()) {
                if !seen_windows.insert(id) {
                    return Err(CanonicalPairError::DuplicateState);
                }
            }
            for id in source.windows.keys() {
                if target.exceptions.contains_key(id) {
                    return Err(CanonicalPairError::DuplicateState);
                }
            }
            for id in target.windows.keys() {
                if source.exceptions.contains_key(id) {
                    return Err(CanonicalPairError::DuplicateState);
                }
            }
            for id in source.retained_float_geometry.keys() {
                if target.retained_float_geometry.contains_key(id) {
                    return Err(CanonicalPairError::DuplicateState);
                }
            }
        }
        let (revision, fingerprint) = match target {
            Some(target) if target.accepted_revision() > source.accepted_revision() => {
                (target.accepted_revision(), target.accepted_fingerprint())
            }
            _ => (source.accepted_revision(), source.accepted_fingerprint()),
        };
        let mut pair = Session::new(
            source.owner.clone(),
            source.generation.clone(),
            revision,
            fingerprint,
            pair_domains.clone(),
        )
        .map_err(|_| CanonicalPairError::DomainMismatch)?;
        // The pair is one session with one policy: the source component's
        // selected policy wins (inputs always agree while COSMIC v1 is the
        // only policy). No revision/divergence/pending state is touched.
        pair.set_policy(source.policy.clone());
        let source_key = source.domains[0].key();
        let pair_source_key = pair_domains[0].key();
        let pair_target_key = pair_domains[1].key();
        let mut trees: BTreeMap<DomainKey, Option<Node>> = BTreeMap::new();
        trees.insert(
            pair_source_key.clone(),
            source.trees.get(&source_key).cloned().flatten(),
        );
        if let Some(target) = target {
            let target_key = target.domains[0].key();
            trees.insert(
                pair_target_key.clone(),
                target.trees.get(&target_key).cloned().flatten(),
            );
        } else {
            trees.insert(pair_target_key.clone(), None);
        }
        let mut windows = source.windows.clone();
        let mut exceptions = source.exceptions.clone();
        let mut retained = source.retained_float_geometry.clone();
        let mut focus_stack = source.focus_stack.clone();
        let mut last_active = source.last_active.clone();
        // Remap per-domain focus bookkeeping from component keys to pair keys.
        // Keys match by (output, workspace) here, but re-key explicitly so a
        // caller-supplied pair with equal keys stays exact.
        remap_domain_map(&mut focus_stack, &source_key, &pair_source_key);
        remap_domain_map(&mut last_active, &source_key, &pair_source_key);
        let mut focused_domain = source.focused_domain.clone();
        if focused_domain.as_ref() == Some(&source_key) {
            focused_domain = Some(pair_source_key.clone());
        }
        let mut focused_leaf = source.focused_leaf.clone();
        let mut target_restore = BTreeMap::new();
        let mut pair_minimized = source.minimized_slots.clone();
        if let Some(target) = target {
            let target_key = target.domains[0].key();
            let target_remap = pair_target_node_remap(source, target);
            target_restore = target_remap
                .iter()
                .map(|(from, to)| (to.clone(), from.clone()))
                .collect();
            for (id, link) in &target.windows {
                let mut link = link.clone();
                remap_node_id(&mut link.leaf, &target_remap);
                windows.insert(id.clone(), link);
            }
            for (id, record) in &target.exceptions {
                exceptions.insert(id.clone(), record.clone());
            }
            for (id, rect) in &target.retained_float_geometry {
                retained.insert(id.clone(), *rect);
            }
            for (id, slot) in &target.minimized_slots {
                if pair_minimized.contains_key(id) {
                    return Err(CanonicalPairError::DuplicateState);
                }
                let mut slot = slot.clone();
                remap_node_id(&mut slot.leaf, &target_remap);
                remap_tree_node_ids(&mut slot.tree, &target_remap);
                pair_minimized.insert(id.clone(), slot);
            }
            for (key, leaves) in &target.focus_stack {
                let remapped = if key == &target_key {
                    pair_target_key.clone()
                } else {
                    key.clone()
                };
                let mut leaves = leaves.clone();
                remap_node_ids(&mut leaves, &target_remap);
                focus_stack.insert(remapped, leaves);
            }
            for (key, leaf) in &target.last_active {
                let remapped = if key == &target_key {
                    pair_target_key.clone()
                } else {
                    key.clone()
                };
                let mut leaf = leaf.clone();
                remap_node_id(&mut leaf, &target_remap);
                last_active.insert(remapped, leaf);
            }
            if focused_domain.is_none() {
                focused_domain = target.focused_domain.as_ref().map(|d| {
                    if d == &target_key {
                        pair_target_key.clone()
                    } else {
                        d.clone()
                    }
                });
                focused_leaf = target.focused_leaf.clone();
                if let Some(leaf) = focused_leaf.as_mut() {
                    remap_node_id(leaf, &target_remap);
                }
            }
            let tree = trees.get_mut(&pair_target_key).expect("pair target tree");
            remap_optional_tree(tree, &target_remap);
        }
        pair.trees = trees;
        pair.windows = windows;
        pair.focused_domain = focused_domain;
        pair.focused_leaf = focused_leaf;
        pair.focus_stack = focus_stack;
        pair.last_active = last_active;
        pair.exceptions = exceptions;
        pair.retained_float_geometry = retained;
        // Fixed-size admission rides the pair verbatim: the source opt-in
        // wins (components always agree on the Linux route), and the
        // per-window automatic/override sets union so cross-domain
        // re-adoption never loses an explicit user tile win (D3).
        pair.fixed_admission = source.fixed_admission;
        pair.fixed_predicate = source.fixed_predicate;
        pair.maximized_focus_fence = source.maximized_focus_fence;
        pair.fixed_send_admission = source.fixed_send_admission;
        pair.automatic_fixed = source.automatic_fixed.clone();
        pair.fixed_tile_override = source.fixed_tile_override.clone();
        pair.anchor_fallback = source.anchor_fallback;
        pair.no_space = source.no_space;
        pair.capacity_floats = source.capacity_floats.clone();
        pair.admission_order = source.admission_order.clone();
        pair.admission_counter = source
            .admission_counter
            .max(target.map(|t| t.admission_counter).unwrap_or(0));
        pair.overlap_accepted = source.overlap_accepted.clone();
        if let Some(target) = target {
            pair.automatic_fixed
                .extend(target.automatic_fixed.iter().cloned());
            pair.fixed_tile_override
                .extend(target.fixed_tile_override.iter().cloned());
            pair.capacity_floats
                .extend(target.capacity_floats.iter().cloned());
            for (id, seq) in &target.admission_order {
                if !pair.admission_order.contains_key(id) {
                    pair.admission_order.insert(id.clone(), *seq);
                }
            }
            pair.overlap_accepted
                .extend(target.overlap_accepted.iter().cloned());
        }
        // D23 minimize slots ride the pair verbatim (source slots keep
        // their node ids, target slots were remapped above). Stale slots
        // for now-present windows are dropped so restores stay exact.
        pair.minimized_slots = pair_minimized;
        pair.minimized_slots
            .retain(|id, _| !pair.windows.contains_key(id) && !pair.exceptions.contains_key(id));
        pair.canonical_pair_target_restore = target_restore;
        if !pair.validate_current_topology() {
            return Err(CanonicalPairError::DomainMismatch);
        }
        Ok(pair)
    }

    /// Split a committed two-domain pair back into canonical single-domain
    /// Sessions without reseeding.
    ///
    /// Requires exactly two domains and no pending/drag/divergence. Each
    /// output keeps its pair domain bounds/gap with adjacency stripped, its
    /// exact tree/shares, homed `WindowLink`s, homed exceptions (including
    /// `floating_geometry`), partitioned focus stack/last-active, and
    /// partitioned retained float geometry. Both outputs inherit the pair's
    /// current owner/generation/accepted revision/fingerprint (pair commits
    /// advance both together). Focus for a domain is the pair global focus
    /// when homed there, else that domain's focus-stack top when it resolves,
    /// else `None`. Orphan retained entries (no known window) stay with the
    /// source output so the union round-trips. The second output is `None`
    /// when its domain holds no tree, no tiled windows, and no exceptions.
    pub fn split_canonical_pair(&self) -> Result<(Session, Option<Session>), CanonicalPairError> {
        if self.domains.len() != 2 {
            return Err(CanonicalPairError::DomainMismatch);
        }
        if self.divergence().is_some()
            || self.has_pending()
            || self.has_pending_desired()
            || self.has_drag()
            || !self.validate_current_topology()
        {
            return Err(CanonicalPairError::UnusableInput);
        }
        let source_domain = OutputDomain {
            id: self.domains[0].id.clone(),
            workspace: self.domains[0].workspace.clone(),
            bounds: self.domains[0].bounds,
            gap: self.domains[0].gap,
            adjacent: BTreeMap::new(),
        };
        let target_domain = OutputDomain {
            id: self.domains[1].id.clone(),
            workspace: self.domains[1].workspace.clone(),
            bounds: self.domains[1].bounds,
            gap: self.domains[1].gap,
            adjacent: BTreeMap::new(),
        };
        let pair_source_key = self.domains[0].key();
        let pair_target_key = self.domains[1].key();
        let mut source = Session::new(
            self.owner.clone(),
            self.generation.clone(),
            self.accepted_revision(),
            self.accepted_fingerprint(),
            vec![source_domain.clone()],
        )
        .map_err(|_| CanonicalPairError::DomainMismatch)?;
        let mut target = Session::new(
            self.owner.clone(),
            self.generation.clone(),
            self.accepted_revision(),
            self.accepted_fingerprint(),
            vec![target_domain.clone()],
        )
        .map_err(|_| CanonicalPairError::DomainMismatch)?;
        // Split outputs inherit the pair's selected policy; no
        // revision/divergence/pending state is touched.
        source.set_policy(self.policy.clone());
        target.set_policy(self.policy.clone());
        let source_key = source_domain.key();
        let target_key = target_domain.key();
        let mut source_trees: BTreeMap<DomainKey, Option<Node>> = BTreeMap::new();
        source_trees.insert(
            source_key.clone(),
            self.trees.get(&pair_source_key).cloned().flatten(),
        );
        let mut target_trees: BTreeMap<DomainKey, Option<Node>> = BTreeMap::new();
        target_trees.insert(
            target_key.clone(),
            self.trees.get(&pair_target_key).cloned().flatten(),
        );
        let mut source_windows: BTreeMap<WindowId, WindowLink> = BTreeMap::new();
        let mut target_windows: BTreeMap<WindowId, WindowLink> = BTreeMap::new();
        for (id, link) in &self.windows {
            if link.output == pair_source_key.output && link.workspace == pair_source_key.workspace
            {
                source_windows.insert(id.clone(), link.clone());
            } else if link.output == pair_target_key.output
                && link.workspace == pair_target_key.workspace
            {
                target_windows.insert(id.clone(), link.clone());
            } else {
                return Err(CanonicalPairError::DomainMismatch);
            }
        }
        let restored_target_ids: BTreeSet<NodeId> = self
            .canonical_pair_target_restore
            .values()
            .cloned()
            .collect();
        let mut moved_target_remap = BTreeMap::new();
        let mut occupied = self.all_node_ids();
        occupied.extend(restored_target_ids.iter().cloned());
        for link in target_windows.values_mut() {
            if !self.canonical_pair_target_restore.contains_key(&link.leaf)
                && restored_target_ids.contains(&link.leaf)
            {
                let original = link.leaf.clone();
                let mut index = 0u32;
                loop {
                    let candidate = NodeId(format!("pair-moved-{index}"));
                    index += 1;
                    if occupied.insert(candidate.clone()) {
                        moved_target_remap.insert(original.clone(), candidate.clone());
                        link.leaf = candidate;
                        break;
                    }
                }
            }
        }
        for link in target_windows.values_mut() {
            remap_node_id(&mut link.leaf, &moved_target_remap);
            remap_node_id(&mut link.leaf, &self.canonical_pair_target_restore);
        }
        let mut source_exceptions: BTreeMap<WindowId, ExceptionRecord> = BTreeMap::new();
        let mut target_exceptions: BTreeMap<WindowId, ExceptionRecord> = BTreeMap::new();
        for (id, record) in &self.exceptions {
            if record.output == pair_source_key.output
                && record.workspace == pair_source_key.workspace
            {
                source_exceptions.insert(id.clone(), record.clone());
            } else if record.output == pair_target_key.output
                && record.workspace == pair_target_key.workspace
            {
                target_exceptions.insert(id.clone(), record.clone());
            } else {
                return Err(CanonicalPairError::DomainMismatch);
            }
        }
        let mut source_retained: BTreeMap<WindowId, Rect> = BTreeMap::new();
        let mut target_retained: BTreeMap<WindowId, Rect> = BTreeMap::new();
        for (id, rect) in &self.retained_float_geometry {
            if source_windows.contains_key(id) || source_exceptions.contains_key(id) {
                source_retained.insert(id.clone(), *rect);
            } else if target_windows.contains_key(id) || target_exceptions.contains_key(id) {
                target_retained.insert(id.clone(), *rect);
            } else {
                source_retained.insert(id.clone(), *rect);
            }
        }
        source.trees = source_trees;
        source.windows = source_windows;
        source.exceptions = source_exceptions;
        source.retained_float_geometry = source_retained;
        // D23 minimize slots partition by homing like exceptions; orphans
        // stay with the source so the union round-trips. Target slots ride
        // the same target node remaps as trees and links.
        let mut source_minimized: BTreeMap<WindowId, MinimizedSlot> = BTreeMap::new();
        let mut target_minimized: BTreeMap<WindowId, MinimizedSlot> = BTreeMap::new();
        for (id, slot) in &self.minimized_slots {
            if slot.output == pair_source_key.output && slot.workspace == pair_source_key.workspace
            {
                source_minimized.insert(id.clone(), slot.clone());
            } else if slot.output == pair_target_key.output
                && slot.workspace == pair_target_key.workspace
            {
                let mut slot = slot.clone();
                remap_node_id(&mut slot.leaf, &moved_target_remap);
                remap_node_id(&mut slot.leaf, &self.canonical_pair_target_restore);
                remap_tree_node_ids(&mut slot.tree, &moved_target_remap);
                remap_tree_node_ids(&mut slot.tree, &self.canonical_pair_target_restore);
                target_minimized.insert(id.clone(), slot);
            } else {
                source_minimized.insert(id.clone(), slot.clone());
            }
        }
        source.minimized_slots = source_minimized;
        target.minimized_slots = target_minimized;
        target.trees = target_trees;
        target.windows = target_windows;
        target.exceptions = target_exceptions;
        target.retained_float_geometry = target_retained;
        // Fixed-size admission partitions with membership: homed windows
        // keep their automatic/override marks, orphans stay with the
        // source so the union round-trips. The opt-in rides both outputs.
        source.fixed_admission = self.fixed_admission;
        target.fixed_admission = self.fixed_admission;
        source.fixed_predicate = self.fixed_predicate;
        target.fixed_predicate = self.fixed_predicate;
        source.maximized_focus_fence = self.maximized_focus_fence;
        target.maximized_focus_fence = self.maximized_focus_fence;
        source.fixed_send_admission = self.fixed_send_admission;
        target.fixed_send_admission = self.fixed_send_admission;
        for id in &self.automatic_fixed {
            if source.windows.contains_key(id) || source.exceptions.contains_key(id) {
                source.automatic_fixed.insert(id.clone());
            } else if target.windows.contains_key(id) || target.exceptions.contains_key(id) {
                target.automatic_fixed.insert(id.clone());
            } else {
                source.automatic_fixed.insert(id.clone());
            }
        }
        for id in &self.fixed_tile_override {
            if source.windows.contains_key(id) || source.exceptions.contains_key(id) {
                source.fixed_tile_override.insert(id.clone());
            } else if target.windows.contains_key(id) || target.exceptions.contains_key(id) {
                target.fixed_tile_override.insert(id.clone());
            } else {
                source.fixed_tile_override.insert(id.clone());
            }
        }
        // Capacity policy rides both outputs; per-window capacity origin
        // and admission order partition with membership like the fixed
        // marks (orphans stay with the source so the union round-trips).
        // Overlap acceptance partitions by domain homing.
        source.anchor_fallback = self.anchor_fallback;
        target.anchor_fallback = self.anchor_fallback;
        source.no_space = self.no_space;
        target.no_space = self.no_space;
        source.admission_counter = self.admission_counter;
        target.admission_counter = self.admission_counter;
        for id in &self.capacity_floats {
            if source.windows.contains_key(id) || source.exceptions.contains_key(id) {
                source.capacity_floats.insert(id.clone());
            } else if target.windows.contains_key(id) || target.exceptions.contains_key(id) {
                target.capacity_floats.insert(id.clone());
            } else {
                source.capacity_floats.insert(id.clone());
            }
        }
        for (id, seq) in &self.admission_order {
            if source.windows.contains_key(id)
                || source.exceptions.contains_key(id)
                || source.minimized_slots.contains_key(id)
            {
                source.admission_order.insert(id.clone(), *seq);
            } else if target.windows.contains_key(id)
                || target.exceptions.contains_key(id)
                || target.minimized_slots.contains_key(id)
            {
                target.admission_order.insert(id.clone(), *seq);
            } else {
                source.admission_order.insert(id.clone(), *seq);
            }
        }
        for key in &self.overlap_accepted {
            if key == &source_key {
                source.overlap_accepted.insert(key.clone());
            } else if key == &target_key {
                target.overlap_accepted.insert(key.clone());
            }
        }
        remap_optional_tree(
            target.trees.get_mut(&target_key).expect("target tree"),
            &moved_target_remap,
        );
        remap_optional_tree(
            target.trees.get_mut(&target_key).expect("target tree"),
            &self.canonical_pair_target_restore,
        );
        if let Some(leaves) = self.focus_stack.get(&pair_source_key) {
            source
                .focus_stack
                .insert(source_key.clone(), leaves.clone());
        }
        if let Some(leaf) = self.last_active.get(&pair_source_key) {
            source.last_active.insert(source_key.clone(), leaf.clone());
        }
        if let Some(leaves) = self.focus_stack.get(&pair_target_key) {
            let mut leaves = leaves.clone();
            remap_node_ids(&mut leaves, &moved_target_remap);
            remap_node_ids(&mut leaves, &self.canonical_pair_target_restore);
            target.focus_stack.insert(target_key.clone(), leaves);
        }
        if let Some(leaf) = self.last_active.get(&pair_target_key) {
            let mut leaf = leaf.clone();
            remap_node_id(&mut leaf, &moved_target_remap);
            remap_node_id(&mut leaf, &self.canonical_pair_target_restore);
            target.last_active.insert(target_key.clone(), leaf);
        }
        let (source_focus, target_focus) =
            self.split_pair_focus(&pair_source_key, &pair_target_key);
        if let Some(leaf) = source_focus {
            source.focused_domain = Some(source_key.clone());
            source.focused_leaf = Some(leaf);
        }
        if let Some(leaf) = target_focus {
            target.focused_domain = Some(target_key.clone());
            let mut leaf = leaf;
            remap_node_id(&mut leaf, &moved_target_remap);
            remap_node_id(&mut leaf, &self.canonical_pair_target_restore);
            target.focused_leaf = Some(leaf);
        }
        if !source.validate_current_topology() || !target.validate_current_topology() {
            return Err(CanonicalPairError::DomainMismatch);
        }
        let target_empty = target.trees.get(&target_key).cloned().flatten().is_none()
            && target.windows.is_empty()
            && target.exceptions.is_empty()
            && target.minimized_slots.is_empty();
        if target_empty {
            Ok((source, None))
        } else {
            Ok((source, Some(target)))
        }
    }

    /// Pair-global focus resolved per output domain for
    /// [`Session::split_canonical_pair`]: the global leaf when homed on that
    /// domain, else that domain's focus-stack top when it still resolves.
    fn split_pair_focus(
        &self,
        pair_source_key: &DomainKey,
        pair_target_key: &DomainKey,
    ) -> (Option<NodeId>, Option<NodeId>) {
        let source_focus = if self.focused_domain.as_ref() == Some(pair_source_key) {
            self.focused_leaf.clone()
        } else {
            self.focus_stack
                .get(pair_source_key)
                .and_then(|leaves| leaves.last().cloned())
                .filter(|leaf| {
                    self.focus_resolves(
                        &Some(pair_source_key.clone()),
                        &Some(leaf.clone()),
                        &self.trees,
                        &self.windows,
                    )
                })
        };
        let target_focus = if self.focused_domain.as_ref() == Some(pair_target_key) {
            self.focused_leaf.clone()
        } else {
            self.focus_stack
                .get(pair_target_key)
                .and_then(|leaves| leaves.last().cloned())
                .filter(|leaf| {
                    self.focus_resolves(
                        &Some(pair_target_key.clone()),
                        &Some(leaf.clone()),
                        &self.trees,
                        &self.windows,
                    )
                })
        };
        (source_focus, target_focus)
    }

    /// Deferred-transfer rest matching: like [`Session::observed_known_match`]
    /// but exempting exactly the held mover `except`, whose native
    /// send-route flags bind through [`Session::deferred_send_eligible`]
    /// instead. Every other tiled link and exception binds exactly, and
    /// any extra observed identity refuses by count, so this also proves
    /// the completeness the ordinary transfer checks separately (the
    /// transfer adds/removes no window identity).
    fn observed_rest_match(&self, observed: &[ObservedWindow], except: &WindowId) -> bool {
        let by_id: BTreeMap<&WindowId, &ObservedWindow> =
            observed.iter().map(|w| (&w.window, w)).collect();
        for (id, link) in &self.windows {
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != link.output || entry.workspace != link.workspace {
                return false;
            }
            if self.fixed_admission {
                if entry.floating {
                    return false;
                }
            } else if entry.flags().any() {
                return false;
            }
        }
        for (id, record) in &self.exceptions {
            if id == except {
                continue;
            }
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != record.output || entry.workspace != record.workspace {
                return false;
            }
            if self.fixed_admission {
                if entry.floating != record.flags.floating {
                    return false;
                }
            } else if entry.flags() != record.flags {
                return false;
            }
        }
        // No extra observed identity beyond the known set (duplicate
        // window ids collapse in the map above, so count exactly).
        observed.len() == self.windows.len() + self.exceptions.len()
    }

    fn observed_known_match(
        &self,
        observed: &[ObservedWindow],
        admit_new: Option<&WindowId>,
    ) -> bool {
        let by_id: BTreeMap<&WindowId, &ObservedWindow> =
            observed.iter().map(|w| (&w.window, w)).collect();
        // Membership matching binds homing plus exception flags exactly
        // (original semantics, always used when the fixed-size opt-in is
        // off). With the opt-in on, fullscreen/maximized/sticky ride as
        // advisory overlays for the Q2 classifier: only the portable
        // `floating` flag binds, so overlay observations never fail an
        // opted-in binding that the legacy path refuses.
        for (id, link) in &self.windows {
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != link.output || entry.workspace != link.workspace {
                return false;
            }
            if self.fixed_admission {
                if entry.floating {
                    return false;
                }
            } else if entry.flags().any() {
                return false;
            }
        }
        for (id, record) in &self.exceptions {
            if Some(id) == admit_new {
                continue;
            }
            let Some(entry) = by_id.get(id) else {
                return false;
            };
            if entry.output != record.output || entry.workspace != record.workspace {
                return false;
            }
            // Diagnostic capacity origin grants no binding leniency (D4):
            // capacity floats bind exactly like ordinary floats. A stale
            // floating:false echo fails the binding here (PartialObservation);
            // the converged observation path owns the re-admission attempt.
            if self.fixed_admission {
                if entry.floating != record.flags.floating {
                    return false;
                }
            } else if entry.flags() != record.flags {
                return false;
            }
        }
        true
    }

    /// Record an explicit adapter acknowledgement (shared binding for movement,
    /// lifecycle, focus, resize, and drag pending plans). Terminal divergence
    /// clears the pending desired state, matching [`Session::note_adapter_loss`].
    pub fn acknowledge(
        &mut self,
        ack: &crate::contract::AdapterAck,
    ) -> Result<AckApplied, AckError> {
        match self.reconciler.acknowledge(ack) {
            Ok(applied) => Ok(applied),
            Err(AckError::NoPending) => Err(AckError::NoPending),
            Err(AckError::Diverged(reason)) => {
                self.pending_desired = None;
                self.drag = None;
                Err(AckError::Diverged(reason))
            }
        }
    }

    /// Withdraw an unacknowledged pending plan without recording divergence.
    /// Exact guards: no recorded divergence, no transient drag capture, an
    /// unacknowledged reconciler pending bound to the exact
    /// correlation/base-revision pair. On success the reconciler pending slot
    /// and the staged desired topology/focus/exceptions clear together with no
    /// revision advancement; committed trees, windows, focus, exceptions,
    /// retained float geometry, and accepted revision/fingerprint are exactly
    /// preserved. Any failure leaves state exactly untouched and records
    /// nothing. Owner/generation binding is enforced by the caller, which
    /// retains the authoritative copies.
    pub fn cancel_unacked_pending(
        &mut self,
        correlation_id: &CorrelationId,
        base_revision: u64,
    ) -> Result<(), crate::reconcile::CancelUnackedError> {
        if self.drag.is_some() {
            // Unreachable through the public API (`begin_drag` refuses while
            // pending and every propose path refuses while dragging); the
            // planner additionally refuses cancelled drags before calling.
            // Fail closed without divergence if ever reached.
            return Err(crate::reconcile::CancelUnackedError::DragActive);
        }
        match self
            .reconciler
            .cancel_unacked(correlation_id, base_revision)
        {
            Ok(()) => {
                self.pending_desired = None;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Commit a pending lifecycle plan after acknowledgement. On commit the
    /// pending desired topology/focus/exceptions apply atomically and the
    /// accepted revision advances by exactly one. Terminal divergence clears
    /// the pending desired state, matching [`Session::note_adapter_loss`].
    pub fn verify_lifecycle(
        &mut self,
        post: &LifecyclePostObservation,
    ) -> Result<Commit, VerifyError> {
        match self.reconciler.verify_lifecycle(post) {
            Ok(commit) => {
                if let Some(desired) = self.pending_desired.take() {
                    self.trees = desired.trees;
                    self.windows = desired.windows;
                    self.focused_domain = desired.focused_domain;
                    self.focused_leaf = desired.focused_leaf;
                    self.focus_stack = self.updated_focus_stack(
                        &self.focused_domain,
                        &self.focused_leaf,
                        &self.trees,
                        &self.windows,
                    );
                    self.last_active = desired.last_active;
                    self.exceptions = desired.exceptions;
                    self.retained_float_geometry = desired.retained_float_geometry;
                    self.automatic_fixed = desired.automatic_fixed;
                    self.fixed_tile_override = desired.fixed_tile_override;
                    self.capacity_floats = desired.capacity_floats;
                    self.admission_order = desired.admission_order;
                    self.admission_counter = desired.admission_counter;
                    self.overlap_accepted = desired.overlap_accepted;
                    // A removed window drops its fixed-size bookkeeping so
                    // a reused id classifies again (D3).
                    self.prune_fixed_size_state();
                    self.prune_capacity_state();
                    self.accepted_fingerprint = commit.fingerprint;
                }
                Ok(commit)
            }
            Err(VerifyError::Diverged(reason)) => {
                self.pending_desired = None;
                self.drag = None;
                Err(VerifyError::Diverged(reason))
            }
            Err(other) => Err(other),
        }
    }

    /// Explicit adapter-loss signal: terminal divergence, pending discarded.
    pub fn note_adapter_loss(&mut self) -> DivergenceKind {
        self.pending_desired = None;
        self.drag = None;
        self.reconciler.note_adapter_loss()
    }

    /// Explicit post-observation mismatch signal for the standalone
    /// workspace-send route: terminal `PostconditionMismatch` divergence,
    /// pending desired state discarded.
    pub fn note_postcondition_mismatch(&mut self) -> DivergenceKind {
        self.pending_desired = None;
        self.drag = None;
        self.reconciler.note_postcondition_mismatch()
    }
}

fn opposite_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

/// Canonical component usability for pair assembly: no divergence,
/// pending, or drag plus a valid current topology.
fn canonical_component_usable(session: &Session) -> bool {
    session.divergence().is_none()
        && !session.has_pending()
        && !session.has_pending_desired()
        && !session.has_drag()
        && session.validate_current_topology()
}

/// Classify a canonical component failure without echoing input.
fn canonical_component_error(session: &Session) -> CanonicalPairError {
    if session.domains.len() != 1 {
        return CanonicalPairError::DomainMismatch;
    }
    if session.divergence().is_some()
        || session.has_pending()
        || session.has_pending_desired()
        || session.has_drag()
        || !session.validate_current_topology()
    {
        return CanonicalPairError::UnusableInput;
    }
    CanonicalPairError::DomainMismatch
}

/// Re-key one per-domain map entry from a component key to its pair key.
fn remap_domain_map<V>(map: &mut BTreeMap<DomainKey, V>, from: &DomainKey, to: &DomainKey) {
    if from == to {
        return;
    }
    if let Some(value) = map.remove(from) {
        map.insert(to.clone(), value);
    }
}

fn pair_target_node_remap(source: &Session, target: &Session) -> BTreeMap<NodeId, NodeId> {
    let mut occupied = source.all_node_ids();
    let mut mapping = BTreeMap::new();
    for id in target.all_node_ids() {
        let mut index = 0u32;
        loop {
            let candidate = NodeId(format!("pair-target-{index}"));
            index += 1;
            if occupied.insert(candidate.clone()) {
                mapping.insert(id.clone(), candidate);
                break;
            }
        }
    }
    mapping
}

fn remap_node_id(id: &mut NodeId, mapping: &BTreeMap<NodeId, NodeId>) {
    if let Some(mapped) = mapping.get(id) {
        *id = mapped.clone();
    }
}

fn remap_node_ids(ids: &mut [NodeId], mapping: &BTreeMap<NodeId, NodeId>) {
    for id in ids {
        remap_node_id(id, mapping);
    }
}

fn remap_optional_tree(tree: &mut Option<Node>, mapping: &BTreeMap<NodeId, NodeId>) {
    if let Some(tree) = tree {
        remap_tree_node_ids(tree, mapping);
    }
}

fn remap_tree_node_ids(node: &mut Node, mapping: &BTreeMap<NodeId, NodeId>) {
    match node {
        Node::Leaf { id } => remap_node_id(id, mapping),
        Node::Group { id, children, .. } => {
            remap_node_id(id, mapping);
            for child in children {
                remap_tree_node_ids(child, mapping);
            }
        }
    }
}

/// Strict adjacency validation: every target is known, non-self, same
/// workspace (resolved as `(target_output, source_workspace)`), and strictly
/// reciprocal via the opposite direction.
fn validate_adjacency(domains: &[OutputDomain]) -> bool {
    use std::collections::BTreeMap;
    let mut by_output: BTreeMap<OutputId, Vec<&OutputDomain>> = BTreeMap::new();
    for domain in domains {
        by_output.entry(domain.id.clone()).or_default().push(domain);
    }
    for domain in domains {
        for (direction, target) in &domain.adjacent {
            if target.0.is_empty() || target == &domain.id {
                return false;
            }
            // Cross-workspace allowed: exactly one domain must own the target
            // output id (ambiguity fails closed). The target workspace is that
            // domain's currently selected logical workspace, not the source
            // workspace.
            let Some(candidates) = by_output.get(target) else {
                return false;
            };
            if candidates.len() != 1 {
                return false;
            }
            let target_domain = candidates[0];
            let opposite = opposite_direction(*direction);
            match target_domain.adjacent.get(&opposite) {
                Some(back) if back == &domain.id => {}
                _ => return false,
            }
        }
    }
    true
}

fn subtree_contains_id(node: &Node, id: &NodeId) -> bool {
    if node.id() == id {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_contains_id(c, id)),
    }
}

fn insert_leaf_into_group(
    policy: &dyn LayoutPolicy,
    tree: Node,
    group_id: &NodeId,
    index: usize,
    leaf: Node,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares,
        } => {
            if &id == group_id {
                if index > children.len() {
                    return None;
                }
                let new_shares = policy.proportional_insertion_shares(&shares, index)?;
                children.insert(index, leaf);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares: new_shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated = insert_leaf_into_group(
                        policy,
                        child.clone(),
                        group_id,
                        index,
                        leaf.clone(),
                    )?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares,
                    });
                }
            }
            None
        }
    }
}

fn find_group(tree: &Node, id: &NodeId) -> Option<(Vec<Node>, Axis, NodeId)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            axis,
            children,
            ..
        } => {
            if gid == id {
                return Some((children.clone(), *axis, gid.clone()));
            }
            for child in children {
                if let Some(found) = find_group(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

fn find_group_shares(tree: &Node, id: &NodeId) -> Option<Vec<u64>> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            children,
            shares,
            ..
        } => {
            if gid == id {
                return Some(shares.clone());
            }
            for child in children {
                if let Some(found) = find_group_shares(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

fn find_group_full(tree: &Node, id: &NodeId) -> Option<(Vec<Node>, Vec<u64>, NodeId, Axis)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            axis,
            children,
            shares,
        } => {
            if gid == id {
                return Some((children.clone(), shares.clone(), gid.clone(), *axis));
            }
            for child in children {
                if let Some(found) = find_group_full(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// Direct parent group id of a leaf, if the leaf is a direct child.
fn direct_parent_of_leaf(tree: &Node, leaf: &NodeId) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            for child in children {
                if child.id() == leaf && matches!(child, Node::Leaf { .. }) {
                    return Some(id.clone());
                }
            }
            for child in children {
                if let Some(found) = direct_parent_of_leaf(child, leaf) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// Direct parent group id of a group, if it is a direct child.
fn parent_of_group(tree: &Node, group: &NodeId) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            for child in children {
                if child.id() == group {
                    return Some(id.clone());
                }
            }
            for child in children {
                if let Some(found) = parent_of_group(child, group) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// Immediate parent group of a leaf: group id, axis, child index of the
/// direct slot containing the leaf, and group shares. `None` when the leaf
/// is the root or absent.
fn find_parent_group(tree: &Node, leaf: &NodeId) -> Option<(NodeId, Axis, usize, Vec<u64>)> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if subtree_contains(child, leaf) {
                    if matches!(child, Node::Leaf { .. }) {
                        return Some((id.clone(), *axis, index, shares.clone()));
                    }
                    return find_parent_group(child, leaf);
                }
            }
            None
        }
    }
}

/// Exact integer greatest common divisor for share normalization
/// (Euclidean; mirrors [`crate::cosmic_v1`] share hygiene).
fn gcd_pair(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Divide every share by the exact common divisor when above one (always
/// exact by GCD construction; ratios and fractions are preserved).
fn normalize_shares(shares: &mut [u64]) {
    let mut divisor = 0u64;
    for share in shares.iter() {
        divisor = gcd_pair(divisor, *share);
        if divisor == 1 {
            break;
        }
    }
    if divisor > 1 {
        for share in shares.iter_mut() {
            *share /= divisor;
        }
    }
}

/// Exact integer greatest common divisor over wide share products
/// (Euclidean; the scaled restore products below can exceed `u64` before
/// reduction, so the narrow [`gcd_pair`] cannot serve them).
fn gcd_wide(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// Full inserted share vector preserving the stored old fraction
/// `old_share / old_total` against current survivor `shares`, entrant at
/// `index`: the entrant takes `old_share * current_total` while each
/// survivor scales by the stored remainder, GCD-reduced. The current
/// weights rescale up to the entrant instead of rounding the entrant down
/// to the current unit, so a skewed old fraction stays exact (for example
/// old 1/10 against current `[1, 1]` rejoins `[2, 9, 9]`, never `[1, 1,
/// 1]`), and survivor ratios are preserved exactly. `None` on
/// empty/zero input, `u64` overflow of any entry, or `u64` overflow of the
/// combined total (group totals must not overflow; callers fall back to
/// fresh admission).
fn comparable_inserted_shares(
    old_share: u64,
    old_total: u128,
    current: &[u64],
    index: usize,
) -> Option<Vec<u64>> {
    if old_share == 0
        || old_total <= old_share as u128
        || current.is_empty()
        || current.contains(&0)
    {
        return None;
    }
    if index > current.len() {
        return None;
    }
    let rest = old_total - old_share as u128;
    let total: u128 = current.iter().map(|share| *share as u128).sum();
    if total == 0 {
        return None;
    }
    let mut scaled: Vec<u128> = Vec::with_capacity(current.len() + 1);
    scaled.push((old_share as u128) * total);
    for share in current {
        scaled.push((*share as u128) * rest);
    }
    let mut divisor: u128 = 0;
    for value in &scaled {
        divisor = gcd_wide(divisor, *value);
        if divisor == 1 {
            break;
        }
    }
    if divisor == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(scaled.len());
    for value in scaled {
        out.push(u64::try_from(value / divisor).ok()?);
    }
    let entrant = out.remove(0);
    out.insert(index, entrant);
    // Group totals must not overflow even when every entry fits: a
    // representable-components/nonrepresentable-total vector falls back
    // instead of producing an invalid tree.
    let _: u64 = out
        .iter()
        .try_fold(0u64, |total, share| total.checked_add(*share))?;
    Some(out)
}

/// Insert a leaf into the named group at `index` carrying caller-computed
/// `shares` (length must equal children plus one, all positive).
/// Structural twin of [`insert_leaf_into_group`] for rejoins whose entrant
/// share preserves the stored old fraction instead of the proportional
/// average.
fn insert_leaf_with_shares(
    tree: Node,
    group_id: &NodeId,
    index: usize,
    leaf: Node,
    shares: Vec<u64>,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares: current,
        } => {
            if &id == group_id {
                if index > children.len()
                    || shares.len() != children.len() + 1
                    || shares.contains(&0)
                {
                    return None;
                }
                children.insert(index, leaf);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated = insert_leaf_with_shares(
                        child.clone(),
                        group_id,
                        index,
                        leaf.clone(),
                        shares.clone(),
                    )?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares: current,
                    });
                }
            }
            None
        }
    }
}

/// Replace a leaf with a new subtree, preserving the leaf's slot share in
/// its parent (the parent keeps its children count and shares; the new
/// subtree carries its own internal shares).
fn replace_leaf(tree: Node, leaf: &NodeId, node: Node) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            shares,
        } => {
            if let Some(pos) = children
                .iter()
                .position(|child| matches!(child, Node::Leaf { id: leaf_id } if leaf_id == leaf))
            {
                children[pos] = node;
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if subtree_contains(child, leaf) {
                    let updated = replace_leaf(child.clone(), leaf, node.clone())?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares,
                    });
                }
            }
            None
        }
    }
}

/// D23 best-effort minimize rejoin (REQ-MNZ-02 option (b)): return the
/// current domain tree with the stored leaf rejoined in its old
/// neighborhood, or `None` for fresh ordinary admission.
///
/// Stored context only (the pre-minimize tree plus the removed leaf; no
/// tree-identity prerequisite): the anchor is the live surviving former
/// leaf nearest the stored leaf in old leaf order, preferring
/// same-old-parent siblings. A survivor is live only while its leaf id is
/// currently linked to a tiled window homed on the slot domain, so stale
/// or reused ids never anchor. The rejoin preserves the old side
/// (before/after the anchor). A surviving old group (same id and axis,
/// still holding a live survivor) takes an insert at the clamped old index
/// with an entrant share preserving the stored old fraction against the
/// current total; otherwise the anchor leaf regroups into a new group on
/// the stored axis and side, splitting the stored old share against the
/// anchor's old share (same-parent anchor) or the stored remainder.
/// Resizes, admissions, sibling closes with a survivor, swaps, other
/// minimize/restore roundtrips, and gaps/resolution changes therefore do
/// not alone invalidate: only an absent live-neighbor set (or an unusable
/// leaf id) returns `None`. Callers gate homing/domain liveness and the
/// exact fast path before this.
fn best_effort_rejoin(
    slot: &MinimizedSlot,
    current: &Option<Node>,
    windows: &BTreeMap<WindowId, WindowLink>,
    existing_ids: &mut BTreeSet<NodeId>,
    window: &WindowId,
    base_revision: u64,
) -> Option<Node> {
    if existing_ids.contains(&slot.leaf) {
        return None;
    }
    let current = current.clone()?;
    let old_order = collect_leaves(&slot.tree);
    let restored_pos = old_order.iter().position(|leaf| leaf == &slot.leaf)?;
    let (old_group, old_axis, old_index, old_shares) = find_parent_group(&slot.tree, &slot.leaf)?;
    let old_share = *old_shares.get(old_index)?;
    // Wide sum: share inputs are `u64` and their total can overflow it.
    let old_total: u128 = old_shares.iter().map(|share| *share as u128).sum();
    if old_total <= old_share as u128 {
        return None;
    }
    // A live survivor is a former leaf (never the restored leaf itself)
    // currently linked to a tiled window homed on the slot domain.
    let live = |leaf: &NodeId| {
        leaf != &slot.leaf
            && windows.values().any(|link| {
                &link.leaf == leaf && link.output == slot.output && link.workspace == slot.workspace
            })
    };
    let mut survivors: Vec<(usize, NodeId)> = old_order
        .iter()
        .enumerate()
        .filter(|(_, leaf)| live(leaf))
        .map(|(pos, leaf)| (pos, leaf.clone()))
        .collect();
    if survivors.is_empty() {
        return None;
    }
    // Surviving old group (same id and axis, still holding a live survivor;
    // the id check alone could match a reused generated id): insert at the
    // clamped old index with a comparable entrant share.
    if let Some((children, shares, _, axis)) = find_group_full(&current, &old_group)
        && axis == old_axis
        && children
            .iter()
            .flat_map(|child| {
                let mut leaves = Vec::new();
                collect_leaves_into(child, &mut leaves);
                leaves
            })
            .any(|leaf| live(&leaf))
    {
        let index = old_index.min(children.len());
        let rejoined = comparable_inserted_shares(old_share, old_total, &shares, index)?;
        let rejoined = insert_leaf_with_shares(
            current,
            &old_group,
            index,
            Node::Leaf {
                id: slot.leaf.clone(),
            },
            rejoined,
        )?;
        existing_ids.insert(slot.leaf.clone());
        return Some(rejoined);
    }
    // Same-old-parent siblings first (sibling regroup), else the nearest
    // live surviving former leaf in old leaf order (deterministic tie-break).
    let same_parent: Vec<(usize, NodeId)> = survivors
        .iter()
        .filter(|(_, leaf)| {
            find_parent_group(&slot.tree, leaf).is_some_and(|(group, _, _, _)| group == old_group)
        })
        .cloned()
        .collect();
    if !same_parent.is_empty() {
        survivors = same_parent;
    }
    survivors.sort_by_key(|(pos, _)| (pos.abs_diff(restored_pos), *pos));
    let (anchor_pos, anchor) = survivors.into_iter().next().expect("nonempty survivors");
    let before = restored_pos < anchor_pos;
    // Anchor-side share: the anchor's own old share for a same-parent
    // anchor, else the stored remainder as the faithful proxy. The
    // remainder is wide arithmetic; an unrepresentable proxy falls back to
    // fresh admission rather than truncating the fraction.
    let anchor_share = find_parent_group(&slot.tree, &anchor)
        .filter(|(group, _, _, _)| group == &old_group)
        .and_then(|(_, _, index, shares)| shares.get(index).copied())
        .or_else(|| u64::try_from(old_total - old_share as u128).ok())?;
    let mut regroup = if before {
        vec![old_share, anchor_share]
    } else {
        vec![anchor_share, old_share]
    };
    normalize_shares(&mut regroup);
    let group_id = generate_group_id(window, base_revision, existing_ids);
    existing_ids.insert(group_id.clone());
    existing_ids.insert(slot.leaf.clone());
    let children = if before {
        vec![
            Node::Leaf {
                id: slot.leaf.clone(),
            },
            Node::Leaf { id: anchor.clone() },
        ]
    } else {
        vec![
            Node::Leaf { id: anchor.clone() },
            Node::Leaf {
                id: slot.leaf.clone(),
            },
        ]
    };
    let regroup = Node::Group {
        id: group_id,
        axis: old_axis,
        children,
        shares: regroup,
    };
    if current == (Node::Leaf { id: anchor.clone() }) {
        return Some(regroup);
    }
    replace_leaf(current, &anchor, regroup)
}

fn find_subtree<'a>(tree: &'a Node, id: &NodeId) -> Option<&'a Node> {
    if tree.id() == id {
        return Some(tree);
    }
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => {
            for child in children {
                if let Some(found) = find_subtree(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

fn replace_group_children(
    tree: Node,
    group_id: &NodeId,
    children: Vec<Node>,
    shares: Vec<u64>,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children: old_children,
            shares: old_shares,
        } => {
            if &id == group_id {
                if children.len() < 2 || shares.len() != children.len() {
                    return None;
                }
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            let mut new_children = old_children;
            for child in new_children.iter_mut() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated = replace_group_children(
                        child.clone(),
                        group_id,
                        children.clone(),
                        shares.clone(),
                    )?;
                    *child = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children: new_children,
                        shares: old_shares,
                    });
                }
            }
            None
        }
    }
}

fn replace_child_at(
    tree: &Node,
    group_id: &NodeId,
    index: usize,
    replacement: Node,
    replacement_share: u64,
) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            if id == group_id {
                if index >= children.len() {
                    return None;
                }
                let mut new_children = children.clone();
                let mut new_shares = shares.clone();
                new_children[index] = replacement;
                new_shares[index] = replacement_share;
                return Some(Node::Group {
                    id: id.clone(),
                    axis: *axis,
                    children: new_children,
                    shares: new_shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == group_id || subtree_contains_id(child, group_id) {
                    let updated = replace_child_at(
                        child,
                        group_id,
                        index,
                        replacement.clone(),
                        replacement_share,
                    )?;
                    let mut new_children = children.clone();
                    new_children[pos] = updated;
                    return Some(Node::Group {
                        id: id.clone(),
                        axis: *axis,
                        children: new_children,
                        shares: shares.clone(),
                    });
                }
            }
            None
        }
    }
}

fn swap_direct_children(tree: Node, container: &NodeId, a: &NodeId, b: &NodeId) -> Option<Node> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis,
            mut children,
            mut shares,
        } => {
            if &id == container {
                let ia = children.iter().position(|c| c.id() == a)?;
                let ib = children.iter().position(|c| c.id() == b)?;
                // Both must be direct leaf children.
                if !matches!(children[ia], Node::Leaf { .. })
                    || !matches!(children[ib], Node::Leaf { .. })
                {
                    return None;
                }
                children.swap(ia, ib);
                shares.swap(ia, ib);
                return Some(Node::Group {
                    id,
                    axis,
                    children,
                    shares,
                });
            }
            for (pos, child) in children.iter().enumerate() {
                if child.id() == container || subtree_contains_id(child, container) {
                    let updated = swap_direct_children(child.clone(), container, a, b)?;
                    children[pos] = updated;
                    return Some(Node::Group {
                        id,
                        axis,
                        children,
                        shares,
                    });
                }
            }
            None
        }
    }
}

/// COSMIC window-drop classification inside a projected target rectangle.
/// Test-asserted helper over the selected policy implementation: the middle
/// third (rounded thirds) is the center stack source fact
/// (`Some(DragSide::Center)`), any other location selects an edge by strict
/// normalized half-distance (ties choose vertical), and points outside yield
/// `None`. Production preview/drop resolve through the held
/// [`LayoutPolicy`] instead; all COSMIC zone semantics live in
/// [`crate::cosmic_v1`]. Center never plans (unsupported stack behavior).
#[cfg(test)]
fn drag_edge_for(rect: &Rect, x: i32, y: i32) -> Option<DragSide> {
    CosmicV1Policy.classify_window_point(rect, x, y)
}

#[allow(dead_code)]
fn project_affected_geometry(
    domains: &[OutputDomain],
    trees: &BTreeMap<DomainKey, Option<Node>>,
    windows: &BTreeMap<WindowId, WindowLink>,
    affected: &[DomainKey],
    hints: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
) -> Result<Vec<DesiredGeometry>, ()> {
    let mut out = Vec::new();
    for key in affected {
        let Some(domain) = domains.iter().find(|d| &d.key() == key) else {
            return Err(());
        };
        let tree = trees.get(key).cloned().flatten();
        let projected = project_output_geometry(Some(domain), tree.as_ref(), windows, key, hints)
            .map_err(|_| ())?;
        out.extend(projected);
    }
    out.sort_by(|a, b| {
        a.output
            .0
            .cmp(&b.output.0)
            .then(a.workspace.0.cmp(&b.workspace.0))
            .then(a.leaf.0.cmp(&b.leaf.0))
    });
    Ok(out)
}

fn geometry_covers_affected(
    geometry: &[DesiredGeometry],
    windows: &BTreeMap<WindowId, WindowLink>,
    affected: &[DomainKey],
) -> bool {
    use std::collections::BTreeSet;
    for key in affected {
        let expected: BTreeSet<&NodeId> = windows
            .values()
            .filter(|l| l.output == key.output && l.workspace == key.workspace)
            .map(|l| &l.leaf)
            .collect();
        let got: BTreeSet<&NodeId> = geometry
            .iter()
            .filter(|g| g.output == key.output && g.workspace == key.workspace)
            .map(|g| &g.leaf)
            .collect();
        if expected != got {
            return false;
        }
        for entry in geometry
            .iter()
            .filter(|g| g.output == key.output && g.workspace == key.workspace)
        {
            if entry.window.0.is_empty() || entry.leaf.0.is_empty() {
                return false;
            }
            if entry.rect.w <= 0 || entry.rect.h <= 0 {
                return false;
            }
            let Some(link) = windows.get(&entry.window) else {
                return false;
            };
            if link.leaf != entry.leaf
                || link.output != entry.output
                || link.workspace != entry.workspace
            {
                return false;
            }
        }
    }
    // No geometry may leak outside affected domains.
    for entry in geometry {
        if !affected
            .iter()
            .any(|k| k.output == entry.output && k.workspace == entry.workspace)
        {
            return false;
        }
    }
    true
}

fn valid_rect_shape(rect: &Rect) -> bool {
    rect.w > 0
        && rect.h > 0
        && rect.x.checked_add(rect.w).is_some()
        && rect.y.checked_add(rect.h).is_some()
}

fn valid_observed_shapes(observed: &[ObservedWindow]) -> bool {
    let mut seen = BTreeSet::new();
    for entry in observed {
        if entry.window.0.is_empty() || entry.output.0.is_empty() || entry.workspace.0.is_empty() {
            return false;
        }
        if !seen.insert(entry.window.clone()) {
            return false;
        }
    }
    true
}

/// COSMIC admission axis for a target geometry. Test-asserted helper over the
/// selected policy implementation: width strictly greater than height selects
/// portable [`Axis::Horizontal`] (splits width), otherwise [`Axis::Vertical`]
/// (vertical on tie). Production admission resolves through the held
/// [`LayoutPolicy`] instead.
#[cfg(test)]
fn orientation_from_bounds(bounds: &Rect) -> Axis {
    CosmicV1Policy.admission_axis_for_rect(bounds)
}

fn collect_node_ids(node: &Node, out: &mut BTreeSet<NodeId>) {
    out.insert(node.id().clone());
    if let Node::Group { children, .. } = node {
        for child in children {
            collect_node_ids(child, out);
        }
    }
}

fn collect_leaves(node: &Node) -> Vec<NodeId> {
    let mut out = Vec::new();
    collect_leaves_into(node, &mut out);
    out
}

fn collect_leaves_into(node: &Node, out: &mut Vec<NodeId>) {
    match node {
        Node::Leaf { id } => out.push(id.clone()),
        Node::Group { children, .. } => {
            for child in children {
                collect_leaves_into(child, out);
            }
        }
    }
}

fn generate_group_id(
    window: &WindowId,
    base_revision: u64,
    existing: &mut BTreeSet<NodeId>,
) -> NodeId {
    let base = format!("grp-{}-r{}", window.0, base_revision);
    if !existing.contains(&NodeId(base.clone())) {
        return NodeId(base);
    }
    let mut n = 1u32;
    loop {
        let candidate = NodeId(format!("{base}-{n}"));
        if !existing.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn find_node_by_id<'a>(node: &'a Node, id: &NodeId) -> Option<&'a Node> {
    if node.id() == id {
        return Some(node);
    }
    match node {
        Node::Leaf { .. } => None,
        Node::Group { children, .. } => {
            for child in children {
                if let Some(found) = find_node_by_id(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

fn subtree_contains(node: &Node, leaf: &NodeId) -> bool {
    if node.id() == leaf {
        return true;
    }
    match node {
        Node::Leaf { .. } => false,
        Node::Group { children, .. } => children.iter().any(|c| subtree_contains(c, leaf)),
    }
}

fn nest_focused_with_new(
    policy: &dyn LayoutPolicy,
    node: Node,
    focused: &NodeId,
    new_leaf: Node,
    group_id: NodeId,
    axis: Axis,
) -> Option<Node> {
    match node {
        Node::Leaf { id } if &id == focused => Some(Node::Group {
            id: group_id,
            axis,
            children: vec![Node::Leaf { id }, new_leaf],
            shares: policy.new_group_shares().to_vec(),
        }),
        Node::Leaf { .. } => None,
        Node::Group {
            id,
            axis: parent_axis,
            children,
            shares,
        } => {
            for (index, child) in children.iter().enumerate() {
                if subtree_contains(child, focused) {
                    let updated = nest_focused_with_new(
                        policy,
                        child.clone(),
                        focused,
                        new_leaf.clone(),
                        group_id.clone(),
                        axis,
                    )?;
                    let mut new_children = children.clone();
                    new_children[index] = updated;
                    return Some(Node::Group {
                        id,
                        axis: parent_axis,
                        children: new_children,
                        shares: shares.clone(),
                    });
                }
            }
            None
        }
    }
}

/// Admission target for a caller-validated eligible focus.
pub(crate) fn admission_placement_for(
    domain: &OutputDomain,
    tree: Option<&Node>,
    eligible: Option<&NodeId>,
) -> Rect {
    if let (Some(tree), Some(leaf)) = (tree, eligible)
        && let Ok(projected) = crate::geometry::project(tree, domain.bounds, domain.gap)
        && let Some(target) = projected.iter().find(|entry| &entry.leaf == leaf)
    {
        return target.rect;
    }
    domain.bounds
}

#[allow(clippy::too_many_arguments)]
fn insert_tiled(
    policy: &dyn LayoutPolicy,
    tree: Option<Node>,
    focused: Option<&NodeId>,
    new_leaf_id: NodeId,
    orientation: Axis,
    existing_ids: &mut BTreeSet<NodeId>,
    window: &WindowId,
    base_revision: u64,
) -> Option<Node> {
    let new_leaf = Node::Leaf { id: new_leaf_id };
    let Some(tree) = tree else {
        return Some(new_leaf);
    };
    let Some(focused) = focused else {
        // No eligible focus in this domain (deterministic project fallback
        // for focus only, not source COSMIC parity): wrap the entire existing
        // root old/new under a new binary group carrying the admission axis
        // and [`crate::cosmic_v1::new_group_shares`].
        let group_id = generate_group_id(window, base_revision, existing_ids);
        existing_ids.insert(group_id.clone());
        return Some(Node::Group {
            id: group_id,
            axis: orientation,
            children: vec![tree, new_leaf],
            shares: policy.new_group_shares().to_vec(),
        });
    };
    // Automatic admission always wraps the focused target leaf in an ordered
    // binary group old/new (source `TilingLayout::new_group` 2898-2936 via
    // `map_to_tree` 548-617): the wrapper takes the old leaf slot with the
    // admission axis and [`crate::cosmic_v1::new_group_shares`]. Same-axis
    // N-ary parent append never applies here; N-ary groups remain for
    // movement/drag representation only.
    let group_id = generate_group_id(window, base_revision, existing_ids);
    existing_ids.insert(group_id.clone());
    nest_focused_with_new(policy, tree, focused, new_leaf, group_id, orientation)
}

fn remove_leaf_from_tree(
    policy: &dyn LayoutPolicy,
    tree: Option<Node>,
    leaf: &NodeId,
) -> Option<Node> {
    let node = tree?;
    remove_node(policy, node, leaf)
}

fn remove_node(policy: &dyn LayoutPolicy, node: Node, leaf: &NodeId) -> Option<Node> {
    match node {
        Node::Leaf { id } => {
            if &id == leaf {
                None
            } else {
                Some(Node::Leaf { id })
            }
        }
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            let original_len = children.len();
            let original_shares = shares.clone();
            let mut new_children = Vec::with_capacity(children.len());
            let mut removed_index: Option<usize> = None;
            for (index, child) in children.into_iter().enumerate() {
                if removed_index.is_none()
                    && child.id() == leaf
                    && matches!(child, Node::Leaf { .. })
                {
                    removed_index = Some(index);
                    continue;
                }
                match remove_node(policy, child, leaf) {
                    Some(updated) => {
                        // If the child subtree changed shape but kept its id,
                        // keep the slot share; emptied subtrees drop below.
                        // Detect change by comparing ids? The recursive call
                        // returns an updated node only when the leaf was
                        // inside; unchanged subtrees return an equal node.
                        // We track removal only when a slot disappears.
                        new_children.push(updated);
                    }
                    None => {
                        // Subtree emptied: drop this slot entirely.
                        if removed_index.is_none() {
                            removed_index = Some(index);
                        } else {
                            // More than one slot vanished: fail closed.
                            return None;
                        }
                    }
                }
            }
            // No removal in this subtree: return unchanged group.
            let Some(removed) = removed_index else {
                // Rebuild to check: if lengths match, nothing was removed.
                // We consumed children; reconstruct from new_children only when
                // no removal happened (lengths equal). Since we pushed every
                // child, lengths equal means no-op.
                if new_children.len() != original_len {
                    return None;
                }
                return Some(Node::Group {
                    id,
                    axis,
                    children: new_children,
                    shares: original_shares,
                });
            };
            let _ = removed;
            match new_children.len() {
                0 => None,
                1 => Some(new_children.into_iter().next().expect("one child")),
                _ => {
                    let new_shares =
                        policy.proportional_removal_shares(&original_shares, removed)?;
                    // Defensive: helper length must match survivors.
                    if new_shares.len() != new_children.len() {
                        return None;
                    }
                    Some(Node::Group {
                        id,
                        axis,
                        children: new_children,
                        shares: new_shares,
                    })
                }
            }
        }
    }
}

fn validate_topology(
    domains: &[OutputDomain],
    trees: &BTreeMap<DomainKey, Option<Node>>,
    windows: &BTreeMap<WindowId, WindowLink>,
    exceptions: &BTreeMap<WindowId, ExceptionRecord>,
    focused_domain: &Option<DomainKey>,
    focused_leaf: &Option<NodeId>,
) -> bool {
    if domains.is_empty() {
        return false;
    }
    // Every domain key must have exactly one tree slot; no unknown keys.
    if trees.len() != domains.len() {
        return false;
    }
    for domain in domains {
        if !domain.validate() {
            return false;
        }
        if !trees.contains_key(&domain.key()) {
            return false;
        }
    }
    for key in trees.keys() {
        if !domains.iter().any(|d| &d.key() == key) {
            return false;
        }
    }
    for domain in domains {
        // Node identity is meaningful only within its logical domain. Two
        // independently retained trees may use the same generated group id.
        let mut node_ids = BTreeSet::new();
        if let Some(tree) = trees.get(&domain.key()).cloned().flatten()
            && !validate_node_shares(&tree, &mut node_ids)
        {
            return false;
        }
    }
    // Tiled windows and deferred exceptions must be disjoint.
    for id in windows.keys() {
        if exceptions.contains_key(id) {
            return false;
        }
    }
    // Exception records must name known domains with non-empty ids.
    for record in exceptions.values() {
        if record.window.0.is_empty() || record.output.0.is_empty() || record.workspace.0.is_empty()
        {
            return false;
        }
        if domains
            .iter()
            .find(|d| d.id == record.output && d.workspace == record.workspace)
            .is_none()
        {
            return false;
        }
    }
    // Reciprocal leaf/window bindings scoped to the exact domain pair.
    let mut seen_leaves = BTreeSet::new();
    for link in windows.values() {
        if link.window.0.is_empty() || link.leaf.0.is_empty() || link.output.0.is_empty() {
            return false;
        }
        if link.workspace.0.is_empty() {
            return false;
        }
        if !seen_leaves.insert(link.leaf.clone()) {
            return false;
        }
        let key = DomainKey {
            output: link.output.clone(),
            workspace: link.workspace.clone(),
        };
        if domains.iter().find(|d| d.key() == key).is_none() {
            return false;
        }
        let Some(tree) = trees.get(&key).cloned().flatten() else {
            return false;
        };
        if !collect_leaves(&tree).contains(&link.leaf) {
            return false;
        }
    }
    // Every leaf needs exactly one link, scoped to its domain.
    for domain in domains {
        if let Some(tree) = trees.get(&domain.key()).cloned().flatten() {
            for leaf in collect_leaves(&tree) {
                if !windows.values().any(|l| {
                    l.leaf == leaf && l.output == domain.id && l.workspace == domain.workspace
                }) {
                    return false;
                }
            }
        }
    }
    // Focus must resolve to a linked leaf in its domain, or be fully empty.
    match (focused_domain, focused_leaf) {
        (None, None) => true,
        (Some(key), Some(leaf)) => {
            let Some(tree) = trees.get(key).cloned().flatten() else {
                return false;
            };
            if !collect_leaves(&tree).contains(leaf) {
                return false;
            }
            windows
                .values()
                .any(|l| &l.leaf == leaf && l.output == key.output && l.workspace == key.workspace)
        }
        _ => false,
    }
}

fn validate_node_shares(node: &Node, seen: &mut BTreeSet<NodeId>) -> bool {
    if node.id().0.is_empty() || !seen.insert(node.id().clone()) {
        return false;
    }
    match node {
        Node::Leaf { .. } => true,
        Node::Group {
            children, shares, ..
        } => {
            if children.len() < 2 || shares.len() != children.len() {
                return false;
            }
            if shares.contains(&0) {
                return false;
            }
            let mut total: u64 = 0;
            for share in shares {
                match total.checked_add(*share) {
                    Some(next) => total = next,
                    None => return false,
                }
            }
            if total == 0 {
                return false;
            }
            children.iter().all(|c| validate_node_shares(c, seen))
        }
    }
}

fn project_output_geometry(
    domain: Option<&OutputDomain>,
    tree: Option<&Node>,
    windows: &BTreeMap<WindowId, WindowLink>,
    key: &DomainKey,
    hints: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
) -> Result<Vec<DesiredGeometry>, ()> {
    let (Some(domain), Some(tree)) = (domain, tree) else {
        return Ok(Vec::new());
    };
    let leaf_to_window: BTreeMap<&NodeId, &WindowId> = windows
        .values()
        .filter(|l| l.output == key.output && l.workspace == key.workspace)
        .map(|l| (&l.leaf, &l.window))
        .collect();
    // Hints resolve leaf -> window -> per-window hints; unknown leaves or
    // windows without hints project exactly as before (advisory only).
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| hints.get(window))
            .copied()
            .unwrap_or_else(crate::size_hints::WindowSizeHints::none)
    };
    let hinted = crate::size_hints::project_with_hints(tree, domain.bounds, domain.gap, &resolve)
        .map_err(|_| ())?;
    let over: BTreeSet<&NodeId> = hinted.overconstrained.iter().collect();
    let mut out = Vec::with_capacity(hinted.leaves.len());
    for leaf in hinted.leaves {
        let Some(window) = leaf_to_window.get(&leaf.leaf) else {
            return Err(());
        };
        if leaf.rect.w <= 0 || leaf.rect.h <= 0 {
            return Err(());
        }
        out.push(DesiredGeometry {
            window: (*window).clone(),
            leaf: leaf.leaf.clone(),
            output: key.output.clone(),
            workspace: key.workspace.clone(),
            rect: leaf.rect,
            overconstrained: over.contains(&leaf.leaf),
            client_clamped: false,
        });
    }
    Ok(out)
}

/// Collect per-window size hints from one observation for projection.
///
/// Every observed window maps (hints resolve per window and sanitize at
/// use); callers without an observation pass an empty map, which projects
/// exactly as before.
fn hints_from_observed(
    observed: &[ObservedWindow],
) -> BTreeMap<WindowId, crate::size_hints::WindowSizeHints> {
    observed
        .iter()
        .map(|entry| (entry.window.clone(), entry.hints))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ops::drag::{
        GroupLayout, apply_group_edge_placement, apply_group_interior_placement, contains_point,
        drag_group_layouts,
    };
    use super::*;
    use crate::contract::Observation;

    fn owner() -> OwnerId {
        OwnerId::parse("owner-1").expect("valid")
    }

    fn generation() -> GenerationId {
        GenerationId::parse("gen-1").expect("valid")
    }

    fn domain() -> OutputDomain {
        OutputDomain {
            id: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    #[test]
    fn orientation_follows_cosmic_admission_axis() {
        // Source map_to_tree wide => source Vertical splits width => portable
        // Horizontal splits width (geometry.rs); tall/tie => portable Vertical.
        assert_eq!(
            orientation_from_bounds(&Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50
            }),
            Axis::Horizontal
        );
        assert_eq!(
            orientation_from_bounds(&Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100
            }),
            Axis::Vertical
        );
        assert_eq!(
            orientation_from_bounds(&Rect {
                x: 0,
                y: 0,
                w: 50,
                h: 100
            }),
            Axis::Vertical
        );
    }

    #[test]
    fn empty_session_snapshot_has_retained_domains() {
        let session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let snapshot = session.snapshot();
        assert_eq!(snapshot.domains.len(), 1);
        assert!(snapshot.domains[0].tree.is_none());
        assert!(snapshot.windows.is_empty());
    }

    #[test]
    fn leaf_removal_collapses_single_child_group() {
        let tree = Node::Group {
            id: NodeId("root".to_owned()),
            axis: Axis::Horizontal,
            children: vec![
                Node::Leaf {
                    id: NodeId("A".to_owned()),
                },
                Node::Group {
                    id: NodeId("inner".to_owned()),
                    axis: Axis::Vertical,
                    children: vec![
                        Node::Leaf {
                            id: NodeId("B".to_owned()),
                        },
                        Node::Leaf {
                            id: NodeId("C".to_owned()),
                        },
                    ],
                    shares: vec![1, 1],
                },
            ],
            shares: vec![1, 1],
        };
        let after = remove_leaf_from_tree(&CosmicV1Policy, Some(tree), &NodeId("B".to_owned()))
            .expect("tree");
        // Inner collapses to C; root keeps [A, C] with preserved shares.
        assert_eq!(
            after,
            Node::Group {
                id: NodeId("root".to_owned()),
                axis: Axis::Horizontal,
                children: vec![
                    Node::Leaf {
                        id: NodeId("A".to_owned()),
                    },
                    Node::Leaf {
                        id: NodeId("C".to_owned()),
                    },
                ],
                shares: vec![1, 1],
            }
        );
    }

    // ---- drag placement slice ----

    use crate::contract::{
        AckOutcome, AdapterAck, DragCapabilities, DragPostObservation, DragSide,
        LifecyclePostObservation,
    };
    use crate::ids::CorrelationId;

    fn domain_two() -> OutputDomain {
        OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn corr(value: &str) -> CorrelationId {
        CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
    }

    fn obs_windows(session: &Session) -> Vec<ObservedWindow> {
        let mut windows: Vec<ObservedWindow> = session
            .snapshot()
            .windows
            .iter()
            .map(|link| ObservedWindow {
                window: link.window.clone(),
                output: link.output.clone(),
                workspace: link.workspace.clone(),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect();
        windows.extend(session.exception_observed());
        windows
    }

    fn obs_for(session: &Session) -> SessionObservation {
        SessionObservation {
            observation: Observation::new(
                owner(),
                generation(),
                session.accepted_revision(),
                1000 + session.accepted_revision(),
            ),
            windows: obs_windows(session),
        }
    }

    /// Admit helper: `horizontal` names the desired portable split axis. The
    /// target geometry selects it through the source rule
    /// ([`crate::cosmic_v1::admission_axis`]: wide targets split portable
    /// Horizontal), so a horizontal split needs a wide target and vice versa.
    fn admit(session: &mut Session, window: &str, horizontal: bool) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let bounds = if horizontal {
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            }
        } else {
            Rect {
                x: 0,
                y: 0,
                w: 50,
                h: 100,
            }
        };
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: bounds,
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-admit-{window}"));
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("admit verify");
    }

    fn admit_exception(session: &mut Session, window: &str) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            exception_behavior: Some(ExceptionBehavior::Defer),
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-admit-{window}"));
        session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                vec![
                    crate::contract::LifecyclePrecondition::WindowObserved,
                    crate::contract::LifecyclePrecondition::DesiredTopologyValid,
                    crate::contract::LifecyclePrecondition::AdapterMustVerifyPostconditions,
                ],
                crate::contract::LifecycleOperation::AdmitDeferred {
                    window: WindowId(window.to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                },
            ))
            .expect("admit verify");
    }

    /// Admit helper with explicit observed flags and caller exceptions:
    /// proposes, acknowledges, and verifies, returning the plan for
    /// operation/allocation assertions.
    fn admit_flags(
        session: &mut Session,
        window: &str,
        observed: ExceptionFlags,
        exceptions: ExceptionFlags,
        exception_behavior: Option<ExceptionBehavior>,
    ) -> SessionPlan {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: observed.floating,
            fullscreen: observed.fullscreen,
            maximized: observed.maximized,
            sticky: observed.sticky,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let command = SessionCommand::Admit {
            window: window_id.clone(),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions,
            exception_behavior,
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-admit-{window}"));
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        // Deferred admissions verify fixed preconditions; normal tiled
        // admissions verify the plan's own dispatch preconditions.
        let (preconditions, operation) = if exceptions.any() {
            (
                vec![
                    crate::contract::LifecyclePrecondition::WindowObserved,
                    crate::contract::LifecyclePrecondition::DesiredTopologyValid,
                    crate::contract::LifecyclePrecondition::AdapterMustVerifyPostconditions,
                ],
                crate::contract::LifecycleOperation::AdmitDeferred {
                    window: window_id,
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                },
            )
        } else {
            (
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            )
        };
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                preconditions,
                operation,
            ))
            .expect("admit verify");
        plan
    }

    #[test]
    fn explicit_defer_maximize_only_preserves_flags_slotless() {
        // Previously-valid explicit deferred admission with
        // exceptions.maximized=true must not trip the R-MAX-06
        // maximized-only tile-carrier normalization into
        // PartialObservation.
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let max_only = ExceptionFlags {
            floating: false,
            fullscreen: false,
            maximized: true,
            sticky: false,
        };
        let plan = admit_flags(
            &mut session,
            "win-max",
            max_only,
            max_only,
            Some(ExceptionBehavior::Defer),
        );
        assert!(
            matches!(
                plan.dispatch.operation,
                crate::contract::LifecycleOperation::AdmitDeferred { .. }
            ),
            "maximize-only defer stays slotless"
        );
        assert!(
            plan.desired_snapshot.windows.is_empty(),
            "deferred admission takes no tile slot"
        );
        let observed = session.exception_observed();
        assert_eq!(observed.len(), 1, "one deferred exception stored");
        assert_eq!(observed[0].flags(), max_only, "maximized flag preserved");
        assert!(
            session.snapshot().windows.is_empty(),
            "committed deferred admission stays slotless"
        );
    }

    #[test]
    fn explicit_defer_floating_maximized_preserves_flags_slotless() {
        // A floating+maximized combo with matching caller exceptions
        // defers with both flags intact (never normalized away).
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let combo = ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: true,
            sticky: false,
        };
        let plan = admit_flags(
            &mut session,
            "win-float-max",
            combo,
            combo,
            Some(ExceptionBehavior::Defer),
        );
        assert!(
            matches!(
                plan.dispatch.operation,
                crate::contract::LifecycleOperation::AdmitDeferred { .. }
            ),
            "floating+max defer stays slotless"
        );
        let observed = session.exception_observed();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].flags(), combo, "combo flags preserved");
        assert!(session.snapshot().windows.is_empty());
    }

    #[test]
    fn normal_maximized_only_carrier_admits_tiled_with_alloc() {
        // The ordinary normal carrier (maximized-only birth, no caller
        // exceptions) still admits as a tile with a safe approved
        // allocation for the adapter clear phase.
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let plan = admit_flags(
            &mut session,
            "win-max",
            ExceptionFlags {
                floating: false,
                fullscreen: false,
                maximized: true,
                sticky: false,
            },
            ExceptionFlags::none(),
            None,
        );
        assert!(
            matches!(
                plan.dispatch.operation,
                crate::contract::LifecycleOperation::Admit { .. }
            ),
            "max-only carrier takes a tile slot"
        );
        assert!(
            !plan.desired_geometry.is_empty(),
            "approved allocation rides the plan"
        );
        assert_eq!(plan.desired_snapshot.windows.len(), 1);
        assert_eq!(session.snapshot().windows.len(), 1);
    }

    #[test]
    fn admit_binding_mismatch_stays_refused() {
        // Exception-binding invariants hold: only the maximized-only tile
        // carrier relaxes matching; anything else binds observed flags
        // exactly, and unselected behavior still refuses.
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let rev = session.accepted_revision();
        let propose = |session: &mut Session,
                       window: &str,
                       floating: bool,
                       maximized: bool,
                       exceptions: ExceptionFlags,
                       behavior: Option<ExceptionBehavior>| {
            let window_id = WindowId(window.to_owned());
            let mut windows = obs_windows(session);
            windows.push(ObservedWindow {
                window: window_id.clone(),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating,
                fullscreen: false,
                maximized,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
            let command = SessionCommand::Admit {
                window: window_id,
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions,
                exception_behavior: behavior,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 50,
                },
                suppress_fixed_float: false,
            };
            let observation = SessionObservation {
                observation: Observation::new(owner(), generation(), rev, 100 + rev),
                windows,
            };
            session.propose(
                &command,
                &observation,
                &corr(&format!("corr-admit-{window}")),
                &LifecycleCapabilities::full(),
            )
        };
        // Maximized-only observed with a floating exception selected.
        let err = propose(
            &mut session,
            "win-a",
            false,
            true,
            ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            Some(ExceptionBehavior::Defer),
        )
        .expect_err("cross-flag binding refuses");
        assert!(matches!(
            err,
            ProposeError::Refused(RefusalKind::PartialObservation)
        ));
        // Floating observed with no exceptions selected.
        let err = propose(
            &mut session,
            "win-b",
            true,
            false,
            ExceptionFlags::none(),
            None,
        )
        .expect_err("overlay without behavior refuses");
        assert!(matches!(
            err,
            ProposeError::Refused(RefusalKind::PartialObservation)
        ));
        // Maximized-only observed with a maximized exception but no
        // behavior selected.
        let err = propose(
            &mut session,
            "win-c",
            false,
            true,
            ExceptionFlags {
                floating: false,
                fullscreen: false,
                maximized: true,
                sticky: false,
            },
            None,
        )
        .expect_err("unselected behavior refuses");
        assert!(matches!(
            err,
            ProposeError::Refused(RefusalKind::ExceptionBehaviorUnselected)
        ));
    }

    /// Two tiled windows: root `H[leaf-win-1 (0-60), leaf-win-2 (60-120)]`,
    /// focus `win-2`.
    fn session_two() -> Session {
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        admit(&mut session, "win-1", true);
        admit(&mut session, "win-2", true);
        session
    }

    /// Three tiled windows: root `H[1 (0-40), 2 (40-80), 3 (80-120)]`, focus
    /// `win-3`.
    fn session_three() -> Session {
        let mut session = session_two();
        admit(&mut session, "win-3", true);
        session
    }

    /// Nested: root `H[1 (0-60 x full), V[2 (y0-40), 3 (y40-80)]]`, focus
    /// `win-3`.
    fn session_nested() -> Session {
        let mut session = session_two();
        admit(&mut session, "win-3", false);
        session
    }

    /// Deep: root `H[1, V[2 (y0-26), 3 (y26-52), 4 (y52-80)]]`, focus `win-4`.
    fn session_deep() -> Session {
        let mut session = session_nested();
        admit(&mut session, "win-4", false);
        session
    }

    fn domain_key() -> DomainKey {
        DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        }
    }

    fn leaf(name: &str) -> NodeId {
        NodeId(name.to_owned())
    }

    fn leaves_of(session: &Session) -> Vec<NodeId> {
        let key = domain_key();
        match session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output == key.output && d.workspace == key.workspace)
            .and_then(|d| d.tree)
        {
            Some(tree) => collect_leaves(&tree),
            None => Vec::new(),
        }
    }

    fn converge_windows(
        session: &Session,
        floating: &[&str],
        extra: &[&str],
        without: &[&str],
    ) -> SessionObservation {
        let mut windows = obs_windows(session);
        for entry in &mut windows {
            entry.floating = floating.contains(&entry.window.0.as_str());
        }
        windows.retain(|entry| !without.contains(&entry.window.0.as_str()));
        for name in extra {
            windows.push(ObservedWindow {
                window: WindowId((*name).to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
        }
        SessionObservation {
            observation: Observation::new(
                owner(),
                generation(),
                session.accepted_revision(),
                5000 + session.accepted_revision(),
            ),
            windows,
        }
    }

    #[test]
    fn converge_observation_skew_missing_new_and_exact() {
        let mut session = session_two();
        let base = session.accepted_revision();
        // Floating skew: retained tiled win-1 observed floating.
        let summary = session
            .converge_observation(
                &converge_windows(&session, &["win-1"], &[], &[]),
                Some(&WindowId("win-2".to_owned())),
            )
            .expect("skew converges");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (0, 0, 1)
        );
        assert_eq!(session.accepted_revision(), base + 1);
        assert!(session.is_exception(&WindowId("win-1".to_owned())));
        assert_eq!(leaves_of(&session), vec![leaf("leaf-win-2")]);
        assert_eq!(session.focus().1, Some(leaf("leaf-win-2")));
        // Missing exception plus brand-new normal in one observation.
        let base = session.accepted_revision();
        let summary = session
            .converge_observation(
                &converge_windows(&session, &[], &["win-3"], &["win-1"]),
                Some(&WindowId("win-2".to_owned())),
            )
            .expect("missing/new converges");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (1, 1, 0)
        );
        assert_eq!(session.accepted_revision(), base + 1);
        assert!(leaves_of(&session).contains(&leaf("leaf-win-2")));
        assert_eq!(
            session.focus().1.map(|l| l.0.ends_with("win-2")),
            Some(true)
        );
        // Exact match advances nothing.
        let base = session.accepted_revision();
        let summary = session
            .converge_observation(&obs_for(&session), Some(&WindowId("win-2".to_owned())))
            .expect("exact converges");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (0, 0, 0)
        );
        assert_eq!(session.accepted_revision(), base);
        // Stale revision diverges fail-closed without mutating membership.
        let before = session.snapshot();
        let mut stale = obs_for(&session);
        stale.observation = Observation::new(owner(), generation(), base + 99, 0);
        assert!(matches!(
            session.converge_observation(&stale, None),
            Err(ProposeError::Diverged(_))
        ));
        assert_eq!(session.snapshot(), before);
    }

    /// Sequential ordinary admit using the exact admission placement policy
    /// (`seed_target_bounds`), mirroring what the Engine/seed path supplies
    /// as `placement_bounds` for each step.
    fn admit_seed(session: &mut Session, window: &str, index: usize) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let domain_state = session.domains()[0].clone();
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: crate::seed::seed_target_bounds(session, &domain_state),
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-seed-{window}-{index}"));
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("admit verify");
    }

    /// Generated group ids are placement nonces; strip them so topology,
    /// order, and shares compare structurally.
    fn anonymize(node: &Node) -> Node {
        match node {
            Node::Leaf { id } => Node::Leaf { id: id.clone() },
            Node::Group {
                axis,
                children,
                shares,
                ..
            } => Node::Group {
                id: NodeId("g".to_owned()),
                axis: *axis,
                children: children.iter().map(anonymize).collect(),
                shares: shares.clone(),
            },
        }
    }

    fn domain_tree(session: &Session) -> Node {
        session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output == domain_key().output && d.workspace == domain_key().workspace)
            .and_then(|d| d.tree)
            .expect("domain tree")
    }

    #[test]
    fn converge_newcomers_match_sequential_seed_admits() {
        // Two newcomers in one observation must land exactly where successive
        // ordinary admits under the same placement policy put them.
        let mut converged = session_two();
        let base = converged.accepted_revision();
        let summary = converged
            .converge_observation(
                &converge_windows(&converged, &[], &["win-3", "win-4"], &[]),
                Some(&WindowId("win-4".to_owned())),
            )
            .expect("newcomers converge");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (0, 2, 0)
        );
        assert_eq!(converged.accepted_revision(), base + 1);
        let mut sequential = session_two();
        admit_seed(&mut sequential, "win-3", 0);
        admit_seed(&mut sequential, "win-4", 1);
        assert_eq!(leaves_of(&converged), leaves_of(&sequential));
        assert_eq!(converged.snapshot().windows, sequential.snapshot().windows);
        assert_eq!(
            anonymize(&domain_tree(&converged)),
            anonymize(&domain_tree(&sequential))
        );
        assert_eq!(converged.focus(), sequential.focus());
        // Focus naming a removed window falls back to surviving MRU focus.
        let mut session = session_two();
        let summary = session
            .converge_observation(
                &converge_windows(&session, &[], &[], &["win-2"]),
                Some(&WindowId("win-2".to_owned())),
            )
            .expect("removed focus converges");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (1, 0, 0)
        );
        assert_eq!(session.focus().1, Some(leaf("leaf-win-1")));
        // Empty session plus empty observation is an exact no-op.
        let mut session = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        let base = session.accepted_revision();
        let empty = SessionObservation {
            observation: Observation::new(owner(), generation(), base, 4242),
            windows: Vec::new(),
        };
        let summary = session
            .converge_observation(&empty, None)
            .expect("empty exact");
        assert_eq!(
            (summary.removed, summary.admitted, summary.flags_adopted),
            (0, 0, 0)
        );
        assert_eq!(session.accepted_revision(), base);
        assert_eq!(session.focus(), (None, None));
    }

    #[test]
    fn converge_refuses_cross_homing() {
        let left = OutputDomain {
            id: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            gap: 0,
            adjacent: [(Direction::Right, OutputId("out-2".to_owned()))]
                .into_iter()
                .collect(),
        };
        let right = OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            gap: 0,
            adjacent: [(Direction::Left, OutputId("out-1".to_owned()))]
                .into_iter()
                .collect(),
        };
        let mut session =
            Session::new(owner(), generation(), 0, 7, vec![left, right]).expect("new");
        admit(&mut session, "win-a", true);
        let before = session.snapshot();
        let rev = session.accepted_revision();
        // win-a retained on out-1 but observed homed on out-2: the Engine
        // converges this as remove-from-source plus admit-into-destination.
        let moved = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 4242),
            windows: vec![ObservedWindow {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
        };
        assert!(matches!(
            session.converge_observation(&moved, None),
            Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch))
        ));
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.accepted_revision(), rev);
    }

    fn begin_focused(session: &mut Session, window: &str) -> DragCapture {
        let before = session.snapshot();
        let capture = session
            .begin_drag(&WindowId(window.to_owned()), &obs_for(session))
            .expect("begin");
        // Begin mutates no authoritative topology and stages no pending.
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_pending());
        assert!(!session.has_pending_desired());
        assert!(session.has_drag());
        capture
    }

    fn drop_planned(session: &mut Session, x: i32, y: i32, correlation: &str) -> SessionDragPlan {
        let before = session.snapshot();
        match session
            .drop_drag(
                x,
                y,
                &obs_for(session),
                &corr(correlation),
                &DragCapabilities::full(),
            )
            .expect("drop")
        {
            DragRelease::Planned(plan) => {
                // Drop stages pending but commits no topology.
                assert_eq!(session.snapshot(), before);
                assert!(session.has_pending());
                assert!(!session.has_drag());
                *plan
            }
            DragRelease::SnapBack(_) => panic!("expected a drag plan at ({x}, {y})"),
        }
    }

    fn commit_drag(session: &mut Session, plan: &SessionDragPlan, correlation: &str) {
        let rev = plan.dispatch.base_revision;
        let correlation = corr(correlation);
        assert_eq!(plan.dispatch.correlation_id, correlation);
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        let commit = session
            .verify_drag(&DragPostObservation::new(
                Observation::new(owner(), generation(), rev, 4242),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("verify");
        assert_eq!(commit.revision, rev + 1);
        assert_eq!(session.accepted_revision(), rev + 1);
        assert_eq!(session.snapshot(), plan.desired_snapshot);
        assert!(!session.has_pending());
    }

    /// Exactly-once leaves/windows, valid shares, positive projected areas.
    fn assert_tree_invariants(session: &Session) {
        let snapshot = session.snapshot();
        let mut leaf_to_window = BTreeMap::new();
        for link in &snapshot.windows {
            assert!(
                leaf_to_window
                    .insert(link.leaf.clone(), link.window.clone())
                    .is_none(),
                "leaf bound twice"
            );
        }
        for view in &snapshot.domains {
            let Some(tree) = view.tree.as_ref() else {
                continue;
            };
            let leaves = collect_leaves(tree);
            let mut seen = BTreeSet::new();
            for leaf in &leaves {
                assert!(seen.insert(leaf.clone()), "leaf twice in tree");
                assert!(
                    leaf_to_window.contains_key(leaf),
                    "leaf without exactly one window"
                );
            }
            assert_valid_shares(tree);
            let domain = session
                .domains()
                .iter()
                .find(|d| d.id == view.output && d.workspace == view.workspace)
                .expect("known domain");
            let projected =
                crate::geometry::project(tree, domain.bounds, domain.gap).expect("projectable");
            assert_eq!(projected.len(), leaves.len());
            for leaf in &projected {
                assert!(leaf.rect.w > 0 && leaf.rect.h > 0, "positive area");
            }
        }
        assert_eq!(
            leaf_to_window.len(),
            snapshot.windows.len(),
            "every window on exactly one leaf"
        );
    }

    fn assert_valid_shares(node: &Node) {
        match node {
            Node::Leaf { id } => assert!(!id.0.is_empty()),
            Node::Group {
                id,
                children,
                shares,
                ..
            } => {
                assert!(!id.0.is_empty());
                assert!(children.len() >= 2);
                assert_eq!(shares.len(), children.len());
                assert!(!shares.contains(&0));
                for child in children {
                    assert_valid_shares(child);
                }
            }
        }
    }

    fn contained(inner: &Rect, outer: &Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.w <= outer.x + outer.w
            && inner.y + inner.h <= outer.y + outer.h
    }

    fn intersects(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn drag_sides_map_to_axis_and_order() {
        // Two-wide: removing win-2 collapses the root, so a left drop onto
        // leaf-win-1 wraps the root target (still Horizontal, before).
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        assert_eq!(capture.source_rect.w, 60);
        let left = session.preview_drag(5, 40, &[]).expect("left preview");
        assert_eq!(left.target_leaf, leaf("leaf-win-1"));
        assert_eq!(left.side, DragSide::Left);
        assert_eq!(left.axis, Axis::Horizontal);
        assert!(left.before);
        assert!(left.wrap);
        session.cancel_drag();
        // Nested three: leaf-win-1 (0-60) right edge zone inserts after.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let right = session.preview_drag(55, 40, &[]).expect("right preview");
        assert_eq!(right.target_leaf, leaf("leaf-win-1"));
        assert_eq!(right.side, DragSide::Right);
        assert_eq!(right.axis, Axis::Horizontal);
        assert!(!right.before);
        assert!(!right.wrap);
        session.cancel_drag();
        // Vertical nested: leaf-win-2 bottom edge zone orders
        // after inside the surviving V parent (no wrap).
        let mut session = session_deep();
        begin_focused(&mut session, "win-4");
        let top = session.preview_drag(90, 5, &[]).expect("top preview");
        assert_eq!(top.target_leaf, leaf("leaf-win-2"));
        assert_eq!(top.side, DragSide::Top);
        assert_eq!(top.axis, Axis::Vertical);
        assert!(top.before);
        assert!(!top.wrap);
        let bottom = session.preview_drag(90, 35, &[]).expect("bottom preview");
        assert_eq!(bottom.target_leaf, leaf("leaf-win-2"));
        assert_eq!(bottom.side, DragSide::Bottom);
        assert_eq!(bottom.axis, Axis::Vertical);
        assert!(!bottom.before);
        assert!(!bottom.wrap);
    }

    #[test]
    fn drag_same_axis_nary_reorder() {
        // H[1,2,3] focus win-3 left onto leaf-win-1 -> H[3,1,2].
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let preview = session.preview_drag(5, 40, &[]).expect("preview");
        assert!(!preview.wrap);
        assert_eq!(preview.insertion_index, 0);
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        assert_eq!(plan.dispatch.operation.target_leaf, leaf("leaf-win-1"));
        assert_eq!(plan.dispatch.operation.side, DragSide::Left);
        assert!(!plan.dispatch.operation.wrap);
        assert_eq!(plan.dispatch.operation.insertion_index, 0);
        commit_drag(&mut session, &plan, "corr-drag-1");
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-3"), leaf("leaf-win-1"), leaf("leaf-win-2")]
        );
        // Focus preserved on the moved window.
        assert_eq!(
            session.focus(),
            (Some(domain_key()), Some(leaf("leaf-win-3")))
        );
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_perpendicular_wraps_only_target_subtree() {
        // H[1,2,3] focus win-3 top onto leaf-win-1: remove 3 -> H[1,2], then
        // wrap leaf-win-1 in a V[3,1] split; leaf-win-2 untouched.
        let mut session = session_three();
        assert_eq!(leaves_of(&session).len(), 3);
        begin_focused(&mut session, "win-3");
        // Middle column (x in [13,27)) top third: perpendicular Top wrap.
        let preview = session.preview_drag(20, 5, &[]).expect("preview");
        assert!(preview.wrap);
        assert_eq!(preview.axis, Axis::Vertical);
        assert!(preview.before);
        let plan = drop_planned(&mut session, 20, 5, "corr-drag-1");
        let operation = &plan.dispatch.operation;
        assert!(operation.wrap);
        let new_group = operation.new_group.clone().expect("fresh group");
        assert_ne!(new_group, leaf("leaf-win-1"));
        // Wrapper inherits the target share slot: root stays [1,1].
        let root = plan
            .desired_snapshot
            .domains
            .clone()
            .into_iter()
            .find(|d| d.output.0 == "out-1")
            .and_then(|d| d.tree)
            .expect("desired tree");
        match &root {
            Node::Group {
                axis: Axis::Horizontal,
                children,
                shares,
                ..
            } => {
                assert_eq!(shares, &vec![1, 1]);
                assert_eq!(children.len(), 2);
                assert_eq!(
                    children[1],
                    Node::Leaf {
                        id: leaf("leaf-win-2")
                    }
                );
                match &children[0] {
                    Node::Group {
                        id,
                        axis: Axis::Vertical,
                        children,
                        shares,
                    } => {
                        assert_eq!(id, &new_group);
                        assert_eq!(shares, &vec![1, 1]);
                        assert_eq!(
                            children,
                            &vec![
                                Node::Leaf {
                                    id: leaf("leaf-win-3")
                                },
                                Node::Leaf {
                                    id: leaf("leaf-win-1")
                                },
                            ]
                        );
                    }
                    other => panic!("expected V wrapper, got {other:?}"),
                }
            }
            other => panic!("expected H root, got {other:?}"),
        }
        commit_drag(&mut session, &plan, "corr-drag-1");
        assert_eq!(
            session.focus(),
            (Some(domain_key()), Some(leaf("leaf-win-3")))
        );
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_deep_nesting_collapse_and_wrap() {
        // H[1, V[2,3,4]] focus win-4 left onto leaf-win-2 (60-80, y0-26):
        // remove 4 -> V[2,3], wrap leaf-win-2 in H[4,2] at V index 0.
        let mut session = session_deep();
        begin_focused(&mut session, "win-4");
        let preview = session.preview_drag(65, 13, &[]).expect("preview");
        assert!(preview.wrap);
        assert_eq!(preview.target_leaf, leaf("leaf-win-2"));
        assert_eq!(preview.axis, Axis::Horizontal);
        assert!(preview.before);
        let plan = drop_planned(&mut session, 65, 13, "corr-drag-1");
        commit_drag(&mut session, &plan, "corr-drag-1");
        let root = session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output.0 == "out-1")
            .and_then(|d| d.tree)
            .expect("tree");
        match root {
            Node::Group { children, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(
                    children[0],
                    Node::Leaf {
                        id: leaf("leaf-win-1")
                    }
                );
                match &children[1] {
                    Node::Group {
                        children, shares, ..
                    } => {
                        assert_eq!(shares, &vec![1, 1]);
                        assert_eq!(children.len(), 2);
                        // Unaffected leaf-win-3 keeps its slot and share.
                        assert_eq!(
                            children[1],
                            Node::Leaf {
                                id: leaf("leaf-win-3")
                            }
                        );
                        match &children[0] {
                            Node::Group {
                                children, shares, ..
                            } => {
                                assert_eq!(shares, &vec![1, 1]);
                                assert_eq!(
                                    children,
                                    &vec![
                                        Node::Leaf {
                                            id: leaf("leaf-win-4")
                                        },
                                        Node::Leaf {
                                            id: leaf("leaf-win-2")
                                        },
                                    ]
                                );
                            }
                            other => panic!("expected H wrapper, got {other:?}"),
                        }
                    }
                    other => panic!("expected V inner, got {other:?}"),
                }
            }
            other => panic!("expected H root, got {other:?}"),
        }
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_source_collapse_promotes_sibling() {
        // H[1,2] focus win-2 top onto leaf-win-1: remove 2 collapses the root
        // to leaf-win-1, then the root target wraps in V[2,1].
        let mut session = session_two();
        begin_focused(&mut session, "win-2");
        let plan = drop_planned(&mut session, 30, 5, "corr-drag-1");
        assert!(plan.dispatch.operation.wrap);
        commit_drag(&mut session, &plan, "corr-drag-1");
        let root = session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output.0 == "out-1")
            .and_then(|d| d.tree)
            .expect("tree");
        assert_eq!(
            root,
            Node::Group {
                id: plan.dispatch.operation.new_group.clone().expect("group"),
                axis: Axis::Vertical,
                children: vec![
                    Node::Leaf {
                        id: leaf("leaf-win-2")
                    },
                    Node::Leaf {
                        id: leaf("leaf-win-1")
                    },
                ],
                shares: vec![1, 1],
            }
        );
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_cross_output_relocates_source_to_pointer_edge() {
        // Source out-1/ws-1 H[win-1, win-2] focused win-2; destination
        // out-2/ws-1 H[win-3]. A left-edge drop on win-3 inserts win-2
        // before it and collapses the source to sole win-1.
        fn admit_on(session: &mut Session, window: &str, output: &str, workspace: &str) {
            let rev = session.accepted_revision();
            let window_id = WindowId(window.to_owned());
            let mut windows = obs_windows(session);
            windows.push(ObservedWindow {
                window: window_id.clone(),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
            let command = SessionCommand::Admit {
                window: window_id,
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 50,
                },
                suppress_fixed_float: false,
            };
            let observation = SessionObservation {
                observation: Observation::new(owner(), generation(), rev, 100 + rev),
                windows,
            };
            let correlation = corr(&format!("corr-admit-{window}"));
            let plan = session
                .propose(
                    &command,
                    &observation,
                    &correlation,
                    &LifecycleCapabilities::full(),
                )
                .expect("admit propose");
            session
                .acknowledge(&AdapterAck::new(
                    correlation.clone(),
                    owner(),
                    generation(),
                    rev,
                    AckOutcome::Accepted,
                ))
                .expect("admit ack");
            session
                .verify_lifecycle(&LifecyclePostObservation::new(
                    Observation::new(owner(), generation(), rev, 200 + rev),
                    correlation,
                    true,
                    plan.dispatch.preconditions.clone(),
                    plan.dispatch.operation.clone(),
                ))
                .expect("admit verify");
        }
        let mut source = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        admit(&mut source, "win-1", true);
        admit(&mut source, "win-2", true);
        let mut dest = Session::new(owner(), generation(), 0, 7, vec![domain_two()]).expect("new");
        admit_on(&mut dest, "win-3", "out-2", "ws-1");
        let mut pair = Session::paired_from_canonical_for_send(
            &source,
            Some(&dest),
            vec![domain(), domain_two()],
        )
        .expect("pair");
        let dest_key = DomainKey {
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        assert!(pair.sync_focus_from_window(&domain_key(), &WindowId("win-2".to_owned())));
        let observation = obs_for(&pair);
        pair.begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        // Destination is 120x80 with sole win-3: x=5 sits in the left edge
        // zone, so the mover inserts before the target.
        let preview = pair
            .preview_drag_cross_output(&dest_key, 5, 40, &observation)
            .expect("preview");
        assert_eq!(preview.domain, dest_key);
        assert_eq!(preview.source_window.0, "win-2");
        assert_eq!(preview.target_window.0, "win-3");
        assert_eq!(preview.side, DragSide::Left);
        assert!(preview.before);
        let plan = match pair
            .drop_drag_cross_output(
                &dest_key,
                5,
                40,
                &observation,
                &corr("corr-drag-x"),
                &DragCapabilities::full(),
            )
            .expect("drop")
        {
            DragRelease::Planned(plan) => *plan,
            DragRelease::SnapBack(_) => panic!("expected a cross-output drag plan"),
        };
        // Preview and drop agree on target, edge, and proposed geometry.
        assert_eq!(preview.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(
            preview.proposed_rect,
            plan.desired_geometry
                .iter()
                .find(|g| g.window.0 == "win-2")
                .expect("mover geometry")
                .rect
        );
        // Honest cross-domain operation record: the target domain plus the
        // capture source, with the cross precondition instead of the
        // same-domain one.
        let operation = &plan.dispatch.operation;
        assert_eq!(operation.domain_output.0, "out-2");
        assert_eq!(
            operation.cross_source,
            Some((OutputId("out-1".to_owned()), WorkspaceId("ws-1".to_owned())))
        );
        assert!(operation.is_cross_output());
        let preconditions = operation.preconditions();
        assert!(preconditions.contains(&crate::contract::DragPrecondition::DragTargetsCrossDomain));
        assert!(!preconditions.contains(&crate::contract::DragPrecondition::DragTargetsSameDomain));
        assert_eq!(plan.dispatch.preconditions, preconditions);
        // Focus follows the mover into the destination domain.
        assert_eq!(plan.desired_focus_domain, dest_key);
        // Complete both-domain geometry: win-1 sole on the source, win-2
        // left of win-3 on the destination.
        assert_eq!(plan.desired_geometry.len(), 3);
        let mover_rect = plan
            .desired_geometry
            .iter()
            .find(|g| g.window.0 == "win-2")
            .expect("mover geometry")
            .rect;
        assert_eq!(
            mover_rect,
            Rect {
                x: 0,
                y: 0,
                w: 60,
                h: 80
            }
        );
        let source_rect = plan
            .desired_geometry
            .iter()
            .find(|g| g.window.0 == "win-1")
            .expect("source geometry")
            .rect;
        assert_eq!(
            source_rect,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            }
        );
        commit_drag(&mut pair, &plan, "corr-drag-x");
        let snapshot = pair.snapshot();
        let mover_link = snapshot
            .windows
            .iter()
            .find(|l| l.window.0 == "win-2")
            .expect("mover link");
        assert_eq!(mover_link.output.0, "out-2");
        assert_eq!(plan.desired_focus_leaf, mover_link.leaf);
        let source_leaves: Vec<String> = snapshot
            .windows
            .iter()
            .filter(|l| l.output.0 == "out-1")
            .map(|l| l.window.0.clone())
            .collect();
        assert_eq!(source_leaves, vec!["win-1".to_owned()]);
        assert_tree_invariants(&pair);
    }

    #[test]
    fn drag_cross_output_invalid_releases_snap_back() {
        let mut source = Session::new(owner(), generation(), 0, 7, vec![domain()]).expect("new");
        admit(&mut source, "win-1", true);
        admit(&mut source, "win-2", true);
        let mut dest = Session::new(owner(), generation(), 0, 7, vec![domain_two()]).expect("new");
        // Admit directly on out-2 via a converged observation.
        let rev = dest.accepted_revision();
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows: vec![ObservedWindow {
                window: WindowId("win-3".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
        };
        let correlation = corr("corr-admit-win-3");
        let command = SessionCommand::Admit {
            window: WindowId("win-3".to_owned()),
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            suppress_fixed_float: false,
        };
        let plan = dest
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        dest.acknowledge(&AdapterAck::new(
            correlation.clone(),
            owner(),
            generation(),
            rev,
            AckOutcome::Accepted,
        ))
        .expect("admit ack");
        dest.verify_lifecycle(&LifecyclePostObservation::new(
            Observation::new(owner(), generation(), rev, 200 + rev),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        ))
        .expect("admit verify");
        let dest_key = DomainKey {
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        // Same-domain destination is not a cross release: snaps back.
        let mut pair = Session::paired_from_canonical_for_send(
            &source,
            Some(&dest),
            vec![domain(), domain_two()],
        )
        .expect("pair");
        assert!(pair.sync_focus_from_window(&domain_key(), &WindowId("win-2".to_owned())));
        let observation = obs_for(&pair);
        // Every invalid release below clears the transient capture, so each
        // case begins fresh exactly like a new production gesture.
        pair.begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        match pair
            .drop_drag_cross_output(
                &domain_key(),
                5,
                40,
                &observation,
                &corr("corr-drag-same"),
                &DragCapabilities::full(),
            )
            .expect("same-domain cross call resolves")
        {
            DragRelease::SnapBack(snap) => assert_eq!(snap.source_window.0, "win-2"),
            DragRelease::Planned(_) => panic!("same-domain cross release must snap back"),
        }
        // Unknown destination domain snaps back.
        let unknown = DomainKey {
            output: OutputId("out-9".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
        };
        pair.begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        match pair
            .drop_drag_cross_output(
                &unknown,
                5,
                40,
                &observation,
                &corr("corr-drag-unknown"),
                &DragCapabilities::full(),
            )
            .expect("unknown-dest cross call resolves")
        {
            DragRelease::SnapBack(snap) => assert_eq!(snap.source_window.0, "win-2"),
            DragRelease::Planned(_) => panic!("unknown destination must snap back"),
        }
        // Center drops fail closed as unsupported with no plan.
        pair.begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        match pair.drop_drag_cross_output(
            &dest_key,
            60,
            40,
            &observation,
            &corr("corr-drag-center"),
            &DragCapabilities::full(),
        ) {
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability)) => {}
            other => panic!("center must refuse unsupported, got {other:?}"),
        }
        // Points outside the destination work area snap back.
        pair.begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        match pair
            .drop_drag_cross_output(
                &dest_key,
                500,
                500,
                &observation,
                &corr("corr-drag-outside"),
                &DragCapabilities::full(),
            )
            .expect("outside cross call resolves")
        {
            DragRelease::SnapBack(snap) => assert_eq!(snap.source_window.0, "win-2"),
            DragRelease::Planned(_) => panic!("outside destination must snap back"),
        }
        assert!(!pair.has_pending());
    }

    #[test]
    fn drag_cross_output_sticky_scope_survives_same_dest() {
        // Same-destination carry preserves the 80px sticky edge across
        // previews into the final drop (see `cross_sticky_world`).
        let (mut session, observation, _, dst_a_key, _, gap) = cross_sticky_world();
        let edge = session
            .preview_drag_cross_output(&dst_a_key, gap.0, gap.1, &observation)
            .expect("edge preview");
        assert_eq!(edge.side, crate::contract::DragSide::Left);
        assert!(edge.prior.is_some(), "gap hover must arm the sticky edge");
        // Negative control: simulate single-shot clone loss by clearing the
        // live stored hover; the sticky-only probe then misses the Left edge
        // (default 32px depth cannot reach it).
        session.drag.as_mut().expect("drag").prior = None;
        let missed = session
            .preview_drag_cross_output(&dst_a_key, gap.0 + 45, gap.1, &observation)
            .expect("probe preview");
        assert!(
            !(missed.side == crate::contract::DragSide::Left
                && missed.target_leaf == edge.target_leaf),
            "uncarried sticky probe must miss"
        );
        let carried = edge.hover_prior();
        assert_eq!(carried.domain.output.0, "out-1");
        assert_eq!(carried.prior_domain, dst_a_key);
        assert!(session.carry_drag_prior(&carried));
        // Sticky-only probe: 45px past the arming point (beyond the default
        // 32px edge depth, inside the 80px sticky depth). Without the carry
        // it misses the Left edge; with it, the 80px hover steers.
        let sticky = session
            .preview_drag_cross_output(&dst_a_key, gap.0 + 45, gap.1, &observation)
            .expect("sticky preview");
        assert_eq!(sticky.side, crate::contract::DragSide::Left);
        assert_eq!(sticky.target_leaf, edge.target_leaf);
        assert!(sticky.prior.is_some(), "sticky edge must survive same dest");
        let plan = match session
            .drop_drag_cross_output(
                &dst_a_key,
                gap.0 + 45,
                gap.1,
                &observation,
                &corr("corr-drag-sticky"),
                &DragCapabilities::full(),
            )
            .expect("sticky drop")
        {
            DragRelease::Planned(plan) => *plan,
            DragRelease::SnapBack(_) => panic!("sticky drop must plan"),
        };
        assert_eq!(sticky.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(sticky.side, plan.dispatch.operation.side);
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_cross_output_sticky_scope_clears_on_dest_change() {
        // A hover scoped to another destination (or an unknown one) never
        // steers: unknown scopes refuse the carry, foreign scopes drop at
        // the resolution gate, and the other destination resolves cleanly.
        let (mut session, observation, _, dst_a_key, dst_b_key, _) = cross_sticky_world();
        let live = session.drag.clone().expect("capture").source_leaf;
        let revision = session.accepted_revision();
        let src_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let foreign = DragHoverPrior {
            domain: src_key.clone(),
            source_leaf: live.clone(),
            source_window: WindowId("win-2".to_owned()),
            revision,
            prior: None,
            prior_domain: DomainKey {
                output: OutputId("out-9".to_owned()),
                workspace: WorkspaceId("ws-9".to_owned()),
            },
        };
        assert!(
            !session.carry_drag_prior(&foreign),
            "unknown resolution scope must not carry"
        );
        let scoped_prior = DragHoverPrior {
            domain: src_key,
            source_leaf: live,
            source_window: WindowId("win-2".to_owned()),
            revision,
            prior: None,
            prior_domain: dst_b_key.clone(),
        };
        assert!(session.carry_drag_prior(&scoped_prior));
        let scoped = session.cross_resolution_scope(&dst_a_key).expect("scope");
        assert!(
            scoped.prior.is_none(),
            "foreign-scoped hover must not steer destination A"
        );
        let plain = session
            .preview_drag_cross_output(&dst_b_key, 5, 40, &observation)
            .expect("dest B preview");
        assert_eq!(plain.target_window.0, "win-6");
        assert!(!session.has_pending());
        assert_tree_invariants(&session);
    }

    /// Three-domain sticky world: source out-1 H[win-1, win-2] focused win-2,
    /// destination A out-2 H[win-3, V[win-4, win-5]] with an 8px gap,
    /// destination B out-3 sole win-6. Returns the begun session, its
    /// observation, the three keys, and a group-edge point on destination
    /// A's inner vertical group (left strip, clear of corner ambiguity).
    fn cross_sticky_world() -> (
        Session,
        SessionObservation,
        DomainKey,
        DomainKey,
        DomainKey,
        (i32, i32),
    ) {
        use super::ops::drag::{contains_point, drag_group_layouts};
        fn domain_on(output: &str, gap: i32) -> OutputDomain {
            OutputDomain {
                id: OutputId(output.to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 120,
                    h: 80,
                },
                gap,
                adjacent: BTreeMap::new(),
            }
        }
        fn admit_on(session: &mut Session, window: &str, output: &str, horizontal: bool) {
            let rev = session.accepted_revision();
            let window_id = WindowId(window.to_owned());
            let mut windows = obs_windows(session);
            windows.push(ObservedWindow {
                window: window_id.clone(),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
            let bounds = if horizontal {
                Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 50,
                }
            } else {
                Rect {
                    x: 0,
                    y: 0,
                    w: 50,
                    h: 100,
                }
            };
            let command = SessionCommand::Admit {
                window: window_id,
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: bounds,
                suppress_fixed_float: false,
            };
            let observation = SessionObservation {
                observation: Observation::new(owner(), generation(), rev, 100 + rev),
                windows,
            };
            let correlation = corr(&format!("corr-admit-{window}"));
            let plan = session
                .propose(
                    &command,
                    &observation,
                    &correlation,
                    &LifecycleCapabilities::full(),
                )
                .expect("admit propose");
            session
                .acknowledge(&AdapterAck::new(
                    correlation.clone(),
                    owner(),
                    generation(),
                    rev,
                    AckOutcome::Accepted,
                ))
                .expect("admit ack");
            session
                .verify_lifecycle(&LifecyclePostObservation::new(
                    Observation::new(owner(), generation(), rev, 200 + rev),
                    correlation,
                    true,
                    plan.dispatch.preconditions.clone(),
                    plan.dispatch.operation.clone(),
                ))
                .expect("admit verify");
        }
        let src = domain_on("out-1", 0);
        let dst_a = domain_on("out-2", 8);
        let dst_b = domain_on("out-3", 0);
        let mut session =
            Session::new(owner(), generation(), 0, 7, vec![src, dst_a.clone(), dst_b])
                .expect("new");
        admit_on(&mut session, "win-1", "out-1", true);
        admit_on(&mut session, "win-2", "out-1", true);
        admit_on(&mut session, "win-3", "out-2", true);
        admit_on(&mut session, "win-4", "out-2", true);
        // Tall admit nests under the focused win-4: H[win-3, V[win-4, win-5]].
        admit_on(&mut session, "win-5", "out-2", false);
        admit_on(&mut session, "win-6", "out-3", true);
        let src_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let dst_a_key = DomainKey {
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let dst_b_key = DomainKey {
            output: OutputId("out-3".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        assert!(session.sync_focus_from_window(&src_key, &WindowId("win-2".to_owned())));
        let observation = obs_for(&session);
        session
            .begin_drag(&WindowId("win-2".to_owned()), &observation)
            .expect("begin");
        // Group-edge point on destination A's inner vertical group: left
        // strip (within the default 32px edge depth) at a mid height clear
        // of corner ambiguity, inside the group union but in no leaf rect.
        let dst_a_tree = session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output.0 == "out-2")
            .and_then(|d| d.tree)
            .expect("dest A tree");
        let projected =
            crate::geometry::project(&dst_a_tree, dst_a.bounds, dst_a.gap).expect("projectable");
        let mut leaf_map = BTreeMap::new();
        for entry in &projected {
            leaf_map.insert(entry.leaf.clone(), entry.rect);
        }
        let inner = drag_group_layouts(&dst_a_tree, &leaf_map)
            .into_iter()
            .find(|l| l.axis == Axis::Vertical)
            .expect("inner V group");
        let cx = inner.rect.x + inner.rect.w / 2;
        let mut gy_opt = None;
        for y in inner.rect.y..inner.rect.y + inner.rect.h {
            if !leaf_map.values().any(|r| contains_point(r, cx, y))
                && y - inner.rect.y >= 32
                && inner.rect.y + inner.rect.h - y > 32
            {
                gy_opt = Some(y);
                break;
            }
        }
        let gy = gy_opt.expect("inner gap row");
        let gx = inner.rect.x + 5;
        assert!(
            !leaf_map.values().any(|r| contains_point(r, gx, gy)),
            "edge probe must be group-only"
        );
        (
            session,
            observation,
            src_key,
            dst_a_key,
            dst_b_key,
            (gx, gy),
        )
    }

    #[test]
    fn drag_preview_drop_structural_and_geometry_agreement() {
        let mut session = session_three();
        let capture = begin_focused(&mut session, "win-3");
        let preview = session.preview_drag(5, 40, &[]).expect("preview");
        // Preview exposes the accepted source rect for the snap-back path.
        assert_eq!(preview.source_rect, capture.source_rect);
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let operation = &plan.dispatch.operation;
        assert_eq!(preview.target_leaf, operation.target_leaf);
        assert_eq!(preview.target_window, operation.target_window);
        assert_eq!(preview.side, operation.side);
        assert_eq!(preview.axis, operation.axis);
        assert_eq!(preview.before, operation.before);
        assert_eq!(preview.wrap, operation.wrap);
        assert_eq!(preview.target_group, operation.target_group);
        assert_eq!(preview.insertion_index, operation.insertion_index);
        // Complete geometry: every tiled window in the domain, positive,
        // contained, disjoint, domain-scoped.
        let windows = plan
            .desired_snapshot
            .windows
            .iter()
            .filter(|l| l.output.0 == "out-1")
            .count();
        assert_eq!(plan.desired_geometry.len(), windows);
        assert_eq!(windows, 3);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        for (index, entry) in plan.desired_geometry.iter().enumerate() {
            assert_eq!(entry.output.0, "out-1");
            assert_eq!(entry.workspace.0, "ws-1");
            assert!(entry.rect.w > 0 && entry.rect.h > 0);
            assert!(contained(&entry.rect, &bounds), "escapes work area");
            for other in plan.desired_geometry.iter().skip(index + 1) {
                assert!(!intersects(&entry.rect, &other.rect), "overlap");
            }
        }
        commit_drag(&mut session, &plan, "corr-drag-1");
        assert_tree_invariants(&session);
    }

    #[test]
    fn drag_begin_refusals_fail_closed() {
        // Unknown window.
        let mut session = session_two();
        let before = session.snapshot();
        assert_eq!(
            session.begin_drag(&WindowId("win-9".to_owned()), &obs_for(&session)),
            Err(ProposeError::Refused(RefusalKind::UnknownWindow))
        );
        // Non-focused tiled window.
        assert_eq!(
            session.begin_drag(&WindowId("win-1".to_owned()), &obs_for(&session)),
            Err(ProposeError::Refused(RefusalKind::FocusMismatch))
        );
        // Malformed id.
        assert_eq!(
            session.begin_drag(&WindowId(String::new()), &obs_for(&session)),
            Err(ProposeError::Refused(RefusalKind::MalformedInput))
        );
        // Active drag blocks a second begin.
        begin_focused(&mut session, "win-2");
        assert_eq!(
            session.begin_drag(&WindowId("win-2".to_owned()), &obs_for(&session)),
            Err(ProposeError::PendingExists)
        );
        session.cancel_drag();
        // Reconciler pending blocks begin.
        let rev = session.accepted_revision();
        let mut windows = obs_windows(&session);
        windows.push(ObservedWindow {
            window: WindowId("win-3".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        session
            .propose(
                &SessionCommand::Admit {
                    window: WindowId("win-3".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                    exceptions: ExceptionFlags::none(),
                    exception_behavior: None,
                    placement_bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 100,
                        h: 50,
                    },
                    suppress_fixed_float: false,
                },
                &SessionObservation {
                    observation: Observation::new(owner(), generation(), rev, 1),
                    windows,
                },
                &corr("corr-pending"),
                &LifecycleCapabilities::full(),
            )
            .expect("pending admit");
        assert_eq!(
            session.begin_drag(&WindowId("win-2".to_owned()), &obs_for(&session)),
            Err(ProposeError::PendingExists)
        );
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_drag());
    }

    #[test]
    fn drag_begin_refuses_exception_window() {
        let mut session = session_two();
        admit_exception(&mut session, "win-x");
        assert_eq!(
            session.begin_drag(&WindowId("win-x".to_owned()), &obs_for(&session)),
            Err(ProposeError::Refused(RefusalKind::NotTiled))
        );
        assert!(!session.has_drag());
    }

    #[test]
    fn drag_preview_refusals_fail_closed() {
        let mut session = session_two();
        // No active drag.
        assert_eq!(
            session.preview_drag(10, 10, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedInput))
        );
        let before = session.snapshot();
        begin_focused(&mut session, "win-2");
        // Self drop carries no structural meaning.
        assert_eq!(
            session.preview_drag(90, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::Unchanged))
        );
        // Target center is the source stack fact with no portable topology:
        // preview fails closed as explicit unsupported stack behavior.
        assert_eq!(
            session.preview_drag(30, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        // Outside the work area is cross-domain.
        assert_eq!(
            session.preview_drag(200, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch))
        );
        assert_eq!(
            session.preview_drag(-1, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch))
        );
        // Refusals stage nothing and mutate nothing.
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_pending());
        assert!(session.has_drag());
    }

    #[test]
    fn drag_noop_insert_refuses_unchanged() {
        // (75, 40) is source-classified Center on leaf-win-2 (middle thirds):
        // portable topology has no stacks, so preview and release fail closed
        // as explicit unsupported stack behavior with no plan.
        let mut session = session_three();
        let before = session.snapshot();
        begin_focused(&mut session, "win-3");
        assert_eq!(
            session.preview_drag(75, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        // Preview stages nothing and keeps the transient drag.
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_pending());
        assert!(!session.has_pending_desired());
        assert!(session.has_drag());
        assert_eq!(
            session.drop_drag(
                75,
                40,
                &obs_for(&session),
                &corr("corr-drag-1"),
                &DragCapabilities::full(),
            ),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        // Center refusal plans no topology and stages no pending.
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert!(!session.has_pending_desired());
    }

    #[test]
    fn drag_cancel_and_invalid_release_snap_back() {
        let mut session = session_two();
        assert_eq!(session.cancel_drag(), None);
        let capture = begin_focused(&mut session, "win-2");
        let snap = session.cancel_drag().expect("snap back");
        assert_eq!(snap.domain, capture.domain);
        assert_eq!(snap.source_leaf, capture.source_leaf);
        assert_eq!(snap.source_window, capture.source_window);
        assert_eq!(snap.source_rect, capture.source_rect);
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-1"), leaf("leaf-win-2")]
        );
        // Invalid self releases snap back with no reconciler use.
        begin_focused(&mut session, "win-2");
        match session
            .drop_drag(
                90,
                40,
                &obs_for(&session),
                &corr("corr-drag-1"),
                &DragCapabilities::full(),
            )
            .expect("self release")
        {
            DragRelease::SnapBack(snap) => assert_eq!(snap.source_rect, capture.source_rect),
            DragRelease::Planned(_) => panic!("self must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        // Center (source stack fact, portable topology excludes stacks) fails
        // closed as explicit unsupported stack behavior with no DragPlan and
        // no topology/focus commit, even with full capabilities.
        begin_focused(&mut session, "win-2");
        assert_eq!(
            session.drop_drag(
                30,
                40,
                &obs_for(&session),
                &corr("corr-drag-2"),
                &DragCapabilities::full(),
            ),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-1"), leaf("leaf-win-2")]
        );
        // Center refusal이에 clears the transient drag; a second center
        // release without a fresh capture is malformed.
        assert_eq!(
            session.drop_drag(
                30,
                40,
                &obs_for(&session),
                &corr("corr-drag-3"),
                &DragCapabilities::full(),
            ),
            Err(ProposeError::Refused(RefusalKind::MalformedInput))
        );
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-1"), leaf("leaf-win-2")]
        );
    }

    #[test]
    fn drag_drop_stale_capability_and_pending_refusals() {
        // Stale observation diverges through the shared slot.
        let mut session = session_two();
        begin_focused(&mut session, "win-2");
        let mut stale = obs_for(&session);
        stale.observation = Observation::new(owner(), generation(), 9, 9);
        assert_eq!(
            session.drop_drag(
                10,
                5,
                &stale,
                &corr("corr-drag-1"),
                &DragCapabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
        );
        assert_eq!(
            session.status().state,
            crate::reconcile::StateKind::Divergent
        );
        assert!(!session.has_drag());
        // Missing capability snaps back and clears the capture.
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        let before = session.snapshot();
        let revision = session.accepted_revision();
        match session
            .drop_drag(
                10,
                5,
                &obs_for(&session),
                &corr("corr-drag-1"),
                &DragCapabilities::none(),
            )
            .expect("unsupported capability snaps back")
        {
            DragRelease::SnapBack(snap) => {
                assert_eq!(snap.source_rect, capture.source_rect);
            }
            DragRelease::Planned(_) => panic!("unsupported capability must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.accepted_revision(), revision);
        // Lifecycle proposals are blocked while a transient drag is active so
        // they cannot stale the capture.
        let mut session = session_two();
        begin_focused(&mut session, "win-2");
        let rev = session.accepted_revision();
        let mut windows = obs_windows(&session);
        windows.push(ObservedWindow {
            window: WindowId("win-3".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        assert_eq!(
            session.propose(
                &SessionCommand::Admit {
                    window: WindowId("win-3".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                    exceptions: ExceptionFlags::none(),
                    exception_behavior: None,
                    placement_bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 100,
                        h: 50,
                    },
                    suppress_fixed_float: false,
                },
                &SessionObservation {
                    observation: Observation::new(owner(), generation(), rev, 1),
                    windows,
                },
                &corr("corr-pending"),
                &LifecycleCapabilities::full(),
            ),
            Err(ProposeError::PendingExists)
        );
        assert!(session.has_drag());
        assert!(!session.has_pending());
    }

    #[test]
    fn drag_ack_verify_commit_and_divergence() {
        // Happy path.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        commit_drag(&mut session, &plan, "corr-drag-1");
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-3"), leaf("leaf-win-1"), leaf("leaf-win-2")]
        );
        // Refused capability ack diverges.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        assert_eq!(
            session.acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                plan.dispatch.base_revision,
                AckOutcome::RefusedCapability,
            )),
            Err(crate::reconcile::AckError::Diverged(
                DivergenceKind::CapabilityRefused
            ))
        );
        assert!(!session.has_pending_desired());
        // Partial ack diverges.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        assert_eq!(
            session.acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                plan.dispatch.base_revision,
                AckOutcome::PartialApplication,
            )),
            Err(crate::reconcile::AckError::Diverged(
                DivergenceKind::PartialApplication
            ))
        );
        // Mismatched verified operation diverges without commit.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let base = plan.dispatch.base_revision;
        session
            .acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        let mut bad_operation = plan.dispatch.operation.clone();
        bad_operation.insertion_index += 1;
        assert_eq!(
            session.verify_drag(&DragPostObservation::new(
                Observation::new(owner(), generation(), base, 9),
                corr("corr-drag-1"),
                true,
                plan.dispatch.preconditions.clone(),
                bad_operation,
            )),
            Err(crate::reconcile::VerifyError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        assert_eq!(
            leaves_of(&session),
            vec![leaf("leaf-win-1"), leaf("leaf-win-2"), leaf("leaf-win-3")]
        );
        // Unverified diverges without commit.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let base = plan.dispatch.base_revision;
        session
            .acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        assert_eq!(
            session.verify_drag(&DragPostObservation::new(
                Observation::new(owner(), generation(), base, 9),
                corr("corr-drag-1"),
                false,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            )),
            Err(crate::reconcile::VerifyError::Diverged(
                DivergenceKind::PostconditionUnverified
            ))
        );
        // Cross-kind verify while drag pending diverges.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let base = plan.dispatch.base_revision;
        session
            .acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        assert_eq!(
            session.verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), base, 9),
                corr("corr-drag-1"),
                true,
                Vec::new(),
                crate::contract::LifecycleOperation::RemoveDeferred {
                    window: WindowId("win-3".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                },
            )),
            Err(crate::reconcile::VerifyError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
    }

    #[test]
    fn drag_deterministic_replay() {
        let replay = || {
            let mut session = session_three();
            begin_focused(&mut session, "win-3");
            let preview = session.preview_drag(5, 40, &[]).expect("preview");
            let release = session
                .drop_drag(
                    5,
                    40,
                    &obs_for(&session),
                    &corr("corr-drag-1"),
                    &DragCapabilities::full(),
                )
                .expect("drop");
            (preview, release)
        };
        let (first_preview, first_release) = replay();
        let (second_preview, second_release) = replay();
        assert_eq!(first_preview, second_preview);
        assert_eq!(first_release, second_release);
    }

    #[test]
    fn drag_bounded_invariant_loop() {
        let mut session =
            Session::new(owner(), generation(), 0, 7, vec![domain(), domain_two()]).expect("new");
        admit(&mut session, "win-1", true);
        admit(&mut session, "win-2", true);
        admit(&mut session, "win-3", false);
        let before = session.snapshot();
        let capture = begin_focused(&mut session, "win-3");
        let mut ok = 0u32;
        let mut refused = 0u32;
        for x in (0..120).step_by(10) {
            for y in (0..80).step_by(10) {
                match session.preview_drag(x, y, &[]) {
                    Ok(preview) => {
                        ok += 1;
                        assert_eq!(preview.domain, capture.domain);
                        assert_eq!(preview.source_leaf, capture.source_leaf);
                        assert_ne!(preview.target_leaf, capture.source_leaf);
                        assert!(preview.target_rect.w > 0 && preview.target_rect.h > 0);
                    }
                    Err(ProposeError::Refused(_)) => refused += 1,
                    Err(ProposeError::Diverged(reason)) => {
                        panic!("preview must not diverge: {reason:?}")
                    }
                    Err(ProposeError::PendingExists) => panic!("preview holds no slot"),
                }
                // No mutation before commit: exactly-once leaves/windows,
                // valid trees/shares, positive areas, domain isolation.
                assert_eq!(session.snapshot(), before);
                assert!(!session.has_pending());
                assert!(!session.has_pending_desired());
                assert_tree_invariants(&session);
                let isolated = session
                    .snapshot()
                    .domains
                    .into_iter()
                    .find(|d| d.output.0 == "out-2")
                    .expect("second domain");
                assert!(isolated.tree.is_none(), "domain isolation");
            }
        }
        assert!(ok > 0, "loop must hit valid edges and stack centers");
        assert!(refused > 0, "loop must hit self/gap/out-of-area");
        session.cancel_drag();
        assert_eq!(session.snapshot(), before);
    }

    #[test]
    fn drag_blocks_all_proposals_while_active() {
        use crate::contract::{FocusCapabilities, ResizeCapabilities};
        use crate::directional::{Capabilities, Direction};
        let mut session = session_three();
        let before = session.snapshot();
        begin_focused(&mut session, "win-3");
        let obs = obs_for(&session);
        // Lifecycle.
        let mut windows = obs_windows(&session);
        windows.push(ObservedWindow {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        assert_eq!(
            session.propose(
                &SessionCommand::Admit {
                    window: WindowId("win-9".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                    exceptions: ExceptionFlags::none(),
                    exception_behavior: None,
                    placement_bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 100,
                        h: 50
                    },
                    suppress_fixed_float: false,
                },
                &SessionObservation {
                    observation: Observation::new(
                        owner(),
                        generation(),
                        session.accepted_revision(),
                        1
                    ),
                    windows,
                },
                &corr("corr-block-1"),
                &LifecycleCapabilities::full(),
            ),
            Err(ProposeError::PendingExists)
        );
        // Move.
        assert_eq!(
            session.propose_move(
                &domain_key(),
                &WindowId("win-3".to_owned()),
                Direction::Left,
                &obs,
                &corr("corr-block-2"),
                &Capabilities::full(),
            ),
            Err(ProposeError::PendingExists)
        );
        // Focus.
        assert_eq!(
            session.propose_focus(
                &domain_key(),
                &WindowId("win-3".to_owned()),
                Direction::Left,
                &obs,
                &corr("corr-block-3"),
                &FocusCapabilities::full(),
            ),
            Err(ProposeError::PendingExists)
        );
        // Resize.
        assert_eq!(
            session.propose_resize(
                &domain_key(),
                &WindowId("win-3".to_owned()),
                Direction::Left,
                crate::contract::ResizeMode::Outwards,
                0,
                &obs,
                &corr("corr-block-4"),
                &ResizeCapabilities::full(),
            ),
            Err(ProposeError::PendingExists)
        );
        assert!(session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        session.cancel_drag();
    }

    #[test]
    fn drag_transient_cleared_on_terminal_paths() {
        // Adapter loss clears the transient drag together with pending desired.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        assert!(session.has_drag());
        session.note_adapter_loss();
        assert!(!session.has_drag());
        assert!(!session.has_pending_desired());
        assert_eq!(
            session.status().state,
            crate::reconcile::StateKind::Divergent
        );
        // Acknowledgement divergence clears pending desired (drag already
        // cleared by a successful drop).
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        assert!(!session.has_drag());
        assert!(session.has_pending());
        assert_eq!(
            session.acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                plan.dispatch.base_revision,
                AckOutcome::RefusedCapability,
            )),
            Err(crate::reconcile::AckError::Diverged(
                DivergenceKind::CapabilityRefused
            ))
        );
        assert!(!session.has_drag());
        assert!(!session.has_pending_desired());
        // Wrong-kind verification diverges fail-closed and clears staging.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let base = plan.dispatch.base_revision;
        session
            .acknowledge(&AdapterAck::new(
                corr("corr-drag-1"),
                owner(),
                generation(),
                base,
                AckOutcome::Accepted,
            ))
            .expect("ack");
        assert_eq!(
            session.verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), base, 9),
                corr("corr-drag-1"),
                true,
                Vec::new(),
                crate::contract::LifecycleOperation::RemoveDeferred {
                    window: WindowId("win-3".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                },
            )),
            Err(crate::reconcile::VerifyError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        assert!(!session.has_drag());
        assert!(!session.has_pending_desired());
    }

    #[test]
    fn drag_preview_validates_full_capture_and_drop_snaps_back_on_stale() {
        // Focus drift stales the capture: preview refuses, drop snaps back.
        let mut session = session_three();
        let preview_ok = {
            begin_focused(&mut session, "win-3");
            session.preview_drag(5, 40, &[]).expect("preview")
        };
        assert!(preview_ok.proposed_rect.w > 0);
        session.focused_leaf = Some(leaf("leaf-win-1"));
        assert_eq!(
            session.preview_drag(5, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
        );
        match session
            .drop_drag(
                5,
                40,
                &obs_for(&session),
                &corr("corr-drag-1"),
                &DragCapabilities::full(),
            )
            .expect("stale capture snaps back")
        {
            DragRelease::SnapBack(_) => {}
            DragRelease::Planned(_) => panic!("stale must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        // Generation drift stales the capture the same way.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        session.preview_drag(5, 40, &[]).expect("preview");
        session.drag.as_mut().expect("drag").generation =
            crate::ids::GenerationId::parse("gen-2").expect("valid");
        assert_eq!(
            session.preview_drag(5, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
        );
        match session
            .drop_drag(
                5,
                40,
                &obs_for(&session),
                &corr("corr-drag-2"),
                &DragCapabilities::full(),
            )
            .expect("stale generation snaps back")
        {
            DragRelease::SnapBack(_) => {}
            DragRelease::Planned(_) => panic!("stale must snap back"),
        }
        assert!(!session.has_drag());
        // Revision drift stales the capture the same way.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        session.drag.as_mut().expect("drag").revision += 100;
        assert_eq!(
            session.preview_drag(5, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
        );
        // Exception-set drift stales the capture.
        let mut session = session_three();
        begin_focused(&mut session, "win-3");
        session.exceptions.insert(
            WindowId("win-x".to_owned()),
            ExceptionRecord {
                window: WindowId("win-x".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                flags: ExceptionFlags::none(),
                floating_geometry: None,
            },
        );
        assert_eq!(
            session.preview_drag(5, 40, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
        );
        session.cancel_drag();
    }

    #[test]
    fn drag_preview_proposed_rect_matches_final_geometry() {
        let mut session = session_three();
        let capture = begin_focused(&mut session, "win-3");
        let preview = session.preview_drag(5, 40, &[]).expect("preview");
        // The proposed rect is the projected desired source rect, not a claim
        // that the target current rect alone is the preview.
        assert!(preview.proposed_rect.w > 0 && preview.proposed_rect.h > 0);
        assert!(contained(&preview.proposed_rect, &capture.work_area));
        let plan = drop_planned(&mut session, 5, 40, "corr-drag-1");
        let Some(final_rect) = plan
            .desired_geometry
            .iter()
            .find(|g| g.leaf == capture.source_leaf)
            .map(|g| g.rect)
        else {
            panic!("final geometry must cover the source");
        };
        assert_eq!(preview.proposed_rect, final_rect);
        assert_eq!(preview.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(preview.wrap, plan.dispatch.operation.wrap);
        assert_eq!(
            preview.insertion_index,
            plan.dispatch.operation.insertion_index
        );
        commit_drag(&mut session, &plan, "corr-drag-1");
    }

    #[test]
    fn drag_preview_uses_observed_size_hints_like_drop() {
        // Two-wide H[win-1, win-2] 120x80 shares [1,1]: plain halves are 60.
        // A min_w 80 hint on win-1 takes sibling slack, so the moved source
        // (win-2) previews at 40 wide with hints and 60 without.
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        let plain = session.preview_drag(5, 40, &[]).expect("plain preview");
        assert_eq!(plain.proposed_rect.w, 60);
        let mut hinted_windows = obs_windows(&session);
        for entry in hinted_windows.iter_mut() {
            if entry.window.0 == "win-1" {
                entry.hints = crate::size_hints::WindowSizeHints {
                    min_w: Some(80),
                    ..crate::size_hints::WindowSizeHints::none()
                };
            }
        }
        let hinted = session
            .preview_drag(5, 40, &hinted_windows)
            .expect("hinted preview");
        assert_eq!(hinted.target_leaf, plain.target_leaf);
        assert_eq!(hinted.side, plain.side);
        assert_eq!(hinted.proposed_rect.w, 40);
        assert_ne!(hinted.proposed_rect, plain.proposed_rect);
        // Same hints through drop agree structurally and geometrically.
        let mut observation = obs_for(&session);
        observation.windows = hinted_windows;
        match session
            .drop_drag(
                5,
                40,
                &observation,
                &corr("corr-drag-hints"),
                &DragCapabilities::full(),
            )
            .expect("hinted drop")
        {
            DragRelease::Planned(plan) => {
                assert_eq!(hinted.target_leaf, plan.dispatch.operation.target_leaf);
                assert_eq!(hinted.side, plan.dispatch.operation.side);
                assert_eq!(hinted.wrap, plan.dispatch.operation.wrap);
                assert_eq!(
                    hinted.insertion_index,
                    plan.dispatch.operation.insertion_index
                );
                let Some(final_rect) = plan
                    .desired_geometry
                    .iter()
                    .find(|g| g.leaf == capture.source_leaf)
                    .map(|g| g.rect)
                else {
                    panic!("final geometry must cover the source");
                };
                assert_eq!(hinted.proposed_rect, final_rect);
                assert_eq!(final_rect.w, 40);
            }
            DragRelease::SnapBack(_) => panic!("hinted drop must plan"),
        }
    }

    #[test]
    fn drag_prior_carry_restores_sticky_edge_across_clones() {
        // Inner V group gap row: (gx, gy) is GroupEdge Left; sticky_x 50px in
        // is Left only with the Left prior (normal 32px depth misses it).
        let mut session = gap_session_nested();
        let (inner, leaf_map) = inner_v_layout(&session);
        let cx = inner.rect.x + inner.rect.w / 2;
        let mut gy_opt = None;
        for y in inner.rect.y..inner.rect.y + inner.rect.h {
            if !leaf_map.values().any(|r| contains_point(r, cx, y))
                && y - inner.rect.y >= 32
                && inner.rect.y + inner.rect.h - y > 32
            {
                gy_opt = Some(y);
                break;
            }
        }
        let gy = gy_opt.expect("inner interior gap point");
        let gx = inner.rect.x + 5;
        let sticky_x = inner.rect.x + 50;
        assert!(!leaf_map.values().any(|r| contains_point(r, gx, gy)));
        assert!(!leaf_map.values().any(|r| contains_point(r, sticky_x, gy)));
        let _capture = begin_focused(&mut session, "win-1");
        let edge = session.preview_drag(gx, gy, &[]).expect("edge preview");
        assert_eq!(edge.side, DragSide::Left);
        assert!(edge.prior.is_some());
        let carried = edge.hover_prior();
        // Simulate single-shot clone loss: clear the stored hover.
        session.drag.as_mut().expect("drag").prior = None;
        // Without the carry the sticky probe misses the Left edge.
        let missed = session.preview_drag(sticky_x, gy, &[]);
        assert!(
            !matches!(
                missed,
                Ok(ref preview) if preview.side == DragSide::Left
                    && preview.target_leaf == inner.id
            ),
            "cleared prior must not resolve via sticky 80"
        );
        // A mismatched carry (wrong revision) is ignored advisory-style.
        let mut bad = carried.clone();
        bad.revision += 1;
        assert!(!session.carry_drag_prior(&bad));
        // The exact carry restores the sticky edge for preview and drop.
        assert!(session.carry_drag_prior(&carried));
        let sticky = session
            .preview_drag(sticky_x, gy, &[])
            .expect("sticky preview");
        assert_eq!(sticky.side, DragSide::Left);
        assert_eq!(sticky.target_leaf, inner.id);
        let plan = drop_planned(&mut session, sticky_x, gy, "corr-sticky-carry");
        assert_eq!(sticky.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(sticky.side, plan.dispatch.operation.side);
        commit_drag(&mut session, &plan, "corr-sticky-carry");
    }

    #[test]
    fn drag_prior_carry_validates_source_and_needs_no_drag() {
        let mut session = session_two();
        // No active drag: carry is ignored.
        let dummy = DragHoverPrior {
            domain: domain_key(),
            source_leaf: leaf("leaf-win-2"),
            source_window: WindowId("win-2".to_owned()),
            revision: session.accepted_revision(),
            prior: None,
            prior_domain: domain_key(),
        };
        assert!(!session.carry_drag_prior(&dummy));
        // Wrong source leaf is ignored without mutation.
        let _capture = begin_focused(&mut session, "win-2");
        let edge = session.preview_drag(5, 40, &[]).expect("preview");
        let mut wrong_leaf = edge.hover_prior();
        wrong_leaf.source_leaf = leaf("leaf-win-1");
        assert!(!session.carry_drag_prior(&wrong_leaf));
        // Exact carry applies (here a window-edge hover carries None).
        assert!(session.carry_drag_prior(&edge.hover_prior()));
        session.cancel_drag();
    }

    #[test]
    fn drag_cosmic_window_zones_stack_center_and_edge_by_half_distance() {
        // COSMIC window zones via cosmic_v1: rounded thirds stack; other
        // locations select horizontal only on strict less (ties vertical).
        // Outside the rect yields None.
        let normal = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 90,
        };
        assert_eq!(drag_edge_for(&normal, 60, 45), Some(DragSide::Center));
        assert_eq!(drag_edge_for(&normal, 5, 45), Some(DragSide::Left));
        assert_eq!(drag_edge_for(&normal, 115, 45), Some(DragSide::Right));
        assert_eq!(drag_edge_for(&normal, 60, 4), Some(DragSide::Top));
        assert_eq!(drag_edge_for(&normal, 60, 86), Some(DragSide::Bottom));
        assert_eq!(drag_edge_for(&normal, 200, 45), None);
        // Rounded thirds: 1px width has third 0 so the full width is middle;
        // 2px has third 1 with an empty middle, so edges win (ties vertical).
        let narrow = Rect {
            x: 10,
            y: 10,
            w: 1,
            h: 10,
        };
        assert_eq!(drag_edge_for(&narrow, 10, 15), Some(DragSide::Center));
        let tiny = Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        };
        assert_eq!(drag_edge_for(&tiny, 0, 0), Some(DragSide::Top));
        assert_eq!(drag_edge_for(&tiny, 1, 1), Some(DragSide::Top));
        // Degenerate rects classify to nothing.
        let flat = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 10,
        };
        assert_eq!(drag_edge_for(&flat, 0, 5), None);
    }

    #[test]
    fn drag_flag_failures_refuse_consistently() {
        // Flagged source observation at begin refuses without staging.
        let mut session = session_two();
        let before = session.snapshot();
        let mut flagged = obs_for(&session);
        for entry in flagged.windows.iter_mut() {
            if entry.window.0 == "win-2" {
                entry.floating = true;
            }
        }
        assert!(matches!(
            session.begin_drag(&WindowId("win-2".to_owned()), &flagged),
            Err(ProposeError::Refused(_))
        ));
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        // Flagged observation at drop snaps back without diverging and clears
        // the capture.
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        let before = session.snapshot();
        let revision = session.accepted_revision();
        let mut flagged_drop = obs_for(&session);
        for entry in flagged_drop.windows.iter_mut() {
            if entry.window.0 == "win-2" {
                entry.floating = true;
            }
        }
        match session
            .drop_drag(
                10,
                5,
                &flagged_drop,
                &corr("corr-drag-1"),
                &DragCapabilities::full(),
            )
            .expect("flagged observation snaps back")
        {
            DragRelease::SnapBack(snap) => {
                assert_eq!(snap.source_rect, capture.source_rect);
            }
            DragRelease::Planned(_) => panic!("flagged observation must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.accepted_revision(), revision);
    }

    #[test]
    fn drag_malformed_observation_snaps_back() {
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        let before = session.snapshot();
        let revision = session.accepted_revision();
        let mut malformed = obs_for(&session);
        malformed.windows[0].window = WindowId(String::new());
        match session
            .drop_drag(
                10,
                5,
                &malformed,
                &corr("corr-drag-malformed"),
                &DragCapabilities::full(),
            )
            .expect("malformed observation snaps back")
        {
            DragRelease::SnapBack(snap) => {
                assert_eq!(snap.source_rect, capture.source_rect);
                assert_eq!(snap.source_leaf, capture.source_leaf);
                assert_eq!(snap.source_window, capture.source_window);
            }
            DragRelease::Planned(_) => panic!("malformed observation must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.accepted_revision(), revision);
    }

    #[test]
    fn drag_unsupported_capability_snaps_back() {
        let mut session = session_two();
        let capture = begin_focused(&mut session, "win-2");
        let before = session.snapshot();
        let revision = session.accepted_revision();
        match session
            .drop_drag(
                10,
                5,
                &obs_for(&session),
                &corr("corr-drag-nocap"),
                &DragCapabilities::none(),
            )
            .expect("unsupported capability snaps back")
        {
            DragRelease::SnapBack(snap) => {
                assert_eq!(snap.source_rect, capture.source_rect);
                assert_eq!(snap.source_leaf, capture.source_leaf);
                assert_eq!(snap.source_window, capture.source_window);
            }
            DragRelease::Planned(_) => panic!("unsupported capability must snap back"),
        }
        assert!(!session.has_drag());
        assert!(!session.has_pending());
        assert_eq!(session.snapshot(), before);
        assert_eq!(session.accepted_revision(), revision);
    }

    fn gap_domain() -> OutputDomain {
        OutputDomain {
            id: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            gap: 8,
            adjacent: BTreeMap::new(),
        }
    }

    fn gap_session_two() -> Session {
        let mut session =
            Session::new(owner(), generation(), 0, 7, vec![gap_domain()]).expect("new");
        admit(&mut session, "win-1", true);
        admit(&mut session, "win-2", true);
        session
    }

    fn gap_session_nested() -> Session {
        // Root H[win-1, V[win-2, win-3]] with gaps; focus win-1 for an
        // outside-group source. Admit win-3 vertical to nest under win-2.
        let mut session =
            Session::new(owner(), generation(), 0, 7, vec![gap_domain()]).expect("new");
        admit(&mut session, "win-1", true);
        admit(&mut session, "win-2", true);
        // Focus is win-2; admit win-3 tall to nest V[win-2, win-3].
        admit(&mut session, "win-3", false);
        // Refocus to win-1 as the outside source via focus moves.
        let key = domain_key();
        // Focus win-1 by moving focus left twice (deterministic project
        // fallback may vary; instead drive focus through movement commits).
        // Simpler: directly set authoritative focus in-test (same module).
        let leaf_win1 = session
            .windows
            .iter()
            .find(|(_, l)| l.window == WindowId("win-1".to_owned()))
            .map(|(_, l)| l.leaf.clone())
            .expect("win-1 leaf");
        session.focused_domain = Some(key);
        session.focused_leaf = Some(leaf_win1);
        session
    }

    fn leaf_rects_of(session: &Session) -> Vec<(NodeId, Rect)> {
        let key = domain_key();
        let tree = session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output == key.output && d.workspace == key.workspace)
            .and_then(|d| d.tree)
            .expect("tree");
        let domain = gap_domain();
        crate::geometry::project(&tree, domain.bounds, domain.gap)
            .expect("project")
            .into_iter()
            .map(|l| (l.leaf, l.rect))
            .collect()
    }

    fn inner_v_layout(session: &Session) -> (GroupLayout, BTreeMap<NodeId, Rect>) {
        let key = domain_key();
        let tree = session
            .snapshot()
            .domains
            .into_iter()
            .find(|d| d.output == key.output && d.workspace == key.workspace)
            .and_then(|d| d.tree)
            .expect("tree");
        let domain = gap_domain();
        let projected =
            crate::geometry::project(&tree, domain.bounds, domain.gap).expect("project");
        let mut leaf_map = BTreeMap::new();
        for l in &projected {
            leaf_map.insert(l.leaf.clone(), l.rect);
        }
        let layouts = drag_group_layouts(&tree, &leaf_map);
        let inner = layouts
            .iter()
            .find(|l| l.axis == Axis::Vertical)
            .expect("inner V group");
        // Clone minimal data (GroupLayout is private; return via tuple of owned values).
        (
            GroupLayout {
                id: inner.id.clone(),
                axis: inner.axis,
                rect: inner.rect,
                child_starts: inner.child_starts.clone(),
            },
            leaf_map,
        )
    }

    #[test]
    fn drag_group_interior_predecessor_agreement_and_invariants() {
        // Nested H[win-1, V[win-2, win-3]] with source win-1 outside the inner
        // V group: inner horizontal gap row at mid-height, away from edge
        // strips, is GroupInterior with source predecessor + min(len, idx+1).
        let mut session = gap_session_nested();
        assert_eq!(leaves_of(&session).len(), 3);
        let before_focus = session.focus();
        let (inner, leaf_map) = inner_v_layout(&session);
        let cx = inner.rect.x + inner.rect.w / 2;
        let mut gy_opt = None;
        for y in inner.rect.y..inner.rect.y + inner.rect.h {
            if !leaf_map.values().any(|r| contains_point(r, cx, y)) {
                // Outside 32px top/bottom strips => interior.
                if y - inner.rect.y >= 32 && inner.rect.y + inner.rect.h - y > 32 {
                    gy_opt = Some(y);
                    break;
                }
            }
        }
        let gy = gy_opt.expect("inner interior gap point");
        let gx = cx;
        let _capture = begin_focused(&mut session, "win-1");
        let preview = session
            .preview_drag(gx, gy, &[])
            .expect("group interior preview");
        assert_eq!(preview.axis, Axis::Vertical);
        assert!(!preview.wrap);
        assert_eq!(preview.target_leaf, inner.id);
        // Predecessor from ordered child starts plus min(len, idx+1).
        let predecessor =
            crate::cosmic_v1::insertion_index_for_offset(&inner.child_starts, i64::from(gy));
        // Post-removal inner len stays 2 (source outside), so slot is min(2, pred+1).
        assert_eq!(preview.insertion_index, 2.min(predecessor + 1));
        let plan = drop_planned(&mut session, gx, gy, "corr-group-interior");
        assert_eq!(preview.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(preview.target_group, plan.dispatch.operation.target_group);
        assert_eq!(
            preview.insertion_index,
            plan.dispatch.operation.insertion_index
        );
        assert_eq!(preview.wrap, plan.dispatch.operation.wrap);
        assert_eq!(
            (
                Some(plan.desired_focus_domain.clone()),
                Some(plan.desired_focus_leaf.clone())
            ),
            before_focus
        );
        for domain in &plan.desired_snapshot.domains {
            if let Some(tree) = &domain.tree {
                let mut seen = BTreeSet::new();
                assert!(validate_node_shares(tree, &mut seen));
            }
        }
        assert!(geometry_covers_affected(
            &plan.desired_geometry,
            &session.windows,
            std::slice::from_ref(&domain_key()),
        ));
        commit_drag(&mut session, &plan, "corr-group-interior");
        assert_eq!(session.focus(), before_focus);
    }

    #[test]
    fn drag_group_edge_perpendicular_wraps_and_sticky_prior() {
        // Inner V group: left-strip (32px) gap-row point is GroupEdge Left
        // (axis Horizontal, perpendicular to inner Vertical) and wraps.
        let mut session = gap_session_nested();
        let (inner, leaf_map) = inner_v_layout(&session);
        let cx = inner.rect.x + inner.rect.w / 2;
        let mut gy_opt = None;
        for y in inner.rect.y..inner.rect.y + inner.rect.h {
            if !leaf_map.values().any(|r| contains_point(r, cx, y)) {
                gy_opt = Some(y);
                break;
            }
        }
        let gy = gy_opt.expect("inner gap row");
        let gx = inner.rect.x + 5;
        assert!(
            !leaf_map.values().any(|r| contains_point(r, gx, gy)),
            "left-strip gap point must be group-only"
        );
        let _capture = begin_focused(&mut session, "win-1");
        let preview = session
            .preview_drag(gx, gy, &[])
            .expect("group edge preview");
        assert_eq!(preview.side, DragSide::Left);
        assert_eq!(preview.axis, Axis::Horizontal);
        assert!(preview.wrap);
        assert_eq!(preview.target_leaf, inner.id);
        // Sticky prior: the same gap row 50px right of the left edge is
        // outside the normal 32px strip but the prior is a different edge
        // now (Left), so a top-strip check is not applicable; instead verify
        // the prior persists by re-hitting the same edge further along the row.
        let sticky = session
            .preview_drag(gx, gy + 2, &[])
            .expect("sticky edge preview");
        assert_eq!(sticky.side, DragSide::Left);
        assert_eq!(sticky.target_leaf, preview.target_leaf);
        let plan = drop_planned(&mut session, gx, gy + 2, "corr-group-edge");
        assert_eq!(sticky.target_leaf, plan.dispatch.operation.target_leaf);
        assert_eq!(sticky.wrap, plan.dispatch.operation.wrap);
        assert_eq!(
            sticky.insertion_index,
            plan.dispatch.operation.insertion_index
        );
        commit_drag(&mut session, &plan, "corr-group-edge");
    }

    #[test]
    fn drag_group_edge_same_axis_nary_first_last_placement() {
        // Direct placement: N-ary H group [a,b,c], mover outside. Left inserts
        // first, Right inserts last, via proportional shares.
        let group = Node::Group {
            id: NodeId("g".to_owned()),
            axis: Axis::Horizontal,
            children: vec![
                Node::Leaf {
                    id: NodeId("a".to_owned()),
                },
                Node::Leaf {
                    id: NodeId("b".to_owned()),
                },
                Node::Leaf {
                    id: NodeId("c".to_owned()),
                },
            ],
            shares: vec![1, 1, 1],
        };
        let tree = Node::Group {
            id: NodeId("root".to_owned()),
            axis: Axis::Vertical,
            children: vec![
                group,
                Node::Leaf {
                    id: NodeId("m".to_owned()),
                },
            ],
            shares: vec![1, 1],
        };
        let left = apply_group_edge_placement(
            &CosmicV1Policy,
            &tree,
            &NodeId("m".to_owned()),
            &NodeId("g".to_owned()),
            DragSide::Left,
            9,
        )
        .expect("left placement");
        assert!(!left.wrap);
        assert_eq!(left.target_group, NodeId("g".to_owned()));
        assert_eq!(left.insertion_index, 0);
        assert!(left.new_group.is_none());
        let right = apply_group_edge_placement(
            &CosmicV1Policy,
            &tree,
            &NodeId("m".to_owned()),
            &NodeId("g".to_owned()),
            DragSide::Right,
            9,
        )
        .expect("right placement");
        assert!(!right.wrap);
        assert_eq!(right.target_group, NodeId("g".to_owned()));
        assert_eq!(right.insertion_index, 3);
        // Perpendicular Top wraps the whole group.
        let top = apply_group_edge_placement(
            &CosmicV1Policy,
            &tree,
            &NodeId("m".to_owned()),
            &NodeId("g".to_owned()),
            DragSide::Top,
            9,
        )
        .expect("top placement");
        assert!(top.wrap);
        assert!(top.new_group.is_some());
        // Interior predecessor 0 gives min(3,1)=1 after removing an outside
        // mover (len stays 3).
        let interior = apply_group_interior_placement(
            &CosmicV1Policy,
            &tree,
            &NodeId("m".to_owned()),
            &NodeId("g".to_owned()),
            0,
            9,
        )
        .expect("interior placement");
        assert!(!interior.wrap);
        assert_eq!(interior.target_group, NodeId("g".to_owned()));
        assert_eq!(interior.insertion_index, 1);
    }

    #[test]
    fn drag_group_stale_capture_rejects_and_center_unsupported() {
        let mut session = gap_session_two();
        let _capture = begin_focused(&mut session, "win-2");
        // Stale focus drift fails closed: preview refuses, drop snaps back.
        session.focused_leaf = Some(NodeId("leaf-win-1".to_owned()));
        assert_eq!(
            session.preview_drag(400, 300, &[]),
            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
        );
        match session
            .drop_drag(
                400,
                300,
                &obs_for(&session),
                &corr("corr-stale"),
                &DragCapabilities::full(),
            )
            .expect("stale snaps back")
        {
            DragRelease::SnapBack(_) => {}
            DragRelease::Planned(_) => panic!("stale must snap back"),
        }
        assert!(!session.has_drag());
        // Fresh capture: window center is explicit unsupported with no plan.
        let mut session = gap_session_two();
        let _capture = begin_focused(&mut session, "win-2");
        let rects = leaf_rects_of(&session);
        let (_, first) = rects.first().expect("leaf");
        let cx = first.x + first.w / 2;
        let cy = first.y + first.h / 2;
        // Center of win-1 (non-source) names the stack fact.
        assert_eq!(
            session.preview_drag(cx, cy, &[]),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        assert!(!session.has_pending());
        assert!(session.has_drag());
        session.cancel_drag();
    }

    #[test]
    fn drag_failed_center_preview_clears_group_edge_stickiness() {
        let mut session = gap_session_nested();
        let before = session.snapshot();
        let revision = session.accepted_revision();
        let (inner, leaf_map) = inner_v_layout(&session);
        let cx = inner.rect.x + inner.rect.w / 2;
        let mut gy_opt = None;
        for y in inner.rect.y..inner.rect.y + inner.rect.h {
            if !leaf_map.values().any(|r| contains_point(r, cx, y))
                && y - inner.rect.y >= 32
                && inner.rect.y + inner.rect.h - y > 32
            {
                gy_opt = Some(y);
                break;
            }
        }
        let gy = gy_opt.expect("inner interior gap point");
        let gx = inner.rect.x + 5;
        assert!(
            !leaf_map.values().any(|r| contains_point(r, gx, gy)),
            "left-strip gap point must be group-only"
        );
        let _capture = begin_focused(&mut session, "win-1");
        let edge = session
            .preview_drag(gx, gy, &[])
            .expect("group edge preview");
        assert_eq!(edge.side, DragSide::Left);
        assert_eq!(edge.target_leaf, inner.id);
        let rects = leaf_rects_of(&session);
        let source_leaf = session
            .windows
            .iter()
            .find(|(_, l)| l.window == WindowId("win-1".to_owned()))
            .map(|(_, l)| l.leaf.clone())
            .expect("win-1 leaf");
        let (_, target_rect) = rects
            .iter()
            .find(|(id, _)| *id != source_leaf)
            .expect("non-source leaf");
        let center_x = target_rect.x + target_rect.w / 2;
        let center_y = target_rect.y + target_rect.h / 2;
        assert_eq!(
            session.preview_drag(center_x, center_y, &[]),
            Err(ProposeError::Refused(RefusalKind::UnsupportedCapability))
        );
        let sticky_x = inner.rect.x + 50;
        assert!(
            contains_point(&inner.rect, sticky_x, gy),
            "sticky probe must stay inside the group"
        );
        assert!(
            !leaf_map.values().any(|r| contains_point(r, sticky_x, gy)),
            "sticky probe must be group-only"
        );
        assert_eq!(
            crate::cosmic_v1::classify_group_point(
                &inner.rect,
                &inner.id,
                sticky_x,
                gy,
                Some(&crate::cosmic_v1::PriorGroupEdge {
                    group: inner.id.clone(),
                    edge: DragSide::Left,
                }),
            ),
            Some(DragSide::Left)
        );
        assert_ne!(
            crate::cosmic_v1::classify_group_point(&inner.rect, &inner.id, sticky_x, gy, None),
            Some(DragSide::Left)
        );
        match session.preview_drag(sticky_x, gy, &[]) {
            Ok(preview) => assert_ne!(
                preview.side,
                DragSide::Left,
                "cleared prior must not resolve via sticky 80"
            ),
            Err(ProposeError::Refused(_)) => {}
            Err(other) => panic!("unexpected preview result: {other:?}"),
        }
        assert_eq!(session.snapshot(), before);
        assert!(!session.has_pending());
        assert!(!session.has_pending_desired());
        assert!(session.has_drag());
        assert_eq!(session.accepted_revision(), revision);
        session.cancel_drag();
    }

    #[test]
    fn relocate_domain_is_atomic_on_validation_failure() {
        // Every refusal leaves revision, domains, trees, windows,
        // exceptions, and focus exactly as before.
        let source_domain = domain();
        let other = domain_two();
        let mut session = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![source_domain.clone(), other.clone()],
        )
        .expect("new");
        let before_snapshot = session.snapshot();
        let before_revision = session.accepted_revision();
        let source_key = source_domain.key();
        let other_key = other.key();
        let target = DomainKey {
            output: OutputId("out-9".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        // Workspace mismatch.
        assert!(!session.relocate_domain(
            &source_key,
            &DomainKey {
                output: OutputId("out-9".to_owned()),
                workspace: WorkspaceId("ws-other".to_owned()),
            },
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            },
            0,
        ));
        // Existing target collision.
        assert!(!session.relocate_domain(
            &source_key,
            &other_key,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            },
            0,
        ));
        // Invalid gap and bounds.
        assert!(!session.relocate_domain(
            &source_key,
            &target,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            },
            999,
        ));
        assert!(!session.relocate_domain(
            &source_key,
            &target,
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 80
            },
            0,
        ));
        // Unknown source.
        assert!(!session.relocate_domain(
            &target,
            &source_key,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            },
            0,
        ));
        assert_eq!(session.snapshot(), before_snapshot);
        assert_eq!(session.accepted_revision(), before_revision);
        assert_eq!(session.focus(), (None, None));
        // Successful relocation preserves revision and moves homing as a unit.
        assert!(session.relocate_domain(
            &source_key,
            &target,
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80
            },
            0,
        ));
        assert_eq!(session.accepted_revision(), before_revision);
        assert!(session.domains().iter().any(|d| d.key() == target));
        assert!(!session.domains().iter().any(|d| d.key() == source_key));
    }

    // ---- canonical pair slice ----

    fn canon_domain(output: &str, workspace: &str) -> OutputDomain {
        OutputDomain {
            id: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn pair_test_domains() -> Vec<OutputDomain> {
        use crate::directional::Direction;
        let mut source = canon_domain("out-1", "ws-1");
        let mut target = canon_domain("out-2", "ws-1");
        source.adjacent.insert(Direction::Right, target.id.clone());
        target.adjacent.insert(Direction::Left, source.id.clone());
        vec![source, target]
    }

    fn admit_in(
        session: &mut Session,
        window: &str,
        output: &str,
        workspace: &str,
        horizontal: bool,
    ) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let bounds = if horizontal {
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            }
        } else {
            Rect {
                x: 0,
                y: 0,
                w: 50,
                h: 100,
            }
        };
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: bounds,
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-admit-{window}"));
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("admit verify");
    }

    fn admit_exception_in(session: &mut Session, window: &str, output: &str, workspace: &str) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let mut windows = obs_windows(session);
        windows.push(ObservedWindow {
            window: window_id.clone(),
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            exceptions: ExceptionFlags {
                floating: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
            },
            exception_behavior: Some(ExceptionBehavior::Defer),
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            suppress_fixed_float: false,
        };
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows,
        };
        let correlation = corr(&format!("corr-admit-{window}"));
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit exception propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit exception ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("admit exception verify");
    }

    fn toggle_float_in(session: &mut Session, window: &str, rect: Option<Rect>, tag: &str) {
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let observation = SessionObservation {
            observation: Observation::new(owner(), generation(), rev, 100 + rev),
            windows: obs_windows(session),
        };
        let correlation = corr(&format!("corr-float-{window}-{tag}"));
        let plan = session
            .propose(
                &SessionCommand::ToggleFloat {
                    window: window_id,
                    float_geometry: rect,
                },
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("toggle propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner(),
                generation(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("toggle ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner(), generation(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("toggle verify");
    }

    fn tree_for(session: &Session, key: &DomainKey) -> Option<Node> {
        session.trees.get(key).cloned().flatten()
    }

    fn windows_for(session: &Session, key: &DomainKey) -> Vec<WindowLink> {
        let mut out: Vec<WindowLink> = session
            .windows
            .values()
            .filter(|l| l.output == key.output && l.workspace == key.workspace)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        out
    }

    fn exceptions_for(session: &Session, key: &DomainKey) -> Vec<ExceptionRecord> {
        let mut out: Vec<ExceptionRecord> = session
            .exceptions
            .values()
            .filter(|r| r.output == key.output && r.workspace == key.workspace)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.window.0.cmp(&b.window.0));
        out
    }

    fn retained_for(session: &Session, key: &DomainKey) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::new();
        for (id, rect) in &session.retained_float_geometry {
            let homed = session
                .windows
                .get(id)
                .is_some_and(|l| l.output == key.output && l.workspace == key.workspace)
                || session
                    .exceptions
                    .get(id)
                    .is_some_and(|r| r.output == key.output && r.workspace == key.workspace);
            if homed {
                out.push((id.clone(), *rect));
            }
        }
        out.sort_by(|a, b| a.0.0.cmp(&b.0.0));
        out
    }

    #[test]
    fn canonical_pair_round_trip_nested_unequal() {
        // Source: nested deep tree plus a live float (geometry + retained),
        // a retained-only tiled window (float then unfloat), and a deferred
        // exception. Target: smaller flat tree plus its own float/exception.
        let mut source = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-1", "ws-1")],
        )
        .expect("source new");
        admit_in(&mut source, "s-win-1", "out-1", "ws-1", true);
        admit_in(&mut source, "s-win-2", "out-1", "ws-1", true);
        admit_in(&mut source, "s-win-3", "out-1", "ws-1", false);
        admit_in(&mut source, "s-win-4", "out-1", "ws-1", false);
        admit_exception_in(&mut source, "s-defer-1", "out-1", "ws-1");
        toggle_float_in(
            &mut source,
            "s-win-1",
            Some(Rect {
                x: 10,
                y: 10,
                w: 40,
                h: 30,
            }),
            "to-float",
        );
        toggle_float_in(&mut source, "s-win-3", None, "to-float");
        toggle_float_in(&mut source, "s-win-3", None, "back-tiled");
        let mut target = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-2", "ws-1")],
        )
        .expect("target new");
        admit_in(&mut target, "t-win-1", "out-2", "ws-1", true);
        admit_in(&mut target, "t-win-2", "out-2", "ws-1", true);
        admit_exception_in(&mut target, "t-defer-1", "out-2", "ws-1");
        toggle_float_in(
            &mut target,
            "t-win-1",
            Some(Rect {
                x: 5,
                y: 5,
                w: 50,
                h: 40,
            }),
            "to-float",
        );
        assert_ne!(source.snapshot(), target.snapshot());
        let source_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let target_key = DomainKey {
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        // Source tree is nested (a group inside the root); target is flat.
        assert!(matches!(
            tree_for(&source, &source_key),
            Some(Node::Group { .. })
        ));
        let pair_domains = pair_test_domains();
        let pair =
            Session::paired_from_canonical(&source, Some(&target), pair_domains).expect("pair");
        assert_eq!(pair.domains().len(), 2);
        assert_eq!(pair.owner(), source.owner());
        assert_eq!(pair.generation(), source.generation());
        assert_eq!(
            pair.accepted_revision(),
            source.accepted_revision().max(target.accepted_revision())
        );
        assert!(pair.divergence().is_none());
        assert!(!pair.has_pending());
        assert!(!pair.has_pending_desired());
        assert!(!pair.has_drag());
        // Source carries verbatim. The transient pair namespaces target node
        // ids so independently fitted components remain globally addressable;
        // split restores target-local ids exactly below.
        assert_eq!(tree_for(&pair, &source_key), tree_for(&source, &source_key));
        assert_eq!(
            windows_for(&pair, &source_key),
            windows_for(&source, &source_key)
        );
        // Split without further commits restores every domain-scoped state.
        let (split_source, split_target) = pair.split_canonical_pair().expect("split");
        let split_target = split_target.expect("target present");
        for (before, after, key) in [
            (&source, &split_source, &source_key),
            (&target, &split_target, &target_key),
        ] {
            assert_eq!(
                tree_for(after, key),
                tree_for(before, key),
                "tree round trip for {key:?}"
            );
            assert_eq!(
                after.snapshot().windows.len(),
                before.snapshot().windows.len()
            );
            assert_eq!(windows_for(after, key), windows_for(before, key));
            assert_eq!(after.focus(), before.focus(), "focus for {key:?}");
            assert_eq!(
                after.focus_stack.get(key),
                before.focus_stack.get(key),
                "focus stack for {key:?}"
            );
            assert_eq!(
                after.last_active.get(key),
                before.last_active.get(key),
                "last-active for {key:?}"
            );
            assert_eq!(
                exceptions_for(after, key),
                exceptions_for(before, key),
                "exceptions for {key:?}"
            );
            for record in exceptions_for(after, key) {
                assert_eq!(
                    after.floating_geometry(&record.window),
                    before.floating_geometry(&record.window),
                    "floating geometry for {:?}",
                    record.window
                );
            }
            assert_eq!(
                retained_for(after, key),
                retained_for(before, key),
                "retained geometry for {key:?}"
            );
            assert_eq!(after.owner(), before.owner());
            assert_eq!(after.generation(), before.generation());
            assert_eq!(after.accepted_revision(), pair.accepted_revision());
            assert_eq!(after.accepted_fingerprint(), pair.accepted_fingerprint());
        }
        // Live float geometry and retained geometry survive the round trip.
        assert_eq!(
            split_source.floating_geometry(&WindowId("s-win-1".to_owned())),
            source.floating_geometry(&WindowId("s-win-1".to_owned()))
        );
        assert!(
            split_source
                .retained_float_geometry(&WindowId("s-win-1".to_owned()))
                .is_some()
        );
        assert!(
            split_source
                .retained_float_geometry(&WindowId("s-win-3".to_owned()))
                .is_some()
        );
        assert_eq!(
            split_target.floating_geometry(&WindowId("t-win-1".to_owned())),
            target.floating_geometry(&WindowId("t-win-1".to_owned()))
        );
        // Canonical outputs carry no adjacency.
        assert!(split_source.domains()[0].adjacent.is_empty());
        assert!(split_target.domains()[0].adjacent.is_empty());
        // Commit inside the pair (float one source tile), then split the
        // committed pair: the floated window leaves the source tree with its
        // geometry retained, and the target domain is untouched.
        let mut committed = pair.clone();
        toggle_float_in(
            &mut committed,
            "s-win-4",
            Some(Rect {
                x: 20,
                y: 20,
                w: 30,
                h: 30,
            }),
            "pair-commit",
        );
        assert!(!committed.has_pending());
        let (committed_source, committed_target) =
            committed.split_canonical_pair().expect("split committed");
        let committed_target = committed_target.expect("target present");
        assert!(committed_source.is_exception(&WindowId("s-win-4".to_owned())));
        assert_eq!(
            committed_source.floating_geometry(&WindowId("s-win-4".to_owned())),
            Some(Rect {
                x: 20,
                y: 20,
                w: 30,
                h: 30
            })
        );
        assert_eq!(
            committed_source.retained_float_geometry(&WindowId("s-win-4".to_owned())),
            Some(Rect {
                x: 20,
                y: 20,
                w: 30,
                h: 30
            })
        );
        assert_eq!(
            tree_for(&committed_target, &target_key),
            tree_for(&target, &target_key)
        );
        assert_eq!(
            committed_source.accepted_revision(),
            committed.accepted_revision()
        );
        assert_eq!(
            committed_target.accepted_revision(),
            committed.accepted_revision()
        );
    }

    #[test]
    fn canonical_pair_same_output_round_trip() {
        let pair_domains = vec![canon_domain("out-1", "ws-1"), canon_domain("out-1", "ws-2")];
        let source_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let target_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
        };
        let mut source = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-1", "ws-1")],
        )
        .expect("source new");
        admit_in(&mut source, "s-win-1", "out-1", "ws-1", true);
        admit_in(&mut source, "s-win-2", "out-1", "ws-1", false);
        let mut target = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-1", "ws-2")],
        )
        .expect("target new");
        admit_in(&mut target, "t-win-1", "out-1", "ws-2", true);
        // Occupied target round-trips topology, membership, and focus.
        let pair = Session::paired_from_canonical(&source, Some(&target), pair_domains.clone())
            .expect("same-output pair");
        assert_eq!(pair.domains().len(), 2);
        let (split_source, split_target) = pair.split_canonical_pair().expect("split");
        let split_target = split_target.expect("occupied target present");
        assert_eq!(
            tree_for(&split_source, &source_key),
            tree_for(&source, &source_key)
        );
        assert_eq!(
            tree_for(&split_target, &target_key),
            tree_for(&target, &target_key)
        );
        assert_eq!(
            windows_for(&split_source, &source_key),
            windows_for(&source, &source_key)
        );
        assert_eq!(
            windows_for(&split_target, &target_key),
            windows_for(&target, &target_key)
        );
        assert_eq!(split_source.focus(), source.focus());
        assert_eq!(split_target.focus(), target.focus());
        assert!(split_source.domains()[0].adjacent.is_empty());
        assert!(split_target.domains()[0].adjacent.is_empty());
        // Explicit empty target splits to None with source preserved.
        let empty_pair =
            Session::paired_from_canonical(&source, None, pair_domains).expect("empty pair");
        assert!(tree_for(&empty_pair, &target_key).is_none());
        let (empty_source, empty_target) = empty_pair.split_canonical_pair().expect("split");
        assert!(empty_target.is_none());
        assert_eq!(
            tree_for(&empty_source, &source_key),
            tree_for(&source, &source_key)
        );
        assert_eq!(empty_source.focus(), source.focus());
    }

    #[test]
    fn canonical_pair_absent_target_splits_none() {
        let mut source = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-1", "ws-1")],
        )
        .expect("source new");
        admit_in(&mut source, "s-win-1", "out-1", "ws-1", true);
        admit_in(&mut source, "s-win-2", "out-1", "ws-1", false);
        let pair =
            Session::paired_from_canonical(&source, None, pair_test_domains()).expect("pair");
        let target_key = DomainKey {
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        assert!(tree_for(&pair, &target_key).is_none());
        let (split_source, split_target) = pair.split_canonical_pair().expect("split");
        assert!(split_target.is_none());
        let source_key = DomainKey {
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        assert_eq!(
            tree_for(&split_source, &source_key),
            tree_for(&source, &source_key)
        );
        assert_eq!(split_source.focus(), source.focus());
    }

    #[test]
    fn canonical_pair_fail_closed() {
        let mut source = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-1", "ws-1")],
        )
        .expect("source new");
        admit_in(&mut source, "s-win-1", "out-1", "ws-1", true);
        let mut target = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-2", "ws-1")],
        )
        .expect("target new");
        admit_in(&mut target, "t-win-1", "out-2", "ws-1", true);
        // Owner/generation mismatch.
        let other_owner = Session::new(
            OwnerId::parse("owner-2").expect("valid"),
            generation(),
            0,
            7,
            vec![canon_domain("out-2", "ws-1")],
        )
        .expect("other new");
        assert_eq!(
            Session::paired_from_canonical(&source, Some(&other_owner), pair_test_domains())
                .expect_err("owner mismatch"),
            CanonicalPairError::MismatchedIdentity
        );
        // Duplicate window across inputs.
        let mut dupe = Session::new(
            owner(),
            generation(),
            0,
            7,
            vec![canon_domain("out-2", "ws-1")],
        )
        .expect("dupe new");
        admit_in(&mut dupe, "s-win-1", "out-2", "ws-1", true);
        assert_eq!(
            Session::paired_from_canonical(&source, Some(&dupe), pair_test_domains())
                .expect_err("duplicate"),
            CanonicalPairError::DuplicateState
        );
        // Mismatched domain keys.
        let wrong = vec![canon_domain("out-1", "ws-1"), canon_domain("out-9", "ws-1")];
        assert_eq!(
            Session::paired_from_canonical(&source, Some(&target), wrong)
                .expect_err("domain mismatch"),
            CanonicalPairError::DomainMismatch
        );
        // Non-single-domain input.
        let two = Session::new(owner(), generation(), 0, 7, pair_test_domains()).expect("two");
        assert_eq!(
            Session::paired_from_canonical(&two, Some(&target), pair_test_domains())
                .expect_err("non-single-domain"),
            CanonicalPairError::DomainMismatch
        );
        // Pending input refuses.
        let mut pending = source.clone();
        let rev = pending.accepted_revision();
        let mut windows = obs_windows(&pending);
        windows.push(ObservedWindow {
            window: WindowId("s-win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        pending
            .propose(
                &SessionCommand::Admit {
                    window: WindowId("s-win-9".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-1".to_owned()),
                    exceptions: ExceptionFlags::none(),
                    exception_behavior: None,
                    placement_bounds: Rect {
                        x: 0,
                        y: 0,
                        w: 100,
                        h: 50,
                    },
                    suppress_fixed_float: false,
                },
                &SessionObservation {
                    observation: Observation::new(owner(), generation(), rev, 1),
                    windows,
                },
                &corr("corr-pending-pair"),
                &LifecycleCapabilities::full(),
            )
            .expect("pending propose");
        assert_eq!(
            Session::paired_from_canonical(&pending, Some(&target), pair_test_domains())
                .expect_err("pending input"),
            CanonicalPairError::UnusableInput
        );
        // Split refuses pending/drag/divergence on the pair.
        let mut pair = Session::paired_from_canonical(&source, Some(&target), pair_test_domains())
            .expect("pair");
        let rev = pair.accepted_revision();
        let mut windows = obs_windows(&pair);
        windows.push(ObservedWindow {
            window: WindowId("pair-win-9".to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        });
        pair.propose(
            &SessionCommand::Admit {
                window: WindowId("pair-win-9".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-1".to_owned()),
                exceptions: ExceptionFlags::none(),
                exception_behavior: None,
                placement_bounds: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 50,
                },
                suppress_fixed_float: false,
            },
            &SessionObservation {
                observation: Observation::new(owner(), generation(), rev, 1),
                windows,
            },
            &corr("corr-pair-pending"),
            &LifecycleCapabilities::full(),
        )
        .expect("pair pending propose");
        assert_eq!(
            pair.split_canonical_pair().expect_err("pair pending"),
            CanonicalPairError::UnusableInput
        );
    }

    #[test]
    fn comparable_inserted_shares_keeps_skewed_fraction_on_small_current() {
        // Old B at 1/10 against normalized-small survivors: the current
        // weights rescale up (entrant 2, survivors 9 and 9) instead of
        // rounding the entrant to 0 and clamping to 1/3.
        assert_eq!(
            comparable_inserted_shares(1, 10, &[1, 1], 0),
            Some(vec![2, 9, 9])
        );
        assert_eq!(
            comparable_inserted_shares(1, 10, &[1, 1], 1),
            Some(vec![9, 2, 9])
        );
        assert_eq!(
            comparable_inserted_shares(1, 10, &[1, 1], 2),
            Some(vec![9, 9, 2])
        );
    }

    #[test]
    fn comparable_inserted_shares_preserves_fraction_and_survivor_ratios() {
        // Old 9/20 against resized survivors: exact fraction (2421/5380)
        // with exact survivor ratio (1254:1705 = 228:310).
        assert_eq!(
            comparable_inserted_shares(9, 20, &[228, 310], 0),
            Some(vec![2421, 1254, 1705])
        );
        // Even old shares rescale exactly (GCD-reduced).
        assert_eq!(
            comparable_inserted_shares(1, 3, &[1, 1], 1),
            Some(vec![1, 1, 1])
        );
    }

    #[test]
    fn comparable_inserted_shares_fails_closed() {
        assert_eq!(comparable_inserted_shares(0, 10, &[1, 1], 0), None);
        assert_eq!(comparable_inserted_shares(1, 1, &[1, 1], 0), None);
        assert_eq!(comparable_inserted_shares(2, 1, &[1, 1], 0), None);
        assert_eq!(comparable_inserted_shares(1, 10, &[], 0), None);
        assert_eq!(comparable_inserted_shares(1, 10, &[1, 0], 0), None);
        assert_eq!(comparable_inserted_shares(1, 10, &[1, 1], 3), None);
        // Unrepresentable post-reduction values fall back to fresh admission.
        assert_eq!(comparable_inserted_shares(1, 2, &[u64::MAX, 1], 0), None);
    }

    #[test]
    fn comparable_inserted_shares_rejects_nonrepresentable_total() {
        // Reduced entries each fit `u64`, but their combined total does
        // not: group totals must not overflow (`directional.rs`
        // validation), so best effort falls back instead of producing an
        // invalid tree. Here the reduced vector would be
        // `[u64::MAX - 1, 1, 1]` totalling `u64::MAX + 1`.
        assert_eq!(
            comparable_inserted_shares((1u64 << 63) - 1, 1u128 << 63, &[1, 1], 0),
            None
        );
        // One below the bound still rejoins exactly: reduced
        // `[2^63 - 2, 1, 1]` totals exactly `2^63`.
        assert_eq!(
            comparable_inserted_shares((1u64 << 63) - 2, 1u128 << 63, &[1, 1], 0),
            Some(vec![(1u64 << 63) - 2, 1, 1])
        );
    }
}
