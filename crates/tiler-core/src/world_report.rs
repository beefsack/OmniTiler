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
use crate::gesture::{GestureIngress, GestureSettled};
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
/// Bound on mechanical frame attempts per report (same lifetime alphabet as
/// windows): one latest-wins attempt per window at most, so the worst case
/// is bounded by the window inventory itself.
pub const MAX_WORLD_FRAMES: usize = 1024;
/// Bound on host-owned native observations per report (only carried when the
/// `host_owned_workspace_domains` capability is set).
pub const MAX_WORLD_NATIVE: usize = 4096;
pub const MAX_NATIVE_PER_OUTPUT: usize = 256;
/// Bound on mechanical membership attempts per report (one latest-wins row
/// per operation window at most).
pub const MAX_WORLD_MEMBER_ATTEMPTS: usize = 1024;
/// Bound on mechanical view attempts per report (one latest-wins row per
/// owning view phase at most).
pub const MAX_WORLD_VIEW_ATTEMPTS: usize = 1024;
/// Bound on mechanical maximize-clear attempts per report (one latest-wins
/// row per retained clear operation at most).
pub const MAX_WORLD_CLEAR_ATTEMPTS: usize = 1024;

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
    /// Set when the last evidence was possibly-executed: the target stays
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
    /// Explicit retained maximize-clear operation (`op-<cmdSeq>`) authorizing
    /// exactly one nonactivating clear for this window. `None` (empty on the
    /// wire) authorizes nothing. Set only while a retained directional clear
    /// is outstanding and undispatched for this window; cleared on dispatch
    /// (spent before the write), proven refusal, external supersession,
    /// close, newer intent, or generation reset. Never re-actuated.
    pub clear_operation: Option<String>,
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

/// Actuation attempt outcome with stable identity. Four states separate what
/// a boolean cannot: never tried, maybe ran (hold, never blindly repeat),
/// proven refusal (settle to observed/held), and external supersession
/// (newer user input wins). The core owns this latch vocabulary for
/// retained transfer/migration settlement (see [`crate::world_settle`]);
/// adapters never send verdicts, only raw mechanical attempts.
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

/// Mechanical outcome of one native frame setter call. Pure actuator
/// disposition, never a policy conclusion: `Accepted` means the setter
/// reported success (the caller still re-reads the frame; success alone
/// never proves geometry completion), `Refused` is an explicit native
/// refusal, and `Threw` is a throwing/ambiguous call that may have executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameSetter {
    Accepted,
    Refused,
    Threw,
}

impl FrameSetter {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Refused => "refused",
            Self::Threw => "threw",
        }
    }
}

/// One normalized raw frame attempt: the mechanical facts behind a single
/// native frame write, reported on the next report for the core-owned
/// anti-fighting policy (see [`crate::world_frames`]) and explicit
/// float-placement completion (see [`crate::world_engine::WorldEngine`]
/// retained floats settled in [`crate::world_settle`]). The adapter writes,
/// re-reads, and reports; the core compares desired and observed and owns
/// suppression, retry backoff, and completion. Never a policy conclusion:
/// no clamp/suppress verdict and no float outcome verdict ride here.
///
/// - `target` is the attempted frame (the core's published desire, echoed
///   as correlation so the core matches the attempt to its desire).
/// - `readback` is the immediate post-write frame (`None` = unreadable).
/// - `dpi_before`/`dpi_after` are the live window DPI around the write
///   (`None` = unreadable, never guessed). A changed basis invalidates the
///   placement: it never becomes a stable clamp and never confirms a float.
/// - `attempt_ms` is the adapter monotonic clock at the write, in the same
///   arbitrary-epoch milliseconds as the report `monotonic_ms`. The core
///   consumes each attempt idempotently by this stamp, so redelivery after
///   a lost acknowledgement never renews backoff.
/// - `operation_id` correlates an explicit float-placement write to its
///   retained core operation (`Some("op-<cmdSeq>")`, exact match on
///   operation id plus window plus target). `None` is an ordinary frame
///   write with no placement authority: it never settles a float.
/// - `prewrite` is the live frame immediately before the write
///   (`None` = unreadable). Only an explicit native refusal with an
///   unchanged readable preimage proves refusal; anything else latches
///   ambiguity. `Some` without an operation id is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameAttempt {
    pub window: WindowId,
    pub target: Rect,
    pub setter: FrameSetter,
    pub readback: Option<Rect>,
    pub dpi_before: Option<u32>,
    pub dpi_after: Option<u32>,
    pub attempt_ms: u64,
    pub operation_id: Option<String>,
    pub prewrite: Option<Rect>,
}

