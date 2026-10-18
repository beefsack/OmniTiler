//! Portable world report/reply contract (D1-D5).
//!
//! Serde-free typed envelope for the full-state boundary in
//! `docs/changes/archive/core-full-state-replies.md`. No transport, JSON, platform,
//! or process imports. Reuses the existing typed carriers
//! ([`crate::seed::EngineWindow`], [`crate::session::OutputDomain`],
//! [`crate::directional`] ids, [`crate::workspace_scope::WorkspaceMode`])
//! rather than lossy clones, so the transition in
//! [`crate::world_engine`] and the protocol codec share one shape.
//!
//! The [`WorldEngine`] owns the transition; this module owns shapes,
//! validation, and freshness. Sizing and the canonical encoding live in
//! [`crate::world_sizing`] and re-export here, so downstream paths
//! (`world_engine`, protocol codec) keep one stable import root.

pub use crate::world_sizing::{
    encode_reply_canonical, int_len, json_string_len, reply_estimate, reply_exact_bytes,
    reply_upper_bound, report_estimate, u64_len, usize_len,
};

use std::collections::BTreeSet;

use crate::active_group::ActiveGroupMember;
use crate::boundary::CoreCommand;
use crate::boundary::NoGroupReason;
use crate::bounds::{GEOMETRY_BOUND, is_gap, is_opaque_id, valid_carried_rect};
use crate::directional::{Direction, NodeId, OutputId, WindowId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{DomainKey, DragHoverPrior, OutputDomain};
use crate::workspace_scope::WorkspaceMode;

/// World envelope version (single source for the wire `v`).
pub const WORLD_CONTRACT_VERSION: u32 = 1;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;

/// Hard input caps so the worst-case budget is bounded before any work.
pub const MAX_WORLD_DOMAINS: usize = 64;
pub const MAX_WORLD_WINDOWS: usize = 10_000;
/// Bound on explicit-close ids per report (same lifetime alphabet as windows).
pub const MAX_WORLD_CLOSED: usize = 10_000;
pub const MAX_NATIVE_PER_OUTPUT: usize = 256;
pub const MAX_WORLD_OUTPUTS: usize = 16;
pub const MAX_WORLD_FEEDBACK: usize = 1024;
pub const MAX_WORLD_BINDINGS: usize = 4096;

/// Tiling policy for one desired workspace row. The core retains one mode
/// per workspace (Planner-owned, toggled only through
/// [`WorldCommand::SetWorkspaceTiling`); the adapter never decides.
/// `Tiled` is core-managed; `Floating` leaves native geometry alone (no
/// frame writes) with the scope explicitly untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TilingMode {
    Tiled,
    Floating,
}

impl TilingMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tiled => "tiled",
            Self::Floating => "floating",
        }
    }
}

/// Scope key for one desired workspace row: the owning output id for
/// per-output-local and global-unique modes, `shared` for the shared mode.
#[must_use]
pub fn workspace_scope_key(mode: WorkspaceMode, output: &OutputId) -> String {
    match mode {
        WorkspaceMode::Shared => "shared".to_owned(),
        WorkspaceMode::PerOutputLocal | WorkspaceMode::GlobalUnique => output.0.clone(),
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HoldReason {
    Unreadable,
    IncompleteInventory,
    GestureBarrier,
    RefusedTarget,
    AmbiguousCreation,
    OccupiedRemoval,
    ConstraintClamp,
    /// Minimized lifetime: retained in its homed domain with no managed
    /// geometry. Explicit row, never silent absence or inferred close.
    Minimized,
}

impl HoldReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::IncompleteInventory => "incomplete-inventory",
            Self::GestureBarrier => "gesture-barrier",
            Self::RefusedTarget => "refused-target",
            Self::AmbiguousCreation => "ambiguous-creation",
            Self::OccupiedRemoval => "occupied-removal",
            Self::ConstraintClamp => "constraint-clamp",
            Self::Minimized => "minimized",
        }
    }
}

/// Per-scope authority: every known scope has an explicit status. Omission
/// means desired absence ONLY inside a completely enumerated controlled
/// inventory; omission inside held/untouched authorizes no write/retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeAuthority {
    Controlled,
    Untouched,
    Hold(HoldReason),
}

impl ScopeAuthority {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Controlled => "controlled",
            Self::Untouched => "untouched",
            Self::Hold(_) => "hold",
        }
    }

    #[must_use]
    pub const fn hold_reason(self) -> Option<HoldReason> {
        match self {
            Self::Hold(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Diagnostic float origin (D4, selected 2026-10-17): provenance only,
/// never a functional branch. `Capacity` means the window floats because
/// no split was available; ordinary user floats carry `None`. Every
/// capacity float is functionally an ordinary float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatOrigin {
    Capacity,
}

impl FloatOrigin {
    /// Wire token for this value, matching the protocol codec vocabulary.
    /// `None` on the row encodes as the empty token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Capacity => "capacity",
        }
    }
}

/// One-shot effect kinds. Closed: focus/raise/stacking only. Transfer/view
/// targets are persistent ([`TransferTarget`]), never effects (D3=A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectKind {
    Focus,
    Raise,
    Stack,
}

impl EffectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Raise => "raise",
            Self::Stack => "stack",
        }
    }
}

/// One-shot effect: the adapter marks it spent BEFORE first native dispatch;
/// duplicates and repeated diffs never reapply. Lost/stale effects drop
/// (at-most-once), never steal focus on a later unrelated report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneShotEffect {
    pub id: String,
    pub kind: EffectKind,
    pub target: WindowId,
    /// Validity bound to the issuing `state_revision`; newer user input
    /// invalidates.
    pub issued_revision: u64,
}

