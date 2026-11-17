//! Portable layout domain, geometry, exception, and refusal vocabulary.
//!
//! Pure data types shared across [`crate::world_layout`], [`crate::world_engine`],
//! and downstream adapters. No retained session machine, pending-plan state,
//! or inner acknowledgement protocol lives here.

use std::collections::BTreeMap;

use crate::directional::{Direction, Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;

/// Logical output domain: separate output/workspace scope with explicit
/// portable bounds and gap for the deterministic projector, plus configured
/// logical output adjacency for R4 planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDomain {
    pub id: OutputId,
    pub workspace: WorkspaceId,
    pub bounds: Rect,
    pub gap: i32,
    pub adjacent: BTreeMap<Direction, OutputId>,
}

impl OutputDomain {
    /// Validity without echoing input (shape only; reciprocal/known/same
    /// workspace membership is validated across all domains at construction).
    #[must_use]
    pub fn validate(&self) -> bool {
        !self.id.0.is_empty()
            && !self.workspace.0.is_empty()
            && self.bounds.w > 0
            && self.bounds.h > 0
            && self.bounds.x.checked_add(self.bounds.w).is_some()
            && self.bounds.y.checked_add(self.bounds.h).is_some()
            && self.gap >= 0
            && self.adjacent.len() <= 4
            && self
                .adjacent
                .iter()
                .all(|(_, target)| !target.0.is_empty() && target != &self.id)
    }

    /// Domain key for this logical domain.
    #[must_use]
    pub fn key(&self) -> DomainKey {
        DomainKey {
            output: self.id.clone(),
            workspace: self.workspace.clone(),
        }
    }
}

/// Logical domain key: distinct `(OutputId, WorkspaceId)` pair with exact
/// opaque ids preserved. Logical outputs and workspaces remain separate
/// domains, so one output id may appear with several workspace ids.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainKey {
    pub output: OutputId,
    pub workspace: WorkspaceId,
}

/// Explicit observed exception flags for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExceptionFlags {
    pub floating: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
}

impl ExceptionFlags {
    /// No exception flags set.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
        }
    }

    /// Whether any exception flag is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.floating || self.fullscreen || self.maximized || self.sticky
    }
}

/// Explicit caller-selected behavior for an observed exception window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionBehavior {
    Defer,
}

/// One adapter-observed window with explicit exception flags plus the
/// ephemeral client size hints (AR12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWindow {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub floating: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
    pub hints: crate::size_hints::WindowSizeHints,
}

impl ObservedWindow {
    /// Exception flags for this observation.
    #[must_use]
    pub const fn flags(&self) -> ExceptionFlags {
        ExceptionFlags {
            floating: self.floating,
            fullscreen: self.fullscreen,
            maximized: self.maximized,
            sticky: self.sticky,
        }
    }
}

/// Stored exception record for a deferred window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRecord {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub flags: ExceptionFlags,
    /// Layout-local floating placement. Never participates in tree geometry
    /// or fresh admission.
    pub floating_geometry: Option<Rect>,
}

/// D23 stored old slot for one minimized tiled window (REQ-MNZ-01/02/03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizedSlot {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub leaf: NodeId,
    pub tree: Node,
}

/// Bounded result of observation convergence: correlated removed, admitted,
/// and flag-adopted counts with no identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationConvergence {
    /// Missing known windows removed.
    pub removed: usize,
    /// Brand-new normal windows admitted.
    pub admitted: usize,
    /// Floating adoptions: tiled<->floating transitions and brand-new
    /// floating exceptions.
    pub flags_adopted: usize,
}

/// Non-divergent layout refusal reasons. Fixed redacted messages only.
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

/// Complete desired geometry for one affected tiled window.
/// Domain-scoped: names the exact `(output, workspace)` domain plus leaf.
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

/// Portable sticky group-edge hover carry: the exact drag capture identity
/// (capture domain/leaf/window plus revision), the stored next hover (`None`
/// when the last hover was not a group edge), and the resolution domain the
/// hover was classified in (`prior_domain`, equal to the capture domain for
/// same-domain hovers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragHoverPrior {
    pub domain: DomainKey,
    pub source_leaf: NodeId,
    pub source_window: WindowId,
    pub revision: u64,
    pub prior: Option<crate::policy::PriorGroupEdge>,
    pub prior_domain: DomainKey,
}

/// Pure maximize-toggle decision: `maximized` is the desired
/// flag after the toggle, `clear_fullscreen` the source fullscreen
/// preimage (the adapter clears fullscreen first when true).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaximizeDecision {
    pub maximized: bool,
    pub clear_fullscreen: bool,
}

/// Pure maximize-toggle decision from the source flags: fullscreen
/// always maximizes (clearing fullscreen first), otherwise the
/// maximized flag flips. No state, no topology.
#[must_use]
pub const fn decide_maximize_toggle(fullscreen: bool, maximized: bool) -> MaximizeDecision {
    if fullscreen {
        MaximizeDecision {
            maximized: true,
            clear_fullscreen: true,
        }
    } else {
        MaximizeDecision {
            maximized: !maximized,
            clear_fullscreen: false,
        }
    }
}
