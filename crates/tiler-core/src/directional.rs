//! Pure, platform-neutral directional movement engine.
//!
//! Structural analogue of the authoritative TypeScript sources:
//! - `directional-movement-planner.ts` (rules R1-R4, validation,
//!   planning algorithm),
//! - `cosmic-move-adapter.ts` (tree-relative focus climb/descend,
//!   proportional fraction sizing helpers only).
//!
//! Boundary rules: no platform imports, concepts, or identity; no native
//! actuation, mutation, batching, recovery, IPC, persistence, UI, FFI, or
//! platform adapters. The engine takes an immutable [`Snapshot`] by shared
//! reference, emits a deterministic owned [`MoveOutcome`], and holds no state.
//! Native realization is never assumed atomic: every emitted plan carries its
//! [`Precondition`]s (including [`Precondition::AdapterMustVerifyPostconditions`])
//! and its [`Capability`], and [`plan_move_with_capabilities`] refuses an
//! unsupported operation before any plan is emitted.
//!
//! Intentional omissions (per bounded brief): no actuation/mutation layer, no
//! persistence/IPC/FFI. Pixel placement lives in [`crate::geometry`], a
//! deterministic portable projector over the [`Node`] topology defined here
//! (fraction sizing helpers need none). Cyclic graphs are impossible by
//! construction: [`Node`]
//! is an owned tree, so a cycle cannot be represented. The TypeScript
//! object-identity cycle/share rejection therefore maps to the strictly
//! stronger global duplicate-[`NodeId`] rejection documented on
//! [`Snapshot`]; no flat graph decoder is provided because cycle input cannot
//! otherwise be constructed.

use std::collections::{BTreeMap, HashMap, HashSet};

/// Movement direction. Analogue of `Direction` in the TypeScript logic source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Split axis of an N-ary group. Analogue of `CosmicAxis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// Axis a direction moves along. Source: `axisFor` in the TS planner.
    #[must_use]
    pub fn for_direction(direction: Direction) -> Self {
        match direction {
            Direction::Left | Direction::Right => Axis::Horizontal,
            Direction::Up | Direction::Down => Axis::Vertical,
        }
    }
}

/// Step along the axis. Source: `stepFor` in the TS planner.
fn step_for(direction: Direction) -> i32 {
    match direction {
        Direction::Right | Direction::Down => 1,
        Direction::Left | Direction::Up => -1,
    }
}

/// Opposite direction, for reciprocal adjacency checks.
fn opposite_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

/// Opaque platform-neutral output identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutputId(pub String);

/// Opaque platform-neutral workspace identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorkspaceId(pub String);

/// Opaque platform-neutral tree-node identity (leaves and groups share one
/// namespace, as in the TS source where `ids` covers both kinds).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub String);

/// Opaque platform-neutral window identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub String);

impl From<&str> for OutputId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<&str> for WorkspaceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<&str> for WindowId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Ordered N-ary tree node. Analogue of `CosmicNode` / `CosmicLeaf` / `CosmicGroup`.
/// A group splits its extent along `axis` across `children` proportionally to
/// `shares`: exactly one positive integer weight per child, in child order.
/// See [`crate::geometry`] for the deterministic portable projector over this
/// topology; the movement planner itself never interprets shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Leaf {
        id: NodeId,
    },
    Group {
        id: NodeId,
        axis: Axis,
        children: Vec<Node>,
        shares: Vec<u64>,
    },
}

impl Node {
    /// Node identity.
    #[must_use]
    pub fn id(&self) -> &NodeId {
        match self {
            Node::Leaf { id } | Node::Group { id, .. } => id,
        }
    }
}

/// Platform-neutral output topology. Analogue of `CosmicOutputTopology`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub id: OutputId,
    pub workspace: WorkspaceId,
    pub tree: Option<Node>,
    pub adjacent: BTreeMap<Direction, OutputId>,
}

/// Platform-neutral window association: window `window` occupies leaf `leaf`
/// on output `output` within `workspace`. There is no platform-native identity
/// here;
/// the adapter maps native handles to these opaque ids before calling the
/// engine. Every leaf must have exactly one link and every link must be
/// reciprocal (see [`Snapshot`] validation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowLink {
    pub window: WindowId,
    pub leaf: NodeId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
}

/// Immutable engine input snapshot.
///
/// Validation (fail-closed [`Rejection`], never partial) detects:
/// - empty output set; empty/duplicate output, workspace, node, or window ids;
/// - groups with fewer than two children;
/// - groups whose `shares` are not exactly one positive integer per child, or
///   whose share total overflows `u64`;
/// - duplicate [`NodeId`]s anywhere in the snapshot. This global uniqueness is
///   an intentional engine snapshot invariant, not a mechanical validation
///   translation: opaque global ids plus [`WindowLink`] require an unambiguous
///   logical topology, so any shared topology can only reappear as a
///   duplicated id, which is rejected. Cyclic graphs need no decoder because
///   they are impossible by construction with the owned [`Node`] tree;
/// - unknown, self, or empty output adjacency targets;
/// - missing source output;
/// - missing, stale (leaf/output/workspace mismatch), or nonreciprocal
///   (leaf without a link, link without a leaf, duplicated window or leaf)
///   window associations, including empty [`WindowLink`] workspace values
///   rejected before output workspace comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub outputs: Vec<Output>,
    pub windows: Vec<WindowLink>,
}

/// Directional move intent against a [`Snapshot`].
///
/// `same_axis_move` selects the R-MOV-03 same-orientation behavior for the
/// R2c neighbor case only: [`SameAxisMove::GroupWithNeighbor`] (default)
/// keeps the COSMIC nested wrap, [`SameAxisMove::SwapWithNeighbor`] swaps an
/// adjacent direct leaf sibling in place. R1, R2a, R2b, R3, and R4 are
/// identical under both values. The mode travels in the intent so strict
/// application can refuse forged operation/mode combinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveIntent {
    pub source_output: OutputId,
    pub focused_leaf: NodeId,
    pub focused_window: WindowId,
    pub direction: Direction,
    pub same_axis_move: SameAxisMove,
}

