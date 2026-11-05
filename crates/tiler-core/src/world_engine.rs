//! Deterministic world transition over the stateful Engine (D1-D5).
//!
//! [`WorldEngine`] retains the [`crate::engine::Engine`] (D1=A) and owns the
//! logical workspace model outright: stable core workspace ids, per-screen
//! rings, views, members, history, lifecycle, and trailing. It accepts one
//! complete [`WorldReport`] per call — screens (FULL rectangles plus work
//! areas) and window facts are the only observation prerequisites — assigns
//! every fact to a logical workspace, projects each logical workspace into
//! its Engine session through the existing [`Engine::handle`] sweep, holds
//! unreadable scopes explicitly, publishes full desired geometry with the
//! existing projector, and applies the shared trailing workspace policy.
//! Native workspace domains are an explicit capability-named opt-in
//! observation and never authorize mapping, membership, or commands.
//!
//! Ordering (see `docs/changes/archive/core-full-state-replies.md`): one globally
//! monotonic `state_revision` per accepted report (even a no-op); an
//! identical latest duplicate returns the cached reply with no new revision;
//! a reused `request_seq` with different input is invalid; older requests get
//! a bounded superseded error. Command sequences are tracked separately from
//! request sequences: a fresh report carrying a consumed command updates
//! observation but never re-executes. Generation changes cancel flights,
//! ordering, and retained world state (never native windows).
//!
//! Transfers (D3=A) stay outstanding until confirmed by observation,
//! superseded by newer external input, or settled by proven refusal. While
//! outstanding, an in-flight window homes to its retained target logical
//! workspace (stripped from the source observation, presented at the target)
//! so the committed retained topology does not flap back during actuation
//! lag. Possibly-executed raw evidence holds the target without re-actuation.
//! Views use the same retention without any window. Focus/raise/stacking are
//! one-shot effects.
//!
//! Budget: the 4 MiB request estimate gates before any convergence; a
//! conservative reply upper bound gates before any mutation; the staged
//! reply is then measured with the exact canonical encoding before commit.
//! Overflow discards the staged candidate and leaves sessions, retained
//! transfers, ordering, and command consumption exactly as before: only
//! reports that pass every cheap gate pay for one engine clone, and only
//! the exact-encoded reply commits. The protocol unit reuses
//! [`crate::world_report::encode_reply_canonical`] for its own exact check.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{
    ApplicationIdentity, HandshakeReply, HandshakeRequest, HostCapabilities, ProtocolKind,
    SettingsReply, SettingsRequest,
};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::gesture::GestureSettled;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::ScopeHistory;
use crate::world_commands::CommandApplied;
use crate::world_gesture::{GestureOutcome, RetainedGesture};
use crate::world_report::{
    AuthorityScope, CommandRejectKind, CommandResult, FrameAttempt, HoldReason, ScopeAuthority,
    ScopeStatus, TilingMode, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION,
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WindowDesire, WorkspaceDesire, WorldCommand,
    WorldReply, WorldReplyV4, WorldReport, WorldReportV4, encode_reply_canonical,
    grade_conflicting_windows, grade_invalid_screens, grade_invalid_windows,
    max_intent_rollback_bound, reply_estimate, reply_upper_bound, report_digest, report_estimate,
    validate_intent_writes_reason, validate_report_reason,
};

pub(crate) const KIND_VERSION: &str = "unsupported-version";
pub(crate) const KIND_OWNER: &str = "owner-invalid";
pub(crate) const KIND_CORRELATION: &str = "correlation-invalid";
pub(crate) const KIND_GENERATION: &str = "generation-mismatch";
pub(crate) const KIND_OVERSIZED: &str = "oversized";
pub(crate) const KIND_SNAPSHOT: &str = "snapshot-invalid";
pub(crate) const KIND_OBS_STALE: &str = "observation-stale";
pub(crate) const KIND_SUPERSEDED: &str = "request-superseded";
pub(crate) const KIND_REUSED: &str = "request-reused";
pub(crate) const KIND_FOCUS: &str = "focus-unknown";
pub(crate) const KIND_SEND_TARGET: &str = "send-target-unknown";
pub(crate) const KIND_SEND_WINDOW: &str = "send-window-unknown";
pub(crate) const KIND_SEND_REFUSED: &str = "send-refused";
pub(crate) const KIND_VIEW_OUTPUT: &str = "view-output-unknown";
pub(crate) const KIND_VIEW_TARGET: &str = "view-target-unknown";
pub(crate) const KIND_IN_FLIGHT: &str = "transfer-in-flight";
pub(crate) const KIND_DIRECTION: &str = "direction-invalid";
pub(crate) const KIND_SCOPE_HELD: &str = "scope-held";
pub(crate) const KIND_REPLY_SIZE: &str = "reply-oversize";
pub(crate) const KIND_REVISION_EXHAUSTED: &str = "revision-exhausted";
pub(crate) const KIND_CREATION_EXHAUSTED: &str = "creation-exhausted";
pub(crate) const KIND_SELECT_UNKNOWN: &str = "selection-unknown";
pub(crate) const KIND_MIGRATION: &str = "migration-refused";
pub(crate) const KIND_OPERATION: &str = "operation-refused";
pub(crate) const KIND_TILING_TARGET: &str = "tiling-target-unknown";

pub(crate) const MSG_VERSION: &str = "unsupported contract version";
pub(crate) const MSG_OWNER: &str = "owner is invalid";
pub(crate) const MSG_CORRELATION: &str = "correlation id is invalid";
pub(crate) const MSG_GENERATION: &str = "generation does not match the core generation";
pub(crate) const MSG_OVERSIZED: &str = "request exceeds size bound";
pub(crate) const MSG_SNAPSHOT: &str = "observation does not cover the known window set";
pub(crate) const MSG_OBS_STALE: &str = "observation sequence is older than the accepted watermark";
pub(crate) const MSG_SUPERSEDED: &str = "request sequence is older than the accepted request";
pub(crate) const MSG_REUSED: &str = "request sequence reused with different input";
pub(crate) const MSG_FOCUS: &str = "focused window is not known";
pub(crate) const MSG_SEND_TARGET: &str = "send target domain is not observed";
pub(crate) const MSG_SEND_WINDOW: &str = "send window is not observed in the source domain";
pub(crate) const MSG_SEND_REFUSED: &str = "send refused: no automatic permission for this window";
pub(crate) const MSG_VIEW_OUTPUT: &str = "view output is not observed";
pub(crate) const MSG_VIEW_TARGET: &str = "view target is not a known workspace";
pub(crate) const MSG_IN_FLIGHT: &str = "a transfer is already outstanding for this window";
pub(crate) const MSG_DIRECTION: &str = "direction is invalid";
pub(crate) const MSG_SCOPE_HELD: &str = "command scope is held";
pub(crate) const MSG_REPLY_SIZE: &str = "reply exceeds size bound";
pub(crate) const MSG_REVISION_EXHAUSTED: &str =
    "global revision exhausted; rebootstrap under a new core generation";
pub(crate) const MSG_CREATION_EXHAUSTED: &str =
    "logical creation counter exhausted; rebootstrap under a new core generation";
pub(crate) const MSG_SELECT_UNKNOWN: &str = "selection target is not a known workspace";
pub(crate) const MSG_MIGRATION: &str = "workspace migration refused by scope policy";
pub(crate) const MSG_MIGRATION_TOPOLOGY: &str = "workspace migration has no adjacent target";
pub(crate) const MSG_MIGRATION_TOPOLOGY_UNREADABLE: &str =
    "workspace migration topology is unreadable";
pub(crate) const MSG_MIGRATION_SHARED: &str = "workspace migration refused in shared mode";
pub(crate) const MSG_MIGRATION_INVENTORY: &str = "workspace migration needs complete inventory";
pub(crate) const MSG_MIGRATION_SOURCE: &str = "workspace migration source is not homed";
pub(crate) const MSG_MIGRATION_SOURCE_UNREADABLE: &str = "workspace migration source is unreadable";
pub(crate) const MSG_MIGRATION_TARGET_INCOMPLETE: &str = "workspace migration target is incomplete";
pub(crate) const MSG_MIGRATION_FLIGHT: &str = "workspace migration already in flight for source";
pub(crate) const MSG_MIGRATION_RESIDUE: &str = "workspace migration target is occupied";
pub(crate) const MSG_MIGRATION_RECIPROCAL: &str = "workspace migration lacks reciprocal topology";
pub(crate) const MSG_MIGRATION_TARGET_GEOMETRY: &str =
    "workspace migration target geometry is invalid";
pub(crate) const MSG_OPERATION: &str = "operation route refused without mutation";
pub(crate) const MSG_TILING_TARGET: &str = "tiling target is not a known live workspace";

/// Inset one carried world domain by its outer gap (legacy planner-protocol
/// parity): the Engine retains the already-inset work area, never the raw
/// client area. `None` when the inset exhausts the bounds (geometric
/// exhaustion, held as constraint-clamp by the caller, never a silent
/// empty). Preserves id/workspace/gap/adjacency; only bounds shrink.
#[must_use]
pub(crate) fn inset_world_domain(domain: &OutputDomain, outer_gap: i32) -> Option<OutputDomain> {
    let inset: Rect = crate::geometry::inset_bounds(domain.bounds, outer_gap).ok()?;
    Some(OutputDomain {
        id: domain.id.clone(),
        workspace: domain.workspace.clone(),
        bounds: inset,
        gap: domain.gap,
        adjacent: domain.adjacent.clone(),
    })
}

/// Exact post-send source focus for one retained stay transfer (see
/// [`WorldEngine::stay_source_focus`]): a resident source window, a
/// proven-empty source, or an unknown/unprovable read that must refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaySourceFocus {
    Resident(WindowId),
    Empty,
    Unknown,
}

/// Neutral safety-exclusion intent ledger: lifetime fixed-size intent the
/// retained Engine sessions held when a safety/own-process exclusion
/// retired the slot. Never a public verdict, never membership, never an
/// effect: only a same-lifetime fallback for the stage-time classifier
/// so lifting the exclusion with current fixed hints restores the
/// approved tile override instead of re-admitting an automatic float.
/// Cleared on explicit native close (never inferred from safety), and a
/// fresh backend generation constructs a fresh ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ExcludedIntent {
    pub(crate) tile_override: bool,
    pub(crate) automatic_fixed: bool,
}

/// Retained transfer plus the source homing needed for shielding and
/// settlement. The reply projects only the [`TransferTarget`]. Policy
/// transfers (hotplug displacement/return member moves) share the exact
/// settlement/shielding path as command transfers; their target rows may
/// name live-but-not-yet-observed homings, which hold when unresolvable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedTransfer {
    pub(crate) target: TransferTarget,
    /// Explicit original command correlation for the v4 transfer lane
    /// (`TransferTargetV4.command_seq`): `Some` from command constructors
    /// (continuation children inherit the originating sequence even when a
    /// later report carries a different sequence), `None` on policy,
    /// commandless, and gesture work. Never inferred from operation-id
    /// spelling at projection time.
    pub(crate) command_seq: Option<u64>,
    /// True for hotplug policy member moves; false for command transfers.
    pub(crate) policy: bool,
    pub(crate) source_output: OutputId,
    pub(crate) source_workspace: String,
    /// Outstanding whole-workspace migration members: the relocated member
    /// set awaiting arrival at the target homing. Empty for every other
    /// transfer kind. Surviving members only: arrived plus
    /// still-outstanding. Explicitly closed and externally-moved members
    /// drop (never fought); absent without a close stays outstanding
    /// (never inferred closed).
    pub(crate) members: Vec<WindowId>,
    /// Expected source-refill logical view for a Migration, stored at commit
    /// so settlement survives early refill-child settlement. `None` when no
    /// refill was selected or for non-migration transfers.
    pub(crate) migration_refill: Option<String>,
    /// True when a Migration committed with zero members
    /// (originally empty). Distinguishes empty migrations (must still verify
    /// target view plus source refill) from migrations whose members all
    /// externally superseded/closed (settle to observed without fighting
    /// newer input).
    pub(crate) migration_initial_empty: bool,
    /// Source ring index of the migrated id at commit, for refusal
    /// rollback position restore (never wraps; clamped on reinsert).
    pub(crate) migration_source_index: Option<usize>,
    /// Core views at commit for refusal rollback reconciliation.
    pub(crate) migration_source_view: Option<String>,
    pub(crate) migration_target_view: Option<String>,
    /// Latched terminal migration outcome (proven refusal or external
    /// supersession) retained under the operation identity after its raw
    /// attempt row is acknowledged. Separate from the no-retry
    /// `TransferTarget.held` authority: `held` alone cannot distinguish a
    /// possibly-executed attempt from a proven terminal one. Only
    /// `RefusedProven`/`SupersededExternal` latch here; later readable
    /// evidence reconciles against the latched outcome. `None` until a
    /// terminal raw attempt arrives.
    pub(crate) migration_terminal: Option<crate::world_report::AttemptOutcome>,
    /// Issuing selection revision for a native-free migration binding
    /// (`pending.rev` at commit, `None` on the legacy host-owned route
    /// which mints no per-window selection targets). Lets terminal
    /// refusal rollback revoke ONLY the exact parent-issued selection
    /// binding/scopes/targets by revision, so a newer selection on the
    /// same outputs survives a late parent refusal. Survives binding
    /// completion (scopes/targets outlive the finite grant).
    pub(crate) migration_selection_rev: Option<u64>,
    /// Latest exact-correlated raw membership evidence staged BEFORE any
    /// readability hold (window transfer) or per-member map (migration
    /// parent), mirroring [`RetainedFloat::pending`]: a gapped report never
    /// erases knowledge the sequencer pending map forgets after
    /// acknowledgement. Reconciled against later readable complete
    /// observations; a spent operation is never re-dispatched.
    pub(crate) pending_member: Option<crate::world_report::MemberAttempt>,
    /// Per-member latest raw evidence for a migration parent, keyed by
    /// member window. Latest-wins per member; pruned with the member set.
    pub(crate) pending_members: BTreeMap<WindowId, crate::world_report::MemberAttempt>,
    /// Latest exact-correlated raw view evidence for the owning phase
    /// (standalone view id, follow parent, migration-target parent, refill
    /// child id), staged before any hold for the same gap/ack survival.
    pub(crate) pending_view: Option<crate::world_report::ViewAttempt>,
}

/// Retained explicit float-placement target: one outstanding authorized
/// frame per window, keyed by the stable command operation identity
/// (`op-<cmdSeq>`). Sourced from the observed frame/context at command
/// time plus the engine-committed effective rect. The window row carries
/// the operation id explicitly; the adapter writes geometry ONLY under
/// that token, exactly once. Settles on observation confirmation (observed
/// frame equals the target), proven refusal from operation-correlated raw
/// frame attempts, explicit close, a newer float intent for the same
/// window, or generation reset. An observed frame that matches neither the
/// source nor the target proves newer native input and supersedes without
/// a fight. No outcome verdict is ever consumed: the adapter reports
/// mechanical write facts only, and the core derives every verdict here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedFloat {
    pub(crate) operation_id: String,
    pub(crate) command_seq: u64,
    pub(crate) window: WindowId,
    pub(crate) target: crate::geometry::Rect,
    pub(crate) source: Option<crate::geometry::Rect>,
    pub(crate) output: OutputId,
    pub(crate) workspace: String,
    /// Latched ambiguity from a correlated raw attempt that neither
    /// confirms nor proves refusal (accepted clamp/mismatch, throw,
    /// unreadable proof, or DPI-invalidated write). Survives the attempt
    /// row being acknowledged so a third frame (matching neither source
    /// nor target) keeps holding instead of being misclassified as newer
    /// external input. Set together with [`RetainedFloat::pending`] while
    /// the observation is gapped, and re-evaluated against later readable
    /// evidence: a readable preimage matching retained refusal evidence
    /// still settles refusal, and the target still settles confirmation.
    /// Clears only on settlement (confirmation, explicit close, proven
    /// refusal, or a newer intent replacing it).
    pub(crate) ambiguous: bool,
    /// Latest exact-correlated raw frame attempt (operation id plus window
    /// plus target) staged BEFORE any readability or authority hold, so a
    /// gapped report never erases knowledge the shared consumed-frames
    /// watermark forgets after acknowledgement. Reconciled against later
    /// readable complete observations: confirmation and proven refusal
    /// still require truthful readable data, and no replay is ever issued
    /// (neither side re-dispatches a spent operation). Replaced by newer
    /// fresh evidence; dropped with the retained target on settlement.
    pub(crate) pending: Option<FrameAttempt>,
}

/// Retained directional maximize-clear: one outstanding authorized clear
/// per window, keyed by the stable originating command operation identity
/// (`op-<cmdSeq>`). Binds the original intent (window, direction, home
/// domain, displayed view, observed focus, original observation sequence)
/// plus whether the original subject routes tiled or float. The core
/// publishes `clear_operation` desire under that identity; the adapter
/// dispatches the native clear at most once and reports a correlated
/// [`ClearAttempt`](crate::world_report::ClearAttempt). A correlated
/// accepted attempt plus complete readable unmaximized facts proves the
/// clear; only then does the original intent resume (tiled-as-tiled or
/// float-as-float) under the ORIGINAL operation id, in the same reply's
/// settlement/sweep/publish ordering. No later user command consumes the
/// binding: any same-window new intent supersedes it and re-evaluates
/// fresh. Explicit close, complete departure, homing loss, or reset prunes
/// it. The desire withholds on held/unreadable/gesture scopes; after an
/// attempt the operation never re-actuates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedClear {
    pub(crate) operation_id: String,
    pub(crate) command_seq: u64,
    pub(crate) window: WindowId,
    pub(crate) direction: String,
    pub(crate) home: DomainKey,
    pub(crate) view: Option<String>,
    pub(crate) focus: String,
    /// True when the original subject routes float (floating or sticky);
    /// false for the tiled route.
    pub(crate) float_route: bool,
    /// Original same-axis mode for the tiled resume path.
    pub(crate) same_axis: crate::directional::SameAxisMove,
    /// Latest exact-correlated raw clear attempt staged BEFORE any
    /// readability or authority hold, mirroring [`RetainedFloat::pending`].
    pub(crate) pending: Option<crate::world_report::ClearAttempt>,
    /// Latched ambiguity from a correlated attempt that neither confirms
    /// nor proves refusal (accepted mismatch, throw, unreadable proof).
    /// Survives acknowledgement; re-evaluated against later readable
    /// evidence. Clears only on settlement or supersession.
    pub(crate) ambiguous: bool,
}

