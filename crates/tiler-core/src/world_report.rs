//! Portable world report/reply contract (D1-D5).
//!
//! Serde-free typed envelope for the full-state boundary in
//! `docs/changes/archive/core-full-state-replies.md`. No transport, JSON, platform,
//! or process imports. Reuses the existing typed carriers
//! ([`crate::directional`] ids, [`crate::geometry::Rect`]) rather than lossy
//! clones, so the transition in [`crate::world_engine`] and the protocol
//! codec share one shape.
//!
//! The [`WorldEngine`] owns the transition; this module owns shapes,
//! validation, and freshness. Sizing and the canonical encoding live in
//! [`crate::world_sizing`] and re-export here, so downstream paths
//! (`world_engine`, protocol codec) keep one stable import root.
//!
//! Contract version 2 is logical-first: the core owns stable logical
//! workspace ids, and screens (FULL rectangles plus work areas) with window
//! facts are the only observation prerequisites. Native workspace domains
//! are an explicit capability-named opt-in observation, never authority.

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
use crate::session::{DomainKey, DragHoverPrior};
use crate::workspace_scope::WorkspaceMode;

/// World envelope version (single source for the wire `v`). Version 2 is
/// the logical-first contract: stable core workspace ids authorize mapping,
/// screens plus window facts are the only observation prerequisites, and
/// native workspace domains are an explicit capability-named opt-in.
pub const WORLD_CONTRACT_VERSION: u32 = 2;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;

/// Hard input caps so the worst-case budget is bounded before any work.
pub const MAX_WORLD_SCREENS: usize = 16;
pub const MAX_WORLD_FACTS: usize = 10_000;
/// Bound on explicit-close ids per report (same lifetime alphabet as windows).
pub const MAX_WORLD_CLOSED: usize = 10_000;
pub const MAX_WORLD_VIEWS: usize = 16;
/// Bound on host-owned native observations per report (only carried when the
/// `host_owned_workspace_domains` capability is set).
pub const MAX_WORLD_NATIVE: usize = 4096;
pub const MAX_NATIVE_PER_OUTPUT: usize = 256;
pub const MAX_WORLD_FEEDBACK: usize = 1024;

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

/// Scope key for one desired workspace row: the owning output id, except in
/// shared mode where every workspace shares one scope.
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
    /// Whole-workspace migration: one live logical workspace id moves scope.
    /// `target_workspace` on the [`TransferTarget`] names the migrated id.
    Migration,
}

/// Retained transfer/view target (D3=A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferTarget {
    pub operation_id: String,
    pub kind: TransferKind,
    pub target_output: OutputId,
    /// Stable core logical workspace id.
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
    /// Stable core logical workspace id homing this window.
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

/// Desired workspace row. `id` is the stable core logical workspace id;
/// `output` is its owning screen. `existence=false` is removal ELIGIBILITY
/// only (the adapter re-enumerates occupants, pins and minimum count before
/// deleting; unknown occupancy keeps). `native_binding` carries a host-owned
/// binding only when the `host_owned_workspace_domains` capability is set;
/// it is always `None` in the default contract and never authorizes mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDesire {
    pub id: String,
    pub output: OutputId,
    pub native_binding: Option<String>,
    /// Owning scope: the output id.
    pub scope: String,
    pub order: usize,
    pub tiling_mode: TilingMode,
    pub existence: bool,
    pub hold: Option<HoldReason>,
}

/// Desired output row: `logical_view` is the persistent core-owned view
/// target (the stable logical workspace id shown). Observed focus is
/// informational only, never a focus command. `tiling_mode` is the
/// retained planner-owned mode of the current logical view (default
/// tiled), so the adapter can see floating outputs.
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

/// One screen observation: the stable output id, the FULL native output
/// rectangle in one coherent adapter coordinate space, and the distinct
/// work area the core tiles into. `gap`/`outer_gap` are settings carried
/// per screen. `None` rectangles stay representable for explicitly
/// unreadable/partial observations; a report claiming complete topology
/// carries valid FULL and work-area rectangles for every screen, and
/// anything else narrowly fences topology-dependent crossing, migration,
/// and remap (never an inferred work area, never a window frame proxy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenReport {
    pub output: OutputId,
    pub full_rect: Option<Rect>,
    pub work_area: Option<Rect>,
    pub gap: i32,
    pub outer_gap: i32,
    pub complete: bool,
    pub primary: bool,
}