impl OneShotEffect {
    /// Mint the explicit-focus outcome of executed focus command `seq`.
    /// The `fx-<seq>` identity correlates the effect with its dispatching
    /// command; automatic admission intents mint `af-<window>` instead and
    /// must never actuate focus on adapters keeping observed-focus
    /// authority (see [`OneShotEffect::is_explicit_focus_for`]).
    #[must_use]
    pub fn explicit_focus(command_seq: u64, target: WindowId, issued_revision: u64) -> Self {
        Self {
            id: format!("fx-{command_seq}"),
            kind: EffectKind::Focus,
            target,
            issued_revision,
        }
    }

    /// True when this effect is the explicit-focus outcome of command `seq`
    /// (exact identity correlation, never a prefix heuristic): every other
    /// focus effect is an automatic admission intent and must not dispatch
    /// native focus where observations keep observed-focus authority.
    #[must_use]
    pub fn is_explicit_focus_for(&self, command_seq: u64) -> bool {
        self.kind == EffectKind::Focus && self.id == format!("fx-{command_seq}")
    }
}

/// Retained transfer/view target (D3=A): stays outstanding until confirmed,
/// superseded by newer external input, or settled by native refusal.
/// Stable operation identity fences ambiguous outcomes; a possibly-executed
/// transfer is never repeated without fresh unambiguous proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferKind {
    Window(WindowId),
    /// View-only operation: no window moves, only the output view changes.
    View,
    /// Whole-workspace migration: one live native workspace id moves scope.
    /// `target_workspace` on the [`TransferTarget`] names the migrated id.
    Migration,
}

/// Retained transfer/view target (D3=A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTarget {
    pub operation_id: String,
    pub kind: TransferKind,
    pub target_output: OutputId,
    /// Native backing workspace id.
    pub target_workspace: String,
    pub follow: bool,
    /// Set when the last feedback was possibly-executed: the target stays
    /// desired but must not be re-actuated.
    pub held: bool,
}

/// Per-property window authority: floats/excluded/game rows may authorize
/// membership without authorizing frame changes, so one blanket boolean is
/// insufficient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowDesire {
    pub lifetime: WindowId,
    pub output: OutputId,
    /// Native backing workspace id.
    pub workspace: String,
    pub allocation: Option<crate::geometry::Rect>,
    pub effective_frame: Option<crate::geometry::Rect>,
    pub frame_authority: ScopeAuthority,
    pub membership_authority: ScopeAuthority,
    /// Explicit ordinary-float classification (D4/D5): true for every
    /// retained float exception row in a controlled scope (user and
    /// capacity floats alike), false for tiled rows and for every
    /// unclassified protected/held/overlay/minimized row. The adapter
    /// must never infer float from a missing allocation alone: only a
    /// row with `floating` true AND untouched frame authority converges
    /// float intent.
    pub floating: bool,
    /// Diagnostic float origin (see [`FloatOrigin`]): `Some(Capacity)`
    /// only on capacity floats, `None` on ordinary floats and on every
    /// non-float row. No functional branches may read this field.
    pub float_origin: Option<FloatOrigin>,
    /// Explicit retained float-placement operation (`op-<cmdSeq>`) authorizing
    /// exactly one geometry write for this window. `None` (empty on the wire)
    /// authorizes nothing: ordinary floats keep their frames and a bare
    /// `Controlled` frame authority never writes float geometry. Set only
    /// while a [`crate::world_engine::WorldEngine`] retained float target is
    /// outstanding for this window; cleared on confirmation, proven refusal,
    /// external supersession, close, newer intent, or generation reset.
    pub frame_operation: Option<String>,
}

/// Desired workspace row. `existence=false` is removal ELIGIBILITY only (the
/// adapter re-enumerates occupants, visibility, pins, mappings and minimum
/// count before deleting; unknown occupancy keeps). `id` is the native
/// backing id once bound, else the pending logical token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDesire {
    pub id: String,
    pub output: OutputId,
    pub native_binding: Option<String>,
    pub logical_id: Option<String>,
    /// Owning scope: the output id (local/global-unique) or `shared`.
    pub scope: String,
    pub order: usize,
    pub tiling_mode: TilingMode,
    pub existence: bool,
    pub hold: Option<HoldReason>,
}

/// The native id the adapter must actuate for this row, or `None` when
/// unbound: the adapter must create through the one-in-flight creation path
/// and MUST NOT use the logical token as a native id, even if the token
/// string happens to appear in the native inventory.
#[must_use]
pub fn actuation_native_id(row: &WorkspaceDesire) -> Option<&str> {
    row.native_binding.as_deref()
}

/// Desired output row: `logical_view` is the persistent view target.
/// Observed active focus is informational only, never a focus command.
/// `tiling_mode` is the retained planner-owned mode of the current native
/// view (default tiled), so the adapter can see floating outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDesire {
    pub output: OutputId,
    /// Logical workspace id shown, when the native→logical mapping is known.
    pub logical_view: Option<String>,
    pub tiling_mode: TilingMode,
    pub hold: Option<HoldReason>,
}

/// Scoped status row: every known scope is explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeStatus {
    pub scope: String,
    pub authority: ScopeAuthority,
}

/// What an [`AttemptFeedback`] row reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttemptKind {
    Transfer,
    View,
    Creation,
    Effect,
    Property,
}

impl AttemptKind {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transfer => "transfer",
            Self::View => "view",
            Self::Creation => "creation",
            Self::Effect => "effect",
            Self::Property => "property",
        }
    }
}

/// Actuation attempt outcome with stable identity. Four states separate what
/// a boolean cannot: never tried, maybe ran (hold, never blindly repeat),
/// proven refusal (settle to observed/held), and external supersession
/// (newer user input wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttemptOutcome {
    Unattempted,
    PossiblyExecuted,
    RefusedProven,
    SupersededExternal,
    Applied,
}

