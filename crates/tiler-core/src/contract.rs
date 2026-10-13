//! Production transport-independent adapter contract.
//!
//! Portable boundary: bounded opaque ids, immutable normalized observations,
//! transport-neutral dispatch payloads, adapter acknowledgements, and typed
//! divergence. No platform, process, IPC, geometry, or native execution
//! imports; only [`crate::directional`] semantic types.
//!
//! Sealed/versioned policy note: the planning policy itself stays frozen in
//! [`crate::directional`] as conformance evidence and is not re-abstracted
//! here. [`CONTRACT_VERSION`] pins this adapter envelope; [`POLICY_VERSION`]
//! pins the policy it carries plans from.
//!
//! Error discipline: validators return `bool` and reconciler errors carry
//! only typed [`DivergenceKind`] variants with fixed redacted messages. Input
//! ids are never echoed and no unbounded allocations are made from adapter
//! input (all id lengths are bounded; precondition vectors are capped).

use crate::directional::{
    Axis, Capability, Direction, MoveIntent, MoveOperation, NodeId, OutputId, Precondition, Rule,
    WindowId, WorkspaceId,
};
use crate::ids::{CorrelationId, GenerationId, OwnerId};

/// Adapter envelope version.
pub const CONTRACT_VERSION: u32 = 1;
/// Sealed planning policy version carried by plans.
pub const POLICY_VERSION: u32 = 1;
/// Portable session lifecycle policy version (`cosmic_v1`).
///
/// The session lifecycle foundation in [`crate::session`] is versioned
/// `cosmic_v1`. Placement fallback inside the session (append after the last
/// root child or nest when there is no eligible focus) is project-selected
/// and makes no source COSMIC parity claim. Frozen R1-R4 movement
/// APIs/fixtures are unaffected; this binds lifecycle plans/dispatch only.
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

/// Immutable normalized adapter observation.
///
/// `fingerprint` is an opaque adapter-computed hash of the normalized
/// snapshot (transport-neutral `u64`, never interpreted here). The reconciler
/// binds proposals to `owner`/`generation`/`revision` and requires a fresh
/// re-observation at the same base revision after acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub revision: u64,
    pub fingerprint: u64,
}

impl Observation {
    /// Typed construction (ids already validated by [`OwnerId::parse`]).
    #[must_use]
    pub fn new(owner: OwnerId, generation: GenerationId, revision: u64, fingerprint: u64) -> Self {
        Self {
            owner,
            generation,
            revision,
            fingerprint,
        }
    }

    /// Narrow string boundary: parses session ids, checks the revision bound.
    #[must_use]
    pub fn from_strings(
        owner: &str,
        generation: &str,
        revision: u64,
        fingerprint: u64,
    ) -> Option<Self> {
        if !is_revision(revision) {
            return None;
        }
        Some(Self {
            owner: OwnerId::parse(owner)?,
            generation: GenerationId::parse(generation)?,
            revision,
            fingerprint,
        })
    }

    /// Validity without echoing input.
    #[must_use]
    pub fn validate(&self) -> bool {
        is_owner_id(self.owner.as_str())
            && is_generation_id(self.generation.as_str())
            && is_revision(self.revision)
    }
}

/// Transport-neutral dispatch payload emitted on a successful proposal.
/// Native execution is outside the reconciler; the adapter actuates from this.
/// `operation` carries the full semantic plan operation (structural ids only,
/// no geometry or native handles); `rule`/`required_capability`/`preconditions`
/// are the bound execution preconditions derived from it. `intent` is the
/// complete originating semantic intent copied from the source plan so a future
/// adapter can interpret the payload without retaining caller-side state.
/// `owner`/`generation`/`correlation_id`/`base_revision` form the
/// self-describing identity binding for the dispatched plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatch {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub required_capability: Capability,
    pub preconditions: Vec<Precondition>,
    pub rule: Rule,
    pub operation: MoveOperation,
    pub intent: MoveIntent,
}

/// Bounded adapter acknowledgement outcome. No free-form error detail: a
/// non-accepted outcome maps to a typed divergence without echo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    Accepted,
    RefusedCapability,
    PartialApplication,
    AdapterLost,
}

/// Explicit adapter acknowledgement of a dispatched plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterAck {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub outcome: AckOutcome,
}

impl AdapterAck {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        correlation_id: CorrelationId,
        owner: OwnerId,
        generation: GenerationId,
        base_revision: u64,
        outcome: AckOutcome,
    ) -> Self {
        Self {
            correlation_id,
            owner,
            generation,
            base_revision,
            outcome,
        }
    }

    /// Narrow string boundary: parses session ids without echo.
    #[must_use]
    pub fn from_strings(
        correlation_id: &str,
        owner: &str,
        generation: &str,
        base_revision: u64,
        outcome: AckOutcome,
    ) -> Option<Self> {
        if !is_revision(base_revision) {
            return None;
        }
        Some(Self {
            correlation_id: CorrelationId::parse(correlation_id)?,
            owner: OwnerId::parse(owner)?,
            generation: GenerationId::parse(generation)?,
            base_revision,
            outcome,
        })
    }

    /// Validity without echoing input.
    #[must_use]
    pub fn validate(&self) -> bool {
        is_correlation_id(self.correlation_id.as_str())
            && is_owner_id(self.owner.as_str())
            && is_generation_id(self.generation.as_str())
            && is_revision(self.base_revision)
    }
}