/// One truthful window fact: the stable window lifetime, the screen where
/// the adapter observes it, its frame, and every safety flag. `rect=None`
/// is an explicitly unreadable frame: the homed scope holds instead of
/// authorizing geometry, never a silent absence. `minimized=true` marks a
/// lifetime the adapter holds minimized: the core homes it on its retained
/// logical workspace with no managed geometry (never silent absence,
/// never inferred close). Core logical workspaces authorize homing, never
/// native workspace vocabulary.
/// Optional membership evidence (never authority by itself):
/// `observed_workspace` is an adapter-observed logical workspace id for
/// this window (facade readback / mechanical application receipt),
/// distinct from core desire. Host-native window membership (if any) is
/// translated by future ports into this observed logical receipt; there is
/// no separate host-native per-window field in the default contract.
/// `None` means unknown, never a proof of absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFact {
    pub window: WindowId,
    pub output: OutputId,
    pub rect: Option<Rect>,
    pub floating: bool,
    pub fit_excluded: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
    pub hints: crate::size_hints::WindowSizeHints,
    pub minimized: bool,
    pub observed_workspace: Option<String>,
}

/// Observed applied logical view per screen: the stable logical workspace id
/// independently verified as shown. Never a copy of a desired view or reply
/// acknowledgement. Empty means unknown: the core keeps its retained view and
/// the row carries no view proof. View transfers and migration refill require
/// an observed receipt exactly naming the expected logical id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenView {
    pub output: OutputId,
    pub view: String,
}

/// Host-owned native domain observation (optional, capability-gated): the
/// adapter-observed native workspace order and current native view for one
/// screen. Pure observation, never mapping authority: core logical
/// workspaces authorize all homing, lifecycle, and commands. Carrying any
/// entry without the `host_owned_workspace_domains` capability refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeObservation {
    pub output: OutputId,
    pub native_order: Vec<String>,
    pub current_native: String,
}

/// One persisted logical workspace ring: the stable output id plus its
/// ordered stable logical workspace ids, as retained by the adapter across
/// a restart. Neutral bootstrap facts: no native domain authority, no
/// applied member/view receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredRing {
    pub output: OutputId,
    pub workspaces: Vec<String>,
}

/// One persisted per-workspace tiling mode: the stable logical workspace on
/// its owning screen plus whether the adapter retains it tiled. Absent
/// entries seed from the reported saved default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredTiling {
    pub output: OutputId,
    pub workspace: String,
    pub tiled: bool,
}

/// One persisted member homing: the opaque window lifetime plus its retained
/// logical workspace on the observed screen. The adapter binds the token to
/// its live native lifetime; the core never verifies identity here. All
/// classification (floating, sticky, minimized, fullscreen, hidden) rides
/// the window facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredMember {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: String,
}

/// Bootstrap restoration ingress: persisted rings, current views,
/// per-workspace tiling, and member homing adopted on the first accepted
/// report of an incarnation only. Restored members are retained (never
/// fresh placement, admission, or focus): genuinely new lifetimes after the
/// restoration bootstrap still focus once through the ordinary
/// admission-focus path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredWorld {
    pub rings: Vec<RestoredRing>,
    pub views: Vec<ScreenView>,
    pub tiling: Vec<RestoredTiling>,
    pub members: Vec<RestoredMember>,
}

/// Bound on restored outputs per report (mirrors the screen cap).
pub const MAX_RESTORED_RINGS: usize = MAX_WORLD_SCREENS;
/// Bound on restored workspace ids per output.
pub const MAX_RESTORED_PER_RING: usize = 256;
/// Bound on restored per-workspace tiling entries per report.
pub const MAX_RESTORED_TILING: usize = 4096;
/// Bound on restored member homings per report (mirrors the fact cap).
pub const MAX_RESTORED_MEMBERS: usize = MAX_WORLD_FACTS;