/// One normalized raw membership attempt: the mechanical facts behind a
/// single native membership write for one window under one retained
/// operation (window transfer or whole-workspace migration member). The
/// adapter checks mechanical authority/freshness/lifetime gates, writes,
/// re-reads homing, and reports; the core derives confirmation, proven
/// refusal, ambiguity, and external supersession from this evidence plus
/// the staged observation. Never a policy conclusion: no outcome verdict
/// rides here.
///
/// - `operation_id` is the retained transfer operation (window transfer
///   id, or the migration parent id for members). Exact match on operation
///   id plus window plus target.
/// - `target_output`/`target_workspace` echo the retained target as
///   correlation so the core matches the attempt to its desire.
/// - `setter` is the mechanical disposition (`Accepted` never proves
///   arrival by itself, `Refused` is an explicit native refusal, `Threw`
///   is a throwing/ambiguous call).
/// - `pre`/`post` are the live homings immediately before/after the write
///   (`None` = unreadable). Only an explicit refusal with an unchanged
///   readable preimage proves refusal; anything else latches ambiguity.
/// - `attempt_ms` is the adapter monotonic clock at the write, matching
///   [`FrameAttempt::attempt_ms`] idempotency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberAttempt {
    pub operation_id: String,
    pub window: WindowId,
    pub target_output: OutputId,
    pub target_workspace: String,
    pub setter: FrameSetter,
    pub pre: Option<(OutputId, String)>,
    pub post: Option<(OutputId, String)>,
    pub attempt_ms: u64,
}

/// One normalized raw view attempt: the mechanical facts behind a single
/// native view write for one owning phase (standalone view id, follow
/// parent, migration-target parent, or refill child id). Same
/// write/re-read/report split as [`MemberAttempt`]: the core derives every
/// verdict. Follow misses stay ambiguous parent membership, never a
/// refused window.
///
/// - `operation_id` is the owning phase id (the retained view id itself
///   for standalone/refill rows, the window parent for follow, the
///   migration parent for the target view).
/// - `pre`/`post` are the live exact-output views immediately
///   before/after the write (`None` = unreadable); `baseline` is the
///   observation-time view (`None` = unknown). Only an explicit refusal
///   with an unchanged readable preimage proves refusal; a readable post
///   that is neither the baseline nor the target proves newer external
///   input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewAttempt {
    pub operation_id: String,
    pub target_output: OutputId,
    pub target_workspace: String,
    pub setter: FrameSetter,
    pub pre: Option<String>,
    pub post: Option<String>,
    pub baseline: Option<String>,
    pub attempt_ms: u64,
}

/// One normalized raw maximize-clear attempt: the mechanical facts behind a
/// single native nonactivating clear for one retained directional clear
/// operation. The adapter checks mechanical authority/freshness/lifetime
/// gates, clears, re-reads maximized state, and reports; the core derives
/// confirmation (unmaximized proof), proven refusal, ambiguity, and external
/// supersession from this evidence plus the staged observation. Never a
/// policy conclusion: no outcome verdict rides here.
///
/// - `operation_id` is the retained clear operation (`op-<cmdSeq>`). Exact
///   match on operation id plus window.
/// - `pre_maximized`/`post_maximized` are the live maximized states
///   immediately before/after the write (`None` = unreadable). Only an
///   explicit refusal with an unchanged readable `Some(true)` preimage
///   proves refusal; anything else latches ambiguity.
/// - `attempt_ms` matches [`FrameAttempt::attempt_ms`] idempotency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearAttempt {
    pub operation_id: String,
    pub window: WindowId,
    pub setter: FrameSetter,
    pub pre_maximized: Option<bool>,
    pub post_maximized: Option<bool>,
    pub attempt_ms: u64,
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

