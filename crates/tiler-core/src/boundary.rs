//! Serde-free typed event/plan/reply boundary.
//!
//! Portable carriers over adapter-normalized integer geometry only: no
//! transport, JSON, platform, or process imports. The protocol layer keeps
//! `RequestDto.command`, `Validated.raw`, wire serialization, summary helpers,
//! correlation echoes, and the ordered ingress
//! fences. It converts already-decoded commands and validated observations
//! into [`CoreEvent`] after the applicable dispatch boundaries. Conversion
//! never re-parses:
//! direction/mode strings cross as opaque carriers so handler-local
//! precedence (`not-tiled`, `*-op-invalid`) stays exactly where
//! it is.
//! This module owns no retained state.
//! All synchronous command orchestration runs through [`crate::engine::Engine::handle`].

use std::collections::{BTreeMap, BTreeSet};

use crate::active_group::{ActiveGroupMember, describe_active_group_with_hints};
use crate::contract::{
    DivergenceKind, FocusOperation, LIFECYCLE_POLICY_VERSION, LifecycleOperation,
    LifecyclePrecondition, ResizeMode, ResizeOperation,
};
use crate::directional::{
    Capability, CrossOutputTarget, Direction, MoveOperation, NodeId, OutputId, Precondition, Rule,
    SameAxisMove, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{
    DesiredGeometry, DomainKey, DragHoverPrior, DragPreview, OutputDomain, Session,
    SessionDragPlan, SessionFocusPlan, SessionMovePlan, SessionPlan, SessionResizePlan,
};

/// Typed command for all 15 wire ops: reconcile, update-gaps, active-group,
/// release-domain, move, focus, resize, pointer-resize, toggle-float,
/// toggle-orientation, `send-to-workspace`, `send-to-output`,
/// `migrate-workspace`, `drag-drop`, and read-only `drag-preview`.
/// Payloads are already-decoded clones; fallible wire vocabularies
/// (direction/mode) cross opaquely so this conversion stays total
/// and handler precedence is untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    Reconcile,
    UpdateGaps,
    ActiveGroup,
    ReleaseDomain,
    Move {
        window: String,
        direction: String,
        cross_output_transfer: bool,
        /// R-MOV-03 same-axis mode: validated at the protocol boundary
        /// (missing wire field decodes to [`SameAxisMove::GroupWithNeighbor`]);
        /// adapters constructing events directly (Windows) pass the default
        /// to preserve the historical wrap behavior.
        same_axis_move: SameAxisMove,
    },
    Focus {
        window: String,
        direction: String,
        cross_output_transfer: bool,
        /// Float-origin cross-output focus (Left/Right miss with no local
        /// float target): the subject is a floating/sticky exception, so the
        /// local tile-edge check is skipped and the existing reciprocal
        /// adjacent remembered-tiled-target fallback applies. False for every
        /// tile-origin request; true only from the adapter float path.
        float_subject: bool,
    },
    Resize {
        window: String,
        direction: String,
        mode: String,
        press_index: u32,
    },
    PointerResize {
        window: String,
        direction: String,
        boundary: i32,
        /// Second-axis corner pair: both `Some` for an atomic corner
        /// (dual-axis) resize, both `None` for an ordinary single-axis
        /// request. A half-present pair never reaches the Engine (the
        /// protocol layer refuses it as `pointer-resize-op-invalid`).
        direction2: Option<String>,
        boundary2: Option<i32>,
    },
    ToggleFloat {
        window: String,
        float_rect: Option<Rect>,
    },
    ToggleOrientation {
        window: String,
    },
    SendToWorkspace {
        window: String,
        target_output: String,
        target_workspace: String,
        /// True follows the mover into the target (default, preserves the
        /// historical follow behavior); false stays on the source domain.
        follow: bool,
    },
    /// Explicit output send (REQ-OUT-04, item 5.3/5.4): the destination
    /// output's current workspace resolved adapter-side, carried as an
    /// explicit cross-output target. Distinct from `SendToWorkspace`, which
    /// stays same-output. Ordinary admission and explicit follow/stay.
    SendToOutput {
        window: String,
        target_output: String,
        target_workspace: String,
        /// True follows the mover into the target (default); false stays on
        /// the source domain.
        follow: bool,
    },
    /// Explicit whole-workspace output migration (R-WS-12): the active
    /// workspace keeps its stable backing id and moves to the adjacent
    /// output resolved adapter-side via full-output-rect adjacency (no wrap).
    /// Follow-only; the adapter shows the migrated workspace on the target
    /// and refills the source natively. Direction crosses opaquely so
    /// handler-local precedence stays untouched.
    MigrateWorkspace {
        direction: String,
    },
    DragDrop {
        window: String,
        x: i32,
        y: i32,
        /// Optional carried sticky hover prior, applied advisory-style between
        /// begin and drop via `Session::carry_drag_prior`. `None` preserves
        /// legacy behavior; mismatched carries are ignored, never refused.
        hover_prior: Option<DragHoverPrior>,
        /// Optional validated Started cross-output source binding (source
        /// domain key). `None` preserves same-output behavior; singleton
        /// success requires `Some` with a different output from the request
        /// destination (validated opaque, cross-output only). Occupied
        /// destinations additionally carry the source-first pair in
        /// `CoreEvent.directional` with the combined both-domain rows in
        /// `CoreEvent.windows`: the Engine then runs one two-domain pair
        /// transaction (source removal plus destination insertion at the
        /// pointer) instead of the singleton admission route.
        source: Option<DomainKey>,
    },
    DragPreview {
        window: String,
        x: i32,
        y: i32,
        /// Optional carried sticky hover prior, applied advisory-style between
        /// begin and preview via `Session::carry_drag_prior`.
        hover_prior: Option<DragHoverPrior>,
        /// Optional validated Started cross-output source binding, mirroring
        /// `DragDrop.source`.
        source: Option<DomainKey>,
    },
}

impl CoreCommand {
    /// Wire `op` token for this command.
    #[must_use]
    pub const fn op(&self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::UpdateGaps => "update-gaps",
            Self::ActiveGroup => "active-group",
            Self::ReleaseDomain => "release-domain",
            Self::Move { .. } => "move",
            Self::Focus { .. } => "focus",
            Self::Resize { .. } => "resize",
            Self::PointerResize { .. } => "pointer-resize",
            Self::ToggleFloat { .. } => "toggle-float",
            Self::ToggleOrientation { .. } => "toggle-orientation",
            Self::SendToWorkspace { .. } => "send-to-workspace",
            Self::SendToOutput { .. } => "send-to-output",
            Self::MigrateWorkspace { .. } => "migrate-workspace",
            Self::DragDrop { .. } => "drag-drop",
            Self::DragPreview { .. } => "drag-preview",
        }
    }
}

/// Typed world-level event: the validated observation envelope plus one typed
/// command. Directional pair and workspace-target observations are cloned core
/// values for the routes that carry them; `None`/empty elsewhere.
/// `directional_target_outer_gap` carries the validated target `outer_gap` for
/// the two-domain directional move/focus request route (`None` elsewhere); the
/// source gap rides in `outer_gap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoreEvent {
    pub owner: OwnerId,
    pub generation: GenerationId,
    pub correlation: CorrelationId,
    pub revision: u64,
    pub fingerprint: u64,
    pub domain: OutputDomain,
    pub domain_key: DomainKey,
    pub outer_gap: i32,
    pub focused_window: WindowId,
    pub windows: Vec<EngineWindow>,
    /// D23 close-versus-minimize distinction: opaque ids the adapter reports
    /// as explicitly closed (never merely absent). Absent stays a minimize
    /// (stored old slot); a closed id never mints a permanent slot and any
    /// stored slot for it is forgotten. Empty by default so legacy
    /// observations behave exactly as before.
    pub closed_windows: Vec<WindowId>,
    pub directional: Option<Vec<(OutputDomain, DomainKey)>>,
    pub directional_target_outer_gap: Option<i32>,
    pub target_domain: Option<(OutputDomain, DomainKey)>,
    pub target_windows: Vec<EngineWindow>,
    pub command: CoreCommand,
}

/// Tiled-plan kind for every `planned`-outcome route. Wire `kind`/`capability`
/// strings stay in protocol; this enum only classifies the typed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum TiledKind {
    Reconcile,
    UpdateGaps,
    Admit,
    Move,
    Focus,
    Resize,
    PointerResize,
    ToggleFloat,
    ToggleOrientation,
    SendToWorkspace,
    DirectionalMove,
    DragDrop,
}