/// R-MOV-03 same-axis move setting (decisions 2026-10-07 item 3.1/3.2,
/// functional IDs 2026-10-08).
///
/// One global setting with two validated values: `group-with-neighbor`
/// (default, `H[A,B*,C,D]` move right gives `H[A,H[B,C],D]`) and
/// `swap-with-neighbor` (i3/sway alternative, `H[A,C,B*,D]` with shares
/// traveling with windows). The wire tokens are `group-with-neighbor` and
/// `swap-with-neighbor`; a missing protocol field decodes to the default.
/// Anything else refuses at the protocol boundary, never here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SameAxisMove {
    /// Group with neighbor (default, COSMIC nested wrap).
    #[default]
    GroupWithNeighbor,
    /// Swap with neighbor (R2c leaf neighbors only).
    SwapWithNeighbor,
}

impl SameAxisMove {
    /// Wire token for this value (`group-with-neighbor` / `swap-with-neighbor`).
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::GroupWithNeighbor => "group-with-neighbor",
            Self::SwapWithNeighbor => "swap-with-neighbor",
        }
    }

    /// Validated parse of a wire token. `None` for anything else, including
    /// empty strings: callers refuse fail-closed.
    #[must_use]
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "group-with-neighbor" => Some(Self::GroupWithNeighbor),
            "swap-with-neighbor" => Some(Self::SwapWithNeighbor),
            _ => None,
        }
    }
}

/// Structural rule. Analogue of `CosmicMoveRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    R1,
    R2a,
    R2b,
    R2c,
    R3,
    R4,
}

/// Adapter-facing native capability required to realize one operation.
/// `InsertChild`, `SplitGroupChild`, `WrapSiblings`, `ReparentLeaf`, and
/// `CrossOutputTransfer` model structural operations an adapter may not
/// declare; the engine refuses them before plan emission unless declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    SwapNeighbor,
    WrapPerpendicular,
    WrapSiblings,
    InsertChild,
    SplitGroupChild,
    ReparentLeaf,
    CrossOutputTransfer,
}

/// Explicit preconditions the adapter must hold/verify to realize a plan.
/// `AdapterMustVerifyPostconditions` is present on every plan: the engine
/// never assumes native realization is atomic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Precondition {
    FocusedLeafOccupiedByFocusedWindow,
    NeighborLeafOccupied,
    ContainerIsDirectParent,
    TargetGroupMembership,
    ParentGroupMembership,
    SourceRootMembershipAndAdjacentSameWorkspaceOutput,
    AdapterMustVerifyPostconditions,
}

/// Structural operation. Structural analogue of `CosmicMoveOperation`
/// (`containerId` rendered as `container` with Rust-idiomatic field names).
/// This POC promises no JSON wire compatibility with the TypeScript source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveOperation {
    WrapPerpendicular {
        rule: Rule,
        container: NodeId,
        axis: Axis,
    },
    SwapNeighbor {
        rule: Rule,
        container: NodeId,
        neighbor: NodeId,
    },
    InsertIntoGroup {
        rule: Rule,
        container: NodeId,
        target_group: NodeId,
        insertion_index: usize,
        insertion: Insertion,
    },
    SplitGroupChild {
        rule: Rule,
        container: NodeId,
        target_group: NodeId,
        target_child: NodeId,
        target_child_index: usize,
        focused_side: FocusedSide,
        axis: Axis,
    },
    WrapNeighbor {
        rule: Rule,
        container: NodeId,
        neighbor: NodeId,
        focused_before_neighbor: bool,
        axis: Axis,
    },
    EscapeParent {
        rule: Rule,
        container: NodeId,
        parent: NodeId,
        container_child_index: usize,
        parent_insertion_index: Option<usize>,
        continuation: EscapeContinuation,
    },
    CrossOutput {
        rule: Rule,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        source_root_child_index: usize,
        target: CrossOutputTarget,
    },
}

/// R2b insertion placement. Analogue of `insertion: "midpoint" | "near-edge"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Insertion {
    Midpoint,
    NearEdge,
}

/// R2b split placement of the focused leaf. Analogue of `focusedSide`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusedSide {
    First,
    Second,
}

/// R3 continuation. Analogue of `continuation: "none" | "R1"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EscapeContinuation {
    None,
    R1,
}

/// R4 target occupancy. Analogue of `target: "empty" | "occupied"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrossOutputTarget {
    Empty,
    Occupied,
}

impl MoveOperation {
    /// Rule of this operation.
    #[must_use]
    pub fn rule(&self) -> Rule {
        match self {
            MoveOperation::WrapPerpendicular { rule, .. }
            | MoveOperation::SwapNeighbor { rule, .. }
            | MoveOperation::InsertIntoGroup { rule, .. }
            | MoveOperation::SplitGroupChild { rule, .. }
            | MoveOperation::WrapNeighbor { rule, .. }
            | MoveOperation::EscapeParent { rule, .. }
            | MoveOperation::CrossOutput { rule, .. } => *rule,
        }
    }

    /// Adapter-facing capability required before emission.
    #[must_use]
    pub fn required_capability(&self) -> Capability {
        match self {
            MoveOperation::WrapPerpendicular { .. } => Capability::WrapPerpendicular,
            MoveOperation::SwapNeighbor { .. } => Capability::SwapNeighbor,
            MoveOperation::InsertIntoGroup { .. } => Capability::InsertChild,
            MoveOperation::SplitGroupChild { .. } => Capability::SplitGroupChild,
            MoveOperation::WrapNeighbor { .. } => Capability::WrapSiblings,
            MoveOperation::EscapeParent { .. } => Capability::ReparentLeaf,
            MoveOperation::CrossOutput { .. } => Capability::CrossOutputTransfer,
        }
    }

    /// Explicit preconditions for realization (always terminated by
    /// [`Precondition::AdapterMustVerifyPostconditions`]).
    #[must_use]
    pub fn preconditions(&self) -> Vec<Precondition> {
        let mut preconditions = match self {
            MoveOperation::WrapPerpendicular { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::ContainerIsDirectParent,
            ],
            MoveOperation::SwapNeighbor { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::NeighborLeafOccupied,
                Precondition::ContainerIsDirectParent,
            ],
            MoveOperation::InsertIntoGroup { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::ContainerIsDirectParent,
                Precondition::TargetGroupMembership,
            ],
            MoveOperation::SplitGroupChild { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::NeighborLeafOccupied,
                Precondition::ContainerIsDirectParent,
                Precondition::TargetGroupMembership,
            ],
            MoveOperation::WrapNeighbor { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::NeighborLeafOccupied,
                Precondition::ContainerIsDirectParent,
            ],
            MoveOperation::EscapeParent { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::ContainerIsDirectParent,
                Precondition::ParentGroupMembership,
            ],
            MoveOperation::CrossOutput { .. } => vec![
                Precondition::FocusedLeafOccupiedByFocusedWindow,
                Precondition::SourceRootMembershipAndAdjacentSameWorkspaceOutput,
            ],
        };
        preconditions.push(Precondition::AdapterMustVerifyPostconditions);
        preconditions
    }
}