impl AttemptOutcome {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unattempted => "unattempted",
            Self::PossiblyExecuted => "possibly-executed",
            Self::RefusedProven => "refused-proven",
            Self::SupersededExternal => "superseded-external",
            Self::Applied => "applied",
        }
    }
}

/// Per-operation actuation feedback reported on the next report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptFeedback {
    pub operation_id: String,
    pub kind: AttemptKind,
    pub outcome: AttemptOutcome,
    pub hold: Option<HoldReason>,
}

/// One domain observation: the full typed [`OutputDomain`] (bounds, gap,
/// adjacency = settings) plus the complete [`EngineWindow`] carriers with
/// all safety flags (floating/fit-excluded/fullscreen/maximized/sticky,
/// fixed assertions, size-hint constraints). No lossy clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainReport {
    pub domain: OutputDomain,
    pub outer_gap: i32,
    /// Focused window in this domain; empty window id means none.
    pub focused: WindowId,
    pub windows: Vec<EngineWindow>,
    /// Explicit closes (D23). Always honored, even when the surrounding
    /// inventory is incomplete; only IMPLICIT missing=closed inference
    /// requires a provably complete inventory.
    pub closed: Vec<WindowId>,
    /// False when the scope was unreadable: the scope becomes an explicit
    /// hold and its rows authorize nothing.
    pub readable: bool,
    pub hold: Option<HoldReason>,
}

/// Per-output native topology: live backing ids in native order, the current
/// view, whether enumeration was complete for this output, the live native
/// FULL output geometry when observed, and whether this output is the
/// active (primary) screen.
///
/// `geometry` is the core-owned topology authority: the FULL native output
/// rectangle in one coherent adapter coordinate space, used for directional
/// planning through the shared strict unique-reciprocal derivation (exact
/// edge-touch, positive overlap, no wrap). A report claiming complete
/// topology carries a valid FULL rectangle for every output; `None` stays
/// representable for explicitly unreadable/partial observations and then
/// narrowly fences topology-dependent crossing, migration, and remap (never
/// an inferred work area, never a window frame proxy, and never permission
/// to derive from the readable subset). Domain `bounds` remains the
/// distinct work-area field, with outer-gap handling unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputTopology {
    pub output: OutputId,
    pub native_order: Vec<String>,
    pub current_native: String,
    pub complete: bool,
    pub geometry: Option<Rect>,
    pub primary: bool,
}

/// Native/logical workspace binding with inventory flags. Pinned and
/// displaced ids are retained against trailing cleanup; unmanaged ids are
/// observed but not owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBinding {
    pub logical_id: String,
    pub native_id: String,
    pub output: OutputId,
    pub pinned: bool,
    pub displaced: bool,
    pub managed: bool,
}

/// Explicit minimized lifetime: a window the adapter holds minimized,
/// homed on its workspace. Reported separately from active tiled domain
/// membership (minimized windows are absent from `DomainReport.windows`);
/// the core never tiles them and never writes frames for them. Overlap
/// with live membership or explicit closes refuses fail-closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizedEntry {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: crate::directional::WorkspaceId,
}

