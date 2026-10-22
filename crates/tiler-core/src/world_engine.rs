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
//! lag. Possibly-executed feedback holds the target without re-actuation.
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

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::ScopeHistory;
use crate::world_commands::CommandApplied;
use crate::world_report::{
    CommandResult, FrameAttempt, HoldReason, TilingMode, TransferKind, TransferTarget,
    WORLD_CONTRACT_VERSION, WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, WorldReply,
    WorldReport, encode_reply_canonical, reply_estimate, reply_upper_bound, report_digest,
    report_estimate, validate_report_reason,
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

/// Retained transfer plus the source homing needed for shielding and
/// settlement. The reply projects only the [`TransferTarget`]. Policy
/// transfers (hotplug displacement/return member moves) share the exact
/// settlement/shielding path as command transfers; their target rows may
/// name live-but-not-yet-observed homings, which hold when unresolvable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedTransfer {
    pub(crate) target: TransferTarget,
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
    /// supersession) retained under the operation identity after its
    /// feedback row is acknowledged. Separate from the no-retry
    /// `TransferTarget.held` authority: `held` alone cannot distinguish a
    /// possibly-executed attempt from a proven terminal one. Only
    /// `RefusedProven`/`SupersededExternal` latch here; later readable
    /// evidence reconciles against the latched outcome. `None` until a
    /// terminal feedback row arrives.
    pub(crate) migration_terminal: Option<crate::world_report::AttemptOutcome>,
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

/// Per-report staged observation: screens as reported, one
/// [`StagedDomain`] per known logical workspace, adapter view echoes, and
/// the raw fact index the settlement path proves against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StagedReport {
    pub(crate) screens: Vec<crate::world_report::ScreenReport>,
    pub(crate) domains: Vec<StagedDomain>,
    /// Non-empty adapter view echoes by output.
    pub(crate) echoes: BTreeMap<String, String>,
    /// Raw observed screen per fact, before any logical assignment.
    pub(crate) fact_output: BTreeMap<WindowId, OutputId>,
    /// Full raw facts by window id.
    pub(crate) facts: BTreeMap<WindowId, crate::world_report::WindowFact>,
    /// Raw explicit closes.
    pub(crate) closes: BTreeSet<WindowId>,
    /// Staged logical homing per window.
    pub(crate) homing: BTreeMap<WindowId, DomainKey>,
    /// Carried gesture barrier for the staged run.
    pub(crate) gesture_barrier: bool,
}

/// Stateful world boundary around the retained [`Engine`].
#[derive(Debug, Clone)]
pub struct WorldEngine {
    pub(crate) engine: Engine,
    pub(crate) core_generation: GenerationId,
    pub(crate) core_generation_str: String,
    pub(crate) state_revision: u64,
    has_last: bool,
    last_request_seq: u64,
    last_digest: u64,
    /// Latest accepted typed report, bounded by the input caps in
    /// [`crate::world_report`]. Duplicate detection compares full typed
    /// input equality against this value; the digest is only a fast
    /// pre-check, never the identity (no hash-collision aliasing).
    last_report: Option<WorldReport>,
    last_observation_seq: u64,
    last_adapter_incarnation: Option<String>,
    pub(crate) last_reply: Option<WorldReply>,
    /// Monotonic command high-watermark per (incarnation, generation):
    /// any `command_seq` at or below it is already consumed, so eviction
    /// can never resurrect a sequence. Commandless reports never move it.
    pub(crate) command_highwater: Option<u64>,
    pub(crate) retained: Vec<RetainedTransfer>,
    /// Outstanding explicit float-placement targets, at most one per window.
    /// Bounded by the window inventory itself.
    pub(crate) retained_floats: Vec<RetainedFloat>,
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
    /// Conservative learned minimum sizes per exact lifetime (stage 3b):
    /// proven app-held growth merged into the normalized hint path before
    /// layout projection. Separate from transient pair suppression: exact
    /// clears never erase these, and unrelated changes never shrink them.
    /// Pruned with the frame maps.
    pub(crate) learned_mins: BTreeMap<WindowId, crate::world_frames::LearnedMin>,
    pub(crate) creation_counter: u64,
    pub(crate) last_views: BTreeMap<OutputId, String>,
    /// Core-owned logical workspace rings (D2): per-screen ordered lists of
    /// stable logical workspace ids, retained across reports. Rings converge
    /// every accepted report through lifecycle (trailing creation/removal),
    /// hotplug displacement, and commands (migration moves the logical id
    /// between screens). Native observations never move ownership.
    pub(crate) mapping: BTreeMap<String, Vec<String>>,
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
    /// [`crate::world_report::WorldCommand::SetWorkspaceTiling`] or seeded
    /// from the reported saved default; the adapter never decides.
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
    /// Fresh-admission focus intents already emitted, by window id (D4):
    /// at-most-once per lifetime, pruned on close so a reused id focuses
    /// again as a fresh newcomer. Survives incarnation resets within the
    /// generation (retained sessions persist there too); a new generation
    /// constructs a new [`WorldEngine`].
    pub(crate) admission_focus_spent: BTreeSet<String>,
    /// Cursor-first behavior settings retained from the latest accepted
    /// report (2026-10-20): switch target, fresh placement, and the
    /// stage-4-owned focus-follows-cursor/cursor-warp flags. Refreshed per
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
}