/// Adapter-declared capabilities. See [`Capability`]. Adapters supply explicit
/// generic capabilities; the engine defines no platform-specific preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub swap_neighbor: bool,
    pub wrap_perpendicular: bool,
    pub wrap_siblings: bool,
    pub insert_child: bool,
    pub split_group_child: bool,
    pub reparent_leaf: bool,
    pub cross_output_transfer: bool,
}

impl Capabilities {
    /// All capabilities declared (tests, future adapters).
    #[must_use]
    pub fn full() -> Self {
        Self {
            swap_neighbor: true,
            wrap_perpendicular: true,
            wrap_siblings: true,
            insert_child: true,
            split_group_child: true,
            reparent_leaf: true,
            cross_output_transfer: true,
        }
    }

    /// None declared.
    #[must_use]
    pub fn none() -> Self {
        Self {
            swap_neighbor: false,
            wrap_perpendicular: false,
            wrap_siblings: false,
            insert_child: false,
            split_group_child: false,
            reparent_leaf: false,
            cross_output_transfer: false,
        }
    }

    /// Whether `capability` is declared.
    #[must_use]
    pub fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::SwapNeighbor => self.swap_neighbor,
            Capability::WrapPerpendicular => self.wrap_perpendicular,
            Capability::WrapSiblings => self.wrap_siblings,
            Capability::InsertChild => self.insert_child,
            Capability::SplitGroupChild => self.split_group_child,
            Capability::ReparentLeaf => self.reparent_leaf,
            Capability::CrossOutputTransfer => self.cross_output_transfer,
        }
    }
}

/// Deterministic movement plan with explicit capability and preconditions.
/// Self-contained: `intent` records the originating typed [`MoveIntent`] so a
/// plan can be interpreted without retaining the caller-side intent value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePlan {
    pub intent: MoveIntent,
    pub rule: Rule,
    pub operation: MoveOperation,
    pub required_capability: Capability,
    pub preconditions: Vec<Precondition>,
}

impl MovePlan {
    fn for_operation(intent: &MoveIntent, operation: MoveOperation) -> Self {
        let required_capability = operation.required_capability();
        let preconditions = operation.preconditions();
        Self {
            intent: (*intent).clone(),
            rule: operation.rule(),
            operation,
            required_capability,
            preconditions,
        }
    }
}

/// No-op reason. Analogue of `CosmicNoopReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoopReason {
    Boundary,
    NoAdjacentOutput,
    SingleRootLeaf,
}

/// Rejection kind. Analogue of `CosmicRejectionKind` plus `UnsupportedTopology`
/// for capability refusal before plan emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectionKind {
    MalformedTopology,
    UnsupportedTopology,
    FocusedLeafNotFound,
}

/// Fail-closed rejection with a diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub kind: RejectionKind,
    pub message: String,
}

/// Engine outcome. Analogue of `CosmicMoveOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveOutcome {
    Planned(MovePlan),
    Noop { reason: NoopReason },
    Rejected { reason: Rejection },
}

fn rejected(kind: RejectionKind, message: impl Into<String>) -> MoveOutcome {
    MoveOutcome::Rejected {
        reason: Rejection {
            kind,
            message: message.into(),
        },
    }
}

/// Deterministic tree-relative focus plan. Analogue of `planCosmicFocus` /
/// `CosmicFocusPlan` in `cosmic-move-adapter.ts`; `route` names the
/// deterministic group-to-leaf descent so callers can verify the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusPlan {
    Focused { leaf: NodeId, route: Vec<NodeId> },
    Edge,
}

// ---- validation ----

struct PathLevel<'a> {
    group_id: NodeId,
    axis: Axis,
    child_index: usize,
    children: &'a [Node],
}

fn find_path<'a>(node: &'a Node, focused: &NodeId, path: &mut Vec<PathLevel<'a>>) -> bool {
    match node {
        Node::Leaf { id } => id == focused,
        Node::Group {
            id, axis, children, ..
        } => {
            for (index, child) in children.iter().enumerate() {
                if find_path(child, focused, path) {
                    path.push(PathLevel {
                        group_id: id.clone(),
                        axis: *axis,
                        child_index: index,
                        children: children.as_slice(),
                    });
                    return true;
                }
            }
            false
        }
    }
}

/// Root-to-leaf path (outermost first). `None` when the leaf is absent.
fn path_to<'a>(node: &'a Node, focused: &NodeId) -> Option<Vec<PathLevel<'a>>> {
    let mut reversed = Vec::new();
    if find_path(node, focused, &mut reversed) {
        reversed.reverse();
        Some(reversed)
    } else {
        None
    }
}

fn validate_node(
    node: &Node,
    ids: &mut HashSet<NodeId>,
    leaf_outputs: &mut HashMap<NodeId, usize>,
    output_index: usize,
    leaf_is_leaf: &mut HashMap<NodeId, bool>,
) -> Result<(), String> {
    if node.id().0.is_empty() {
        return Err("node identities must be non-empty".to_string());
    }
    if !ids.insert(node.id().clone()) {
        return Err(
            "node identities must be unique; shared or cyclic topology is unsupported".to_string(),
        );
    }
    match node {
        Node::Leaf { id } => {
            leaf_outputs.insert(id.clone(), output_index);
            leaf_is_leaf.insert(id.clone(), true);
            Ok(())
        }
        Node::Group {
            children, shares, ..
        } => {
            leaf_is_leaf.insert(node.id().clone(), false);
            if children.len() < 2 {
                return Err("groups must have at least two children".to_string());
            }
            if shares.len() != children.len() {
                return Err("group shares must align exactly with children".to_string());
            }
            if shares.contains(&0) {
                return Err("group shares must be positive".to_string());
            }
            let mut total: u64 = 0;
            for share in shares {
                total = total
                    .checked_add(*share)
                    .ok_or_else(|| "group shares total overflows".to_string())?;
            }
            if total == 0 {
                return Err("group shares must be positive".to_string());
            }
            for child in children {
                validate_node(child, ids, leaf_outputs, output_index, leaf_is_leaf)?;
            }
            Ok(())
        }
    }
}

struct Validated<'a> {
    source: &'a Output,
}