/// Explicit world commands. At most one per report; observation-only
/// reports carry none. Every variant consumes one `command_seq` (bound to
/// the adapter incarnation/core generation high-watermark) and is retained
/// or spent exactly like the original three: selection/view commands retain
/// a [`TransferKind::View`] or [`TransferKind::Migration`] target,
/// `Send`/`SendRelative`/`SendNumbered`-to-bound-target and send operations
/// retain a window transfer, focus operations emit one at-most-once effect,
/// `SetWorkspaceTiling` retains planner-owned tiling mode, and read-only
/// operations reconcile only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldCommand {
    /// One-shot focus attempt (emits an effect, never retained).
    Focus { window: WindowId, direction: String },
    /// Window transfer (retained until confirmed/refused/superseded).
    Send {
        window: WindowId,
        target_output: OutputId,
        target_workspace: String,
        follow: bool,
    },
    /// View-only switch (retained until confirmed/superseded; no window).
    SwitchView { output: OutputId, workspace: String },
    /// Planner-resolved direct selection (D2): records history and retains
    /// a view target like [`WorldCommand::SwitchView`].
    SelectWorkspace { output: OutputId, workspace: String },
    /// Planner-resolved numbered selection (D2): positional `1..=9` over
    /// the scoped ring (`0` is the trailing path and is invalid here).
    SelectNumbered { output: OutputId, index: u8 },
    /// Planner-resolved relative selection (D2): `delta` is `-1`/`+1` with
    /// first/last wrap over the scoped ring.
    SelectRelative { output: OutputId, delta: i32 },
    /// Planner-resolved previous-toggle selection (D2): no-op without
    /// recorded history, never recreates.
    SelectPrevious { output: OutputId },
    /// Trailing selection (D2): reuse the live trailing empty or create a
    /// stable pending logical token, exactly like lifecycle.
    SelectTrailing { output: OutputId },
    /// Relative send (D2): the scoped ring resolves once before transfer;
    /// filling a resolved trailing empty is an ordinary transfer.
    SendRelative {
        window: WindowId,
        output: OutputId,
        delta: i32,
        follow: bool,
    },
    /// Numbered send: positional `0..=9` over the scoped send ring for
    /// `output` (`0` is the trailing path: reuse the live trailing empty
    /// or create a stable pending token, exactly like lifecycle).
    /// Resolving to a bound native is an ordinary transfer and never
    /// view-switches, including `follow=false` stays. Resolving to an
    /// unbound pending token pends the send under core (stable operation
    /// identity, no view target, never the token as a native id) and the
    /// engine send starts once the binding is reported.
    SendNumbered {
        window: WindowId,
        output: OutputId,
        index: u8,
        follow: bool,
    },
    /// Planner-owned per-workspace tiling toggle (legacy Meta+Y parity):
    /// retain `tiled` for one live native workspace. Disabling releases
    /// the retained session (no frame writes while floating); re-enabling
    /// re-adopts through the fresh route. Never decided by the adapter.
    SetWorkspaceTiling {
        output: OutputId,
        workspace: String,
        tiled: bool,
    },
    /// Whole-workspace migration (D2): move one live native workspace id
    /// from the source scope to the target scope. Retained until the
    /// topology/bindings confirm it.
    MigrateWorkspace {
        source_output: OutputId,
        target_output: OutputId,
        workspace: String,
    },
    /// Direction-only focus (core-owned topology): the adapter names the
    /// window and direction only, never the target or `crossOutput`. Core
    /// resolves the strict unique-reciprocal target from retained FULL
    /// rects; when it exists and the KDE focus policy allows crossing
    /// (left/right only), the cross-output pair runs, otherwise the local
    /// single-domain focus runs. Up/down never cross. Unreadable or held
    /// scopes refuse without mutation.
    FocusDirectional {
        window: WindowId,
        direction: String,
        float_subject: bool,
    },
    /// Direction-only move (core-owned topology): the adapter names the
    /// window, direction and same-axis mode only. Core resolves the strict
    /// target; when it exists the cross-output pair runs in all four
    /// directions, otherwise the local R1-R3 move runs. Never silently
    /// refuses a local move for missing topology.
    MoveDirectional {
        window: WindowId,
        direction: String,
        same_axis: crate::directional::SameAxisMove,
    },
    /// Direction-only window send (core-owned topology): the adapter names
    /// the window, direction and follow only. Core resolves the strict
    /// target output plus its current native view (same rule as the removed
    /// adapter `adjacentTarget`), then retains an ordinary window transfer.
    /// No resolvable target refuses without mutation.
    SendDirectional {
        window: WindowId,
        direction: String,
        follow: bool,
    },
    /// Direction-only whole-workspace migration (core-owned topology): the
    /// adapter names the source output, its current workspace and the
    /// direction only, never the target. Core resolves the strict target
    /// and runs the world migration path. Explicit source context is
    /// required even with no active window. No resolvable target refuses
    /// without mutation.
    MigrateDirectional {
        source_output: OutputId,
        workspace: String,
        direction: String,
    },
    /// Existing operation route: one [`CoreCommand`] executed against the
    /// homing (or first readable) domain through the full [`CoreEvent`].
    /// `SendToWorkspace`/`SendToOutput` retain a window transfer exactly
    /// like [`WorldCommand::Send`]; directional focus emits one effect;
    /// read-only queries reconcile only. Cross-output drag sources,
    /// directional migration, and domain release have dedicated world
    /// commands or no world route and are refused here.
    Operation(CoreCommand),
}

/// World report: complete normalized observation on EVERY update, covering
/// hidden and retained domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldReport {
    pub contract_version: u32,
    pub owner: String,
    pub adapter_incarnation: String,
    pub core_generation: String,
    /// Adapter-advanced on any invalidating native input, INCLUDING during
    /// an outstanding request. Merely numbering requests is insufficient.
    pub observation_seq: u64,
    /// Monotonic per (incarnation, generation) request counter.
    pub request_seq: u64,
    pub request_id: String,
    pub command_seq: Option<u64>,
    pub command: Option<WorldCommand>,
    pub workspace_mode: WorkspaceMode,
    pub topology: Vec<OutputTopology>,
    pub bindings: Vec<WorkspaceBinding>,
    pub domains: Vec<DomainReport>,
    /// Explicit minimized lifetimes, separate from active tiled domain
    /// membership. Minimized windows are absent from `domains[].windows`;
    /// each entry carries an explicit desired hold row instead.
    pub minimized: Vec<MinimizedEntry>,
    /// Ingress fixed-size admission predicate (R-SPC-04 D1): the core
    /// applies it to subsequent admissions only, never reclassifying
    /// retained windows.
    pub fixed_size_predicate: crate::size_hints::FixedSizePredicate,
    /// Ingress migration source-refill mode (R-WS-12 G-37, main.xml
    /// `migrationSourceRefill`, catalog `migration.refill`): the core
    /// applies it to subsequent migrations only, never moving an
    /// already-migrated workspace. Never reset committed state on change.
    pub migration_source_refill: crate::output_selection::MigrationSourceRefill,
    /// Ingress D2 anchor fallback (main.xml `anchorFallback`): selects the
    /// fallback anchor for subsequent capacity admissions only. Missing or
    /// invalid tokens backfill the largest-splittable default; changing it
    /// never reflows, retries, or reclassifies retained state (D10).
    pub anchor_fallback: crate::admission::AnchorFallback,
    /// Ingress D3 no-space result (main.xml `noSpaceResult`): selects the
    /// no-space outcome for subsequent admission or actual capacity-repair
    /// decisions only. Missing or invalid tokens backfill float; changing
    /// it never reflows retained overlap allocations until a real capacity
    /// decision (D10).
    pub no_space_result: crate::admission::NoSpaceResult,
    /// Global active window id ("" when none): the adapter-observed
    /// foreground lifetime across all outputs. Drives focus-conditioned
    /// policy (hotplug survivor/return views); never focus proof.
    pub focused_global: String,
    /// Saved default tiling seed (legacy `defaultTiled` parity): seeds
    /// newly observed native workspaces (startup seeds all; live changes
    /// seed only subsequently observed workspaces). Manual per-workspace
    /// toggles already retained never reseed. Backfills `true` when the
    /// adapter predates the field.
    pub default_tiled: bool,
    pub inventory_complete: bool,
    pub gesture_barrier: bool,
    pub feedback: Vec<AttemptFeedback>,
}