/// Platform opt-ins for [`WorldEngine`] construction: the only Engine
/// policy carried at world construction time. `fixed_size_admission` is
/// always on and is not configurable here; nothing else may be injected.
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
            state_revision: 0,
            has_last: false,
            last_request_seq: 0,
            last_digest: 0,
            last_report: None,
            last_observation_seq: 0,
            last_adapter_incarnation: None,
            last_reply: None,
            command_highwater: None,
            retained: Vec::new(),
            retained_floats: Vec::new(),
            frames: BTreeMap::new(),
            consumed_frames: BTreeMap::new(),
            learned_mins: BTreeMap::new(),
            creation_counter: 0,
            last_views: BTreeMap::new(),
            mapping: BTreeMap::new(),
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
            placement_bootstrapped: false,
            admission_focus_spent: BTreeSet::new(),
            switch_target: crate::cursor_targeting::SwitchTarget::default(),
            fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
            focus_follows_cursor: false,
            cursor_warp: false,
            last_cursor: None,
            fresh_focus: crate::fresh_focus::FreshFocusMemory::default(),
            staged: StagedReport::default(),
        })
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
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            command_result: None,
            presentation_bytes: 0,
        }
    }

    /// Cancel retained world state for a new adapter incarnation on a staged
    /// candidate: flights, ordering, desired caches, logical tokens, command
    /// high-watermark, and workspace mapping/history. Never touches native
    /// windows, retained engine sessions, the core generation, or the
    /// global revision.
    fn reset_incarnation(&mut self, adapter_incarnation: &str) {
        self.has_last = false;
        self.last_request_seq = 0;
        self.last_digest = 0;
        self.last_observation_seq = 0;
        self.last_report = None;
        self.last_reply = None;
        self.command_highwater = None;
        self.retained.clear();
        self.retained_floats.clear();
        self.frames.clear();
        self.consumed_frames.clear();
        self.learned_mins.clear();
        self.tiling.clear();
        self.members.clear();
        self.floating_scopes.clear();
        self.last_views.clear();
        self.mapping.clear();
        self.views.clear();
        self.history = ScopeHistory::new();
        self.displaced.clear();
        self.output_facts.clear();
        self.live_topology.clear();
        self.command_result = None;
        self.staged = StagedReport::default();
        self.placement_bootstrapped = false;
        self.fresh_focus = crate::fresh_focus::FreshFocusMemory::default();
        self.last_adapter_incarnation = Some(adapter_incarnation.to_owned());
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
    pub(crate) fn fresh_placement_armed_for(
        &self,
        report: &crate::world_report::WorldReport,
    ) -> bool {
        self.placement_bootstrapped && report.inventory_complete
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

    /// Retained focus-follows-cursor setting (default off). Stage-4-owned
    /// actuation; core retains the setting only.
    #[must_use]
    pub fn focus_follows_cursor(&self) -> bool {
        self.focus_follows_cursor
    }

    /// Retained cursor-warp setting (default off). Stage-4-owned
    /// actuation; core retains the setting only.
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
        // Generation binding first: a delayed reply from another incarnation
        // can never reset ordering.
        if report.core_generation != self.core_generation_str {
            return WorldOutcome::Rejected {
                kind: KIND_GENERATION,
                message: MSG_GENERATION,
            };
        }
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
        let owner = match OwnerId::parse(&report.owner) {
            Some(owner) => owner,
            None => {
                return WorldOutcome::Rejected {
                    kind: KIND_OWNER,
                    message: MSG_OWNER,
                };
            }
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
        // New adapter incarnation: fresh sequence namespace. The reset runs
        // on the staged candidate below, never on `self`: when the staged
        // report rejects, the previous incarnation's flights, ordering,
        // desired caches, logical tokens, command high-watermark, and
        // workspace mapping/history stay exactly as before. Freshness
        // classifies against the effective namespace, so a new incarnation
        // always classifies as new regardless of reused sequence numbers.
        let is_new_incarnation =
            self.last_adapter_incarnation.as_deref() != Some(report.adapter_incarnation.as_str());
        let digest = report_digest(report);
        let (has_last, last_seq, last_digest, last_obs) = if is_new_incarnation {
            (false, 0, 0, 0)
        } else {
            (
                self.has_last,
                self.last_request_seq,
                self.last_digest,
                self.last_observation_seq,
            )
        };
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
        // cannot fit. A new incarnation stages from empty retained flights
        // and creations, so the bound uses the staged (zeroed) lens for
        // those; retained sessions persist across incarnations, so their
        // lens stays exact.
        let (retained_len, created_len) = if is_new_incarnation {
            (0, 0)
        } else {
            (
                self.retained.len() + self.retained_floats.len(),
                // New logical workspaces per report are bounded by the live
                // screens (at most one trailing creation each).
                report.screens.len(),
            )
        };
        if reply_upper_bound(
            report,
            retained_len,
            self.engine.domain_keys().len(),
            created_len,
        ) > WORLD_MAX_REPLY_BYTES
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
        // Fresh report carrying an already-consumed command sequence:
        // reconcile the observation on the staged candidate but never
        // re-execute the command. The high-watermark is per (incarnation,
        // generation): a new incarnation owns a fresh watermark, so nothing
        // is consumed there.
        let effective_highwater = if is_new_incarnation {
            None
        } else {
            self.command_highwater
        };
        let consumed = match (report.command_seq, report.command.as_ref()) {
            (Some(seq), Some(_)) => effective_highwater.is_some_and(|mark| seq <= mark),
            (Some(_), None) | (None, Some(_)) => {
                return WorldOutcome::Rejected {
                    kind: KIND_SNAPSHOT,
                    message: "command-pair-invalid",
                };
            }
            (None, None) => false,
        };
        // Stage everything past this point on a clone. Exact-encoding
        // overflow (or any late failure) drops the candidate; `self` is
        // adopted only after the staged reply encodes within budget.
        // The incarnation reset lands here: retained world state (flights,
        // ordering, desired caches, logical tokens, command high-watermark,
        // workspace mapping/history) is cancelled on the candidate; native
        // windows are untouched. Owner rebinding inside the staged run
        // clears engine sessions when the owner actually changed.
        let mut staged = self.clone();
        if is_new_incarnation {
            staged.reset_incarnation(&report.adapter_incarnation);
        }
        let outcome = staged.run_accepted(report, &owner, &correlation, digest, consumed);
        match outcome {
            WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
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
        // Logical rings/views converge first (bootstrap, hotplug, tiling
        // seeds) so staging assigns facts against this report's ownership.
        self.stage_facts(report);
        self.converge_logical_pre(report);
        // View flights settle against raw echoes before staging: confirms
        // and external supersessions move core views, so homing assigns
        // against adapter truth, never a stale desire.
        self.settle_views_pre(report);
        // Stage the observation: homing plus one domain per known logical
        // workspace.
        self.build_staged_domains(report);
        // Remaining flights (window, migration, floats) settle against the
        // staged homing and echoes. Settlement classifies against the
        // pre-settlement staged homing (raw observation versus the
        // authoritative desired projection); a proven migration refusal can
        // roll back mapping/members/views/session ownership, so restage
        // afterwards: the sweep and publish below must use post-rollback
        // domains, never the pre-rollback ones.
        self.settle_rest(report);
        self.build_staged_domains(report);
        // Core-owned frame policy: fold mechanical write/readback facts into
        // retained clamp/transient memory (scoped by exact staged lifetime)
        // so this same reply publishes suppression.
        self.settle_frames(report);
        // Bind the engine owner/generation (rebinding clears sessions).
        self.engine.sync_binding(owner, &self.core_generation);
        // Ingress settings: the reported fixed-size predicate selects
        // subsequent admissions only, never reclassifying retained windows.
        // The D2/D3 capacity settings likewise select subsequent capacity
        // decisions only: no reflow, retry, or reclassification on Apply.
        self.engine
            .set_fixed_size_predicate(report.fixed_size_predicate);
        self.engine.set_anchor_fallback(report.anchor_fallback);
        self.engine.set_no_space_result(report.no_space_result);
        // Cursor-first ingress: switch target, fresh placement, the
        // stage-4-owned coupling flags, and the normalized cursor seat
        // with this observation. All apply to subsequent selections and
        // admissions only, never to retained state.
        self.switch_target = report.switch_target;
        self.fresh_placement = report.fresh_placement;
        self.focus_follows_cursor = report.focus_follows_cursor;
        self.cursor_warp = report.cursor_warp;
        self.last_cursor = report.cursor;
        self.engine.set_fresh_placement(report.fresh_placement);
        self.engine.set_fresh_cursor(report.cursor);
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
        self.converge_logical_post(report);
        self.retain_live_topology(report);
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
        // Reconcile every readable tiled domain; hold the rest explicitly.
        // Floating domains skip convergence (untouched, never held).
        // Hold reasons are per-report staging only: cleared here so a
        // recovered scope never inherits a previous hold.
        self.hold_reasons.clear();
        let mut held_scopes: BTreeSet<DomainKey> = BTreeSet::new();
        // Clone the staged domains for iteration: sweeps borrow `self`
        // mutably while reading staged homing.
        let domains = self.staged.domains.clone();
        for domain in &domains {
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
            if !report.inventory_complete
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
        for (key, window) in std::mem::take(&mut self.pending_forget) {
            self.engine.forget_minimized_slot(&key, &window);
        }
        // Fresh-admission focus intent for this report's sweep, if any.
        // Command execution mints no newcomers, so every publish below
        // shares it.
        let mut effects = self.take_admission_focus(report, &known_before, &held_scopes);
        // Command path: consumed sequences reconcile only, never re-execute.
        // A consumed command never re-executes, so its result never
        // re-emits either: consumed replies carry no result.
        self.command_result = None;
        if consumed {
            let reply = self.publish(report, true, effects, None, &held_scopes);
            return self.finish(report, digest, reply);
        }
        if let Some(command) = report.command.as_ref() {
            let command_seq = report.command_seq.expect("validated pair");
            let was_floating = self.floating_scopes.clone();
            match self.execute_command(report, command_seq, command, &held_scopes) {
                CommandApplied::Effect(effect) => {
                    self.resweep_retiled(
                        report,
                        owner,
                        correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    let result = self.command_result.take();
                    // Commands move rings/views; restage so publishing
                    // assigns facts against the committed ownership.
                    // Homing stays truthful: echoes still gate views, and
                    // in-flight members home by raw screen.
                    self.build_staged_domains(report);
                    // Admission intent first: an explicit command
                    // focuses after it, so user intent wins when they
                    // coincide.
                    effects.push(effect);
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::NoEffect => {
                    self.resweep_retiled(
                        report,
                        owner,
                        correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    // See above: restage against committed ownership.
                    self.build_staged_domains(report);
                    let result = self.command_result.take();
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::Rejected { kind, message } => {
                    return WorldOutcome::Rejected { kind, message };
                }
            }
        }
        let reply = self.publish(report, false, effects, None, &held_scopes);
        self.finish(report, digest, reply)
    }

    /// Publish one globally monotonic revision and record watermarks on the
    /// staged candidate. Every accepted report advances the revision, even a
    /// semantic no-op. The revision cannot wrap: exhaustion is refused
    /// before staging.
    fn finish(&mut self, report: &WorldReport, digest: u64, mut reply: WorldReply) -> WorldOutcome {
        // Retain the last-known focused output before membership converges,
        // so the latch reflects strictly prior retention: a stealing
        // newcomer or an empty report never overwrites pre-admission
        // context. Genuine managed focus only.
        self.note_focused_output(report);
        // Membership owner settles last on the staged candidate so the next
        // report homes through retained truth (floating release, minimized
        // and unreadable holds, view-switch stability). Included in the
        // adopted candidate, never a separate commit.
        self.reconcile_members(report);
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
        self.state_revision += 1;
        reply.state_revision = self.state_revision;
        if report.observation_seq > self.last_observation_seq || !self.has_last {
            self.last_observation_seq = report.observation_seq;
        }
        self.has_last = true;
        self.last_request_seq = report.request_seq;
        self.last_digest = digest;
        self.last_report = Some(report.clone());
        // First accepted complete inventory establishes the startup set
        // (possibly empty). Later complete reports route newcomers.
        if report.inventory_complete {
            self.placement_bootstrapped = true;
        }
        for view in &report.views {
            if view.view.is_empty() {
                continue;
            }
            self.last_views
                .insert(view.output.clone(), view.view.clone());
        }
        if reply.command_consumed {
            WorldOutcome::CommandConsumed(reply)
        } else {
            WorldOutcome::Accepted(reply)
        }
    }
}