#[allow(clippy::result_large_err)]
fn validate_snapshot<'a>(
    snapshot: &'a Snapshot,
    intent: &MoveIntent,
) -> Result<Validated<'a>, MoveOutcome> {
    if snapshot.outputs.is_empty() {
        return Err(rejected(
            RejectionKind::MalformedTopology,
            "at least one output topology is required",
        ));
    }
    if intent.source_output.0.is_empty()
        || intent.focused_leaf.0.is_empty()
        || intent.focused_window.0.is_empty()
    {
        return Err(rejected(
            RejectionKind::MalformedTopology,
            "directional movement intent is malformed",
        ));
    }
    let mut seen_outputs = HashSet::new();
    for output in &snapshot.outputs {
        if output.id.0.is_empty() || !seen_outputs.insert(output.id.clone()) {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "output identities must be non-empty and unique",
            ));
        }
        if output.workspace.0.is_empty() {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "output scope must be present",
            ));
        }
    }
    let by_id: HashMap<&OutputId, &Output> = snapshot
        .outputs
        .iter()
        .map(|output| (&output.id, output))
        .collect();
    for output in &snapshot.outputs {
        for target in output.adjacent.values() {
            if target.0.is_empty() || target == &output.id || !by_id.contains_key(target) {
                return Err(rejected(
                    RejectionKind::MalformedTopology,
                    "output adjacency references an unknown, empty, or self output",
                ));
            }
        }
    }
    let Some(source) = by_id.get(&intent.source_output) else {
        return Err(rejected(
            RejectionKind::MalformedTopology,
            "source output is not present",
        ));
    };
    let source_index = snapshot
        .outputs
        .iter()
        .position(|output| output.id == intent.source_output)
        .unwrap_or(0);

    let mut ids = HashSet::new();
    let mut leaf_output = HashMap::new();
    let mut leaf_is_leaf = HashMap::new();
    for (index, output) in snapshot.outputs.iter().enumerate() {
        if let Some(tree) = &output.tree
            && let Err(message) =
                validate_node(tree, &mut ids, &mut leaf_output, index, &mut leaf_is_leaf)
        {
            return Err(rejected(RejectionKind::MalformedTopology, message));
        }
    }

    // Window associations: unique windows/leaves, reciprocal leaf links scoped
    // to the link output and workspace.
    let mut seen_windows = HashSet::new();
    let mut linked_leaves = HashSet::new();
    for link in &snapshot.windows {
        if link.window.0.is_empty() || link.leaf.0.is_empty() || link.output.0.is_empty() {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window associations must be non-empty",
            ));
        }
        if link.workspace.0.is_empty() {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window association workspace must be non-empty",
            ));
        }
        if !seen_windows.insert(link.window.clone()) {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window associations must reference each window at most once",
            ));
        }
        if !linked_leaves.insert(link.leaf.clone()) {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window associations must reference each leaf at most once",
            ));
        }
        let Some(link_output_index) = snapshot
            .outputs
            .iter()
            .position(|output| output.id == link.output)
        else {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window association references an unknown output",
            ));
        };
        match leaf_output.get(&link.leaf) {
            Some(actual) if *actual == link_output_index => {}
            _ => {
                return Err(rejected(
                    RejectionKind::MalformedTopology,
                    "window association is stale or nonreciprocal: leaf link mismatch",
                ));
            }
        }
        if leaf_is_leaf.get(&link.leaf) != Some(&true) {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window association targets a non-leaf node",
            ));
        }
        if snapshot.outputs[link_output_index].workspace != link.workspace {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "window association workspace does not match the output scope",
            ));
        }
    }
    for leaf in leaf_output.keys() {
        if leaf_is_leaf.get(leaf) == Some(&true) && !linked_leaves.contains(leaf) {
            return Err(rejected(
                RejectionKind::MalformedTopology,
                "every leaf requires exactly one window association",
            ));
        }
    }
    if !snapshot
        .windows
        .iter()
        .any(|link| link.window == intent.focused_window && link.leaf == intent.focused_leaf)
    {
        // Distinguish a focused leaf that does not exist at all (TS parity:
        // `focused-leaf-not-found`) from a stale/nonreciprocal association.
        if leaf_output.get(&intent.focused_leaf) != Some(&source_index) {
            return Err(rejected(
                RejectionKind::FocusedLeafNotFound,
                "focused leaf is not present in the source topology",
            ));
        }
        return Err(rejected(
            RejectionKind::MalformedTopology,
            "focused window association is missing, stale, or nonreciprocal",
        ));
    }

    Ok(Validated { source })
}

// ---- movement planning (mechanical translation of planLocalMove) ----