/// World reply: single globally ordered full desired snapshot. No
/// tree/shares/focus-history/serialized Engine.
///
/// `command_result` carries the correlated per-command outcome that the
/// snapshot alone cannot express, coexisting with the snapshot ordering
/// (never a separate presentation policy envelope):
/// - `DragPreview`: read-only proposed rect plus the carried hover state
///   (`None` means no result for this reply, e.g. observation-only).
/// - `Float`: intentional-float placement (rect `None` for unfloat-to-tile).
/// - `ActiveGroup`/`NoGroup`: read-only highlight resolution.
/// - `Migration`: whole-workspace move provenance (members/floats).
/// - `None` for move/resize/tiled-admit/focus/send operations: their
///   geometry, membership, and focus ride the snapshot, transfers, and
///   effects below, so no separate result is needed (not silently
///   discarded: the snapshot IS the result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldReply {
    pub contract_version: u32,
    pub adapter_incarnation: String,
    pub core_generation: String,
    pub request_id: String,
    pub based_on_observation_seq: u64,
    pub state_revision: u64,
    pub ready: bool,
    /// Set when the carried command sequence was already consumed: the
    /// observation reconciled but the command never re-executed.
    pub command_consumed: bool,
    pub scopes: Vec<ScopeStatus>,
    pub workspaces: Vec<WorkspaceDesire>,
    pub outputs: Vec<OutputDesire>,
    pub windows: Vec<WindowDesire>,
    pub transfers: Vec<TransferTarget>,
    pub effects: Vec<OneShotEffect>,
    /// Correlated per-command outcome coexisting with the snapshot (see
    /// above). `None` for observation-only replies and for operations whose
    /// result the snapshot already carries.
    pub command_result: Option<CommandResult>,
    /// Reserved envelope only: negotiated full sections are not implemented
    /// in this migration.
    pub presentation_bytes: usize,
}

/// Correlated per-command outcome that the snapshot alone cannot express.
/// Wire shape is `{"op", ...}` with canonical key order frozen in
/// [`encode_reply_canonical`](crate::world_report::encode_reply_canonical);
/// every `u64` counter encodes as a bounded decimal string (never a JSON
/// number) so JavaScript adapters never lose precision past 2^53.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    /// Read-only drag-preview: the derived proposed rect plus the carried
    /// hover prior for the next preview or the final drop. `domain` is the
    /// resolution (pointer) domain, `source` the capture domain the mover
    /// leaves. `target` is the insertion-neighbor window the overlay orders
    /// above (core-resolved, never the mover: carrier ordering needs the
    /// neighbor identity, not the subject). Never commits, never advances
    /// the revision.
    DragPreview {
        rect: Rect,
        hover: Option<DragHoverPrior>,
        domain: DomainKey,
        source: DomainKey,
        target: WindowId,
        revision: u64,
    },
    /// Intentional-float placement: `rect` is the effective float rectangle
    /// (`None` for unfloat-to-tile). The adapter owns floating geometry and
    /// must not rewrite floats from snapshot rows; this result is the
    /// one-shot explicit placement provenance. `no_space` is true only
    /// when an explicit unfloat retry re-floated for lack of space (D5):
    /// the window stays floated, the adapter must not record a tile
    /// override, and the snapshot (still floated) IS the placement.
    Float {
        window: WindowId,
        rect: Option<Rect>,
        no_space: bool,
    },
    /// Read-only active-group highlight resolution with everything the
    /// renderer needs. Coexists with the snapshot; never a border policy.
    ActiveGroup {
        base_revision: u64,
        group: NodeId,
        focused_leaf: NodeId,
        focused_window: WindowId,
        members: Vec<ActiveGroupMember>,
        bounds: Rect,
    },
    /// Read-only active-group refusal with the retained base revision when
    /// known. The renderer clears on this result.
    NoGroup {
        base_revision: Option<u64>,
        reason: NoGroupReason,
    },
    /// Whole-workspace migration provenance: `members` is the full relocated
    /// member set (tiled links plus carried float exceptions, excluding
    /// stayed sticky), `floats` the relocated float exception count.
    /// Actuation itself rides the snapshot member rows plus the retained
    /// migration transfer and the source/refill views; the adapter moves
    /// each member exactly once under the stable transfer identity.
    Migration {
        direction: Direction,
        source: DomainKey,
        target: DomainKey,
        members: Vec<WindowId>,
        floats: usize,
    },
}

/// Request freshness verdict over `request_seq` alone. Command consumption
/// is tracked separately by command sequence, never conflated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestFreshness {
    /// No prior request: accept.
    New,
    /// Identical latest digest: return the cached reply with NO new revision.
    Duplicate,
    /// Reused sequence with different input: invalid, never execution.
    ReusedInvalid,
    /// Older sequence: bounded superseded error, never execution.
    Stale,
}

/// Classify request freshness. Pure over the monotonic `request_seq` bound
/// to the adapter incarnation/core generation.
#[must_use]
pub fn classify_request(
    incoming_seq: u64,
    digest: u64,
    has_last: bool,
    last_seq: u64,
    last_digest: u64,
) -> RequestFreshness {
    if !has_last {
        return RequestFreshness::New;
    }
    if incoming_seq < last_seq {
        return RequestFreshness::Stale;
    }
    if incoming_seq == last_seq {
        if digest == last_digest {
            return RequestFreshness::Duplicate;
        }
        return RequestFreshness::ReusedInvalid;
    }
    RequestFreshness::New
}