/// Fresh post-observation plus explicit native verification flag. The adapter
/// sets `verified` only after natively verifying plan postconditions, and
/// reports the exact verified precondition vector in `verified_preconditions`
/// plus the exact verified operation in `verified_operation`. The reconciler
/// binds the vector exactly to the dispatched preconditions (capped by
/// [`MAX_PRECONDITIONS`]) and the operation exactly to the dispatched
/// semantic operation; any mismatch diverges as
/// [`DivergenceKind::PostconditionMismatch`] with no commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostObservation {
    pub observation: Observation,
    pub correlation_id: CorrelationId,
    pub verified: bool,
    pub verified_preconditions: Vec<Precondition>,
    pub verified_operation: MoveOperation,
}

impl PostObservation {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<Precondition>,
        verified_operation: MoveOperation,
    ) -> Self {
        Self {
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
        }
    }

    /// Validity without echoing input (observation plus correlation shape and
    /// bounded precondition vector).
    #[must_use]
    pub fn validate(&self) -> bool {
        self.observation.validate()
            && is_correlation_id(self.correlation_id.as_str())
            && self.verified_preconditions.len() <= MAX_PRECONDITIONS
    }
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

/// Adapter-facing lifecycle capability required to realize one lifecycle
/// operation. Separate from [`Capability`] movement capabilities so the frozen
/// R1-R4 movement surface stays unchanged; lifecycle admission, removal,
/// same-output workspace transfer, and parent split-axis toggle each gate on
/// their own explicit capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleCapability {
    AdmitTiled,
    RemoveTiled,
    MoveTiled,
    ToggleOrientation,
}

impl LifecycleCapability {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdmitTiled => "admit-tiled",
            Self::RemoveTiled => "remove-tiled",
            Self::MoveTiled => "move-tiled",
            Self::ToggleOrientation => "toggle-orientation",
        }
    }
}

/// Adapter-declared lifecycle capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleCapabilities {
    pub admit_tiled: bool,
    pub remove_tiled: bool,
    pub move_tiled: bool,
    pub toggle_orientation: bool,
}

impl LifecycleCapabilities {
    /// All lifecycle capabilities declared.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            admit_tiled: true,
            remove_tiled: true,
            move_tiled: true,
            toggle_orientation: true,
        }
    }

    /// None declared.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            admit_tiled: false,
            remove_tiled: false,
            move_tiled: false,
            toggle_orientation: false,
        }
    }

    /// Whether `capability` is declared.
    #[must_use]
    pub const fn supports(&self, capability: LifecycleCapability) -> bool {
        match capability {
            LifecycleCapability::AdmitTiled => self.admit_tiled,
            LifecycleCapability::RemoveTiled => self.remove_tiled,
            LifecycleCapability::MoveTiled => self.move_tiled,
            LifecycleCapability::ToggleOrientation => self.toggle_orientation,
        }
    }
}

/// Explicit preconditions the adapter must hold/verify to realize a lifecycle
/// plan. `AdapterMustVerifyPostconditions` is present on every lifecycle plan,
/// mirroring movement plans: realization is never assumed atomic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecyclePrecondition {
    WindowObserved,
    DesiredTopologyValid,
    AdapterMustVerifyPostconditions,
}

/// Semantic lifecycle intent: admit a window into an output/workspace domain,
/// remove a window from the session, move the focused tiled window to an
/// explicit same-output target workspace domain, or toggle the focused tiled
/// window's immediate parent split axis. Structural resolution (leaf identity,
/// deferred exception handling) lives in [`LifecycleOperation`];
/// the intent records the originating request so a plan can be interpreted
/// without retaining caller-side state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleIntent {
    Admit {
        window: WindowId,
        output: OutputId,
        workspace: WorkspaceId,
    },
    Remove {
        window: WindowId,
    },
    MoveToWorkspace {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// Follow/stay selection: true follows the mover into the target
        /// (COSMIC `MoveToWorkspace`), false leaves the source selected with
        /// source-MRU focus (COSMIC `SendToWorkspace`). Structural transfer
        /// is identical; only the desired focus differs.
        follow: bool,
    },
    /// Explicit output send (REQ-OUT-04, item 5.3/5.4): the focused tiled
    /// window moves to the destination output's current workspace, resolved
    /// adapter-side and carried as an explicit cross-output target domain.
    /// Distinct from [`LifecycleIntent::MoveToWorkspace`], which stays
    /// same-output. Admission (remembered leaf, destination focus history,
    /// root fallback) and follow/stay are ordinary; only the target scope
    /// differs.
    MoveToOutput {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// Follow/stay selection, identical to `MoveToWorkspace`.
        follow: bool,
    },
    ToggleOrientation {
        window: WindowId,
        output: OutputId,
        workspace: WorkspaceId,
    },
}