impl TiledKind {
    /// Wire `detail.kind` token for this plan kind.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn kind_str(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::UpdateGaps => "update-gaps",
            Self::Admit => "admit",
            Self::Move => "move",
            Self::Focus => "focus",
            Self::Resize => "resize",
            Self::PointerResize => "pointer-resize",
            Self::ToggleFloat => "toggle-float",
            Self::ToggleOrientation => "toggle-orientation",
            Self::SendToWorkspace => "send-to-workspace",
            Self::DirectionalMove => "directional-move",
            Self::DragDrop => "drag-drop",
        }
    }

    /// Wire `detail.capability` literal for plan kinds whose reply carries a
    /// fixed capability token. `None` for kinds whose capability is owned
    /// elsewhere: reconcile/update-gaps (see [`ProjectionKind`]) and
    /// move/directional-move (whose wire capability is a dynamically
    /// formatted Debug token, out of scope until that family migrates).
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn capability_str(self) -> Option<&'static str> {
        match self {
            Self::Admit => Some("admit-tiled"),
            Self::Focus => Some("directional-focus"),
            Self::Resize => Some("keyboard-resize"),
            Self::PointerResize => Some("pointer-resize"),
            Self::ToggleFloat => Some("intentional-float"),
            Self::ToggleOrientation => Some("toggle-orientation"),
            Self::SendToWorkspace => Some("move-tiled"),
            Self::DragDrop => Some("place-tiled"),
            Self::Reconcile | Self::UpdateGaps | Self::Move | Self::DirectionalMove => None,
        }
    }
}

/// Generic tiled success plan: base revision, policy version, classified kind,
/// full desired geometry, retained focus, and optional intentional-float rect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TiledPlan {
    pub base_revision: u64,
    pub policy_version: u32,
    pub kind: TiledKind,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: Option<DomainKey>,
    pub focus_leaf: Option<NodeId>,
    pub float_window: Option<WindowId>,
    pub float_rect: Option<Rect>,
}

impl TiledPlan {
    /// Typed construction from an authoritative lifecycle [`SessionPlan`]:
    /// carries base revision, policy version, geometry, and focus without
    /// re-validating. Float stays empty; use [`TiledPlan::for_toggle_float`]
    /// for the intentional-float route.
    #[must_use]
    pub(crate) fn from_lifecycle(kind: TiledKind, plan: &SessionPlan) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            policy_version: plan.dispatch.policy_version,
            kind,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
            float_window: None,
            float_rect: None,
        }
    }

    /// Typed construction for the intentional-float route: as
    /// [`TiledPlan::from_lifecycle`] plus the effective float rectangle and
    /// its window derived from the plan operation (the `Remove` window, else
    /// an empty window exactly like the legacy reply when a rect is set).
    #[must_use]
    pub(crate) fn for_toggle_float(plan: &SessionPlan, float_rect: Option<Rect>) -> Self {
        let mut tiled = Self::from_lifecycle(TiledKind::ToggleFloat, plan);
        if let Some(rect) = float_rect {
            tiled.float_window = Some(match &plan.dispatch.operation {
                LifecycleOperation::Remove { window, .. } => window.clone(),
                _ => WindowId(String::new()),
            });
            tiled.float_rect = Some(rect);
        }
        tiled
    }

    /// Typed construction for the synchronous drag-drop route from an
    /// authoritative [`SessionDragPlan`]: carries base revision, the shared
    /// lifecycle policy version, full desired geometry, and focus preserved
    /// on the moved window. Never validates; the session policy owns
    /// placement.
    #[must_use]
    pub(crate) fn from_drag(plan: &SessionDragPlan) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            policy_version: LIFECYCLE_POLICY_VERSION,
            kind: TiledKind::DragDrop,
            geometry: plan.desired_geometry.clone(),
            focus_domain: Some(plan.desired_focus_domain.clone()),
            focus_leaf: Some(plan.desired_focus_leaf.clone()),
            float_window: None,
            float_rect: None,
        }
    }
}

/// Typed workspace-send success plan: base revision, policy version, full
/// desired geometry, retained focus, follow/stay selection, and the exact
/// `MoveTiled` (or slotless `MoveDeferred`) operation plus lifecycle
/// preconditions for native assignment.
/// Construction is fallible (`None` unless the dispatch carries both the
/// `MoveTiled` operation and an explicit send intent with its
/// follow/stay selection); the caller maps that to its existing
/// `move-op-invalid` rejection at the exact legacy position. Never validates.
/// Shared by workspace send (`MoveToWorkspace` intent) and explicit output
/// send (`MoveToOutput` intent): the transfer shape is ordinary in both; only
/// the target scope differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendWorkspacePlan {
    pub base_revision: u64,
    pub policy_version: u32,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: Option<DomainKey>,
    pub focus_leaf: Option<NodeId>,
    pub follow: bool,
    pub operation: LifecycleOperation,
    pub preconditions: Vec<LifecyclePrecondition>,
}

impl SendWorkspacePlan {
    /// Typed construction from an authoritative workspace [`SessionPlan`]:
    /// requires an explicit send intent (which carries the
    /// follow/stay selection) alongside the `MoveTiled` operation, the
    /// slotless `MoveDeferred` operation (D28 born-fullscreen carry: window
    /// plus source/target identities, no leaf), or the capacity-float
    /// `MoveToFloat` operation (D3: window plus dropped source leaf and
    /// source/target identities, no target slot). A move operation under any
    /// other intent is not a send and refuses.
    #[must_use]
    pub(crate) fn from_session(plan: &SessionPlan) -> Option<Self> {
        let follow = match &plan.dispatch.intent {
            crate::contract::LifecycleIntent::MoveToWorkspace { follow, .. }
            | crate::contract::LifecycleIntent::MoveToOutput { follow, .. } => *follow,
            _ => return None,
        };
        match &plan.dispatch.operation {
            LifecycleOperation::MoveTiled { .. }
            | LifecycleOperation::MoveDeferred { .. }
            | LifecycleOperation::MoveToFloat { .. } => Some(Self {
                base_revision: plan.dispatch.base_revision,
                policy_version: plan.dispatch.policy_version,
                geometry: plan.desired_geometry.clone(),
                focus_domain: plan.desired_focus_domain.clone(),
                focus_leaf: plan.desired_focus_leaf.clone(),
                follow,
                operation: plan.dispatch.operation.clone(),
                preconditions: plan.dispatch.preconditions.clone(),
            }),
            _ => None,
        }
    }
}

/// Typed whole-workspace migration success plan (R-WS-12): base revision,
/// requested direction, frozen source/target domain keys, projected target
/// geometry, retained focus, the active client, and the lifecycle
/// preconditions the adapter must fence (including
/// `adapter-must-verify-postconditions`). The retained rekey already
/// committed; `planned` never claims native completion. The adapter transfers
/// natively and gates follow/completion on verified membership/view readback.
/// `focus_domain`/`focus_leaf` name the tiled layout focus, present only when
/// the active client is a migrating tiled member; `active_window` names the
/// migrated active member for native activation (`None` when the request
/// carried no active client or the active stayed sticky: follow-only, never
/// activate a source window on the target). Never validates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrateWorkspacePlan {
    pub base_revision: u64,
    pub direction: Direction,
    pub source: DomainKey,
    pub target: DomainKey,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: Option<DomainKey>,
    pub focus_leaf: Option<NodeId>,
    pub active_window: Option<WindowId>,
    /// Relocated member count (tiled links plus workspace float exceptions),
    /// excluding stayed sticky. Count only, for wire detail and logs.
    pub members: usize,
    /// Relocated float exception count. Count only.
    pub floats: usize,
    pub preconditions: Vec<LifecyclePrecondition>,
}

/// Typed local move success plan: base revision, structural rule and required
/// capability (protocol formats the exact wire `Debug` tokens), requested
/// direction, full desired geometry, and retained focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MovePlanReply {
    pub base_revision: u64,
    pub rule: Rule,
    pub capability: Capability,
    pub direction: Direction,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: DomainKey,
    pub focus_leaf: NodeId,
    pub cross: Option<MoveCrossView>,
}

/// Typed R4 cross-output move echo: intent direction/window/leaf plus the
/// target side and preconditions the adapter must fence. The source side
/// comes from the protocol pair lookup (which owns its fence), so the
/// constructor takes it as already-checked input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MoveCrossView {
    pub rule: Rule,
    pub intent_direction: Direction,
    pub intent_window: WindowId,
    pub intent_leaf: NodeId,
    pub source_output: OutputId,
    pub source_workspace: WorkspaceId,
    pub target_output: OutputId,
    pub target_workspace: WorkspaceId,
    pub source_root_child_index: usize,
    pub target: CrossOutputTarget,
    pub preconditions: Vec<Precondition>,
}

impl MovePlanReply {
    /// Typed construction for synchronous local (R1-R3) move plans.
    #[must_use]
    pub(crate) fn from_local(direction: Direction, plan: &SessionMovePlan) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            rule: plan.dispatch.rule,
            capability: plan.dispatch.required_capability,
            direction,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
            cross: None,
        }
    }

    /// Typed construction for R4 cross-output move plans: `None` unless the
    /// plan operation actually crossed. Never stages pending itself.
    #[must_use]
    pub(crate) fn from_cross(
        plan: &SessionMovePlan,
        source_output: &OutputId,
        source_workspace: &WorkspaceId,
    ) -> Option<Self> {
        let MoveOperation::CrossOutput {
            rule,
            target_output,
            target_workspace,
            source_root_child_index,
            target,
        } = &plan.dispatch.operation
        else {
            return None;
        };
        Some(Self {
            base_revision: plan.dispatch.base_revision,
            rule: *rule,
            capability: plan.dispatch.required_capability,
            direction: plan.dispatch.intent.direction,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
            cross: Some(MoveCrossView {
                rule: *rule,
                intent_direction: plan.dispatch.intent.direction,
                intent_window: plan.dispatch.intent.focused_window.clone(),
                intent_leaf: plan.dispatch.intent.focused_leaf.clone(),
                source_output: source_output.clone(),
                source_workspace: source_workspace.clone(),
                target_output: target_output.clone(),
                target_workspace: target_workspace.clone(),
                source_root_child_index: *source_root_child_index,
                target: *target,
                preconditions: plan.dispatch.preconditions.clone(),
            }),
        })
    }
}