/// Retained commandless born maximize-clear: one outstanding authorized
/// clear per born window, keyed by a stable core-minted operation identity
/// (`op-born-<n>`), never a command sequence. The core publishes
/// `clear_operation` desire under that identity; the adapter dispatches
/// the native clear at most once and reports a correlated
/// [`ClearAttempt`](crate::world_report::ClearAttempt). A correlated
/// accepted attempt plus complete readable unmaximized facts proves the
/// clear; only then does the binding retire (no directional continuation
/// follows: the window already rides its admitted session row). Explicit
/// close, complete departure, homing loss or superseding float intent prunes
/// it. The desire withholds on safety/unreadable/gesture scopes, but may ride
/// a geometry-convergence hold under independent birth property authority.
/// After an attempt the operation never re-actuates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedBornClear {
    pub(crate) operation_id: String,
    pub(crate) window: WindowId,
    pub(crate) home: DomainKey,
    /// Latest exact-correlated raw clear attempt staged BEFORE any
    /// readability or authority hold, mirroring [`RetainedClear::pending`].
    pub(crate) pending: Option<crate::world_report::ClearAttempt>,
    /// Latched ambiguity from a correlated attempt that neither confirms
    /// nor proves refusal (accepted mismatch, throw, unreadable proof).
    /// Survives acknowledgement; re-evaluated against later readable
    /// evidence. Clears only on settlement.
    pub(crate) ambiguous: bool,
}
/// Retained host-owned native creation: one outstanding core-owned
/// authorization for the adapter to create exactly one native desktop for
/// one logical workspace, executed at most once per `operation_id`
/// (replay idempotent). The op id mints from the core serial
/// (`op-native-<n>`), never from a command sequence: trailing spare
/// creations are observation-caused and several outputs/workspaces can share
/// one command. When the adapter's fresh native order shows the created
/// desktop late, core binds it to the pending logical (never adopts it as an
/// outside creation); an ambiguous creation holds as
/// [`HoldReason::AmbiguousCreation`](crate::world_report::HoldReason) and
/// never blind-recreates.
/// Capability-named note: core owns binding and settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedNativeCreation {
    pub(crate) operation_id: String,
    pub(crate) logical_workspace: String,
    pub(crate) target_output: OutputId,
    /// Live native order observed when the authorization minted (strict
    /// attempt matching compares the attempt's `before` against this).
    pub(crate) order_before: Vec<String>,
    /// Report clock (`monotonic_ms`) when the authorization minted (`0`
    /// when unreported): attempts stamped before this never prove the
    /// creation (stale) and latch ambiguity instead.
    pub(crate) minted_ms: u64,
    /// Latest exact-correlated raw creation attempt (op plus logical plus
    /// output), staged for gap survival like the float/clear pending rows.
    pub(crate) pending: Option<crate::world_report::NativeCreationAttempt>,
    /// Latched ambiguity (refused, threw, mismatched, or colliding outside
    /// creation): publishes `AmbiguousCreation` and never mints a blind
    /// replacement.
    pub(crate) ambiguous: bool,
}

/// Bounded outcome of one [`WorldEngine::handle_report`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldOutcome {
    Accepted(WorldReply),
    /// Identical latest duplicate: cached reply, no new revision.
    Duplicate(WorldReply),
    /// Fresh report with an already-consumed command: observation
    /// reconciled, command never re-executed.
    CommandConsumed(WorldReply),
    Rejected {
        kind: &'static str,
        message: &'static str,
    },
}

/// One logical workspace projected into an Engine session: the stable core
/// id, its owning screen, and the per-report staged observation assigned by
/// [`WorldEngine::build_staged_domains`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StagedDomain {
    pub(crate) key: DomainKey,
    pub(crate) bounds: Rect,
    pub(crate) gap: i32,
    pub(crate) outer_gap: i32,
    /// Homed non-minimized facts with readable frames, as Engine carriers.
    pub(crate) windows: Vec<EngineWindow>,
    /// Adapter-observed focused lifetime homed here (empty when none):
    /// the staged focus proof for this domain.
    pub(crate) focused: WindowId,
    /// Homed minimized facts (kept out of the Engine carriers; the sweep
    /// retains their slots and publish emits explicit minimized rows).
    pub(crate) minimized: Vec<WindowId>,
    /// Homed facts with unreadable frames: the scope holds instead of
    /// authorizing geometry, never a silent absence.
    pub(crate) unreadable: Vec<WindowId>,
    /// Explicit closes homed here.
    pub(crate) closed: Vec<WindowId>,
    /// False when the owning screen is incomplete or carries an unreadable
    /// window fact: the scope becomes an explicit hold and its rows
    /// authorize nothing.
    pub(crate) readable: bool,
    pub(crate) hold: Option<HoldReason>,
}

/// Raw per-report observation capture: verbatim report lanes staged before
/// any classification. Both intakes populate the same boundary directly;
/// lane, command and lifetime consumers migrate to it in later increments.
/// No envelope admission, request/command correlation, or dispatch state
/// lives here: only the `observation_seq` watermark is retained for
/// observation ordering. Later helpers take explicit correlation arguments.
/// `raw_windows` keeps every typed row in source order, including
/// own-process and safety-excluded subjects; the classified `facts`/
/// `derived` carriers below stay the consume-lane view.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StagedObservation {
    pub(crate) inventory_complete: bool,
    pub(crate) observation_seq: u64,
    pub(crate) raw_windows: Vec<crate::world_report::WindowFact>,
    pub(crate) closed: Vec<WindowId>,
    pub(crate) native: Vec<crate::world_report::NativeObservation>,
    pub(crate) cursor: Option<(i32, i32)>,
    pub(crate) cursor_hit: Option<WindowId>,
    pub(crate) focused_global: String,
    pub(crate) action_foreground: Option<OutputId>,
    pub(crate) action_cached: Option<OutputId>,
    pub(crate) action_outputs: Vec<OutputId>,
    pub(crate) primary_output: Option<OutputId>,
    pub(crate) gesture: Option<crate::gesture::GestureIngress>,
    pub(crate) monotonic_ms: u64,
    pub(crate) frames: Vec<crate::world_report::FrameAttempt>,
    pub(crate) member_attempts: Vec<crate::world_report::MemberAttempt>,
    pub(crate) view_attempts: Vec<crate::world_report::ViewAttempt>,
    pub(crate) clear_attempts: Vec<crate::world_report::ClearAttempt>,
    pub(crate) creation_attempts: Vec<crate::world_report::NativeCreationAttempt>,
    pub(crate) state_attempts: Vec<crate::world_report::StateAttempt>,
    pub(crate) focus_feedback: Vec<crate::world_report::FocusFeedback>,
    pub(crate) intent_records: Vec<crate::world_report::IntentRecord>,
}

/// Per-report staged observation: screens as reported, one
/// [`StagedDomain`] per known logical workspace, adapter view echoes, and
/// the raw fact index the settlement path proves against.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct StagedReport {
    /// Captured raw observation for this report, staged before
    /// classification. See [`StagedObservation`].
    pub(crate) observation: StagedObservation,
    pub(crate) screens: Vec<crate::world_report::ScreenReport>,
    /// Identifiable bad screen rows held narrowly, in report order.
    pub(crate) held_screens: Vec<crate::world_report::HeldScreenRow>,
    /// Identifiable bad window rows held narrowly, in report order.
    pub(crate) held_windows: Vec<crate::world_report::HeldWindowRow>,
    /// Identifiable native rows held narrowly, in report order. The legacy
    /// intake never fills this lane (stays empty).
    pub(crate) held_native: Vec<crate::world_report::HeldNativeRow>,
    /// Identifiable attempt rows held narrowly, in report order. The
    /// legacy intake never fills this lane (stays empty).
    pub(crate) held_attempts: Vec<crate::world_report::HeldAttemptRow>,
    /// Staged v4 presentation outcome, verbatim from the report: the valid
    /// ingress or the held singleton row, never a defaulted ingress. `None`
    /// for the legacy intake (v3 carries its own presentation shape).
    pub(crate) presentation_v4: Option<crate::world_report::PresentationRowV4>,
    /// Staged v4 renderer input, verbatim from the report with no
    /// validation here. `None` for the legacy intake and for v4 reports
    /// without renderer input.
    pub(crate) renderer_input: Option<crate::world_report::RendererInput>,
    /// Safety/own-process exclusions observed on this report: known
    /// exclusions (staged facts classifying excluded) plus own-process
    /// windows (dropped before facts). Never members, slots, or control:
    /// converge removes them cleanly (never a minimize slot, never a
    /// native close), reconciles drop them complete and incomplete, and
    /// pending actions withhold writes/focus. Only an explicit native
    /// close resets lifetime identity.
    pub(crate) excluded: BTreeSet<WindowId>,
    /// Typed narrow-hold diagnostics emitted with this report's reply:
    /// one per held screen/window row plus one per use-time scale hold.
    pub(crate) diagnostics: Vec<crate::world_report::RowDiagnostic>,
    pub(crate) domains: Vec<StagedDomain>,
    /// Non-empty adapter view echoes by output.
    pub(crate) echoes: BTreeMap<String, String>,
    /// Raw observed screen per fact, before any logical assignment.
    /// `None` is unknown affinity, never a current-output guess.
    pub(crate) fact_output: BTreeMap<WindowId, Option<OutputId>>,
    /// Full raw facts by window id.
    pub(crate) facts: BTreeMap<WindowId, crate::world_report::WindowFact>,
    /// Core-derived per-window classification (see
    /// [`crate::window_classify`]): membership, hold, and exclusion
    /// carriers for the consume lanes. Raw facts stay the sole public
    /// input; these carriers never leave the core.
    pub(crate) derived: BTreeMap<WindowId, crate::window_classify::ClassifiedWindow>,
    /// Raw explicit closes.
    pub(crate) closes: BTreeSet<WindowId>,
    /// Effective classified window rows for this report: the
    /// capability-graded rows on the v4 intake, the verbatim rows on the
    /// legacy intake. Classification-only restages consume these, never
    /// the raw capture; the raw capture stays the ingestion boundary.
    pub(crate) effective_windows: Vec<crate::world_report::WindowFact>,
    /// Staged logical homing per window.
    pub(crate) homing: BTreeMap<WindowId, DomainKey>,
    /// Carried gesture barrier for the staged run.
    pub(crate) gesture_barrier: bool,
}

/// Explicit typed event identity for command-path event construction.
/// Threaded from the validated envelope instead of re-parsing a report.
#[derive(Debug, Clone)]
pub(crate) struct EventIssue {
    pub(crate) owner: OwnerId,
    pub(crate) correlation: CorrelationId,
}

/// Borrowed raw routing context for observation-dependent routing leaves.
/// Built from the source report for pre-capture freezes (prior retained
/// homes/views) or from staged state for post-capture commands (current
/// observations). No cloning, no ingestion.
pub(crate) struct RawRouting<'a> {
    pub(crate) inventory_complete: bool,
    pub(crate) cursor: Option<(i32, i32)>,
    pub(crate) focused_global: &'a str,
    pub(crate) action_foreground: Option<&'a OutputId>,
    pub(crate) action_cached: Option<&'a OutputId>,
    pub(crate) action_outputs: &'a [OutputId],
    pub(crate) primary_output: Option<&'a OutputId>,
    pub(crate) screens: &'a [crate::world_report::ScreenReport],
    pub(crate) held_screens: &'a [crate::world_report::HeldScreenRow],
}

impl<'a> RawRouting<'a> {
    pub(crate) fn from_source(report: &'a WorldReport) -> Self {
        Self {
            inventory_complete: report.inventory_complete,
            cursor: report.cursor,
            focused_global: report.focused_global.as_str(),
            action_foreground: report.action_foreground.as_ref(),
            action_cached: report.action_cached.as_ref(),
            action_outputs: report.action_outputs.as_slice(),
            primary_output: report.primary_output.as_ref(),
            screens: report.screens.as_slice(),
            held_screens: report.held_screens.as_slice(),
        }
    }

    pub(crate) fn from_staged(staged: &'a StagedReport) -> Self {
        Self {
            inventory_complete: staged.observation.inventory_complete,
            cursor: staged.observation.cursor,
            focused_global: staged.observation.focused_global.as_str(),
            action_foreground: staged.observation.action_foreground.as_ref(),
            action_cached: staged.observation.action_cached.as_ref(),
            action_outputs: staged.observation.action_outputs.as_slice(),
            primary_output: staged.observation.primary_output.as_ref(),
            screens: staged.screens.as_slice(),
            held_screens: staged.held_screens.as_slice(),
        }
    }

    /// Genuine v4 routing constructor: the same borrowed raw lanes read
    /// directly from the typed v4 report, never through a v4-to-legacy
    /// conversion. The caller passes the original v4 rows so the freeze
    /// below captures against prior retained state.
    // allow(dead_code): pending 7d accepted-run assembly owns this
    // constructor; tests drive it.
    #[allow(dead_code)]
    pub(crate) fn from_v4(report: &'a WorldReportV4) -> Self {
        Self {
            inventory_complete: report.inventory_complete,
            cursor: report.cursor,
            focused_global: report.focused_global.as_str(),
            action_foreground: report.action_foreground.as_ref(),
            action_cached: report.action_cached.as_ref(),
            action_outputs: report.action_outputs.as_slice(),
            primary_output: report.primary_output.as_ref(),
            screens: report.screens.as_slice(),
            held_screens: report.held_screens.as_slice(),
        }
    }
}

impl WorldEngine {
    /// Freeze the pre-observation command sources against prior retained
    /// state: the implicit send source (retained home else the action
    /// context) and the direction-only migration source (action output plus
    /// its retained view). Takes the command explicitly and the borrowed
    /// raw routing built from the source report; reads retained homing and
    /// views only. Never ingests intent, never stages observations, and
    /// never touches downstream staged gates: call before capture/ingest
    /// so first-tick admissions, staged rehoming, and same-report view
    /// settlement cannot retarget the current intent. Explicit send
    /// sources and report-only commands freeze nothing.
    pub(crate) fn freeze_command_sources(
        &self,
        command: Option<&WorldCommand>,
        routing: &RawRouting<'_>,
    ) -> (Option<OutputId>, Option<(OutputId, String)>) {
        let frozen_send_source = match command {
            Some(WorldCommand::SendRelative { window, source, .. })
            | Some(WorldCommand::SendNumbered { window, source, .. }) => match source {
                crate::world_report::SendSource::Explicit(_) => None,
                crate::world_report::SendSource::HomeThenAction => {
                    self.resolve_home_then_action(routing, window)
                }
            },
            _ => None,
        };
        let frozen_migration_source = match command {
            Some(WorldCommand::MigrateDirectional { .. }) => {
                let resolved = self.resolve_action_output(routing);
                match resolved {
                    Some(output) => self.logical_view(&output.0).map(|view| (output, view)),
                    None => None,
                }
            }
            _ => None,
        };
        (frozen_send_source, frozen_migration_source)
    }
}

/// Retained prior-accepted observation/publication facts for shared lanes.
///
/// Previous raw global focus and the actually published controlled-empty
/// workspace set. Written only at `finish` from the accepted candidate's
/// staged observation and published rows. The default supplies no prior
/// proof. Rejected candidates never reach the write, and the value clones
/// and resets with the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PriorAcceptedContext {
    pub(crate) prev_focused_global: String,
    pub(crate) empty_controlled: BTreeSet<DomainKey>,
}

impl PriorAcceptedContext {
    /// Derive the context from one accepted publication: the staged raw
    /// focus plus the workspace rows that published with a matching
    /// controlled output-or-workspace scope and no homed window row of
    /// any authority. Unknown rosters stay unproven by absence.
    pub(crate) fn capture(
        focused_global: &str,
        workspaces: &[WorkspaceDesire],
        scopes: &[ScopeStatus],
        windows: &[WindowDesire],
    ) -> Self {
        let mut empty_controlled = BTreeSet::new();
        for workspace in workspaces {
            let key = DomainKey {
                output: workspace.output.clone(),
                workspace: WorkspaceId(workspace.id.clone()),
            };
            let controlled = scopes.iter().any(|status| {
                status.authority == ScopeAuthority::Controlled
                    && match &status.scope {
                        AuthorityScope::Output(output) => output == &key.output,
                        AuthorityScope::Workspace { output, workspace } => {
                            output == &key.output && workspace.0 == key.workspace.0
                        }
                    }
            });
            if !controlled {
                continue;
            }
            if windows
                .iter()
                .any(|window| window.output == key.output && window.workspace == key.workspace.0)
            {
                continue;
            }
            empty_controlled.insert(key);
        }
        Self {
            prev_focused_global: focused_global.to_owned(),
            empty_controlled,
        }
    }
}