fn plan_local(intent: &MoveIntent, source: &Output, path: &[PathLevel<'_>]) -> MoveOutcome {
    if path.is_empty() {
        return MoveOutcome::Noop {
            reason: NoopReason::SingleRootLeaf,
        };
    }
    let direction_axis = Axis::for_direction(intent.direction);
    let step = step_for(intent.direction);
    // Mechanical note: the TS source loops from the deepest level outward,
    // but every branch returns on the first (deepest) iteration, so the loop
    // never advances. The engine resolves the deepest level directly with
    // identical semantics.
    let level = path.len() - 1;
    let ancestor = &path[level];
    if ancestor.axis != direction_axis {
        return MoveOutcome::Planned(MovePlan::for_operation(
            intent,
            MoveOperation::WrapPerpendicular {
                rule: Rule::R1,
                container: ancestor.group_id.clone(),
                axis: direction_axis,
            },
        ));
    }
    let neighbor_index = ancestor.child_index as i32 + step;
    let neighbor = if neighbor_index >= 0 {
        ancestor.children.get(neighbor_index as usize)
    } else {
        None
    };
    if let Some(neighbor) = neighbor {
        if ancestor.children.len() == 2 {
            match neighbor {
                Node::Leaf { id } => {
                    return MoveOutcome::Planned(MovePlan::for_operation(
                        intent,
                        MoveOperation::SwapNeighbor {
                            rule: Rule::R2a,
                            container: ancestor.group_id.clone(),
                            neighbor: id.clone(),
                        },
                    ));
                }
                Node::Group {
                    id, axis, children, ..
                } => {
                    if *axis == ancestor.axis {
                        let insertion_index = if ancestor.child_index < neighbor_index as usize {
                            0
                        } else {
                            children.len()
                        };
                        return MoveOutcome::Planned(MovePlan::for_operation(
                            intent,
                            MoveOperation::InsertIntoGroup {
                                rule: Rule::R2b,
                                container: ancestor.group_id.clone(),
                                target_group: id.clone(),
                                insertion_index,
                                insertion: Insertion::NearEdge,
                            },
                        ));
                    }
                    let target_child_index = children.len() / 2;
                    if children.len() % 2 == 0 {
                        return MoveOutcome::Planned(MovePlan::for_operation(
                            intent,
                            MoveOperation::InsertIntoGroup {
                                rule: Rule::R2b,
                                container: ancestor.group_id.clone(),
                                target_group: id.clone(),
                                insertion_index: target_child_index,
                                insertion: Insertion::Midpoint,
                            },
                        ));
                    }
                    let Some(target_child) = children.get(target_child_index) else {
                        return MoveOutcome::Noop {
                            reason: NoopReason::Boundary,
                        };
                    };
                    return MoveOutcome::Planned(MovePlan::for_operation(
                        intent,
                        MoveOperation::SplitGroupChild {
                            rule: Rule::R2b,
                            container: ancestor.group_id.clone(),
                            target_group: id.clone(),
                            target_child: target_child.id().clone(),
                            target_child_index,
                            focused_side: if step == -1 {
                                FocusedSide::First
                            } else {
                                FocusedSide::Second
                            },
                            axis: direction_axis,
                        },
                    ));
                }
            }
        }
        // R-MOV-03 swap-with-neighbor (item 3.2): replaces only this R2c case, and
        // only when the directional neighbor is an adjacent direct leaf
        // sibling in the same group. Group neighbors, R2a binary swaps, R2b
        // inserts/splits, and R3 escapes keep the wrap-branch rules below.
        // The emitted swap reuses SwapNeighbor (shares travel with windows at
        // application, like R2a); the rule stays R2c so strict application
        // can tell the N-ary swap from the binary R2a swap.
        if intent.same_axis_move == SameAxisMove::SwapWithNeighbor
            && matches!(neighbor, Node::Leaf { .. })
            && matches!(
                ancestor.children.get(ancestor.child_index),
                Some(Node::Leaf { .. })
            )
        {
            return MoveOutcome::Planned(MovePlan::for_operation(
                intent,
                MoveOperation::SwapNeighbor {
                    rule: Rule::R2c,
                    container: ancestor.group_id.clone(),
                    neighbor: neighbor.id().clone(),
                },
            ));
        }
        return MoveOutcome::Planned(MovePlan::for_operation(
            intent,
            MoveOperation::WrapNeighbor {
                rule: Rule::R2c,
                container: ancestor.group_id.clone(),
                neighbor: neighbor.id().clone(),
                focused_before_neighbor: ancestor.child_index < neighbor_index as usize,
                axis: ancestor.axis,
            },
        ));
    }
    if level == 0 {
        if !source.adjacent.contains_key(&intent.direction) {
            return MoveOutcome::Noop {
                reason: NoopReason::NoAdjacentOutput,
            };
        }
        return MoveOutcome::Noop {
            reason: NoopReason::Boundary,
        };
    }
    let Some(parent) = path.get(level - 1) else {
        return MoveOutcome::Noop {
            reason: NoopReason::Boundary,
        };
    };
    let parent_insertion_index = parent.child_index + usize::from(step == 1);
    let same_axis = parent.axis == ancestor.axis;
    MoveOutcome::Planned(MovePlan::for_operation(
        intent,
        MoveOperation::EscapeParent {
            rule: Rule::R3,
            container: ancestor.group_id.clone(),
            parent: parent.group_id.clone(),
            container_child_index: parent.child_index,
            parent_insertion_index: same_axis.then_some(parent_insertion_index),
            continuation: if same_axis {
                EscapeContinuation::None
            } else {
                EscapeContinuation::R1
            },
        },
    ))
}

/// Deterministic directional move plan for `intent` against `snapshot`.
///
/// The snapshot is borrowed and never mutated; all errors leave the input
/// observably unchanged. Full capabilities are assumed; use
/// [`plan_move_with_capabilities`] for adapter-facing gating.
#[must_use]
pub fn plan_move(snapshot: &Snapshot, intent: &MoveIntent) -> MoveOutcome {
    plan_move_with_capabilities(snapshot, intent, &Capabilities::full())
}

/// [`plan_move`] gated by adapter-declared [`Capabilities`]: an operation
/// whose [`Capability`] is not declared is refused as
/// [`RejectionKind::UnsupportedTopology`] before any plan is emitted.
#[must_use]
pub fn plan_move_with_capabilities(
    snapshot: &Snapshot,
    intent: &MoveIntent,
    capabilities: &Capabilities,
) -> MoveOutcome {
    let validated = match validate_snapshot(snapshot, intent) {
        Ok(validated) => validated,
        Err(outcome) => return outcome,
    };
    let Some(tree) = &validated.source.tree else {
        return MoveOutcome::Noop {
            reason: NoopReason::Boundary,
        };
    };
    let Some(path) = path_to(tree, &intent.focused_leaf) else {
        return rejected(
            RejectionKind::FocusedLeafNotFound,
            "focused leaf is not present in the source topology",
        );
    };
    let local = plan_local(intent, validated.source, &path);
    // Local restructure/swap/escape wins first: any planned local outcome
    // returns unchanged (gated by capabilities only).
    if matches!(local, MoveOutcome::Planned(_)) {
        return gate(local, capabilities);
    }
    // Cross eligibility (REQ-MOV-08/OUT-01, item 5.1): the mover is exhausted
    // locally with no applicable restructure, either at the root edge of a
    // root group (`Boundary` with a one-level path) or as a sole root leaf
    // (`SingleRootLeaf` with an empty path over a lone leaf tree). Any other
    // noop (notably `NoAdjacentOutput`: no candidate in this direction) is
    // returned unchanged with no cross attempt and no output wrapping.
    let sole_leaf = matches!(
        local,
        MoveOutcome::Noop {
            reason: NoopReason::SingleRootLeaf,
        }
    ) && path.is_empty()
        && matches!(tree, Node::Leaf { .. });
    let root_edge = matches!(
        local,
        MoveOutcome::Noop {
            reason: NoopReason::Boundary,
        }
    ) && path.len() == 1
        && matches!(tree, Node::Group { .. });
    if !sole_leaf && !root_edge {
        return gate(local, capabilities);
    }
    // Four-direction cross (item 5.2, 2026-10-09 selection): the adapter owns
    // output topology and resolves the candidate over FULL rectangles
    // (panel gaps never block: geometry selection is adapter-side, never
    // work-area gated here). Multiple candidates rank adapter-side by
    // window-centre projection, then span overlap, then left/top
    // (migration: largest shared edge); the core crosses the named adjacent
    // output in the requested direction. No candidate is a no-op;
    // non-reciprocal or unreadable topology refuses fail-closed; outputs
    // never wrap.
    let Some(target_id) = validated.source.adjacent.get(&intent.direction) else {
        return MoveOutcome::Noop {
            reason: NoopReason::NoAdjacentOutput,
        };
    };
    let Some(target) = snapshot
        .outputs
        .iter()
        .find(|output| &output.id == target_id)
    else {
        return rejected(
            RejectionKind::MalformedTopology,
            "output adjacency references an output missing from the snapshot",
        );
    };
    // Reciprocity existence (not uniqueness): the cross target must name
    // the source back on the opposite side. A one-sided or mismatched edge
    // refuses; a target touched by two sources stays valid forward (valid
    // reverse ambiguity accepted, resolved adapter-side).
    if target.adjacent.get(&opposite_direction(intent.direction)) != Some(&validated.source.id) {
        return rejected(
            RejectionKind::MalformedTopology,
            "output adjacency is not reciprocal in the requested direction",
        );
    }
    // Cross-workspace allowed: the target is the adjacent output's currently
    // selected logical workspace (its Snapshot workspace), not the same
    // backing desktop index. Ambiguous duplicate output ids fail closed at
    // validation (global duplicate NodeIds aside, duplicate OutputIds would
    // make find() ambiguous); here the first match wins deterministically but
    // session-layer adjacency validation requires exactly one domain per
    // output id for cross-output to avoid ambiguity.
    let source_root_child_index = if sole_leaf { 0 } else { path[0].child_index };
    let outcome = MoveOutcome::Planned(MovePlan::for_operation(
        intent,
        MoveOperation::CrossOutput {
            rule: Rule::R4,
            target_output: target.id.clone(),
            target_workspace: target.workspace.clone(),
            source_root_child_index,
            target: if target.tree.is_none() {
                CrossOutputTarget::Empty
            } else {
                CrossOutputTarget::Occupied
            },
        },
    ));
    gate(outcome, capabilities)
}

fn gate(outcome: MoveOutcome, capabilities: &Capabilities) -> MoveOutcome {
    match outcome {
        MoveOutcome::Planned(plan) if !capabilities.supports(plan.required_capability) => rejected(
            RejectionKind::UnsupportedTopology,
            format!(
                "adapter does not declare required capability {:?}; refusing before plan emission",
                plan.required_capability
            ),
        ),
        other => other,
    }
}

// ---- tree-relative focus (analogue of planCosmicFocus) ----

fn check_focus_node(node: &Node, seen: &mut HashSet<NodeId>) -> bool {
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
            children.iter().all(|child| check_focus_node(child, seen))
        }
    }
}

