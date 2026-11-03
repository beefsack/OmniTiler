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
//! Contract version 3 is logical-first plus the required neutral
//! all-native-window occupancy lane on every native row (complete,
//! readable, or explicit unknown; strict coverage of each ordered id).

pub use crate::world_sizing::{
    MAX_INTENT_ROLLBACK_DATA_JSON, MAX_INTENT_ROLLBACK_ROW_BYTES, NonFiniteNumber,
    encode_finite_json_number, encode_held_screen_canonical, encode_held_window_canonical,
    encode_reply_canonical, encode_screen_canonical, encode_window_canonical, int_len,
    json_number_len, json_string_len, max_intent_rollback_bound, push_json_number,
    push_raw_setting_value, push_row_diagnostic, raw_setting_value_len, reply_estimate,
    reply_exact_bytes, reply_upper_bound, report_estimate, u64_len, usize_len,
};

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::CoreCommand;
use crate::bounds::{GEOMETRY_BOUND, is_opaque_id, valid_carried_rect};
use crate::directional::{Direction, OutputId, WindowId};
use crate::geometry::Rect;
use crate::gesture::{GestureIngress, GestureSettled};
use crate::ids::NativeWorkspaceId;
use crate::session::{DomainKey, DragHoverPrior};

/// World envelope version (single source for the wire `v`). Version 3 is
/// the logical-first contract plus the required neutral all-native-window
/// occupancy lane: stable core workspace ids authorize mapping, screens
/// plus window facts are the only observation prerequisites, native
/// workspace domains are an explicit capability-named opt-in, and every
/// native row carries an explicit occupancy row per ordered desktop
/// (`count: null` is unknown, never an inferred zero).
pub const WORLD_CONTRACT_VERSION: u32 = 3;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// D4 selected transport budget: 4 MiB request AND reply.
pub const WORLD_MAX_REPLY_BYTES: usize = 4 * 1024 * 1024;

/// Typed authority target; wire encoding is a closed tagged record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AuthorityScope {
    Output(OutputId),
    Workspace {
        output: OutputId,
        workspace: crate::directional::WorkspaceId,
    },
}

impl AuthorityScope {
    #[must_use]
    pub fn covers_workspace(&self, output: &OutputId, workspace: &str) -> bool {
        match self {
            Self::Output(scope_output) => scope_output == output,
            Self::Workspace {
                output: scope_output,
                workspace: scope_workspace,
            } => scope_output == output && scope_workspace.0 == workspace,
        }
    }
}

/// Workspace reference with an explicit logical/native namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WorkspaceRef {
    Logical {
        output: OutputId,
        workspace: crate::directional::WorkspaceId,
    },
    Native {
        output: OutputId,
        workspace: NativeWorkspaceId,
    },
}

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
/// Bound on mechanical native creation attempts per report (one latest-wins
/// row per outstanding creation operation at most).
pub const MAX_WORLD_CREATION_ATTEMPTS: usize = 256;
/// Bound on mechanical native-state attempts per report (one latest-wins
/// row per window property at most).
pub const MAX_WORLD_STATE_ATTEMPTS: usize = 1024;
/// Bound on the presentation native order per report (same lifetime
/// alphabet as windows): the adapter's top-to-bottom stacking snapshot.
pub const MAX_PRESENTATION_ORDER: usize = 1024;

/// Bound on host-scaled presentation style numbers per report: adapter DPI
/// scaling already applied, so values stay in ordinary pixel range.
pub const MAX_PRESENTATION_STYLE_PX: i32 = 1024;
/// Bound on one native occupant count: neutral all-window occupancy per
/// ordered desktop (managed, unmanaged, and deleted occupants alike).
/// `None` on the wire (`null`) is explicit unknown, never an inferred zero.
pub const MAX_NATIVE_OCCUPANTS_PER_DESKTOP: u32 = 10_000;
/// Bound on one output identity text in UTF-8 bytes (manufacturer, model,
/// serial, connector): overbound identity holds the identifiable screen
/// row narrowly instead of refusing the report.
pub const MAX_IDENTITY_TEXT_BYTES: usize = 256;

/// Bound on one native window metadata text in UTF-8 bytes (role, appId,
/// executable, class, title): overbound text holds the identifiable window
/// row narrowly instead of refusing the report. Shared with the handshake
/// identity grammar in [`crate::connection::valid_identity_text`].
pub const MAX_METADATA_TEXT_BYTES: usize = 4096;

/// Bound on opaque host intent return rows per report (`intentRecords`).
pub const MAX_INTENT_RECORDS: usize = 10_000;
/// Bound on core intent save/clear decisions per reply (`intentWrites`).
pub const MAX_INTENT_WRITES: usize = 10_000;

/// Opaque host intent return row: the storage-verified live subject plus the
/// exact core record bytes, never interpreted by the adapter. The outer
/// `data` string only carries the slot bound (<=1024 UTF-8 bytes); inner
/// version/shape verdicts never change this envelope validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRecord {
    pub win: WindowId,
    pub data: String,
}

/// Core intent save/clear decision: the exact subject plus the opaque save
/// value, or `None` (wire `null`) to explicitly clear the lifetime slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentWrite {
    pub win: WindowId,
    pub data: Option<String>,
}

/// Tiling policy for one desired workspace row. The core retains one mode
/// per workspace (Planner-owned, set through
/// [`WorldCommand::SetWorkspaceTiling`] or flipped through
/// [`WorldCommand::ToggleWorkspaceTiling`); the adapter never decides.
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

/// Typed authority scope for a particular logical workspace on its owning
/// live output. Shared internal workspace identity is projected by callers
/// through each owning output, never serialized as a synthetic `shared` id.
#[must_use]
pub fn workspace_authority_scope(output: &OutputId, workspace: &str) -> AuthorityScope {
    AuthorityScope::Workspace {
        output: output.clone(),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
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

/// Composed visibility target (outbound only): the desired shown state,
/// never a native unminimize command. `revision` is the world revision at
/// target authorization, retained across observation-only replies.
/// `fallback` is core permission for hide-only fallback after failed or
/// unproven focus; never set for shows. `None` on the row carries no
/// visibility target: no derived policy, no new authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityTarget {
    pub shown: bool,
    pub revision: u64,
    pub fallback: bool,
}

/// Independent native property for [`StateTarget`]: one target per
/// property. `above` is the persistent project stacking ownership; the
/// one-shot stack effect stays a distinct action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StateProperty {
    Fullscreen,
    Maximized,
    Sticky,
    Above,
}

impl StateProperty {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fullscreen => "fullscreen",
            Self::Maximized => "maximized",
            Self::Sticky => "sticky",
            Self::Above => "above",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "fullscreen" => Self::Fullscreen,
            "maximized" => Self::Maximized,
            "sticky" => Self::Sticky,
            "above" => Self::Above,
            _ => return None,
        })
    }
}

/// Independent native-state target (outbound only): the core-resolved
/// target for one property. `authority` carries this property's write
/// authority only; `command_seq` names the originating explicit command,
/// never parsed operation text. Empty on every row until the state lane
/// migrates: no derived policy, no new authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateTarget {
    pub property: StateProperty,
    pub value: bool,
    pub authority: ScopeAuthority,
    pub command_seq: Option<u64>,
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
    /// Explicit command correlation for the published frame/clear target:
    /// `Some` only with a published explicit retained operation token from
    /// the originating command, never a parsed id and never the latest
    /// report sequence. `None` with no published token; born commandless
    /// clears publish no sequence.
    pub command_seq: Option<u64>,
    /// Composed visibility target, if any. `None` on every row until the
    /// visibility lane migrates.
    pub visibility: Option<VisibilityTarget>,
    /// Independent visibility write authority. Untouched on every row
    /// until the visibility lane migrates.
    pub visibility_authority: ScopeAuthority,
    /// Independent native-state targets. Empty on every row until the
    /// state lane migrates.
    pub state: Vec<StateTarget>,
}

/// Desired workspace row. `id` is the stable core logical workspace id;
/// `output` is its owning screen. `existence=false` is removal ELIGIBILITY
/// only (the adapter re-enumerates occupants, pins and minimum count before
/// deleting; unknown occupancy keeps). `native_binding` carries the
/// host-owned native desktop id bound to this logical workspace, only when
/// the `host_owned_workspace_domains` capability is set (`None` in the
/// default contract and for the ordinary trailing spare); it never
/// authorizes mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDesire {
    pub id: String,
    pub output: OutputId,
    pub native_binding: Option<String>,
    pub scope: AuthorityScope,
    pub order: usize,
    pub tiling_mode: TilingMode,
    pub existence: bool,
    pub hold: Option<HoldReason>,
    /// Explicit trailing-spare designation: true only for the final
    /// parity-padded ring row when it is the empty unoccupied lifecycle
    /// spare. The wire omits the key unless true; adapters never infer
    /// the spare from an unbound row alone.
    pub trailing: bool,
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
    pub scope: AuthorityScope,
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
/// The next report's window observations must be captured after this
/// attempt's immediate readback. Later observation is not, by itself,
/// proof that an asynchronous native configure completed.
///
/// - `target` is the attempted frame (the core's published desire, echoed
///   as correlation so the core matches the attempt to its desire).
/// - `readback` is the immediate post-write frame (`None` = unreadable).
/// - `dpi_before`/`dpi_after` are the live window DPI around the write
///   (`None` = unreadable, or the adapter coordinate space has no per-window
///   basis; never guessed). A changed basis invalidates the placement: it
///   never becomes a stable clamp and never confirms a float.
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
/// - `pre`/`post` are namespaced references observed immediately before and
///   after the write (`None` = unreadable). Member refs are logical on the
///   native-free route and native on the host-owned route. The embedded
///   output is authoritative for affinity and native translation. Only an
///   explicit refusal with an unchanged readable preimage proves refusal.
/// - `attempt_ms` is the adapter monotonic clock at the write, matching
///   [`FrameAttempt::attempt_ms`] idempotency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberAttempt {
    pub operation_id: String,
    pub window: WindowId,
    pub target_output: OutputId,
    pub target_workspace: String,
    pub setter: FrameSetter,
    pub pre: Option<WorkspaceRef>,
    pub post: Option<WorkspaceRef>,
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
/// - `pre`/`post` are namespaced native refs immediately before/after the
///   write (`None` = unreadable); `baseline` is the native observation-time
///   view (`None` = unknown). View attempts exist only for host-owned
///   selectors. Only an explicit refusal
///   with an unchanged readable preimage proves refusal; a readable post
///   that is neither the baseline nor the target proves newer external
///   input.
///   The core translates each reference through its own binding at the
///   output carried by that reference. A translated post proving the target
///   was reached followed by a different known current proves the proven
///   view was abandoned (newer external input wins). Native-free visibility
///   and focus observations remain separate from selector-attempt evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewAttempt {
    pub operation_id: String,
    pub target_output: OutputId,
    pub target_workspace: String,
    pub setter: FrameSetter,
    pub pre: Option<WorkspaceRef>,
    pub post: Option<WorkspaceRef>,
    pub baseline: Option<WorkspaceRef>,
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

/// One normalized raw native-state attempt: the mechanical facts behind a
/// single native fullscreen/maximize/sticky/above write, reported on the
/// next report. The adapter writes, re-reads the property, and reports;
/// the core owns every verdict. Never a policy conclusion: no outcome
/// verdict rides here.
///
/// - `window`/`property`/`target` echo the exact core native-state target
///   as correlation so the core matches the attempt to its desire.
/// - `revision` is the world revision at target authorization, retained
///   across observation-only replies (wire `rev`, quoted decimal).
/// - `setter` is the mechanical disposition (`Accepted` never proves
///   completion by itself, `Refused` is an explicit native refusal,
///   `Threw` is a throwing/ambiguous call).
/// - `pre`/`post` are the live property states immediately before/after
///   the write (`None` = unreadable). Only an explicit refusal with an
///   unchanged readable preimage proves refusal; anything else latches
///   ambiguity.
/// - `attempt_ms` matches [`FrameAttempt::attempt_ms`] idempotency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAttempt {
    pub window: WindowId,
    pub property: StateProperty,
    pub target: bool,
    pub revision: u64,
    pub setter: FrameSetter,
    pub pre: Option<bool>,
    pub post: Option<bool>,
    pub attempt_ms: u64,
}

/// One normalized pager frame source row (stage 5): an opaque member
/// lifetime plus its distinct frame sources in adapter units. `live` is the
/// fresh native frame from this tick's enumeration (observed visible frame
/// or retained overlay row frame); `float_frame`/`tile_frame` are the
/// adapter's retained slot snapshots. Every slot is optional; the core
/// resolves live first, then float, then tile. Sources are normalized
/// observations only: they never admit members or authorize writes, and
/// the core never reads `WindowFact.rect` for pager content (fact frames
/// are normalized for homing/overlay policy, e.g. a retained overlay row
/// carries its tile slot while the fresh native frame rides `live`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationPagerFrame {
    pub window: WindowId,
    pub live: Option<Rect>,
    pub float_frame: Option<Rect>,
    pub tile_frame: Option<Rect>,
}