/// Structural lifecycle operation with fully resolved portable identities.
/// Tiled variants name the affected leaf; deferred variants track an observed
/// exception window with no topology effect. No geometry or native handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleOperation {
    Admit {
        window: WindowId,
        leaf: NodeId,
        output: OutputId,
        workspace: WorkspaceId,
    },
    AdmitDeferred {
        window: WindowId,
        output: OutputId,
        workspace: WorkspaceId,
    },
    Remove {
        window: WindowId,
        leaf: NodeId,
        output: OutputId,
        workspace: WorkspaceId,
    },
    RemoveDeferred {
        window: WindowId,
        output: OutputId,
        workspace: WorkspaceId,
    },
    MoveTiled {
        window: WindowId,
        leaf: NodeId,
        source_output: OutputId,
        source_workspace: WorkspaceId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
    },
    /// Slotless exception-member transfer (D28 born-fullscreen carry): the
    /// mover keeps its held exception record with only the homing
    /// output/workspace retargeted. No leaf exists (never fabricated), no
    /// tree mutation, no allocation change. The explicit send intent
    /// (`MoveToWorkspace`/`MoveToOutput` with follow/stay) carries the
    /// follow selection exactly like [`LifecycleOperation::MoveTiled`].
    MoveDeferred {
        window: WindowId,
        source_output: OutputId,
        source_workspace: WorkspaceId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
    },
    ToggleOrientation {
        window: WindowId,
        leaf: NodeId,
        group: NodeId,
        output: OutputId,
        workspace: WorkspaceId,
    },
}

impl LifecycleOperation {
    /// Adapter-facing capability required before emission.
    #[must_use]
    pub const fn required_capability(&self) -> LifecycleCapability {
        match self {
            Self::Admit { .. } | Self::AdmitDeferred { .. } => LifecycleCapability::AdmitTiled,
            Self::Remove { .. } | Self::RemoveDeferred { .. } => LifecycleCapability::RemoveTiled,
            Self::MoveTiled { .. } | Self::MoveDeferred { .. } => LifecycleCapability::MoveTiled,
            Self::ToggleOrientation { .. } => LifecycleCapability::ToggleOrientation,
        }
    }

    /// Explicit preconditions for realization (always terminated by
    /// [`LifecyclePrecondition::AdapterMustVerifyPostconditions`]).
    #[must_use]
    pub fn preconditions(&self) -> Vec<LifecyclePrecondition> {
        vec![
            LifecyclePrecondition::WindowObserved,
            LifecyclePrecondition::DesiredTopologyValid,
            LifecyclePrecondition::AdapterMustVerifyPostconditions,
        ]
    }
}

/// Deterministic lifecycle plan with explicit capability and preconditions.
/// Self-contained: `intent` records the originating semantic intent.
/// `policy_version` binds the plan to [`LIFECYCLE_POLICY_VERSION`]
/// (`cosmic_v1` lifecycle policy); the reconciler rejects any other version
/// without affecting frozen R1-R4 movement plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecyclePlan {
    pub intent: LifecycleIntent,
    pub operation: LifecycleOperation,
    pub required_capability: LifecycleCapability,
    pub preconditions: Vec<LifecyclePrecondition>,
    pub policy_version: u32,
}

impl LifecyclePlan {
    /// Construct from an intent and a resolved operation, deriving capability
    /// and preconditions deterministically.
    #[must_use]
    pub fn for_operation(intent: LifecycleIntent, operation: LifecycleOperation) -> Self {
        let required_capability = operation.required_capability();
        let preconditions = operation.preconditions();
        Self {
            intent,
            operation,
            required_capability,
            preconditions,
            policy_version: LIFECYCLE_POLICY_VERSION,
        }
    }

    /// Validity of the portable lifecycle policy binding.
    #[must_use]
    pub const fn valid_policy(&self) -> bool {
        self.policy_version == LIFECYCLE_POLICY_VERSION
    }
}

/// Transport-neutral lifecycle dispatch payload emitted on a successful
/// lifecycle proposal. Mirrors [`Dispatch`] identity binding
/// (owner/generation/correlation/base revision) plus the complete semantic
/// lifecycle plan. Desired topology/focus/geometry are carried by the session
/// layer (`crate::session`) so this envelope stays geometry-free like
/// [`Dispatch`]; native execution stays outside the reconciler.
/// `policy_version` echoes the plan binding ([`LIFECYCLE_POLICY_VERSION`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleDispatch {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub required_capability: LifecycleCapability,
    pub preconditions: Vec<LifecyclePrecondition>,
    pub intent: LifecycleIntent,
    pub operation: LifecycleOperation,
    pub policy_version: u32,
}