fn subtree_contains_leaf(node: &Node, target: &NodeId) -> bool {
    match node {
        Node::Leaf { id } => id == target,
        Node::Group { children, .. } => children
            .iter()
            .any(|child| subtree_contains_leaf(child, target)),
    }
}

fn select_perpendicular_child<'a>(
    children: &'a [Node],
    history_newest_first: &[NodeId],
) -> Option<&'a Node> {
    for leaf in history_newest_first {
        if let Some(child) = children
            .iter()
            .find(|child| subtree_contains_leaf(child, leaf))
        {
            return Some(child);
        }
    }
    children.first()
}

fn descend_focus_target(
    mut current: &Node,
    direction: Direction,
    history_newest_first: &[NodeId],
) -> Option<FocusPlan> {
    let mut route = vec![current.id().clone()];
    loop {
        match current {
            Node::Leaf { id } => {
                return Some(FocusPlan::Focused {
                    leaf: id.clone(),
                    route,
                });
            }
            Node::Group { axis, children, .. } => {
                if children.is_empty() {
                    return None;
                }
                let child = if *axis == Axis::for_direction(direction) {
                    if step_for(direction) == 1 {
                        children.first()
                    } else {
                        children.last()
                    }
                } else {
                    select_perpendicular_child(children, history_newest_first)
                };
                current = child?;
                route.push(current.id().clone());
            }
        }
    }
}

/// Tree-relative directional focus: climb ancestors until a matching-axis
/// sibling exists in direction `direction`, then descend (same-axis edge
/// child; perpendicular most recently focused descendant or first child).
/// Exhausted edges report [`FocusPlan::Edge`]. Fail-closed (`None`) on
/// malformed, duplicate, single-child, or missing-leaf topology. Analogue of
/// `planCosmicFocus`.
#[must_use]
pub fn plan_focus(tree: &Node, focused_leaf: &NodeId, direction: Direction) -> Option<FocusPlan> {
    plan_focus_with_history(tree, focused_leaf, direction, &[])
}

/// Tree-relative directional focus with domain focus history (newest-first):
/// same-axis descent stays on the bordering edge child; perpendicular descent
/// at each perpendicular level selects the child containing the most recently
/// focused eligible descendant in `history_newest_first`, falling back to the
/// first child when no descendant has history.
#[must_use]
pub fn plan_focus_with_history(
    tree: &Node,
    focused_leaf: &NodeId,
    direction: Direction,
    history_newest_first: &[NodeId],
) -> Option<FocusPlan> {
    if focused_leaf.0.is_empty() {
        return None;
    }
    if !check_focus_node(tree, &mut HashSet::new()) {
        return None;
    }
    let path = path_to(tree, focused_leaf)?;
    if path.is_empty() {
        return Some(FocusPlan::Edge);
    }
    let wanted = Axis::for_direction(direction);
    let step = step_for(direction);
    for ancestor in path.iter().rev() {
        if ancestor.axis != wanted {
            continue;
        }
        let sibling_index = ancestor.child_index as i32 + step;
        let sibling = if sibling_index >= 0 {
            ancestor.children.get(sibling_index as usize)
        } else {
            None
        };
        let Some(sibling) = sibling else { continue };
        return descend_focus_target(sibling, direction, history_newest_first);
    }
    Some(FocusPlan::Edge)
}

// ---- split-share keyboard resize (portable, display-independent) ----

/// Share-step denominator: a resize transfers exactly 1/16 of the selected
/// adjacent pair total.
pub const RESIZE_STEP_DENOMINATOR: u64 = 16;