/// Source-output intent resolved before this report changes retained homing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendSource {
    /// Genuinely semantic explicit output; passes through unchanged.
    Explicit(OutputId),
    /// Retained pre-observation home, else the full migration/control
    /// action context (foreground, pointer, cache, known-registry order).
    /// The CLI numbered/relative send route.
    HomeThenAction,
    /// Retained pre-observation home, else the cached output when live,
    /// else the first live screen in report order. The keyboard
    /// digit/relative send route.
    HomeThenCachedLive,
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
    /// `None` resolves the output in core from the cursor-first switch
    /// target; `Some` names a genuinely semantic explicit output.
    SelectNumbered { output: Option<OutputId>, index: u8 },
    /// Planner-resolved relative selection (D2): `delta` is `-1`/`+1` with
    /// first/last wrap over the scoped ring. Output resolution as above.
    SelectRelative {
        output: Option<OutputId>,
        delta: i32,
    },
    /// Planner-resolved previous-toggle selection (D2): no-op without
    /// recorded history, never recreates. Output resolution as above.
    SelectPrevious { output: Option<OutputId> },
    /// Trailing selection (D2): reuse the live trailing empty or create a
    /// stable pending logical token, exactly like lifecycle. Output
    /// resolution as above.
    SelectTrailing { output: Option<OutputId> },
    /// Relative send (D2): the scoped ring resolves once before transfer;
    /// filling a resolved trailing empty is an ordinary transfer. The
    /// source resolves in core from the [`SendSource`] intent: explicit
    /// outputs pass through unchanged, implicit sources resolve from the
    /// pre-observation retained home plus their fallback chain. Never an
    /// adapter output decision.
    SendRelative {
        window: WindowId,
        source: SendSource,
        delta: i32,
        follow: bool,
    },
    /// Numbered send: positional `0..=9` over the scoped send ring for the
    /// resolved source (`0` is the trailing path: reuse the live trailing
    /// empty or create the trailing workspace immediately, exactly like
    /// lifecycle). Commits an ordinary transfer; follow additionally selects
    /// the same-output target view while `follow=false` stays never move
    /// any view. Source resolution as above; explicit outputs bypass
    /// unchanged.
    SendNumbered {
        window: WindowId,
        source: SendSource,
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
    /// scopes refuse without mutation. Float subjects resolve through the
    /// world-owned float-focus lattice (candidates/target in core), never
    /// an adapter bypass.
    FocusDirectional { window: WindowId, direction: String },
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
    /// adapter names the direction only, never the source or the target.
    /// Core resolves the action source from the neutral report facts
    /// (foreground, pointer, cache, then known-registry order) plus the
    /// source's current logical view, then resolves the strict target and
    /// runs the world migration path. No resolvable source or target
    /// refuses without mutation. Genuine explicit migrations route
    /// [`WorldCommand::MigrateWorkspace`] with both scopes named.
    MigrateDirectional { direction: String },
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
    /// Ingress cursor-first targeting (2026-10-20): selects the output
    /// numbered/relative/previous/trailing selections act on
    /// (cursor-output default, focused-output option). Applies to
    /// subsequent selections only, never to retained views.
    pub switch_target: crate::cursor_targeting::SwitchTarget,
    /// Ingress fresh-window placement (2026-10-20): selects the output and
    /// anchor for truly new lifetimes (cursor-output/last-focused default,
    /// cursor-output/cursor-closest, or focused-output/last-focused).
    /// Applies to subsequent fresh admissions only: never moves existing
    /// members and never reflows, retries, or reclassifies retained state
    /// on change. Reports before any accepted complete adoption adopt
    /// native affinity (startup/restart/rebind quiet).
    pub fresh_placement: crate::cursor_targeting::FreshPlacement,
    /// Ingress focus-follows-cursor setting (default off). The stage-4b
    /// core policy mints one hover-coupling focus effect per report while
    /// this is on (see [`crate::world_cursor`]).
    pub focus_follows_cursor: bool,
    /// Ingress cursor-warp setting (default off). The stage-4b core policy
    /// publishes the cross-output warp point while this is on (see
    /// [`crate::world_cursor`]).
    pub cursor_warp: bool,
    /// Normalized cursor position in adapter units (same space as screen
    /// FULL rectangles and fact frames), `None` when the adapter cannot
    /// read it. Seated with `observation_seq`: retained per accepted
    /// report with the behavior settings and focused facts above, never
    /// on a separate stale timer. Cursor lookup uses FULL bounds; an
    /// absent cursor or a point outside every screen falls back through
    /// the focused output to the primary/first readable output.
    pub cursor: Option<(i32, i32)>,
    /// Normalized cursor-hit managed lifetime: the exact managed window
    /// under the reported cursor at observation time, resolved by the
    /// adapter through native hit testing plus exact managed-lifetime
    /// binding (never a rectangle guess, never platform focus policy).
    /// `None` when the cursor is unreadable, during the focused
    /// fullscreen-game lane, or when the point lands on foreign, hidden,
    /// or otherwise unmanaged pixels. The core treats an absent hit as no
    /// hover: only this fact plus the core's own eligibility and current
    /// view choose focus-follows-cursor. A hit naming no reported window,
    /// or any hit without a cursor, refuses the report.
    pub cursor_hit: Option<WindowId>,
    /// Global active window id ("" when none): the adapter-observed
    /// foreground lifetime across all outputs. Drives focus-conditioned
    /// policy (hotplug survivor/return views); never focus proof.
    pub focused_global: String,
    /// Neutral migration/control action foreground: the adapter-grounded
    /// output of the live foreground window's frame (rect grounding, same
    /// rule as observed affinity, mapped to wire), `None` without a
    /// readable foreground frame or on an unmapped device. Core filters it
    /// to the known registry and applies it as the first action-context
    /// leg (foreground, pointer, cache, registry order); never focus proof
    /// and never an adapter output decision.
    pub action_foreground: Option<OutputId>,
    /// Neutral migration/control action cache: the adapter's cached output
    /// context (wire id, `None` when empty or unmapped). Core applies it as
    /// the third action-context leg after foreground and pointer; a mapped
    /// but removed entry still selects from the retained registry and
    /// refuses downstream instead of retargeting. Never an adapter output
    /// decision.
    pub action_cached: Option<OutputId>,
    /// Neutral ordered known-output registry for the migration/control
    /// action context: every native registry key the adapter remembers
    /// (including disconnected mappings), sorted in native registry order
    /// and translated to wire. The core filters every action-context leg
    /// through this vector with first-entry fallback, exactly the retired
    /// platform selection universe; the adapter never preselects. Empty
    /// only when the adapter knows no output at all.
    pub action_outputs: Vec<OutputId>,
    /// Saved default tiling seed (legacy `defaultTiled` parity): seeds
    /// newly created logical workspaces (bootstrap seeds all; live changes
    /// seed only subsequently created workspaces). Manual per-workspace
    /// toggles already retained never reseed. Backfills `true` when the
    /// adapter predates the field.
    pub default_tiled: bool,
    pub inventory_complete: bool,
    pub gesture_barrier: bool,
    /// Normalized gesture lifecycle ingress (stage 4a): `None` on
    /// observation-only reports. While a native move/size gesture is open
    /// the adapter reports exactly one phase per report (started, updated
    /// with the live cursor, released, or cancelled), every phase carrying
    /// the stable nonzero gesture id allocated at native START plus the
    /// START down point. The core retains the session, resolves the
    /// pointer target, publishes the drop-overlay shape, and commits the
    /// release over the existing drag/resize machinery, ordering sessions
    /// and at-most-once terminals by id. The adapter only observes,
    /// captures, renders, and applies. A missing ingress while a session
    /// is retained holds the session (never an implicit cancel); an
    /// explicit close or a cancel phase clears it (a hidden, incomplete,
    /// or minimized departure holds, never a false close).
    pub gesture: Option<GestureIngress>,
    /// Neutral adapter monotonic clock in milliseconds (arbitrary process
    /// epoch, never wall-clock): the observation time of this report. Times
    /// frame retry backoff together with the per-attempt stamps in
    /// [`FrameAttempt::attempt_ms`]; `0` means unreported (unknown). A
    /// separate sample per report only, never a continuous poll. Duplicate
    /// detection still compares full typed input equality; the clock never
    /// aliases reports by itself.
    pub monotonic_ms: u64,
    /// Mechanical frame attempts since the previous report (at most one
    /// latest-wins entry per window, bounded by [`MAX_WORLD_FRAMES`]).
    /// Raw write/readback facts only; the core owns the clamp/transient
    /// policy over them (see [`crate::world_frames`]).
    pub frames: Vec<FrameAttempt>,
    /// Mechanical membership attempts since the previous report (at most
    /// one latest-wins entry per operation window, bounded by
    /// [`MAX_WORLD_MEMBER_ATTEMPTS`]). Raw setter/pre/post homing facts
    /// only; the core owns window/migration transfer completion over them
    /// plus the staged observation (see [`crate::world_settle`]).
    pub member_attempts: Vec<MemberAttempt>,
    /// Mechanical view attempts since the previous report (at most one
    /// latest-wins entry per owning view phase, bounded by
    /// [`MAX_WORLD_VIEW_ATTEMPTS`]). Raw setter/pre/post/baseline facts
    /// only; the core owns standalone/follow/target/refill completion.
    pub view_attempts: Vec<ViewAttempt>,
    /// Mechanical maximize-clear attempts since the previous report (at most
    /// one latest-wins entry per retained clear operation, bounded by
    /// [`MAX_WORLD_CLEAR_ATTEMPTS`]). Raw setter/pre/post maximized facts
    /// only; the core owns directional clear completion over them plus the
    /// staged observation (see [`crate::world_settle`]).
    pub clear_attempts: Vec<ClearAttempt>,
}