/// Typed focus success plan: base revision, requested direction, resolved
/// target window, full desired geometry, and retained focus. The cross-output
/// operation echo rides along only for crossed plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusPlanReply {
    pub base_revision: u64,
    pub direction: Direction,
    pub to_window: WindowId,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: DomainKey,
    pub focus_leaf: NodeId,
    pub cross_operation: Option<FocusOperation>,
}

impl FocusPlanReply {
    /// Typed construction for local focus plans.
    #[must_use]
    pub(crate) fn from_local(direction: Direction, plan: &SessionFocusPlan) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            direction,
            to_window: plan.dispatch.operation.to_window.clone(),
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
            cross_operation: None,
        }
    }

    /// Typed construction for crossed focus plans (carries the operation the
    /// adapter must fence; preconditions derive from it in protocol).
    #[must_use]
    pub(crate) fn from_cross(direction: Direction, plan: &SessionFocusPlan) -> Self {
        let mut reply = Self::from_local(direction, plan);
        reply.cross_operation = Some(plan.dispatch.operation.clone());
        reply
    }
}

/// Typed resize success plan: base revision, requested direction, keyboard
/// mode or pointer boundary (exactly one, set by the matching constructor),
/// full operation for share/group detail, geometry, and retained focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResizePlanReply {
    pub base_revision: u64,
    pub direction: Direction,
    pub mode: Option<ResizeMode>,
    pub boundary: Option<i32>,
    pub operation: ResizeOperation,
    /// Second-axis corner detail: `Some` only for an atomic corner reply,
    /// carrying the vertical direction, proposed boundary, and operation.
    /// `None` for every single-axis reply, whose wire shape is unchanged.
    /// Boxed: corner detail rides cold-path only.
    pub secondary: Option<Box<SecondaryPointerResize>>,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: DomainKey,
    pub focus_leaf: NodeId,
}

/// Second-axis detail of an atomic corner pointer-resize reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecondaryPointerResize {
    pub direction: Direction,
    pub boundary: i32,
    pub operation: ResizeOperation,
}

impl ResizePlanReply {
    /// Typed construction for keyboard resize plans.
    #[must_use]
    pub(crate) fn from_keyboard(
        direction: Direction,
        mode: ResizeMode,
        plan: &SessionResizePlan,
    ) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            direction,
            mode: Some(mode),
            boundary: None,
            operation: plan.dispatch.operation.clone(),
            secondary: None,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
        }
    }

    /// Typed construction for pointer resize plans.
    #[must_use]
    pub(crate) fn from_pointer(
        direction: Direction,
        boundary: i32,
        plan: &SessionResizePlan,
    ) -> Self {
        Self {
            base_revision: plan.dispatch.base_revision,
            direction,
            mode: None,
            boundary: Some(boundary),
            operation: plan.dispatch.operation.clone(),
            secondary: None,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
        }
    }

    /// Typed construction for atomic corner pointer resize plans: the
    /// primary (horizontal) direction/boundary plus the bound vertical
    /// second axis from the plan's secondary operation.
    #[must_use]
    pub(crate) fn from_pointer_corner(
        direction: Direction,
        boundary: i32,
        direction2: Direction,
        boundary2: i32,
        plan: &SessionResizePlan,
    ) -> Self {
        let secondary = plan.secondary_plan.as_ref().map(|secondary| {
            Box::new(SecondaryPointerResize {
                direction: direction2,
                boundary: boundary2,
                operation: secondary.operation.clone(),
            })
        });
        Self {
            base_revision: plan.dispatch.base_revision,
            direction,
            mode: None,
            boundary: Some(boundary),
            operation: plan.dispatch.operation.clone(),
            secondary,
            geometry: plan.desired_geometry.clone(),
            focus_domain: plan.desired_focus_domain.clone(),
            focus_leaf: plan.desired_focus_leaf.clone(),
        }
    }
}

/// Successful active-group resolution with everything the wire reply echoes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveGroupFound {
    pub base_revision: u64,
    pub group: NodeId,
    pub focused_leaf: NodeId,
    pub focused_window: WindowId,
    pub members: Vec<ActiveGroupMember>,
    pub bounds: Rect,
}

/// Closed read-only `no-group` reason registry (wire tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoGroupReason {
    NoSession,
    Diverged,
    Pending,
    DomainMismatch,
    StaleRevision,
    FocusMismatch,
    FocusUnmapped,
    NoTree,
    NoParentGroup,
}

impl NoGroupReason {
    /// Wire `reason` token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSession => "no-session",
            Self::Diverged => "diverged",
            Self::Pending => "pending",
            Self::DomainMismatch => "domain-mismatch",
            Self::StaleRevision => "stale-revision",
            Self::FocusMismatch => "focus-mismatch",
            Self::FocusUnmapped => "focus-unmapped",
            Self::NoTree => "no-tree",
            Self::NoParentGroup => "no-parent-group",
        }
    }
}

/// Typed active-group resolution: either the found group or a classified
/// `no-group` with the retained base revision when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActiveGroupResolution {
    Found(ActiveGroupFound),
    NoGroup {
        base_revision: Option<u64>,
        reason: NoGroupReason,
    },
}

/// Read-only drag-preview success plan: the retained base revision plus the
/// authoritative [`DragPreview`] from a discarded working clone. Carries the
/// proposed rect and the carried hover state (`preview.hover_prior()`); never
/// commits, never advances the revision, never seeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DragPreviewPlan {
    pub base_revision: u64,
    pub preview: DragPreview,
}

/// Typed reply across all 14 ops plus every rejection shape. Success variants
/// carry core plans; rejection variants carry the closed
/// `&'static str` kind/message/detail vocabulary (single sources live in
/// [`crate::session`]/[`crate::contract`] and the protocol `MSG_*`
/// constants). Serialization stays entirely in protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoreReply {
    Projection(ProjectionPlan),
    Tiled(TiledPlan),
    SendWorkspace(SendWorkspacePlan),
    /// Explicit output send success: the same ordinary [`SendWorkspacePlan`]
    /// transfer shape under the distinct `send-to-output` wire kind.
    SendOutput(SendWorkspacePlan),
    /// Explicit whole-workspace migration success: the retained
    /// [`MigrateWorkspacePlan`] rekey shape under the distinct
    /// `migrate-workspace` wire kind.
    MigrateWorkspace(MigrateWorkspacePlan),
    MoveDirectional(MovePlanReply),
    FocusDirectional(FocusPlanReply),
    Resize(ResizePlanReply),
    ActiveGroup(ActiveGroupFound),
    NoGroup {
        base_revision: Option<u64>,
        reason: NoGroupReason,
    },
    DragPreview(DragPreviewPlan),
    /// Explicit domain release: the exact domain slot is gone, so there is
    /// no geometry, focus, or revision to report. The adapter must perform
    /// zero native writes for this reply.
    Released,
    Rejected {
        kind: &'static str,
        message: &'static str,
    },
    SnapshotInvalid {
        message: &'static str,
        detail: &'static str,
    },
    Diverged(DivergenceKind),
}

/// Pure-projection plan kind for the reconcile/update-gaps family: both are
/// read-only retained-tree projections with no pending, no policy version in
/// the wire detail, and identical reply shapes. The wire `kind`/`capability`
/// tokens live here as the single source; protocol only serializes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionKind {
    Reconcile,
    UpdateGaps,
}

impl ProjectionKind {
    /// Wire `detail.kind` token.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn kind_str(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::UpdateGaps => "update-gaps",
        }
    }

    /// Wire `detail.capability` token.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn capability_str(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile-geometry",
            Self::UpdateGaps => "update-gaps-geometry",
        }
    }
}

/// Typed pure-projection success plan: base revision, family kind, full
/// desired geometry in wire order, and retained focus (`None` only for the
/// empty-domain path, which carries no geometry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionPlan {
    pub base_revision: u64,
    pub kind: ProjectionKind,
    pub geometry: Vec<DesiredGeometry>,
    pub focus_domain: Option<DomainKey>,
    pub focus_leaf: Option<NodeId>,
}