/// Normalized per-report presentation ingress (stage 5): the genuinely
/// observed foreground lifetime plus the host-scaled style and movement
/// facts the core needs to publish uniform border/underlay/pager
/// presentation in the reply. Every report carries exactly one; the core
/// owns every verdict, the adapter only captures, observes and renders.
///
/// - `foreground` is the genuinely observed foreground lifetime
///   (proof-verified adapter-side, `""` when none). It may name an unmanaged
///   lifetime: ordinary floats and scoped unmanaged foregrounds stay
///   presentation-eligible without acquiring membership.
/// - `foreground_frame` is its live frame in adapter units (`None` when
///   unreadable; the border hides instead of guessing).
/// - `border_gap_px`/`border_width_px`/`underlay_extension_px` are the
///   already host-scaled style numbers in the same coherent adapter
///   units as screens and fact frames (physical pixels on Windows,
///   KWin logical pixels on KDE). DPI conversion stays adapter-side
///   and runs before this report; the extension sentinel is already
///   resolved, so the outset here is literal.
/// - `chord_held` is the movement-only Win+Shift level; `move_window` names
///   the classified focused Move subject (`""` when none). The core derives
///   the underlay trigger as chord OR a matching focused move.
/// - `paused` is the focused-fullscreen-game lane: the adapter sets it while
///   a fullscreen game holds the session and the core skips presentation
///   work (hidden presentation, empty pager) instead of projecting.
/// - `pager_order` is the single native stacking snapshot, top-to-bottom,
///   over opaque member lifetimes. It may name first-paint candidates not
///   yet admitted; unknown lifetimes never admit members.
/// - `pager_frames` carries one bounded source row per window with optional
///   live, float, and tile rectangles (fresh enumeration frame, then
///   retained slot snapshots). The core resolves live first, then float,
///   then tile, and never reads `WindowFact.rect` for pager content:
///   members with no resolved source render nothing, never a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentationIngress {
    pub foreground: String,
    pub foreground_minimized: bool,
    pub foreground_maximized: bool,
    pub foreground_fullscreen: bool,
    pub foreground_cloaked: bool,
    pub foreground_hidden_workspace: bool,
    pub foreground_shell: bool,
    pub foreground_frame: Option<Rect>,
    pub border_enabled: bool,
    pub underlay_enabled: bool,
    pub border_gap_px: i32,
    pub border_width_px: i32,
    pub underlay_extension_px: i32,
    pub chord_held: bool,
    pub move_window: String,
    pub paused: bool,
    pub pager_order: Vec<WindowId>,
    pub pager_frames: Vec<PresentationPagerFrame>,
}

impl PresentationIngress {
    /// Presentation-silent ingress for reports that carry no presentation
    /// facts: both overlays disabled, no foreground, no order. The reply
    /// carries hidden border/underlay and an empty pager.
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }
}

/// Native reconnect identity metadata for one output: neutral adapter-read
/// strings, never adapter identity policy. Each of the four required keys
/// is `None` (wire `null`) when unknown; empty strings stay preserved as
/// `Some("")` (never normalized to unknown). Present values are bounded to
/// 256 UTF-8 bytes each; the connector names the native connection
/// identity only when its physical-connection normalization is proved
/// (never a display id, UUID, ordinal, or unit number alone).
/// The core derives physical reconnect identity from
/// manufacturer/model/serial, using the connector if unreadable or
/// duplicated; ambiguous reconnect holds rather than picking a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputIdentity {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub connector: Option<String>,
}

impl OutputIdentity {
    /// Fully unknown identity: every field `None`.
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            manufacturer: None,
            model: None,
            serial: None,
            connector: None,
        }
    }
}

/// Typed narrow-hold diagnostic for one bad identifiable row. The scope
/// names the narrow held scope (never a policy verdict supplied by the
/// adapter); `row` names the affected lane; `id` names the identifiable
/// row (rows without identity refuse the envelope instead of holding);
/// `kind` is the closed diagnostic vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowDiagnosticKind {
    Unreadable,
    InvalidFact,
    ConflictingFact,
}

impl RowDiagnosticKind {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::InvalidFact => "invalid-fact",
            Self::ConflictingFact => "conflicting-fact",
        }
    }
}

/// Affected report lane for a [`RowDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowDiagnosticLane {
    Screen,
    Window,
    Native,
    Attempt,
    Presentation,
}

impl RowDiagnosticLane {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Window => "window",
            Self::Native => "native",
            Self::Attempt => "attempt",
            Self::Presentation => "presentation",
        }
    }
}

/// One narrow-hold diagnostic: the held scope plus the affected lane, the
/// identifiable row id, and the closed diagnostic kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowDiagnostic {
    pub scope: AuthorityScope,
    pub row: RowDiagnosticLane,
    pub id: Option<String>,
    pub kind: RowDiagnosticKind,
}

/// One identifiable screen row that failed v4 shape checks, held narrowly
/// instead of refusing the whole report. `output` is the valid live output
/// handle; `ordinal` is the row's original index in the carried `screens`
/// array so a future exact request encoder can reconstruct the original
/// order (valid rows keep relative order in `screens`); `fields` retains
/// every carried screen key in canonical schema key order (`out`, `full`,
/// `work`, `complete`, `scale`, `identity`, then any other carried keys in
/// lexicographic order) as JSON-agnostic
/// [`RawSettingValue`](crate::connection::RawSettingValue) facts so the
/// canonical writer round-trips the row losslessly (absence, `null`, and
/// value are all preserved, including bad types, overbound identity text,
/// and retired extras; the nested `identity` object itself is retained in
/// schema order `manufacturer`, `model`, `serial`, `connector` with unknown
/// sub-keys after in lexicographic order); `diagnostic` is the emitted
/// typed diagnostic for the held scope.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldScreenRow {
    pub output: OutputId,
    pub ordinal: usize,
    pub fields: Vec<(String, crate::connection::RawSettingValue)>,
    pub diagnostic: RowDiagnostic,
}

/// One identifiable window row that failed v4 shape checks, held narrowly
/// instead of refusing the whole report. `window` is the valid lifetime
/// handle; `ordinal` is the row's original index in the carried `windows`
/// array so a future exact request encoder can reconstruct the original
/// order (valid rows keep relative order in `windows`); `output` is the
/// row's actual reported output (`None` when the row carried `out: null`:
/// the authority source, never a guessed screen); `fields` retains
/// every carried window key in canonical schema key order (`win`, `out`,
/// `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`, `hints`,
/// `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`,
/// `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`,
/// `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`,
/// `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`,
/// `layer`, `fullscreenOwner`, `fullscreenExitable`,
/// `suppressionAvailable`, `visibility`, `lastWrite`, then any other
/// carried keys in lexicographic order) as JSON-agnostic
/// [`RawSettingValue`](crate::connection::RawSettingValue) facts so the
/// canonical writer round-trips the row losslessly (absence, `null`, and
/// value are all preserved, including bad types, overbound metadata text,
/// and retired extras; the nested `hints`, `visibility`, and `lastWrite`
/// objects are retained in schema order with unknown sub-keys after in
/// lexicographic order); `diagnostic` is `Some` typed diagnostic for the
/// row's actual reported output, and `None` when the row carried no
/// output: such a row has no output authority (its evidence is retained
/// in `fields`; engine staging may resolve a real scope from the
/// retained home, but never a fabricated one). `kind` is the mandatory
/// narrow-hold reason (`InvalidFact` or `ConflictingFact`), independent
/// of the scoped `diagnostic`: scope resolution must not lose the
/// invalid-vs-conflicting distinction, including when `diagnostic` is
/// `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldWindowRow {
    pub window: WindowId,
    pub ordinal: usize,
    pub output: Option<OutputId>,
    pub fields: Vec<(String, crate::connection::RawSettingValue)>,
    pub diagnostic: Option<RowDiagnostic>,
    pub kind: RowDiagnosticKind,
}

/// One screen observation: the stable output id, the FULL native output
/// rectangle in one coherent adapter coordinate space, the distinct work
/// area the core tiles into, the target output scale (adapter units per
/// logical pixel, finite positive), and the neutral native reconnect
/// identity metadata. `gap`/`outer_gap`/`primary` are not screen keys:
/// the core derives per-output physical gaps from the logical settings
/// times this scale, and the report carries at most one host-designated
/// primary. `None` rectangles stay representable for explicitly
/// unreadable/partial observations; a report claiming complete topology
/// carries valid FULL and work-area rectangles for every screen, and
/// anything else narrowly fences topology-dependent crossing, migration,
/// and remap (never an inferred work area, never a window frame proxy).
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenReport {
    pub output: OutputId,
    pub full_rect: Option<Rect>,
    pub work_area: Option<Rect>,
    pub complete: bool,
    pub scale: f64,
    pub identity: OutputIdentity,
}

/// Neutral native window kind, mechanically normalized by the adapter, never
/// an admission or eligibility verdict. Closed vocabulary: `normal`, `dialog`,
/// `sheet`, `utility`, `desktop`, `dock`, `menu`, `tooltip`, `notification`,
/// `splash`, `other`. Unreadable kind is null, never `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowType {
    Normal,
    Dialog,
    Sheet,
    Utility,
    Desktop,
    Dock,
    Menu,
    Tooltip,
    Notification,
    Splash,
    Other,
}

impl WindowType {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Dialog => "dialog",
            Self::Sheet => "sheet",
            Self::Utility => "utility",
            Self::Desktop => "desktop",
            Self::Dock => "dock",
            Self::Menu => "menu",
            Self::Tooltip => "tooltip",
            Self::Notification => "notification",
            Self::Splash => "splash",
            Self::Other => "other",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "normal" => Self::Normal,
            "dialog" => Self::Dialog,
            "sheet" => Self::Sheet,
            "utility" => Self::Utility,
            "desktop" => Self::Desktop,
            "dock" => Self::Dock,
            "menu" => Self::Menu,
            "tooltip" => Self::Tooltip,
            "notification" => Self::Notification,
            "splash" => Self::Splash,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

/// Neutral native application activation policy, never inferred from
/// `acceptsInput`. Closed vocabulary: `regular`, `accessory`, `prohibited`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivationPolicy {
    Regular,
    Accessory,
    Prohibited,
}

impl ActivationPolicy {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Accessory => "accessory",
            Self::Prohibited => "prohibited",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "regular" => Self::Regular,
            "accessory" => Self::Accessory,
            "prohibited" => Self::Prohibited,
            _ => return None,
        })
    }
}

/// Neutral native fullscreen owner: whether the tiler or the app owns the
/// current fullscreen state. Unknown does not mean app-owned or exitable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FullscreenOwner {
    Tiler,
    App,
}

impl FullscreenOwner {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tiler => "tiler",
            Self::App => "app",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "tiler" => Self::Tiler,
            "app" => Self::App,
            _ => return None,
        })
    }
}

/// Current native visibility and recovery ownership: the native shown state
/// plus whether the project owns a suppression of this window. Both nullable:
/// unknown never proves readiness and never substitutes for suppression
/// support. Minimal consumption only; the full visibility/focus lane is a
/// separate migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityFact {
    pub visible: Option<bool>,
    pub suppression_owned: Option<bool>,
}

/// One identity-bound visibility read around a visibility write or no-call:
/// the invalidation watermark at the read, the lifetime/tag match, the
/// native state at the read, and the separate native ineligibility facts.
/// Facts only, never a copied target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilitySample {
    pub obs: u64,
    pub lifetime_matched: Option<bool>,
    pub visibility: VisibilityFact,
    pub minimized: Option<bool>,
    pub host_hidden: Option<bool>,
}

/// Latest visibility write evidence: the property tag (reserved for later
/// lane migrations; only `visibility` today), the desired-state target and
/// existing revision (no visibility id), the actual execution route, the
/// setter disposition (`not-called` is not a successful native call), the
/// bounded typed mechanical reason, and the identity-bound reads around
/// the action or no-call. No work id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastWrite {
    pub property: LastWriteProperty,
    pub target: bool,
    pub revision: u64,
    pub path: LastWritePath,
    pub setter: VisibilitySetter,
    pub reason: VisibilityReason,
    pub pre: Option<VisibilitySample>,
    pub post: Option<VisibilitySample>,
}

/// Property tag for [`LastWrite`]: only `visibility` today, reserved for
/// later separate lane migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LastWriteProperty {
    Visibility,
}

impl LastWriteProperty {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visibility => "visibility",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "visibility" => Self::Visibility,
            _ => return None,
        })
    }
}

/// Actual execution route for [`LastWrite`]: `staged`, `fallback`, or
/// `independent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LastWritePath {
    Staged,
    Fallback,
    Independent,
}

impl LastWritePath {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Fallback => "fallback",
            Self::Independent => "independent",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "staged" => Self::Staged,
            "fallback" => Self::Fallback,
            "independent" => Self::Independent,
            _ => return None,
        })
    }
}

/// Mechanical disposition of one visibility write: the native setter outcome
/// or `not-called` (no call is not a successful native call). Pure actuator
/// disposition, never a policy conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisibilitySetter {
    Accepted,
    Refused,
    Threw,
    NotCalled,
}