/// Deterministic FNV-1a digest over the canonical report content. Covers
/// sequences, mode, owner, request/command identity, domains (bounds, gap,
/// adjacency, focused, outer gap, readability, hold), windows (identity,
/// homing, frame, every safety flag, size-hint bounds), closes, topology
/// (output, native order, current view, completeness), bindings (logical,
/// native, output, pin/displacement/managed flags), inventory completeness,
/// the default-tiling seed, the migration source-refill mode, the D2/D3
/// capacity settings, the global focus, the gesture barrier, and
/// feedback (identity, kind, outcome, hold).
/// Duplicate detection still compares full typed input equality (see
/// [`WorldEngine`](crate::world_engine::WorldEngine)); the digest is only
/// a fast pre-check, never the identity.
#[must_use]
pub fn report_digest(report: &WorldReport) -> u64 {
    fn feed(hash: &mut u64, text: &str) {
        for byte in text.bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(1099511628211);
        }
    }
    fn sep(hash: &mut u64) {
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(1099511628211);
    }
    fn flag(hash: &mut u64, set: bool) {
        feed(hash, if set { "1" } else { "0" });
        sep(hash);
    }
    let mut hash: u64 = 14695981039346656037;
    for part in [
        report.request_seq.to_string(),
        report.observation_seq.to_string(),
        report.contract_version.to_string(),
        crate::workspace_scope::mode_token(report.workspace_mode).to_owned(),
        report.owner.clone(),
        report.adapter_incarnation.clone(),
        report.core_generation.clone(),
        report.request_id.clone(),
        report
            .command_seq
            .map(|s| s.to_string())
            .unwrap_or_default(),
        format!("{:?}", report.command),
    ] {
        feed(&mut hash, &part);
        sep(&mut hash);
    }
    flag(&mut hash, report.inventory_complete);
    flag(&mut hash, report.default_tiled);
    feed(&mut hash, &report.focused_global);
    sep(&mut hash);
    flag(&mut hash, report.gesture_barrier);
    feed(&mut hash, report.fixed_size_predicate.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.migration_source_refill.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.anchor_fallback.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.no_space_result.as_wire_str());
    sep(&mut hash);
    for entry in &report.minimized {
        for part in [
            entry.window.0.clone(),
            entry.output.0.clone(),
            entry.workspace.0.clone(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
    }
    for topology in &report.topology {
        for part in [
            topology.output.0.clone(),
            topology.current_native.clone(),
            (if topology.complete { "c" } else { "i" }).to_owned(),
            (if topology.primary { "p" } else { "s" }).to_owned(),
            topology
                .geometry
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                .unwrap_or_default(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        for native in &topology.native_order {
            feed(&mut hash, native);
            sep(&mut hash);
        }
    }
    for binding in &report.bindings {
        for part in [
            binding.logical_id.clone(),
            binding.native_id.clone(),
            binding.output.0.clone(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        flag(&mut hash, binding.pinned);
        flag(&mut hash, binding.displaced);
        flag(&mut hash, binding.managed);
    }
    for domain in &report.domains {
        for part in [
            domain.domain.id.0.clone(),
            domain.domain.workspace.0.clone(),
            domain.domain.bounds.x.to_string(),
            domain.domain.bounds.y.to_string(),
            domain.domain.bounds.w.to_string(),
            domain.domain.bounds.h.to_string(),
            domain.domain.gap.to_string(),
            domain.outer_gap.to_string(),
            domain.focused.0.clone(),
            (if domain.readable { "r" } else { "h" }).to_owned(),
            format!("{:?}", domain.hold),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        let mut adjacent: Vec<_> = domain.domain.adjacent.iter().collect();
        adjacent.sort_by_key(|(direction, _)| format!("{direction:?}"));
        for (direction, target) in adjacent {
            feed(&mut hash, &format!("{direction:?}"));
            sep(&mut hash);
            feed(&mut hash, &target.0);
            sep(&mut hash);
        }
        for window in &domain.windows {
            for part in [
                window.window.0.clone(),
                window.output.0.clone(),
                window.workspace.0.clone(),
                window.rect.x.to_string(),
                window.rect.y.to_string(),
                window.rect.w.to_string(),
                window.rect.h.to_string(),
            ] {
                feed(&mut hash, &part);
                sep(&mut hash);
            }
            flag(&mut hash, window.floating);
            flag(&mut hash, window.fit_excluded);
            flag(&mut hash, window.fullscreen);
            flag(&mut hash, window.maximized);
            flag(&mut hash, window.sticky);
            flag(&mut hash, window.fixed_auto);
            flag(&mut hash, window.fixed_suppress);
            for bound in [
                window.hints.min_w,
                window.hints.min_h,
                window.hints.max_w,
                window.hints.max_h,
            ] {
                feed(&mut hash, &bound.map(|v| v.to_string()).unwrap_or_default());
                sep(&mut hash);
            }
        }
        for id in &domain.closed {
            feed(&mut hash, &id.0);
            sep(&mut hash);
        }
    }
    for feedback in &report.feedback {
        feed(&mut hash, &feedback.operation_id);
        sep(&mut hash);
        feed(&mut hash, &format!("{:?}", feedback.kind));
        sep(&mut hash);
        feed(&mut hash, &format!("{:?}", feedback.outcome));
        sep(&mut hash);
        feed(&mut hash, &format!("{:?}", feedback.hold));
        sep(&mut hash);
    }
    hash
}

/// Validate a world report shape without echoing payload bytes. Every
/// carried string is gated (opaque ids, generation token, closed direction
/// vocabularies, bounded numeric ranges) and every section is capped, so
/// the estimators and the transition can rely on bounded input. Explicit
/// closes are ALWAYS allowed (even with incomplete inventory); only implicit
/// missing=closed inference requires completeness, and that inference lives
/// in the transition, not here.
#[must_use]
pub fn validate_report(report: &WorldReport) -> bool {
    validate_report_reason(report).is_none()
}

/// Name the failed [`validate_report`] check in fixed vocabulary (never
/// payload bytes): `None` when the report is valid, otherwise the static
/// reason token the rejection carries as its message under kind
/// `snapshot-invalid`, so one log line diagnoses the report.
#[must_use]
pub fn validate_report_reason(report: &WorldReport) -> Option<&'static str> {
    if report.contract_version != WORLD_CONTRACT_VERSION {
        return Some("version-mismatch");
    }
    if !is_opaque_id(&report.owner)
        || !is_opaque_id(&report.adapter_incarnation)
        || !is_opaque_id(&report.request_id)
    {
        return Some("identity-invalid");
    }
    if !crate::ids::is_generation_id(&report.core_generation) {
        return Some("generation-invalid");
    }
    if report.domains.len() > MAX_WORLD_DOMAINS
        || report.topology.len() > MAX_WORLD_OUTPUTS
        || report.bindings.len() > MAX_WORLD_BINDINGS
        || report.feedback.len() > MAX_WORLD_FEEDBACK
    {
        return Some("section-over-cap");
    }
    if report.command_seq.is_none() != report.command.is_none() {
        return Some("command-pair-invalid");
    }
    if let Some(command) = report.command.as_ref()
        && !valid_command(command)
    {
        return Some("command-invalid");
    }
    let mut seen_windows = BTreeSet::new();
    let mut total_windows = 0usize;
    let mut total_closed = 0usize;
    let mut seen_closed = BTreeSet::new();
    for domain in &report.domains {
        if !domain.domain.validate() {
            return Some("domain-invalid");
        }
        if !is_opaque_id(&domain.domain.id.0) || !is_opaque_id(&domain.domain.workspace.0) {
            return Some("domain-id-invalid");
        }
        if !valid_carried_rect(
            domain.domain.bounds.x,
            domain.domain.bounds.y,
            domain.domain.bounds.w,
            domain.domain.bounds.h,
        ) {
            return Some("domain-bounds-invalid");
        }
        if !is_gap(domain.domain.gap) || !is_gap(domain.outer_gap) {
            return Some("gap-invalid");
        }
        if domain.domain.adjacent.len() > 4 {
            return Some("adjacent-invalid");
        }
        for target in domain.domain.adjacent.values() {
            if !is_opaque_id(&target.0) {
                return Some("adjacent-invalid");
            }
        }
        if !domain.focused.0.is_empty() && !is_opaque_id(&domain.focused.0) {
            return Some("focused-id-invalid");
        }
        if domain.windows.len() > MAX_WORLD_WINDOWS || domain.closed.len() > MAX_WORLD_CLOSED {
            return Some("domain-entry-over-cap");
        }
        for window in &domain.windows {
            if !is_opaque_id(&window.window.0)
                || !is_opaque_id(&window.output.0)
                || !is_opaque_id(&window.workspace.0)
            {
                return Some("window-id-invalid");
            }
            if !valid_carried_rect(window.rect.x, window.rect.y, window.rect.w, window.rect.h) {
                return Some("window-rect-invalid");
            }
            // Size hints are advisory only: zero, negative, contradictory,
            // and out-of-bound values (Qt 0x0 default minimums,
            // QWIDGETSIZE_MAX / i32::MAX unbounded maximums) sanitize to
            // absent at every use site (see
            // `crate::size_hints::WindowSizeHints`), never fail the
            // report. A single valid native window reporting such default
            // or sentinel hints invalidates the whole world when gated
            // here, so every DescribeWorld carrying one was refused.
            if !seen_windows.insert(window.window.0.clone()) {
                return Some("duplicate-window");
            }
            total_windows += 1;
        }
        for id in &domain.closed {
            if !is_opaque_id(&id.0) {
                return Some("closed-id-invalid");
            }
            // Closed ids must not also be live: one id cannot be both
            // observed and reported closed.
            if seen_windows.contains(&id.0) {
                return Some("closed-overlaps-live");
            }
            if !seen_closed.insert(id.0.clone()) {
                return Some("duplicate-closed");
            }
            total_closed += 1;
        }
        if domain.readable != domain.hold.is_none() {
            return Some("readable-hold-mismatch");
        }
    }
    if total_windows > MAX_WORLD_WINDOWS || total_closed > MAX_WORLD_CLOSED {
        return Some("inventory-over-cap");
    }
    // Explicit minimized lifetimes: opaque homing, no duplicates, never
    // also live (a minimized window is absent from domain membership) and
    // never also explicitly closed (contradictory input refuses).
    {
        let mut seen_minimized = BTreeSet::new();
        for entry in &report.minimized {
            if !is_opaque_id(&entry.window.0)
                || !is_opaque_id(&entry.output.0)
                || !is_opaque_id(&entry.workspace.0)
            {
                return Some("minimized-id-invalid");
            }
            if seen_windows.contains(&entry.window.0) || seen_closed.contains(&entry.window.0) {
                return Some("minimized-overlaps");
            }
            if !seen_minimized.insert(entry.window.0.clone()) {
                return Some("duplicate-minimized");
            }
        }
        if report.minimized.len() > MAX_WORLD_WINDOWS {
            return Some("minimized-over-cap");
        }
    }
    let mut total_native = 0usize;
    for topology in &report.topology {
        if !is_opaque_id(&topology.output.0)
            || !is_opaque_id(&topology.current_native)
            || topology.native_order.len() > MAX_NATIVE_PER_OUTPUT
        {
            return Some("topology-invalid");
        }
        for native in &topology.native_order {
            if !is_opaque_id(native) {
                return Some("native-id-invalid");
            }
        }
        total_native += topology.native_order.len();
    }
    if total_native > MAX_WORLD_OUTPUTS.saturating_mul(MAX_NATIVE_PER_OUTPUT) {
        return Some("topology-over-cap");
    }
    for binding in &report.bindings {
        if !is_opaque_id(&binding.logical_id)
            || !is_opaque_id(&binding.native_id)
            || !is_opaque_id(&binding.output.0)
        {
            return Some("binding-invalid");
        }
    }
    for feedback in &report.feedback {
        if !is_opaque_id(&feedback.operation_id) {
            return Some("feedback-invalid");
        }
    }
    None
}

/// Validate one world command: every carried id is opaque, every direction
/// token is in a closed vocabulary, every index/delta is in range, and
/// every rect/coordinate is within carried bounds.
#[must_use]
pub fn valid_command(command: &WorldCommand) -> bool {
    const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];
    match command {
        WorldCommand::Focus { window, direction } => {
            is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str())
        }
        WorldCommand::Send {
            window,
            target_output,
            target_workspace,
            ..
        } => {
            is_opaque_id(&window.0)
                && is_opaque_id(&target_output.0)
                && is_opaque_id(target_workspace)
        }
        WorldCommand::SwitchView { output, workspace }
        | WorldCommand::SelectWorkspace { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(workspace)
        }
        WorldCommand::SelectNumbered { output, index } => {
            is_opaque_id(&output.0) && (1..=9).contains(index)
        }
        WorldCommand::SelectRelative { output, delta } => {
            is_opaque_id(&output.0) && (*delta == -1 || *delta == 1)
        }
        WorldCommand::SelectPrevious { output } | WorldCommand::SelectTrailing { output } => {
            is_opaque_id(&output.0)
        }
        WorldCommand::SendRelative {
            window,
            output,
            delta,
            ..
        } => is_opaque_id(&window.0) && is_opaque_id(&output.0) && (*delta == -1 || *delta == 1),
        WorldCommand::SendNumbered {
            window,
            output,
            index,
            ..
        } => is_opaque_id(&window.0) && is_opaque_id(&output.0) && *index <= 9,
        WorldCommand::SetWorkspaceTiling {
            output, workspace, ..
        } => is_opaque_id(&output.0) && is_opaque_id(workspace),
        WorldCommand::MigrateWorkspace {
            source_output,
            target_output,
            workspace,
        } => {
            is_opaque_id(&source_output.0)
                && is_opaque_id(&target_output.0)
                && is_opaque_id(workspace)
                && source_output != target_output
        }
        WorldCommand::FocusDirectional {
            window,
            direction,
            ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::MoveDirectional {
            window,
            direction,
            ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::SendDirectional {
            window,
            direction,
            ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::MigrateDirectional {
            source_output,
            workspace,
            direction,
        } => {
            is_opaque_id(&source_output.0)
                && is_opaque_id(workspace)
                && DIRECTIONS.contains(&direction.as_str())
        }
        WorldCommand::Operation(command) => valid_operation(command),
    }
}

/// Validate one operation payload without executing it.
#[must_use]
pub fn valid_operation(command: &CoreCommand) -> bool {
    const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];
    const RESIZE_MODES: [&str; 4] = ["inwards", "outwards", "horizontal", "vertical"];
    match command {
        CoreCommand::Reconcile
        | CoreCommand::UpdateGaps
        | CoreCommand::ActiveGroup
        | CoreCommand::ReleaseDomain => true,
        CoreCommand::Move {
            window, direction, ..
        }
        | CoreCommand::Focus {
            window, direction, ..
        } => is_opaque_id(window) && DIRECTIONS.contains(&direction.as_str()),
        CoreCommand::Resize {
            window,
            direction,
            mode,
            ..
        } => {
            is_opaque_id(window)
                && DIRECTIONS.contains(&direction.as_str())
                && RESIZE_MODES.contains(&mode.as_str())
        }
        CoreCommand::PointerResize {
            window,
            direction,
            boundary,
            direction2,
            boundary2,
        } => {
            is_opaque_id(window)
                && DIRECTIONS.contains(&direction.as_str())
                && (-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(boundary)
                && direction2
                    .as_deref()
                    .is_none_or(|d| DIRECTIONS.contains(&d))
                && boundary2.is_none_or(|b| (-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&b))
                && direction2.is_some() == boundary2.is_some()
        }
        CoreCommand::ToggleFloat { window, float_rect } => {
            is_opaque_id(window)
                && float_rect.is_none_or(|r| valid_carried_rect(r.x, r.y, r.w, r.h))
        }
        CoreCommand::ToggleOrientation { window } => is_opaque_id(window),
        CoreCommand::SendToWorkspace {
            window,
            target_output,
            target_workspace,
            ..
        }
        | CoreCommand::SendToOutput {
            window,
            target_output,
            target_workspace,
            ..
        } => is_opaque_id(window) && is_opaque_id(target_output) && is_opaque_id(target_workspace),
        CoreCommand::MigrateWorkspace { direction } => DIRECTIONS.contains(&direction.as_str()),
        CoreCommand::DragDrop {
            window,
            x,
            y,
            source,
            ..
        }
        | CoreCommand::DragPreview {
            window,
            x,
            y,
            source,
            ..
        } => {
            is_opaque_id(window)
                && (-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(x)
                && (-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(y)
                && source
                    .as_ref()
                    .is_none_or(|key| is_opaque_id(&key.output.0) && is_opaque_id(&key.workspace.0))
        }
    }
}

#[cfg(test)]
#[path = "world_report_tests.rs"]
mod tests;