/// Pure retained-tree tiled projection for the reconcile/update-gaps family.
///
/// Projects the retained domain tree into `bounds` with `gap`, honoring the
/// per-window client size hints (`hints`, AR12) exactly like every other
/// workflow: satisfiable minimums take slack from siblings, unsatisfiable
/// windows keep the proportional fallback flagged `overconstrained` on their
/// [`DesiredGeometry`]. Rebuilds authoritative desired geometry from retained
/// topology only (observed client rectangles are never adopted, shares
/// untouched), sorts by (output, workspace, leaf), and refuses tiled-coverage
/// mismatch.
///
/// The observed rectangles (`observed`, floating/fit-excluded entries
/// skipped: exception rects are compositor-owned, never layout-driven) are
/// compared against the desired allocation through
/// [`crate::size_hints::assess_window_clamp`]: a window whose observed size
/// accepts as a client clamp of its desired size is flagged `client_clamped`
/// on its [`DesiredGeometry`]. The adapter must then neither rewrite that
/// window nor count it toward drift/park; the retained desired rectangle
/// stays the authoritative truth so no drift accumulates. Overconstrained
/// windows carry only the overconstrained flag (never reasserted either).
/// Windows without meaningful hints never accept: unhinted drift still
/// reasserts fully.
///
/// Returns `None` for every projection-shape failure; the caller maps that
/// to its existing `malformed-topology` rejection at the exact legacy
/// position. All fences (domain binding, divergence, pending, membership,
/// focus, outer gap) stay in the caller, which passes its already-checked
/// focus through untouched.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_retained_tiled_geometry(
    session: &Session,
    domain_key: &DomainKey,
    bounds: Rect,
    gap: i32,
    focus: Option<(DomainKey, NodeId)>,
    kind: ProjectionKind,
    hints: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
    observed: &[EngineWindow],
) -> Option<ProjectionPlan> {
    let snapshot = session.snapshot();
    let tree = snapshot
        .domains
        .into_iter()
        .find(|domain| {
            domain.output == domain_key.output && domain.workspace == domain_key.workspace
        })
        .and_then(|domain| domain.tree)?;
    let leaf_to_window: BTreeMap<String, String> = snapshot
        .windows
        .into_iter()
        .filter(|link| link.output == domain_key.output && link.workspace == domain_key.workspace)
        .map(|link| (link.leaf.0, link.window.0))
        .collect();
    let resolve = |leaf: &crate::directional::NodeId| {
        leaf_to_window
            .get(&leaf.0)
            .and_then(|window| hints.get(&WindowId(window.clone())))
            .copied()
            .unwrap_or_else(crate::size_hints::WindowSizeHints::none)
    };
    // Strict first; overlap only through the D9 gate: the no-space
    // setting selects it, or the domain was overlap-accepted (retained
    // allocations stay publishable after switching to float until a
    // real capacity decision). Anything else keeps the truthful hold.
    enum Projected {
        Strict(Vec<crate::geometry::ProjectedLeaf>, BTreeSet<String>),
        Overlap(Vec<crate::geometry::ProjectedLeaf>, BTreeSet<String>),
    }
    let projected: Projected =
        match crate::size_hints::project_with_hints(&tree, bounds, gap, &resolve) {
            Ok(hinted) => Projected::Strict(
                hinted.leaves,
                hinted
                    .overconstrained
                    .iter()
                    .map(|id| id.0.clone())
                    .collect(),
            ),
            Err(_) => {
                let allowed = session.no_space_result()
                    == crate::admission::NoSpaceResult::AllowOverlap
                    || session.overlap_accepted(domain_key);
                if !allowed {
                    return None;
                }
                let hinted =
                    crate::size_hints::project_overlap_with_hints(&tree, bounds, gap, &resolve)
                        .ok()?;
                Projected::Overlap(
                    hinted.leaves,
                    hinted
                        .overconstrained
                        .iter()
                        .map(|id| id.0.clone())
                        .collect(),
                )
            }
        };
    let (leaves, over) = match projected {
        Projected::Strict(leaves, over) => (leaves, over),
        Projected::Overlap(leaves, over) => (leaves, over),
    };
    // Observed layout-driven rectangles by window id for clamp assessment.
    let mut observed_rects: BTreeMap<&str, &Rect> = BTreeMap::new();
    for entry in observed {
        if entry.floating || entry.fit_excluded {
            continue;
        }
        observed_rects.insert(entry.window.0.as_str(), &entry.rect);
    }
    let mut geometry: Vec<DesiredGeometry> = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let window = leaf_to_window.get(&leaf.leaf.0)?;
        if leaf.rect.w <= 0 || leaf.rect.h <= 0 {
            return None;
        }
        // `over` carries leaf ids; map through the projected leaf here,
        // not the window id.
        let overconstrained = over.contains(&leaf.leaf.0);
        let window_id = WindowId(window.clone());
        // Accepted clamps never perturb shares (this path is read-only) and
        // never synthesize a plan: the flag only tells the adapter to leave
        // the observed rectangle alone. Overconstrained windows rely solely
        // on their own flag.
        let client_clamped = !overconstrained
            && observed_rects.get(window.as_str()).is_some_and(|rect| {
                **rect != leaf.rect
                    && crate::size_hints::assess_window_clamp(
                        rect,
                        &leaf.rect,
                        &hints
                            .get(&window_id)
                            .copied()
                            .unwrap_or_else(crate::size_hints::WindowSizeHints::none),
                    )
                    .accepted
            });
        geometry.push(DesiredGeometry {
            window: window_id,
            leaf: leaf.leaf.clone(),
            output: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            rect: leaf.rect,
            overconstrained,
            client_clamped,
        });
    }
    geometry.sort_by(|a, b| {
        a.output
            .0
            .cmp(&b.output.0)
            .then(a.workspace.0.cmp(&b.workspace.0))
            .then(a.leaf.0.cmp(&b.leaf.0))
    });
    if geometry.len() != leaf_to_window.len() {
        return None;
    }
    let (focus_domain, focus_leaf) = match focus {
        Some((domain, leaf)) => (Some(domain), Some(leaf)),
        None => (None, None),
    };
    Some(ProjectionPlan {
        base_revision: session.accepted_revision(),
        kind,
        geometry,
        focus_domain,
        focus_leaf,
    })
}