/// Fresh post-observation plus explicit native verification flag for a pending
/// lifecycle plan. Binds exactly like [`PostObservation`]: the reported
/// `verified_preconditions` must equal the dispatched preconditions (capped by
/// [`MAX_PRECONDITIONS`]) and `verified_operation` must equal the dispatched
/// lifecycle operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecyclePostObservation {
    pub observation: Observation,
    pub correlation_id: CorrelationId,
    pub verified: bool,
    pub verified_preconditions: Vec<LifecyclePrecondition>,
    pub verified_operation: LifecycleOperation,
}

impl LifecyclePostObservation {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<LifecyclePrecondition>,
        verified_operation: LifecycleOperation,
    ) -> Self {
        Self {
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
        }
    }

    /// Validity without echoing input (observation plus correlation shape and
    /// bounded precondition vector).
    #[must_use]
    pub fn validate(&self) -> bool {
        self.observation.validate()
            && is_correlation_id(self.correlation_id.as_str())
            && self.verified_preconditions.len() <= MAX_PRECONDITIONS
    }
}

/// Adapter-facing focus capability required to realize a focus plan.
/// Separate from movement [`Capability`] and lifecycle [`LifecycleCapability`]
/// so frozen R1-R4 movement types are never misused for focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusCapability {
    DirectionalFocus,
}

impl FocusCapability {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectionalFocus => "directional-focus",
        }
    }
}

/// Adapter-declared focus capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusCapabilities {
    pub directional_focus: bool,
}

impl FocusCapabilities {
    /// All focus capabilities declared.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            directional_focus: true,
        }
    }

    /// None declared.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            directional_focus: false,
        }
    }

    /// Whether `capability` is declared.
    #[must_use]
    pub const fn supports(&self, capability: FocusCapability) -> bool {
        match capability {
            FocusCapability::DirectionalFocus => self.directional_focus,
        }
    }
}

/// Explicit preconditions the adapter must hold/verify to realize a focus
/// plan. `AdapterMustVerifyPostconditions` is present on every focus plan,
/// mirroring movement/lifecycle plans: realization is never assumed atomic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusPrecondition {
    FocusedLeafOccupiedByFocusedWindow,
    /// Float-origin subject: the focused window is a floating/sticky
    /// exception with no tile leaf, so no leaf-occupancy claim applies.
    /// Selected exactly when the operation carries no `from_leaf`; tile
    /// operations always carry the leaf-occupancy token instead.
    FocusedFloatingWindow,
    TargetLeafOccupied,
    FocusTargetsSameDomain,
    FocusTargetsAdjacentOutput,
    AdapterMustVerifyPostconditions,
}

/// Semantic focus intent: directional navigation from the focused window in
/// one exact logical domain. Records the originating request so a plan can
/// be interpreted without retaining caller-side state. Tile origins carry
/// the focused tile leaf; float origins (floating/sticky exception subject)
/// carry `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusIntent {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub focused_leaf: Option<NodeId>,
    pub focused_window: WindowId,
    pub direction: Direction,
}

/// Structural focus operation with fully resolved portable identities.
///
/// For local focus, `domain_output`/`domain_workspace` name the single domain
/// containing both leaves and `cross_source` is `None`. For cross-output
/// focus (Left/Right exhausted edge to a horizontally adjacent output's
/// currently selected logical workspace), `domain_output`/`domain_workspace`
/// name the TARGET domain, `from_*` name the source focused leaf/window,
/// `to_*` name the target focused leaf/window, `cross_source` names the
/// source domain, and `route` is the single-element `[to_leaf]` (no
/// tree-relative descent across domains). Float-origin cross-output focus
/// (adapter local float search missed Left/Right) carries `from_leaf: None`:
/// the subject is a floating/sticky exception with no tile leaf, so `from`
/// names only the source window. No geometry or native handles; desired
/// topology is unmodified and carried by the session layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusOperation {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub from_leaf: Option<NodeId>,
    pub to_leaf: NodeId,
    pub from_window: WindowId,
    pub to_window: WindowId,
    pub direction: Direction,
    pub route: Vec<NodeId>,
    pub cross_source_output: Option<OutputId>,
    pub cross_source_workspace: Option<WorkspaceId>,
}

impl FocusOperation {
    /// Adapter-facing capability required before emission.
    #[must_use]
    pub const fn required_capability(&self) -> FocusCapability {
        FocusCapability::DirectionalFocus
    }