/// Stateful world boundary around the retained [`Engine`].
#[derive(Debug, Clone)]
pub struct WorldEngine {
    pub(crate) engine: Engine,
    pub(crate) core_generation: GenerationId,
    pub(crate) core_generation_str: String,
    generation_seed: String,
    connection_number: u64,
    config: WorldEngineConfig,
    adapter: Option<String>,
    pub(crate) connection_owner: Option<OwnerId>,
    application_identity: Option<ApplicationIdentity>,
    capabilities: Option<HostCapabilities>,
    pub(crate) settings: crate::settings_state::SettingsState,
    pub(crate) initial_settings_accepted: bool,
    pub(crate) ready_latched: bool,
    pub(crate) state_revision: u64,
    has_last: bool,
    last_request_seq: u64,
    last_digest: u64,
    /// Latest accepted typed report, bounded by the input caps in
    /// [`crate::world_report`]. Duplicate detection compares full typed
    /// input equality against this value; the digest is only a fast
    /// pre-check, never the identity (no hash-collision aliasing).
    pub(crate) last_report: Option<WorldReport>,
    last_observation_seq: u64,
    pub(crate) last_reply: Option<WorldReply>,
    /// Typed v4 replay bookkeeping, separate from the legacy
    /// `last_report`/`last_reply` cache above: the latest accepted v4
    /// report, its digest, and the cached v4 reply. The digest is only a
    /// fast pre-check; full typed input equality decides duplicates.
    /// Written only by the internal v4 admission boundary on accepted
    /// candidates; cleared with every fresh engine, cloned with every
    /// staged candidate, never touched by rejected candidates or the
    /// legacy path.
    pub(crate) has_last_v4: bool,
    pub(crate) last_request_seq_v4: u64,
    pub(crate) last_digest_v4: u64,
    pub(crate) last_observation_seq_v4: u64,
    pub(crate) last_report_v4: Option<WorldReportV4>,
    pub(crate) last_reply_v4: Option<WorldReplyV4>,
    /// Retained prior-accepted observation/publication facts for shared
    /// lanes: previous raw focus plus the actually published
    /// controlled-empty workspace set. Written at `finish` from the
    /// accepted candidate; cleared with every fresh engine, cloned with
    /// every staged candidate, never touched by rejected candidates.
    pub(crate) prior_context: PriorAcceptedContext,
    /// Monotonic command high-watermark per (incarnation, generation):
    /// any `command_seq` at or below it is already consumed, so eviction
    /// can never resurrect a sequence. Commandless reports never move it.
    pub(crate) command_highwater: Option<u64>,
    pub(crate) retained: Vec<RetainedTransfer>,
    /// Outstanding explicit float-placement targets, at most one per window.
    /// Bounded by the window inventory itself.
    pub(crate) retained_floats: Vec<RetainedFloat>,
    /// Outstanding internal native-state bindings (state lane, early
    /// increment): exact authorized targets plus retained raw evidence, spent
    /// latches, and internal dispositions. See [`crate::world_state`]. At
    /// most four per known lifetime (one per property), globally capped by
    /// [`crate::world_state::MAX_RETAINED_STATE_BINDINGS`]; never a row,
    /// slot, wire value, or membership proof.
    pub(crate) retained_states: BTreeMap<
        (WindowId, crate::world_report::StateProperty),
        crate::world_state::RetainedStateBinding,
    >,
    /// Outstanding directional maximize-clear authorizations, at most one
    /// per window. Bounded by the window inventory itself; each entry is
    /// commandless (no second command consumes it), superseded by any
    /// same-window new intent, pruned when its window closes, departs, or
    /// loses homing. See [`RetainedClear`].
    pub(crate) retained_clears: Vec<RetainedClear>,
    /// Outstanding commandless born maximize-clear authorizations, at most
    /// one per window. Bounded by the window inventory itself; each entry
    /// is settled commandlessly (no command consumes it), superseded by
    /// close/departure/homing loss, and never re-actuated after an
    /// attempt. See [`RetainedBornClear`].
    pub(crate) retained_born: Vec<RetainedBornClear>,
    /// Core-owned born-clear operation serial: mints `op-born-<n>`
    /// identities (never from a command sequence). Monotonic within the
    /// generation; never reset on adapter-incarnation rotation.
    pub(crate) born_seq: u64,
    /// Born windows whose one-shot clear already resolved (proven,
    /// refused, or abandoned): never re-mint for the same lifetime.
    /// Pruned on explicit close so id reuse starts a fresh lifetime.
    /// Retained across adapter-incarnation rotation with the engine
    /// sessions that prove admitted lifetimes.
    pub(crate) born_attempted: BTreeSet<WindowId>,
    /// First-seen fullscreen births awaiting their first exit, by lifetime.
    /// Later fullscreen on admitted tiles never enters. Pruned like
    /// `born_attempted`.
    pub(crate) born_held_fullscreen: BTreeSet<WindowId>,
    /// Floating-first sightings (R-MAX-03): lifetimes first observed
    /// floating on an untiled scope. They keep their overlay under both
    /// born modes, and the held exit lane never bypasses this safety.
    /// Pruned like `born_attempted`.
    pub(crate) born_floating_first: BTreeSet<WindowId>,
    /// Lifetimes observed non-fullscreen in this backend generation: a
    /// later fullscreen on one of these is a managed overlay transition,
    /// never a birth. Pruned like `born_attempted`.
    pub(crate) born_seen_nonfullscreen: BTreeSet<WindowId>,
    /// Lifetimes latched keep-overlay at tiled-first evaluation
    /// ([`born_keep_latch`](crate::window_policy::born_keep_latch)): a later
    /// flip to tile-on-launch never reclears them. Pruned like
    /// `born_attempted`.
    pub(crate) born_keep_overlay: BTreeSet<WindowId>,
    /// Lifetime-local core-composed maximize bits (`caps.maximize == false`
    /// only): windows whose published frame is the output work area under
    /// the existing `Controlled` frame lane. The tile slot/allocation or
    /// float base placement stays retained in the session; clear restores
    /// it. No native `Maximized` target is emitted, no intent record is
    /// written, and restart drops the bit with the engine. Pruned with the
    /// live inventory in [`WorldEngine::finish`].
    pub(crate) composed_maximized: BTreeSet<WindowId>,
    /// Core-owned frame anti-fighting memory (stage 3a): per-lifetime clamp
    /// suppression plus transient retry backoff over mechanical
    /// [`FrameAttempt`](crate::world_report::FrameAttempt) facts. Bounded by
    /// the window inventory itself; pruned every accepted report.
    pub(crate) frames: BTreeMap<WindowId, crate::world_frames::FrameMemory>,
    /// Last consumed raw frame attempt per lifetime, retained independently
    /// of [`Self::frames`] so an exact-match clear never forgets what was
    /// already consumed: a lost-ack redelivery of an older or identical
    /// attempt is ignored instead of resurrecting stale suppression or
    /// clobbering newer authority. Same lifetime bound and pruning as
    /// [`Self::frames`]; cleared on incarnation reset, fresh on a new
    /// core generation with the engine itself.
    pub(crate) consumed_frames: BTreeMap<WindowId, crate::world_report::FrameAttempt>,
    /// Tiled burst budget (anti-loop): consumed mechanical attempts per
    /// lifetime and unchanged desired target inside a continuous observed
    /// frame-change burst. Same lifetime bound and pruning as
    /// [`Self::frames`]; cleared on incarnation reset, fresh on a new core
    /// generation with the engine itself.
    pub(crate) frame_bursts: BTreeMap<WindowId, crate::world_frames::FrameBurst>,
    /// Conservative learned minimum sizes per exact lifetime (stage 3b):
    /// proven app-held growth merged into the normalized hint path before
    /// layout projection. Separate from transient pair suppression: exact
    /// clears never erase these, and unrelated changes never shrink them.
    /// Pruned with the frame maps.
    pub(crate) learned_mins: BTreeMap<WindowId, crate::world_frames::LearnedMin>,
    pub(crate) creation_counter: u64,
    /// Core-owned native creation serial: mints `op-native-<n>` creation
    /// operation ids (never from a command sequence). Monotonic within the
    /// generation: a fresh backend incarnation (new [`WorldEngine`]) restarts
    /// it, while an adapter-incarnation reset only clears the retained
    /// creations and never reuses an id.
    pub(crate) native_creation_seq: u64,
    /// Outstanding core-owned native creation authorizations (capability
    /// only). At most one per unbound logical workspace; stable op id until
    /// the fresh native readback binds it or ambiguity latches. Reset on
    /// generation/incarnation. Capability-named note: explicit desire only.
    pub(crate) retained_creations: Vec<RetainedNativeCreation>,
    /// Deferred option-1 source releases: `(output, workspace)` pairs whose
    /// native backing must still release once a complete list allows it
    /// and no creation pends on the output. Pushed when arrival-gated
    /// release defers; drained in native intake (converge) so cleanup is
    /// never silently settled away with a retired flight. Cleared on
    /// generation/incarnation reset like the other native state.
    pub(crate) pending_source_releases: Vec<(String, String)>,
    pub(crate) last_views: BTreeMap<OutputId, String>,
    /// Core-owned logical workspace rings (D2): per-screen ordered lists of
    /// stable logical workspace ids, retained across reports. Rings converge
    /// every accepted report through lifecycle (trailing creation/removal),
    /// hotplug displacement, and commands (migration moves the logical id
    /// between screens). Native observations never move ownership.
    pub(crate) mapping: BTreeMap<String, Vec<String>>,
    /// Host-owned native binding (capability-gated adoption): maps
    /// `(output, native desktop id)` to the stable core logical workspace
    /// id adopted for it. Built once per output from the first complete
    /// `native_order` (native order preserved), extended when an outside
    /// host creates a native desktop (inserted at native-order position).
    /// A complete snapshot (`inventory_complete`, complete screens, and a
    /// present native row for the output) prunes vanished ids: removal, not
    /// rename. Partial or absent rows never delete mappings. Reset on
    /// generation/incarnation. Empty in the native-free default.
    pub(crate) native_map: BTreeMap<(String, String), String>,
    /// Core-owned current view per screen: the stable logical workspace id
    /// shown, retained across reports. Commands and policy moves update it;
    /// adapter echoes prove it.
    pub(crate) views: BTreeMap<String, String>,
    /// Planner-owned previous-view history (D2): per-output entries
    /// observed from verified views only.
    pub(crate) history: ScopeHistory,
    /// Planner-owned hotplug displacement: removed screens mapped to their
    /// preserved logical workspace units plus the hosting survivor.
    pub(crate) displaced: BTreeMap<String, crate::workspace_scope::DisplacedEntry>,
    /// Last observed screen facts by stable output key (FULL rectangle,
    /// work area, plus active-screen primary). Updated for every live
    /// screen on every accepted report; removed screens keep their
    /// disconnect-time facts while a displacement record needs placement.
    /// Never a window frame proxy.
    pub(crate) output_facts: BTreeMap<String, crate::workspace_scope::OutputFacts>,
    /// Core-owned live output topology: validated FULL rectangles by stable
    /// output id, retained from the last accepted report claiming complete
    /// screens. Directional world commands resolve against this map through
    /// the shared strict unique-reciprocal helper, never any carried
    /// adjacency. A report with no screens, any incomplete screen, or a
    /// missing/invalid/duplicate FULL rect clears the map, so
    /// topology-dependent crossing, migration, and remap fence narrowly
    /// until the next complete report instead of actuating on stale edges.
    /// Unrelated window-inventory incompleteness never clears this map.
    /// Disconnect-time facts for nearest-survivor placement stay in
    /// [`WorldEngine::output_facts`].
    pub(crate) live_topology: BTreeMap<String, Rect>,
    /// Planner-owned per-workspace tiling mode (legacy Meta+Y parity):
    /// absent means [`TilingMode::Tiled`]. Set through
    /// [`crate::world_report::WorldCommand::SetWorkspaceTiling`], flipped
    /// at execution time through
    /// [`crate::world_report::WorldCommand::ToggleWorkspaceTiling`], or
    /// seeded from the reported saved default; the adapter never decides.
    /// Floating workspaces skip convergence and publish explicit untouched
    /// rows (no frame writes while floating).
    pub(crate) tiling: BTreeMap<DomainKey, TilingMode>,
    /// Core-owned logical membership (D2): stable window-lifetime homing
    /// independent of layout sessions. The single homing owner: floating
    /// release, minimized/unreadable holds, and view switches never move
    /// members except through explicit commits, truthful receipts, or
    /// observed cross-output moves. See [`crate::world_members`].
    pub(crate) members: BTreeMap<WindowId, crate::world_members::MemberState>,
    /// Floating scopes for the report currently being staged: derived from
    /// [`WorldEngine::tiling`] at the start of every accepted run, read by
    /// the command gates and publishing.
    pub(crate) floating_scopes: BTreeSet<DomainKey>,
    /// Per-scope sweep failure classification for the report currently
    /// being staged: [`WorldEngine::classify_sweep_hold`] reasons for scope
    /// keys in the staged held set. Cleared at the start of every accepted
    /// run and published by [`WorldEngine::publish`]; never survives across
    /// reports, incarnations, or generations.
    pub(crate) hold_reasons: BTreeMap<DomainKey, HoldReason>,
    /// Retained gesture session (stage 4a): one open native move/size
    /// gesture bound to an exact window lifetime plus its stable adapter
    /// gesture id. Updated per accepted gesture report; cleared on
    /// release, cancel, Esc, close, or incarnation/generation reset.
    /// Never a command and never consumed.
    pub(crate) gesture: Option<RetainedGesture>,
    /// Consumed gesture id high watermark (stage 4a at-most-once): every
    /// incorporated terminal advances this to at least its id. Binds and
    /// terminal replays order against this single scalar, never a map:
    /// ids at or below it are consumed replays, ids above it are new.
    /// Superseded only by incarnation reset, never cleared otherwise (not
    /// on Updated/Started replays).
    pub(crate) consumed_gesture_seq: u64,
    /// Precise result of the latest incorporated gesture terminal, staged
    /// for the reply being published ([`WorldEngine::publish`] takes it
    /// into [`WorldReply::gesture_settled`]). Single slot: each terminal
    /// overwrites; each publish takes. Never drained by the adapter; the
    /// reply carries the per-report result and the next unrelated report
    /// publishes `None`.
    pub(crate) gesture_terminal: Option<GestureSettled>,
    /// Fresh-placement bootstrap phase: false until the first accepted
    /// complete inventory establishes the startup lifetime set (including
    /// an empty roster). Once true, genuinely new lifetimes observed
    /// under complete inventory route by cursor/focus policy, even after
    /// an empty seed, close-all, or an incomplete gap. Incomplete reports
    /// never set it, so unseen startup stays quiet/native until the first
    /// complete baseline. Adapter rebind clears it for a new adoption
    /// baseline; retained engine sessions still shield known lifetimes.
    pub(crate) placement_bootstrapped: bool,
    /// Minimize slots to forget after the sweep (proven-refusal cleanup:
    /// converge may re-store the slot during the unshielded sweep, so the
    /// forget runs after, never before).
    pub(crate) pending_forget: Vec<(DomainKey, WindowId)>,
    /// Correlated per-command outcome staged by the command path below and
    /// consumed once by [`WorldEngine::publish`] for the reply being built.
    /// `None` for observation-only replies and for operations whose result
    /// the snapshot already carries (move/resize/tiled-admit/focus/send).
    /// Cleared at the start of every staged run so a previous command's
    /// result never leaks into an unrelated reply.
    pub(crate) command_result: Option<CommandResult>,
    /// Terminal retained-operation lifecycle events staged by settlement
    /// ([`WorldEngine::settle_clears`], [`WorldEngine::settle_floats`]) and
    /// consumed once by [`WorldEngine::publish`] for the reply being built.
    /// Empty on holds and on replies with no operation settlement. Cleared
    /// at the start of every staged run so a previous report's events never
    /// leak.
    pub(crate) operation_settled: Vec<crate::world_report::OperationEvent>,
    /// Terminal selection events staged by switch settlement
    /// and consumed once by [`WorldEngine::publish`] for the reply being
    /// built. Empty on holds and on reports with no switch settlement.
    /// Cleared at the start of every staged run so a previous report's
    /// events never leak.
    pub(crate) switch_settled: Vec<crate::world_report::SwitchEvent>,
    /// Fresh-admission focus intents already emitted, by window id (D4):
    /// at-most-once per lifetime, pruned on close so a reused id focuses
    /// again as a fresh newcomer. Survives incarnation resets within the
    /// generation (retained sessions persist there too); a new generation
    /// constructs a new [`WorldEngine`].
    pub(crate) admission_focus_spent: BTreeSet<String>,
    /// Compact automatic-focus identity tokens by live lifetime: the first
    /// automatic-focus candidate's issuing revision for that lifetime
    /// (`self.state_revision + 1` at first allocation), not necessarily
    /// the first emitted effect revision. Admission (`af-`) and
    /// hover (`ffc-`) effects spell `prefix-token` from this map, so ids stay
    /// within the opaque bound for maximum-length subjects. The map entry is
    /// reused across hover transitions and observations, so a repeated hover
    /// for the same lifetime retains its identical effect id (the adapter's
    /// at-most-once spent fence stays authoritative). Explicit `fx-`
    /// identities are disjoint. Entries are retained through
    /// holds/exclusions/incomplete gaps and engine/member-known absence, and
    /// pruned only on explicit close or proven lifetime departure like
    /// [`Self::retained_focus`]. Never a row, slot, or wire value.
    pub(crate) automatic_focus_tokens: BTreeMap<WindowId, u64>,
    /// Retained ordinary focus settlement for the latest emitted focus
    /// batch (see [`crate::world_focus`]): at most two grants (admission
    /// plus explicit command), each with its emitted effect, first
    /// exact-tuple correlated feedback row, and historical verdict. Pruned
    /// on explicit close and proven complete departure like the other
    /// lifetime ledgers; reset on handshake by fresh construction.
    /// Never a row, slot, or wire value.
    pub(crate) retained_focus: Vec<crate::world_focus::RetainedFocusGrant>,
    /// Pending native-free selection grant between the real selection
    /// command and publish only (see [`crate::world_selection`]): the
    /// dispatching command plus the selected/departing homes, issuing
    /// revision, and observation watermark. Never a row, slot, or wire
    /// value.
    pub(crate) pending_selection: Option<crate::world_selection::PendingSelection>,
    /// Pending commandless policy selections, one per policy view parent
    /// (see [`WorldEngine::retain_policy_view`]): same carrier as the
    /// command pending above, but each keeps its own `op-dpv-*` parent
    /// identity instead of merging. Latest per output wins. Never a row,
    /// slot, or wire value.
    pub(crate) pending_policy_selections: Vec<crate::world_selection::PendingSelection>,
    /// Retained per-window native-free selection visibility authorizations
    /// by live lifetime (see [`crate::world_selection`]): the logical home
    /// at grant, desired shown state, and issuing revision. Never a row,
    /// slot, or wire value.
    pub(crate) selection_targets: BTreeMap<WindowId, crate::world_selection::SelectionTarget>,
    /// Retained `awaitShown` selection bindings, latest per focus output
    /// (see [`crate::world_selection`]): the emitted effect, frozen arrival
    /// snapshot, first correlated feedback, and distinct foreground/arrival
    /// facts. Never a row, slot, or wire value.
    pub(crate) retained_selection: BTreeMap<OutputId, crate::world_selection::RetainedSelection>,
    /// Retained active native-free selection scopes, latest per affected
    /// output (see [`crate::world_selection`]): the selected/departing
    /// homes plus the issuing revision. Survives the finite grant; cleared
    /// only by superseding selections, the central view reconcile, or true
    /// output loss. Never a row, slot, or wire value.
    pub(crate) selection_scopes: BTreeMap<OutputId, crate::world_selection::SelectionScope>,
    /// Cursor-first behavior settings retained from the latest accepted
    /// report (2026-10-20): switch target, fresh placement, and the
    /// stage-4b focus-follows-cursor/cursor-warp flags. Refreshed per
    /// accepted run alongside the ingress capacity settings below; never
    /// move existing members or reflow retained state on change.
    pub(crate) switch_target: crate::cursor_targeting::SwitchTarget,
    pub(crate) fresh_placement: crate::cursor_targeting::FreshPlacement,
    pub(crate) focus_follows_cursor: bool,
    pub(crate) cursor_warp: bool,
    /// Normalized cursor retained from the latest accepted report, `None`
    /// when the adapter could not read it. Seated with its observation
    /// sequence and refreshed per accepted run, never on a separate stale
    /// timer; an absent cursor falls back through focused output to the
    /// primary/first readable output.
    pub(crate) last_cursor: Option<(i32, i32)>,
    /// Cursor of the previously accepted report (the movement baseline for
    /// focus-follows-cursor: a static cursor never re-steals focus).
    /// Updated alongside [`WorldEngine::last_cursor`], cleared on
    /// adapter incarnation reset.
    pub(crate) prev_cursor: Option<(i32, i32)>,
    /// Last hover target the focus-follows-cursor lane observed, minted or
    /// not: a static cursor over an unchanged hover never re-mints after a
    /// keyboard command moves focus elsewhere. Cleared on adapter
    /// incarnation reset.
    pub(crate) ffc_last_hover: Option<WindowId>,
    /// Retained last-known focused output for fresh-window homing (i3/sway
    /// focused-output placement): genuine managed focus only, cleared on
    /// adapter incarnation reset, filtered against live topology at use.
    /// Never moves existing members or reflows retained state.
    pub(crate) fresh_focus: crate::fresh_focus::FreshFocusMemory,
    /// Staged logical observation for the report currently being staged:
    /// screens, one domain per known logical workspace, view echoes, raw
    /// facts, homing, and the carried gesture barrier. Built once per
    /// accepted run and read by settling, sweeping, commands, and
    /// publishing; never survives across runs.
    pub(crate) staged: StagedReport,
    /// Neutral retained fixed-size intent for safety-excluded lifetimes
    /// (see [`ExcludedIntent`]). Keyed by stable lifetime; never a row,
    /// never a slot, never a public wire value.
    pub(crate) excluded_intent: BTreeMap<WindowId, ExcludedIntent>,
    /// Pending opaque intent restores not yet admitted (first-admission
    /// import only). Usable records only: non-sticky `float` `Some` rows
    /// plus whole sticky-bearing records (`stickyPrior` `Some` with any
    /// `float` lane, including `None`); invalid, null/null, conflicting,
    /// and closed subjects never store. Jointly bounded with
    /// [`Self::intent_conflicts`] by
    /// [`crate::world_report::MAX_INTENT_RECORDS`].
    pub(crate) pending_intent: BTreeMap<WindowId, crate::intent_data::IntentData>,
    /// Windows with conflicting duplicate intent rows (same report distinct
    /// raw strings, or cross-report differing values before admission).
    /// Conflicting subjects confer no import authority. Jointly bounded
    /// with [`Self::pending_intent`].
    pub(crate) intent_conflicts: BTreeSet<WindowId>,
    /// Lifetimes whose first successful admission already happened. Late
    /// intent rows for these lifetimes are ignored forever (even through
    /// temporary exclusion, holds, or minimize), so newer runtime intent
    /// is never overwritten. Pruned on explicit close and proven complete
    /// departure; reset on handshake.
    pub(crate) intent_admitted: BTreeSet<WindowId>,
    /// Durable explicit-float provenance for restored intentional floats
    /// (`float:true` wins) plus successful runtime float commands. Feeds
    /// the stage-time classifier so a restored float survives later
    /// ordinary facts and safety exclusion/recovery with no geometry,
    /// focus, or clear ops. Only explicit wins enter: automatic and
    /// capacity floats never do. Cleared when the lifetime tiles again
    /// (runtime unfloat wins), on explicit close, and on proven complete
    /// departure; reset on handshake. Never a row, slot, or wire value.
    pub(crate) restored_floats: BTreeSet<WindowId>,
    /// Lifetime-scoped sticky-prior provenance for restored semantic
    /// sticky intent: the exact `stickyPrior` bool committed only on a real
    /// healthy float success (non-automatic exception, including healthy
    /// floating-workspace newcomers). Feeds the stage-time classifier so a
    /// restored sticky float survives later ordinary facts and safety
    /// exclusion/recovery with no geometry, focus, or clear ops, and never
    /// converts into ordinary [`Self::restored_floats`] or a tile win.
    /// Survives record-free reports, holds, minimize, exclusion, observed
    /// native `false`, and handoff across workspace floating/tiling;
    /// pruned only on explicit close, proven own-process, and proven
    /// complete departure; reset on handshake (fresh construction). Never
    /// a row, slot, wire value, or native-fact completion. Jointly bounded
    /// with [`Self::pending_intent`]/[`Self::intent_conflicts`] (a
    /// pending-to-retained commit holds the joint count steady).
    pub(crate) restored_sticky: BTreeMap<WindowId, bool>,
    /// Lifetime-scoped exact saved-record knowledge for sticky rollback:
    /// the last published or first-imported [`IntentData`] per window,
    /// including nullable imported shapes (never synthesized from the
    /// current float bool). Established by first accepted imports and by
    /// prior emitted ToggleFloat/sticky saves; late ignored imports never
    /// clobber newer runtime state. Pruned on own-process proof, explicit
    /// close, proven complete departure, and handshake reset like the
    /// other intent ledgers. Bounded by [`MAX_INTENT_RECORDS`].
    pub(crate) sticky_saved: BTreeMap<WindowId, crate::intent_data::IntentData>,
    /// Generation-scoped fail-closed latch for exact saved-record knowledge:
    /// set when a first-admission exact prior is lost to a bound
    /// (`sticky_saved` or joint-ledger cap) or to conflicting imports, and
    /// reset on handshake (fresh construction). While set, native commands
    /// refuse as unknown even if the maps later shrink, because a pruned
    /// length can no longer prove the prior was `None`. Late ignored
    /// imports (already admitted) and malformed records never set it;
    /// ordinary/composed save emission is unchanged, only native prior
    /// reads fail closed.
    pub(crate) sticky_saved_incomplete: bool,
    /// Stable composed-visibility authorization revision per sticky window
    /// (issuing reply revision, retained across observation-only replies).
    /// Plain observation never renews it; a fresh explicit command
    /// reauthorizes. Pruned with [`Self::sticky_saved`].
    pub(crate) sticky_visible_rev: BTreeMap<WindowId, u64>,
    /// Staged sticky command save for the reply being built (execute
    /// stages, publish drains next unit). At most one per command: only
    /// the successful accepted transition stages; same-state consumes
    /// stage nothing; envelope rejections never reach staging.
    pub(crate) sticky_command_save: Option<crate::world_report::IntentWrite>,
    /// Staged proven-refusal rollback writes for the reply being built
    /// (followup stages pre-sweep, publish drains). Latest-wins per window:
    /// at most one write per lifetime, bounded by the live inventory. The
    /// write restores the exact pre-command saved record (or null-clears
    /// when there was none). Drained on commandless, consumed, and
    /// saturated replies alike; a same-window fresh command save
    /// supersedes at merge. Envelope rejections drop the whole staged
    /// candidate, so nothing leaks. Budget prebound in the reply-size
    /// reservation via [`crate::world_sizing::max_intent_rollback_bound`].
    pub(crate) sticky_rollback_saves: BTreeMap<WindowId, crate::world_report::IntentWrite>,
    /// Lifetime-scoped save blocks for deferred sticky intent: lifetimes
    /// with a pre-admission `stickyPrior` non-null record. A blocked
    /// lifetime publishes no intent save, even on a later successful
    /// runtime ToggleFloat, because sticky semantics are not implemented
    /// yet and a null-sticky save would erase the deferred record. The
    /// set carries no sticky contents (none are chosen); it confers no
    /// admission or native authority. Survives admission (unlike
    /// conflicts), holds, exclusion, minimize, and record-free reports;
    /// dropped on explicit close, proven own-process, and proven complete
    /// departure; reset on handshake. Jointly bounded with
    /// [`Self::pending_intent`]/[`Self::intent_conflicts`].
    pub(crate) intent_sticky_blocks: BTreeSet<WindowId>,
    /// Generation-scoped saturation latch: set when a sticky block cannot
    /// be retained at the joint cap, and suppresses every intent save
    /// until handshake reset. Fail-closed by construction: a dropped
    /// block would allow a save to erase deferred sticky intent.
    pub(crate) intent_sticky_saturated: bool,
    /// Retained pager presentation live frames (stage 5): fresh native
    /// frames per roster lifetime, rebuilt wholesale by every
    /// [`WorldEngine::retain_pager_presentation`] call from the normalized
    /// ingress. First in the pager frame fallback after membership.
    /// Presentation-only; never mapping, membership, or geometry authority.
    /// Members with no resolved source render no miniature, never a guess.
    pub(crate) presentation_live: BTreeMap<WindowId, Rect>,
    /// Retained pager presentation float snapshots (stage 5): saved float
    /// frames per roster lifetime, second in the pager frame fallback.
    /// Presentation-only like above.
    pub(crate) presentation_float: BTreeMap<WindowId, Rect>,
    /// Retained pager presentation tile/hide-time snapshots (stage 5):
    /// saved tile slot and hide-time frames per roster lifetime, last in
    /// the pager frame fallback. Presentation-only like above.
    pub(crate) presentation_tile: BTreeMap<WindowId, Rect>,
    /// Retained pager presentation order (stage 5): top-to-bottom native
    /// stacking over admitted member lifetimes plus first-paint candidates
    /// (see [`WorldEngine::retain_pager_presentation`]). Sole retained
    /// stacking authority; per-workspace content lists paint bottom first.
    pub(crate) presentation_order: Vec<WindowId>,
    /// Pager presentation focus (stage 5): the adapter-observed focused
    /// lifetime from the latest accepted report (`None` when none or
    /// unmanaged). Presentation-only; the content gates it on retained homing.
    pub(crate) presentation_focused: Option<WindowId>,
    /// Retained v4 pager presentation order: top-to-bottom native stacking
    /// over admitted member lifetimes plus first-paint candidates, from the
    /// v4 `pagerOrder`/`pagerFrames` rows only. Never shares the v3 order
    /// maps; the v3 path stays untouched until cutover.
    pub(crate) presentation_order_v4: Vec<WindowId>,
    /// Retained v4 pager presentation live frames: last-known fresh frame
    /// per roster lifetime, accumulated by every
    /// [`WorldEngine::retain_pager_presentation_v4`] call. v4 keeps
    /// live sources only: no float/tile fallback, never a guess.
    pub(crate) presentation_live_v4: BTreeMap<WindowId, Rect>,
    /// v4 pager presentation focus: the adapter-observed focused lifetime
    /// from the latest staged v4 report.
    pub(crate) presentation_focused_v4: Option<WindowId>,
    /// Latest published v4 presentation revision, stamped by `publish_v4`
    /// from its supplied issuing revision. Read by 7d renderer-input
    /// validation BEFORE calling publication (the staged producer revision
    /// stays a placeholder and is never read there); the stored value only
    /// becomes authoritative when 7e adopts the publication candidate.
    pub(crate) latest_published_presentation_rev_v4: u64,
    /// Test-only count of [`Self::ingest_staged_intent`] executions on this
    /// engine, including ingests whose rows the admitted latch then ignores.
    /// Discriminates exactly-once ingestion from latch-ignored late imports
    /// without global instrumentation.
    #[cfg(test)]
    pub(crate) ingest_calls: u64,
    /// Test-only clone-shared count of [`Self::ingest_staged_intent`]
    /// executions across every trial clone, including staged candidates
    /// that are later discarded. Clones share the same counter, so a
    /// discarded trial stays observable on the parent after the clone
    /// drops; [`Self::ingest_calls`] only counts adoptions on `self`.
    /// Never read outside tests; production behavior is unchanged.
    #[cfg(test)]
    pub(crate) ingest_trials: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Canonical last-active identity per workspace (R-WS-09 memory):
    /// one slot per effective domain across sticky/link migration.
    /// Written by accepted command choices before native writes and by
    /// changed native focus observations under the same fail-closed gates;
    /// unchanged re-reports never overwrite. Read by view return before
    /// the tiled MRU fallback. Pruned to the live inventory on finish.
    /// Equal precedence for tile/float/sticky: most recent wins.
    pub(crate) last_active_window: BTreeMap<DomainKey, WindowId>,
    /// Opt-in neutral focused-fullscreen presentation suspension (stage 5):
    /// when set, a focused surface that is truthfully fullscreen on its
    /// observed output suspends published border/underlay/pager shapes on
    /// that output only. KWin only offers truthful `fullScreen`, never game
    /// identity, so this names the observation (focused fullscreen), never a
    /// platform or product. Off by default; the Linux service carrier opts
    /// in explicitly. Never mapping, membership, or command semantics.
    pub(crate) focused_fullscreen_presentation_suspension: bool,
}

