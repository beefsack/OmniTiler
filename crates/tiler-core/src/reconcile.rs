//! Deterministic transport-independent reconciler.
//!
//! Portable state machine over [`crate::contract`] observations and already
//! computed [`crate::directional::MovePlan`]s plus lifecycle
//! [`crate::contract::LifecyclePlan`]s. No platform, process, IPC,
//! geometry, or native execution imports.
//!
//! State transitions:
//! - `Verified --propose--> PendingUnacked --acknowledge--> PendingAcked
//!   --verify--> Verified(base + 1)`.
//! - Movement (`propose`/`verify`), lifecycle
//!   (`propose_lifecycle`/`verify_lifecycle`), focus
//!   (`propose_focus`/`verify_focus`), resize
//!   (`propose_resize`/`propose_pointer_resize`/`verify_resize`), and drag
//!   (`propose_drag`/`verify_drag`) share at most one pending plan:
//!   a second proposal of any kind while pending is `PendingExists` without
//!   divergence; acknowledgement binds any kind by owner/generation/base
//!   revision/correlation; verification must use the matching kind-specific
//!   verifier.
//! - Any stale/mismatched/partial/capability-refusal/adapter-loss/unverified
//!   or mismatch condition records a typed [`DivergenceKind`] and enters
//!   terminal `Divergent`: no further dispatch or mutation.
//!
//! Duplicate/out-of-order semantics (simple, documented in type names):
//! - `ProposeRejected::PendingExists`: second proposal while pending is
//!   rejected without divergence and without state change.
//! - `AckApplied::DuplicateDiscarded`: an identical accepted acknowledgement
//!   replayed while already acknowledged is discarded without advancement.
//! - Any acknowledgement with no pending plan is `AckRejected::NoPending`
//!   (discarded, no divergence, no advancement).
//! - Verification before acknowledgement is
//!   `VerifyRejected::NotAcknowledged` (rejected, no divergence).
//! - A second verification after a commit finds no pending plan
//!   (`VerifyRejected::NoPending`); the revision advanced exactly once.
//! - Any acknowledgement/verification binding mismatch (owner, generation,
//!   base revision, correlation) diverges fail-closed.
//! - Non-accepted acknowledgement outcomes (refused/partial/lost) diverge
//!   fail-closed; arbitrary adapter detail is never stored or echoed.

use crate::contract::{
    AdapterAck, Dispatch, DivergenceKind, DragCapabilities, DragDispatch, DragOperation, DragPlan,
    DragPostObservation, DragPrecondition, FocusCapabilities, FocusDispatch, FocusOperation,
    FocusPlanContract, FocusPostObservation, FocusPrecondition, LIFECYCLE_POLICY_VERSION,
    LifecycleCapabilities, LifecycleDispatch, LifecycleOperation, LifecyclePlan,
    LifecyclePostObservation, LifecyclePrecondition, MAX_PRECONDITIONS, Observation,
    PostObservation, ResizeCapabilities, ResizeDispatch, ResizeOperation, ResizePlan,
    ResizePostObservation, ResizePrecondition, is_correlation_id, is_generation_id, is_owner_id,
    is_revision,
};
use crate::directional::{Capabilities, MovePlan, Precondition};
use crate::ids::{CorrelationId, GenerationId, OwnerId};

/// Visible reconciler state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    Verified,
    PendingUnacked,
    PendingAcked,
    Divergent,
}

/// Redacted status view: state, revision, pending flag, divergence. Carries
/// no opaque ids and no adapter detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusView {
    pub state: StateKind,
    pub revision: u64,
    pub pending: bool,
    pub divergence: Option<DivergenceKind>,
}

/// Successful commit receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Commit {
    pub revision: u64,
    pub fingerprint: u64,
}

/// Acknowledgement application outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckApplied {
    /// First matching accepted acknowledgement; pending is now acked.
    Accepted,
    /// Identical accepted acknowledgement replay; discarded, no advancement.
    DuplicateDiscarded,
}

/// Non-divergent-capable proposal failure. `Diverged` is terminal and records
/// [`DivergenceKind`] inside the reconciler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposeError {
    /// At most one pending plan: rejected without divergence or mutation.
    PendingExists,
    /// Fail-closed divergence; no further dispatch or mutation.
    Diverged(DivergenceKind),
}

impl ProposeError {
    /// Stable kind string, never echoes input.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::PendingExists => "pending-exists",
            Self::Diverged(reason) => reason.as_str(),
        }
    }

    /// Fixed redacted message, never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::PendingExists => "complete the pending plan before proposing",
            Self::Diverged(reason) => reason.message(),
        }
    }
}

/// Acknowledgement failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckError {
    /// No pending plan: discarded without divergence or advancement.
    NoPending,
    /// Fail-closed divergence.
    Diverged(DivergenceKind),
}

impl AckError {
    /// Stable kind string, never echoes input.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NoPending => "no-pending",
            Self::Diverged(reason) => reason.as_str(),
        }
    }

    /// Fixed redacted message, never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoPending => "no dispatched plan awaits acknowledgement",
            Self::Diverged(reason) => reason.message(),
        }
    }
}

/// Verification failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// No pending plan: rejected without divergence.
    NoPending,
    /// Pending plan not yet acknowledged: rejected without divergence.
    NotAcknowledged,
    /// Fail-closed divergence.
    Diverged(DivergenceKind),
}

impl VerifyError {
    /// Stable kind string, never echoes input.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NoPending => "no-pending",
            Self::NotAcknowledged => "not-acknowledged",
            Self::Diverged(reason) => reason.as_str(),
        }
    }

    /// Fixed redacted message, never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoPending => "no dispatched plan awaits verification",
            Self::NotAcknowledged => "acknowledge the dispatched plan first",
            Self::Diverged(reason) => reason.message(),
        }
    }
}

/// Construction failure (pre-state, never divergence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewError {
    InvalidOwner,
    InvalidGeneration,
    RevisionOutOfBounds,
}

impl NewError {
    /// Stable kind string.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::InvalidOwner => "owner-invalid",
            Self::InvalidGeneration => "generation-invalid",
            Self::RevisionOutOfBounds => "revision-out-of-bounds",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingKind {
    Move {
        preconditions: Vec<Precondition>,
        operation: crate::directional::MoveOperation,
    },
    Lifecycle {
        preconditions: Vec<LifecyclePrecondition>,
        operation: LifecycleOperation,
    },
    Focus {
        preconditions: Vec<FocusPrecondition>,
        operation: FocusOperation,
    },
    Resize {
        preconditions: Vec<ResizePrecondition>,
        operation: ResizeOperation,
        /// Second-axis operation for an atomic corner pointer resize;
        /// `None` for every single-axis plan. Boxed: the single pending
        /// slot keeps its single-axis footprint.
        secondary_operation: Option<Box<ResizeOperation>>,
    },
    Drag {
        preconditions: Vec<DragPrecondition>,
        operation: DragOperation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    correlation_id: CorrelationId,
    base_revision: u64,
    acked: bool,
    kind: PendingKind,
}

/// Deterministic reconciler: at most one pending plan bound exactly to
/// owner/generation/base revision/correlation plus plan preconditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciler {
    owner: OwnerId,
    generation: GenerationId,
    verified_revision: u64,
    verified_fingerprint: u64,
    pending: Option<Pending>,
    diverged: Option<DivergenceKind>,
}

/// Non-divergent cancellation failure. Unlike acknowledgement/verification
/// mismatches, a cancellation probe must never record divergence: it only
/// reports whether the pending plan may be withdrawn, so a stale or foreign
/// probe leaves the live transaction exactly untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelUnackedError {
    /// No pending plan: discarded without divergence or advancement.
    NoPending,
    /// Pending plan already acknowledged: withdrawal refused without
    /// divergence or state change (an applied ack may already have committed
    /// elsewhere, including behind a lost commit reply).
    AlreadyAcknowledged,
    /// Correlation or base revision does not bind the pending plan: refused
    /// without divergence or state change.
    BindingMismatch,
    /// A transient drag capture remains active: withdrawal refused without
    /// divergence or state change.
    DragActive,
    /// Fail-closed divergence already recorded.
    Diverged(DivergenceKind),
}

impl CancelUnackedError {
    /// Stable kind string, never echoes input.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NoPending => "no-pending",
            Self::AlreadyAcknowledged => "already-acknowledged",
            Self::BindingMismatch => "binding-mismatch",
            Self::DragActive => "drag-active",
            Self::Diverged(reason) => reason.as_str(),
        }
    }

    /// Fixed redacted message, never echoes input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoPending => "no dispatched plan awaits cancellation",
            Self::AlreadyAcknowledged => "plan was already acknowledged",
            Self::BindingMismatch => "cancellation does not bind the pending plan",
            Self::DragActive => "pending plan holds a drag capture",
            Self::Diverged(reason) => reason.message(),
        }
    }
}

impl Reconciler {
    /// Pin stable owner/generation metadata and seed the verified revision.
    pub fn new(
        owner: OwnerId,
        generation: GenerationId,
        initial_revision: u64,
        initial_fingerprint: u64,
    ) -> Result<Self, NewError> {
        if !crate::contract::is_owner_id(owner.as_str()) {
            return Err(NewError::InvalidOwner);
        }
        if !crate::contract::is_generation_id(generation.as_str()) {
            return Err(NewError::InvalidGeneration);
        }
        if !crate::contract::is_revision(initial_revision) {
            return Err(NewError::RevisionOutOfBounds);
        }
        Ok(Self {
            owner,
            generation,
            verified_revision: initial_revision,
            verified_fingerprint: initial_fingerprint,
            pending: None,
            diverged: None,
        })
    }

    /// Narrow string boundary: parses session ids without echo.
    pub fn new_from_strings(
        owner: &str,
        generation: &str,
        initial_revision: u64,
        initial_fingerprint: u64,
    ) -> Result<Self, NewError> {
        let Some(owner) = OwnerId::parse(owner) else {
            return Err(NewError::InvalidOwner);
        };
        let Some(generation) = GenerationId::parse(generation) else {
            return Err(NewError::InvalidGeneration);
        };
        Self::new(owner, generation, initial_revision, initial_fingerprint)
    }

    /// Current verified revision.
    #[must_use]
    pub const fn verified_revision(&self) -> u64 {
        self.verified_revision
    }

    /// Recorded divergence, if fail-closed.
    #[must_use]
    pub const fn divergence(&self) -> Option<DivergenceKind> {
        self.diverged
    }

    /// Redacted status view.
    #[must_use]
    pub fn status(&self) -> StatusView {
        let state = if self.diverged.is_some() {
            StateKind::Divergent
        } else if let Some(pending) = &self.pending {
            if pending.acked {
                StateKind::PendingAcked
            } else {
                StateKind::PendingUnacked
            }
        } else {
            StateKind::Verified
        };
        StatusView {
            state,
            revision: self.verified_revision,
            pending: self.pending.is_some(),
            divergence: self.diverged,
        }
    }

    fn diverge(&mut self, reason: DivergenceKind) -> DivergenceKind {
        self.diverged = Some(reason);
        self.pending = None;
        reason
    }