    /// Explicit preconditions for realization (always terminated by
    /// [`FocusPrecondition::AdapterMustVerifyPostconditions`]). Leafless
    /// (float-origin) operations select
    /// [`FocusPrecondition::FocusedFloatingWindow`] instead of the
    /// leaf-occupancy token; every other shape is unchanged.
    #[must_use]
    pub fn preconditions(&self) -> Vec<FocusPrecondition> {
        let domain_target =
            if self.cross_source_output.is_some() || self.cross_source_workspace.is_some() {
                FocusPrecondition::FocusTargetsAdjacentOutput
            } else {
                FocusPrecondition::FocusTargetsSameDomain
            };
        let source_token = if self.from_leaf.is_none() {
            FocusPrecondition::FocusedFloatingWindow
        } else {
            FocusPrecondition::FocusedLeafOccupiedByFocusedWindow
        };
        vec![
            source_token,
            FocusPrecondition::TargetLeafOccupied,
            domain_target,
            FocusPrecondition::AdapterMustVerifyPostconditions,
        ]
    }

    /// Whether this operation is a cross-output focus (has an explicit
    /// cross source distinct from the target domain).
    #[must_use]
    pub fn is_cross_output(&self) -> bool {
        self.cross_source_output.is_some() || self.cross_source_workspace.is_some()
    }
}

/// Deterministic focus plan with explicit capability and preconditions.
/// Self-contained: `intent` records the originating semantic intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusPlanContract {
    pub intent: FocusIntent,
    pub operation: FocusOperation,
    pub required_capability: FocusCapability,
    pub preconditions: Vec<FocusPrecondition>,
}

impl FocusPlanContract {
    /// Construct from an intent and a resolved operation, deriving capability
    /// and preconditions deterministically.
    #[must_use]
    pub fn for_operation(intent: FocusIntent, operation: FocusOperation) -> Self {
        let required_capability = operation.required_capability();
        let preconditions = operation.preconditions();
        Self {
            intent,
            operation,
            required_capability,
            preconditions,
        }
    }
}

/// Transport-neutral focus dispatch payload emitted on a successful focus
/// proposal. Mirrors [`Dispatch`] identity binding
/// (owner/generation/correlation/base revision) plus the complete semantic
/// focus plan. Desired topology/focus/geometry are carried by the session
/// layer so this envelope stays geometry-free like [`Dispatch`]; native
/// execution stays outside the reconciler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusDispatch {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub required_capability: FocusCapability,
    pub preconditions: Vec<FocusPrecondition>,
    pub intent: FocusIntent,
    pub operation: FocusOperation,
}

/// Fresh post-observation plus explicit native verification flag for a pending
/// focus plan. Binds exactly like [`PostObservation`]: the reported
/// `verified_preconditions` must equal the dispatched preconditions (capped by
/// [`MAX_PRECONDITIONS`]) and `verified_operation` must equal the dispatched
/// focus operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusPostObservation {
    pub observation: Observation,
    pub correlation_id: CorrelationId,
    pub verified: bool,
    pub verified_preconditions: Vec<FocusPrecondition>,
    pub verified_operation: FocusOperation,
}

impl FocusPostObservation {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<FocusPrecondition>,
        verified_operation: FocusOperation,
    ) -> Self {
        Self {
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
        }
    }

    /// Validity without echoing input (observation plus correlation shape and
    /// bounded precondition vector).
    #[must_use]
    pub fn validate(&self) -> bool {
        self.observation.validate()
            && is_correlation_id(self.correlation_id.as_str())
            && self.verified_preconditions.len() <= MAX_PRECONDITIONS
    }
}

/// Adapter-facing resize capability required to realize a split-share
/// resize plan. Separate from movement [`Capability`], lifecycle
/// [`LifecycleCapability`], and focus [`FocusCapability`] so frozen movement
/// behavior is never misused for resize. Keyboard and pointer resize routes
/// require their own explicit capability so platforms can advertise either
/// independently. COSMIC fixed 360/240 minima live under
/// [`crate::cosmic_v1`] and the normalized projector supplies physical
/// geometry; no separate native capability exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeCapability {
    KeyboardResize,
    PointerResize,
}

impl ResizeCapability {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KeyboardResize => "keyboard-resize",
            Self::PointerResize => "pointer-resize",
        }
    }
}

/// Adapter-declared resize capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeCapabilities {
    pub keyboard_resize: bool,
    pub pointer_resize: bool,
}

impl ResizeCapabilities {
    /// All resize capabilities declared.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            keyboard_resize: true,
            pointer_resize: true,
        }
    }

    /// None declared.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            keyboard_resize: false,
            pointer_resize: false,
        }
    }

    /// Whether `capability` is declared.
    #[must_use]
    pub const fn supports(&self, capability: ResizeCapability) -> bool {
        match capability {
            ResizeCapability::KeyboardResize => self.keyboard_resize,
            ResizeCapability::PointerResize => self.pointer_resize,
        }
    }
}