/// Platform opt-ins for [`WorldEngine`] construction: the only Engine
/// policy carried at world construction time. `fixed_size_admission` is
/// always on and is not configurable here; nothing else may be injected.
/// Presentation-only carrier opt-ins ride explicit [`WorldEngine`] builder
/// methods instead, never this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldEngineConfig {
    /// Deferred sends for automatic fixed-size floats (Windows workspace
    /// sends). Off by default; the Windows carrier enables it.
    pub fixed_send_admission: bool,
    /// G-06 maximized directional focus fence (REQ-MAX-08). On by default
    /// (Linux route); the Windows carrier disables it.
    pub maximized_focus_fence: bool,
}

impl Default for WorldEngineConfig {
    /// Exactly the historical [`WorldEngine::new`] semantics: automatic
    /// fixed-float sends off, maximized focus fence on.
    fn default() -> Self {
        Self {
            fixed_send_admission: false,
            maximized_focus_fence: true,
        }
    }
}

impl WorldEngine {
    #[doc(hidden)]
    pub fn new_test_bound(generation_seed: &str, adapter: &str) -> Option<Self> {
        Self::new_test_bound_with_capabilities(
            generation_seed,
            adapter,
            HostCapabilities {
                // Synthetic fixtures carry native sticky and maximize
                // facts; opt into the native routes explicitly so the
                // intended lanes stay valid.
                native_sticky: true,
                maximize: true,
                ..HostCapabilities::default()
            },
        )
    }

    #[doc(hidden)]
    pub fn new_test_bound_with_capabilities(
        generation_seed: &str,
        adapter: &str,
        capabilities: HostCapabilities,
    ) -> Option<Self> {
        Self::new_test_bound_with_initial_settings(
            generation_seed,
            adapter,
            capabilities,
            Vec::new(),
        )
    }

    #[doc(hidden)]
    pub fn new_test_bound_with_initial_settings(
        generation_seed: &str,
        adapter: &str,
        capabilities: HostCapabilities,
        values: Vec<(String, crate::connection::RawSettingValue)>,
    ) -> Option<Self> {
        Self::new_test_bound_with_config_and_initial_settings(
            generation_seed,
            adapter,
            capabilities,
            WorldEngineConfig::default(),
            values,
        )
    }

    #[doc(hidden)]
    pub fn new_test_bound_with_config_and_initial_settings(
        generation_seed: &str,
        adapter: &str,
        capabilities: HostCapabilities,
        config: WorldEngineConfig,
        mut values: Vec<(String, crate::connection::RawSettingValue)>,
    ) -> Option<Self> {
        let mut engine = Self::new_with_config(generation_seed, config)?;
        // The caller owns availability: the synthetic test identity carries
        // exactly the facts the given capabilities support, and the
        // capabilities pass through unmodified (never silently extended).
        let unavailable = &capabilities.unavailable_facts;
        let handshake = HandshakeRequest {
            adapter: adapter.to_owned(),
            request: "test-handshake".to_owned(),
            application_identity: ApplicationIdentity {
                app_id: None,
                executable: if unavailable.contains(&crate::connection::FactName::Executable) {
                    None
                } else {
                    Some("test-own-executable".to_owned())
                },
                process_id: if unavailable.contains(&crate::connection::FactName::ProcessId) {
                    None
                } else {
                    Some("test-own-pid".to_owned())
                },
            },
            capabilities,
        };
        let handshake_reply = engine.handshake(&handshake);
        if handshake_reply.kind.is_some() {
            return None;
        }
        if !values.iter().any(|(key, _)| key == "mode") {
            values.push((
                "mode".to_owned(),
                crate::connection::RawSettingValue::String(
                    crate::workspace_scope::WORKSPACE_MODE_TOKENS[0].to_owned(),
                ),
            ));
        }
        let settings = SettingsRequest {
            adapter: handshake_reply.adapter,
            generation: handshake_reply.generation?,
            request: "test-settings".to_owned(),
            revision: 0,
            values,
        };
        engine.handle_settings(&settings)?;
        Some(engine)
    }

    #[doc(hidden)]
    pub fn new_test_host_owned(generation_seed: &str, adapter: &str) -> Option<Self> {
        let capabilities = HostCapabilities {
            host_owned: true,
            // Synthetic fixtures carry native sticky and maximize facts;
            // opt into the native routes explicitly so the intended lanes
            // stay valid.
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        };
        Self::new_test_bound_with_capabilities(generation_seed, adapter, capabilities)
    }

    #[doc(hidden)]
    pub fn new_test_host_owned_equal_count(generation_seed: &str, adapter: &str) -> Option<Self> {
        let capabilities = HostCapabilities {
            host_owned: true,
            equal_count: true,
            // Same native-sticky/maximize fixture rationale as the other
            // defaults.
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        };
        Self::new_test_bound_with_capabilities(generation_seed, adapter, capabilities)
    }

    #[doc(hidden)]
    pub fn apply_test_settings(
        &mut self,
        revision: u64,
        values: Vec<(String, crate::connection::RawSettingValue)>,
    ) -> Option<SettingsReply> {
        self.handle_settings(&SettingsRequest {
            adapter: self.adapter.clone()?,
            generation: self.core_generation_str.clone(),
            request: format!("test-settings-{revision}"),
            revision,
            values,
        })
    }

    /// Fresh backend incarnation. `None` on a malformed generation token.
    /// A new incarnation (process start, Engine replacement, poison
    /// recovery, world reset) must construct a new [`WorldEngine`]; the
    /// adapter rebinds through bootstrap and never replays old sequences,
    /// effects, or logical creation tokens across generations.
    #[must_use]
    pub fn new(core_generation: &str) -> Option<Self> {
        Self::new_with_config(core_generation, WorldEngineConfig::default())
    }

    /// Fresh backend incarnation with explicit platform opt-ins. `None` on
    /// a malformed generation token. `Default` is exactly [`WorldEngine::new`].
    #[must_use]
    pub fn new_with_config(core_generation: &str, config: WorldEngineConfig) -> Option<Self> {
        let parsed = GenerationId::parse(core_generation)?;
        let mut engine = Engine::new();
        // Fixed-size admission stays enabled for every world carrier.
        // The default keeps the Linux opt-ins; native carriers can select
        // their existing send and maximized-focus policies explicitly.
        engine.set_fixed_size_admission(true);
        engine.set_maximized_focus_fence(config.maximized_focus_fence);
        engine.set_fixed_send_admission(config.fixed_send_admission);
        Some(Self {
            engine,
            core_generation: parsed,
            core_generation_str: core_generation.to_owned(),
            generation_seed: core_generation.to_owned(),
            connection_number: 0,
            config,
            adapter: None,
            connection_owner: None,
            application_identity: None,
            capabilities: None,
            settings: crate::settings_state::SettingsState::default(),
            initial_settings_accepted: false,
            ready_latched: false,
            state_revision: 0,
            has_last: false,
            last_request_seq: 0,
            last_digest: 0,
            last_report: None,
            last_observation_seq: 0,
            last_reply: None,
            has_last_v4: false,
            last_request_seq_v4: 0,
            last_digest_v4: 0,
            last_observation_seq_v4: 0,
            last_report_v4: None,
            last_reply_v4: None,
            prior_context: PriorAcceptedContext::default(),
            command_highwater: None,
            retained: Vec::new(),
            retained_floats: Vec::new(),
            retained_states: BTreeMap::new(),
            retained_clears: Vec::new(),
            retained_born: Vec::new(),
            born_seq: 0,
            born_attempted: BTreeSet::new(),
            born_held_fullscreen: BTreeSet::new(),
            born_floating_first: BTreeSet::new(),
            born_seen_nonfullscreen: BTreeSet::new(),
            born_keep_overlay: BTreeSet::new(),
            composed_maximized: BTreeSet::new(),
            frames: BTreeMap::new(),
            consumed_frames: BTreeMap::new(),
            frame_bursts: BTreeMap::new(),
            learned_mins: BTreeMap::new(),
            creation_counter: 0,
            native_creation_seq: 0,
            retained_creations: Vec::new(),
            pending_source_releases: Vec::new(),
            gesture: None,
            consumed_gesture_seq: 0,
            gesture_terminal: None,
            last_views: BTreeMap::new(),
            mapping: BTreeMap::new(),
            native_map: BTreeMap::new(),
            views: BTreeMap::new(),
            history: ScopeHistory::new(),
            displaced: BTreeMap::new(),
            output_facts: BTreeMap::new(),
            live_topology: BTreeMap::new(),
            tiling: BTreeMap::new(),
            members: BTreeMap::new(),
            floating_scopes: BTreeSet::new(),
            hold_reasons: BTreeMap::new(),
            pending_forget: Vec::new(),
            command_result: None,
            operation_settled: Vec::new(),
            switch_settled: Vec::new(),
            placement_bootstrapped: false,
            admission_focus_spent: BTreeSet::new(),
            automatic_focus_tokens: BTreeMap::new(),
            retained_focus: Vec::new(),
            pending_selection: None,
            pending_policy_selections: Vec::new(),
            selection_targets: BTreeMap::new(),
            retained_selection: BTreeMap::new(),
            selection_scopes: BTreeMap::new(),
            switch_target: crate::cursor_targeting::SwitchTarget::default(),
            fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
            focus_follows_cursor: false,
            cursor_warp: false,
            last_cursor: None,
            prev_cursor: None,
            ffc_last_hover: None,
            fresh_focus: crate::fresh_focus::FreshFocusMemory::default(),
            staged: StagedReport::default(),
            excluded_intent: BTreeMap::new(),
            pending_intent: BTreeMap::new(),
            intent_conflicts: BTreeSet::new(),
            intent_admitted: BTreeSet::new(),
            restored_floats: BTreeSet::new(),
            restored_sticky: BTreeMap::new(),
            sticky_saved: BTreeMap::new(),
            sticky_saved_incomplete: false,
            sticky_visible_rev: BTreeMap::new(),
            sticky_command_save: None,
            sticky_rollback_saves: BTreeMap::new(),
            intent_sticky_blocks: BTreeSet::new(),
            intent_sticky_saturated: false,
            presentation_live: BTreeMap::new(),
            presentation_float: BTreeMap::new(),
            presentation_tile: BTreeMap::new(),
            presentation_order: Vec::new(),
            presentation_focused: None,
            presentation_order_v4: Vec::new(),
            presentation_live_v4: BTreeMap::new(),
            presentation_focused_v4: None,
            latest_published_presentation_rev_v4: 0,
            #[cfg(test)]
            ingest_calls: 0,
            #[cfg(test)]
            ingest_trials: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_active_window: BTreeMap::new(),
            focused_fullscreen_presentation_suspension: false,
        })
    }