impl VisibilitySetter {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Refused => "refused",
            Self::Threw => "threw",
            Self::NotCalled => "not-called",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "accepted" => Self::Accepted,
            "refused" => Self::Refused,
            "threw" => Self::Threw,
            "not-called" => Self::NotCalled,
            _ => return None,
        })
    }
}

/// Bounded typed mechanical reason for [`LastWrite`]: empty on an ordinary
/// call, otherwise one of the closed tokens. Facts only, never a copied
/// target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisibilityReason {
    Empty,
    IdentityStale,
    ScopeHeld,
    Unreadable,
    Unsupported,
    PreimageUnavailable,
    ForegroundChanged,
    Fullscreen,
    Game,
    ShowBlocked,
    FocusUnproven,
    Superseded,
    NativeRefused,
}

impl VisibilityReason {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::IdentityStale => "identity-stale",
            Self::ScopeHeld => "scope-held",
            Self::Unreadable => "unreadable",
            Self::Unsupported => "unsupported",
            Self::PreimageUnavailable => "preimage-unavailable",
            Self::ForegroundChanged => "foreground-changed",
            Self::Fullscreen => "fullscreen",
            Self::Game => "game",
            Self::ShowBlocked => "show-blocked",
            Self::FocusUnproven => "focus-unproven",
            Self::Superseded => "superseded",
            Self::NativeRefused => "native-refused",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "" => Self::Empty,
            "identity-stale" => Self::IdentityStale,
            "scope-held" => Self::ScopeHeld,
            "unreadable" => Self::Unreadable,
            "unsupported" => Self::Unsupported,
            "preimage-unavailable" => Self::PreimageUnavailable,
            "foreground-changed" => Self::ForegroundChanged,
            "fullscreen" => Self::Fullscreen,
            "game" => Self::Game,
            "show-blocked" => Self::ShowBlocked,
            "focus-unproven" => Self::FocusUnproven,
            "superseded" => Self::Superseded,
            "native-refused" => Self::NativeRefused,
            _ => return None,
        })
    }
}

/// One truthful raw window fact: the stable window lifetime, the observed
/// screen affinity (`None` is unknown, never a current-output guess), the
/// frame, and the raw native facts the core classifies from. `rect=None`
/// is an explicitly unreadable frame: the homed scope holds instead of
/// authorizing geometry, never a silent absence. `minimized=Some(true)` (or
/// `host_hidden=Some(true)`) marks a lifetime the host holds back: the core
/// homes it on its retained logical workspace with no managed geometry
/// (never silent absence, never inferred close). Every nullable input that
/// is `None` is unknown now and holds only the classification or action
/// that needs it, never unrelated windows or properties. Supported-null
/// inputs hold their dependent classification; predicates whose fact is
/// listed unavailable are skipped, never turned into a false default.
/// Core logical workspaces authorize all homing, never native workspace
/// vocabulary.
/// `native_workspace` is the capability-gated host-owned native desktop id
/// for this window (KWin desktop id, macOS Space id, opaque platform
/// vocabulary, separate namespace from logical ids). It is only meaningful
/// when `host_owned_workspace_domains` is set; without the capability any
/// carried value refuses, like `native[]`. The core maps
/// `(output, native_workspace)` to a stable logical id and derives the
/// logical receipt from that mapping; the adapter never mints logical ids
/// from native ids. All three unknown spellings mean unknown, never a proof
/// of absence: with the capability set, unknown native membership holds
/// (never collapses into the current view). The wire spelling is retained
/// (`Omitted` for an absent key, `Null` for explicit `null`,
/// `Value("")` for explicit empty, `Value(id)` for a known id) so a
/// decoded row re-encodes to its exact carried bytes; `as_deref` exposes
/// the semantic view (`Some` only for a known non-empty id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeWorkspaceFact {
    Omitted,
    Null,
    Value(String),
}

impl NativeWorkspaceFact {
    #[must_use]
    pub fn as_deref(&self) -> Option<&str> {
        match self {
            Self::Omitted | Self::Null => None,
            Self::Value(id) if id.is_empty() => None,
            Self::Value(id) => Some(id.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFact {
    pub window: WindowId,
    pub output: Option<OutputId>,
    pub rect: Option<Rect>,
    pub fullscreen: Option<bool>,
    pub maximized: Option<bool>,
    pub sticky: Option<bool>,
    pub minimized: Option<bool>,
    pub hints: Option<crate::size_hints::WindowSizeHints>,
    pub native_workspace: NativeWorkspaceFact,
    pub host_hidden: Option<bool>,
    pub privileged: Option<bool>,
    pub managed: Option<bool>,
    pub accepts_input: Option<bool>,
    pub activation_inhibited: Option<bool>,
    pub tool_window: Option<bool>,
    pub owned: Option<bool>,
    pub transient: Option<bool>,
    pub modal: Option<bool>,
    pub resizable: Option<bool>,
    pub above: Option<bool>,
    pub below: Option<bool>,
    pub owner: Option<WindowId>,
    pub parent: Option<WindowId>,
    pub role: Option<String>,
    pub app_id: Option<String>,
    pub executable: Option<String>,
    pub process_id: Option<String>,
    pub class: Option<String>,
    pub title: Option<String>,
    pub window_type: Option<WindowType>,
    pub activation_policy: Option<ActivationPolicy>,
    pub layer: Option<i32>,
    pub fullscreen_owner: Option<FullscreenOwner>,
    pub fullscreen_exitable: Option<bool>,
    pub suppression_available: Option<bool>,
    pub visibility: VisibilityFact,
    pub last_write: Option<LastWrite>,
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

/// Neutral all-native-window occupancy for one ordered desktop, independent
/// of management. `None` (wire `null`) means unreadable or incomplete, never
/// empty. Exact zero requires a complete readable native window inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeOccupancy {
    pub native: String,
    pub count: Option<u32>,
}

/// Host-owned native domain observation (optional, capability-gated): the
/// adapter-observed native workspace order and current native view for one
/// screen. Pure observation, never mapping authority: core logical
/// workspaces authorize all homing, lifecycle, and commands. Carrying any
/// entry without the `host_owned_workspace_domains` capability refuses.
/// Completeness contract (producer requirement): `native_order` is the
/// complete observed native list for `output` only when that screen's
/// `complete` is true AND the report `inventory_complete` is true.
/// Otherwise the list is partial and must not create mappings: the core
/// holds. An absent `NativeObservation` row for a live output never
/// deletes mappings (no inferred native deletion). Within one output,
/// `native_order` ids must be unique and `current_native` (when present)
/// should name a member of that order; contradictory duplicate rows for the
/// same output refuse. The same native id string may repeat across
/// different outputs (KWin global ids): each `(output, native id)` pair
/// maps independently in separate-view modes; linked views share logical ids
/// while retaining only the native bindings actually observed on each output.
/// `current_native: None` (wire `null`) means the current surface is not an
/// ordered desktop (for example a fullscreen Space is showing): the core
/// retains the logical view while still using the reported order. It is
/// never adopted and never moves the view. In shared (linked-view) mode one native id string
/// observed on several outputs maps to a single unified logical id, but a
/// binding publishes only for outputs that actually observed it. A fresh
/// shared projection whose first-appearance union would reverse some
/// output's reported order is ambiguous: the core adopts nothing for it
/// (no merge policy is selected) and holds the affected rows instead.
///
/// Capability-named note: `None` keeps the retained view; only ordered
/// desktops in `native_order` ever adopt.
/// `occupancy` is keyed coverage: exactly one row per ordered desktop, in
/// any order. Unknown counts cannot prove native retirement eligibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeObservation {
    pub output: OutputId,
    pub native_order: Vec<String>,
    pub current_native: Option<String>,
    pub occupancy: Vec<NativeOccupancy>,
}

/// Neutral host-owned native creation desire (capability-gated): one
/// core-owned authorization for the adapter to create exactly one native
/// desktop for one logical workspace, executed at most once per
/// `operation_id` (replay idempotent). The core mints the id from its own
/// serial (`op-native-<n>`), never from a command sequence: trailing spare
/// creations are observation-caused and several outputs/workspaces can share
/// one command. An unbound logical row without a desire row authorizes
/// nothing: the adapter never treats it as creation permission.
/// Capability-named note: core binds, adapter executes once per op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCreationDesire {
    pub operation_id: String,
    pub logical_workspace: String,
    pub target_output: OutputId,
}

/// One normalized raw native creation attempt: the mechanical facts behind
/// a single native desktop create call for one outstanding creation
/// operation. Follows the existing raw-attempt pattern (setter plus
/// before/after evidence, never a policy conclusion): the adapter writes,
/// re-reads the native order, and reports; the core owns binding and
/// settlement. `order_before` is the live native order immediately before
/// the write; `order_after` is the first successfully observed full order
/// afterwards. If that read is deferred, omit the receipt until readable,
/// keep the original write stamp and leave `created_native` unknown unless
/// independent host evidence proves causality. The report `native[]` row
/// is the fresh readback the core settles against. `created_native` is the neutral
/// optional host-correlated created native id: when the host can prove which
/// fresh desktop its create call produced, it names that id here (`None`
/// when it cannot). A correlated id binds exactly that newly created id
/// causally at most once, even with racing outside additions; invalid,
/// reused, or changed receipts hold conservatively. Exact match on operation
/// id plus logical plus output; `attempt_ms` is the adapter monotonic stamp
/// for idempotent redelivery.
/// Capability-named note: core-owned binding, one authorization per op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCreationAttempt {
    pub operation_id: String,
    pub logical_workspace: String,
    pub target_output: OutputId,
    pub setter: FrameSetter,
    pub order_before: Vec<String>,
    pub order_after: Vec<String>,
    /// Host-correlated created native id (`None` when the host cannot prove
    /// which fresh desktop its create call produced).
    pub created_native: Option<String>,
    pub attempt_ms: u64,
}

/// Source-output intent resolved before this report changes retained homing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendSource {
    /// Genuinely semantic explicit output; passes through unchanged.
    Explicit(OutputId),
    /// Retained pre-observation home, else the full migration/control
    /// action context (foreground, pointer, cache, known-registry order).
    /// The implicit send route. Core uses retained home first, then the
    /// action-context fallback chain.
    HomeThenAction,
}