/// Explicit world commands. At most one per report; observation-only
/// reports carry none. Every variant consumes one `command_seq` (bound to
/// the adapter incarnation/core generation high-watermark) and is retained
/// or spent exactly like the original three: selection/view commands retain
/// a [`TransferKind::View`] or [`TransferKind::Migration`] target,
/// `Send`/`SendRelative`/`SendNumbered`-to-bound-target and send operations
/// retain a window transfer, focus operations emit one at-most-once effect,
/// `SetWorkspaceTiling` retains planner-owned tiling mode, and read-only
/// operations reconcile only.
///
/// Every `workspace` string names a stable core LOGICAL workspace id owned
/// by `output` (never a native desktop name): the single world route speaks
/// logical authority in both directions, and native workspaces stay
/// capability-gated observations that never authorize commands.
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
    /// or create the trailing workspace immediately, exactly like
    /// lifecycle). Commits an ordinary transfer and never view-switches,
    /// including `follow=false` stays.
    SendNumbered {
        window: WindowId,
        output: OutputId,
        index: u8,
        follow: bool,
    },
    /// Planner-owned per-workspace tiling toggle (legacy Meta+Y parity):
    /// retain `tiled` for one live logical workspace. Disabling releases
    /// the retained session (no frame writes while floating); re-enabling
    /// re-adopts through the fresh route. Never decided by the adapter.
    SetWorkspaceTiling {
        output: OutputId,
        workspace: String,
        tiled: bool,
    },
    /// Whole-workspace migration (D2): move one live logical workspace id
    /// from the source scope to the target scope. Retained until member
    /// arrival plus both view proofs confirm it.
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
/// screens, window facts, and view echoes. The default contract needs no
/// native workspace concept at all: screens (FULL rectangles plus work
/// areas) and window facts bootstrap and operate the whole logical model.
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
    /// Logical workspace scope mode for this report: per-output-local
    /// (default), global-unique (scoped numbering with visible-elsewhere
    /// swap), or shared (one ring/history across all outputs). The mode is
    /// authoritative for ring, view, history, hotplug, startup, and migration
    /// policy on this report; changing it across reports re-scopes policy
    /// without rewriting history beyond the mode rules.
    pub workspace_mode: WorkspaceMode,
    /// Screen observations: FULL rectangles plus work areas, completeness,
    /// and the active (primary) screen. Core topology authority.
    pub screens: Vec<ScreenReport>,
    /// Truthful window facts: observed screen affinity, frames, flags, and
    /// minimized state. Core logical workspaces authorize all homing.
    pub windows: Vec<WindowFact>,
    /// Adapter-echoed logical views per screen (view proof for retained
    /// view transfers and migration refill). Empty means unknown.
    pub views: Vec<ScreenView>,
    /// Explicit closes. Always honored, even when the surrounding
    /// inventory is incomplete; only IMPLICIT missing=closed inference
    /// requires a provably complete inventory.
    pub closed: Vec<WindowId>,
    /// Capability opt-in: the adapter additionally observes host-owned
    /// workspace domains (see [`NativeObservation`]). Pure observation,
    /// never mapping authority. Off by default.
    pub host_owned_workspace_domains: bool,
    /// Capability opt-in: keep workspace counts equal across screens
    /// (neutral parity for native desktops spanning outputs). Off by
    /// default; replaces the superseded aligned-grid option.
    pub equal_workspace_count: bool,
    /// Host-owned native domain observations. Must be empty unless the
    /// `host_owned_workspace_domains` capability is set.
    pub native: Vec<NativeObservation>,
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
    /// newly created logical workspaces (bootstrap seeds all; live changes
    /// seed only subsequently created workspaces). Manual per-workspace
    /// toggles already retained never reseed. Backfills `true` when the
    /// adapter predates the field.
    pub default_tiled: bool,
    pub inventory_complete: bool,
    pub gesture_barrier: bool,
    pub feedback: Vec<AttemptFeedback>,
    /// Bootstrap restoration ingress (restart/persistence adoption): the
    /// adapter's persisted logical workspace rings, current views,
    /// per-workspace tiling modes, and member homing, as neutral opaque
    /// facts. `None` is a fresh bootstrap (core mints rings/views from the
    /// live screens); `Some` is a restoration bootstrap adopted
    /// transactionally on the first accepted report of an incarnation only.
    /// Later reports carrying restoration refuse as stale without mutation.
    /// Native identity/lifetime association stays adapter-mechanical (member
    /// windows are opaque tokens the adapter binds to its live lifetimes);
    /// core logical mapping is the sole homing authority. Floating, sticky,
    /// minimized, fullscreen, and hidden truth rides the window facts, never
    /// this ingress. Malformed or dangling restoration refuses as
    /// `snapshot-invalid` without mutation.
    pub restored: Option<RestoredWorld>,
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
/// sequences, owner, request/command identity, screens (output, FULL and
/// work-area rectangles, gaps, completeness, primary), facts (identity,
/// observed screen, frame, every safety flag, size-hint bounds, minimized
/// state), view echoes, explicit closes, capability flags, host-owned
/// native observations, inventory completeness, the default-tiling seed,
/// the migration source-refill mode, the D2/D3 capacity settings, the
/// global focus, the gesture barrier, and feedback (identity, kind,
/// outcome, hold).
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
    fn rect_part(rect: Option<Rect>) -> String {
        rect.map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
            .unwrap_or_default()
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
    flag(&mut hash, report.host_owned_workspace_domains);
    flag(&mut hash, report.equal_workspace_count);
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
    for screen in &report.screens {
        for part in [
            screen.output.0.clone(),
            rect_part(screen.full_rect),
            rect_part(screen.work_area),
            screen.gap.to_string(),
            screen.outer_gap.to_string(),
            (if screen.complete { "c" } else { "i" }).to_owned(),
            (if screen.primary { "p" } else { "s" }).to_owned(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
    }
    for fact in &report.windows {
        for part in [
            fact.window.0.clone(),
            fact.output.0.clone(),
            rect_part(fact.rect),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        flag(&mut hash, fact.floating);
        flag(&mut hash, fact.fit_excluded);
        flag(&mut hash, fact.fullscreen);
        flag(&mut hash, fact.maximized);
        flag(&mut hash, fact.sticky);
        flag(&mut hash, fact.fixed_auto);
        flag(&mut hash, fact.fixed_suppress);
        flag(&mut hash, fact.minimized);
        feed(
            &mut hash,
            fact.observed_workspace.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        for bound in [
            fact.hints.min_w,
            fact.hints.min_h,
            fact.hints.max_w,
            fact.hints.max_h,
        ] {
            feed(&mut hash, &bound.map(|v| v.to_string()).unwrap_or_default());
            sep(&mut hash);
        }
    }
    for view in &report.views {
        feed(&mut hash, &view.output.0);
        sep(&mut hash);
        feed(&mut hash, &view.view);
        sep(&mut hash);
    }
    for id in &report.closed {
        feed(&mut hash, &id.0);
        sep(&mut hash);
    }
    for native in &report.native {
        feed(&mut hash, &native.output.0);
        sep(&mut hash);
        feed(&mut hash, &native.current_native);
        sep(&mut hash);
        for id in &native.native_order {
            feed(&mut hash, id);
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
    match report.restored.as_ref() {
        None => flag(&mut hash, false),
        Some(restored) => {
            flag(&mut hash, true);
            for ring in &restored.rings {
                feed(&mut hash, &ring.output.0);
                sep(&mut hash);
                for id in &ring.workspaces {
                    feed(&mut hash, id);
                    sep(&mut hash);
                }
            }
            for view in &restored.views {
                feed(&mut hash, &view.output.0);
                sep(&mut hash);
                feed(&mut hash, &view.view);
                sep(&mut hash);
            }
            for entry in &restored.tiling {
                feed(&mut hash, &entry.output.0);
                sep(&mut hash);
                feed(&mut hash, &entry.workspace);
                sep(&mut hash);
                flag(&mut hash, entry.tiled);
            }
            for member in &restored.members {
                feed(&mut hash, &member.window.0);
                sep(&mut hash);
                feed(&mut hash, &member.output.0);
                sep(&mut hash);
                feed(&mut hash, &member.workspace);
                sep(&mut hash);
            }
        }
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
    if report.screens.len() > MAX_WORLD_SCREENS
        || report.windows.len() > MAX_WORLD_FACTS
        || report.closed.len() > MAX_WORLD_CLOSED
        || report.views.len() > MAX_WORLD_VIEWS
        || report.native.len() > MAX_WORLD_SCREENS
        || report.feedback.len() > MAX_WORLD_FEEDBACK
    {
        return Some("section-over-cap");
    }
    if !report.host_owned_workspace_domains && !report.native.is_empty() {
        return Some("native-without-capability");
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
    let mut seen_outputs = BTreeSet::new();
    for screen in &report.screens {
        if !is_opaque_id(&screen.output.0) {
            return Some("screen-id-invalid");
        }
        if !seen_outputs.insert(screen.output.0.clone()) {
            return Some("duplicate-screen");
        }
        if !is_gap(screen.gap) || !is_gap(screen.outer_gap) {
            return Some("gap-invalid");
        }
        match (screen.full_rect, screen.work_area) {
            (Some(full), Some(work)) => {
                if !valid_carried_rect(full.x, full.y, full.w, full.h)
                    || !valid_carried_rect(work.x, work.y, work.w, work.h)
                {
                    return Some("screen-rect-invalid");
                }
            }
            (None, None) => {}
            _ => return Some("screen-rect-partial"),
        }
        if screen.complete
            && (screen.full_rect.is_none()
                || screen.work_area.is_none()
                || screen
                    .full_rect
                    .is_some_and(|rect| !valid_carried_rect(rect.x, rect.y, rect.w, rect.h))
                || screen
                    .work_area
                    .is_some_and(|rect| !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)))
        {
            return Some("screen-complete-invalid");
        }
    }
    for fact in &report.windows {
        if !is_opaque_id(&fact.window.0) || !is_opaque_id(&fact.output.0) {
            return Some("window-id-invalid");
        }
        if !seen_outputs.contains(&fact.output.0) {
            return Some("window-output-unknown");
        }
        if let Some(rect) = fact.rect
            && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
        {
            return Some("window-rect-invalid");
        }
        // Size hints are advisory only: zero, negative, contradictory,
        // and out-of-bound values (Qt 0x0 default minimums,
        // QWIDGETSIZE_MAX / i32::MAX unbounded maximums) sanitize to
        // absent at every use site (see
        // `crate::size_hints::WindowSizeHints`), never fail the
        // report. A single valid window reporting such default or
        // sentinel hints invalidates the whole world when gated here,
        // so every report carrying one was refused.
        if !seen_windows.insert(fact.window.0.clone()) {
            return Some("duplicate-window");
        }
        if let Some(observed) = fact.observed_workspace.as_deref()
            && !is_opaque_id(observed)
        {
            return Some("window-observed-invalid");
        }
    }
    for id in &report.closed {
        if !is_opaque_id(&id.0) {
            return Some("closed-id-invalid");
        }
        // Closed ids must not also be live: one id cannot be both
        // observed and reported closed.
        if seen_windows.contains(&id.0) {
            return Some("closed-overlaps-live");
        }
    }
    {
        let mut seen_closed = BTreeSet::new();
        for id in &report.closed {
            if !seen_closed.insert(id.0.clone()) {
                return Some("duplicate-closed");
            }
        }
    }
    {
        let mut seen_views = BTreeSet::new();
        for view in &report.views {
            if !is_opaque_id(&view.output.0) {
                return Some("view-id-invalid");
            }
            if !seen_outputs.contains(&view.output.0) {
                return Some("view-output-unknown");
            }
            if !view.view.is_empty() && !is_opaque_id(&view.view) {
                return Some("view-target-invalid");
            }
            if !seen_views.insert(view.output.0.clone()) {
                return Some("duplicate-view");
            }
        }
    }
    let mut total_native = 0usize;
    for native in &report.native {
        if !is_opaque_id(&native.output.0)
            || !native.current_native.is_empty() && !is_opaque_id(&native.current_native)
            || native.native_order.len() > MAX_NATIVE_PER_OUTPUT
        {
            return Some("native-invalid");
        }
        if !seen_outputs.contains(&native.output.0) {
            return Some("native-output-unknown");
        }
        for id in &native.native_order {
            if !is_opaque_id(id) {
                return Some("native-id-invalid");
            }
        }
        total_native += native.native_order.len();
    }
    if total_native > MAX_WORLD_NATIVE {
        return Some("native-over-cap");
    }
    for feedback in &report.feedback {
        if !is_opaque_id(&feedback.operation_id) {
            return Some("feedback-invalid");
        }
    }
    if let Some(restored) = report.restored.as_ref() {
        let facts: BTreeSet<&str> = report
            .windows
            .iter()
            .map(|fact| fact.window.0.as_str())
            .collect();
        let closed: BTreeSet<&str> = report.closed.iter().map(|id| id.0.as_str()).collect();
        if let Some(reason) = validate_restored(restored, &facts, &closed) {
            return Some(reason);
        }
    }
    None
}

/// Validate bootstrap restoration ingress without echoing payload bytes:
/// every carried id is opaque, rings/views/tiling/members are cross-linked
/// (views, tiling, and members must name ring entries), and every restored
/// member must ride the report's window facts (a restored window that is
/// explicitly closed or unknown is stale). `None` when valid. Restored
/// outputs need not name reported screens: units homed on a disconnected
/// screen displace onto a survivor through the ordinary hotplug path.
/// First-report-only enforcement lives in the engine (`restored-stale`),
/// not here.
#[must_use]
pub fn validate_restored(
    restored: &RestoredWorld,
    facts: &BTreeSet<&str>,
    closed: &BTreeSet<&str>,
) -> Option<&'static str> {
    if restored.rings.is_empty() {
        return Some("restored-ring-invalid");
    }
    if restored.rings.len() > MAX_RESTORED_RINGS
        || restored.views.len() > MAX_WORLD_VIEWS
        || restored.tiling.len() > MAX_RESTORED_TILING
        || restored.members.len() > MAX_RESTORED_MEMBERS
    {
        return Some("section-over-cap");
    }
    let mut ringed: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut ring_outputs: BTreeSet<&str> = BTreeSet::new();
    for ring in &restored.rings {
        if !is_opaque_id(&ring.output.0) || !ring_outputs.insert(ring.output.0.as_str()) {
            return Some("restored-output-invalid");
        }
        if ring.workspaces.is_empty() || ring.workspaces.len() > MAX_RESTORED_PER_RING {
            return Some("restored-workspace-invalid");
        }
        for id in &ring.workspaces {
            if !is_opaque_id(id) || !ringed.insert((ring.output.0.as_str(), id.as_str())) {
                return Some("restored-workspace-invalid");
            }
        }
    }
    let mut view_outputs: BTreeSet<&str> = BTreeSet::new();
    for view in &restored.views {
        if !is_opaque_id(&view.output.0) || !view_outputs.insert(view.output.0.as_str()) {
            return Some("restored-view-invalid");
        }
        if !ring_outputs.contains(view.output.0.as_str()) {
            return Some("restored-view-unknown");
        }
        if view.view.is_empty() || !ringed.contains(&(view.output.0.as_str(), view.view.as_str())) {
            return Some("restored-view-invalid");
        }
    }
    let mut tiling_keys: BTreeSet<(&str, &str)> = BTreeSet::new();
    for entry in &restored.tiling {
        if !is_opaque_id(&entry.output.0) || !is_opaque_id(&entry.workspace) {
            return Some("restored-tiling-invalid");
        }
        if !ringed.contains(&(entry.output.0.as_str(), entry.workspace.as_str())) {
            return Some("restored-tiling-unknown");
        }
        if !tiling_keys.insert((entry.output.0.as_str(), entry.workspace.as_str())) {
            return Some("restored-tiling-duplicate");
        }
    }
    let mut member_windows: BTreeSet<&str> = BTreeSet::new();
    for member in &restored.members {
        if !is_opaque_id(&member.window.0)
            || !is_opaque_id(&member.output.0)
            || !is_opaque_id(&member.workspace)
        {
            return Some("restored-member-invalid");
        }
        if !ringed.contains(&(member.output.0.as_str(), member.workspace.as_str())) {
            return Some("restored-member-unknown");
        }
        if !member_windows.insert(member.window.0.as_str()) {
            return Some("restored-member-duplicate");
        }
        if closed.contains(member.window.0.as_str()) {
            return Some("restored-member-closed");
        }
        if !facts.contains(member.window.0.as_str()) {
            return Some("restored-member-unknown");
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
            window, direction, ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::MoveDirectional {
            window, direction, ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::SendDirectional {
            window, direction, ..
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