/// Terminal retained-operation lifecycle outcome for one core-owned
/// operation: a directional maximize-clear binding or an explicit
/// float-placement target. Emitted once when the retained item retires
/// (refusal, abandonment, supersession, continuation, continuation refusal,
/// or frame-proven float completion), never retained across reports.
/// Pending desire rides the `clear_operation`/`frame_operation` row, actual
/// raw attempts ride the `world-apply` diag (`clears`/`frames`),
/// ambiguous/gap holds emit nothing (the binding persists with its desire
/// withheld). A `ClearContinuedTiled`/`ClearContinuedFloat` event means the
/// original clear intent resumed under the original op, never that the
/// final frame arrived; only `FloatFrameProven` means native observation
/// confirmed the retained float target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    ClearContinuedTiled,
    ClearContinuedFloat,
    ClearRefusedProven,
    ClearAbandonedExternalClear,
    ClearAbandonedFocusChanged,
    ClearAbandonedViewChanged,
    ClearAbandonedHomingChanged,
    ClearAbandonedClosed,
    ClearAbandonedFullscreen,
    ClearSuperseded,
    ClearContinuationRefused,
    FloatFrameProven,
    FloatRefusedProven,
    FloatAbandoned,
}

impl OperationOutcome {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClearContinuedTiled => "continued-tiled",
            Self::ClearContinuedFloat => "continued-float",
            Self::ClearRefusedProven => "refused-proven",
            Self::ClearAbandonedExternalClear => "abandoned-external-clear",
            Self::ClearAbandonedFocusChanged => "abandoned-focus-changed",
            Self::ClearAbandonedViewChanged => "abandoned-view-changed",
            Self::ClearAbandonedHomingChanged => "abandoned-homing-changed",
            Self::ClearAbandonedClosed => "abandoned-closed",
            Self::ClearAbandonedFullscreen => "abandoned-fullscreen",
            Self::ClearSuperseded => "superseded",
            Self::ClearContinuationRefused => "continuation-refused",
            Self::FloatFrameProven => "float-frame-proven",
            Self::FloatRefusedProven => "float-refused-proven",
            Self::FloatAbandoned => "float-abandoned",
        }
    }
}