/// Resolved keyboard resize step: the stable target split (`target_group`),
/// the selected adjacent pair (`focused_child` grows toward the resize
/// direction, `neighbor_child` shrinks), their group indices, and the full
/// selected-group share vectors before (`old_shares`) and after
/// (`new_shares`). Descendants/topology/order are unchanged; only the two
/// selected shares change plus an exact x16 ratio-preserving normalization of
/// the whole group when the pair total is not divisible by 16.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResizeStep {
    pub target_group: NodeId,
    pub focused_index: usize,
    pub neighbor_index: usize,
    pub focused_child: NodeId,
    pub neighbor_child: NodeId,
    pub old_shares: Vec<u64>,
    pub new_shares: Vec<u64>,
}

/// Resize planning failure: `NoBoundary` means no applicable directional
/// boundary exists (non-divergent unchanged); `Malformed` means topology,
/// shares, or arithmetic are malformed/unrepresentable (fail closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizePlanError {
    NoBoundary,
    Malformed,
}

fn resize_group_shares(tree: &Node, id: &NodeId) -> Option<Vec<u64>> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group {
            id: gid,
            shares,
            children,
            ..
        } => {
            if gid == id {
                return Some(shares.clone());
            }
            for child in children {
                if let Some(found) = resize_group_shares(child, id) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// Pure share-step transfer over one N-ary group share vector.
///
/// Transfers exactly 1/16 of the selected adjacent pair total from the
/// directional neighbor (`neighbor_index`, donor) to the focused-containing
/// child (`focused_index`, recipient). Before transfer, if the pair total is
/// not divisible by 16, every share in the group is multiplied by 16 as an
/// exact ratio-preserving normalization; then `delta = pair_total / 16`.
///
/// Checked arithmetic throughout: scaling/total/add/subtract overflow, zero
/// shares, out-of-bounds or non-adjacent indices fail closed (`None`). If the
/// donor would fall below one, the transfer clamps to `donor - 1`; a zero
/// clamped transfer (donor at one) also fails closed (`None`) so callers can
/// refuse unchanged. Pair sum is conserved after normalization.
#[must_use]
pub fn expected_resize_shares(
    old_shares: &[u64],
    focused_index: usize,
    neighbor_index: usize,
) -> Option<Vec<u64>> {
    if old_shares.len() < 2
        || focused_index >= old_shares.len()
        || neighbor_index >= old_shares.len()
        || focused_index == neighbor_index
    {
        return None;
    }
    if (focused_index as i32 - neighbor_index as i32).abs() != 1 {
        return None;
    }
    if old_shares.contains(&0) {
        return None;
    }
    let mut total: u64 = 0;
    for share in old_shares {
        total = total.checked_add(*share)?;
    }
    if total == 0 {
        return None;
    }
    let pair_total = old_shares[focused_index].checked_add(old_shares[neighbor_index])?;
    if pair_total == 0 {
        return None;
    }
    let scaled: Vec<u64> = if pair_total % RESIZE_STEP_DENOMINATOR != 0 {
        let mut out = Vec::with_capacity(old_shares.len());
        for share in old_shares {
            out.push(share.checked_mul(RESIZE_STEP_DENOMINATOR)?);
        }
        out
    } else {
        old_shares.to_vec()
    };
    let mut scaled_total: u64 = 0;
    for share in &scaled {
        scaled_total = scaled_total.checked_add(*share)?;
    }
    let scaled_pair = scaled[focused_index].checked_add(scaled[neighbor_index])?;
    if scaled_pair % RESIZE_STEP_DENOMINATOR != 0 {
        return None;
    }
    let mut delta = scaled_pair / RESIZE_STEP_DENOMINATOR;
    if delta == 0 {
        return None;
    }
    let donor = scaled[neighbor_index];
    if donor <= 1 {
        return None;
    }
    if donor <= delta {
        delta = donor.checked_sub(1)?;
        if delta == 0 {
            return None;
        }
    }
    let mut next = scaled.clone();
    next[focused_index] = scaled[focused_index].checked_add(delta)?;
    next[neighbor_index] = scaled[neighbor_index].checked_sub(delta)?;
    if next.contains(&0) {
        return None;
    }
    // Pair sum conserved after normalization.
    if next[focused_index].checked_add(next[neighbor_index])? != scaled_pair {
        return None;
    }
    if next == scaled {
        return None;
    }
    Some(next)
}

/// Reusable pure primitive for normalized target boundary/share operations.
///
/// Validates the entire input tree before applying: every node identity is
/// non-empty and globally unique, and every group carries exactly one
/// positive share per child (at least two children) with a non-overflowing
/// total. Then validates `target_group` names exactly one existing group
/// with exactly `new_shares.len()` children, all new shares positive with a
/// non-overflowing total, and returns a new tree with only that group's
/// shares replaced. Topology, order, axis, descendants, and all other groups
/// are unchanged. Fail-closed (`None`) on any mismatch, including duplicate
/// ids (no first-match duplicate path: the whole tree is scanned and exactly
/// one replacement must apply). Future pointer-resize can submit a
/// normalized boundary/share operation through this primitive without
/// duplicating operation validation/application.
#[must_use]
pub fn apply_resize_shares(tree: &Node, target_group: &NodeId, new_shares: &[u64]) -> Option<Node> {
    if target_group.0.is_empty() || new_shares.len() < 2 || new_shares.contains(&0) {
        return None;
    }
    let mut total: u64 = 0;
    for share in new_shares {
        total = total.checked_add(*share)?;
    }
    if total == 0 {
        return None;
    }
    if !validate_resize_tree(tree, &mut HashSet::new()) {
        return None;
    }
    let mut replacements = 0usize;
    let next = apply_resize_shares_checked(tree, target_group, new_shares, &mut replacements)?;
    if replacements != 1 {
        return None;
    }
    Some(next)
}

fn validate_resize_tree(node: &Node, seen: &mut HashSet<NodeId>) -> bool {
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
            children
                .iter()
                .all(|child| validate_resize_tree(child, seen))
        }
    }
}

fn apply_resize_shares_checked(
    node: &Node,
    target_group: &NodeId,
    new_shares: &[u64],
    replacements: &mut usize,
) -> Option<Node> {
    match node {
        Node::Leaf { id } => Some(Node::Leaf { id: id.clone() }),
        Node::Group {
            id,
            axis,
            children,
            shares,
        } => {
            let mut next_children = Vec::with_capacity(children.len());
            for child in children {
                next_children.push(apply_resize_shares_checked(
                    child,
                    target_group,
                    new_shares,
                    replacements,
                )?);
            }
            if id == target_group {
                if children.len() != new_shares.len() || shares.len() != children.len() {
                    return None;
                }
                *replacements += 1;
                return Some(Node::Group {
                    id: id.clone(),
                    axis: *axis,
                    children: next_children,
                    shares: new_shares.to_vec(),
                });
            }
            Some(Node::Group {
                id: id.clone(),
                axis: *axis,
                children: next_children,
                shares: shares.clone(),
            })
        }
    }
}