    /// Propose an already-computed plan against a normalized observation.
    ///
    /// Binds exactly to owner/generation/base revision/correlation plus the
    /// plan's preconditions and declared capabilities; emits a
    /// transport-neutral [`Dispatch`]. Native execution stays outside.
    pub fn propose(
        &mut self,
        plan: &MovePlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &Capabilities,
    ) -> Result<Dispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            // Validate shape first so malformed ids still fail closed as a
            // typed mismatch without echoing which field carried input.
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        // Reject internally inconsistent hand-built plans before dispatch.
        // The plan must exactly match its own semantic operation via the
        // existing directional derivation; any divergence fails closed.
        if plan.rule != plan.operation.rule() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != plan.operation.required_capability() {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(plan.operation.required_capability()) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&Precondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        // Bound the cloned precondition vector (trusted planner input, but
        // still capped so dispatch stays bounded).
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = Dispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            rule: plan.rule,
            operation: plan.operation.clone(),
            intent: plan.intent.clone(),
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Move {
                preconditions,
                operation: plan.operation.clone(),
            },
        });
        Ok(dispatch)
    }

    /// Propose an already-computed lifecycle plan against a normalized
    /// observation.
    ///
    /// Shares the single pending slot with movement [`Reconciler::propose`]:
    /// at most one pending plan of either kind; acknowledgement
    /// ([`Reconciler::acknowledge`]) binds either kind identically, while
    /// verification must use the kind-specific verifier
    /// ([`Reconciler::verify`] for moves, [`Reconciler::verify_lifecycle`]
    /// here). Binds exactly to owner/generation/base revision/correlation
    /// plus the lifecycle plan's preconditions and declared lifecycle
    /// capabilities; emits a transport-neutral [`LifecycleDispatch`].
    pub fn propose_lifecycle(
        &mut self,
        plan: &LifecyclePlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &LifecycleCapabilities,
    ) -> Result<LifecycleDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        // Reject internally inconsistent hand-built lifecycle plans before
        // dispatch; mirrors the movement consistency gate. The portable
        // lifecycle policy binding (`cosmic_v1`) is validated here; frozen
        // R1-R4 movement plans never carry it.
        if plan.policy_version != LIFECYCLE_POLICY_VERSION || !plan.valid_policy() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != plan.operation.required_capability() {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(plan.operation.required_capability()) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&LifecyclePrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = LifecycleDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            intent: plan.intent.clone(),
            operation: plan.operation.clone(),
            policy_version: plan.policy_version,
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Lifecycle {
                preconditions,
                operation: plan.operation.clone(),
            },
        });
        Ok(dispatch)
    }

    /// Propose an already-computed focus plan against a normalized observation.
    ///
    /// Shares the single pending slot with movement and lifecycle: at most one
    /// pending plan of any kind; acknowledgement binds identically, while
    /// verification must use [`Reconciler::verify_focus`] here. Binds exactly
    /// to owner/generation/base revision/correlation plus the focus plan's
    /// preconditions and declared focus capabilities; emits a
    /// transport-neutral [`FocusDispatch`].
    pub fn propose_focus(
        &mut self,
        plan: &FocusPlanContract,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &FocusCapabilities,
    ) -> Result<FocusDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != plan.operation.required_capability() {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(plan.operation.required_capability()) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&FocusPrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.operation.from_leaf.as_ref() == Some(&plan.operation.to_leaf) {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        // Leafless (float-origin) operations ride the explicit cross-output
        // route only: a missing `from_leaf` without a cross source diverges.
        if plan.operation.from_leaf.is_none() && !plan.operation.is_cross_output() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.operation.route.is_empty() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = FocusDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            intent: plan.intent.clone(),
            operation: plan.operation.clone(),
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Focus {
                preconditions,
                operation: plan.operation.clone(),
            },
        });
        Ok(dispatch)
    }

    /// Propose an already-computed resize plan against a normalized observation.
    ///
    /// Shares the single pending slot with movement, lifecycle, focus, and
    /// drag: at
    /// most one pending plan of any kind; acknowledgement binds identically,
    /// while verification must use [`Reconciler::verify_resize`] here. Binds
    /// exactly to owner/generation/base revision/correlation plus the resize
    /// plan's preconditions and declared resize capabilities; emits a
    /// transport-neutral [`ResizeDispatch`]. The keyboard route requires
    /// [`crate::contract::ResizeCapability::KeyboardResize`] independently of
    /// the pointer route.
    pub fn propose_resize(
        &mut self,
        plan: &ResizePlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<ResizeDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != crate::contract::ResizeCapability::KeyboardResize
            || plan.required_capability != plan.operation.required_capability()
        {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::KeyboardResize) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&ResizePrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.intent.domain_output != plan.operation.domain_output
            || plan.intent.domain_workspace != plan.operation.domain_workspace
            || plan.intent.focused_leaf != plan.operation.focused_leaf
            || plan.intent.focused_window != plan.operation.focused_window
            || plan.intent.direction != plan.operation.direction
            || plan.intent.mode != plan.operation.mode
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !valid_resize_operation(&plan.operation) {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = ResizeDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            intent: plan.intent.clone(),
            operation: plan.operation.clone(),
            secondary_operation: None,
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Resize {
                preconditions,
                operation: plan.operation.clone(),
                secondary_operation: None,
            },
        });
        Ok(dispatch)
    }

    /// Propose an already-computed pointer split-share resize plan.
    ///
    /// Shares the single pending slot with every other kind; acknowledgement
    /// binds identically and verification reuses [`Reconciler::verify_resize`].
    /// Binds exactly to owner/generation/base revision/correlation plus the
    /// resize plan's preconditions and declared resize capabilities, requiring
    /// [`crate::contract::ResizeCapability::PointerResize`] independently of
    /// the keyboard route. Unlike
    /// [`Reconciler::propose_resize`], the share transfer is not required to
    /// carry keyboard mode semantics: [`valid_fixed_share_operation`]
    /// accepts any adjacent-only redistribution preserving positivity with
    /// an optional exact whole-group ratio-preserving integer scaling.
    pub fn propose_pointer_resize(
        &mut self,
        plan: &ResizePlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<ResizeDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != crate::contract::ResizeCapability::PointerResize {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::PointerResize) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&ResizePrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.intent.domain_output != plan.operation.domain_output
            || plan.intent.domain_workspace != plan.operation.domain_workspace
            || plan.intent.focused_leaf != plan.operation.focused_leaf
            || plan.intent.focused_window != plan.operation.focused_window
            || plan.intent.direction != plan.operation.direction
            || plan.intent.mode != plan.operation.mode
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !valid_fixed_share_operation(&plan.operation) {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = ResizeDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            intent: plan.intent.clone(),
            operation: plan.operation.clone(),
            secondary_operation: None,
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Resize {
                preconditions,
                operation: plan.operation.clone(),
                secondary_operation: None,
            },
        });
        Ok(dispatch)
    }

    /// Propose an already-computed atomic corner (dual-axis) pointer
    /// split-share resize plan: one horizontal plus one vertical axis from a
    /// single `pointer-resize` request.
    ///
    /// Shares the single pending slot with every other kind: exactly one
    /// pending entry carries both operations, acknowledgement binds
    /// identically, and verification reuses [`Reconciler::verify_resize`]
    /// with both operation echoes bound. Each axis plan must independently
    /// satisfy the single-axis pointer checks (capability, preconditions,
    /// intent/operation consistency, adjacent-only share validity), and the
    /// two axes must differ (one horizontal, one vertical).
    /// `primary` selects the dispatch intent/operation; `secondary` rides as
    /// the bound second axis.
    pub fn propose_pointer_resize_corner(
        &mut self,
        primary: &ResizePlan,
        secondary: &ResizePlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &ResizeCapabilities,
    ) -> Result<ResizeDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        for plan in [primary, secondary] {
            if plan.required_capability != crate::contract::ResizeCapability::PointerResize {
                let reason = self.diverge(DivergenceKind::CapabilityRefused);
                return Err(ProposeError::Diverged(reason));
            }
            if plan.preconditions != plan.operation.preconditions() {
                let reason = self.diverge(DivergenceKind::PostconditionMismatch);
                return Err(ProposeError::Diverged(reason));
            }
            if plan.intent.domain_output != plan.operation.domain_output
                || plan.intent.domain_workspace != plan.operation.domain_workspace
                || plan.intent.focused_leaf != plan.operation.focused_leaf
                || plan.intent.focused_window != plan.operation.focused_window
                || plan.intent.direction != plan.operation.direction
                || plan.intent.mode != plan.operation.mode
            {
                let reason = self.diverge(DivergenceKind::PostconditionMismatch);
                return Err(ProposeError::Diverged(reason));
            }
            if !valid_fixed_share_operation(&plan.operation) {
                let reason = self.diverge(DivergenceKind::PostconditionMismatch);
                return Err(ProposeError::Diverged(reason));
            }
        }
        if primary.preconditions != secondary.preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if crate::directional::Axis::for_direction(primary.operation.direction)
            == crate::directional::Axis::for_direction(secondary.operation.direction)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(crate::contract::ResizeCapability::PointerResize) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if primary.preconditions.len() > MAX_PRECONDITIONS
            || !primary
                .preconditions
                .contains(&ResizePrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if primary.intent.domain_output != secondary.intent.domain_output
            || primary.intent.domain_workspace != secondary.intent.domain_workspace
            || primary.intent.focused_leaf != secondary.intent.focused_leaf
            || primary.intent.focused_window != secondary.intent.focused_window
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(primary.preconditions.len());
        preconditions.extend_from_slice(&primary.preconditions);
        let dispatch = ResizeDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: primary.required_capability,
            preconditions: preconditions.clone(),
            intent: primary.intent.clone(),
            operation: primary.operation.clone(),
            secondary_operation: Some(Box::new(secondary.operation.clone())),
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Resize {
                preconditions,
                operation: primary.operation.clone(),
                secondary_operation: Some(Box::new(secondary.operation.clone())),
            },
        });
        Ok(dispatch)
    }

    /// Withdraw an unacknowledged pending plan without recording divergence.
    /// Requires an exact correlation/base-revision binding against the pending
    /// plan; owner/generation binding is enforced by the caller, which retains
    /// the authoritative copies. On success the pending slot clears with no
    /// revision advancement and no divergence recorded. Any failure leaves
    /// state exactly untouched, including already-recorded divergence.
    pub fn cancel_unacked(
        &mut self,
        correlation_id: &CorrelationId,
        base_revision: u64,
    ) -> Result<(), CancelUnackedError> {
        if let Some(reason) = self.diverged {
            return Err(CancelUnackedError::Diverged(reason));
        }
        let Some(pending) = self.pending.as_ref() else {
            return Err(CancelUnackedError::NoPending);
        };
        if pending.acked {
            return Err(CancelUnackedError::AlreadyAcknowledged);
        }
        if pending.correlation_id != *correlation_id || pending.base_revision != base_revision {
            return Err(CancelUnackedError::BindingMismatch);
        }
        self.pending = None;
        Ok(())
    }

    /// Record an explicit adapter acknowledgement. Requires an exact binding
    /// match; non-accepted outcomes diverge fail-closed. Malformed shapes
    /// classify to their own typed kind (owner/generation/revision/
    /// correlation) without echo.
    pub fn acknowledge(&mut self, ack: &AdapterAck) -> Result<AckApplied, AckError> {
        if let Some(reason) = self.diverged {
            return Err(AckError::Diverged(reason));
        }
        let Some(pending) = self.pending.as_mut() else {
            return Err(AckError::NoPending);
        };
        if !is_correlation_id(ack.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(AckError::Diverged(reason));
        }
        if !is_owner_id(ack.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(AckError::Diverged(reason));
        }
        if !is_generation_id(ack.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(AckError::Diverged(reason));
        }
        if !is_revision(ack.base_revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(AckError::Diverged(reason));
        }
        if ack.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(AckError::Diverged(reason));
        }
        if ack.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(AckError::Diverged(reason));
        }
        if ack.correlation_id != pending.correlation_id
            || ack.base_revision != pending.base_revision
        {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(AckError::Diverged(reason));
        }
        match ack.outcome {
            crate::contract::AckOutcome::Accepted => {
                if pending.acked {
                    Ok(AckApplied::DuplicateDiscarded)
                } else {
                    pending.acked = true;
                    Ok(AckApplied::Accepted)
                }
            }
            crate::contract::AckOutcome::RefusedCapability => {
                let reason = self.diverge(DivergenceKind::CapabilityRefused);
                Err(AckError::Diverged(reason))
            }
            crate::contract::AckOutcome::PartialApplication => {
                let reason = self.diverge(DivergenceKind::PartialApplication);
                Err(AckError::Diverged(reason))
            }
            crate::contract::AckOutcome::AdapterLost => {
                let reason = self.diverge(DivergenceKind::AdapterLost);
                Err(AckError::Diverged(reason))
            }
        }
    }

    /// Commit after acknowledgement given a matching fresh post-observation
    /// with explicit native verification. The reported
    /// `verified_preconditions` must bind exactly to the dispatched
    /// preconditions (capped by [`MAX_PRECONDITIONS`]) and the reported
    /// `verified_operation` must bind exactly to the dispatched semantic
    /// operation; any mismatch diverges
    /// as [`DivergenceKind::PostconditionMismatch`] with no commit. Advances
    /// verified state by exactly one revision.
    pub fn verify(&mut self, post: &PostObservation) -> Result<Commit, VerifyError> {
        if let Some(reason) = self.diverged {
            return Err(VerifyError::Diverged(reason));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(VerifyError::NoPending);
        };
        if !pending.acked {
            return Err(VerifyError::NotAcknowledged);
        }
        // Cap the adapter-reported vector before shape validation so an
        // overlong report maps to PostconditionMismatch, not a correlation
        // shape error.
        if post.verified_preconditions.len() > MAX_PRECONDITIONS {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_correlation_id(post.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_owner_id(post.observation.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_generation_id(post.observation.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_revision(post.observation.revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if post.correlation_id != pending.correlation_id {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.revision != pending.base_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if !post.verified {
            let reason = self.diverge(DivergenceKind::PostconditionUnverified);
            return Err(VerifyError::Diverged(reason));
        }
        let PendingKind::Move {
            preconditions,
            operation,
        } = &pending.kind
        else {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        };
        if post.verified_preconditions != *preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_operation != *operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if pending.base_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(VerifyError::Diverged(reason));
        }
        self.verified_revision = pending.base_revision + 1;
        self.verified_fingerprint = post.observation.fingerprint;
        self.pending = None;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }

    /// Commit after acknowledgement given a matching fresh lifecycle
    /// post-observation with explicit native verification. Mirrors
    /// [`Reconciler::verify`] for [`LifecyclePostObservation`]: the reported
    /// verified preconditions/operation must bind exactly to the pending
    /// lifecycle dispatch. Advances verified state by exactly one revision.
    /// Calling this while a movement plan is pending (or [`Reconciler::verify`]
    /// while a lifecycle plan is pending) diverges as
    /// [`DivergenceKind::PostconditionMismatch`].
    pub fn verify_lifecycle(
        &mut self,
        post: &LifecyclePostObservation,
    ) -> Result<Commit, VerifyError> {
        if let Some(reason) = self.diverged {
            return Err(VerifyError::Diverged(reason));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(VerifyError::NoPending);
        };
        if !pending.acked {
            return Err(VerifyError::NotAcknowledged);
        }
        if post.verified_preconditions.len() > MAX_PRECONDITIONS {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_correlation_id(post.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_owner_id(post.observation.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_generation_id(post.observation.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_revision(post.observation.revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if post.correlation_id != pending.correlation_id {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.revision != pending.base_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if !post.verified {
            let reason = self.diverge(DivergenceKind::PostconditionUnverified);
            return Err(VerifyError::Diverged(reason));
        }
        let PendingKind::Lifecycle {
            preconditions,
            operation,
        } = &pending.kind
        else {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        };
        if post.verified_preconditions != *preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_operation != *operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if pending.base_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(VerifyError::Diverged(reason));
        }
        self.verified_revision = pending.base_revision + 1;
        self.verified_fingerprint = post.observation.fingerprint;
        self.pending = None;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }

    /// Commit after acknowledgement given a matching fresh focus
    /// post-observation with explicit native verification. Mirrors
    /// [`Reconciler::verify`] for [`FocusPostObservation`]: the reported
    /// verified preconditions/operation must bind exactly to the pending focus
    /// dispatch. Advances verified state by exactly one revision. Calling this
    /// while a movement or lifecycle plan is pending (or those verifiers while
    /// a focus plan is pending) diverges as
    /// [`DivergenceKind::PostconditionMismatch`].
    pub fn verify_focus(&mut self, post: &FocusPostObservation) -> Result<Commit, VerifyError> {
        if let Some(reason) = self.diverged {
            return Err(VerifyError::Diverged(reason));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(VerifyError::NoPending);
        };
        if !pending.acked {
            return Err(VerifyError::NotAcknowledged);
        }
        if post.verified_preconditions.len() > MAX_PRECONDITIONS {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_correlation_id(post.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_owner_id(post.observation.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_generation_id(post.observation.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_revision(post.observation.revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if post.correlation_id != pending.correlation_id {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.revision != pending.base_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if !post.verified {
            let reason = self.diverge(DivergenceKind::PostconditionUnverified);
            return Err(VerifyError::Diverged(reason));
        }
        let PendingKind::Focus {
            preconditions,
            operation,
        } = &pending.kind
        else {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        };
        if post.verified_preconditions != *preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_operation != *operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if pending.base_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(VerifyError::Diverged(reason));
        }
        self.verified_revision = pending.base_revision + 1;
        self.verified_fingerprint = post.observation.fingerprint;
        self.pending = None;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }

    /// Commit after acknowledgement given a matching fresh resize
    /// post-observation with explicit native verification. Mirrors
    /// [`Reconciler::verify`] for [`ResizePostObservation`]: the reported
    /// verified preconditions/operation must bind exactly to the pending resize
    /// dispatch. Advances verified state by exactly one revision. Calling this
    /// while a movement, lifecycle, focus, or drag plan is pending (or those
    /// verifiers while a resize plan is pending) diverges as
    /// [`DivergenceKind::PostconditionMismatch`].
    pub fn verify_resize(&mut self, post: &ResizePostObservation) -> Result<Commit, VerifyError> {
        if let Some(reason) = self.diverged {
            return Err(VerifyError::Diverged(reason));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(VerifyError::NoPending);
        };
        if !pending.acked {
            return Err(VerifyError::NotAcknowledged);
        }
        if post.verified_preconditions.len() > MAX_PRECONDITIONS {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_correlation_id(post.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_owner_id(post.observation.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_generation_id(post.observation.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_revision(post.observation.revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if post.correlation_id != pending.correlation_id {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.revision != pending.base_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if !post.verified {
            let reason = self.diverge(DivergenceKind::PostconditionUnverified);
            return Err(VerifyError::Diverged(reason));
        }
        let PendingKind::Resize {
            preconditions,
            operation,
            secondary_operation,
        } = &pending.kind
        else {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        };
        if post.verified_preconditions != *preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_operation != *operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_secondary_operation != *secondary_operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if pending.base_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(VerifyError::Diverged(reason));
        }
        self.verified_revision = pending.base_revision + 1;
        self.verified_fingerprint = post.observation.fingerprint;
        self.pending = None;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }

    /// Propose an already-computed drag plan against a normalized observation.
    ///
    /// Shares the single pending slot with movement, lifecycle, focus, and
    /// resize: at most one pending plan of any kind; acknowledgement binds
    /// identically, while verification must use [`Reconciler::verify_drag`]
    /// here. Binds exactly to owner/generation/base revision/correlation plus
    /// the drag plan's preconditions and declared drag capabilities; emits a
    /// transport-neutral [`DragDispatch`].
    pub fn propose_drag(
        &mut self,
        plan: &DragPlan,
        observation: &Observation,
        correlation_id: &CorrelationId,
        capabilities: &DragCapabilities,
    ) -> Result<DragDispatch, ProposeError> {
        if let Some(reason) = self.diverged {
            return Err(ProposeError::Diverged(reason));
        }
        if self.pending.is_some() {
            return Err(ProposeError::PendingExists);
        }
        if !is_correlation_id(correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !observation.validate() || observation.owner != self.owner {
            let reason = if observation.owner != self.owner {
                self.diverge(DivergenceKind::OwnerMismatch)
            } else if !crate::contract::is_generation_id(observation.generation.as_str()) {
                self.diverge(DivergenceKind::GenerationMismatch)
            } else {
                self.diverge(DivergenceKind::StaleRevision)
            };
            return Err(ProposeError::Diverged(reason));
        }
        if observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if observation.revision != self.verified_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(ProposeError::Diverged(reason));
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.required_capability != plan.operation.required_capability() {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions != plan.operation.preconditions() {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !capabilities.supports(plan.operation.required_capability()) {
            let reason = self.diverge(DivergenceKind::CapabilityRefused);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.preconditions.len() > MAX_PRECONDITIONS
            || !plan
                .preconditions
                .contains(&DragPrecondition::AdapterMustVerifyPostconditions)
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if plan.intent.domain_output != plan.operation.domain_output
            || plan.intent.domain_workspace != plan.operation.domain_workspace
            || plan.intent.source_leaf != plan.operation.source_leaf
            || plan.intent.source_window != plan.operation.source_window
            || plan.intent.target_leaf != plan.operation.target_leaf
            || plan.intent.target_window != plan.operation.target_window
            || plan.intent.side != plan.operation.side
            || plan.intent.cross_source != plan.operation.cross_source
        {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        if !valid_drag_operation(&plan.operation) {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(ProposeError::Diverged(reason));
        }
        let mut preconditions = Vec::with_capacity(plan.preconditions.len());
        preconditions.extend_from_slice(&plan.preconditions);
        let dispatch = DragDispatch {
            correlation_id: correlation_id.clone(),
            owner: self.owner.clone(),
            generation: self.generation.clone(),
            base_revision: self.verified_revision,
            required_capability: plan.required_capability,
            preconditions: preconditions.clone(),
            intent: plan.intent.clone(),
            operation: plan.operation.clone(),
        };
        self.pending = Some(Pending {
            correlation_id: correlation_id.clone(),
            base_revision: self.verified_revision,
            acked: false,
            kind: PendingKind::Drag {
                preconditions,
                operation: plan.operation.clone(),
            },
        });
        Ok(dispatch)
    }

    /// Commit after acknowledgement given a matching fresh drag
    /// post-observation with explicit native verification. Mirrors
    /// [`Reconciler::verify`] for [`DragPostObservation`]: the reported
    /// verified preconditions/operation must bind exactly to the pending drag
    /// dispatch. Advances verified state by exactly one revision. Calling this
    /// while a movement, lifecycle, focus, or resize plan is pending (or those
    /// verifiers while a drag plan is pending) diverges as
    /// [`DivergenceKind::PostconditionMismatch`].
    pub fn verify_drag(&mut self, post: &DragPostObservation) -> Result<Commit, VerifyError> {
        if let Some(reason) = self.diverged {
            return Err(VerifyError::Diverged(reason));
        }
        let Some(pending) = self.pending.clone() else {
            return Err(VerifyError::NoPending);
        };
        if !pending.acked {
            return Err(VerifyError::NotAcknowledged);
        }
        if post.verified_preconditions.len() > MAX_PRECONDITIONS {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_correlation_id(post.correlation_id.as_str()) {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_owner_id(post.observation.owner.as_str()) {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_generation_id(post.observation.generation.as_str()) {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if !is_revision(post.observation.revision) {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if post.correlation_id != pending.correlation_id {
            let reason = self.diverge(DivergenceKind::CorrelationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.owner != self.owner {
            let reason = self.diverge(DivergenceKind::OwnerMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.generation != self.generation {
            let reason = self.diverge(DivergenceKind::GenerationMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.observation.revision != pending.base_revision {
            let reason = self.diverge(DivergenceKind::StaleRevision);
            return Err(VerifyError::Diverged(reason));
        }
        if !post.verified {
            let reason = self.diverge(DivergenceKind::PostconditionUnverified);
            return Err(VerifyError::Diverged(reason));
        }
        let PendingKind::Drag {
            preconditions,
            operation,
        } = &pending.kind
        else {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        };
        if post.verified_preconditions != *preconditions {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if post.verified_operation != *operation {
            let reason = self.diverge(DivergenceKind::PostconditionMismatch);
            return Err(VerifyError::Diverged(reason));
        }
        if pending.base_revision >= crate::contract::MAX_REVISION {
            let reason = self.diverge(DivergenceKind::RevisionExhausted);
            return Err(VerifyError::Diverged(reason));
        }
        self.verified_revision = pending.base_revision + 1;
        self.verified_fingerprint = post.observation.fingerprint;
        self.pending = None;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }

    /// Explicit adapter-loss signal: enters typed fail-closed divergence with
    /// no further dispatch or mutation.
    pub fn note_adapter_loss(&mut self) -> DivergenceKind {
        if let Some(reason) = self.diverged {
            return reason;
        }
        self.diverge(DivergenceKind::AdapterLost)
    }

    /// Explicit post-observation mismatch signal for the standalone
    /// workspace-send route: terminal `PostconditionMismatch` divergence,
    /// pending discarded.
    pub fn note_postcondition_mismatch(&mut self) -> DivergenceKind {
        if let Some(reason) = self.diverged {
            return reason;
        }
        self.diverge(DivergenceKind::PostconditionMismatch)
    }

    /// Record a fail-closed binding divergence for direct observation
    /// convergence (owner/generation/revision mismatch). Mirrors the
    /// propose-path divergence without staging a pending plan.
    pub(crate) fn diverge_convergence(&mut self, reason: DivergenceKind) -> DivergenceKind {
        if let Some(existing) = self.diverged {
            return existing;
        }
        self.diverge(reason)
    }

    /// Advance verified revision/fingerprint for one atomic observation
    /// convergence. Requires no pending plan and no recorded divergence;
    /// advances by exactly one revision. The caller guards revision
    /// exhaustion before calling; every failure here leaves the reconciler
    /// untouched so a transient observation never bricks the session.
    pub(crate) fn commit_observation_convergence(
        &mut self,
        fingerprint: u64,
    ) -> Result<Commit, DivergenceKind> {
        if let Some(reason) = self.diverged {
            return Err(reason);
        }
        if self.pending.is_some() {
            return Err(DivergenceKind::PostconditionMismatch);
        }
        if self.verified_revision >= crate::contract::MAX_REVISION {
            return Err(DivergenceKind::RevisionExhausted);
        }
        self.verified_revision += 1;
        self.verified_fingerprint = fingerprint;
        Ok(Commit {
            revision: self.verified_revision,
            fingerprint: self.verified_fingerprint,
        })
    }
}

fn resize_step_for(direction: crate::directional::Direction) -> i32 {
    match direction {
        crate::directional::Direction::Right | crate::directional::Direction::Down => 1,
        crate::directional::Direction::Left | crate::directional::Direction::Up => -1,
    }
}

/// Topology-free drag operation validity: portable ids are non-empty, source
/// and target are distinct, `axis`/`before` derive exactly from `side`, and
/// the wrap form carries a fresh distinct `new_group` id (inserts carry none).
/// A root target names itself as its own `target_group` under `wrap`; group
/// same-axis N-ary inserts name the target group itself (target_group ==
/// target_leaf, wrap false, source GroupEdge/Interior); any other placement
/// names a distinct parent group. Structural bindings against the live
/// topology are validated by the session layer.
fn valid_drag_operation(operation: &DragOperation) -> bool {
    if operation.domain_output.0.is_empty()
        || operation.domain_workspace.0.is_empty()
        || operation.source_leaf.0.is_empty()
        || operation.source_window.0.is_empty()
        || operation.target_leaf.0.is_empty()
        || operation.target_window.0.is_empty()
        || operation.target_group.0.is_empty()
    {
        return false;
    }
    if operation.source_leaf == operation.target_leaf
        || operation.source_window == operation.target_window
    {
        return false;
    }
    // Cross-output operations name a source domain distinct from the target
    // domain; same-domain operations carry no cross source.
    if operation.is_cross_output() {
        let Some((ref source_output, ref source_workspace)) = operation.cross_source else {
            return false;
        };
        if source_output.0.is_empty() || source_workspace.0.is_empty() {
            return false;
        }
    } else if operation.cross_source.is_some() {
        return false;
    }
    let (Some(side_axis), Some(side_before)) = (operation.side.axis(), operation.side.before())
    else {
        // Center stack fact never validates as an operation.
        return false;
    };
    if operation.axis != side_axis || operation.before != side_before {
        return false;
    }
    if operation.wrap {
        let Some(new_group) = &operation.new_group else {
            return false;
        };
        if new_group.0.is_empty()
            || new_group == &operation.source_leaf
            || new_group == &operation.target_leaf
            || new_group == &operation.target_group
        {
            return false;
        }
        // Root targets name themselves with insertion index 0; otherwise the
        // parent is distinct from both leaves.
        if operation.target_group == operation.target_leaf {
            if operation.insertion_index != 0 {
                return false;
            }
        } else if operation.target_group == operation.source_leaf {
            return false;
        }
    } else {
        if operation.new_group.is_some() {
            return false;
        }
        // Window same-axis inserts name a distinct parent; group same-axis
        // N-ary inserts name the target group itself (target_group ==
        // target_leaf). Only the source leaf is forbidden here.
        if operation.target_group == operation.source_leaf {
            return false;
        }
    }
    true
}

fn valid_resize_operation(operation: &ResizeOperation) -> bool {
    // Explicit keyboard reconciliation boundary: the fixed-share relation
    // below plus keyboard mode semantics. Source COSMIC keyboard shares are
    // pixel-derived and intentionally not the 1/16 project step, so this
    // never uses `directional::expected_resize_shares`. The one-sided
    // keyboard clamp (`cosmic_v1::clamp_keyboard_shrink_pair`) is enforced at
    // derivation in the session layer; reconciliation validates the resulting
    // fixed share/semantic operation (adjacent pair moves, non-pair shares
    // scale exactly, focused share moves in the mode direction) before it may
    // commit.
    valid_fixed_share_operation(operation) && valid_keyboard_resize_mode(operation)
}

/// Keyboard-specific semantic validation: focus share movement direction must
/// match mode after the exact whole-group scale used by the neutral
/// fixed-share relation. `Inwards` must shrink focused, `Outwards` must grow
/// focused. Scale derives safely from old/new totals; any overflow fails
/// false.
fn valid_keyboard_resize_mode(operation: &ResizeOperation) -> bool {
    if operation.old_shares.len() != operation.new_shares.len()
        || operation.focused_index >= operation.old_shares.len()
    {
        return false;
    }
    let mut old_total: u64 = 0;
    for share in &operation.old_shares {
        match old_total.checked_add(*share) {
            Some(next) => old_total = next,
            None => return false,
        }
    }
    let mut new_total: u64 = 0;
    for share in &operation.new_shares {
        match new_total.checked_add(*share) {
            Some(next) => new_total = next,
            None => return false,
        }
    }
    if old_total == 0 || new_total == 0 || !new_total.is_multiple_of(old_total) {
        return false;
    }
    let scale = new_total / old_total;
    if scale == 0 {
        return false;
    }
    match old_total.checked_mul(scale) {
        Some(expected) if expected == new_total => {}
        _ => return false,
    }
    let scaled = match operation.old_shares[operation.focused_index].checked_mul(scale) {
        Some(value) => value,
        None => return false,
    };
    let new_focused = operation.new_shares[operation.focused_index];
    match operation.mode {
        crate::contract::ResizeMode::Inwards => new_focused < scaled,
        crate::contract::ResizeMode::Outwards => new_focused > scaled,
    }
}

/// Neutral fixed-share operation validity: identity/shape/direction binding
/// plus any adjacent-only redistribution preserving positivity, with an
/// optional exact whole-group ratio-preserving integer scaling (`new = K * old` for
/// non-pair shares and `new_pair_total = K * old_pair_total`, mirroring the
/// exact integer-factor principle at any exact integer factor for pixel precision).
/// Non-adjacent shares must match exactly after the same integer scaling;
/// the adjacent pair total must be conserved after scaling and the pair must
/// actually move.
fn valid_fixed_share_operation(operation: &ResizeOperation) -> bool {
    if operation.domain_output.0.is_empty()
        || operation.domain_workspace.0.is_empty()
        || operation.focused_leaf.0.is_empty()
        || operation.focused_window.0.is_empty()
        || operation.target_group.0.is_empty()
        || operation.focused_child.0.is_empty()
        || operation.neighbor_child.0.is_empty()
    {
        return false;
    }
    if operation.focused_child == operation.neighbor_child {
        return false;
    }
    if operation.old_shares.len() < 2 || operation.new_shares.len() != operation.old_shares.len() {
        return false;
    }
    if operation.focused_index >= operation.old_shares.len()
        || operation.neighbor_index >= operation.old_shares.len()
        || operation.focused_index == operation.neighbor_index
    {
        return false;
    }
    if (operation.focused_index as i32 - operation.neighbor_index as i32).abs() != 1 {
        return false;
    }
    if operation.neighbor_index as i32 - operation.focused_index as i32
        != resize_step_for(operation.direction)
    {
        return false;
    }
    if operation.old_shares.contains(&0) || operation.new_shares.contains(&0) {
        return false;
    }
    if operation.old_shares == operation.new_shares {
        return false;
    }
    let n = operation.old_shares.len();
    let fi = operation.focused_index;
    let ni = operation.neighbor_index;
    // Totals must not overflow.
    let mut old_total: u64 = 0;
    for share in &operation.old_shares {
        match old_total.checked_add(*share) {
            Some(next) => old_total = next,
            None => return false,
        }
    }
    let mut new_total: u64 = 0;
    for share in &operation.new_shares {
        match new_total.checked_add(*share) {
            Some(next) => new_total = next,
            None => return false,
        }
    }
    if old_total == 0 || new_total == 0 {
        return false;
    }
    // Exact integer scaling factor `K >= 1` shared by the whole group:
    // `new[i] = K * old[i]` off-pair and `new_pair = K * old_pair`, with the
    // pair actually moving. `K = 1` is the unscaled case; `K = 16` is the
    // pixel precision.
    let old_pair = match operation.old_shares[fi].checked_add(operation.old_shares[ni]) {
        Some(pair) if pair != 0 => pair,
        _ => return false,
    };
    let new_pair = match operation.new_shares[fi].checked_add(operation.new_shares[ni]) {
        Some(pair) if pair != 0 => pair,
        _ => return false,
    };
    let has_non_pair = (0..n).any(|i| i != fi && i != ni);
    if has_non_pair {
        let first = (0..n)
            .find(|i| *i != fi && *i != ni)
            .expect("non-pair present");
        let old_base = operation.old_shares[first];
        let new_base = operation.new_shares[first];
        if old_base == 0 || !new_base.is_multiple_of(old_base) {
            return false;
        }
        let scale = new_base / old_base;
        if scale == 0 {
            return false;
        }
        for i in 0..n {
            if i == fi || i == ni {
                continue;
            }
            match operation.old_shares[i].checked_mul(scale) {
                Some(expected) if expected == operation.new_shares[i] => {}
                _ => return false,
            }
        }
        match old_pair.checked_mul(scale) {
            Some(expected) if expected == new_pair => {}
            _ => return false,
        }
        match old_total.checked_mul(scale) {
            Some(expected) if expected == new_total => {}
            _ => return false,
        }
        let scaled_fi = match operation.old_shares[fi].checked_mul(scale) {
            Some(v) => v,
            None => return false,
        };
        let scaled_ni = match operation.old_shares[ni].checked_mul(scale) {
            Some(v) => v,
            None => return false,
        };
        if operation.new_shares[fi] == scaled_fi || operation.new_shares[ni] == scaled_ni {
            return false;
        }
        return true;
    }
    // Two-child group: no non-pair witness, so the scale comes from the pair
    // total alone and must divide exactly.
    if new_pair % old_pair != 0 {
        return false;
    }
    let scale = new_pair / old_pair;
    if scale == 0 {
        return false;
    }
    match old_total.checked_mul(scale) {
        Some(expected) if expected == new_total => {}
        _ => return false,
    }
    let scaled_fi = match operation.old_shares[fi].checked_mul(scale) {
        Some(v) => v,
        None => return false,
    };
    let scaled_ni = match operation.old_shares[ni].checked_mul(scale) {
        Some(v) => v,
        None => return false,
    };
    if operation.new_shares[fi] == scaled_fi || operation.new_shares[ni] == scaled_ni {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{AckOutcome, Observation};
    use crate::directional::{
        Capabilities, Capability, Direction, MoveIntent, MoveOperation, NodeId, OutputId,
        Precondition, Rule, WindowId,
    };
    use crate::ids::{CorrelationId, GenerationId, OwnerId};

    fn owner() -> OwnerId {
        OwnerId::parse("owner-1").expect("valid owner")
    }

    fn generation() -> GenerationId {
        GenerationId::parse("gen-1").expect("valid generation")
    }

    fn correlation(value: &str) -> CorrelationId {
        CorrelationId::parse(value).unwrap_or_else(|| panic!("valid correlation {value}"))
    }

    fn intent() -> MoveIntent {
        MoveIntent {
            source_output: OutputId("out-1".to_owned()),
            focused_leaf: NodeId("leaf-1".to_owned()),
            focused_window: WindowId("win-1".to_owned()),
            direction: Direction::Right,
            same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
        }
    }

    fn plan() -> MovePlan {
        let operation = MoveOperation::SwapNeighbor {
            rule: Rule::R2a,
            container: NodeId("root".to_owned()),
            neighbor: NodeId("leaf-2".to_owned()),
        };
        let preconditions = operation.preconditions();
        MovePlan {
            intent: intent(),
            rule: Rule::R2a,
            operation,
            required_capability: Capability::SwapNeighbor,
            preconditions,
        }
    }

    fn observation(revision: u64) -> Observation {
        Observation::new(owner(), generation(), revision, 11)
    }

    fn reconciler() -> Reconciler {
        Reconciler::new(owner(), generation(), 0, 11).expect("valid seed")
    }

    fn ack_for(correlation: &str, base: u64, outcome: AckOutcome) -> AdapterAck {
        AdapterAck::new(
            self::correlation(correlation),
            owner(),
            generation(),
            base,
            outcome,
        )
    }

    fn post_for(correlation: &str, revision: u64, verified: bool) -> PostObservation {
        PostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            self::correlation(correlation),
            verified,
            plan().preconditions.clone(),
            plan().operation.clone(),
        )
    }

    fn post_with_preconditions(
        correlation: &str,
        revision: u64,
        verified: bool,
        verified_preconditions: Vec<Precondition>,
    ) -> PostObservation {
        PostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            self::correlation(correlation),
            verified,
            verified_preconditions,
            plan().operation.clone(),
        )
    }

    fn post_with_operation(
        correlation: &str,
        revision: u64,
        verified: bool,
        verified_operation: MoveOperation,
    ) -> PostObservation {
        PostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            self::correlation(correlation),
            verified,
            plan().preconditions.clone(),
            verified_operation,
        )
    }

    #[test]
    fn happy_path_commits_exactly_one_revision() {
        let mut r = reconciler();
        let source = plan();
        let dispatch = r
            .propose(
                &source,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full(),
            )
            .expect("propose");
        assert_eq!(dispatch.base_revision, 0);
        assert_eq!(dispatch.correlation_id.as_str(), "corr-1");
        assert_eq!(dispatch.owner.as_str(), "owner-1");
        assert_eq!(dispatch.generation.as_str(), "gen-1");
        assert_eq!(dispatch.preconditions, source.preconditions);
        assert_eq!(dispatch.intent, source.intent);
        assert_eq!(dispatch.operation, source.operation);
        assert_eq!(r.status().state, StateKind::PendingUnacked);
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted)),
            Ok(AckApplied::Accepted)
        );
        assert_eq!(r.status().state, StateKind::PendingAcked);
        let commit = r.verify(&post_for("corr-1", 0, true)).expect("verify");
        assert_eq!(commit.revision, 1);
        assert_eq!(r.verified_revision(), 1);
        assert_eq!(r.status().state, StateKind::Verified);
        // Duplicate verify after commit cannot advance again.
        assert_eq!(
            r.verify(&post_for("corr-1", 0, true)),
            Err(VerifyError::NoPending)
        );
        assert_eq!(r.verified_revision(), 1);
    }

    #[test]
    fn at_most_one_pending_plan() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("first");
        assert_eq!(
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-2"),
                &Capabilities::full()
            ),
            Err(ProposeError::PendingExists)
        );
        // Still pending, not diverged; acknowledgement still works.
        assert_eq!(r.status().state, StateKind::PendingUnacked);
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted)),
            Ok(AckApplied::Accepted)
        );
    }

    #[test]
    fn stale_observation_diverges_and_fail_closes() {
        let mut r = reconciler();
        assert_eq!(
            r.propose(
                &plan(),
                &observation(7),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        // No further dispatch or mutation.
        assert_eq!(
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-2"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
        );
        assert_eq!(
            r.acknowledge(&ack_for("corr-2", 0, AckOutcome::Accepted)),
            Err(AckError::Diverged(DivergenceKind::StaleRevision))
        );
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn owner_and_generation_mismatch_diverge() {
        let mut r = reconciler();
        let mut bad = observation(0);
        bad.owner = OwnerId::parse("owner-2").expect("valid");
        assert_eq!(
            r.propose(&plan(), &bad, &correlation("corr-1"), &Capabilities::full()),
            Err(ProposeError::Diverged(DivergenceKind::OwnerMismatch))
        );
        let mut r = reconciler();
        let mut bad = observation(0);
        bad.generation = GenerationId::parse("gen-2").expect("valid");
        assert_eq!(
            r.propose(&plan(), &bad, &correlation("corr-1"), &Capabilities::full()),
            Err(ProposeError::Diverged(DivergenceKind::GenerationMismatch))
        );
    }

    #[test]
    fn capability_refusal_diverges() {
        let mut r = reconciler();
        assert_eq!(
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::none()
            ),
            Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
    }

    #[test]
    fn plan_without_verify_postcondition_diverges() {
        let mut r = reconciler();
        let mut bad = plan();
        bad.preconditions = vec![Precondition::FocusedLeafOccupiedByFocusedWindow];
        assert_eq!(
            r.propose(
                &bad,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
    }

    #[test]
    fn ack_without_pending_is_discarded_without_divergence() {
        let mut r = reconciler();
        assert_eq!(
            r.acknowledge(&ack_for("corr-9", 0, AckOutcome::Accepted)),
            Err(AckError::NoPending)
        );
        assert_eq!(r.status().state, StateKind::Verified);
        // Still usable.
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("usable after discard");
    }

    #[test]
    fn duplicate_ack_is_discarded_without_advancement() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted)),
            Ok(AckApplied::Accepted)
        );
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted)),
            Ok(AckApplied::DuplicateDiscarded)
        );
        assert_eq!(r.status().state, StateKind::PendingAcked);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn out_of_order_ack_diverges() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.acknowledge(&ack_for("corr-2", 0, AckOutcome::Accepted)),
            Err(AckError::Diverged(DivergenceKind::CorrelationMismatch))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
    }

    #[test]
    fn refused_partial_lost_acks_diverge_without_echo() {
        for (outcome, kind) in [
            (
                AckOutcome::RefusedCapability,
                DivergenceKind::CapabilityRefused,
            ),
            (
                AckOutcome::PartialApplication,
                DivergenceKind::PartialApplication,
            ),
            (AckOutcome::AdapterLost, DivergenceKind::AdapterLost),
        ] {
            let mut r = reconciler();
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full(),
            )
            .expect("propose");
            assert_eq!(
                r.acknowledge(&ack_for("corr-1", 0, outcome)),
                Err(AckError::Diverged(kind))
            );
            assert!(!format!("{:?}", r.status()).contains("SECRET"));
        }
    }

    #[test]
    fn verify_before_ack_is_rejected_without_divergence() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.verify(&post_for("corr-1", 0, true)),
            Err(VerifyError::NotAcknowledged)
        );
        assert_eq!(r.status().state, StateKind::PendingUnacked);
    }

    #[test]
    fn unverified_or_stale_post_diverges() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        assert_eq!(
            r.verify(&post_for("corr-1", 0, false)),
            Err(VerifyError::Diverged(
                DivergenceKind::PostconditionUnverified
            ))
        );
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        assert_eq!(
            r.verify(&post_for("corr-1", 9, true)),
            Err(VerifyError::Diverged(DivergenceKind::StaleRevision))
        );
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        assert_eq!(
            r.verify(&post_for("corr-2", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::CorrelationMismatch))
        );
    }

    #[test]
    fn adapter_loss_is_terminal() {
        let mut r = reconciler();
        assert_eq!(r.note_adapter_loss(), DivergenceKind::AdapterLost);
        assert_eq!(r.status().state, StateKind::Divergent);
        assert_eq!(
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::AdapterLost))
        );
    }

    #[test]
    fn invalid_ids_never_echo_and_diverge_typed() {
        // Malformed session ids cannot construct typed ids at the boundary.
        assert!(CorrelationId::parse("bad corr id SECRET").is_none());
        assert!(OwnerId::parse("bad owner SECRET").is_none());
        assert!(GenerationId::parse("BAD-GEN SECRET").is_none());
        // A valid pending then a mismatched correlation diverges typed, redacted.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        let err = r
            .acknowledge(&ack_for("corr-2", 0, AckOutcome::Accepted))
            .expect_err("mismatched correlation must diverge");
        assert_eq!(err, AckError::Diverged(DivergenceKind::CorrelationMismatch));
        assert_eq!(err.kind(), "correlation-mismatch");
        assert!(!err.message().contains("SECRET"));
        assert!(!format!("{err:?}").contains("SECRET"));
        // Typed ids themselves redact debug output.
        let typed = correlation("corr-1");
        assert!(!format!("{typed:?}").contains("corr-1"));
    }

    #[test]
    fn mismatched_verified_preconditions_diverge_without_commit() {
        let mut r = reconciler();
        let source = plan();
        let dispatch = r
            .propose(
                &source,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full(),
            )
            .expect("propose");
        assert_eq!(dispatch.preconditions, source.preconditions);
        assert_eq!(dispatch.intent, source.intent);
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        let mut mismatched = source.preconditions.clone();
        mismatched.pop();
        assert_eq!(
            r.verify(&post_with_preconditions("corr-1", 0, true, mismatched)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert_eq!(
            r.status().divergence,
            Some(DivergenceKind::PostconditionMismatch)
        );
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn overlong_verified_preconditions_diverge_without_commit() {
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        let overlong = vec![Precondition::AdapterMustVerifyPostconditions; MAX_PRECONDITIONS + 1];
        assert_eq!(
            r.verify(&post_with_preconditions("corr-1", 0, true, overlong)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn dispatch_carries_complete_semantic_plan_payload() {
        let mut r = reconciler();
        let source = plan();
        let dispatch = r
            .propose(
                &source,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full(),
            )
            .expect("propose");
        assert_eq!(dispatch.intent, source.intent);
        assert_eq!(dispatch.operation, source.operation);
        assert_eq!(dispatch.rule, source.rule);
        assert_eq!(dispatch.required_capability, source.required_capability);
        assert_eq!(dispatch.preconditions, source.preconditions);
        assert_eq!(dispatch.correlation_id.as_str(), "corr-1");
        assert_eq!(dispatch.owner.as_str(), "owner-1");
        assert_eq!(dispatch.generation.as_str(), "gen-1");
        assert_eq!(dispatch.base_revision, 0);
    }

    #[test]
    fn errors_are_redacted_and_bounded() {
        assert_eq!(ProposeError::PendingExists.kind(), "pending-exists");
        assert!(!ProposeError::PendingExists.message().contains("corr"));
        assert_eq!(AckError::NoPending.kind(), "no-pending");
        assert_eq!(VerifyError::NotAcknowledged.kind(), "not-acknowledged");
    }

    #[test]
    fn inconsistent_rule_diverges_without_dispatch() {
        let mut r = reconciler();
        let mut bad = plan();
        bad.rule = Rule::R1;
        assert_eq!(
            r.propose(
                &bad,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert!(!r.status().pending);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn inconsistent_capability_diverges_without_dispatch() {
        let mut r = reconciler();
        let mut bad = plan();
        bad.required_capability = Capability::WrapPerpendicular;
        assert_eq!(
            r.propose(
                &bad,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert!(!r.status().pending);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn inconsistent_preconditions_diverge_without_dispatch() {
        let mut r = reconciler();
        let mut bad = plan();
        let mut preconditions = bad.operation.preconditions();
        preconditions.pop();
        bad.preconditions = preconditions;
        assert_eq!(
            r.propose(
                &bad,
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert!(!r.status().pending);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn mismatched_verified_operation_diverges_without_commit() {
        let mut r = reconciler();
        let source = plan();
        r.propose(
            &source,
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        let other = MoveOperation::WrapPerpendicular {
            rule: Rule::R1,
            container: NodeId("root".to_owned()),
            axis: crate::directional::Axis::Horizontal,
        };
        assert_eq!(
            r.verify(&post_with_operation("corr-1", 0, true, other)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        assert_eq!(r.status().state, StateKind::Divergent);
        assert_eq!(r.verified_revision(), 0);
    }

    #[test]
    fn malformed_ack_shapes_classify_typed() {
        // Malformed session strings cannot construct typed ids at the boundary.
        assert!(OwnerId::parse("bad owner").is_none());
        assert!(GenerationId::parse("GEN-1").is_none());
        assert!(CorrelationId::parse("bad corr").is_none());
        // Valid-but-mismatched owner while pending diverges as owner mismatch.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        let mut bad = ack_for("corr-1", 0, AckOutcome::Accepted);
        bad.owner = OwnerId::parse("owner-2").expect("valid");
        assert_eq!(
            r.acknowledge(&bad),
            Err(AckError::Diverged(DivergenceKind::OwnerMismatch))
        );

        // Valid-but-mismatched generation diverges as generation mismatch.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        let mut bad = ack_for("corr-1", 0, AckOutcome::Accepted);
        bad.generation = GenerationId::parse("gen-2").expect("valid");
        assert_eq!(
            r.acknowledge(&bad),
            Err(AckError::Diverged(DivergenceKind::GenerationMismatch))
        );

        // Out-of-bounds revision shape diverges as stale revision.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        let bad = ack_for(
            "corr-1",
            crate::contract::MAX_REVISION + 1,
            AckOutcome::Accepted,
        );
        assert_eq!(
            r.acknowledge(&bad),
            Err(AckError::Diverged(DivergenceKind::StaleRevision))
        );

        // Mismatched correlation still diverges as correlation mismatch.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("propose");
        let bad = ack_for("corr-2", 0, AckOutcome::Accepted);
        assert_eq!(
            r.acknowledge(&bad),
            Err(AckError::Diverged(DivergenceKind::CorrelationMismatch))
        );
    }

    #[test]
    fn malformed_verify_shapes_classify_typed() {
        fn acked(r: &mut Reconciler) {
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-1"),
                &Capabilities::full(),
            )
            .expect("propose");
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
                .expect("ack");
        }
        // Malformed correlation strings cannot construct typed ids.
        assert!(CorrelationId::parse("bad corr").is_none());
        assert!(OwnerId::parse("bad owner").is_none());
        assert!(GenerationId::parse("GEN-1").is_none());
        // Mismatched correlation diverges as correlation mismatch.
        let mut r = reconciler();
        acked(&mut r);
        let bad = post_for("corr-2", 0, true);
        assert_eq!(
            r.verify(&bad),
            Err(VerifyError::Diverged(DivergenceKind::CorrelationMismatch))
        );
        // Mismatched owner diverges as owner mismatch.
        let mut r = reconciler();
        acked(&mut r);
        let mut bad = post_for("corr-1", 0, true);
        bad.observation.owner = OwnerId::parse("owner-2").expect("valid");
        assert_eq!(
            r.verify(&bad),
            Err(VerifyError::Diverged(DivergenceKind::OwnerMismatch))
        );
        // Mismatched generation diverges as generation mismatch.
        let mut r = reconciler();
        acked(&mut r);
        let mut bad = post_for("corr-1", 0, true);
        bad.observation.generation = GenerationId::parse("gen-2").expect("valid");
        assert_eq!(
            r.verify(&bad),
            Err(VerifyError::Diverged(DivergenceKind::GenerationMismatch))
        );
        // Out-of-bounds revision shape.
        let mut r = reconciler();
        acked(&mut r);
        let bad = post_for("corr-1", crate::contract::MAX_REVISION + 1, true);
        assert_eq!(
            r.verify(&bad),
            Err(VerifyError::Diverged(DivergenceKind::StaleRevision))
        );
    }

    // ---- resize plan validation (semantic, topology-free) ----

    fn resize_operation() -> ResizeOperation {
        ResizeOperation {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            focused_leaf: NodeId("leaf-win-2".to_owned()),
            focused_window: WindowId("win-2".to_owned()),
            direction: Direction::Left,
            mode: crate::contract::ResizeMode::Outwards,
            target_group: NodeId("root".to_owned()),
            focused_child: NodeId("leaf-win-2".to_owned()),
            neighbor_child: NodeId("leaf-win-1".to_owned()),
            focused_index: 1,
            neighbor_index: 0,
            old_shares: vec![1, 1],
            new_shares: vec![14, 18],
        }
    }

    fn resize_intent() -> crate::contract::ResizeIntent {
        crate::contract::ResizeIntent {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            focused_leaf: NodeId("leaf-win-2".to_owned()),
            focused_window: WindowId("win-2".to_owned()),
            direction: Direction::Left,
            mode: crate::contract::ResizeMode::Outwards,
        }
    }

    fn resize_plan() -> ResizePlan {
        ResizePlan::for_operation(resize_intent(), resize_operation())
    }

    fn resize_post(correlation: &str, revision: u64, verified: bool) -> ResizePostObservation {
        let plan = resize_plan();
        ResizePostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            self::correlation(correlation),
            verified,
            plan.preconditions.clone(),
            plan.operation.clone(),
        )
    }

    fn acked_resize(r: &mut Reconciler) {
        r.propose_resize(
            &resize_plan(),
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
    }

    #[test]
    fn resize_intent_binds_operation_exactly() {
        for mutate in [
            "domain_output",
            "domain_workspace",
            "focused_leaf",
            "focused_window",
            "direction",
            "mode",
        ] {
            let mut r = reconciler();
            let mut intent = resize_intent();
            let mut operation = resize_operation();
            match mutate {
                "domain_output" => {
                    intent.domain_output = OutputId("out-9".to_owned());
                }
                "domain_workspace" => {
                    intent.domain_workspace = crate::directional::WorkspaceId("ws-9".to_owned());
                }
                "focused_leaf" => {
                    intent.focused_leaf = NodeId("leaf-9".to_owned());
                }
                "focused_window" => {
                    intent.focused_window = WindowId("win-9".to_owned());
                }
                "direction" => {
                    intent.direction = Direction::Right;
                }
                "mode" => {
                    intent.mode = crate::contract::ResizeMode::Inwards;
                }
                _ => unreachable!(),
            }
            let plan = ResizePlan::for_operation(intent, operation.clone());
            // `for_operation` derives capability/preconditions honestly, so a
            // bare intent drift is the only inconsistency under test.
            assert_eq!(
                r.propose_resize(
                    &plan,
                    &observation(0),
                    &correlation("corr-1"),
                    &ResizeCapabilities::full()
                ),
                Err(ProposeError::Diverged(
                    DivergenceKind::PostconditionMismatch
                )),
                "{mutate}"
            );
            // Drift in the operation direction alone also breaks intent
            // binding (and orientation).
            let mut r = reconciler();
            operation.direction = Direction::Right;
            let plan = ResizePlan::for_operation(resize_intent(), operation);
            assert_eq!(
                r.propose_resize(
                    &plan,
                    &observation(0),
                    &correlation("corr-1"),
                    &ResizeCapabilities::full()
                ),
                Err(ProposeError::Diverged(
                    DivergenceKind::PostconditionMismatch
                )),
                "{mutate} operation"
            );
        }
    }

    #[test]
    fn resize_capability_and_preconditions_bind() {
        // Tampered capability diverges.
        let mut r = reconciler();
        let mut plan = resize_plan();
        plan.required_capability = crate::contract::ResizeCapability::KeyboardResize;
        plan.preconditions.pop();
        assert_eq!(
            r.propose_resize(
                &plan,
                &observation(0),
                &correlation("corr-1"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        // Missing terminal precondition diverges even with capability held.
        let mut r = reconciler();
        let operation = resize_operation();
        let mut preconditions = operation.preconditions();
        preconditions.pop();
        let plan = ResizePlan {
            intent: resize_intent(),
            operation,
            required_capability: crate::contract::ResizeCapability::KeyboardResize,
            preconditions,
        };
        assert_eq!(
            r.propose_resize(
                &plan,
                &observation(0),
                &correlation("corr-1"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        // Missing adapter capability diverges as capability-refused.
        let mut r = reconciler();
        assert_eq!(
            r.propose_resize(
                &resize_plan(),
                &observation(0),
                &correlation("corr-1"),
                &ResizeCapabilities::none()
            ),
            Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
        );
    }

    #[test]
    fn resize_operation_semantics_validated_without_topology() {
        // Each mutation keeps ids well-formed but breaks adjacency
        // orientation, index validity, or the exact share transfer.
        let cases: Vec<ResizeOperation> = vec![
            // Non-adjacent indices.
            ResizeOperation {
                focused_index: 0,
                neighbor_index: 0,
                ..resize_operation()
            },
            // Wrong orientation for Direction::Left (needs neighbor - focused == -1).
            ResizeOperation {
                focused_index: 0,
                neighbor_index: 1,
                ..resize_operation()
            },
            // No change.
            ResizeOperation {
                new_shares: vec![1, 1],
                ..resize_operation()
            },
            // Pair total breaks exact integer scaling (31 vs old pair 2).
            ResizeOperation {
                new_shares: vec![14, 17],
                ..resize_operation()
            },
            // Zero share.
            ResizeOperation {
                new_shares: vec![0, 32],
                ..resize_operation()
            },
            // Identical children.
            ResizeOperation {
                neighbor_child: NodeId("leaf-win-2".to_owned()),
                ..resize_operation()
            },
        ];
        for (index, operation) in cases.into_iter().enumerate() {
            let mut r = reconciler();
            let plan = ResizePlan::for_operation(resize_intent(), operation);
            assert_eq!(
                r.propose_resize(
                    &plan,
                    &observation(0),
                    &correlation("corr-1"),
                    &ResizeCapabilities::full()
                ),
                Err(ProposeError::Diverged(
                    DivergenceKind::PostconditionMismatch
                )),
                "case {index}"
            );
        }
    }

    #[test]
    fn keyboard_pixel_shares_validate_on_dedicated_path() {
        // Dedicated keyboard path accepts COSMIC pixel redistribution
        // ([1,1] -> [15,17]: pair 2 -> 32 at K=16 with movement) that the
        // legacy 1/16 step would reject as inexact, while still rejecting a
        // broken scaling ([14,17] pair 31). Pointer accepts the same
        // pixel shape via the shared neutral relation without mode semantics.
        let mut r = reconciler();
        let ok = ResizeOperation {
            new_shares: vec![15, 17],
            ..resize_operation()
        };
        let plan = ResizePlan::for_operation(resize_intent(), ok);
        r.propose_resize(
            &plan,
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("keyboard pixel shares accept");
        let mut r2 = reconciler();
        let bad = ResizeOperation {
            new_shares: vec![14, 17],
            ..resize_operation()
        };
        let bad_plan = ResizePlan::for_operation(resize_intent(), bad);
        assert_eq!(
            r2.propose_resize(
                &bad_plan,
                &observation(0),
                &correlation("corr-1"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
    }

    #[test]
    fn sixty_five_share_resize_validates_without_count_cap() {
        // 65-child group (beyond the old 64-share bound) with exact K=16
        // pixel scaling on the adjacent pair: valid through the real
        // reconciler path, proving the old length refusal now succeeds.
        // All other gates (>=2, length equality, adjacent indices,
        // orientation, positivity, exact scaling, checked totals) apply
        // unchanged.
        let old_shares = vec![1_u64; 65];
        let mut new_shares = vec![16_u64; 65];
        new_shares[0] = 14;
        new_shares[1] = 18;
        let operation = ResizeOperation {
            old_shares,
            new_shares,
            ..resize_operation()
        };
        let plan = ResizePlan::for_operation(resize_intent(), operation);
        let mut r = reconciler();
        r.propose_resize(
            &plan,
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("65-share pixel operation accepts");
    }

    #[test]
    fn keyboard_mode_direction_validated_on_dedicated_path() {
        // Otherwise-valid operation with flipped mode diverges; the valid
        // keyboard operation stays accepted. Pointer validation is unchanged.
        let mut r = reconciler();
        r.propose_resize(
            &resize_plan(),
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("valid keyboard operation accepted");
        let mut flipped_operation = resize_operation();
        flipped_operation.mode = crate::contract::ResizeMode::Inwards;
        let mut flipped_intent = resize_intent();
        flipped_intent.mode = crate::contract::ResizeMode::Inwards;
        let flipped_plan = ResizePlan::for_operation(flipped_intent, flipped_operation);
        let mut r2 = reconciler();
        assert_eq!(
            r2.propose_resize(
                &flipped_plan,
                &observation(0),
                &correlation("corr-1"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
    }

    #[test]
    fn resize_shares_single_pending_slot_across_kinds() {
        let mut r = reconciler();
        r.propose_resize(
            &resize_plan(),
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("resize first");
        assert_eq!(
            r.propose(
                &plan(),
                &observation(0),
                &correlation("corr-2"),
                &Capabilities::full()
            ),
            Err(ProposeError::PendingExists)
        );
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("move first");
        assert_eq!(
            r.propose_resize(
                &resize_plan(),
                &observation(0),
                &correlation("corr-2"),
                &ResizeCapabilities::full()
            ),
            Err(ProposeError::PendingExists)
        );
    }

    #[test]
    fn resize_ack_verify_failure_paths() {
        // Verify without acknowledgement.
        let mut r = reconciler();
        r.propose_resize(
            &resize_plan(),
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.verify_resize(&resize_post("corr-1", 0, true)),
            Err(VerifyError::NotAcknowledged)
        );
        // Non-accepted acknowledgement diverges.
        let mut r = reconciler();
        r.propose_resize(
            &resize_plan(),
            &observation(0),
            &correlation("corr-1"),
            &ResizeCapabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::RefusedCapability)),
            Err(AckError::Diverged(DivergenceKind::CapabilityRefused))
        );
        // Wrong verified preconditions diverge.
        let mut r = reconciler();
        acked_resize(&mut r);
        let mut bad = resize_post("corr-1", 0, true);
        bad.verified_preconditions.pop();
        assert_eq!(
            r.verify_resize(&bad),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Wrong verified operation diverges.
        let mut r = reconciler();
        acked_resize(&mut r);
        let mut bad = resize_post("corr-1", 0, true);
        bad.verified_operation.new_shares = vec![15, 17];
        assert_eq!(
            r.verify_resize(&bad),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Unverified diverges without committing.
        let mut r = reconciler();
        acked_resize(&mut r);
        assert_eq!(
            r.verify_resize(&resize_post("corr-1", 0, false)),
            Err(VerifyError::Diverged(
                DivergenceKind::PostconditionUnverified
            ))
        );
        // Cross-kind verify while resize pending diverges.
        let mut r = reconciler();
        acked_resize(&mut r);
        assert_eq!(
            r.verify(&post_for("corr-1", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Resize verify while movement pending diverges.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("move");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        assert_eq!(
            r.verify_resize(&resize_post("corr-1", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Happy path still commits exactly one revision.
        let mut r = reconciler();
        acked_resize(&mut r);
        let commit = r
            .verify_resize(&resize_post("corr-1", 0, true))
            .expect("verify");
        assert_eq!(commit.revision, 1);
        assert_eq!(r.verified_revision(), 1);
    }

    // ---- drag plan validation ----

    fn drag_operation() -> DragOperation {
        DragOperation {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            source_leaf: NodeId("leaf-win-1".to_owned()),
            source_window: WindowId("win-1".to_owned()),
            target_leaf: NodeId("leaf-win-2".to_owned()),
            target_window: WindowId("win-2".to_owned()),
            side: crate::contract::DragSide::Right,
            axis: crate::directional::Axis::Horizontal,
            before: false,
            target_group: NodeId("root".to_owned()),
            insertion_index: 2,
            wrap: false,
            new_group: None,
            cross_source: None,
        }
    }

    fn drag_intent() -> crate::contract::DragIntent {
        crate::contract::DragIntent {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            source_leaf: NodeId("leaf-win-1".to_owned()),
            source_window: WindowId("win-1".to_owned()),
            target_leaf: NodeId("leaf-win-2".to_owned()),
            target_window: WindowId("win-2".to_owned()),
            side: crate::contract::DragSide::Right,
            cross_source: None,
        }
    }

    fn drag_plan() -> DragPlan {
        DragPlan::for_operation(drag_intent(), drag_operation())
    }

    fn drag_post(correlation: &str, revision: u64, verified: bool) -> DragPostObservation {
        let plan = drag_plan();
        DragPostObservation::new(
            Observation::new(owner(), generation(), revision, 22),
            self::correlation(correlation),
            verified,
            plan.preconditions.clone(),
            plan.operation.clone(),
        )
    }

    fn acked_drag(r: &mut Reconciler) {
        r.propose_drag(
            &drag_plan(),
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("propose");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
    }

    #[test]
    fn drag_happy_path_commits_exactly_one_revision() {
        let mut r = reconciler();
        let source = drag_plan();
        let dispatch = r
            .propose_drag(
                &source,
                &observation(0),
                &correlation("corr-1"),
                &DragCapabilities::full(),
            )
            .expect("propose");
        assert_eq!(dispatch.base_revision, 0);
        assert_eq!(dispatch.correlation_id.as_str(), "corr-1");
        assert_eq!(dispatch.intent, source.intent);
        assert_eq!(dispatch.operation, source.operation);
        assert_eq!(
            dispatch.required_capability,
            crate::contract::DragCapability::PlaceTiled
        );
        assert!(
            dispatch
                .preconditions
                .contains(&crate::contract::DragPrecondition::AdapterMustVerifyPostconditions)
        );
        assert_eq!(r.status().state, StateKind::PendingUnacked);
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted)),
            Ok(AckApplied::Accepted)
        );
        let commit = r
            .verify_drag(&drag_post("corr-1", 0, true))
            .expect("verify");
        assert_eq!(commit.revision, 1);
        assert_eq!(r.verified_revision(), 1);
        assert_eq!(r.status().state, StateKind::Verified);
        assert_eq!(
            r.verify_drag(&drag_post("corr-1", 0, true)),
            Err(VerifyError::NoPending)
        );
    }

    #[test]
    fn drag_intent_binds_operation_exactly() {
        for mutate in [
            "domain_output",
            "source_leaf",
            "target_leaf",
            "target_window",
            "side",
        ] {
            let mut r = reconciler();
            let mut intent = drag_intent();
            match mutate {
                "domain_output" => {
                    intent.domain_output = OutputId("out-9".to_owned());
                }
                "source_leaf" => {
                    intent.source_leaf = NodeId("leaf-9".to_owned());
                }
                "target_leaf" => {
                    intent.target_leaf = NodeId("leaf-9".to_owned());
                }
                "target_window" => {
                    intent.target_window = WindowId("win-9".to_owned());
                }
                "side" => {
                    intent.side = crate::contract::DragSide::Left;
                }
                _ => unreachable!(),
            }
            let plan = DragPlan::for_operation(intent, drag_operation());
            assert_eq!(
                r.propose_drag(
                    &plan,
                    &observation(0),
                    &correlation("corr-1"),
                    &DragCapabilities::full()
                ),
                Err(ProposeError::Diverged(
                    DivergenceKind::PostconditionMismatch
                )),
                "{mutate}"
            );
        }
    }

    #[test]
    fn drag_operation_semantics_validated_without_topology() {
        let cases: Vec<DragOperation> = vec![
            // Self drop carries no structural meaning.
            DragOperation {
                target_leaf: NodeId("leaf-win-1".to_owned()),
                target_window: WindowId("win-1".to_owned()),
                ..drag_operation()
            },
            // Axis must derive from the side.
            DragOperation {
                axis: crate::directional::Axis::Vertical,
                ..drag_operation()
            },
            // Order must derive from the side.
            DragOperation {
                before: true,
                ..drag_operation()
            },
            // Insert form carries no fresh group.
            DragOperation {
                new_group: Some(NodeId("grp-1".to_owned())),
                ..drag_operation()
            },
            // Wrap form requires a fresh distinct group.
            DragOperation {
                wrap: true,
                new_group: None,
                ..drag_operation()
            },
            DragOperation {
                wrap: true,
                new_group: Some(NodeId("leaf-win-1".to_owned())),
                ..drag_operation()
            },
            // Non-root window target parent is distinct from the source leaf;
            // group same-axis N-ary inserts name the target group itself, so
            // the source leaf remains the forbidden parent here.
            DragOperation {
                target_group: NodeId("leaf-win-1".to_owned()),
                ..drag_operation()
            },
        ];
        for (index, operation) in cases.into_iter().enumerate() {
            let mut r = reconciler();
            // Rebuild the intent so only the operation shape is under test.
            let intent = crate::contract::DragIntent {
                target_leaf: operation.target_leaf.clone(),
                target_window: operation.target_window.clone(),
                ..drag_intent()
            };
            let plan = DragPlan::for_operation(intent, operation);
            assert_eq!(
                r.propose_drag(
                    &plan,
                    &observation(0),
                    &correlation("corr-1"),
                    &DragCapabilities::full()
                ),
                Err(ProposeError::Diverged(
                    DivergenceKind::PostconditionMismatch
                )),
                "case {index}"
            );
        }
    }

    #[test]
    fn drag_capability_and_preconditions_bind() {
        // Missing adapter capability diverges as capability-refused.
        let mut r = reconciler();
        assert_eq!(
            r.propose_drag(
                &drag_plan(),
                &observation(0),
                &correlation("corr-1"),
                &DragCapabilities::none()
            ),
            Err(ProposeError::Diverged(DivergenceKind::CapabilityRefused))
        );
        // Missing terminal precondition diverges as mismatch.
        let mut r = reconciler();
        let operation = drag_operation();
        let mut preconditions = operation.preconditions();
        preconditions.pop();
        let plan = DragPlan {
            intent: drag_intent(),
            operation,
            required_capability: crate::contract::DragCapability::PlaceTiled,
            preconditions,
        };
        assert_eq!(
            r.propose_drag(
                &plan,
                &observation(0),
                &correlation("corr-1"),
                &DragCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
        // Stale observation diverges without dispatch.
        let mut r = reconciler();
        assert_eq!(
            r.propose_drag(
                &drag_plan(),
                &observation(7),
                &correlation("corr-1"),
                &DragCapabilities::full()
            ),
            Err(ProposeError::Diverged(DivergenceKind::StaleRevision))
        );
    }

    #[test]
    fn drag_shares_single_pending_slot_across_kinds() {
        let mut r = reconciler();
        r.propose_drag(
            &drag_plan(),
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("drag first");
        for second in ["move", "resize"] {
            if second == "move" {
                assert_eq!(
                    r.propose(
                        &plan(),
                        &observation(0),
                        &correlation("corr-2"),
                        &Capabilities::full()
                    ),
                    Err(ProposeError::PendingExists),
                    "drag blocks move"
                );
            } else {
                assert_eq!(
                    r.propose_resize(
                        &resize_plan(),
                        &observation(0),
                        &correlation("corr-2"),
                        &ResizeCapabilities::full()
                    ),
                    Err(ProposeError::PendingExists),
                    "drag blocks resize"
                );
            }
        }
        // Focus and lifecycle proposals are blocked the same way.
        let focus_plan = FocusPlanContract::for_operation(
            crate::contract::FocusIntent {
                domain_output: OutputId("out-1".to_owned()),
                domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
                focused_leaf: Some(NodeId("a".to_owned())),
                focused_window: WindowId("win-a".to_owned()),
                direction: Direction::Right,
            },
            crate::contract::FocusOperation {
                domain_output: OutputId("out-1".to_owned()),
                domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
                from_leaf: Some(NodeId("a".to_owned())),
                to_leaf: NodeId("b".to_owned()),
                from_window: WindowId("win-a".to_owned()),
                to_window: WindowId("win-b".to_owned()),
                direction: Direction::Right,
                route: vec![NodeId("b".to_owned())],
                cross_source_output: None,
                cross_source_workspace: None,
            },
        );
        assert_eq!(
            r.propose_focus(
                &focus_plan,
                &observation(0),
                &correlation("corr-3"),
                &FocusCapabilities::full()
            ),
            Err(ProposeError::PendingExists),
            "drag blocks focus"
        );
        // And any pending plan blocks a drag proposal.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("move first");
        assert_eq!(
            r.propose_drag(
                &drag_plan(),
                &observation(0),
                &correlation("corr-2"),
                &DragCapabilities::full()
            ),
            Err(ProposeError::PendingExists),
            "move blocks drag"
        );
    }

    #[test]
    fn drag_ack_verify_failure_paths_diverge() {
        // Verify without acknowledgement.
        let mut r = reconciler();
        r.propose_drag(
            &drag_plan(),
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.verify_drag(&drag_post("corr-1", 0, true)),
            Err(VerifyError::NotAcknowledged)
        );
        // Non-accepted acknowledgement diverges.
        let mut r = reconciler();
        r.propose_drag(
            &drag_plan(),
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("propose");
        assert_eq!(
            r.acknowledge(&ack_for("corr-1", 0, AckOutcome::PartialApplication)),
            Err(AckError::Diverged(DivergenceKind::PartialApplication))
        );
        // Wrong verified preconditions diverge.
        let mut r = reconciler();
        acked_drag(&mut r);
        let mut bad = drag_post("corr-1", 0, true);
        bad.verified_preconditions.pop();
        assert_eq!(
            r.verify_drag(&bad),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Wrong verified operation diverges.
        let mut r = reconciler();
        acked_drag(&mut r);
        let mut bad = drag_post("corr-1", 0, true);
        bad.verified_operation.insertion_index += 1;
        assert_eq!(
            r.verify_drag(&bad),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Unverified diverges without committing.
        let mut r = reconciler();
        acked_drag(&mut r);
        assert_eq!(
            r.verify_drag(&drag_post("corr-1", 0, false)),
            Err(VerifyError::Diverged(
                DivergenceKind::PostconditionUnverified
            ))
        );
        assert_eq!(r.verified_revision(), 0);
        // Cross-kind verify while drag pending diverges.
        let mut r = reconciler();
        acked_drag(&mut r);
        assert_eq!(
            r.verify(&post_for("corr-1", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        assert_eq!(
            r.verify_resize(&resize_post("corr-1", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
        // Drag verify while movement pending diverges.
        let mut r = reconciler();
        r.propose(
            &plan(),
            &observation(0),
            &correlation("corr-1"),
            &Capabilities::full(),
        )
        .expect("move");
        r.acknowledge(&ack_for("corr-1", 0, AckOutcome::Accepted))
            .expect("ack");
        assert_eq!(
            r.verify_drag(&drag_post("corr-1", 0, true)),
            Err(VerifyError::Diverged(DivergenceKind::PostconditionMismatch))
        );
    }

    #[test]
    fn drag_root_wrap_requires_insertion_index_zero() {
        // Root wrap names the target leaf as its own group with index 0.
        let mut ok_operation = drag_operation();
        ok_operation.wrap = true;
        ok_operation.target_group = NodeId("leaf-win-2".to_owned());
        ok_operation.insertion_index = 0;
        ok_operation.new_group = Some(NodeId("grp-fresh".to_owned()));
        let ok_intent = crate::contract::DragIntent {
            target_leaf: ok_operation.target_leaf.clone(),
            target_window: ok_operation.target_window.clone(),
            ..drag_intent()
        };
        let mut r = reconciler();
        r.propose_drag(
            &DragPlan::for_operation(ok_intent, ok_operation),
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("root wrap with index 0 proposes");
        // Same shape with a nonzero index is not a valid root wrap.
        let mut bad_operation = drag_operation();
        bad_operation.wrap = true;
        bad_operation.target_group = NodeId("leaf-win-2".to_owned());
        bad_operation.insertion_index = 1;
        bad_operation.new_group = Some(NodeId("grp-fresh".to_owned()));
        let bad_intent = crate::contract::DragIntent {
            target_leaf: bad_operation.target_leaf.clone(),
            target_window: bad_operation.target_window.clone(),
            ..drag_intent()
        };
        let mut r = reconciler();
        assert_eq!(
            r.propose_drag(
                &DragPlan::for_operation(bad_intent, bad_operation),
                &observation(0),
                &correlation("corr-1"),
                &DragCapabilities::full()
            ),
            Err(ProposeError::Diverged(
                DivergenceKind::PostconditionMismatch
            ))
        );
    }

    #[test]
    fn drag_insertion_index_sixty_five_validates_without_count_cap() {
        // Insertion index 65 (beyond the old 64 bound) validates through the
        // real reconciler path on the non-wrap insert shape, proving the old
        // index refusal now succeeds. Wrap-root index-0 and all structural
        // checks apply unchanged.
        let operation = DragOperation {
            insertion_index: 65,
            ..drag_operation()
        };
        let plan = DragPlan::for_operation(drag_intent(), operation);
        let mut r = reconciler();
        r.propose_drag(
            &plan,
            &observation(0),
            &correlation("corr-1"),
            &DragCapabilities::full(),
        )
        .expect("insertion index 65 accepts");
    }

    #[test]
    fn focus_route_sixty_five_validates_without_count_cap() {
        // 65-node focus route (beyond the old 64 bound) validates through
        // the real reconciler path, proving the old route-length refusal
        // now succeeds. Nonempty route and preconditions <= 8 apply
        // unchanged.
        let route: Vec<NodeId> = (0..65).map(|i| NodeId(format!("r-{i}"))).collect();
        let operation = crate::contract::FocusOperation {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            from_leaf: Some(NodeId("a".to_owned())),
            to_leaf: NodeId("b".to_owned()),
            from_window: WindowId("win-a".to_owned()),
            to_window: WindowId("win-b".to_owned()),
            direction: Direction::Right,
            route,
            cross_source_output: None,
            cross_source_workspace: None,
        };
        let intent = crate::contract::FocusIntent {
            domain_output: OutputId("out-1".to_owned()),
            domain_workspace: crate::directional::WorkspaceId("ws-1".to_owned()),
            focused_leaf: Some(NodeId("a".to_owned())),
            focused_window: WindowId("win-a".to_owned()),
            direction: Direction::Right,
        };
        let plan = FocusPlanContract::for_operation(intent, operation);
        let mut r = reconciler();
        r.propose_focus(
            &plan,
            &observation(0),
            &correlation("corr-1"),
            &FocusCapabilities::full(),
        )
        .expect("65-node focus route accepts");
    }
}