/// Explicit preconditions the adapter must hold/verify to realize a resize
/// plan. `AdapterMustVerifyPostconditions` is present on every resize plan,
/// mirroring movement/lifecycle/focus plans: realization is never assumed
/// atomic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizePrecondition {
    FocusedLeafOccupiedByFocusedWindow,
    TargetBoundaryValid,
    ResizeTargetsSameDomain,
    AdapterMustVerifyPostconditions,
}

/// Portable keyboard resize direction mode: source
/// `ResizeDirection::{Inwards, Outwards}` (`tiling/mod.rs` 2514-2600).
/// `Inwards` shrinks the focused/selected node, `Outwards` grows it. Carried
/// with the cardinal edge (`Direction` as edge: edge axis selects the nearest
/// matching-axis ancestor, edge determines the neighbor side).
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

/// Semantic resize intent: keyboard pixel resize from the focused leaf in one
/// exact logical domain with source `ResizeDirection` mode plus cardinal edge
/// (`direction` as edge). Records the originating request so a plan can be
/// interpreted without retaining caller-side state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeIntent {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub focused_leaf: NodeId,
    pub focused_window: WindowId,
    pub direction: Direction,
    pub mode: ResizeMode,
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
/// handles; desired topology/geometry are carried by the session layer.
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

impl ResizeOperation {
    /// Adapter-facing capability required before emission on the keyboard
    /// route. The pointer route carries the same operation shape but binds
    /// [`ResizeCapability::PointerResize`] explicitly at the session and
    /// reconciler boundary.
    #[must_use]
    pub const fn required_capability(&self) -> ResizeCapability {
        ResizeCapability::KeyboardResize
    }

    /// Explicit preconditions for realization (always terminated by
    /// [`ResizePrecondition::AdapterMustVerifyPostconditions`]).
    #[must_use]
    pub fn preconditions(&self) -> Vec<ResizePrecondition> {
        vec![
            ResizePrecondition::FocusedLeafOccupiedByFocusedWindow,
            ResizePrecondition::TargetBoundaryValid,
            ResizePrecondition::ResizeTargetsSameDomain,
            ResizePrecondition::AdapterMustVerifyPostconditions,
        ]
    }
}

/// Deterministic resize plan with explicit capability and preconditions.
/// Self-contained: `intent` records the originating semantic intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizePlan {
    pub intent: ResizeIntent,
    pub operation: ResizeOperation,
    pub required_capability: ResizeCapability,
    pub preconditions: Vec<ResizePrecondition>,
}

impl ResizePlan {
    /// Construct from an intent and a resolved operation, deriving capability
    /// and preconditions deterministically.
    #[must_use]
    pub fn for_operation(intent: ResizeIntent, operation: ResizeOperation) -> Self {
        let required_capability = operation.required_capability();
        let preconditions = operation.preconditions();
        Self {
            intent,
            operation,
            required_capability,
            preconditions,
        }
    }
}

/// Transport-neutral resize dispatch payload emitted on a successful resize
/// proposal. Mirrors [`Dispatch`] identity binding
/// (owner/generation/correlation/base revision) plus the complete semantic
/// resize plan. Desired topology/focus/geometry are carried by the session
/// layer so this envelope stays geometry-free like [`Dispatch`]; native
/// execution stays outside the reconciler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeDispatch {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub required_capability: ResizeCapability,
    pub preconditions: Vec<ResizePrecondition>,
    pub intent: ResizeIntent,
    pub operation: ResizeOperation,
    /// Second-axis operation for an atomic corner (dual-axis) pointer
    /// resize. `None` for every single-axis plan (keyboard plus pointer);
    /// `Some` only when one `pointer-resize` request carries both a
    /// horizontal and a vertical boundary through the single pending slot.
    /// Boxed: the corner axis rides cold-path only, keeping the dispatch
    /// footprint at the single-axis size.
    pub secondary_operation: Option<Box<ResizeOperation>>,
}

/// Fresh post-observation plus explicit native verification flag for a pending
/// resize plan. Binds exactly like [`PostObservation`]: the reported
/// `verified_preconditions` must equal the dispatched preconditions (capped by
/// [`MAX_PRECONDITIONS`]) and `verified_operation` must equal the dispatched
/// resize operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizePostObservation {
    pub observation: Observation,
    pub correlation_id: CorrelationId,
    pub verified: bool,
    pub verified_preconditions: Vec<ResizePrecondition>,
    pub verified_operation: ResizeOperation,
    /// Second-axis operation echo for an atomic corner resize. Compared by
    /// exact `Option` equality against the pending corner plan, so a corner
    /// pending requires `Some` and every single-axis pending requires `None`.
    /// Boxed like the dispatch echo it binds.
    pub verified_secondary_operation: Option<Box<ResizeOperation>>,
}