/// Deterministic nearest applicable matching-axis ancestor boundary.
///
/// Walks focus-leaf ancestors nearest outward; the first ancestor with
/// `Axis::for_direction(direction)` and a direct child on the requested side
/// is selected. At a matching-axis edge, the walk continues outward. Returns
/// [`ResizePlanError::NoBoundary`] when no candidate exists and
/// [`ResizePlanError::Malformed`] on malformed topology/shares or
/// unrepresentable arithmetic.
pub fn plan_resize_step(
    tree: &Node,
    focused_leaf: &NodeId,
    direction: Direction,
) -> Result<ResizeStep, ResizePlanError> {
    if focused_leaf.0.is_empty() {
        return Err(ResizePlanError::Malformed);
    }
    if !check_focus_node(tree, &mut HashSet::new()) {
        return Err(ResizePlanError::Malformed);
    }
    let path = path_to(tree, focused_leaf).ok_or(ResizePlanError::Malformed)?;
    if path.is_empty() {
        return Err(ResizePlanError::NoBoundary);
    }
    let wanted = Axis::for_direction(direction);
    let step = step_for(direction);
    for ancestor in path.iter().rev() {
        if ancestor.axis != wanted {
            continue;
        }
        let neighbor = ancestor.child_index as i32 + step;
        if neighbor < 0 || (neighbor as usize) >= ancestor.children.len() {
            continue;
        }
        let neighbor_index = neighbor as usize;
        let focused_index = ancestor.child_index;
        let focused_child = ancestor.children[focused_index].id().clone();
        let neighbor_child = ancestor.children[neighbor_index].id().clone();
        let old_shares =
            resize_group_shares(tree, &ancestor.group_id).ok_or(ResizePlanError::Malformed)?;
        if old_shares.len() != ancestor.children.len() {
            return Err(ResizePlanError::Malformed);
        }
        // An exhausted donor (post-normalization donor <= 1, or zero
        // permitted transfer) makes this boundary not applicable: continue
        // outward to the next matching-axis ancestor. Overflow or malformed
        // shares/topology still fail closed.
        if old_shares.contains(&0) {
            return Err(ResizePlanError::Malformed);
        }
        let pair_total = old_shares[focused_index]
            .checked_add(old_shares[neighbor_index])
            .ok_or(ResizePlanError::Malformed)?;
        let needs_scaling = pair_total % RESIZE_STEP_DENOMINATOR != 0;
        let scaled_donor = if needs_scaling {
            old_shares[neighbor_index]
                .checked_mul(RESIZE_STEP_DENOMINATOR)
                .ok_or(ResizePlanError::Malformed)?
        } else {
            old_shares[neighbor_index]
        };
        if scaled_donor <= 1 {
            continue;
        }
        let scaled_pair = if needs_scaling {
            let scaled_focused = old_shares[focused_index]
                .checked_mul(RESIZE_STEP_DENOMINATOR)
                .ok_or(ResizePlanError::Malformed)?;
            scaled_focused
                .checked_add(scaled_donor)
                .ok_or(ResizePlanError::Malformed)?
        } else {
            pair_total
        };
        if scaled_pair / RESIZE_STEP_DENOMINATOR == 0 {
            continue;
        }
        if scaled_donor <= scaled_pair / RESIZE_STEP_DENOMINATOR
            && scaled_donor
                .checked_sub(1)
                .ok_or(ResizePlanError::Malformed)?
                == 0
        {
            continue;
        }
        let Some(new_shares) = expected_resize_shares(&old_shares, focused_index, neighbor_index)
        else {
            return Err(ResizePlanError::Malformed);
        };
        return Ok(ResizeStep {
            target_group: ancestor.group_id.clone(),
            focused_index,
            neighbor_index,
            focused_child,
            neighbor_child,
            old_shares,
            new_shares,
        });
    }
    Err(ResizePlanError::NoBoundary)
}

// ---- proportional fraction sizing (analogue of cosmic-move-adapter.ts) ----

fn valid_shares(existing: &[f64]) -> bool {
    !existing.is_empty()
        && existing
            .iter()
            .all(|share| *share > 0.0 && share.is_finite())
        && existing.iter().sum::<f64>() > 0.0
}

/// Entrant takes an equal share; survivors scale proportionally preserving
/// ratios. Source: `cosmicInsertShares` (upstream `Data::add_window`).
/// Fail-closed (`None`) on malformed input.
#[must_use]
pub fn insert_shares(existing: &[f64], index: usize) -> Option<Vec<f64>> {
    if !valid_shares(existing) || index > existing.len() {
        return None;
    }
    let total: f64 = existing.iter().sum();
    let count = existing.len() + 1;
    let entrant = total / count as f64;
    let remainder = total - entrant;
    let scaled: Vec<f64> = existing
        .iter()
        .map(|share| (share / total) * remainder)
        .collect();
    let mut result = Vec::with_capacity(count);
    result.extend_from_slice(&scaled[..index]);
    result.push(entrant);
    result.extend_from_slice(&scaled[index..]);
    Some(result)
}

/// Removed share redistributes proportionally. Source: `cosmicRemoveShares`
/// (upstream `Data::remove_window`). Fail-closed (`None`) on malformed input.
#[must_use]
pub fn remove_shares(existing: &[f64], index: usize) -> Option<Vec<f64>> {
    if existing.len() < 2 || index >= existing.len() || !valid_shares(existing) {
        return None;
    }
    let removed = existing[index];
    let remaining: Vec<f64> = existing
        .iter()
        .enumerate()
        .filter(|(position, _)| *position != index)
        .map(|(_, share)| *share)
        .collect();
    let remaining_sum: f64 = remaining.iter().sum();
    if remaining_sum <= 0.0 {
        return None;
    }
    Some(
        remaining
            .iter()
            .map(|share| share + (share / remaining_sum) * removed)
            .collect(),
    )
}

/// Half-half new-group shares. Source: `cosmicHalfSplit` (upstream
/// `Data::new_group`).
#[must_use]
pub const fn half_split() -> [f64; 2] {
    [0.5, 0.5]
}

#[cfg(test)]
mod tests;