/// Explicit world commands. At most one per report; observation-only
/// reports carry none. Every variant consumes one `command_seq` (bound to
/// the adapter incarnation/core generation high-watermark) and is retained
/// or spent exactly like the original three: selection/view commands retain
/// a [`TransferKind::View`] or [`TransferKind::Migration`] target,
/// `Send`/`SendRelative`/`SendNumbered`-to-bound-target and send operations
/// retain a window transfer, focus operations emit one at-most-once effect,
/// `SetWorkspaceTiling` retains planner-owned tiling mode (and
/// `ToggleWorkspaceTiling` flips it at execution time), and read-only
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
    /// Callers that genuinely choose a mode submit this; racing toggles
    /// submit [`WorldCommand::ToggleWorkspaceTiling`] instead.
    SetWorkspaceTiling {
        output: OutputId,
        workspace: String,
        tiled: bool,
    },
    /// Neutral per-workspace tiling toggle: flip the mode retained for one
    /// live logical workspace at command-execution time. Queued toggles
    /// compose (each execution re-reads the retained mode), so racing
    /// adapters resolve identically with no snapshot preselection.
    /// Semantics otherwise match [`WorldCommand::SetWorkspaceTiling`].
    ToggleWorkspaceTiling { output: OutputId, workspace: String },
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
    ///
    /// An empty window names no focused window: the core starts from the
    /// current workspace/output (neutral action context) and crosses by
    /// the existing cross-output selection, otherwise no effect.
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
    /// Maximize toggle: flip the maximized flag on the focused
    /// subject at execution time. `clear_fullscreen` carries the source
    /// fullscreen preimage (true when the subject was fullscreen, so the
    /// adapter clears fullscreen first); `was_maximized` carries the source
    /// maximized preimage. Never retains or mutates topology: the snapshot
    /// retains base membership; the result authorizes the native transition.
    ToggleMaximize { window: WindowId },
    /// Neutral sticky set: the explicit on/off sticky intent for one
    /// window. Core resolves and retains the target; the adapter never
    /// decides.
    SetSticky { window: WindowId, value: bool },
    /// Neutral sticky toggle: flip the retained sticky state for one
    /// window at command-execution time. Queued toggles compose (each
    /// execution re-reads the retained state).
    ToggleSticky { window: WindowId },
    /// Native fullscreen set: the explicit on/off fullscreen intent for one
    /// window. Core resolves and retains the target; the adapter never
    /// decides.
    SetFullscreen { window: WindowId, value: bool },
    /// Native fullscreen toggle: flip the readable native fullscreen fact
    /// at command-execution time. An unconfirmed target is not a native fact.
    ToggleFullscreen { window: WindowId },
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
#[derive(Debug, Clone, PartialEq)]
pub struct WorldReport {
    pub contract_version: u32,
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
    /// Screen observations: FULL rectangles plus work areas, per-output
    /// scale, and neutral reconnect identity. Host-designated primary
    /// rides the report-level `primary_output`, never a per-screen flag.
    /// Core topology authority.
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
    /// Host-owned native domain observations. Core accepts them only when
    /// the bound handshake advertises the corresponding capability.
    pub native: Vec<NativeObservation>,
    /// Normalized cursor position in adapter units (same space as screen
    /// FULL rectangles and fact frames), `None` when the adapter cannot
    /// read it. Seated with `observation_seq`: retained per accepted
    /// report with the behavior settings and focused facts above, never
    /// on a separate stale timer. Cursor lookup uses FULL bounds; an
    /// absent cursor or a point outside every screen falls back through
    /// the focused output to the designated-primary/first readable output.
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
    /// Host-designated primary output (wire `primary`): `None` when the
    /// designation is unknown (the key is omitted). Wire `null` refuses:
    /// unknown is absence, never an explicit null. The core falls back to
    /// the first stable screen order when this names no live output, never
    /// to the active screen.
    pub primary_output: Option<OutputId>,
    /// Identifiable screen rows that failed v4 shape checks, held narrowly
    /// with their raw fields retained (see [`HeldScreenRow`]). Valid rows
    /// stay in `screens`; every entry here holds its output scope and emits
    /// its typed diagnostic instead of refusing the report.
    pub held_screens: Vec<HeldScreenRow>,
    /// Identifiable window rows that failed v4 shape checks, held narrowly
    /// with their raw fields retained (see [`HeldWindowRow`]). Valid rows
    /// stay in `windows`; every entry here holds its window scope and emits
    /// its typed diagnostic instead of refusing the report.
    pub held_windows: Vec<HeldWindowRow>,
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
    /// Mechanical native creation attempts since the previous report (at most
    /// one latest-wins entry per outstanding creation operation, bounded by
    /// [`MAX_WORLD_CREATION_ATTEMPTS`]). Raw setter/before/after facts
    /// only; the core owns binding and settlement over them plus the fresh
    /// `native[]` readback. Capability-gated like `native[]`.
    pub creation_attempts: Vec<NativeCreationAttempt>,
    /// Mechanical native-state attempts since the previous report (at most
    /// one latest-wins entry per window property, bounded by
    /// [`MAX_WORLD_STATE_ATTEMPTS`]). Raw setter/pre/post property facts
    /// only; the core owns every verdict over them plus the staged
    /// observation. Required on the wire (`stateAttempts`, possibly
    /// empty); raw rows ride verbatim, including original stamps.
    pub state_attempts: Vec<StateAttempt>,
    /// Normalized presentation ingress (stage 5): the genuinely observed
    /// foreground lifetime plus host-scaled style and movement facts. The
    /// core publishes uniform border/underlay/pager presentation in the
    /// reply; the adapter only captures, observes and renders.
    pub presentation: PresentationIngress,
    /// Opaque host storage return: the verified live subjects plus their
    /// exact core record bytes, never interpreted by the adapter. Required
    /// on the wire (`intentRecords`, possibly empty); raw strings ride
    /// verbatim, including unsupported or malformed inner JSON.
    pub intent_records: Vec<IntentRecord>,
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
    /// Narrow-hold row diagnostics for this report: one typed entry per
    /// held identifiable row (see [`RowDiagnostic`]). Empty when every
    /// row was usable.
    pub diagnostics: Vec<RowDiagnostic>,
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
    /// Neutral host-owned native creation desires (capability-gated): one
    /// entry per outstanding core-owned creation authorization, each
    /// executed at most once per `operation_id` by the adapter. Empty in
    /// the native-free default. Capability-named note: explicit desire
    /// only, never an implicit unbound-row permission.
    pub native_creations: Vec<NativeCreationDesire>,
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
    /// Uniform presentation (stage 5): the tick-time foreground token the
    /// shapes were computed for, the border shape, the underlay shape, and
    /// the pager content. The renderer revalidates the live foreground
    /// against the token and never paints stale shapes. A paused (game-lane)
    /// report carries hidden border/underlay and an empty pager.
    pub presentation: crate::world_presentation::WorldPresentation,
    /// Core save/clear decisions for the lifetime-bound intent slots:
    /// opaque save values, or `None` (wire `null`) to explicitly clear.
    /// Required on the wire (`intentWrites`); empty until a later increment
    /// publishes decisions. The engine emits no writes in this increment.
    pub intent_writes: Vec<IntentWrite>,
}

/// Closed command-rejection catalog for an accepted report whose command
/// is refused without mutation. Wire token is the kebab-case spelling in
/// [`CommandRejectKind::as_str`]; adapters never fall back to another
/// shape. Envelope failures are not command outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandRejectKind {
    FocusUnknown,
    SendTargetUnknown,
    SendWindowUnknown,
    SendRefused,
    ViewOutputUnknown,
    ViewTargetUnknown,
    TransferInFlight,
    DirectionInvalid,
    ScopeHeld,
    SelectionUnknown,
    MigrationRefused,
    OperationRefused,
    TilingTargetUnknown,
    StateUnsupported,
    StateUnknown,
    FullscreenNotExitable,
    StaleRenderTarget,
}