/// Resolve the read-only active-group query against one authoritative session.
///
/// Pure over retained state plus the carried advisory size hints: validates
/// domain binding, focus mapping, and tree presence, then derives the focused
/// leaf's immediate parent group through
/// [`describe_active_group_with_hints`] with the retained bounds/gap and the
/// hint-aware engine projection ordinary plans use. Never mutates; carried
/// windows supply advisory size hints only (leaf -> retained window ->
/// carried hints, unknown resolving to none, exactly like ordinary plans;
/// overlay entries ride the same map and simply never match a retained tiled
/// leaf). Carried rectangles and membership are never consulted for topology
/// or geometry. A `None` session reports `NoSession` with no base revision.
/// The carried revision is intentionally not gated: this is a current-state
/// snapshot.
pub(crate) fn resolve_active_group(
    session: Option<&Session>,
    event: &CoreEvent,
) -> ActiveGroupResolution {
    let Some(session) = session else {
        return ActiveGroupResolution::NoGroup {
            base_revision: None,
            reason: NoGroupReason::NoSession,
        };
    };
    let base = session.accepted_revision();
    let no_group = |reason| ActiveGroupResolution::NoGroup {
        base_revision: Some(base),
        reason,
    };
    if session.divergence().is_some() {
        return no_group(NoGroupReason::Diverged);
    }
    if session.has_pending() || session.has_pending_desired() || session.has_drag() {
        return no_group(NoGroupReason::Pending);
    }
    let Some(retained_domain) = session
        .domains()
        .iter()
        .find(|domain| domain.key() == event.domain_key)
        .cloned()
    else {
        return no_group(NoGroupReason::DomainMismatch);
    };
    if retained_domain.bounds != event.domain.bounds || retained_domain.gap != event.domain.gap {
        return no_group(NoGroupReason::DomainMismatch);
    }
    let (focus_domain, focus_leaf) = session.focus();
    let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) else {
        return no_group(NoGroupReason::FocusMismatch);
    };
    if focus_domain != event.domain_key {
        return no_group(NoGroupReason::FocusMismatch);
    }
    let snapshot = session.snapshot();
    let Some(tree) = snapshot
        .domains
        .into_iter()
        .find(|domain| {
            domain.output == event.domain_key.output
                && domain.workspace == event.domain_key.workspace
        })
        .and_then(|domain| domain.tree)
    else {
        return no_group(NoGroupReason::NoTree);
    };
    let leaf_to_window: BTreeMap<NodeId, WindowId> = snapshot
        .windows
        .into_iter()
        .filter(|link| {
            link.output == event.domain_key.output && link.workspace == event.domain_key.workspace
        })
        .map(|link| (link.leaf, link.window))
        .collect();
    match leaf_to_window.get(&focus_leaf) {
        Some(window) if *window == event.focused_window => {}
        _ => return no_group(NoGroupReason::FocusUnmapped),
    }
    // Advisory hints resolve leaf -> retained window -> carried hints with no
    // flag filtering, exactly like ordinary plans: overlays and floats ride
    // the same map and never match a retained tiled leaf.
    let carried_hints: BTreeMap<&WindowId, crate::size_hints::WindowSizeHints> = event
        .windows
        .iter()
        .map(|entry| (&entry.window, entry.hints))
        .collect();
    let resolve = |leaf: &NodeId| {
        leaf_to_window
            .get(leaf)
            .and_then(|window| carried_hints.get(window))
            .copied()
            .unwrap_or_else(crate::size_hints::WindowSizeHints::none)
    };
    let Some(group) = describe_active_group_with_hints(
        &tree,
        retained_domain.bounds,
        retained_domain.gap,
        &focus_leaf,
        &leaf_to_window,
        &resolve,
    ) else {
        return no_group(NoGroupReason::NoParentGroup);
    };
    ActiveGroupResolution::Found(ActiveGroupFound {
        base_revision: base,
        group: group.group,
        focused_leaf: focus_leaf,
        focused_window: event.focused_window.clone(),
        members: group.members,
        bounds: group.bounds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::LifecycleIntent;
    use crate::directional::Node;
    use crate::ids::{CorrelationId, GenerationId, OwnerId};
    use crate::session::OutputDomain;

    fn ids() -> (OwnerId, GenerationId, CorrelationId) {
        (
            OwnerId::parse("owner-a").expect("valid"),
            GenerationId::parse("gen-1").expect("valid"),
            CorrelationId::parse("corr-1").expect("valid"),
        )
    }

    fn domain_fixture() -> (OutputDomain, DomainKey) {
        let domain = OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1200,
                h: 800,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        };
        let key = domain.key();
        (domain, key)
    }

    fn event_fixture(command: CoreCommand) -> CoreEvent {
        let (owner, generation, correlation) = ids();
        let (domain, domain_key) = domain_fixture();
        CoreEvent {
            owner,
            generation,
            correlation,
            revision: 0,
            fingerprint: 0,
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: Vec::new(),
            closed_windows: Vec::new(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            domain,
            domain_key,
            command,
        }
    }

    #[test]
    fn all_fifteen_ops_have_distinct_wire_tokens() {
        use std::collections::HashSet;
        let commands = vec![
            CoreCommand::Reconcile,
            CoreCommand::UpdateGaps,
            CoreCommand::ActiveGroup,
            CoreCommand::ReleaseDomain,
            CoreCommand::Move {
                window: "w".to_owned(),
                direction: "left".to_owned(),
                cross_output_transfer: true,
                same_axis_move: SameAxisMove::GroupWithNeighbor,
            },
            CoreCommand::Focus {
                window: "w".to_owned(),
                direction: "left".to_owned(),
                cross_output_transfer: true,
                float_subject: false,
            },
            CoreCommand::Resize {
                window: "w".to_owned(),
                direction: "left".to_owned(),
                mode: "inwards".to_owned(),
                press_index: 0,
            },
            CoreCommand::PointerResize {
                window: "w".to_owned(),
                direction: "left".to_owned(),
                boundary: 0,
                direction2: None,
                boundary2: None,
            },
            CoreCommand::ToggleFloat {
                window: "w".to_owned(),
                float_rect: None,
            },
            CoreCommand::ToggleOrientation {
                window: "w".to_owned(),
            },
            CoreCommand::SendToWorkspace {
                window: "w".to_owned(),
                target_output: "o".to_owned(),
                target_workspace: "s".to_owned(),
                follow: true,
            },
            CoreCommand::SendToOutput {
                window: "w".to_owned(),
                target_output: "o".to_owned(),
                target_workspace: "s".to_owned(),
                follow: true,
            },
            CoreCommand::MigrateWorkspace {
                direction: "right".to_owned(),
            },
            CoreCommand::DragDrop {
                window: "w".to_owned(),
                x: 0,
                y: 0,
                hover_prior: None,
                source: None,
            },
            CoreCommand::DragPreview {
                window: "w".to_owned(),
                x: 0,
                y: 0,
                hover_prior: None,
                source: None,
            },
        ];
        assert_eq!(commands.len(), 15);
        let tokens: HashSet<&'static str> = commands.iter().map(|c| c.op()).collect();
        assert_eq!(tokens.len(), 15);
        assert!(tokens.contains("reconcile"));
        assert!(tokens.contains("release-domain"));
        assert!(tokens.contains("send-to-workspace"));
        assert!(tokens.contains("send-to-output"));
        assert!(tokens.contains("migrate-workspace"));
        assert!(tokens.contains("drag-drop"));
        assert!(tokens.contains("drag-preview"));
        assert_eq!(TiledKind::DragDrop.kind_str(), "drag-drop");
        assert_eq!(TiledKind::DragDrop.capability_str(), Some("place-tiled"));
    }

    #[test]
    fn missing_session_reports_no_session_without_base() {
        let event = event_fixture(CoreCommand::ActiveGroup);
        assert_eq!(
            resolve_active_group(None, &event),
            ActiveGroupResolution::NoGroup {
                base_revision: None,
                reason: NoGroupReason::NoSession,
            }
        );
        assert_eq!(NoGroupReason::NoSession.as_str(), "no-session");
        assert_eq!(TiledKind::Admit.kind_str(), "admit");
    }

    #[test]
    fn empty_session_without_tree_reports_no_tree() {
        let (owner, generation, _) = ids();
        let (domain, _) = domain_fixture();
        let session = Session::new(owner, generation, 0, 0, vec![domain]).expect("session");
        let mut event = event_fixture(CoreCommand::ActiveGroup);
        event.domain_key = session.domains().first().expect("domain").key();
        event.domain = session.domains().first().expect("domain").clone();
        match resolve_active_group(Some(&session), &event) {
            ActiveGroupResolution::NoGroup { reason, .. } => {
                assert!(matches!(
                    reason,
                    NoGroupReason::FocusMismatch | NoGroupReason::NoTree
                ));
            }
            ActiveGroupResolution::Found(_) => panic!("empty session must not resolve"),
        }
    }

    #[test]
    fn projection_kind_tokens_match_wire_detail() {
        assert_eq!(ProjectionKind::Reconcile.kind_str(), "reconcile");
        assert_eq!(
            ProjectionKind::Reconcile.capability_str(),
            "reconcile-geometry"
        );
        assert_eq!(ProjectionKind::UpdateGaps.kind_str(), "update-gaps");
        assert_eq!(
            ProjectionKind::UpdateGaps.capability_str(),
            "update-gaps-geometry"
        );
    }

    #[test]
    fn projection_helper_rejects_missing_tree() {
        let (owner, generation, _) = ids();
        let (domain, key) = domain_fixture();
        let session = Session::new(owner, generation, 0, 0, vec![domain]).expect("session");
        assert!(
            project_retained_tiled_geometry(
                &session,
                &key,
                Rect {
                    x: 0,
                    y: 0,
                    w: 1200,
                    h: 800,
                },
                0,
                None,
                ProjectionKind::Reconcile,
                &BTreeMap::new(),
                &[],
            )
            .is_none()
        );
    }

    fn committed_pair_session() -> (Session, DomainKey, crate::directional::NodeId, SessionPlan) {
        use crate::directional::{Axis, NodeId};
        let (owner, generation, _) = ids();
        let (domain, _) = domain_fixture();
        let mut session = Session::new(owner, generation, 0, 0, vec![domain]).expect("session");
        let key = session.domains().first().expect("domain").key();
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Horizontal,
            children: vec![
                Node::Leaf {
                    id: NodeId::from("a"),
                },
                Node::Leaf {
                    id: NodeId::from("b"),
                },
            ],
            shares: vec![1, 1],
        };
        let links = vec![
            crate::directional::WindowLink {
                window: WindowId("win-a".to_owned()),
                leaf: NodeId::from("a"),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
            },
            crate::directional::WindowLink {
                window: WindowId("win-b".to_owned()),
                leaf: NodeId::from("b"),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
            },
        ];
        let observation = crate::session::SessionObservation {
            observation: crate::contract::Observation::new(
                session.owner().clone(),
                session.generation().clone(),
                0,
                0,
            ),
            windows: ["win-a", "win-b"]
                .iter()
                .map(|name| crate::session::ObservedWindow {
                    window: WindowId((*name).to_owned()),
                    output: key.output.clone(),
                    workspace: key.workspace.clone(),
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                })
                .collect(),
        };
        let correlation = CorrelationId::parse("corr-2").expect("valid");
        let plan = session
            .propose_fitted_admit(
                tree,
                links,
                NodeId::from("a"),
                &WindowId("win-a".to_owned()),
                &key.output,
                &key.workspace,
                &observation,
                &correlation,
                &crate::contract::LifecycleCapabilities::full(),
            )
            .expect("fit");
        let base = plan.dispatch.base_revision;
        let ack = crate::contract::AdapterAck::new(
            correlation.clone(),
            session.owner().clone(),
            session.generation().clone(),
            base,
            crate::contract::AckOutcome::Accepted,
        );
        session.acknowledge(&ack).expect("ack");
        let post = crate::contract::LifecyclePostObservation::new(
            crate::contract::Observation::new(
                session.owner().clone(),
                session.generation().clone(),
                base,
                0,
            ),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_lifecycle(&post).expect("commit");
        let (focus_domain, focus_leaf) = session.focus();
        assert_eq!(focus_domain, Some(key.clone()));
        (session, key, focus_leaf.expect("focus"), plan)
    }

    #[test]
    fn grouped_session_resolves_typed_found() {
        let (session, key, focus_leaf, _) = committed_pair_session();
        let mut event = event_fixture(CoreCommand::ActiveGroup);
        event.domain_key = key;
        event.domain = session.domains().first().expect("domain").clone();
        event.focused_window = session
            .snapshot()
            .windows
            .iter()
            .find(|l| l.leaf == focus_leaf)
            .expect("mapped")
            .window
            .clone();
        match resolve_active_group(Some(&session), &event) {
            ActiveGroupResolution::Found(found) => {
                assert_eq!(found.members.len(), 2);
                assert_eq!(found.focused_leaf, focus_leaf);
            }
            ActiveGroupResolution::NoGroup { reason, .. } => {
                panic!("expected found, got {}", reason.as_str())
            }
        }
    }

    /// Commit a three-window nested session with explicit shares and hinted
    /// admission, returning the session, domain key, focus leaf, and the
    /// ordinary-plan hints map. The observation carries the minimums, so the
    /// committed plan is hint-enforced like any live layout.
    fn committed_hinted_session(
        tree: Node,
        triples: &[(&str, &str, i32, i32)],
        focus_leaf: NodeId,
    ) -> (
        Session,
        DomainKey,
        NodeId,
        BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
    ) {
        let (owner, generation, _) = ids();
        let domain = OutputDomain {
            id: OutputId("o1".to_owned()),
            workspace: WorkspaceId("ws1".to_owned()),
            bounds: Rect {
                x: 8,
                y: 8,
                w: 2544,
                h: 1364,
            },
            gap: 8,
            adjacent: BTreeMap::new(),
        };
        let mut session = Session::new(owner, generation, 0, 0, vec![domain]).expect("session");
        let key = session.domains().first().expect("domain").key();
        let links: Vec<crate::directional::WindowLink> = triples
            .iter()
            .map(|(leaf, window, _, _)| crate::directional::WindowLink {
                window: WindowId((*window).to_owned()),
                leaf: NodeId::from(*leaf),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
            })
            .collect();
        let hints_map: BTreeMap<WindowId, crate::size_hints::WindowSizeHints> = triples
            .iter()
            .map(|(_, window, min_w, min_h)| {
                (
                    WindowId((*window).to_owned()),
                    crate::size_hints::WindowSizeHints {
                        min_w: Some(*min_w),
                        min_h: Some(*min_h),
                        max_w: None,
                        max_h: None,
                    },
                )
            })
            .collect();
        let observation = crate::session::SessionObservation {
            observation: crate::contract::Observation::new(
                session.owner().clone(),
                session.generation().clone(),
                0,
                0,
            ),
            windows: triples
                .iter()
                .map(|(_, window, _, _)| crate::session::ObservedWindow {
                    window: WindowId((*window).to_owned()),
                    output: key.output.clone(),
                    workspace: key.workspace.clone(),
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: hints_map[&WindowId((*window).to_owned())],
                })
                .collect(),
        };
        let admit_window = triples
            .iter()
            .find(|(leaf, _, _, _)| NodeId::from(*leaf) == focus_leaf)
            .map(|(_, window, _, _)| WindowId((*window).to_owned()))
            .expect("focus leaf mapped");
        let correlation = CorrelationId::parse("corr-hints").expect("valid");
        let plan = session
            .propose_fitted_admit(
                tree,
                links,
                focus_leaf.clone(),
                &admit_window,
                &key.output,
                &key.workspace,
                &observation,
                &correlation,
                &crate::contract::LifecycleCapabilities::full(),
            )
            .expect("fit");
        let base = plan.dispatch.base_revision;
        let ack = crate::contract::AdapterAck::new(
            correlation.clone(),
            session.owner().clone(),
            session.generation().clone(),
            base,
            crate::contract::AckOutcome::Accepted,
        );
        session.acknowledge(&ack).expect("ack");
        let post = crate::contract::LifecyclePostObservation::new(
            crate::contract::Observation::new(
                session.owner().clone(),
                session.generation().clone(),
                base,
                0,
            ),
            correlation,
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_lifecycle(&post).expect("commit");
        let (focus_domain, focus) = session.focus();
        assert_eq!(focus_domain, Some(key.clone()));
        assert_eq!(focus, Some(focus_leaf.clone()));
        (session, key, focus_leaf, hints_map)
    }

    /// Hints-only ActiveGroup query entries: opaque window identity plus the
    /// retained advisory hints with an empty rectangle. The resolver reads
    /// window and hints alone; carried geometry never exists here.
    fn hint_carriers(
        key: &DomainKey,
        hints_map: &BTreeMap<WindowId, crate::size_hints::WindowSizeHints>,
    ) -> Vec<EngineWindow> {
        hints_map
            .iter()
            .map(|(window, hints)| EngineWindow {
                window: window.clone(),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: *hints,
            })
            .collect()
    }

    #[test]
    fn active_group_matches_hinted_plan_union_tick136() {
        use crate::directional::Axis;
        // Live tick-136 shape: nested vertical stack whose retained shares
        // under-allocate the focused pair until minimums enforce.
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Vertical,
            children: vec![
                Node::Leaf {
                    id: NodeId::from("w25-leaf"),
                },
                Node::Group {
                    id: NodeId::from("inner"),
                    axis: Axis::Vertical,
                    children: vec![
                        Node::Leaf {
                            id: NodeId::from("w24-leaf"),
                        },
                        Node::Leaf {
                            id: NodeId::from("w23-leaf"),
                        },
                    ],
                    shares: vec![461, 117],
                },
            ],
            shares: vec![765, 583],
        };
        let (session, key, focus_leaf, hints_map) = committed_hinted_session(
            tree,
            &[
                ("w25-leaf", "w25", 401, 246),
                ("w24-leaf", "w24", 1263, 750),
                ("w23-leaf", "w23", 582, 118),
            ],
            NodeId::from("w24-leaf"),
        );
        let retained = session.domains().first().expect("domain").clone();
        // Ordinary-plan oracle: the Reconcile projection over the same hints.
        let plan = project_retained_tiled_geometry(
            &session,
            &key,
            retained.bounds,
            retained.gap,
            Some((key.clone(), focus_leaf.clone())),
            ProjectionKind::Reconcile,
            &hints_map,
            &[],
        )
        .expect("projects");
        let rect_of = |window: &str| {
            plan.geometry
                .iter()
                .find(|geometry| geometry.window == WindowId(window.to_owned()))
                .expect("planned")
                .rect
        };
        assert_eq!(
            rect_of("w25"),
            Rect {
                x: 8,
                y: 8,
                w: 2544,
                h: 480
            }
        );
        assert_eq!(
            rect_of("w24"),
            Rect {
                x: 8,
                y: 496,
                w: 2544,
                h: 750
            }
        );
        assert_eq!(
            rect_of("w23"),
            Rect {
                x: 8,
                y: 1254,
                w: 2544,
                h: 118
            }
        );
        // ActiveGroup with the same carried hints resolves the hint-correct
        // pair union, equal to the ordinary plan's pair union.
        let mut event = event_fixture(CoreCommand::ActiveGroup);
        event.domain_key = key.clone();
        event.domain = retained;
        event.focused_window = WindowId("w24".to_owned());
        event.windows = hint_carriers(&key, &hints_map);
        match resolve_active_group(Some(&session), &event) {
            ActiveGroupResolution::Found(found) => {
                assert_eq!(found.members.len(), 2);
                assert_eq!(
                    found.bounds,
                    Rect {
                        x: 8,
                        y: 496,
                        w: 2544,
                        h: 876
                    },
                    "hint-correct pair union matches the displayed layout"
                );
            }
            ActiveGroupResolution::NoGroup { reason, .. } => {
                panic!("expected found, got {}", reason.as_str())
            }
        }
        // Without carried hints the legacy hint-free union is preserved.
        let mut bare = event_fixture(CoreCommand::ActiveGroup);
        bare.domain_key = key.clone();
        bare.domain = session.domains().first().expect("domain").clone();
        bare.focused_window = WindowId("w24".to_owned());
        match resolve_active_group(Some(&session), &bare) {
            ActiveGroupResolution::Found(found) => {
                assert_eq!(found.members.len(), 2);
                assert_eq!(
                    found.bounds,
                    Rect {
                        x: 8,
                        y: 785,
                        w: 2544,
                        h: 587
                    },
                    "hintless query keeps the legacy union"
                );
            }
            ActiveGroupResolution::NoGroup { reason, .. } => {
                panic!("expected found, got {}", reason.as_str())
            }
        }
    }

    #[test]
    fn active_group_matches_hinted_plan_union_nested_subgroup() {
        use crate::directional::Axis;
        // Reported nested shape V[W1 H[W2 W3]]: the H rectangle itself sits
        // lower without hints because the outer split ignores its minimum.
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Vertical,
            children: vec![
                Node::Leaf {
                    id: NodeId::from("w1-leaf"),
                },
                Node::Group {
                    id: NodeId::from("inner"),
                    axis: Axis::Horizontal,
                    children: vec![
                        Node::Leaf {
                            id: NodeId::from("w2-leaf"),
                        },
                        Node::Leaf {
                            id: NodeId::from("w3-leaf"),
                        },
                    ],
                    shares: vec![1268, 1268],
                },
            ],
            shares: vec![800, 548],
        };
        let (session, key, focus_leaf, hints_map) = committed_hinted_session(
            tree,
            &[
                ("w1-leaf", "w1", 401, 246),
                ("w2-leaf", "w2", 1263, 750),
                ("w3-leaf", "w3", 582, 118),
            ],
            NodeId::from("w2-leaf"),
        );
        let retained = session.domains().first().expect("domain").clone();
        let plan = project_retained_tiled_geometry(
            &session,
            &key,
            retained.bounds,
            retained.gap,
            Some((key.clone(), focus_leaf.clone())),
            ProjectionKind::Reconcile,
            &hints_map,
            &[],
        )
        .expect("projects");
        let rect_of = |window: &str| {
            plan.geometry
                .iter()
                .find(|geometry| geometry.window == WindowId(window.to_owned()))
                .expect("planned")
                .rect
        };
        assert_eq!(
            rect_of("w2"),
            Rect {
                x: 8,
                y: 622,
                w: 1268,
                h: 750
            }
        );
        assert_eq!(
            rect_of("w3"),
            Rect {
                x: 1284,
                y: 622,
                w: 1268,
                h: 750
            }
        );
        let mut event = event_fixture(CoreCommand::ActiveGroup);
        event.domain_key = key.clone();
        event.domain = retained;
        event.focused_window = WindowId("w2".to_owned());
        event.windows = hint_carriers(&key, &hints_map);
        match resolve_active_group(Some(&session), &event) {
            ActiveGroupResolution::Found(found) => {
                assert_eq!(found.group, NodeId::from("inner"));
                assert_eq!(found.members.len(), 2);
                assert_eq!(
                    found.bounds,
                    Rect {
                        x: 8,
                        y: 622,
                        w: 2544,
                        h: 750
                    },
                    "hint-correct H union matches the displayed subgroup"
                );
            }
            ActiveGroupResolution::NoGroup { reason, .. } => {
                panic!("expected found, got {}", reason.as_str())
            }
        }
    }

    #[test]
    fn projection_helper_projects_committed_pair_in_wire_order() {
        let (session, key, focus_leaf, _) = committed_pair_session();
        let retained = session.domains().first().expect("domain").clone();
        let plan = project_retained_tiled_geometry(
            &session,
            &key,
            retained.bounds,
            retained.gap,
            Some((key.clone(), focus_leaf.clone())),
            ProjectionKind::Reconcile,
            &BTreeMap::new(),
            &[],
        )
        .expect("projects");
        assert_eq!(plan.base_revision, session.accepted_revision());
        assert_eq!(plan.kind, ProjectionKind::Reconcile);
        assert_eq!(plan.geometry.len(), 2);
        assert_eq!(
            plan.focus_domain.as_ref(),
            Some(&key),
            "focus passes through untouched"
        );
        assert_eq!(plan.focus_leaf.as_ref(), Some(&focus_leaf));
        let leaves: Vec<&str> = plan.geometry.iter().map(|g| g.leaf.0.as_str()).collect();
        assert_eq!(
            leaves,
            vec!["a", "b"],
            "wire (output, workspace, leaf) order"
        );
        for entry in &plan.geometry {
            assert!(entry.rect.w > 0 && entry.rect.h > 0);
            assert_eq!(entry.output, key.output);
            assert_eq!(entry.workspace, key.workspace);
        }
        // Update-gaps kind shares the helper; tokens differ only in detail.
        let gaps = project_retained_tiled_geometry(
            &session,
            &key,
            retained.bounds,
            retained.gap,
            Some((key.clone(), focus_leaf.clone())),
            ProjectionKind::UpdateGaps,
            &BTreeMap::new(),
            &[],
        )
        .expect("projects");
        assert_eq!(gaps.kind, ProjectionKind::UpdateGaps);
        assert_eq!(gaps.geometry, plan.geometry);
    }

    #[test]
    fn tiled_capability_tokens_match_wire_detail() {
        assert_eq!(TiledKind::Admit.capability_str(), Some("admit-tiled"));
        assert_eq!(TiledKind::Focus.capability_str(), Some("directional-focus"));
        assert_eq!(TiledKind::Resize.capability_str(), Some("keyboard-resize"));
        assert_eq!(
            TiledKind::PointerResize.capability_str(),
            Some("pointer-resize")
        );
        assert_eq!(
            TiledKind::ToggleFloat.capability_str(),
            Some("intentional-float")
        );
        assert_eq!(
            TiledKind::SendToWorkspace.capability_str(),
            Some("move-tiled")
        );
        assert_eq!(
            TiledKind::ToggleOrientation.capability_str(),
            Some("toggle-orientation")
        );
        assert_eq!(TiledKind::Move.capability_str(), None);
        assert_eq!(TiledKind::DirectionalMove.capability_str(), None);
        assert_eq!(TiledKind::Reconcile.capability_str(), None);
        assert_eq!(TiledKind::UpdateGaps.capability_str(), None);
    }

    #[test]
    fn tiled_plan_from_lifecycle_carries_session_fields() {
        let (session, key, focus_leaf, plan) = committed_pair_session();
        let tiled = TiledPlan::from_lifecycle(TiledKind::Admit, &plan);
        assert_eq!(tiled.base_revision, plan.dispatch.base_revision);
        assert_eq!(tiled.policy_version, plan.dispatch.policy_version);
        assert_eq!(tiled.kind, TiledKind::Admit);
        assert_eq!(tiled.geometry, plan.desired_geometry);
        assert_eq!(tiled.focus_domain, plan.desired_focus_domain);
        assert_eq!(tiled.focus_leaf, plan.desired_focus_leaf);
        assert_eq!(tiled.float_window, None);
        assert_eq!(tiled.float_rect, None);
        assert_eq!(
            session.accepted_revision(),
            plan.dispatch.base_revision + 1,
            "commit advances the revision by exactly one"
        );
        assert_eq!(key, plan.desired_focus_domain.expect("focus domain"));
        assert_eq!(focus_leaf, plan.desired_focus_leaf.expect("focus leaf"));
    }

    fn move_plan_fixture(
        operation: crate::directional::MoveOperation,
    ) -> (SessionMovePlan, DomainKey) {
        use crate::directional::{Capability, MoveIntent, Rule};
        let (owner, generation, correlation) = ids();
        let (_, key) = domain_fixture();
        let plan = SessionMovePlan {
            dispatch: crate::contract::Dispatch {
                correlation_id: correlation,
                owner,
                generation,
                base_revision: 7,
                required_capability: Capability::SwapNeighbor,
                preconditions: Vec::new(),
                rule: Rule::R2a,
                operation,
                intent: MoveIntent {
                    source_output: key.output.clone(),
                    focused_leaf: NodeId::from("leaf-1"),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                    same_axis_move: SameAxisMove::GroupWithNeighbor,
                },
            },
            desired_snapshot: crate::session::SessionSnapshot {
                domains: Vec::new(),
                windows: Vec::new(),
            },
            desired_focus_domain: key.clone(),
            desired_focus_leaf: NodeId::from("leaf-1"),
            desired_geometry: Vec::new(),
        };
        (plan, key)
    }

    #[test]
    fn move_local_reply_carries_rule_capability_direction() {
        use crate::directional::{MoveOperation, NodeId, Rule};
        let (plan, key) = move_plan_fixture(MoveOperation::SwapNeighbor {
            rule: Rule::R2a,
            container: NodeId::from("root"),
            neighbor: NodeId::from("n"),
        });
        let reply = MovePlanReply::from_local(Direction::Right, &plan);
        assert_eq!(reply.base_revision, 7);
        assert_eq!(reply.rule, Rule::R2a);
        assert_eq!(
            reply.capability,
            crate::directional::Capability::SwapNeighbor
        );
        assert_eq!(reply.direction, Direction::Right);
        assert_eq!(reply.focus_domain, key);
        assert_eq!(reply.cross, None);
    }

    #[test]
    fn move_cross_view_builds_only_for_cross_output() {
        use crate::directional::{CrossOutputTarget, MoveOperation, Rule};
        let (local, _) = move_plan_fixture(MoveOperation::SwapNeighbor {
            rule: Rule::R2a,
            container: NodeId::from("root"),
            neighbor: NodeId::from("n"),
        });
        assert!(
            MovePlanReply::from_cross(
                &local,
                &OutputId("out".to_owned()),
                &WorkspaceId("ws".to_owned()),
            )
            .is_none()
        );
        let (plan, _) = move_plan_fixture(MoveOperation::CrossOutput {
            rule: Rule::R4,
            target_output: OutputId("out-2".to_owned()),
            target_workspace: WorkspaceId("ws-1".to_owned()),
            source_root_child_index: 1,
            target: CrossOutputTarget::Occupied,
        });
        let reply = MovePlanReply::from_cross(
            &plan,
            &OutputId("out".to_owned()),
            &WorkspaceId("ws".to_owned()),
        )
        .expect("cross builds");
        assert_eq!(reply.direction, Direction::Right);
        let cross = reply.cross.expect("cross view");
        assert_eq!(cross.rule, Rule::R4);
        assert_eq!(cross.target_output.0, "out-2");
        assert_eq!(cross.source_output.0, "out");
        assert_eq!(cross.source_root_child_index, 1);
        assert_eq!(cross.target, CrossOutputTarget::Occupied);
        assert_eq!(cross.intent_window.0, "win-1");
    }

    fn focus_plan_fixture() -> (SessionFocusPlan, DomainKey) {
        use crate::contract::{FocusCapability, FocusDispatch, FocusIntent};
        use crate::directional::FocusPlan;
        let (owner, generation, correlation) = ids();
        let (domain, key) = domain_fixture();
        let operation = FocusOperation {
            domain_output: key.output.clone(),
            domain_workspace: key.workspace.clone(),
            from_leaf: Some(NodeId::from("a")),
            to_leaf: NodeId::from("b"),
            from_window: WindowId("win-1".to_owned()),
            to_window: WindowId("win-2".to_owned()),
            direction: Direction::Right,
            route: vec![NodeId::from("a"), NodeId::from("b")],
            cross_source_output: None,
            cross_source_workspace: None,
        };
        let plan = SessionFocusPlan {
            dispatch: FocusDispatch {
                correlation_id: correlation,
                owner,
                generation,
                base_revision: 3,
                required_capability: FocusCapability::DirectionalFocus,
                preconditions: Vec::new(),
                intent: FocusIntent {
                    domain_output: key.output.clone(),
                    domain_workspace: key.workspace.clone(),
                    focused_leaf: Some(NodeId::from("a")),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                },
                operation,
            },
            focus_plan: FocusPlan::Focused {
                leaf: NodeId::from("b"),
                route: vec![NodeId::from("a"), NodeId::from("b")],
            },
            desired_snapshot: crate::session::SessionSnapshot {
                domains: Vec::new(),
                windows: Vec::new(),
            },
            desired_focus_domain: key.clone(),
            desired_focus_leaf: NodeId::from("b"),
            desired_geometry: Vec::new(),
        };
        let _ = domain;
        (plan, key)
    }

    #[test]
    fn focus_reply_marks_cross_only_for_crossed() {
        let (plan, key) = focus_plan_fixture();
        let local = FocusPlanReply::from_local(Direction::Right, &plan);
        assert_eq!(local.base_revision, 3);
        assert_eq!(local.direction, Direction::Right);
        assert_eq!(local.to_window.0, "win-2");
        assert_eq!(local.focus_domain, key);
        assert_eq!(local.cross_operation, None);
        let crossed = FocusPlanReply::from_cross(Direction::Right, &plan);
        let operation = crossed.cross_operation.expect("cross echo");
        assert_eq!(operation.to_window.0, "win-2");
        assert_eq!(operation.route.len(), 2);
    }

    fn resize_plan_fixture() -> SessionResizePlan {
        use crate::contract::{ResizeCapability, ResizeDispatch, ResizeIntent, ResizeOperation};
        let (owner, generation, correlation) = ids();
        let (_, key) = domain_fixture();
        SessionResizePlan {
            dispatch: ResizeDispatch {
                correlation_id: correlation,
                owner,
                generation,
                base_revision: 5,
                required_capability: ResizeCapability::KeyboardResize,
                preconditions: Vec::new(),
                intent: ResizeIntent {
                    domain_output: key.output.clone(),
                    domain_workspace: key.workspace.clone(),
                    focused_leaf: NodeId::from("a"),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                    mode: ResizeMode::Outwards,
                },
                operation: ResizeOperation {
                    domain_output: key.output.clone(),
                    domain_workspace: key.workspace.clone(),
                    focused_leaf: NodeId::from("a"),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                    mode: ResizeMode::Outwards,
                    target_group: NodeId::from("grp"),
                    focused_child: NodeId::from("a"),
                    neighbor_child: NodeId::from("b"),
                    focused_index: 0,
                    neighbor_index: 1,
                    old_shares: vec![1, 1],
                    new_shares: vec![611, 587],
                },
                secondary_operation: None,
            },
            resize_plan: crate::contract::ResizePlan::for_operation(
                crate::contract::ResizeIntent {
                    domain_output: key.output.clone(),
                    domain_workspace: key.workspace.clone(),
                    focused_leaf: NodeId::from("a"),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                    mode: ResizeMode::Outwards,
                },
                ResizeOperation {
                    domain_output: key.output.clone(),
                    domain_workspace: key.workspace.clone(),
                    focused_leaf: NodeId::from("a"),
                    focused_window: WindowId("win-1".to_owned()),
                    direction: Direction::Right,
                    mode: ResizeMode::Outwards,
                    target_group: NodeId::from("grp"),
                    focused_child: NodeId::from("a"),
                    neighbor_child: NodeId::from("b"),
                    focused_index: 0,
                    neighbor_index: 1,
                    old_shares: vec![1, 1],
                    new_shares: vec![611, 587],
                },
            ),
            desired_snapshot: crate::session::SessionSnapshot {
                domains: Vec::new(),
                windows: Vec::new(),
            },
            secondary_plan: None,
            desired_focus_domain: key.clone(),
            desired_focus_leaf: NodeId::from("a"),
            desired_geometry: Vec::new(),
        }
    }

    #[test]
    fn resize_reply_splits_keyboard_and_pointer() {
        let plan = resize_plan_fixture();
        let keyboard =
            ResizePlanReply::from_keyboard(Direction::Right, ResizeMode::Outwards, &plan);
        assert_eq!(keyboard.base_revision, 5);
        assert_eq!(keyboard.mode, Some(ResizeMode::Outwards));
        assert_eq!(keyboard.boundary, None);
        assert_eq!(keyboard.operation.target_group.0, "grp");
        assert_eq!(keyboard.operation.new_shares, vec![611, 587]);
        let pointer = ResizePlanReply::from_pointer(Direction::Left, 120, &plan);
        assert_eq!(pointer.mode, None);
        assert_eq!(pointer.boundary, Some(120));
        assert_eq!(pointer.direction, Direction::Left);
        assert_eq!(pointer.operation.target_group.0, "grp");
    }

    #[test]
    fn toggle_float_plan_carries_effective_rect_and_window() {
        let (_, _, _, plan) = committed_pair_session();
        let rect = Rect {
            x: 240,
            y: 160,
            w: 720,
            h: 480,
        };
        let floated = TiledPlan::for_toggle_float(&plan, Some(rect));
        assert_eq!(floated.kind, TiledKind::ToggleFloat);
        assert_eq!(floated.float_rect, Some(rect));
        // The fitted-admit operation is not `Remove`, so the legacy empty
        // window branch applies exactly.
        assert_eq!(
            floated.float_window.as_ref().map(|w| w.0.as_str()),
            Some("")
        );
        let plain = TiledPlan::for_toggle_float(&plan, None);
        assert_eq!(plain.float_window, None);
        assert_eq!(plain.float_rect, None);
    }

    #[test]
    fn send_workspace_plan_requires_move_intent_with_explicit_follow() {
        let (_, _, _, plan) = committed_pair_session();
        // The admit fixture never proposes a workspace move.
        assert!(SendWorkspacePlan::from_session(&plan).is_none());
        let key = plan.desired_focus_domain.clone().expect("focus domain");
        let mut moved = plan.clone();
        moved.dispatch.operation = LifecycleOperation::MoveTiled {
            window: WindowId("win-a".to_owned()),
            leaf: crate::directional::NodeId::from("a"),
            source_output: key.output.clone(),
            source_workspace: key.workspace.clone(),
            target_output: key.output.clone(),
            target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
        };
        // The operation alone is not enough: a MoveTiled operation under a
        // non-workspace intent is not a workspace send and refuses.
        assert!(SendWorkspacePlan::from_session(&moved).is_none());
        // The explicit workspace-move intent constructs, carrying its
        // follow/stay selection verbatim.
        for follow in [true, false] {
            let mut intentful = moved.clone();
            intentful.dispatch.intent = LifecycleIntent::MoveToWorkspace {
                window: WindowId("win-a".to_owned()),
                target_output: key.output.clone(),
                target_workspace: crate::directional::WorkspaceId("ws-2".to_owned()),
                follow,
            };
            let typed = SendWorkspacePlan::from_session(&intentful).expect("intent builds");
            assert_eq!(typed.follow, follow);
            assert_eq!(typed.base_revision, intentful.dispatch.base_revision);
            assert_eq!(typed.policy_version, intentful.dispatch.policy_version);
            assert_eq!(typed.geometry, intentful.desired_geometry);
            assert_eq!(typed.preconditions, intentful.dispatch.preconditions);
            assert!(matches!(
                typed.operation,
                LifecycleOperation::MoveTiled { .. }
            ));
        }
    }
}