impl ResizePostObservation {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<ResizePrecondition>,
        verified_operation: ResizeOperation,
    ) -> Self {
        Self::new_with_secondary(
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
            None,
        )
    }

    /// Typed construction carrying the second-axis operation echo for an
    /// atomic corner resize.
    #[must_use]
    pub fn new_with_secondary(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<ResizePrecondition>,
        verified_operation: ResizeOperation,
        verified_secondary_operation: Option<Box<ResizeOperation>>,
    ) -> Self {
        Self {
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
            verified_secondary_operation,
        }
    }

    /// Validity without echoing input (observation plus correlation shape and
    /// bounded precondition vector).
    #[must_use]
    pub fn validate(&self) -> bool {
        self.observation.validate()
            && is_correlation_id(self.correlation_id.as_str())
            && self.verified_preconditions.len() <= MAX_PRECONDITIONS
    }
}

/// Adapter-facing drag capability required to realize a pointer drag-placement
/// plan. Separate from movement [`Capability`], lifecycle
/// [`LifecycleCapability`], focus [`FocusCapability`], and resize
/// [`ResizeCapability`] so frozen movement behavior is never misused for drag.
/// Portable topology excludes stacks (no dual authority); center is an
/// explicit unsupported stack behavior and never plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragCapability {
    PlaceTiled,
}

impl DragCapability {
    /// Stable kind string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlaceTiled => "place-tiled",
        }
    }
}

/// Adapter-declared drag capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragCapabilities {
    pub place_tiled: bool,
}

impl DragCapabilities {
    /// All drag capabilities declared.
    #[must_use]
    pub const fn full() -> Self {
        Self { place_tiled: true }
    }

    /// None declared.
    #[must_use]
    pub const fn none() -> Self {
        Self { place_tiled: false }
    }

    /// Whether `capability` is declared.
    #[must_use]
    pub const fn supports(&self, capability: DragCapability) -> bool {
        match capability {
            DragCapability::PlaceTiled => self.place_tiled,
        }
    }
}

/// Explicit preconditions the adapter must hold/verify to realize a drag plan.
/// `AdapterMustVerifyPostconditions` is present on every drag plan, mirroring
/// movement/lifecycle/focus/resize plans: realization is never assumed atomic.
/// Same-domain plans carry `DragTargetsSameDomain`; cross-output plans (the
/// mover leaves the capture domain for the pointer output's current domain)
/// carry `DragTargetsCrossDomain` instead, never both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragPrecondition {
    SourceLeafOccupiedBySourceWindow,
    TargetLeafOccupied,
    DragTargetsSameDomain,
    DragTargetsCrossDomain,
    AdapterMustVerifyPostconditions,
}

/// Portable drop edge plus the COSMIC center stack source fact. Left/right
/// order along [`Axis::Horizontal`], top/bottom along [`Axis::Vertical`].
/// `Center` names the source middle-third stack fact
/// (`tiling/mod.rs` `drop_window` 2770-2788; portable topology excludes stacks
/// so the session fails closed on it as explicit unsupported stack behavior
/// with no `DragPlan`). `axis`/`before` are edge-only (`None` for `Center`);
/// callers must branch on `Center` first and only edges reach split placement.
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

/// Semantic drag intent: pointer placement of the focused source tile onto a
/// target leaf edge. Records the originating request so a plan can be
/// interpreted without retaining caller-side state. `domain_*` names the
/// TARGET domain (the pointer output's current domain); `cross_source` names
/// the capture domain the mover leaves (`None` for same-domain drags, where
/// target and capture coincide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragIntent {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub target_leaf: NodeId,
    pub target_window: WindowId,
    pub side: DragSide,
    pub cross_source: Option<(OutputId, WorkspaceId)>,
}

/// Structural drag operation with fully resolved portable identities.
///
/// Names the target logical domain, the source and target leaves/windows, the
/// drop `side` with its derived `axis`/`before` ordering, the target parent
/// (`target_group` with the post-removal `insertion_index` for the source) and
/// the wrap form: same-axis inserts carry `wrap == false` with `new_group ==
/// None`, while perpendicular placements wrap only the target subtree in a new
/// smallest 2-child split (`wrap == true` with the fresh `new_group` id; for a
/// root target the `target_group` names the target leaf itself). `cross_source`
/// names the capture domain the mover leaves (`None` for same-domain drops,
/// where the source leaf is removed from the target tree itself). No geometry or
/// native handles; desired topology/geometry are carried by the session layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragOperation {
    pub domain_output: OutputId,
    pub domain_workspace: WorkspaceId,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub target_leaf: NodeId,
    pub target_window: WindowId,
    pub side: DragSide,
    pub axis: Axis,
    pub before: bool,
    pub target_group: NodeId,
    pub insertion_index: usize,
    pub wrap: bool,
    pub new_group: Option<NodeId>,
    pub cross_source: Option<(OutputId, WorkspaceId)>,
}