impl CommandRejectKind {
    /// Wire token for this value, matching the protocol codec vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FocusUnknown => "focus-unknown",
            Self::SendTargetUnknown => "send-target-unknown",
            Self::SendWindowUnknown => "send-window-unknown",
            Self::SendRefused => "send-refused",
            Self::ViewOutputUnknown => "view-output-unknown",
            Self::ViewTargetUnknown => "view-target-unknown",
            Self::TransferInFlight => "transfer-in-flight",
            Self::DirectionInvalid => "direction-invalid",
            Self::ScopeHeld => "scope-held",
            Self::SelectionUnknown => "selection-unknown",
            Self::MigrationRefused => "migration-refused",
            Self::OperationRefused => "operation-refused",
            Self::TilingTargetUnknown => "tiling-target-unknown",
            Self::StateUnsupported => "state-unsupported",
            Self::StateUnknown => "state-unknown",
            Self::FullscreenNotExitable => "fullscreen-not-exitable",
            Self::StaleRenderTarget => "stale-render-target",
        }
    }

    /// Parse a wire token; `None` for unknown tokens (the codec refuses).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "focus-unknown" => Self::FocusUnknown,
            "send-target-unknown" => Self::SendTargetUnknown,
            "send-window-unknown" => Self::SendWindowUnknown,
            "send-refused" => Self::SendRefused,
            "view-output-unknown" => Self::ViewOutputUnknown,
            "view-target-unknown" => Self::ViewTargetUnknown,
            "transfer-in-flight" => Self::TransferInFlight,
            "direction-invalid" => Self::DirectionInvalid,
            "scope-held" => Self::ScopeHeld,
            "selection-unknown" => Self::SelectionUnknown,
            "migration-refused" => Self::MigrationRefused,
            "operation-refused" => Self::OperationRefused,
            "tiling-target-unknown" => Self::TilingTargetUnknown,
            "state-unsupported" => Self::StateUnsupported,
            "state-unknown" => Self::StateUnknown,
            "fullscreen-not-exitable" => Self::FullscreenNotExitable,
            "stale-render-target" => Self::StaleRenderTarget,
            _ => return None,
        })
    }
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
    /// Maximize-toggle decision: `maximized` is the desired
    /// maximized flag after the toggle, `clear_fullscreen` the source
    /// fullscreen preimage (the adapter clears fullscreen first when
    /// true), `was_maximized` the source maximized preimage.
    Maximize {
        window: WindowId,
        maximized: bool,
        clear_fullscreen: bool,
        was_maximized: bool,
    },
    /// Accepted-report command rejection: the observations incorporated
    /// but the command refused without mutation. `kind` is the closed
    /// [`CommandRejectKind`] reason, never an adapter fallback.
    Rejected { kind: CommandRejectKind },
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
/// work-area rectangles, scale, identity, completeness), the report-level
/// primary designation, held screen rows (output, original ordinal,
/// retained fields, and diagnostic), facts (identity,
/// observed screen, frame, every safety flag, size-hint bounds, minimized
/// state, observed logical receipt plus capability-gated native membership),
/// view echoes, explicit closes, capability flags, host-owned
/// native observations, inventory completeness, the default-tiling seed,
/// the migration source-refill mode, the D2/D3 capacity settings, the
/// switch-target and fresh-placement modes, the focus-follows-cursor and
/// cursor-warp settings, the cursor position and cursor hit, the gesture
/// ingress, the global focus, the gesture barrier, the monotonic clock, frame attempts (window, target, setter, readback, DPI
/// pair, attempt stamp, placement operation id, pre-write frame), member
/// attempts (operation, window, target, setter, pre/post homing, native
/// pre/post ids, stamp), view attempts (owning operation, target, setter,
/// pre/post/baseline views, native pre/post/baseline ids, stamp), and clear attempts (operation, window, setter,
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
            (if screen.complete { "c" } else { "i" }).to_owned(),
            screen.scale.to_bits().to_string(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        for part in [
            screen.identity.manufacturer.as_deref().unwrap_or_default(),
            screen.identity.model.as_deref().unwrap_or_default(),
            screen.identity.serial.as_deref().unwrap_or_default(),
            screen.identity.connector.as_deref().unwrap_or_default(),
        ] {
            feed(&mut hash, part);
            sep(&mut hash);
        }
        flag(&mut hash, screen.identity.manufacturer.is_some());
        flag(&mut hash, screen.identity.model.is_some());
        flag(&mut hash, screen.identity.serial.is_some());
        flag(&mut hash, screen.identity.connector.is_some());
    }
    feed(
        &mut hash,
        report
            .primary_output
            .as_ref()
            .map(|out| out.0.as_str())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    flag(&mut hash, report.primary_output.is_some());
    for held in &report.held_screens {
        feed(&mut hash, &held.output.0);
        sep(&mut hash);
        feed(&mut hash, &held.ordinal.to_string());
        sep(&mut hash);
        fn feed_raw(hash: &mut u64, value: &crate::connection::RawSettingValue) {
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
            match value {
                crate::connection::RawSettingValue::Null => feed(hash, "null"),
                crate::connection::RawSettingValue::Bool(set) => {
                    feed(hash, if *set { "true" } else { "false" });
                }
                crate::connection::RawSettingValue::Number(number) => {
                    feed(hash, &number.to_bits().to_string());
                }
                crate::connection::RawSettingValue::String(text) => feed(hash, text),
                crate::connection::RawSettingValue::Array(items) => {
                    for item in items {
                        feed_raw(hash, item);
                        sep(hash);
                    }
                }
                crate::connection::RawSettingValue::Object(entries) => {
                    for (key, item) in entries {
                        feed(hash, key);
                        sep(hash);
                        feed_raw(hash, item);
                        sep(hash);
                    }
                }
            }
            sep(hash);
        }
        for (key, value) in &held.fields {
            feed(&mut hash, key);
            sep(&mut hash);
            feed_raw(&mut hash, value);
        }
        feed(&mut hash, held.diagnostic.kind.as_str());
        sep(&mut hash);
        feed(&mut hash, held.diagnostic.row.as_str());
        sep(&mut hash);
        feed(&mut hash, held.diagnostic.id.as_deref().unwrap_or_default());
        sep(&mut hash);
        match &held.diagnostic.scope {
            AuthorityScope::Output(output) => {
                feed(&mut hash, "output");
                sep(&mut hash);
                feed(&mut hash, &output.0);
                sep(&mut hash);
            }
            AuthorityScope::Workspace { output, workspace } => {
                feed(&mut hash, "workspace");
                sep(&mut hash);
                feed(&mut hash, &output.0);
                sep(&mut hash);
                feed(&mut hash, &workspace.0);
                sep(&mut hash);
            }
        }
        flag(&mut hash, held.diagnostic.id.is_some());
    }
    fn feed_visibility_raw(hash: &mut u64, value: &crate::connection::RawSettingValue) {
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
        match value {
            crate::connection::RawSettingValue::Null => feed(hash, "null"),
            crate::connection::RawSettingValue::Bool(set) => {
                feed(hash, if *set { "true" } else { "false" });
            }
            crate::connection::RawSettingValue::Number(number) => {
                feed(hash, &number.to_bits().to_string());
            }
            crate::connection::RawSettingValue::String(text) => feed(hash, text),
            crate::connection::RawSettingValue::Array(items) => {
                for item in items {
                    feed_visibility_raw(hash, item);
                    sep(hash);
                }
            }
            crate::connection::RawSettingValue::Object(entries) => {
                for (key, item) in entries {
                    feed(hash, key);
                    sep(hash);
                    feed_visibility_raw(hash, item);
                    sep(hash);
                }
            }
        }
        sep(hash);
    }
    for held in &report.held_windows {
        feed(&mut hash, &held.window.0);
        sep(&mut hash);
        feed(&mut hash, &held.ordinal.to_string());
        sep(&mut hash);
        for (key, value) in &held.fields {
            feed(&mut hash, key);
            sep(&mut hash);
            feed_visibility_raw(&mut hash, value);
        }
        // The diagnostic is presence-explicit: an output-less row carries
        // no scope authority, and its raw fields above already separate
        // it from every diagnosed row. The mandatory kind separates
        // invalid-vs-conflicting even when the diagnostic is absent.
        feed(&mut hash, held.kind.as_str());
        sep(&mut hash);
        flag(&mut hash, held.diagnostic.is_some());
        if let Some(diagnostic) = held.diagnostic.as_ref() {
            feed(&mut hash, diagnostic.kind.as_str());
            sep(&mut hash);
            feed(&mut hash, diagnostic.row.as_str());
            sep(&mut hash);
            feed(&mut hash, diagnostic.id.as_deref().unwrap_or_default());
            sep(&mut hash);
            match &diagnostic.scope {
                AuthorityScope::Output(output) => {
                    feed(&mut hash, "output");
                    sep(&mut hash);
                    feed(&mut hash, &output.0);
                    sep(&mut hash);
                }
                AuthorityScope::Workspace { output, workspace } => {
                    feed(&mut hash, "workspace");
                    sep(&mut hash);
                    feed(&mut hash, &output.0);
                    sep(&mut hash);
                    feed(&mut hash, &workspace.0);
                    sep(&mut hash);
                }
            }
            flag(&mut hash, diagnostic.id.is_some());
        }
    }
    for fact in &report.windows {
        for part in [
            fact.window.0.clone(),
            fact.output
                .as_ref()
                .map(|out| out.0.clone())
                .unwrap_or_default(),
            rect_part(fact.rect),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        flag(&mut hash, fact.output.is_some());
        for value in [fact.fullscreen, fact.maximized, fact.sticky, fact.minimized] {
            feed(
                &mut hash,
                &value.map(|set| set.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        feed(
            &mut hash,
            fact.native_workspace.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, fact.native_workspace.as_deref().is_some());
        // Exact request identity keeps the wire spelling distinct even
        // though all three unknown spellings mean the same semantically.
        flag(
            &mut hash,
            !matches!(fact.native_workspace, NativeWorkspaceFact::Omitted),
        );
        flag(
            &mut hash,
            matches!(fact.native_workspace, NativeWorkspaceFact::Value(_)),
        );
        for value in [
            fact.host_hidden,
            fact.privileged,
            fact.managed,
            fact.accepts_input,
            fact.activation_inhibited,
            fact.tool_window,
            fact.owned,
            fact.transient,
            fact.modal,
            fact.resizable,
            fact.above,
            fact.below,
        ] {
            feed(
                &mut hash,
                &value.map(|set| set.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        for id in [&fact.owner, &fact.parent] {
            feed(
                &mut hash,
                id.as_ref()
                    .map(|window| window.0.as_str())
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, id.is_some());
        }
        for text in [
            fact.role.as_deref(),
            fact.app_id.as_deref(),
            fact.executable.as_deref(),
            fact.process_id.as_deref(),
            fact.class.as_deref(),
            fact.title.as_deref(),
        ] {
            feed(&mut hash, text.unwrap_or_default());
            sep(&mut hash);
            flag(&mut hash, text.is_some());
        }
        feed(
            &mut hash,
            fact.window_type
                .map(|kind| kind.as_str())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, fact.window_type.is_some());
        feed(
            &mut hash,
            fact.activation_policy
                .map(|policy| policy.as_str())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, fact.activation_policy.is_some());
        feed(
            &mut hash,
            &fact
                .layer
                .map(|layer| layer.to_string())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, fact.layer.is_some());
        feed(
            &mut hash,
            fact.fullscreen_owner
                .map(|owner| owner.as_str())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, fact.fullscreen_owner.is_some());
        for value in [fact.fullscreen_exitable, fact.suppression_available] {
            feed(
                &mut hash,
                &value.map(|set| set.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        for value in [fact.visibility.visible, fact.visibility.suppression_owned] {
            feed(
                &mut hash,
                &value.map(|set| set.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        match fact.last_write.as_ref() {
            None => {
                feed(&mut hash, "no-last-write");
                sep(&mut hash);
            }
            Some(last) => {
                feed(&mut hash, last.property.as_str());
                sep(&mut hash);
                flag(&mut hash, last.target);
                feed(&mut hash, &last.revision.to_string());
                sep(&mut hash);
                feed(&mut hash, last.path.as_str());
                sep(&mut hash);
                feed(&mut hash, last.setter.as_str());
                sep(&mut hash);
                feed(&mut hash, last.reason.as_str());
                sep(&mut hash);
                for sample in [last.pre.as_ref(), last.post.as_ref()] {
                    match sample {
                        None => {
                            feed(&mut hash, "no-sample");
                            sep(&mut hash);
                        }
                        Some(sample) => {
                            feed(&mut hash, &sample.obs.to_string());
                            sep(&mut hash);
                            for value in [
                                sample.lifetime_matched,
                                sample.visibility.visible,
                                sample.visibility.suppression_owned,
                                sample.minimized,
                                sample.host_hidden,
                            ] {
                                feed(
                                    &mut hash,
                                    &value.map(|set| set.to_string()).unwrap_or_default(),
                                );
                                sep(&mut hash);
                                flag(&mut hash, value.is_some());
                            }
                        }
                    }
                }
            }
        }
        flag(&mut hash, fact.last_write.is_some());
        // Unknown hints digest distinctly from an empty hint set: absence
        // is never a default empty truth.
        flag(&mut hash, fact.hints.is_some());
        let hints = fact
            .hints
            .unwrap_or(crate::size_hints::WindowSizeHints::none());
        for bound in [hints.min_w, hints.min_h, hints.max_w, hints.max_h] {
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
        feed(
            &mut hash,
            native.current_native.as_deref().unwrap_or_default(),
        );
        // Presence of a current value separates the digest from its
        // absence: `null` (not an ordered desktop) never aliases a named
        // desktop.
        flag(&mut hash, native.current_native.is_some());
        for id in &native.native_order {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        for row in &native.occupancy {
            feed(&mut hash, &row.native);
            sep(&mut hash);
            feed(
                &mut hash,
                &row.count.map(|count| count.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, row.count.is_some());
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
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post
                .as_ref()
                .map(workspace_ref_fingerprint)
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
        feed(
            &mut hash,
            &attempt
                .pre
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .baseline
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
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
    for attempt in &report.creation_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.logical_workspace);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        for id in &attempt.order_before {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        feed(&mut hash, "#before-end");
        sep(&mut hash);
        for id in &attempt.order_after {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        feed(
            &mut hash,
            attempt.created_native.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    // Raw native-state attempts: every carried field, presence bit, row
    // order, and original stamp separates the digest (duplicate detection
    // compares full typed input equality; changed reuse still fences the
    // hold).
    for attempt in &report.state_attempts {
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, attempt.property.as_str());
        sep(&mut hash);
        flag(&mut hash, attempt.target);
        feed(&mut hash, &attempt.revision.to_string());
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        for value in [attempt.pre, attempt.post] {
            feed(
                &mut hash,
                &value
                    .map(|set| if set { "1" } else { "0" }.to_owned())
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    // Stage-5 presentation ingress: every normalized fact separates the
    // digest (duplicate detection still compares full typed input).
    let ingress = &report.presentation;
    feed(&mut hash, &ingress.foreground);
    sep(&mut hash);
    flag(&mut hash, ingress.foreground_minimized);
    flag(&mut hash, ingress.foreground_maximized);
    flag(&mut hash, ingress.foreground_fullscreen);
    flag(&mut hash, ingress.foreground_cloaked);
    flag(&mut hash, ingress.foreground_hidden_workspace);
    flag(&mut hash, ingress.foreground_shell);
    feed(
        &mut hash,
        &ingress
            .foreground_frame
            .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
            .unwrap_or_default(),
    );
    sep(&mut hash);
    flag(&mut hash, ingress.border_enabled);
    flag(&mut hash, ingress.underlay_enabled);
    feed(&mut hash, &ingress.border_gap_px.to_string());
    sep(&mut hash);
    feed(&mut hash, &ingress.border_width_px.to_string());
    sep(&mut hash);
    feed(&mut hash, &ingress.underlay_extension_px.to_string());
    sep(&mut hash);
    flag(&mut hash, ingress.chord_held);
    feed(&mut hash, &ingress.move_window);
    sep(&mut hash);
    flag(&mut hash, ingress.paused);
    for window in &ingress.pager_order {
        feed(&mut hash, &window.0);
        sep(&mut hash);
    }
    for row in &ingress.pager_frames {
        feed(&mut hash, &row.window.0);
        sep(&mut hash);
        for slot in [&row.live, &row.float_frame, &row.tile_frame] {
            match slot {
                Some(rect) => {
                    flag(&mut hash, true);
                    feed(
                        &mut hash,
                        &format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h),
                    );
                    sep(&mut hash);
                }
                None => flag(&mut hash, false),
            }
        }
    }
    // Opaque host storage return: every carried record separates the
    // digest by exact subject identity, order, and raw data bytes
    // (duplicate detection compares full typed input equality).
    for record in &report.intent_records {
        feed(&mut hash, &record.win.0);
        sep(&mut hash);
        feed(&mut hash, &record.data);
        sep(&mut hash);
    }
    hash
}

fn workspace_ref_fingerprint(reference: &WorkspaceRef) -> String {
    match reference {
        WorkspaceRef::Logical { output, workspace } => {
            format!("logical/{}/{}", output.0, workspace.0)
        }
        WorkspaceRef::Native { output, workspace } => {
            format!("native/{}/{}", output.0, workspace.as_str())
        }
    }
}

/// Validate a world report shape without echoing payload bytes. Every
/// carried string is gated (opaque ids, generation token, closed direction
/// vocabularies, bounded numeric ranges) and every section is capped, so
/// the estimators and the transition can rely on bounded input. Explicit
/// closes are ALWAYS allowed (even with incomplete inventory); only implicit
/// missing=closed inference requires completeness, and that inference lives
/// in the transition, not here.
///
/// Pure shape gate: directly constructed identifiable invalid scale or
/// identity rows refuse here. The engine grades those rows to narrow holds
/// first (see [`grade_invalid_screens`]); callers that bypass the engine
/// see the refusal.
#[must_use]
pub fn validate_report(report: &WorldReport) -> bool {
    validate_report_reason(report).is_none()
}

/// Grade directly constructed identifiable invalid screen rows to narrow
/// holds before engine authority/readiness: the protocol codec already
/// holds these rows at decode, while [`validate_report_reason`] refuses
/// them. Grading closes that causal gap without weakening any structural
/// gate.
///
/// A screens row is gradable only when it is identifiable (a valid opaque
/// output id) and its only violations are the v4 scale/metadata values
/// (non-finite or non-positive scale, identity text over
/// [`MAX_IDENTITY_TEXT_BYTES`] UTF-8 bytes). Structural violations (bad
/// ids, duplicates, counts, geometry, complete-topology claims) still
/// refuse: grading re-runs [`validate_report_reason`] on the normalized
/// report and returns `None` unless it passes.
///
/// Normalization preserves every carried fact: graded rows take the free
/// carried-array slots as held ordinals in row order (pre-existing held
/// ordinals are preserved, valid rows keep relative order), fields are
/// retained in canonical schema order (`out`, `full`, `work`, `complete`,
/// `scale`, `identity`; typed rows carry no extras), and each graded row
/// carries an `InvalidFact` screen diagnostic over its output scope,
/// exactly like its decoded twin. A non-finite scale is retained
/// faithfully as a held number fact: the checked canonical writers refuse
/// to spell it (never a silent `null`); only the pre-validation estimator
/// bounds it.
#[must_use]
pub fn grade_invalid_screens(report: &WorldReport) -> Option<WorldReport> {
    if !matches!(
        validate_report_reason(report),
        Some("screen-scale-invalid" | "screen-identity-invalid")
    ) {
        return None;
    }
    {
        // Structural pre-check over EVERY carried row before any removal:
        // validation stops at the first scale/identity failure, which could
        // hide a structural violation in a later row that removal would
        // then erase. Any structural failure refuses the whole report.
        let mut seen = BTreeSet::new();
        for screen in &report.screens {
            if check_screen_structure(screen, &mut seen).is_some() {
                return None;
            }
        }
    }
    let total = report
        .screens
        .len()
        .saturating_add(report.held_screens.len());
    let used: BTreeSet<usize> = report
        .held_screens
        .iter()
        .map(|held| held.ordinal)
        .collect();
    let mut free = (0..total).filter(|slot| !used.contains(slot));
    let mut screens = Vec::with_capacity(report.screens.len());
    let mut graded = Vec::new();
    for screen in &report.screens {
        let slot = free.next()?;
        if gradable_screen(screen) {
            graded.push(graded_held_row(screen, slot));
        } else {
            screens.push(screen.clone());
        }
    }
    if graded.is_empty() {
        return None;
    }
    let mut normalized = report.clone();
    normalized.screens = screens;
    normalized.held_screens.extend(graded);
    normalized.held_screens.sort_by_key(|held| held.ordinal);
    if validate_report_reason(&normalized).is_some() {
        return None;
    }
    Some(normalized)
}

/// Structural per-screen checks shared by [`validate_report_reason`] and
/// [`grade_invalid_screens`]: output id, duplicate output, rect-pair shape,
/// and the complete-topology claim. Scale and identity value checks are NOT
/// included: validation reports them after these, grading treats them as
/// gradable. One policy, two call sites, identical reasons.
///
/// Also shared by the protocol decoder, which must refuse topology
/// violations before holding a row for bad scale, overbound identity, or
/// retired keys: hidden to keep the narrow hold vocabulary the only
/// public surface.
#[doc(hidden)]
pub fn check_screen_structure(
    screen: &ScreenReport,
    seen_outputs: &mut BTreeSet<String>,
) -> Option<&'static str> {
    if !is_opaque_id(&screen.output.0) {
        return Some("screen-id-invalid");
    }
    if !seen_outputs.insert(screen.output.0.clone()) {
        return Some("duplicate-screen");
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
    None
}

/// True when a screens row is identifiable and its only violations are the
/// gradable v4 scale/metadata values (see [`grade_invalid_screens`]).
fn gradable_screen(screen: &ScreenReport) -> bool {
    if !is_opaque_id(&screen.output.0) {
        return false;
    }
    if !screen.scale.is_finite() || screen.scale <= 0.0 {
        return true;
    }
    [
        screen.identity.manufacturer.as_deref(),
        screen.identity.model.as_deref(),
        screen.identity.serial.as_deref(),
        screen.identity.connector.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.len() > MAX_IDENTITY_TEXT_BYTES)
}

/// Retain one directly constructed invalid screen row as a narrow hold,
/// mirroring the protocol codec's retained fields and diagnostic.
fn graded_held_row(screen: &ScreenReport, ordinal: usize) -> HeldScreenRow {
    use crate::connection::RawSettingValue;
    fn rect_value(rect: Option<Rect>) -> RawSettingValue {
        rect.map_or(RawSettingValue::Null, |row| {
            RawSettingValue::Array(vec![
                RawSettingValue::Number(f64::from(row.x)),
                RawSettingValue::Number(f64::from(row.y)),
                RawSettingValue::Number(f64::from(row.w)),
                RawSettingValue::Number(f64::from(row.h)),
            ])
        })
    }
    fn identity_text(text: &Option<String>) -> RawSettingValue {
        text.as_deref().map_or(RawSettingValue::Null, |value| {
            RawSettingValue::String(value.to_owned())
        })
    }
    HeldScreenRow {
        output: screen.output.clone(),
        ordinal,
        fields: vec![
            (
                "out".to_owned(),
                RawSettingValue::String(screen.output.0.clone()),
            ),
            ("full".to_owned(), rect_value(screen.full_rect)),
            ("work".to_owned(), rect_value(screen.work_area)),
            (
                "complete".to_owned(),
                RawSettingValue::Bool(screen.complete),
            ),
            ("scale".to_owned(), RawSettingValue::Number(screen.scale)),
            (
                "identity".to_owned(),
                RawSettingValue::Object(vec![
                    (
                        "manufacturer".to_owned(),
                        identity_text(&screen.identity.manufacturer),
                    ),
                    ("model".to_owned(), identity_text(&screen.identity.model)),
                    ("serial".to_owned(), identity_text(&screen.identity.serial)),
                    (
                        "connector".to_owned(),
                        identity_text(&screen.identity.connector),
                    ),
                ]),
            ),
        ],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(screen.output.clone()),
            row: RowDiagnosticLane::Screen,
            id: Some(screen.output.0.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    }
}

/// Canonical schema key order for carried window rows, mirrored by the
/// protocol decoder, the estimators, and the canonical writer.
#[doc(hidden)]
pub const WINDOW_SCHEMA_ORDER: &[&str] = &[
    "win",
    "out",
    "rect",
    "fullscreen",
    "maximized",
    "sticky",
    "minimized",
    "hints",
    "nativeWs",
    "hostHidden",
    "privileged",
    "managed",
    "acceptsInput",
    "activationInhibited",
    "toolWindow",
    "owned",
    "transient",
    "modal",
    "resizable",
    "above",
    "below",
    "owner",
    "parent",
    "role",
    "appId",
    "executable",
    "processId",
    "class",
    "title",
    "type",
    "activationPolicy",
    "layer",
    "fullscreenOwner",
    "fullscreenExitable",
    "suppressionAvailable",
    "visibility",
    "lastWrite",
];

/// Structural per-window checks shared by [`validate_report_reason`] and
/// the window graders: lifetime id, output id shape, duplicate lifetime,
/// and rect-pair shape. An unknown `out` (valid id naming no live screen)
/// is valid here: the engine retains the prior home and holds the scope,
/// never guessing the current output. Metadata bounds, visibility/lastWrite
/// shape, owner/parent/native/process grammars, and closed-overlap are NOT
/// included: validation reports those after these, grading treats the
/// gradable subset as holds. One policy, three call sites, identical
/// reasons.
///
/// Also shared by the protocol decoder, which must refuse id/duplicate/rect
/// violations before holding a row for bad metadata, retired keys, or
/// conflicting facts: hidden to keep the narrow hold vocabulary the only
/// public surface.
#[doc(hidden)]
pub fn check_window_structure(
    fact: &WindowFact,
    seen_windows: &mut BTreeSet<String>,
) -> Option<&'static str> {
    if !is_opaque_id(&fact.window.0) {
        return Some("window-id-invalid");
    }
    if let Some(out) = fact.output.as_ref()
        && !is_opaque_id(&out.0)
    {
        return Some("window-id-invalid");
    }
    if !seen_windows.insert(fact.window.0.clone()) {
        return Some("duplicate-window");
    }
    if let Some(rect) = fact.rect
        && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
    {
        return Some("window-rect-invalid");
    }
    None
}

/// Hidden structural id violations masked inside a gradable row: bad
/// owner/parent references or a bad native workspace id. These refuse the
/// whole report like other id shapes; they are never graded to a hold even
/// when the same row also carries overbound text or a bad process id.
fn hidden_window_id_violation(fact: &WindowFact) -> bool {
    for id in [&fact.owner, &fact.parent].into_iter().flatten() {
        if !is_opaque_id(&id.0) {
            return true;
        }
    }
    if let Some(native) = fact.native_workspace.as_deref()
        && !is_opaque_id(native)
    {
        return true;
    }
    false
}

/// True when a window row is identifiable and its only violations are the
/// gradable v4 value failures (metadata text over
/// [`MAX_METADATA_TEXT_BYTES`] UTF-8 bytes, or a non-grammatical optional
/// process id). Structural violations (bad ids, duplicates, geometry)
/// still refuse: grading re-runs [`validate_report_reason`] on the
/// normalized report and returns `None` unless it passes.
fn gradable_window(fact: &WindowFact) -> bool {
    if !is_opaque_id(&fact.window.0) {
        return false;
    }
    if let Some(out) = fact.output.as_ref()
        && !is_opaque_id(&out.0)
    {
        return false;
    }
    // Hidden id violations are never gradable: grading them would erase
    // the refusal that validation reports for the same row.
    if hidden_window_id_violation(fact) {
        return false;
    }
    if [
        fact.role.as_deref(),
        fact.app_id.as_deref(),
        fact.executable.as_deref(),
        fact.class.as_deref(),
        fact.title.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|field| field.len() > MAX_METADATA_TEXT_BYTES)
    {
        return true;
    }
    if let Some(process) = fact.process_id.as_deref()
        && !crate::connection::valid_process_identity_text(process)
    {
        return true;
    }
    false
}

/// Retain one directly constructed invalid window row as a narrow hold,
/// mirroring the protocol codec's retained fields and diagnostic: a
/// diagnostic only for the row's actual reported output, never a
/// fabricated scope.
fn graded_held_window_row(fact: &WindowFact, ordinal: usize) -> HeldWindowRow {
    HeldWindowRow {
        window: fact.window.clone(),
        ordinal,
        output: fact.output.clone(),
        fields: retained_window_fields(fact),
        diagnostic: fact.output.as_ref().map(|out| RowDiagnostic {
            scope: AuthorityScope::Output(out.clone()),
            row: RowDiagnosticLane::Window,
            id: Some(fact.window.0.clone()),
            kind: RowDiagnosticKind::InvalidFact,
        }),
        kind: RowDiagnosticKind::InvalidFact,
    }
}

/// Retain every carried window fact in canonical schema order
/// ([`WINDOW_SCHEMA_ORDER`]) as JSON-agnostic facts, with the nested
/// `hints`, `visibility`, and `lastWrite` objects in schema order. Used by
/// both the invalid and the conflicting graders: one retention, two
/// diagnostics.
pub(crate) fn retained_window_fields(
    fact: &WindowFact,
) -> Vec<(String, crate::connection::RawSettingValue)> {
    use crate::connection::RawSettingValue;
    fn opt_bool(value: Option<bool>) -> RawSettingValue {
        value.map_or(RawSettingValue::Null, RawSettingValue::Bool)
    }
    fn opt_text(value: &Option<String>) -> RawSettingValue {
        value.as_deref().map_or(RawSettingValue::Null, |text| {
            RawSettingValue::String(text.to_owned())
        })
    }
    fn opt_id(value: &Option<WindowId>) -> RawSettingValue {
        value.as_ref().map_or(RawSettingValue::Null, |id| {
            RawSettingValue::String(id.0.clone())
        })
    }
    fn rect_value(rect: Option<Rect>) -> RawSettingValue {
        rect.map_or(RawSettingValue::Null, |row| {
            RawSettingValue::Array(vec![
                RawSettingValue::Number(f64::from(row.x)),
                RawSettingValue::Number(f64::from(row.y)),
                RawSettingValue::Number(f64::from(row.w)),
                RawSettingValue::Number(f64::from(row.h)),
            ])
        })
    }
    fn visibility_value(visibility: VisibilityFact) -> RawSettingValue {
        RawSettingValue::Object(vec![
            ("visible".to_owned(), opt_bool(visibility.visible)),
            (
                "suppressionOwned".to_owned(),
                opt_bool(visibility.suppression_owned),
            ),
        ])
    }
    fn sample_value(sample: &VisibilitySample) -> RawSettingValue {
        RawSettingValue::Object(vec![
            (
                "obs".to_owned(),
                RawSettingValue::String(sample.obs.to_string()),
            ),
            (
                "lifetimeMatched".to_owned(),
                opt_bool(sample.lifetime_matched),
            ),
            ("visibility".to_owned(), visibility_value(sample.visibility)),
            ("minimized".to_owned(), opt_bool(sample.minimized)),
            ("hostHidden".to_owned(), opt_bool(sample.host_hidden)),
        ])
    }
    fn last_write_value(last: &LastWrite) -> RawSettingValue {
        RawSettingValue::Object(vec![
            (
                "property".to_owned(),
                RawSettingValue::String(last.property.as_str().to_owned()),
            ),
            ("target".to_owned(), RawSettingValue::Bool(last.target)),
            (
                "revision".to_owned(),
                RawSettingValue::String(last.revision.to_string()),
            ),
            (
                "path".to_owned(),
                RawSettingValue::String(last.path.as_str().to_owned()),
            ),
            (
                "setter".to_owned(),
                RawSettingValue::String(last.setter.as_str().to_owned()),
            ),
            (
                "reason".to_owned(),
                RawSettingValue::String(last.reason.as_str().to_owned()),
            ),
            (
                "pre".to_owned(),
                last.pre
                    .as_ref()
                    .map_or(RawSettingValue::Null, sample_value),
            ),
            (
                "post".to_owned(),
                last.post
                    .as_ref()
                    .map_or(RawSettingValue::Null, sample_value),
            ),
        ])
    }
    fn native_value(native: &NativeWorkspaceFact) -> Option<RawSettingValue> {
        match native {
            NativeWorkspaceFact::Omitted => None,
            NativeWorkspaceFact::Null => Some(RawSettingValue::Null),
            NativeWorkspaceFact::Value(id) => Some(RawSettingValue::String(id.clone())),
        }
    }
    let mut fields = vec![
        (
            "win".to_owned(),
            RawSettingValue::String(fact.window.0.clone()),
        ),
        (
            "out".to_owned(),
            fact.output.as_ref().map_or(RawSettingValue::Null, |out| {
                RawSettingValue::String(out.0.clone())
            }),
        ),
        ("rect".to_owned(), rect_value(fact.rect)),
        ("fullscreen".to_owned(), opt_bool(fact.fullscreen)),
        ("maximized".to_owned(), opt_bool(fact.maximized)),
        ("sticky".to_owned(), opt_bool(fact.sticky)),
        ("minimized".to_owned(), opt_bool(fact.minimized)),
        (
            "hints".to_owned(),
            fact.hints.map_or(RawSettingValue::Null, |hints| {
                RawSettingValue::Object(vec![
                    (
                        "minW".to_owned(),
                        hints.min_w.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "minH".to_owned(),
                        hints.min_h.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "maxW".to_owned(),
                        hints.max_w.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                    (
                        "maxH".to_owned(),
                        hints.max_h.map_or(RawSettingValue::Null, |bound| {
                            RawSettingValue::Number(f64::from(bound))
                        }),
                    ),
                ])
            }),
        ),
    ];
    if let Some(native) = native_value(&fact.native_workspace) {
        fields.push(("nativeWs".to_owned(), native));
    }
    fields.extend([
        ("hostHidden".to_owned(), opt_bool(fact.host_hidden)),
        ("privileged".to_owned(), opt_bool(fact.privileged)),
        ("managed".to_owned(), opt_bool(fact.managed)),
        ("acceptsInput".to_owned(), opt_bool(fact.accepts_input)),
        (
            "activationInhibited".to_owned(),
            opt_bool(fact.activation_inhibited),
        ),
        ("toolWindow".to_owned(), opt_bool(fact.tool_window)),
        ("owned".to_owned(), opt_bool(fact.owned)),
        ("transient".to_owned(), opt_bool(fact.transient)),
        ("modal".to_owned(), opt_bool(fact.modal)),
        ("resizable".to_owned(), opt_bool(fact.resizable)),
        ("above".to_owned(), opt_bool(fact.above)),
        ("below".to_owned(), opt_bool(fact.below)),
        ("owner".to_owned(), opt_id(&fact.owner)),
        ("parent".to_owned(), opt_id(&fact.parent)),
        ("role".to_owned(), opt_text(&fact.role)),
        ("appId".to_owned(), opt_text(&fact.app_id)),
        ("executable".to_owned(), opt_text(&fact.executable)),
        ("processId".to_owned(), opt_text(&fact.process_id)),
        ("class".to_owned(), opt_text(&fact.class)),
        ("title".to_owned(), opt_text(&fact.title)),
        (
            "type".to_owned(),
            fact.window_type.map_or(RawSettingValue::Null, |kind| {
                RawSettingValue::String(kind.as_str().to_owned())
            }),
        ),
        (
            "activationPolicy".to_owned(),
            fact.activation_policy
                .map_or(RawSettingValue::Null, |policy| {
                    RawSettingValue::String(policy.as_str().to_owned())
                }),
        ),
        (
            "layer".to_owned(),
            fact.layer.map_or(RawSettingValue::Null, |layer| {
                RawSettingValue::Number(f64::from(layer))
            }),
        ),
        (
            "fullscreenOwner".to_owned(),
            fact.fullscreen_owner
                .map_or(RawSettingValue::Null, |owner| {
                    RawSettingValue::String(owner.as_str().to_owned())
                }),
        ),
        (
            "fullscreenExitable".to_owned(),
            opt_bool(fact.fullscreen_exitable),
        ),
        (
            "suppressionAvailable".to_owned(),
            opt_bool(fact.suppression_available),
        ),
        ("visibility".to_owned(), visibility_value(fact.visibility)),
        (
            "lastWrite".to_owned(),
            fact.last_write
                .as_ref()
                .map_or(RawSettingValue::Null, last_write_value),
        ),
    ]);
    fields
}

/// Grade directly constructed identifiable invalid window rows to narrow
/// holds before engine authority/readiness: the protocol codec already
/// holds these rows at decode, while [`validate_report_reason`] refuses
/// them. Grading closes that causal gap without weakening any structural
/// gate.
///
/// A window row is gradable only when it is identifiable (a valid opaque
/// lifetime id with a valid-or-absent output) and its only violations are
/// the v4 metadata/process values (overbound text, non-grammatical optional
/// process id). Structural violations (bad ids, duplicates, geometry,
/// closed-overlap) still refuse: grading re-runs
/// [`validate_report_reason`] on the normalized report and returns `None`
/// unless it passes.
///
/// Normalization preserves every carried fact: graded rows take the free
/// carried-array slots as held ordinals in row order (pre-existing held
/// ordinals are preserved, valid rows keep relative order), fields are
/// retained in canonical schema order, and each graded row carries an
/// `InvalidFact` window diagnostic over its reported output scope (or the
/// window-id output handle when the row carries no output).
#[must_use]
pub fn grade_invalid_windows(report: &WorldReport) -> Option<WorldReport> {
    if !matches!(
        validate_report_reason(report),
        Some("window-metadata-invalid" | "window-identity-invalid")
    ) {
        return None;
    }
    {
        // Structural pre-check over EVERY carried row before any removal:
        // validation stops at the first metadata failure, which could hide
        // a structural violation in a later row that removal would then
        // erase. Any structural failure refuses the whole report.
        let mut seen = BTreeSet::new();
        for fact in &report.windows {
            if check_window_structure(fact, &mut seen).is_some() {
                return None;
            }
            // A gradable overbound/bad-process row must not mask a hidden
            // owner/parent/native violation in the same row.
            if hidden_window_id_violation(fact) {
                return None;
            }
        }
        for held in &report.held_windows {
            if !is_opaque_id(&held.window.0) || !seen.insert(held.window.0.clone()) {
                return None;
            }
        }
    }
    let total = report
        .windows
        .len()
        .saturating_add(report.held_windows.len());
    let used: BTreeSet<usize> = report
        .held_windows
        .iter()
        .map(|held| held.ordinal)
        .collect();
    let mut free = (0..total).filter(|slot| !used.contains(slot));
    let mut windows = Vec::with_capacity(report.windows.len());
    let mut graded = Vec::new();
    for fact in &report.windows {
        let slot = free.next()?;
        if gradable_window(fact) {
            graded.push(graded_held_window_row(fact, slot));
        } else {
            windows.push(fact.clone());
        }
    }
    if graded.is_empty() {
        return None;
    }
    let mut normalized = report.clone();
    normalized.windows = windows;
    normalized.held_windows.extend(graded);
    normalized.held_windows.sort_by_key(|held| held.ordinal);
    if validate_report_reason(&normalized).is_some() {
        return None;
    }
    Some(normalized)
}

/// True when a window row contradicts the connection's unavailable facts: a
/// listed-unavailable classification input carried non-null. Required
/// identity/geometry/visibility inputs (win, out, rect, hints, visibility,
/// lastWrite, fullscreen/maximized/sticky/minimized state, nativeWs,
/// fullscreen/layer ownership and suppression support) are never waived by
/// this list and never conflict. Sticky contradicts separately: without
/// `nativeSticky` support any non-null sticky (false or true) conflicts.
fn conflicting_window(
    fact: &WindowFact,
    unavailable: &[crate::connection::FactName],
    native_sticky_supported: bool,
    maximize_supported: bool,
) -> bool {
    if !native_sticky_supported && fact.sticky.is_some() {
        return true;
    }
    if !maximize_supported && fact.maximized.is_some() {
        return true;
    }
    use crate::connection::FactName;
    unavailable.iter().any(|name| match name {
        FactName::Privileged => fact.privileged.is_some(),
        FactName::Managed => fact.managed.is_some(),
        FactName::AcceptsInput => fact.accepts_input.is_some(),
        FactName::ActivationInhibited => fact.activation_inhibited.is_some(),
        FactName::ToolWindow => fact.tool_window.is_some(),
        FactName::Owned => fact.owned.is_some(),
        FactName::Transient => fact.transient.is_some(),
        FactName::Modal => fact.modal.is_some(),
        FactName::Resizable => fact.resizable.is_some(),
        FactName::Above => fact.above.is_some(),
        FactName::Below => fact.below.is_some(),
        FactName::Owner => fact.owner.is_some(),
        FactName::Parent => fact.parent.is_some(),
        FactName::Role => fact.role.is_some(),
        FactName::AppId => fact.app_id.is_some(),
        FactName::Executable => fact.executable.is_some(),
        FactName::ProcessId => fact.process_id.is_some(),
        FactName::Class => fact.class.is_some(),
        FactName::Title => fact.title.is_some(),
        FactName::ActivationPolicy => fact.activation_policy.is_some(),
        FactName::Layer => fact.layer.is_some(),
    })
}

/// Grade unavailable-fact contradictions to narrow holds before engine
/// authority/readiness. The protocol decoder cannot do this: it has no
/// connection capabilities. Rows already held (or gradable-invalid, which
/// the invalid grader owns) are skipped, never double-held. Structural
/// pre-checks cover every carried row including masked (already held) rows:
/// any id/duplicate/rect violation refuses the whole report instead of
/// grading around it.
///
/// Capability-aware for sticky and maximize: without `nativeSticky`
/// support any non-null sticky (false or true) grades to the same narrow
/// hold, and without `maximize` support any non-null maximized (false or
/// true) does likewise. A composed-route `sticky: null` or
/// `maximized: null` never grades.
///
/// Normalization mirrors [`grade_invalid_windows`]: conflicting rows move to
/// `held_windows` with their fields retained and a `ConflictingFact`
/// window diagnostic.
#[must_use]
pub fn grade_conflicting_windows(
    report: &WorldReport,
    unavailable: &[crate::connection::FactName],
    native_sticky_supported: bool,
    maximize_supported: bool,
) -> Option<WorldReport> {
    if unavailable.is_empty() && native_sticky_supported && maximize_supported {
        return None;
    }
    {
        let mut seen = BTreeSet::new();
        for held in &report.held_windows {
            if !is_opaque_id(&held.window.0) || !seen.insert(held.window.0.clone()) {
                return None;
            }
        }
        for fact in &report.windows {
            if check_window_structure(fact, &mut seen).is_some() {
                return None;
            }
            // A conflicting row must not mask a hidden
            // owner/parent/native violation in the same row.
            if hidden_window_id_violation(fact) {
                return None;
            }
        }
    }
    let total = report
        .windows
        .len()
        .saturating_add(report.held_windows.len());
    let used: BTreeSet<usize> = report
        .held_windows
        .iter()
        .map(|held| held.ordinal)
        .collect();
    let mut free = (0..total).filter(|slot| !used.contains(slot));
    let mut windows = Vec::with_capacity(report.windows.len());
    let mut graded = Vec::new();
    for fact in &report.windows {
        let slot = free.next()?;
        // Invalid-gradable rows belong to the invalid grader; conflicting
        // grading never double-holds them.
        if !gradable_window(fact)
            && conflicting_window(
                fact,
                unavailable,
                native_sticky_supported,
                maximize_supported,
            )
        {
            graded.push(HeldWindowRow {
                window: fact.window.clone(),
                ordinal: slot,
                output: fact.output.clone(),
                fields: retained_window_fields(fact),
                diagnostic: fact.output.as_ref().map(|out| RowDiagnostic {
                    scope: AuthorityScope::Output(out.clone()),
                    row: RowDiagnosticLane::Window,
                    id: Some(fact.window.0.clone()),
                    kind: RowDiagnosticKind::ConflictingFact,
                }),
                kind: RowDiagnosticKind::ConflictingFact,
            });
        } else {
            windows.push(fact.clone());
        }
    }
    if graded.is_empty() {
        return None;
    }
    let mut normalized = report.clone();
    normalized.windows = windows;
    normalized.held_windows.extend(graded);
    normalized.held_windows.sort_by_key(|held| held.ordinal);
    if validate_report_reason(&normalized).is_some() {
        return None;
    }
    Some(normalized)
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
    if !is_opaque_id(&report.adapter_incarnation) || !is_opaque_id(&report.request_id) {
        return Some("identity-invalid");
    }
    if !crate::ids::is_generation_id(&report.core_generation) {
        return Some("generation-invalid");
    }
    if report
        .screens
        .len()
        .saturating_add(report.held_screens.len())
        > MAX_WORLD_SCREENS
        || report
            .windows
            .len()
            .saturating_add(report.held_windows.len())
            > MAX_WORLD_FACTS
        || report.closed.len() > MAX_WORLD_CLOSED
        || report.views.len() > MAX_WORLD_VIEWS
        || report.native.len() > MAX_WORLD_SCREENS
        || report.frames.len() > MAX_WORLD_FRAMES
        || report.member_attempts.len() > MAX_WORLD_MEMBER_ATTEMPTS
        || report.view_attempts.len() > MAX_WORLD_VIEW_ATTEMPTS
        || report.clear_attempts.len() > MAX_WORLD_CLEAR_ATTEMPTS
        || report.creation_attempts.len() > MAX_WORLD_CREATION_ATTEMPTS
        || report.state_attempts.len() > MAX_WORLD_STATE_ATTEMPTS
        || report.intent_records.len() > MAX_INTENT_RECORDS
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
    if let Some(primary) = report.primary_output.as_ref()
        && !is_opaque_id(&primary.0)
    {
        return Some("primary-invalid");
    }
    let mut seen_windows = BTreeSet::new();
    let mut seen_outputs = BTreeSet::new();
    for screen in &report.screens {
        if let Some(reason) = check_screen_structure(screen, &mut seen_outputs) {
            return Some(reason);
        }
        // v4 ingress shape: finite positive scale plus 256-byte identity
        // texts. The protocol codec holds identifiable bad rows narrowly
        // instead of producing these typed violations; this gate refuses
        // directly constructed violations before mutation.
        if !screen.scale.is_finite() || screen.scale <= 0.0 {
            return Some("screen-scale-invalid");
        }
        for field in [
            screen.identity.manufacturer.as_deref(),
            screen.identity.model.as_deref(),
            screen.identity.serial.as_deref(),
            screen.identity.connector.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if field.len() > MAX_IDENTITY_TEXT_BYTES {
                return Some("screen-identity-invalid");
            }
        }
    }
    for held in &report.held_screens {
        if !is_opaque_id(&held.output.0) {
            return Some("screen-id-invalid");
        }
        // A held row and a valid row never share an output: one live
        // handle carries exactly one row per report.
        if !seen_outputs.insert(held.output.0.clone()) {
            return Some("duplicate-screen");
        }
    }
    {
        // Held ordinals are the rows' original `screens` indexes: each one
        // names a distinct slot in the carried array, so a future exact
        // request encoder can reconstruct the original order.
        let total = report
            .screens
            .len()
            .saturating_add(report.held_screens.len());
        let mut seen_ordinals = BTreeSet::new();
        for held in &report.held_screens {
            if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                return Some("held-ordinal-invalid");
            }
        }
    }
    {
        for fact in &report.windows {
            if let Some(reason) = check_window_structure(fact, &mut seen_windows) {
                return Some(reason);
            }
            // Nullable metadata text is bounded: overbound text holds the
            // identifiable row narrowly (gradable below) instead of
            // refusing the report. Identity-grammar process ids refuse
            // like other id shapes: only the metadata text bound grades.
            for field in [
                fact.role.as_deref(),
                fact.app_id.as_deref(),
                fact.executable.as_deref(),
                fact.class.as_deref(),
                fact.title.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if field.len() > MAX_METADATA_TEXT_BYTES {
                    return Some("window-metadata-invalid");
                }
            }
            if let Some(native) = fact.native_workspace.as_deref()
                && !is_opaque_id(native)
            {
                return Some("window-native-invalid");
            }
            if let Some(process) = fact.process_id.as_deref()
                && !crate::connection::valid_process_identity_text(process)
            {
                return Some("window-identity-invalid");
            }
            for id in [&fact.owner, &fact.parent].into_iter().flatten() {
                if !is_opaque_id(&id.0) {
                    return Some("window-id-invalid");
                }
            }
        }
        // Held rows keep their window handle unique against the valid rows:
        // one lifetime carries exactly one row per report.
        for held in &report.held_windows {
            if !is_opaque_id(&held.window.0) {
                return Some("window-id-invalid");
            }
            if !seen_windows.insert(held.window.0.clone()) {
                return Some("duplicate-window");
            }
        }
    }
    {
        // Held ordinals are the rows' original `windows` indexes: each one
        // names a distinct slot in the carried array, so a future exact
        // request encoder can reconstruct the original order.
        let total = report
            .windows
            .len()
            .saturating_add(report.held_windows.len());
        let mut seen_ordinals = BTreeSet::new();
        for held in &report.held_windows {
            if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                return Some("held-ordinal-invalid");
            }
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
    {
        let mut seen_native_outputs = BTreeSet::new();
        for native in &report.native {
            // Strict codec: `current_native` is `Some` (an ordered desktop
            // id) or `None` (wire `null`, the current surface is not an
            // ordered desktop). No empty-string or missing tolerance is
            // invented here; the codec refuses those shapes.
            if !is_opaque_id(&native.output.0)
                || native
                    .current_native
                    .as_deref()
                    .is_some_and(|current| !is_opaque_id(current))
                || native.native_order.len() > MAX_NATIVE_PER_OUTPUT
            {
                return Some("native-invalid");
            }
            if !seen_outputs.contains(&native.output.0) {
                return Some("native-output-unknown");
            }
            if !seen_native_outputs.insert(native.output.0.clone()) {
                return Some("duplicate-native");
            }
            {
                let mut seen_ids = BTreeSet::new();
                for id in &native.native_order {
                    if !is_opaque_id(id) {
                        return Some("native-id-invalid");
                    }
                    if !seen_ids.insert(id.clone()) {
                        return Some("duplicate-native");
                    }
                }
            }
            {
                // v3 required complete occupancy: exactly one row per
                // ordered id, no uncovered order id, no occupancy id
                // outside the order, no duplicate occupancy rows.
                // `count: None` is explicit unknown (never fabricated
                // zero); exact zero is only valid on readable empty.
                if native.occupancy.len() != native.native_order.len() {
                    return Some("native-occupancy-incomplete");
                }
                let mut seen_occupancy = BTreeSet::new();
                for row in &native.occupancy {
                    if !is_opaque_id(&row.native) {
                        return Some("native-occupancy-invalid");
                    }
                    if !seen_occupancy.insert(row.native.clone()) {
                        return Some("duplicate-native-occupancy");
                    }
                    if row
                        .count
                        .is_some_and(|count| count > MAX_NATIVE_OCCUPANTS_PER_DESKTOP)
                    {
                        return Some("native-occupancy-invalid");
                    }
                }
                for id in &native.native_order {
                    if !seen_occupancy.contains(id) {
                        return Some("native-occupancy-incomplete");
                    }
                }
            }
            total_native += native.native_order.len();
        }
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
            for reference in [&attempt.pre, &attempt.post].into_iter().flatten() {
                if !valid_attempt_workspace_ref(reference) {
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
            for reference in [&attempt.pre, &attempt.post, &attempt.baseline]
                .into_iter()
                .flatten()
            {
                if !valid_attempt_workspace_ref(reference) {
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
    {
        // One latest-wins row per window property: the same window naming
        // two different revisions for one property is still a duplicate
        // entry for that property, never two settled facts.
        let mut seen_states = BTreeSet::new();
        for attempt in &report.state_attempts {
            if !is_opaque_id(&attempt.window.0) {
                return Some("state-window-invalid");
            }
            if !seen_states.insert((attempt.window.0.clone(), attempt.property)) {
                return Some("duplicate-state-attempt");
            }
        }
    }
    {
        // Capability-named note: one execution authorization per op; exact
        // op-plus-logical-plus-output match, strict order bounds, replay
        // idempotent by op id. One op naming two triples is contradictory
        // evidence and refuses outright (never a silent absent-fallback for
        // the wrong row). Timestamp monotonicity rides the report
        // `monotonic_ms` pairing at use; the digest covers every stamp.
        let mut creation_triples: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut creation_receipts: BTreeSet<String> = BTreeSet::new();
        let mut total_creation_ids = 0usize;
        for attempt in &report.creation_attempts {
            if !is_opaque_id(&attempt.operation_id)
                || !is_opaque_id(&attempt.logical_workspace)
                || !is_opaque_id(&attempt.target_output.0)
            {
                return Some("creation-target-invalid");
            }
            if let Some(created) = attempt.created_native.as_deref()
                && !is_opaque_id(created)
            {
                return Some("creation-id-invalid");
            }
            if attempt.order_before.len() > MAX_NATIVE_PER_OUTPUT
                || attempt.order_after.len() > MAX_NATIVE_PER_OUTPUT
            {
                return Some("creation-order-over-cap");
            }
            for id in attempt
                .order_before
                .iter()
                .chain(attempt.order_after.iter())
            {
                if !is_opaque_id(id) {
                    return Some("creation-id-invalid");
                }
            }
            {
                let mut seen_before = BTreeSet::new();
                for id in &attempt.order_before {
                    if !seen_before.insert(id.clone()) {
                        return Some("duplicate-creation-attempt");
                    }
                }
                let mut seen_after = BTreeSet::new();
                for id in &attempt.order_after {
                    if !seen_after.insert(id.clone()) {
                        return Some("duplicate-creation-attempt");
                    }
                }
            }
            match creation_triples.insert(
                attempt.operation_id.clone(),
                (
                    attempt.logical_workspace.clone(),
                    attempt.target_output.0.clone(),
                ),
            ) {
                None => {}
                Some(prev)
                    if prev
                        == (
                            attempt.logical_workspace.clone(),
                            attempt.target_output.0.clone(),
                        ) =>
                {
                    return Some("duplicate-creation-attempt");
                }
                Some(_) => return Some("creation-triple-mismatch"),
            }
            total_creation_ids += attempt.order_before.len() + attempt.order_after.len();
            if let Some(created) = attempt.created_native.as_deref() {
                total_creation_ids += 1;
                if !creation_receipts.insert(created.to_owned()) {
                    return Some("creation-receipt-reused");
                }
            }
        }
        if total_creation_ids > MAX_WORLD_NATIVE.saturating_mul(2) {
            return Some("creation-over-cap");
        }
    }
    // Stage-5 presentation ingress: strict shape, never payload bytes.
    // Empty foreground/move tokens mean none; non-empty tokens must be
    // opaque. Frames must be valid carried rects. Style numbers are
    // host-scaled pixels and stay within ordinary range. The pager order is
    // bounded like the window inventory.
    let ingress = &report.presentation;
    if !ingress.foreground.is_empty() && !is_opaque_id(&ingress.foreground) {
        return Some("presentation-foreground-invalid");
    }
    if !ingress.move_window.is_empty() && !is_opaque_id(&ingress.move_window) {
        return Some("presentation-move-invalid");
    }
    if let Some(frame) = ingress.foreground_frame
        && !valid_carried_rect(frame.x, frame.y, frame.w, frame.h)
    {
        return Some("presentation-frame-invalid");
    }
    for px in [
        ingress.border_gap_px,
        ingress.border_width_px,
        ingress.underlay_extension_px,
    ] {
        if !(0..=MAX_PRESENTATION_STYLE_PX).contains(&px) {
            return Some("presentation-style-invalid");
        }
    }
    if ingress.pager_order.len() > MAX_PRESENTATION_ORDER {
        return Some("presentation-order-over-cap");
    }
    for window in &ingress.pager_order {
        if !is_opaque_id(&window.0) {
            return Some("presentation-order-invalid");
        }
    }
    if ingress.pager_frames.len() > MAX_WORLD_FACTS {
        return Some("presentation-frames-over-cap");
    }
    for row in &ingress.pager_frames {
        if !is_opaque_id(&row.window.0) {
            return Some("presentation-frame-invalid");
        }
        for slot in [&row.live, &row.float_frame, &row.tile_frame] {
            if let Some(rect) = slot
                && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
            {
                return Some("presentation-frame-invalid");
            }
        }
    }
    // Opaque host storage return: opaque live subjects only, and outer
    // record strings bounded to the slot (UTF-8 bytes, not chars). Inner
    // version/shape verdicts never change this envelope validation: raw
    // unsupported or malformed inner JSON stays accepted here.
    for record in &report.intent_records {
        if !is_opaque_id(&record.win.0) {
            return Some("window-id-invalid");
        }
        if record.data.len() > crate::intent_data::MAX_INTENT_DATA_BYTES {
            return Some("intent-data-invalid");
        }
    }
    None
}

/// Name the failed outbound intent save/clear check in fixed vocabulary
/// (never payload bytes): `None` when every write row is publishable,
/// otherwise the static reason token the engine carries as its rejection
/// message under kind `snapshot-invalid`.
///
/// Intent-specific typed helper only: it validates the `intentWrites`
/// outbound lane (section cap [`MAX_INTENT_WRITES`], opaque subject ids,
/// save slot bound [`crate::intent_data::MAX_INTENT_DATA_BYTES`]), never a
/// full reply. The legacy whole-reply canonical writers
/// ([`encode_reply_canonical`](crate::world_sizing::encode_reply_canonical))
/// return `String` and stay unchecked: callers must prove this helper is
/// `None` before encoding (the engine publication gate does); the writer
/// spells whatever rows it is given.
#[must_use]
pub fn validate_intent_writes_reason(writes: &[IntentWrite]) -> Option<&'static str> {
    if writes.len() > MAX_INTENT_WRITES {
        return Some("section-over-cap");
    }
    for write in writes {
        if !is_opaque_id(&write.win.0) {
            return Some("window-id-invalid");
        }
        if write
            .data
            .as_deref()
            .is_some_and(|data| data.len() > crate::intent_data::MAX_INTENT_DATA_BYTES)
        {
            return Some("intent-data-invalid");
        }
    }
    None
}

fn valid_attempt_workspace_ref(reference: &WorkspaceRef) -> bool {
    match reference {
        WorkspaceRef::Logical { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(&workspace.0)
        }
        WorkspaceRef::Native { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(workspace.as_str())
        }
    }
}

/// Validate one send source: explicit outputs carry opaque ids, implicit
/// variants are always well-formed (resolution refuses unknown sources
/// without inventing focus).
#[must_use]
pub fn valid_send_source(source: &SendSource) -> bool {
    match source {
        SendSource::Explicit(output) => is_opaque_id(&output.0),
        SendSource::HomeThenAction => true,
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
        }
        | WorldCommand::ToggleWorkspaceTiling { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(workspace)
        }
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
        } => {
            // An empty window is the no-focused-window form (start from
            // the current workspace/output); every other form names its
            // subject.
            (window.0.is_empty() || is_opaque_id(&window.0))
                && DIRECTIONS.contains(&direction.as_str())
        }
        WorldCommand::MoveDirectional {
            window, direction, ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::SendDirectional {
            window, direction, ..
        } => is_opaque_id(&window.0) && DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::MigrateDirectional { direction } => DIRECTIONS.contains(&direction.as_str()),
        WorldCommand::ToggleMaximize { window } => is_opaque_id(&window.0),
        WorldCommand::SetSticky { window, .. } | WorldCommand::ToggleSticky { window } => {
            is_opaque_id(&window.0)
        }
        WorldCommand::SetFullscreen { window, .. } | WorldCommand::ToggleFullscreen { window } => {
            is_opaque_id(&window.0)
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
        CoreCommand::Reconcile | CoreCommand::UpdateGaps | CoreCommand::ReleaseDomain => true,
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