    /// Bind one adapter and its immutable host facts for a connection
    /// lifetime. Re-handshaking resets the world by replacing it with a
    /// fresh engine under a core-minted generation.
    pub fn handshake(&mut self, request: &HandshakeRequest) -> HandshakeReply {
        let reply = |adapter, correlation, generation, ready, kind| HandshakeReply {
            adapter,
            request: correlation,
            generation,
            ready,
            kind,
        };
        let Some(correlation) = CorrelationId::parse(&request.request) else {
            return reply(
                OwnerId::parse(&request.adapter)
                    .map(|id| id.as_str().to_owned())
                    .unwrap_or_else(|| "invalid".to_owned()),
                "invalid".to_owned(),
                None,
                false,
                Some(ProtocolKind::CorrelationInvalid),
            );
        };
        let Some(owner) = OwnerId::parse(&request.adapter) else {
            return reply(
                "invalid".to_owned(),
                correlation.as_str().to_owned(),
                None,
                false,
                Some(ProtocolKind::OwnerInvalid),
            );
        };
        // Direct typed own-identity gate (same rules as the strict wire
        // gate): support consistency plus carried-text grammar for appId,
        // executable, and processId. No unsupported/null bypass and no
        // ill-formed text bypass, even for callers that skip the wire codec.
        // Refusal mints no generation and mutates no state.
        if !request
            .application_identity
            .support_consistent(&request.capabilities.unavailable_facts)
            || !request.application_identity.grammar_valid()
        {
            return reply(
                request.adapter.clone(),
                request.request.clone(),
                None,
                false,
                Some(ProtocolKind::Malformed),
            );
        };
        let next_number = if self.adapter.is_none() {
            self.connection_number
        } else {
            let Some(number) = self.connection_number.checked_add(1) else {
                return reply(
                    request.adapter.clone(),
                    request.request.clone(),
                    None,
                    false,
                    Some(ProtocolKind::RevisionExhausted),
                );
            };
            number
        };
        let generation = if next_number == 0 {
            self.generation_seed.clone()
        } else {
            format!("{}-{next_number}", self.generation_seed)
        };
        if GenerationId::parse(&generation).is_none() {
            return reply(
                request.adapter.clone(),
                request.request.clone(),
                None,
                false,
                Some(ProtocolKind::RevisionExhausted),
            );
        }
        let Some(mut fresh) = Self::new_with_config(&generation, self.config) else {
            return reply(
                request.adapter.clone(),
                request.request.clone(),
                None,
                false,
                Some(ProtocolKind::RevisionExhausted),
            );
        };
        fresh.generation_seed.clone_from(&self.generation_seed);
        fresh.connection_number = next_number;
        fresh.focused_fullscreen_presentation_suspension =
            self.focused_fullscreen_presentation_suspension;
        fresh.adapter = Some(request.adapter.clone());
        fresh.connection_owner = Some(owner);
        fresh.engine.sync_binding(
            fresh.connection_owner.as_ref().expect("connection owner"),
            &fresh.core_generation,
        );
        fresh.application_identity = Some(request.application_identity.clone());
        fresh.capabilities = Some(request.capabilities.clone());
        *self = fresh;
        reply(
            request.adapter.clone(),
            request.request.clone(),
            Some(generation),
            false,
            None,
        )
    }

    /// Accept settings only for the exact bound adapter and generation.
    /// Stale or foreign envelopes cannot reach `SettingsState`.
    pub fn handle_settings(&mut self, request: &SettingsRequest) -> Option<SettingsReply> {
        if CorrelationId::parse(&request.request).is_none()
            || self.adapter.as_deref() != Some(request.adapter.as_str())
            || self.core_generation_str != request.generation
        {
            return None;
        }
        let caps = self.capabilities.as_ref()?;
        let reply = self.settings.process(request, caps);
        if reply.status == crate::connection::SettingsStatus::Accepted {
            self.initial_settings_accepted = true;
            self.engine
                .set_fixed_size_predicate(self.settings.fixed_size_predicate);
            self.engine
                .set_anchor_fallback(self.settings.anchor_fallback);
            self.engine
                .set_no_space_result(self.settings.no_space_result);
            self.engine
                .set_fresh_placement(self.settings.fresh_placement);
            self.switch_target = self.settings.switch_target;
            self.fresh_placement = self.settings.fresh_placement;
            self.focus_follows_cursor = self
                .settings
                .effective
                .get(crate::settings_catalog::FIELD_FOCUS_FOLLOWS_CURSOR)
                == Some(&crate::settings_state::EffectiveSetting::Bool(true));
            self.cursor_warp = self
                .settings
                .effective
                .get(crate::settings_catalog::FIELD_CURSOR_WARP)
                == Some(&crate::settings_state::EffectiveSetting::Bool(true))
                && caps.warp;
        }
        Some(reply)
    }

    #[must_use]
    pub fn capabilities(&self) -> Option<&HostCapabilities> {
        self.capabilities.as_ref()
    }

    #[must_use]
    pub fn application_identity(&self) -> Option<&ApplicationIdentity> {
        self.application_identity.as_ref()
    }

    #[must_use]
    pub fn settings(&self) -> &crate::settings_state::SettingsState {
        &self.settings
    }