impl DragOperation {
    /// Adapter-facing capability required before emission (edge drops only;
    /// center never plans).
    #[must_use]
    pub const fn required_capability(&self) -> DragCapability {
        DragCapability::PlaceTiled
    }

    /// Explicit preconditions for realization (always terminated by
    /// [`DragPrecondition::AdapterMustVerifyPostconditions`]). Cross-output
    /// operations (a `cross_source` distinct from the target domain) carry
    /// `DragTargetsCrossDomain` in place of `DragTargetsSameDomain`.
    #[must_use]
    pub fn preconditions(&self) -> Vec<DragPrecondition> {
        let domain = if self.is_cross_output() {
            DragPrecondition::DragTargetsCrossDomain
        } else {
            DragPrecondition::DragTargetsSameDomain
        };
        vec![
            DragPrecondition::SourceLeafOccupiedBySourceWindow,
            DragPrecondition::TargetLeafOccupied,
            domain,
            DragPrecondition::AdapterMustVerifyPostconditions,
        ]
    }

    /// Whether this operation moves the mover across output domains: a
    /// `cross_source` distinct from the target domain.
    #[must_use]
    pub fn is_cross_output(&self) -> bool {
        self.cross_source
            .as_ref()
            .is_some_and(|(output, workspace)| {
                output != &self.domain_output || workspace != &self.domain_workspace
            })
    }
}

/// Deterministic drag plan with explicit capability and preconditions.
/// Self-contained: `intent` records the originating semantic intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPlan {
    pub intent: DragIntent,
    pub operation: DragOperation,
    pub required_capability: DragCapability,
    pub preconditions: Vec<DragPrecondition>,
}

impl DragPlan {
    /// Construct from an intent and a resolved operation, deriving capability
    /// and preconditions deterministically.
    #[must_use]
    pub fn for_operation(intent: DragIntent, operation: DragOperation) -> Self {
        let required_capability = operation.required_capability();
        let preconditions = operation.preconditions();
        Self {
            intent,
            operation,
            required_capability,
            preconditions,
        }
    }
}

/// Transport-neutral drag dispatch payload emitted on a successful drag
/// proposal. Mirrors [`Dispatch`] identity binding
/// (owner/generation/correlation/base revision) plus the complete semantic
/// drag plan. Desired topology/focus/geometry are carried by the session layer
/// so this envelope stays geometry-free like [`Dispatch`]; native execution
/// stays outside the reconciler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragDispatch {
    pub correlation_id: CorrelationId,
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub base_revision: u64,
    pub required_capability: DragCapability,
    pub preconditions: Vec<DragPrecondition>,
    pub intent: DragIntent,
    pub operation: DragOperation,
}

/// Fresh post-observation plus explicit native verification flag for a pending
/// drag plan. Binds exactly like [`PostObservation`]: the reported
/// `verified_preconditions` must equal the dispatched preconditions (capped by
/// [`MAX_PRECONDITIONS`]) and `verified_operation` must equal the dispatched
/// drag operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragPostObservation {
    pub observation: Observation,
    pub correlation_id: CorrelationId,
    pub verified: bool,
    pub verified_preconditions: Vec<DragPrecondition>,
    pub verified_operation: DragOperation,
}

impl DragPostObservation {
    /// Typed construction (ids already validated by `ids` parsers).
    #[must_use]
    pub fn new(
        observation: Observation,
        correlation_id: CorrelationId,
        verified: bool,
        verified_preconditions: Vec<DragPrecondition>,
        verified_operation: DragOperation,
    ) -> Self {
        Self {
            observation,
            correlation_id,
            verified,
            verified_preconditions,
            verified_operation,
        }
    }

    /// Validity without echoing input (observation plus correlation shape and
    /// bounded precondition vector).
    #[must_use]
    pub fn validate(&self) -> bool {
        self.observation.validate()
            && is_correlation_id(self.correlation_id.as_str())
            && self.verified_preconditions.len() <= MAX_PRECONDITIONS
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
    fn observation_validation_is_typed() {
        let good = Observation::from_strings("owner-1", "gen-1", 0, 7).expect("valid");
        assert!(good.validate());
        assert!(Observation::from_strings("bad owner", "gen-1", 0, 7).is_none());
        assert!(Observation::from_strings("owner-1", "gen-1", MAX_REVISION + 1, 7).is_none());
        assert!(
            AdapterAck::from_strings(
                "corr-1",
                "owner-1",
                "gen-1",
                MAX_REVISION + 1,
                AckOutcome::Accepted,
            )
            .is_none()
        );
        assert!(OwnerId::parse("bad owner").is_none());
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
        assert_eq!(DragCapability::PlaceTiled.as_str(), "place-tiled");
        assert!(DragCapabilities::full().supports(DragCapability::PlaceTiled));
        assert!(!DragCapabilities::none().supports(DragCapability::PlaceTiled));
    }
}