/// One terminal retained-operation lifecycle event: the original operation
/// id plus window plus outcome (and a narrow cause for refusals and
/// abandonments). At most one per retired item per report, bounded by the
/// window inventory itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEvent {
    pub operation_id: String,
    pub window: WindowId,
    pub outcome: OperationOutcome,
    pub cause: Option<String>,
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
    /// Precise result of the gesture terminal incorporated by this report,
    /// if any: the stable gesture id plus whether the core committed a
    /// move/resize release. `None` when this report incorporated no
    /// terminal (observation-only, preview update, held session). Set per
    /// accepted report only; never retained across reports. Duplicate
    /// deliveries repeat the cached reply (and its terminal); the id plus
    /// the consumed watermark keep replays safe.
    pub gesture_settled: Option<GestureSettled>,
    /// Terminal retained-operation lifecycle events incorporated by
    /// this report (see [`OperationEvent`]): one per retired clear binding
    /// or float placement, never retained across reports. Empty on
    /// observation-only holds and on replies with no operation settlement.
    /// Duplicate deliveries repeat the cached reply (and its events).
    pub operation_settled: Vec<OperationEvent>,
    /// Desired cursor-warp point in adapter units (same space as screen
    /// FULL rectangles), `None` when no warp is desired on this report.
    /// Set only when `cursor_warp` is on, the inventory is complete, the
    /// cursor reads with a known cursor output, no focused-fullscreen-game
    /// or gesture pause holds, and a published focus effect targets a
    /// different output than the cursor output (cross-output only, the
    /// approved i3/sway preset behavior): the focused target's
    /// effective-frame centre clamped into its work area. The adapter
    /// applies it mechanically with fresh game/lifetime/focus proof, at
    /// most once per observation, and never under its own game lane.
    pub cursor_warp: Option<(i32, i32)>,
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
    /// World-owned directional decision (stage 5): the ordered refusal
    /// lattice, float-focus target, constrained half-snap rect, and
    /// maximize-clear need for one
    /// [`WorldCommand::FocusDirectional`]/[`WorldCommand::MoveDirectional`]
    /// dispatch. `outcome` is the exact snap vocabulary token the adapter
    /// logs 1:1; `target` carries the float-focus window when one was
    /// selected (focus) or the cleared window needing the single clear
    /// (need-clear) or the stale subject (unknown-window stale refusals);
    /// `rect` carries the authorized visible half-snap desire for a float
    /// move, dispatched once through the retained float lane: the token
    /// itself never proves application, only native observation confirms
    /// it; `cause` carries the narrow refusal cause when one applies.
    /// Tiled success carries no result: the snapshot, transfers, and focus
    /// effects ARE the outcome. A need-clear result publishes a one-shot
    /// `clear_operation` desire under the originating op; the adapter
    /// dispatches the native clear at most once and reports a correlated
    /// clear attempt, and the commandless core settlement resumes the
    /// original intent under the same op on proof.
    Directional {
        outcome: crate::world_directional::DirectionalOutcome,
        target: Option<WindowId>,
        rect: Option<Rect>,
        cause: Option<String>,
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
/// switch-target and fresh-placement modes, the focus-follows-cursor and
/// cursor-warp settings, the cursor position and cursor hit, the gesture
/// ingress, the global focus, the gesture barrier, the monotonic clock, frame attempts (window, target, setter, readback, DPI
/// pair, attempt stamp, placement operation id, pre-write frame), member
/// attempts (operation, window, target, setter, pre/post homing, stamp),
/// view attempts (owning operation, target, setter, pre/post/baseline
/// views, stamp), and clear attempts (operation, window, setter,
/// pre/post maximized states, stamp).
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
    feed(
        &mut hash,
        &report
            .action_foreground
            .as_ref()
            .map(|out| out.0.clone())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    feed(
        &mut hash,
        &report
            .action_cached
            .as_ref()
            .map(|out| out.0.clone())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    for output in &report.action_outputs {
        feed(&mut hash, &output.0);
        sep(&mut hash);
    }
    flag(&mut hash, report.gesture_barrier);
    if let Some(gesture) = report.gesture.as_ref() {
        feed(&mut hash, &gesture.window.0);
        sep(&mut hash);
        feed(&mut hash, &gesture.id.to_string());
        sep(&mut hash);
        feed(&mut hash, gesture.kind.as_str());
        sep(&mut hash);
        feed(&mut hash, gesture.phase.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &gesture
                .cursor
                .map(|(x, y)| format!("{x},{y}"))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &gesture
                .start_cursor
                .map(|(x, y)| format!("{x},{y}"))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, gesture.esc);
        flag(&mut hash, gesture.modal_move);
        feed(
            &mut hash,
            &format!(
                "{},{},{},{}",
                gesture.start_frame.x,
                gesture.start_frame.y,
                gesture.start_frame.w,
                gesture.start_frame.h
            ),
        );
        sep(&mut hash);
    } else {
        // Absent ingress still separates the digest from a present one.
        feed(&mut hash, "no-gesture");
        sep(&mut hash);
    }
    flag(&mut hash, report.focus_follows_cursor);
    flag(&mut hash, report.cursor_warp);
    feed(&mut hash, report.fixed_size_predicate.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.migration_source_refill.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.anchor_fallback.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.no_space_result.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.switch_target.as_wire_str());
    sep(&mut hash);
    feed(&mut hash, report.fresh_placement.as_wire_str());
    sep(&mut hash);
    feed(
        &mut hash,
        &report
            .cursor
            .map(|(x, y)| format!("{x},{y}"))
            .unwrap_or_default(),
    );
    sep(&mut hash);
    // The hit rides beside the cursor so a changed hit alone separates
    // the digest (duplicate detection still compares full typed input).
    feed(
        &mut hash,
        &report
            .cursor_hit
            .as_ref()
            .map(|hit| hit.0.clone())
            .unwrap_or_default(),
    );
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
    feed(&mut hash, &report.monotonic_ms.to_string());
    sep(&mut hash);
    for attempt in &report.frames {
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(
            &mut hash,
            &format!(
                "{},{},{},{}",
                attempt.target.x, attempt.target.y, attempt.target.w, attempt.target.h
            ),
        );
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .readback
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .dpi_before
                .map(|dpi| dpi.to_string())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .dpi_after
                .map(|dpi| dpi.to_string())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
        feed(
            &mut hash,
            attempt.operation_id.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .prewrite
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                .unwrap_or_default(),
        );
        sep(&mut hash);
    }
    for attempt in &report.member_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_workspace);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .pre
                .as_ref()
                .map(|(out, ws)| format!("{out}/{ws}", out = out.0))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post
                .as_ref()
                .map(|(out, ws)| format!("{out}/{ws}", out = out.0))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for attempt in &report.view_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_workspace);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(&mut hash, attempt.pre.as_deref().unwrap_or_default());
        sep(&mut hash);
        feed(&mut hash, attempt.post.as_deref().unwrap_or_default());
        sep(&mut hash);
        feed(&mut hash, attempt.baseline.as_deref().unwrap_or_default());
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for attempt in &report.clear_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .pre_maximized
                .map(|v| if v { "1" } else { "0" }.to_owned())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post_maximized
                .map(|v| if v { "1" } else { "0" }.to_owned())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
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
    if report.screens.len() > MAX_WORLD_SCREENS
        || report.windows.len() > MAX_WORLD_FACTS
        || report.closed.len() > MAX_WORLD_CLOSED
        || report.views.len() > MAX_WORLD_VIEWS
        || report.native.len() > MAX_WORLD_SCREENS
        || report.frames.len() > MAX_WORLD_FRAMES
        || report.member_attempts.len() > MAX_WORLD_MEMBER_ATTEMPTS
        || report.view_attempts.len() > MAX_WORLD_VIEW_ATTEMPTS
        || report.clear_attempts.len() > MAX_WORLD_CLEAR_ATTEMPTS
    {
        return Some("section-over-cap");
    }
    // The cursor rides the same carried geometry bounds as rects so the
    // estimators and the lookup stay exact over the full `i32` range.
    if let Some((x, y)) = report.cursor
        && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
            || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
    {
        return Some("cursor-invalid");
    }
    // The cursor hit is fact reporting, never policy: it must name a
    // reported window (a stale or foreign id proves nothing) and it
    // never rides without the cursor position it was resolved from.
    if let Some(hit) = report.cursor_hit.as_ref() {
        if report.cursor.is_none() {
            return Some("cursor-hit-without-cursor");
        }
        if !report.windows.iter().any(|fact| fact.window == *hit) {
            return Some("cursor-hit-unknown");
        }
    }
    if !report.host_owned_workspace_domains && !report.native.is_empty() {
        return Some("native-without-capability");
    }
    // Action-context facts are neutral adapter observations: opaque ids
    // only here. Registry membership filters at use (a mapped but
    // removed entry still selects and refuses downstream instead of
    // retargeting), so validation never retargets. The registry itself
    // carries opaque unique ids in adapter registry order.
    for output in [&report.action_foreground, &report.action_cached]
        .into_iter()
        .flatten()
    {
        if !is_opaque_id(&output.0) {
            return Some("action-output-invalid");
        }
    }
    {
        let mut seen_action_outputs = BTreeSet::new();
        for output in &report.action_outputs {
            if !is_opaque_id(&output.0) {
                return Some("action-outputs-invalid");
            }
            if !seen_action_outputs.insert(output.0.clone()) {
                return Some("duplicate-action-output");
            }
        }
    }
    if let Some(gesture) = report.gesture.as_ref() {
        if !is_opaque_id(&gesture.window.0) {
            return Some("gesture-window-invalid");
        }
        if gesture.id == 0 {
            return Some("gesture-id-invalid");
        }
        if let Some((x, y)) = gesture.cursor
            && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
                || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
        {
            return Some("gesture-cursor-invalid");
        }
        if let Some((x, y)) = gesture.start_cursor
            && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
                || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
        {
            return Some("gesture-cursor-invalid");
        }
        if !valid_carried_rect(
            gesture.start_frame.x,
            gesture.start_frame.y,
            gesture.start_frame.w,
            gesture.start_frame.h,
        ) {
            return Some("gesture-frame-invalid");
        }
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
    {
        let mut seen_frames = BTreeSet::new();
        for attempt in &report.frames {
            if !is_opaque_id(&attempt.window.0) {
                return Some("frame-window-invalid");
            }
            if !valid_carried_rect(
                attempt.target.x,
                attempt.target.y,
                attempt.target.w,
                attempt.target.h,
            ) {
                return Some("frame-target-invalid");
            }
            if let Some(readback) = attempt.readback
                && !valid_carried_rect(readback.x, readback.y, readback.w, readback.h)
            {
                return Some("frame-readback-invalid");
            }
            if let Some(prewrite) = attempt.prewrite
                && !valid_carried_rect(prewrite.x, prewrite.y, prewrite.w, prewrite.h)
            {
                return Some("frame-prewrite-invalid");
            }
            if let Some(op) = attempt.operation_id.as_deref()
                && !op.is_empty()
                && !is_opaque_id(op)
            {
                return Some("frame-operation-invalid");
            }
            if attempt.operation_id.as_deref().is_none_or(str::is_empty)
                && attempt.prewrite.is_some()
            {
                return Some("frame-pre-without-operation");
            }
            if !seen_frames.insert(attempt.window.0.clone()) {
                return Some("duplicate-frame");
            }
        }
    }
    {
        let mut seen_members = BTreeSet::new();
        for attempt in &report.member_attempts {
            if !is_opaque_id(&attempt.operation_id) || !is_opaque_id(&attempt.window.0) {
                return Some("member-window-invalid");
            }
            if !is_opaque_id(&attempt.target_output.0) || !is_opaque_id(&attempt.target_workspace) {
                return Some("member-target-invalid");
            }
            for homing in [&attempt.pre, &attempt.post] {
                if let Some((out, ws)) = homing
                    && (!is_opaque_id(&out.0) || !is_opaque_id(ws))
                {
                    return Some("member-homing-invalid");
                }
            }
            if !seen_members.insert((attempt.operation_id.clone(), attempt.window.0.clone())) {
                return Some("duplicate-member");
            }
        }
    }
    {
        let mut seen_views = BTreeSet::new();
        for attempt in &report.view_attempts {
            if !is_opaque_id(&attempt.operation_id) {
                return Some("view-operation-invalid");
            }
            if !is_opaque_id(&attempt.target_output.0) || !is_opaque_id(&attempt.target_workspace) {
                return Some("view-target-invalid");
            }
            for view in [&attempt.pre, &attempt.post, &attempt.baseline] {
                if let Some(view) = view
                    && !is_opaque_id(view)
                {
                    return Some("view-proof-invalid");
                }
            }
            if !seen_views.insert(attempt.operation_id.clone()) {
                return Some("duplicate-view-attempt");
            }
        }
    }
    {
        let mut seen_clears = BTreeSet::new();
        for attempt in &report.clear_attempts {
            if !is_opaque_id(&attempt.operation_id) || !is_opaque_id(&attempt.window.0) {
                return Some("clear-window-invalid");
            }
            if !seen_clears.insert((attempt.operation_id.clone(), attempt.window.0.clone())) {
                return Some("duplicate-clear-attempt");
            }
        }
    }
    None
}

/// Validate one send source: explicit outputs carry opaque ids, implicit
/// variants are always well-formed (resolution refuses unknown sources
/// without inventing focus).
#[must_use]
pub fn valid_send_source(source: &SendSource) -> bool {
    match source {
        SendSource::Explicit(output) => is_opaque_id(&output.0),
        SendSource::HomeThenAction | SendSource::HomeThenCachedLive => true,
    }
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
            output.as_ref().is_none_or(|out| is_opaque_id(&out.0)) && (1..=9).contains(index)
        }
        WorldCommand::SelectRelative { output, delta } => {
            output.as_ref().is_none_or(|out| is_opaque_id(&out.0)) && (*delta == -1 || *delta == 1)
        }
        WorldCommand::SelectPrevious { output } | WorldCommand::SelectTrailing { output } => {
            output.as_ref().is_none_or(|out| is_opaque_id(&out.0))
        }
        WorldCommand::SendRelative {
            window,
            source,
            delta,
            ..
        } => is_opaque_id(&window.0) && valid_send_source(source) && (*delta == -1 || *delta == 1),
        WorldCommand::SendNumbered {
            window,
            source,
            index,
            ..
        } => is_opaque_id(&window.0) && valid_send_source(source) && *index <= 9,
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
        WorldCommand::MigrateDirectional { direction } => DIRECTIONS.contains(&direction.as_str()),
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