    pub(crate) fn host_owned_workspace_domains(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.host_owned)
    }

    pub(crate) fn equal_workspace_count(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.equal_count)
    }

    pub(crate) fn birth_clear(&self) -> bool {
        self.capabilities
            .as_ref()
            .is_some_and(|caps| caps.birth_clear)
    }

    pub(crate) fn switch_target_setting(&self) -> crate::cursor_targeting::SwitchTarget {
        self.settings.switch_target
    }

    pub(crate) fn fresh_placement_setting(&self) -> crate::cursor_targeting::FreshPlacement {
        self.settings.fresh_placement
    }

    pub(crate) fn focus_follows_cursor_setting(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_FOCUS_FOLLOWS_CURSOR),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        )
    }

    pub(crate) fn cursor_warp_setting(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_CURSOR_WARP),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        ) && self.capabilities.as_ref().is_some_and(|caps| caps.warp)
    }

    pub(crate) fn gap_setting(&self) -> i32 {
        match self
            .settings
            .effective
            .get(crate::settings_catalog::FIELD_INNER_GAP)
        {
            Some(crate::settings_state::EffectiveSetting::Int(value)) => *value,
            _ => 8,
        }
    }

    pub(crate) fn outer_gap_setting(&self) -> i32 {
        match self
            .settings
            .effective
            .get(crate::settings_catalog::FIELD_OUTER_GAP)
        {
            Some(crate::settings_state::EffectiveSetting::Int(value)) => *value,
            _ => 8,
        }
    }

    /// Scale one logical pixel setting into physical adapter units for one
    /// output: `floor(logical * scale + 0.5)`, checked. `None` on overflow
    /// (callers hold narrowly instead); never a 64-bit clamp.
    pub(crate) fn scale_once(logical: i32, scale: f64) -> Option<i32> {
        let scaled = (f64::from(logical) * scale + 0.5).floor();
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&scaled) {
            return None;
        }
        Some(scaled as i32)
    }

    /// Scale one logical float style setting into physical adapter units
    /// for one output: `floor(logical * scale + 0.5)`, checked. Same
    /// nearest, halves-upward rule as [`Self::scale_once`], for the v4
    /// float style settings (border width/gap/radius, underlay extension).
    /// `None` on non-finite inputs, overflow, or a scaled value above the
    /// existing [`crate::world_report::MAX_PRESENTATION_STYLE_PX`] cap
    /// (1024 valid, 1025 holds); callers fail closed and the v4 stage
    /// helper stages the narrow unreadable hold.
    pub(crate) fn scale_float_once(logical: f64, scale: f64) -> Option<i32> {
        if !logical.is_finite() || !scale.is_finite() {
            return None;
        }
        let scaled = (logical * scale + 0.5).floor();
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&scaled) {
            return None;
        }
        if scaled > f64::from(crate::world_report::MAX_PRESENTATION_STYLE_PX) {
            return None;
        }
        Some(scaled as i32)
    }

    /// Per-output physical gaps: the logical inner/outer settings scaled
    /// once by `scale`. `None` when either scaled value overflows.
    pub(crate) fn scaled_gaps(&self, scale: f64) -> Option<(i32, i32)> {
        Some((
            Self::scale_once(self.gap_setting(), scale)?,
            Self::scale_once(self.outer_gap_setting(), scale)?,
        ))
    }

    pub(crate) fn workspace_mode_setting(&self) -> crate::workspace_scope::WorkspaceMode {
        self.settings.workspace_mode
    }

    pub(crate) fn fixed_size_predicate_setting(&self) -> crate::size_hints::FixedSizePredicate {
        self.settings.fixed_size_predicate
    }

    pub(crate) fn born_maximized_setting(&self) -> crate::born_maximized::BornMaximized {
        self.settings.born_maximized
    }

    pub(crate) fn default_tiled(&self) -> bool {
        matches!(
            self.settings
                .effective
                .get(crate::settings_catalog::FIELD_WORKSPACE_DEFAULT_TILED),
            Some(crate::settings_state::EffectiveSetting::Bool(true))
        )
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready_latched
    }

    #[must_use]
    pub fn core_generation(&self) -> &str {
        &self.core_generation_str
    }

    /// Opt in to neutral focused-fullscreen presentation suspension (stage 5).
    /// KWin only offers truthful `fullScreen`, never game identity: when the
    /// focused surface is fullscreen on its observed output, published
    /// presentation suspends on that output only. No game/app/size
    /// heuristics; no hotkey/input suppression; normal fullscreen window
    /// actions, membership, and shortcuts are unchanged.
    #[must_use]
    pub fn with_focused_fullscreen_presentation_suspension(mut self) -> Self {
        self.focused_fullscreen_presentation_suspension = true;
        self
    }

    /// Bootstrap reply for discovery: generation/capabilities with
    /// `ready=false` and no applicable desired state. The adapter sends a
    /// full report before any ready reply; empty pre-bootstrap state never
    /// authorizes removal.
    #[must_use]
    pub fn bootstrap_reply(&self, adapter_incarnation: &str, request_id: &str) -> WorldReply {
        WorldReply {
            contract_version: WORLD_CONTRACT_VERSION,
            adapter_incarnation: adapter_incarnation.to_owned(),
            core_generation: self.core_generation_str.clone(),
            request_id: request_id.to_owned(),
            based_on_observation_seq: 0,
            state_revision: self.state_revision,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            diagnostics: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            // Empty bootstrap mirrors the current revision: no counter
            // advance, no authority.
            presentation: crate::world_presentation::WorldPresentation {
                rev: self.state_revision,
                ..Default::default()
            },
            intent_writes: Vec::new(),
            switch_settled: Vec::new(),
        }
    }

    /// Advance the consumed gesture id watermark to at least `id`.
    /// Monotonic: replays can never move it backward.
    pub(crate) fn consume_gesture_id(&mut self, id: u64) {
        self.consumed_gesture_seq = self.consumed_gesture_seq.max(id);
    }

    /// Record one incorporated gesture terminal and advance the watermark.
    /// Called exactly once per incorporated terminal ingress (release with
    /// or without commit, cancel, Esc, explicit-close clear), including
    /// terminals that refuse without binding: the identity is consumed so
    /// a retry can never commit. Stages the result for the reply being
    /// published; [`WorldEngine::publish`] takes it exactly once.
    pub(crate) fn record_gesture_terminal(&mut self, id: u64, committed: bool) {
        self.consume_gesture_id(id);
        self.gesture_terminal = Some(GestureSettled { id, committed });
    }

    /// Whether one terminal ingress replays an already-settled gesture:
    /// its stable id sits at or below the consumed watermark.
    pub(crate) fn gesture_replayed(&self, id: u64) -> bool {
        id <= self.consumed_gesture_seq
    }

    /// Borrow the retained engine (read-only inspection for in-crate callers).
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Fresh-placement bootstrap gate: policy routing applies only for a
    /// genuinely new lifetime observed under complete inventory after the
    /// startup baseline was accepted. The baseline report itself (first
    /// complete) and every pre-bootstrap report adopt native affinity;
    /// once bootstrapped, newcomers route even after an empty seed,
    /// close-all, or an incomplete gap. Retained homings never re-route.
    pub(crate) fn fresh_placement_armed_for(&self, ctx: &RawRouting<'_>) -> bool {
        self.placement_bootstrapped && ctx.inventory_complete
    }

    /// Retained cursor-first switch target from the latest accepted
    /// report (cursor-output default).
    #[must_use]
    pub fn switch_target(&self) -> crate::cursor_targeting::SwitchTarget {
        self.switch_target
    }

    /// Retained fresh-window placement from the latest accepted report
    /// (cursor-output/last-focused default).
    #[must_use]
    pub fn fresh_placement(&self) -> crate::cursor_targeting::FreshPlacement {
        self.fresh_placement
    }

    /// Retained focus-follows-cursor setting (default off); the stage-4b
    /// core policy in [`crate::world_cursor`] actuates it.
    #[must_use]
    pub fn focus_follows_cursor(&self) -> bool {
        self.focus_follows_cursor
    }

    /// Retained cursor-warp setting (default off); the stage-4b core
    /// policy in [`crate::world_cursor`] actuates it.
    #[must_use]
    pub fn cursor_warp(&self) -> bool {
        self.cursor_warp
    }

    /// Normalized cursor retained from the latest accepted report, `None`
    /// when the adapter could not read it.
    #[must_use]
    pub fn last_cursor(&self) -> Option<(i32, i32)> {
        self.last_cursor
    }

    /// Trace-only snapshot of the last fresh focused-leg routing decision:
    /// the current managed output, the retained last-known output, the
    /// chosen leg output, and the winning basis. This is the cached
    /// historical leg decision, not the current report's focus fact; the
    /// adapter pairs it with the current reported token. Stable output ids
    /// only; no titles, pids, frames, or input. The chosen leg feeds fresh
    /// placement (cursor-first), so it is the focused leg alone, not the
    /// final policy output.
    #[must_use]
    pub fn fresh_focus_trace(
        &self,
    ) -> (Option<String>, Option<String>, Option<String>, &'static str) {
        let trace = &self.fresh_focus.trace;
        (
            trace.current.as_ref().map(|output| output.0.clone()),
            trace.retained.as_ref().map(|output| output.0.clone()),
            trace.chosen.as_ref().map(|output| output.0.clone()),
            trace.basis.as_str(),
        )
    }

    /// Outstanding retained transfer/view targets.
    #[must_use]
    pub fn retained_transfers(&self) -> Vec<TransferTarget> {
        self.retained.iter().map(|r| r.target.clone()).collect()
    }

    /// Exact post-send source focus for one retained stay transfer: the
    /// committed source-domain focus after a `follow=false` send, read from
    /// committed session state (never recomputed, never simulated). Matches
    /// the retained transfer by stable operation identity plus source
    /// homing; anything else is `Unknown` and must refuse. `Resident` names
    /// a window still homed to the source (never the mover): either the
    /// committed focus when it names a source resident, or the sole
    /// surviving source resident when committed focus is `None` (a collapsed
    /// single-leaf tree carries a fresh leaf id the pre-send stack cannot
    /// name, so the single survivor is exact by elimination, never a
    /// guess). `Empty` is a proven-empty source with committed focus `None`;
    /// `Unknown` covers no such transfer, a follow transfer, a source
    /// mismatch, an unknown session, a focus still naming the mover (as in
    /// floating-only transfers with no Engine plan), or multiple residents
    /// with no committed focus (ambiguous, never guessed). Callers map
    /// `Unknown` to refusal and `Empty` to a `None` fallback; only
    /// `Resident` pins.
    #[must_use]
    pub fn stay_source_focus(&self, operation_id: &str, source: &DomainKey) -> StaySourceFocus {
        let Some(retained) = self
            .retained
            .iter()
            .find(|r| r.target.operation_id == operation_id)
        else {
            return StaySourceFocus::Unknown;
        };
        if retained.target.follow {
            return StaySourceFocus::Unknown;
        }
        if retained.source_output != source.output
            || retained.source_workspace != source.workspace.0
        {
            return StaySourceFocus::Unknown;
        }
        let mover = match &retained.target.kind {
            TransferKind::Window(window) => window.clone(),
            TransferKind::View | TransferKind::Migration => return StaySourceFocus::Unknown,
        };
        let Some(session) = self.engine.session(source) else {
            // No committed session for a retained stay source means the send
            // emptied and retired it: proven-empty, never unknown. (A
            // mismatched source already refused above, and a floating-only
            // transfer with no Engine plan leaves its session behind.)
            return StaySourceFocus::Empty;
        };
        let (focused_domain, focused_leaf) = session.focus();
        let snapshot = session.snapshot();
        if let (Some(domain), Some(leaf)) = (focused_domain.clone(), focused_leaf.clone()) {
            if domain != *source {
                return StaySourceFocus::Unknown;
            }
            let Some(link) = snapshot.windows.iter().find(|link| {
                link.leaf == leaf
                    && link.output == source.output
                    && link.workspace == source.workspace
            }) else {
                return StaySourceFocus::Unknown;
            };
            if link.window == mover {
                return StaySourceFocus::Unknown;
            }
            StaySourceFocus::Resident(link.window.clone())
        } else if focused_domain.is_none() && focused_leaf.is_none() {
            let mut residents: Vec<WindowId> = snapshot
                .windows
                .iter()
                .filter(|link| link.output == source.output && link.workspace == source.workspace)
                .map(|link| link.window.clone())
                .collect();
            residents.sort();
            residents.dedup();
            if residents.contains(&mover) {
                return StaySourceFocus::Unknown;
            }
            match residents.len() {
                0 => StaySourceFocus::Empty,
                1 => StaySourceFocus::Resident(residents.remove(0)),
                _ => StaySourceFocus::Unknown,
            }
        } else {
            StaySourceFocus::Unknown
        }
    }

    /// Latest published global revision.
    #[must_use]
    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Adapter routing projection (read-only, never a second policy):
    /// ordered logical outputs known to the core.
    #[must_use]
    pub fn logical_outputs(&self) -> Vec<String> {
        let mut out: Vec<String> = self.mapping.keys().cloned().collect();
        out.sort();
        out
    }

    /// Adapter routing projection: ordered logical ring for one output.
    #[must_use]
    pub fn logical_ring(&self, output: &str) -> Vec<String> {
        self.mapping.get(output).cloned().unwrap_or_default()
    }

    /// Adapter routing projection: retained logical view for one output.
    #[must_use]
    pub fn logical_view(&self, output: &str) -> Option<String> {
        self.views.get(output).cloned()
    }

    /// Adapter routing projection: retained tiling mode (absent seeds tiled).
    #[must_use]
    pub fn logical_tiling(&self, output: &str, workspace: &str) -> TilingMode {
        let key = DomainKey {
            output: OutputId(output.to_owned()),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        self.tiling.get(&key).copied().unwrap_or(TilingMode::Tiled)
    }

    /// Adapter routing projection: members homed to one logical workspace.
    #[must_use]
    pub fn logical_members(&self, output: &str, workspace: &str) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = self
            .members
            .iter()
            .filter(|(_, state)| {
                state.home.output.0 == output && state.home.workspace.0 == workspace
            })
            .map(|(window, _)| window.clone())
            .collect();
        out.sort();
        out
    }

    /// Adapter routing projection: retained logical homing for one lifetime.
    #[must_use]
    pub fn adapter_homing(&self, window: &WindowId) -> Option<(String, String)> {
        self.members
            .get(window)
            .map(|state| (state.home.output.0.clone(), state.home.workspace.0.clone()))
    }

    /// Adapter routing projection: all known lifetimes (sorted).
    #[must_use]
    pub fn logical_members_all(&self) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = self.members.keys().cloned().collect();
        out.sort();
        out
    }

    /// Adapter routing projection: retained previous-view history per output
    /// (observed views only, never desires).
    #[must_use]
    pub fn logical_previous(&self, output: &str) -> Option<String> {
        self.history.previous(output)
    }

    /// Accept one complete report and publish one globally ordered reply.
    ///
    /// Transactional: every cheap gate (generation, request estimate,
    /// version, shape, identity, freshness, observation order, conservative
    /// reply upper bound, revision exhaustion) runs before any mutation.
    /// Accepted inputs then run to completion on a staged clone; the exact
    /// canonical encoding gates the staged reply before it commits. A
    /// staged overflow leaves sessions, retained transfers, ordering, and
    /// command consumption exactly as before. Only reports passing every
    /// cheap gate pay for the clone.
    pub fn handle_report(&mut self, report: &WorldReport) -> WorldOutcome {
        // A report can only refer to the lifetime established by handshake.
        if self.adapter.as_deref() != Some(report.adapter_incarnation.as_str()) {
            return WorldOutcome::Rejected {
                kind: KIND_OWNER,
                message: MSG_OWNER,
            };
        }
        if report.core_generation != self.core_generation_str {
            return WorldOutcome::Rejected {
                kind: KIND_GENERATION,
                message: MSG_GENERATION,
            };
        }
        // Core-owned grading: directly constructed identifiable invalid
        // scale/metadata rows become narrow holds like their
        // protocol-decoded twins before any estimate, validation, or
        // mutation. Structural violations still refuse below; the pure
        // shape gate is unchanged for callers that bypass the engine.
        // Window grading follows the screen grading: invalid metadata
        // values hold first, then unavailable-fact contradictions hold
        // against this connection's capabilities.
        let graded = grade_invalid_screens(report);
        let report = graded.as_ref().map_or(report, |owned| owned);
        let graded_windows = grade_invalid_windows(report);
        let report = graded_windows.as_ref().map_or(report, |owned| owned);
        let unavailable = self
            .capabilities
            .as_ref()
            .map(|capabilities| capabilities.unavailable_facts.as_slice())
            .unwrap_or(&[]);
        let native_sticky_supported = crate::world_state::state_property_supported(
            self.capabilities(),
            crate::world_report::StateProperty::Sticky,
        );
        let maximize_supported = crate::world_state::state_property_supported(
            self.capabilities(),
            crate::world_report::StateProperty::Maximized,
        );
        let graded_conflicting = grade_conflicting_windows(
            report,
            unavailable,
            native_sticky_supported,
            maximize_supported,
        );
        let report = graded_conflicting.as_ref().map_or(report, |owned| owned);
        if report_estimate(report) > WORLD_MAX_REQUEST_BYTES {
            return WorldOutcome::Rejected {
                kind: KIND_OVERSIZED,
                message: MSG_OVERSIZED,
            };
        }
        if report.contract_version != WORLD_CONTRACT_VERSION {
            return WorldOutcome::Rejected {
                kind: KIND_VERSION,
                message: MSG_VERSION,
            };
        }
        if let Some(reason) = validate_report_reason(report) {
            return WorldOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                // Fixed check vocabulary, never payload bytes.
                message: reason,
            };
        }
        let native_evidence = !report.native.is_empty()
            || report
                .windows
                .iter()
                .any(|window| window.native_workspace.as_deref().is_some())
            || report.member_attempts.iter().any(|attempt| {
                matches!(
                    attempt.pre.as_ref(),
                    Some(crate::world_report::WorkspaceRef::Native { .. })
                ) || matches!(
                    attempt.post.as_ref(),
                    Some(crate::world_report::WorkspaceRef::Native { .. })
                )
            })
            || !report.view_attempts.is_empty()
            || !report.creation_attempts.is_empty();
        if native_evidence && !self.host_owned_workspace_domains() {
            return WorldOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "native-without-capability",
            };
        }
        // Authoritative host-arm gate for readiness brackets: the native
        // arm rides only on host-owned hosts, the observed-output arm only
        // elsewhere. Checked before any adoption even when home/out is
        // null; a null location is valid evidence but never proves
        // readiness. Context-free structural validation above allows both
        // shapes.
        let host_owned = self.host_owned_workspace_domains();
        let arrival_host_mismatch = report.focus_feedback.iter().any(|row| {
            row.arrival
                .iter()
                .any(|bracket| bracket.location.is_native() != host_owned)
        });
        if arrival_host_mismatch {
            return WorldOutcome::Rejected {
                kind: KIND_SNAPSHOT,
                message: "arrival-host-mismatch",
            };
        }
        let Some(owner) = self.connection_owner.clone() else {
            return WorldOutcome::Rejected {
                kind: KIND_OWNER,
                message: MSG_OWNER,
            };
        };
        let correlation = match CorrelationId::parse(&report.request_id) {
            Some(correlation) => correlation,
            None => {
                return WorldOutcome::Rejected {
                    kind: KIND_CORRELATION,
                    message: MSG_CORRELATION,
                };
            }
        };
        let digest = report_digest(report);
        let (has_last, last_seq, last_digest, last_obs) = (
            self.has_last,
            self.last_request_seq,
            self.last_digest,
            self.last_observation_seq,
        );
        if has_last {
            if report.request_seq < last_seq {
                return WorldOutcome::Rejected {
                    kind: KIND_SUPERSEDED,
                    message: MSG_SUPERSEDED,
                };
            }
            if report.request_seq == last_seq {
                // Fast digest pre-check first; true input equality decides.
                if digest != last_digest {
                    return WorldOutcome::Rejected {
                        kind: KIND_REUSED,
                        message: MSG_REUSED,
                    };
                }
                if self.last_report.as_ref().is_some_and(|last| last == report) {
                    let Some(cached) = self.last_reply.clone() else {
                        return WorldOutcome::Rejected {
                            kind: KIND_SUPERSEDED,
                            message: MSG_SUPERSEDED,
                        };
                    };
                    return WorldOutcome::Duplicate(cached);
                }
                return WorldOutcome::Rejected {
                    kind: KIND_REUSED,
                    message: MSG_REUSED,
                };
            }
        }
        if has_last && report.observation_seq < last_obs {
            return WorldOutcome::Rejected {
                kind: KIND_OBS_STALE,
                message: MSG_OBS_STALE,
            };
        }
        // Conservative pre-mutation reply bound: no observation convergence,
        // slot cleanup, or command mutation runs when the worst-case reply
        // cannot fit.
        let retained_len = self.retained.len() + self.retained_floats.len();
        let created_len = report.screens.len();
        let staged_windows = self
            .staged
            .facts
            .len()
            .saturating_add(self.staged.held_windows.len());
        if reply_upper_bound(
            report,
            retained_len,
            self.engine.domain_keys().len(),
            created_len,
            staged_windows,
            self.retained_publish_footprint(),
        )
        .saturating_add(max_intent_rollback_bound(self.rollback_reserve_count()))
        .saturating_add(self.switch_settle_reserve())
        // A select-family command can mint one zero-obligation issuing
        // terminal while old bindings settle in the same report; reserve
        // it alongside the retained bindings, never from lane slack.
        .saturating_add(self.switch_issuing_reserve(report.command.as_ref()))
        // Commandless policy views can mint at most one zero-obligation
        // terminal per fresh displaced/returned workspace at publish;
        // bound the placements knowable before convergence.
        .saturating_add(self.policy_terminal_reserve(report))
            > WORLD_MAX_REPLY_BYTES
        {
            return WorldOutcome::Rejected {
                kind: KIND_REPLY_SIZE,
                message: MSG_REPLY_SIZE,
            };
        }
        // Counter exhaustion refuses and re-bootstraps under a new
        // generation; it never wraps in place.
        if self.state_revision == u64::MAX {
            return WorldOutcome::Rejected {
                kind: KIND_REVISION_EXHAUSTED,
                message: MSG_REVISION_EXHAUSTED,
            };
        }
        // Fresh reports carrying an already-consumed command sequence
        // reconcile observation but never re-execute the command.
        let consumed = match (report.command_seq, report.command.as_ref()) {
            (Some(seq), Some(_)) => self.command_highwater.is_some_and(|mark| seq <= mark),
            (Some(_), None) | (None, Some(_)) => {
                return WorldOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    message: "command-pair-invalid",
                };
            }
            (None, None) => false,
        };
        let usable_initial_observation = report.inventory_complete
            && !report.screens.is_empty()
            && report.screens.iter().all(|screen| {
                screen.complete
                    && screen
                        .full_rect
                        .is_some_and(|rect| rect.w > 0 && rect.h > 0)
                    && screen
                        .work_area
                        .is_some_and(|rect| rect.w > 0 && rect.h > 0)
            });
        if !self.initial_settings_accepted || (!self.ready_latched && !usable_initial_observation) {
            // Bootstrap still ingests validated intent rows so records
            // delivered in incomplete pre-ready inventories survive until
            // the first usable fact. No admission, sweep, or writes here.
            // Complete/closed cleanup applies immediately too so stale
            // orphans and conflicts cannot exhaust the joint bound before
            // ready; retention pre-ready is bounded pending data, never
            // report or write authority.
            self.ingest_staged_intent(&Self::capture_observation(report));
            if report.inventory_complete {
                let live: BTreeSet<WindowId> = report
                    .windows
                    .iter()
                    .map(|fact| fact.window.clone())
                    .collect();
                let held: BTreeSet<WindowId> = report
                    .held_windows
                    .iter()
                    .map(|held| held.window.clone())
                    .collect();
                self.pending_intent
                    .retain(|id, _| live.contains(id) || held.contains(id));
                self.intent_conflicts
                    .retain(|id| live.contains(id) || held.contains(id));
                self.restored_floats
                    .retain(|id| live.contains(id) || held.contains(id));
                self.restored_sticky
                    .retain(|id, _| live.contains(id) || held.contains(id));
                self.intent_admitted
                    .retain(|id| live.contains(id) || held.contains(id));
                self.intent_sticky_blocks
                    .retain(|id| live.contains(id) || held.contains(id));
            }
            return WorldOutcome::Accepted(self.bootstrap_report_reply(report, &correlation));
        }
        // Stage everything past this point on a clone. Exact-encoding
        // overflow (or any late failure) drops the candidate; `self` is
        // adopted only after the staged reply encodes within budget.
        let mut staged = self.clone();
        if usable_initial_observation {
            staged.ready_latched = true;
        }
        let outcome = staged.run_accepted(report, &owner, &correlation, digest, consumed);
        match outcome {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
                // Outbound intent lane gate: every published/cached reply
                // originates valid. This increment publishes at most one
                // ToggleFloat save (see `toggle_float_intent_save`); the
                // gate admits empty and single-save replies alike and must
                // pass here before the candidate commits or encodes. Fixed
                // check vocabulary, never payload.
                if let Some(reason) = validate_intent_writes_reason(&reply.intent_writes) {
                    return WorldOutcome::Rejected {
                        kind: KIND_SNAPSHOT,
                        message: reason,
                    };
                }
                if reply_estimate(&reply) > WORLD_MAX_REPLY_BYTES
                    || encode_reply_canonical(&reply).len() > WORLD_MAX_REPLY_BYTES
                {
                    return WorldOutcome::Rejected {
                        kind: KIND_REPLY_SIZE,
                        message: MSG_REPLY_SIZE,
                    };
                }
                *self = staged;
                self.last_reply = Some(reply.clone());
                if consumed {
                    WorldOutcome::CommandConsumed(reply)
                } else {
                    WorldOutcome::Accepted(reply)
                }
            }
            WorldOutcome::Duplicate(_) => {
                // Staged runs never duplicate: the latest duplicate returns
                // above, before staging.
                WorldOutcome::Rejected {
                    kind: KIND_SUPERSEDED,
                    message: MSG_SUPERSEDED,
                }
            }
            rejected => rejected,
        }
    }

    fn bootstrap_report_reply(
        &self,
        report: &WorldReport,
        correlation: &CorrelationId,
    ) -> WorldReply {
        // Bootstrap publishes no intent decisions: the empty `intent_writes`
        // below trivially satisfies `validate_intent_writes_reason`, so the
        // staged commit gate above stays the single enforcing boundary.
        WorldReply {
            contract_version: WORLD_CONTRACT_VERSION,
            adapter_incarnation: report.adapter_incarnation.clone(),
            core_generation: report.core_generation.clone(),
            request_id: correlation.as_str().to_owned(),
            based_on_observation_seq: report.observation_seq,
            state_revision: self.state_revision,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            diagnostics: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            // Empty bootstrap mirrors the current revision: no counter
            // advance, no authority.
            presentation: crate::world_presentation::WorldPresentation {
                rev: self.state_revision,
                ..Default::default()
            },
            intent_writes: Vec::new(),
            switch_settled: Vec::new(),
        }
    }

    /// Joint intent ledger occupancy: pending restores plus conflict marks
    /// plus sticky save blocks plus retained sticky-prior provenance share
    /// the single [`crate::world_report::MAX_INTENT_RECORDS`] cap, so every
    /// insert branch below (and the sticky-block insert) must gate on this,
    /// never on a partial sum. A pending-to-retained sticky commit removes
    /// the pending entry first, so it holds the joint count steady.
    pub(crate) fn intent_ledger_used(&self) -> usize {
        self.pending_intent.len()
            + self.intent_conflicts.len()
            + self.intent_sticky_blocks.len()
            + self.restored_sticky.len()
    }

    /// Record one lifetime-scoped save block for deferred sticky intent.
    /// Jointly bounded with pending/conflicts by
    /// [`crate::world_report::MAX_INTENT_RECORDS`]: when the bound leaves
    /// no room, the generation-scoped saturation latch trips instead and
    /// every save is suppressed until handshake reset. Fail-closed: a
    /// dropped block would let a save erase deferred sticky intent.
    /// Blocks confer no admission or native authority.
    pub(crate) fn insert_intent_sticky_block(&mut self, win: &WindowId) {
        if self.intent_sticky_blocks.contains(win) {
            return;
        }
        if self.intent_ledger_used() < crate::world_report::MAX_INTENT_RECORDS {
            self.intent_sticky_blocks.insert(win.clone());
        } else {
            self.intent_sticky_saturated = true;
        }
    }

    /// First-accepted exact saved-record store: inserts only when no newer
    /// runtime save exists (no late clobber). A cap drop latches
    /// [`Self::sticky_saved_incomplete`] fail-closed instead of silently
    /// losing the exact prior.
    pub(crate) fn remember_first_sticky_saved(
        &mut self,
        win: &WindowId,
        data: crate::intent_data::IntentData,
    ) {
        if self.sticky_saved.contains_key(win) {
            return;
        }
        if self.sticky_saved.len() < crate::world_report::MAX_INTENT_RECORDS {
            self.sticky_saved.insert(win.clone(), data);
        } else {
            self.sticky_saved_incomplete = true;
        }
    }

    /// Conflicting imports invalidate any first-cached exact prior: drop
    /// the bogus saved record and latch incomplete fail-closed.
    pub(crate) fn drop_saved_as_conflicting(&mut self, win: &WindowId) {
        self.sticky_saved.remove(win);
        self.sticky_saved_incomplete = true;
    }

    /// Ingest opaque intent rows from a staged observation capture, before
    /// classification, so the classifier sees pending first-admission wins
    /// from this same report. Reads only the capture's closed subjects,
    /// raw window facts (for own-process pruning), and intent records.
    /// Initial intakes call this exactly once per report before staging;
    /// staging and command restages never ingest.
    pub(crate) fn ingest_staged_intent(&mut self, observation: &StagedObservation) {
        use std::collections::{BTreeMap, BTreeSet};
        #[cfg(test)]
        {
            self.ingest_calls = self.ingest_calls.saturating_add(1);
            self.ingest_trials
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let closed: BTreeSet<&WindowId> = observation.closed.iter().collect();
        // Explicit closes end the lifetime: drop every ledger entry and
        // never store a closed subject.
        for id in &observation.closed {
            self.pending_intent.remove(id);
            self.intent_conflicts.remove(id);
            self.intent_admitted.remove(id);
            self.restored_floats.remove(id);
            self.restored_sticky.remove(id);
            self.intent_sticky_blocks.remove(id);
            self.sticky_saved.remove(id);
            self.sticky_visible_rev.remove(id);
            self.sticky_rollback_saves.remove(id);
        }
        // Own-process facts prove no import authority: drop pending and
        // conflict entries for proven-own subjects without touching
        // unrelated safety-held windows (which must stay recoverable).
        // Safety exclusion (`owned`/privileged) is not own-process and
        // never prunes here.
        let own_pid: Option<String> = self
            .application_identity
            .as_ref()
            .and_then(|identity| identity.process_id.clone());
        let mut own_proven: BTreeSet<WindowId> = BTreeSet::new();
        if let Some(own) = own_pid.as_deref() {
            for fact in &observation.raw_windows {
                if fact.process_id.as_deref() == Some(own) {
                    own_proven.insert(fact.window.clone());
                }
            }
        }
        for id in &own_proven {
            self.pending_intent.remove(id);
            self.intent_conflicts.remove(id);
            self.restored_floats.remove(id);
            self.restored_sticky.remove(id);
            self.intent_sticky_blocks.remove(id);
            self.sticky_saved.remove(id);
            self.sticky_visible_rev.remove(id);
            self.sticky_rollback_saves.remove(id);
        }
        // Group raw rows by subject; >1 distinct string is a conflict.
        let mut grouped: BTreeMap<&WindowId, BTreeSet<&str>> = BTreeMap::new();
        for record in &observation.intent_records {
            if closed.contains(&record.win) {
                continue;
            }
            if own_proven.contains(&record.win) {
                continue;
            }
            grouped
                .entry(&record.win)
                .or_default()
                .insert(record.data.as_str());
        }
        for (win, raws) in grouped {
            // First-admission fence: already admitted lifetimes ignore late
            // rows forever, even through exclusion/holds/minimize. Late
            // rows never inhibit later runtime saves either.
            if self.intent_admitted.contains(win) {
                continue;
            }
            // Sticky-preservation scan before any conflict handling: a
            // deferred stickyPrior non-null value in ANY duplicate row
            // blocks later saves for this lifetime, even when the mixed
            // rows also conflict. Sticky semantics are not implemented
            // yet, so a null-sticky save must never overwrite them. A
            // later non-sticky row never clears the block.
            if raws.iter().any(|raw| {
                matches!(
                    crate::intent_data::decode_intent_data(raw),
                    Ok(data) if data.sticky_prior.is_some()
                )
            }) {
                self.insert_intent_sticky_block(win);
            }
            if self.intent_conflicts.contains(win) {
                continue;
            }
            if raws.len() > 1 {
                self.pending_intent.remove(win);
                self.drop_saved_as_conflicting(win);
                if self.intent_ledger_used() < crate::world_report::MAX_INTENT_RECORDS {
                    self.intent_conflicts.insert(win.clone());
                }
                continue;
            }
            let raw = raws.iter().next().expect("grouped raw");
            let decoded = match crate::intent_data::decode_intent_data(raw) {
                Ok(data) => data,
                Err(_) => continue,
            };
            // First-admission coherence across the null/null boundary:
            // null/null carries no restore authority but its exact shape is
            // still saved knowledge, so a cached shape differing from this
            // import on either null/null side is a conflicting preimage.
            // Drop the bogus saved prior and latch fail-closed; pending
            // semantics below run unchanged so composed admission keeps its
            // old behavior and only native exact knowledge fails closed.
            // Valid-vs-valid differences keep the existing pending/conflict
            // path. Already admitted lifetimes returned above, so late rows
            // never latch.
            let decoded_null = decoded.float.is_none() && decoded.sticky_prior.is_none();
            let saved_boundary_conflict =
                self.sticky_saved.get(win).copied().is_some_and(|cached| {
                    cached != decoded
                        && (decoded_null
                            || (cached.float.is_none() && cached.sticky_prior.is_none()))
                });
            if saved_boundary_conflict {
                self.drop_saved_as_conflicting(win);
            }
            // Whole sticky-bearing records restore as a whole: sticky
            // dominates the float lane and the prior stays the exact bool,
            // so every `float` lane (`None`/`false`/`true`) with a non-null
            // prior is a usable pending restore, never partly reconstructed.
            // A null/null record carries no restore authority but its exact
            // shape is still first-accepted saved knowledge for rollback.
            if decoded_null {
                if !saved_boundary_conflict {
                    self.remember_first_sticky_saved(win, decoded);
                }
                continue;
            }
            match self.pending_intent.get(win).copied() {
                Some(existing) if existing == decoded => {
                    // First-accepted import establishes exact saved knowledge
                    // when no newer runtime save exists; repeats never
                    // clobber.
                    if !saved_boundary_conflict {
                        self.remember_first_sticky_saved(win, decoded);
                    }
                }
                Some(_) => {
                    self.pending_intent.remove(win);
                    self.drop_saved_as_conflicting(win);
                    if self.intent_ledger_used() < crate::world_report::MAX_INTENT_RECORDS {
                        self.intent_conflicts.insert(win.clone());
                    }
                }
                None => {
                    if self.intent_ledger_used() < crate::world_report::MAX_INTENT_RECORDS {
                        self.pending_intent.insert(win.clone(), decoded);
                        if !saved_boundary_conflict {
                            self.remember_first_sticky_saved(win, decoded);
                        }
                    } else {
                        // Joint-ledger drop loses the exact prior even when
                        // the saved map still has room; fail closed.
                        self.sticky_saved_incomplete = true;
                        self.remember_first_sticky_saved(win, decoded);
                    }
                }
            }
        }
    }

    pub(crate) fn engine_knows_window_any(&self, window: &WindowId) -> bool {
        if self.members.contains_key(window) {
            return true;
        }
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            if session
                .snapshot()
                .windows
                .iter()
                .any(|link| link.window == *window)
            {
                return true;
            }
            if session.is_exception(window) {
                return true;
            }
            if session.minimized_slot(window).is_some() {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_tiled_window_any(&self, window: &WindowId) -> bool {
        self.engine.domain_keys().iter().any(|key| {
            self.engine.session(key).is_some_and(|session| {
                session
                    .snapshot()
                    .windows
                    .iter()
                    .any(|link| link.window == *window)
            })
        })
    }

    pub(crate) fn is_exception_window_any(&self, window: &WindowId) -> bool {
        self.engine.domain_keys().iter().any(|key| {
            self.engine
                .session(key)
                .is_some_and(|session| session.is_exception(window))
        })
    }

    /// Conservative pre-mutation count of retained lifetimes that can
    /// publish a target-lane row without a live fact in this report: every
    /// retained session's tiled links plus exceptions plus minimize slots
    /// (surviving a gapped previous report that left staged small), plus
    /// retained mover rows (one per outstanding window transfer plus every
    /// migration member). A union upper bound via the sum: deduped
    /// lifetimes counted twice still fit.
    pub(crate) fn retained_publish_footprint(&self) -> usize {
        let mut total = 0usize;
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            total = total
                .saturating_add(session.snapshot().windows.len())
                .saturating_add(session.exception_count())
                .saturating_add(session.minimized_count());
        }
        for retained in &self.retained {
            match &retained.target.kind {
                TransferKind::Window(_) => total = total.saturating_add(1),
                TransferKind::Migration => {
                    total = total.saturating_add(retained.members.len());
                }
                TransferKind::View => {}
            }
        }
        total
    }

    /// Pre-mutation count of potentially generated native sticky rollback
    /// writes: every sticky binding with metadata and `!followup_done` (any
    /// disposition can still settle into a queued rollback on a healthy
    /// commandless proof or record-free recovery), plus already-queued
    /// rollback writes. The sum is conservative on overlap (dedup optional):
    /// a window present in both still fits. Never derived from the report's
    /// own `stateAttempts` length, so record-free priors reserve alike.
    pub(crate) fn rollback_reserve_count(&self) -> usize {
        let pending = self
            .retained_states
            .values()
            .filter(|binding| {
                binding
                    .sticky_meta
                    .as_ref()
                    .is_some_and(|meta| !meta.followup_done)
            })
            .count();
        pending.saturating_add(self.sticky_rollback_saves.len())
    }

    /// Latch first successful admissions, commit durable intent, and keep
    /// the exclusion ledger truthful to proven session state. Called after
    /// each sweep (and resweep). Pending drops only on real success
    /// (tiled for `float:false`, non-automatic exception for
    /// `float:true`); minimized placeholders and holds retain pending.
    /// Sessionless floating-route admissions latch separately after
    /// membership converges (see `latch_floating_admissions`): settle
    /// runs before reconcile creates those members.
    /// The exclusion ledger syncs to current session truth for live
    /// sessions only, so a successful runtime float clears a stale tile
    /// win while exclusion gaps preserve continuity.
    pub(crate) fn settle_intent_admissions(&mut self) {
        // Own-process facts staged this report prove no import authority:
        // drop pending/conflict/restored for proven-own subjects.
        // Safety-held windows never match here and stay recoverable.
        let own_excluded: Vec<WindowId> = self
            .staged
            .derived
            .iter()
            .filter_map(|(id, classified)| {
                if classified.own_excluded {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        for id in own_excluded {
            self.pending_intent.remove(&id);
            self.intent_conflicts.remove(&id);
            self.restored_floats.remove(&id);
            self.restored_sticky.remove(&id);
            self.intent_sticky_blocks.remove(&id);
            self.sticky_saved.remove(&id);
            self.sticky_visible_rev.remove(&id);
        }
        // Latch first successful admissions: late rows for already
        // admitted lifetimes are ignored forever, even through
        // exclusion/holds/minimize. Session tiled links and
        // floating-base session exceptions are real success;
        // overlay-only placeholders (non-floating fullscreen/maximized
        // births) and minimized slots alone never latch. Membership
        // alone never proves admission either: reconcile homes rows
        // even when held, so members latch only on staged health.
        // Pending-only latching would miss record-free first admissions.
        let mut known: Vec<WindowId> = Vec::new();
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            for link in &session.snapshot().windows {
                known.push(link.window.clone());
            }
            for observed in session.exception_observed() {
                if !observed.floating && (observed.fullscreen || observed.maximized) {
                    continue;
                }
                known.push(observed.window.clone());
            }
        }
        let member_windows: Vec<WindowId> = self.members.keys().cloned().collect();
        for window in member_windows {
            if self.staged.excluded.contains(&window) {
                continue;
            }
            if self
                .staged
                .held_windows
                .iter()
                .any(|held| held.window == window)
            {
                continue;
            }
            if !self.staged.facts.contains_key(&window) {
                continue;
            }
            let derived = self.derived_for(&window);
            if derived.hold || derived.excluded || derived.own_excluded || derived.hidden {
                continue;
            }
            if !derived.floating && (derived.overlay_fullscreen || derived.overlay_maximized) {
                continue;
            }
            known.push(window);
        }
        for win in &known {
            self.intent_admitted.insert(win.clone());
        }
        // Conflicts confer no winner: drop them once the lifetime admits
        // instead of consuming the joint bound forever.
        for win in &known {
            self.intent_conflicts.remove(win);
        }
        let pending: Vec<(WindowId, crate::intent_data::IntentData)> = self
            .pending_intent
            .iter()
            .map(|(id, data)| (id.clone(), *data))
            .collect();
        for (win, data) in pending {
            // Sticky-bearing restores commit only the exact prior into
            // sticky provenance, never a tile override or ordinary float:
            // sticky dominates the float lane. Real float success is a
            // healthy staged non-automatic exception (never a merely homed
            // held/hidden/minimized/overlay placeholder); pending survives
            // otherwise, exactly like the float lane below.
            if let Some(prior) = data.sticky_prior {
                let healthy = self.staged.facts.contains_key(&win)
                    && !self.staged.excluded.contains(&win)
                    && !self
                        .staged
                        .held_windows
                        .iter()
                        .any(|held| held.window == win)
                    && {
                        let derived = self.derived_for(&win);
                        !derived.hold
                            && !derived.excluded
                            && !derived.own_excluded
                            && !derived.hidden
                            && (derived.floating
                                || !(derived.overlay_fullscreen || derived.overlay_maximized))
                    };
                if !healthy {
                    continue;
                }
                if !self.is_exception_window_any(&win)
                    || self.engine.is_automatic_fixed_float_any(&win)
                {
                    continue;
                }
                // Pending leaves first so the retained commit holds the
                // joint ledger count steady.
                self.pending_intent.remove(&win);
                self.restored_sticky.insert(win.clone(), prior);
                // Composed restore initializes the stable visibility
                // revision absent-only: first safe admission authorizes,
                // ordinary observations never renew.
                if self.sticky_route() == crate::world_sticky::StickyRoute::Composed
                    && !self.sticky_visible_rev.contains_key(&win)
                {
                    self.sticky_visible_rev.insert(win, self.state_revision + 1);
                }
            } else if data.float == Some(false) {
                // Real tile success is a tiled link, not a minimized
                // placeholder or an exception: retain pending otherwise.
                if !self.is_tiled_window_any(&win) {
                    continue;
                }
                self.engine.commit_fixed_tile_override(&win);
                self.pending_intent.remove(&win);
            } else if data.float == Some(true) {
                // Real float success is a non-automatic exception: retain
                // pending through minimized placeholders, and never
                // promote automatic floats to explicit.
                if !self.is_exception_window_any(&win)
                    || self.engine.is_automatic_fixed_float_any(&win)
                {
                    continue;
                }
                self.restored_floats.insert(win.clone());
                self.pending_intent.remove(&win);
            }
        }
        // Exclusion-ledger truth sync for live sessions only: current
        // session marks win over stale mirrors (e.g. a successful runtime
        // float clears a restored tile win), while exclusion gaps with no
        // live session preserve continuity untouched.
        let mut live: Vec<WindowId> = Vec::new();
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            for link in &session.snapshot().windows {
                live.push(link.window.clone());
            }
            for observed in session.exception_observed() {
                live.push(observed.window.clone());
            }
            for (window, _, _) in session.minimized_entries() {
                live.push(window.clone());
            }
        }
        live.sort();
        live.dedup();
        for win in live {
            let tile = self.engine.has_fixed_tile_override_any(&win);
            let automatic = self.engine.is_automatic_fixed_float_any(&win);
            if tile || automatic {
                self.excluded_intent.insert(
                    win,
                    ExcludedIntent {
                        tile_override: tile,
                        automatic_fixed: automatic,
                    },
                );
            } else {
                self.excluded_intent.remove(&win);
            }
        }
        // Runtime unfloat wins over restored float provenance: a lifetime
        // the session now proves tiled leaves the explicit-float set.
        // Unknown/excluded lifetimes keep the mark for recovery.
        let floated_marked: Vec<WindowId> = self.restored_floats.iter().cloned().collect();
        for win in floated_marked {
            if self.is_tiled_window_any(&win) {
                self.restored_floats.remove(&win);
            }
        }
    }

    /// Latch healthy floating-route first admissions after membership
    /// converges. Floating workspaces release their sessions, so a
    /// floating newcomer never appears in session links at settle time
    /// and only gains membership at reconcile (after settle). Health is
    /// staged truth, never membership alone: held, hidden, excluded,
    /// or overlay-only placeholders never latch. A pending true
    /// consumes into explicit-float provenance at the observed frame
    /// (no geometry op); a pending sticky-bearing record consumes only
    /// its exact prior into sticky provenance (never ordinary float, never
    /// a tile win); a pending false stays pending as the explicit
    /// tile base for a later tiling enable (never forced tiled now).
    pub(crate) fn latch_floating_admissions(&mut self) {
        let candidates: Vec<(WindowId, bool)> = self
            .members
            .iter()
            .filter(|(_, state)| self.floating_scopes.contains(&state.home))
            .filter(|(id, _)| !self.intent_admitted.contains(*id))
            .map(|(id, _)| {
                let healthy = self.staged.facts.contains_key(id)
                    && !self.staged.excluded.contains(id)
                    && !self
                        .staged
                        .held_windows
                        .iter()
                        .any(|held| &held.window == id)
                    && {
                        let derived = self.derived_for(id);
                        !derived.hold
                            && !derived.excluded
                            && !derived.own_excluded
                            && !derived.hidden
                            && (derived.floating
                                || !(derived.overlay_fullscreen || derived.overlay_maximized))
                    };
                (id.clone(), healthy)
            })
            .collect();
        for (win, healthy) in candidates {
            if !healthy {
                continue;
            }
            self.intent_admitted.insert(win.clone());
            self.intent_conflicts.remove(&win);
            if let Some(prior) = self
                .pending_intent
                .get(&win)
                .copied()
                .and_then(|data| data.sticky_prior)
            {
                // Pending leaves first so the retained commit holds the
                // joint ledger count steady.
                self.pending_intent.remove(&win);
                self.restored_sticky.insert(win.clone(), prior);
                // Composed floating restore initializes the stable
                // visibility revision absent-only, same as the session
                // route above.
                if self.sticky_route() == crate::world_sticky::StickyRoute::Composed
                    && !self.sticky_visible_rev.contains_key(&win)
                {
                    self.sticky_visible_rev.insert(win, self.state_revision + 1);
                }
            } else if self
                .pending_intent
                .get(&win)
                .is_some_and(|data| data.float == Some(true))
            {
                self.restored_floats.insert(win.clone());
                self.pending_intent.remove(&win);
            }
        }
    }

    /// Synchronize explicit-float provenance with a successful runtime
    /// float toggle: a toggled-to-float non-automatic exception gains the
    /// durable mark, a toggled-to-tile lifetime loses it. Unknown or held
    /// subjects keep their mark. Only explicit command wins enter; generic
    /// automatic/capacity floats never do.
    pub(crate) fn sync_toggle_float_command(&mut self, command: &WorldCommand) {
        let window = match command {
            WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                window, ..
            }) => WindowId(window.clone()),
            _ => return,
        };
        if self.is_tiled_window_any(&window) {
            self.restored_floats.remove(&window);
        } else if self.is_exception_window_any(&window)
            && !self.engine.is_automatic_fixed_float_any(&window)
        {
            self.restored_floats.insert(window);
        }
    }

    pub(crate) fn capture_excluded_intent(&mut self) {
        // Snapshot lifetime intent before the sweep retires excluded
        // slots: an exclusion is never a native close, so the approved
        // fixed tile override and automatic fixed mark survive the
        // same-lifetime gap. Only records proven intent: never
        // overwrites a retained win with a retired-session false.
        for id in self.staged.excluded.clone() {
            let tile = self.engine.has_fixed_tile_override_any(&id);
            let automatic = self.engine.is_automatic_fixed_float_any(&id);
            if tile || automatic {
                self.excluded_intent.insert(
                    id,
                    ExcludedIntent {
                        tile_override: tile,
                        automatic_fixed: automatic,
                    },
                );
            }
        }
        // Explicit native closes end the lifetime: never resurrect intent
        // for a reused id.
        for id in self.staged.closes.clone() {
            self.excluded_intent.remove(&id);
        }
    }

    /// Run one accepted report on the staged candidate: converge logical
    /// rings/views, stage the observation, settle retained state, sweep
    /// every readable logical workspace, execute at most one command, and
    /// publish one globally monotonic revision. Called only on the clone;
    /// the caller adopts the candidate after the exact-encoding gate.
    fn run_accepted(
        &mut self,
        report: &WorldReport,
        owner: &OwnerId,
        correlation: &CorrelationId,
        digest: u64,
        consumed: bool,
    ) -> WorldOutcome {
        // Freeze implicit send sources from pre-observation retained
        // homing, before staging can admit newcomers or settle rehoming.
        // The frozen explicit output is the only source the command below
        // may use; the home leg must predate stage_facts/settle/reconcile
        // so first-tick admissions and rehomed movers keep the retired
        // adapter contract exactly (homed pre-tick output, never post-sweep
        // membership). Report-only fallback legs need no freeze.
        // Direction-only migration freezes here too: the old production
        // route pinned the pre-tick retained view, so view settlement or
        // bootstrap in this report must never retarget the current intent.
        // First-tick unbootstrapped migration refuses; same-report created
        // views never migrate. Retained topology target selection and all
        // gates stay downstream on staged state.
        let source_routing = RawRouting::from_source(report);
        let (frozen_send_source, frozen_migration_source) =
            self.freeze_command_sources(report.command.as_ref(), &source_routing);
        // Typed event identity for the command path below: the validated
        // owner/correlation pair, never a re-parse of the report.
        let issue = EventIssue {
            owner: owner.clone(),
            correlation: correlation.clone(),
        };
        // Logical rings/views converge first (bootstrap, hotplug, tiling
        // seeds) so staging assigns facts against this report's ownership.
        // Exactly-once initial ingestion: capture, ingest from the
        // capture, then stage from that same capture, so the classifier
        // sees pending first-admission wins this same report with no
        // second ingestion. Later command restages reuse the
        // classification-only `restage_classification` and never reingest.
        let observation = Self::capture_observation(report);
        self.ingest_staged_intent(&observation);
        self.stage_captured(report, observation);
        self.converge_logical_pre();
        // View flights settle with staged native proof before staging:
        // host-owned confirms and external supersessions move core views,
        // so homing assigns against adapter truth, never a stale desire.
        // Native-free flights are kept for their selection binding and
        // never settle from echoes here.
        self.settle_views_pre();
        // Stage the observation: homing plus one domain per known logical
        // workspace.
        self.build_staged_domains();
        // Stage-4a gesture bind (observation half): validate the ingress
        // and retain the session before settlement and the sweep, so both
        // converge with the shielded homing. Never commits; the act half
        // runs after the sweep.
        self.gesture_bind(&BTreeSet::new());
        // Remaining flights (window, migration, floats) settle against the
        // staged homing and echoes. Settlement classifies against the
        // pre-settlement staged homing (raw observation versus the
        // authoritative desired projection); a proven migration refusal can
        // roll back mapping/members/views/session ownership, so restage
        // afterwards: the sweep and publish below must use post-rollback
        // domains, never the pre-rollback ones.
        self.settle_rest(report.command.as_ref(), &issue, consumed);
        self.build_staged_domains();
        // State bindings settle against post-rollback staged domains: the
        // readability join must see the committed homing, and the raw capture
        // runs before sweep holds. Once per accepted report, including
        // consumed-command reports (no early return above).
        self.settle_states();
        // Ordinary focus feedback settles against the retained latest
        // batch before the sweep: historical correlation only, never
        // current-authority or readability gated, so consumed and
        // observation-only reports settle exactly like command reports.
        self.settle_focus_feedback();
        // Selection arrival bindings settle the same way: first exact
        // opaque-tuple correlation latches raw evidence with no authority
        // upgrade, so consumed and observation-only reports settle alike.
        self.settle_selection_feedback();
        // Selection switch terminals settle next from the latched feedback
        // plus first-correlated LastWrite evidence, still before the
        // command-only rollback boundary.
        self.settle_selection_switch();
        // Core-owned frame policy: fold mechanical write/readback facts into
        // retained clamp/transient memory (scoped by exact staged lifetime)
        // so this same reply publishes suppression.
        self.settle_frames();
        // Cursor observations remain report facts; computational policy is
        // retained from the connection settings envelope.
        self.prev_cursor = self.last_cursor;
        self.last_cursor = self.staged.observation.cursor;
        self.engine.set_fresh_cursor(self.staged.observation.cursor);
        // Floating workspaces skip convergence entirely: no session is
        // created or converged there, and no frame writes publish for them.
        self.floating_scopes = self
            .staged
            .domains
            .iter()
            .map(|domain| domain.key.clone())
            .filter(|key| {
                self.tiling
                    .get(key)
                    .is_some_and(|mode| *mode == TilingMode::Floating)
            })
            .collect();
        // Trailing lifecycle and history converge after staging so they see
        // homed occupancy; core-owned topology retains so the same report's
        // commands resolve against its own geometry.
        self.converge_logical_post();
        self.retain_live_topology();
        // Retained membership before the sweep, per swept domain: the
        // admission-focus pass diffs this against converged state to find
        // genuinely new lifetimes. Only domains with a retained session
        // count; fresh seeds (bootstrap, restart, disable/enable
        // re-adoption) never mint admission intents.
        let mut known_before: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for domain in &self.staged.domains {
            if !domain.readable {
                continue;
            }
            if self.floating_scopes.contains(&domain.key) {
                continue;
            }
            let Some(session) = self.engine.session(&domain.key) else {
                continue;
            };
            let mut known = BTreeSet::new();
            for link in session.snapshot().windows {
                if link.output == domain.key.output && link.workspace == domain.key.workspace {
                    known.insert(link.window.0.clone());
                }
            }
            for observed in session.exception_observed() {
                if observed.output == domain.key.output
                    && observed.workspace == domain.key.workspace
                {
                    known.insert(observed.window.0.clone());
                }
            }
            for (window, output, workspace) in session.minimized_entries() {
                if output == domain.key.output && workspace == domain.key.workspace {
                    known.insert(window.0.clone());
                }
            }
            known_before.insert(domain.key.clone(), known);
        }
        // Honest retained tile-slot context for the born predicates: every
        // tiled link the engine held before this report's sweep, across all
        // sessions (a slot in an unswept domain still proves the lifetime is
        // no birth). Computed pre-sweep like `known_before`.
        let mut slotted_before: BTreeSet<WindowId> = BTreeSet::new();
        for domain in &self.staged.domains {
            let Some(session) = self.engine.session(&domain.key) else {
                continue;
            };
            for link in session.snapshot().windows {
                slotted_before.insert(link.window.clone());
            }
        }
        // Reconcile every readable tiled domain; hold the rest explicitly.
        // Floating domains skip convergence (untouched, never held).
        // Hold reasons are per-report staging only: cleared here so a
        // recovered scope never inherits a previous hold.
        self.hold_reasons.clear();
        let mut held_scopes: BTreeSet<DomainKey> = BTreeSet::new();
        // Host-owned adoption fence: a fact on an authoritative output
        // whose native evidence is unknown, unmapped, or conflicting keeps
        // its retained homing but must not bless writes derived from that
        // retained state. Fence the homed scope with the existing
        // Unreadable authority (no new lane, no new enum): publish
        // withholds frame and membership writes there. Unhomed unknowns
        // (newcomers awaiting evidence) publish no row at all.
        if self.host_owned_workspace_domains() {
            let mut fence: Vec<DomainKey> = Vec::new();
            for (window, fact) in &self.staged.facts {
                let authoritative = match fact.output.as_ref() {
                    Some(_) => self.native_authoritative(),
                    // Unknown output under host-owned domains never proves
                    // membership: fence the retained homing like any
                    // unknown receipt.
                    None => self.host_owned_workspace_domains(),
                };
                if !authoritative {
                    continue;
                }
                if self.effective_logical_receipt(fact).is_some() {
                    continue;
                }
                if let Some(key) = self.staged.homing.get(window) {
                    fence.push(key.clone());
                }
            }
            // Incompatible fresh shared projection: fence every staged
            // scope (the fallback singletons), not just homed members.
            if self.native_order_conflict() {
                for domain in &self.staged.domains {
                    if !fence.contains(&domain.key) {
                        fence.push(domain.key.clone());
                    }
                }
            }
            for key in fence {
                self.hold_reasons
                    .insert(key.clone(), HoldReason::Unreadable);
                held_scopes.insert(key);
            }
        }
        // Narrowly held rows keep their retained homes: hold those scopes
        // so the sweep never converges live sessions away from under them.
        // Unhomed held rows (newcomers with no facts) hold nothing: no
        // authority, no membership.
        let held_window_scopes: Vec<DomainKey> = self
            .staged
            .held_windows
            .iter()
            .filter_map(|held| self.core_homing(&held.window))
            .collect();
        for key in &held_window_scopes {
            self.hold_reasons
                .entry(key.clone())
                .or_insert(HoldReason::Unreadable);
            held_scopes.insert(key.clone());
        }
        // Snapshot safety-exclusion intent before the sweep retires the
        // excluded slots (the sweep converges them as closes for slot
        // removal only, never a native close). Same-lifetime lift below
        // restores from this ledger through the stage-time fallback.
        self.capture_excluded_intent();
        // One-time native sticky dependent followup: exact-tuple Applied
        // off/clear proof lands the dependent tile/float membership, ON
        // marks done, and exact-tuple RefusedProven proof rolls back the
        // exact pre-command runtime plus saved record (one queued write),
        // now that floating scopes are fresh and held scopes are staged,
        // before the ordinary sweep converges. Ambiguous proof never
        // retries.
        self.run_native_sticky_followups(&issue, &held_scopes);
        // Clone the staged domains for iteration: sweeps borrow `self`
        // mutably while reading staged homing.
        let domains = self.staged.domains.clone();
        for domain in &domains {
            if held_window_scopes.contains(&domain.key) {
                continue;
            }
            if !domain.readable {
                continue;
            }
            if self.floating_scopes.contains(&domain.key) {
                continue;
            }
            // Gap preservation: incomplete inventory never infers
            // minimize/close from absence. Outstanding migration source and
            // target scopes hold explicitly so the retained sessions (tree,
            // shares, focus) survive the gap; settlement already holds the
            // flight narrowly.
            if !self.staged.observation.inventory_complete
                && self.retained.iter().any(|r| {
                    matches!(&r.target.kind, TransferKind::Migration)
                        && (DomainKey {
                            output: r.source_output.clone(),
                            workspace: crate::directional::WorkspaceId(r.source_workspace.clone()),
                        } == domain.key
                            || DomainKey {
                                output: r.target.target_output.clone(),
                                workspace: crate::directional::WorkspaceId(
                                    r.target.target_workspace.clone(),
                                ),
                            } == domain.key)
                })
            {
                self.hold_reasons.insert(
                    domain.key.clone(),
                    crate::world_report::HoldReason::IncompleteInventory,
                );
                held_scopes.insert(domain.key.clone());
                continue;
            }
            self.sweep_domain(domain, owner, correlation, &mut held_scopes);
        }
        // First-admission latch plus durable explicit-tile commit, before
        // focus/commands: restored intent must be committed so a
        // same-report float command can supersede it transactionally.
        self.settle_intent_admissions();
        // Changed native focus observation under the same fail-closed
        // gates as command recording: a changed `focused_global` naming a
        // live lifetime records the canonical slot through
        // `record_canonical` (tiled ingestion gates: readable, unheld,
        // owning-Session validation). Unchanged re-reports carry no new
        // information and never overwrite an unlanded command choice;
        // empty focus records nothing.
        {
            let prev = self.prior_context.prev_focused_global.clone();
            let current_focused = self.staged.observation.focused_global.clone();
            if !current_focused.is_empty() && prev != current_focused {
                let active = WindowId(current_focused);
                let _ = self.record_canonical(&active, &held_scopes, false);
            }
        }
        // Commandless born maximize-clear minting: first-seen maximized /
        // fullscreen births bind here (post-sweep, so `known_before` names
        // the pre-sweep retained lifetimes) and ride this same reply's
        // `clear_operation` desire. Runs for consumed reports too:
        // reconciliation never mints commands but born admission is
        // observation, never a command.
        {
            let mut pre_known: BTreeSet<WindowId> = BTreeSet::new();
            for known in known_before.values() {
                for id in known {
                    pre_known.insert(WindowId(id.clone()));
                }
            }
            self.mint_born_clears(report, &pre_known, &slotted_before);
        }
        for (key, window) in std::mem::take(&mut self.pending_forget) {
            self.engine.forget_minimized_slot(&key, &window);
        }
        // Stage-4a gesture ingress before admission focus and commands:
        // preview and release read the swept observation; a committed
        // release resweeps and restages exactly like the command path so
        // admission focus, commands, and publishing below see the
        // committed ownership. Gesture ingress is observation, never a
        // command: it runs even on consumed-command reports, and it never
        // consumes a sequence.
        self.command_result = None;
        match self.gesture_act(&issue, &mut held_scopes) {
            GestureOutcome::Absent | GestureOutcome::Held => {}
            GestureOutcome::Settled { committed, result } => {
                if committed {
                    let was_floating = self.floating_scopes.clone();
                    self.resweep_retiled(owner, correlation, &mut held_scopes, &was_floating);
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                }
                self.command_result = result.map(|boxed| *boxed);
            }
        }
        // Fresh-admission focus intent for this report's sweep, if any.
        // Command execution mints no newcomers, so every publish below
        // shares it.
        let mut effects = self.take_admission_focus(&known_before, &held_scopes);
        // Command path: consumed sequences reconcile only, never re-execute.
        // A consumed command never re-executes, so its result never
        // re-emits either; a gesture result is the gesture's own, never
        // the command's, so consumed replies may still carry it.
        if consumed {
            crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
            let result = self.command_result.take();
            let reply = self.publish(report, true, effects, result, &held_scopes);
            return self.finish(report, digest, reply);
        }
        if let Some(command) = report.command.as_ref() {
            let command_seq = report.command_seq.expect("validated pair");
            let was_floating = self.floating_scopes.clone();
            // Observation-only pre-command image: settlement, sweep,
            // gesture, admission focus, and intent effects above are
            // trusted observations. A command refusal restores exactly
            // this image so no partial command mutation survives, while
            // pre-command observation effects stay published.
            let precommand = self.clone();
            match self.execute_command(
                &issue,
                command_seq,
                command,
                &held_scopes,
                frozen_send_source.clone(),
                frozen_migration_source.clone(),
            ) {
                CommandApplied::Effect(effect) => {
                    self.resweep_retiled(owner, correlation, &mut held_scopes, &was_floating);
                    let result = self.command_result.take();
                    // Commands move rings/views; restage so publishing
                    // assigns facts against the committed ownership.
                    // Homing stays truthful: echoes still gate views, and
                    // in-flight members home by raw screen.
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(command);
                    // Admission intent first: an explicit command
                    // focuses after it, so user intent wins when they
                    // coincide. A same-target admission duplicate is
                    // redundant with the explicit winner: drop it so one
                    // Focus effect names the native target once.
                    if effect.kind == crate::world_report::EffectKind::Focus {
                        effects.retain(|existing| {
                            !(existing.kind == crate::world_report::EffectKind::Focus
                                && existing.target == effect.target)
                        });
                    }
                    effects.push(effect);
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::NoEffect => {
                    self.resweep_retiled(owner, correlation, &mut held_scopes, &was_floating);
                    // See above: restage against committed ownership.
                    self.build_staged_domains();
                    self.settle_intent_admissions();
                    self.sync_toggle_float_command(command);
                    let result = self.command_result.take();
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::Rejected { kind, message } => {
                    // Creation exhaustion is a protocol envelope failure:
                    // it discards the staged candidate with no adoption,
                    // exactly like every pre-acceptance gate.
                    if kind == KIND_CREATION_EXHAUSTED {
                        return WorldOutcome::Rejected { kind, message };
                    }
                    // Restore the observation-only pre-command image so
                    // no partial command mutation (e.g. a trailing create
                    // before a view refusal) survives. Pre-command
                    // attempts, frames, transfers, and admission effects
                    // stay; the command sequence still consumes.
                    *self = precommand;
                    self.note_command(command_seq);
                    let typed = CommandRejectKind::parse(kind)
                        .unwrap_or(CommandRejectKind::OperationRefused);
                    self.command_result = Some(CommandResult::Rejected { kind: typed });
                    crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
                    let result = self.command_result.take();
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
            }
        }
        let result = self.command_result.take();
        crate::world_cursor::maybe_push_focus_follows(self, &held_scopes, &mut effects);
        let reply = self.publish(report, false, effects, result, &held_scopes);
        self.finish(report, digest, reply)
    }

    /// Prune every retained-authority lifetime ledger against the current
    /// staged capture: canonical memory, exclusion/intent ledgers, the
    /// state/focus/selection lane pruners, and lifetime-local composed
    /// maximize. Lifetime presence reads the staged raw observation plus
    /// the staged held rows, never the classified facts alone: only
    /// own-process subjects are dropped from classification, while
    /// safety-excluded facts stay classified but ineligible; both stay
    /// lifetime evidence, and capability-held rows keep presence without
    /// authority. A complete report drops every dead identity; an
    /// incomplete one only drops explicit closes. Closed ids never survive
    /// even under reuse.
    pub(crate) fn prune_lifetime_state(&mut self) {
        // Canonical memory prunes to the live inventory: a complete report
        // drops every dead identity, an incomplete one only drops explicit
        // closes. Closed ids never survive even under reuse.
        // Owned narrow snapshots: the staged capture is re-read per region
        // below so no observation borrow crosses the lane-pruner calls.
        // Narrow sets stay scoped to the branch that needs them: incomplete
        // gaps never clone the raw inventory.
        let inventory_complete = self.staged.observation.inventory_complete;
        if inventory_complete {
            let live: BTreeSet<WindowId> = self
                .staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| fact.window.clone())
                .collect();
            let held: BTreeSet<WindowId> = self
                .staged
                .held_windows
                .iter()
                .map(|held| held.window.clone())
                .collect();
            self.last_active_window
                .retain(|_, window| live.contains(window));
            // Same lifetime bound for the exclusion ledger: a complete
            // inventory that carries neither the fact nor the exclusion
            // proves departure, so drop the retained intent. Explicit
            // closes already cleared at capture time.
            let excluded = self.staged.excluded.clone();
            self.excluded_intent
                .retain(|id, _| live.contains(id) || excluded.contains(id));
            // Pending/conflict ledgers share the proven-departure
            // fence: a complete inventory carrying neither a usable
            // fact, a held row, nor an exclusion proves the subject
            // departed before first success, so drop it. A carried
            // slot row alone is not live proof: it never retains.
            // Engine-known (admitted, minimized, exception, member)
            // subjects stay: complete absence after admission is
            // handled through the admitted latch and close path, never
            // by resurrecting pending. Closed subjects were already
            // dropped on ingest.
            let known: BTreeSet<WindowId> = self
                .pending_intent
                .keys()
                .chain(self.intent_conflicts.iter())
                .chain(self.intent_admitted.iter())
                .chain(self.restored_floats.iter())
                .chain(self.restored_sticky.keys())
                .chain(self.sticky_saved.keys())
                .filter(|id| self.engine_knows_window_any(id) || self.members.contains_key(*id))
                .cloned()
                .collect();
            self.pending_intent.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.intent_conflicts.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.restored_floats.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.restored_sticky.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            // Admitted lifetimes end on proven complete departure
            // (neither live, held, excluded, nor engine/member-known).
            // Temporary exclusion/holds keep the latch so late rows
            // stay ignored.
            self.intent_admitted.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.intent_sticky_blocks.retain(|id| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.sticky_saved.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
            self.sticky_rollback_saves
                .retain(|id, _| live.contains(id) || held.contains(id) || excluded.contains(id));
            self.sticky_visible_rev.retain(|id, _| {
                live.contains(id)
                    || held.contains(id)
                    || excluded.contains(id)
                    || known.contains(id)
            });
        } else if !self.staged.observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> =
                self.staged.observation.closed.iter().cloned().collect();
            self.last_active_window
                .retain(|_, window| !closed.contains(window));
            self.excluded_intent.retain(|id, _| !closed.contains(id));
        }
        self.prune_state_bindings();
        self.prune_focus_grants();
        self.prune_selection_grants();
        self.prune_selection_scopes();
        self.prune_selection_targets();
        self.prune_automatic_focus_tokens();
        // Lifetime-local composed maximize follows the live inventory:
        // a complete report drops dead identities (engine/member-known
        // lifetimes stay like the state bindings above), explicit closes
        // always prune, and restart drops the bit with the engine.
        if !self.staged.observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> =
                self.staged.observation.closed.iter().cloned().collect();
            self.composed_maximized
                .retain(|window| !closed.contains(window));
        }
        if inventory_complete {
            let live_ref: BTreeSet<&WindowId> = self
                .staged
                .observation
                .raw_windows
                .iter()
                .map(|fact| &fact.window)
                .collect();
            let held_ref: BTreeSet<&WindowId> = self
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            let excluded = self.staged.excluded.clone();
            let known: BTreeSet<WindowId> = self
                .composed_maximized
                .iter()
                .filter(|window| {
                    self.engine_knows_window_any(window) || self.members.contains_key(*window)
                })
                .cloned()
                .collect();
            self.composed_maximized.retain(|window| {
                live_ref.contains(window)
                    || held_ref.contains(window)
                    || excluded.contains(window)
                    || known.contains(window)
            });
        }
    }

    /// Publish one globally monotonic revision and record watermarks on the
    /// staged candidate. Every accepted report advances the revision, even a
    /// semantic no-op. The revision cannot wrap: exhaustion is refused
    /// before staging.
    fn finish(&mut self, report: &WorldReport, digest: u64, mut reply: WorldReply) -> WorldOutcome {
        self.prune_lifetime_state();
        // Retain the last-known focused output before membership converges,
        // so the latch reflects strictly prior retention: a stealing
        // newcomer or an empty report never overwrites pre-admission
        // context. Genuine managed focus only.
        self.note_focused_output();
        // Membership owner settles last on the staged candidate so the next
        // report homes through retained truth (floating release, minimized
        // and unreadable holds, view-switch stability). Included in the
        // adopted candidate, never a separate commit.
        self.reconcile_members();
        // Drop membership for workspaces lifecycle just removed (empty only,
        // so this is a backstop, never a live-member delete).
        let mut ringed: BTreeSet<DomainKey> = BTreeSet::new();
        for (output, ring) in &self.mapping {
            for id in ring {
                ringed.insert(DomainKey {
                    output: OutputId(output.clone()),
                    workspace: WorkspaceId(id.clone()),
                });
            }
        }
        self.members.retain(|_, state| {
            ringed.contains(&state.home) || self.displaced.contains_key(&state.home.output.0)
        });
        // Sessionless floating-route admissions latch here: settle runs
        // before reconcile creates those members, so no earlier pass can
        // see them. Health-gated on staged truth, never membership alone.
        self.latch_floating_admissions();
        // Uniform presentation retains last (stage 5): pager frames, order,
        // and focus settle against converged membership, then the reply
        // carries the computed border/underlay/pager presentation. The
        // paused (game-lane) report skips projection work inside the compute.
        self.retain_pager_presentation(report);
        reply.presentation = self.presentation_for_publish(report);
        self.state_revision += 1;
        reply.state_revision = self.state_revision;
        // Common publication token: every ready publication recomputes the
        // presentation above, so its issuing label is this reply's own
        // revision. No new counter, no retention, no comparison.
        reply.presentation.rev = reply.state_revision;
        if report.observation_seq > self.last_observation_seq || !self.has_last {
            self.last_observation_seq = report.observation_seq;
        }
        self.has_last = true;
        self.last_request_seq = report.request_seq;
        self.last_digest = digest;
        self.last_report = Some(report.clone());
        // Retain the prior-accepted context from this accepted candidate:
        // the staged raw focus plus the just-published common rows. Every
        // publish path funnels through here, including consumed-command
        // reports; rejected candidates never reach this boundary.
        self.prior_context = PriorAcceptedContext::capture(
            &self.staged.observation.focused_global,
            &reply.workspaces,
            &reply.scopes,
            &reply.windows,
        );
        // First accepted complete inventory establishes the startup set
        // (possibly empty). Later complete reports route newcomers.
        if report.inventory_complete {
            self.placement_bootstrapped = true;
        }
        // Latest proving echo per output for external-supersession
        // detection: host-owned domains track the native-derived echo
        // (never the explicit logical echo). Native-free outputs retain no
        // last views: their views ride the frozen selection binding.
        if self.host_owned_workspace_domains() {
            self.retain_native_last_views();
        }
        if reply.command_consumed {
            WorldOutcome::CommandConsumed(reply)
        } else {
            WorldOutcome::Accepted(reply)
        }
    }
}
