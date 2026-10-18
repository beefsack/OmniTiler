//! Deterministic world transition over the stateful Engine (D1-D5).
//!
//! [`WorldEngine`] retains the [`crate::engine::Engine`] (D1=A) and accepts
//! one complete [`WorldReport`] per call, reconciling every readable domain
//! through the existing [`Engine::handle`] sweep, holding unreadable scopes
//! explicitly, projecting full desired geometry with the existing projector,
//! and applying the shared trailing workspace policy in all three modes.
//! The protocol maps transport observations into this typed boundary.
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
//! outstanding, the window is shielded from convergence (stripped from the
//! source observation, presented at the target) so the committed retained
//! topology does not flap back during actuation lag. Possibly-executed
//! feedback holds the target without re-actuation. Views use the same
//! retention without any window. Focus/raise/stacking are one-shot effects.
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

use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::engine::Engine;
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::seed::EngineWindow;
use crate::session::{DomainKey, OutputDomain};
use crate::workspace_scope::ScopeHistory;
use crate::world_commands::CommandApplied;
use crate::world_publish::{current_view, migration_confirmed, observed_closed, observed_homing};
use crate::world_report::{
    AttemptOutcome, CommandResult, EffectKind, HoldReason, OneShotEffect, ScopeAuthority,
    TilingMode, TransferKind, TransferTarget, WORLD_CONTRACT_VERSION, WORLD_MAX_REPLY_BYTES,
    WORLD_MAX_REQUEST_BYTES, WorldReply, WorldReport, encode_reply_canonical, reply_estimate,
    reply_upper_bound, report_digest, report_estimate, validate_report_reason,
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

/// Retained transfer plus the source homing needed for shielding and
/// settlement. The reply projects only the [`TransferTarget`]. Policy
/// transfers (hotplug displacement/return member moves) share the exact
/// settlement/shielding path as command transfers; their target rows may
/// name live-but-not-yet-reported homings, which the adapter resolves
/// natively and holds when unresolvable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedTransfer {
    pub(crate) target: TransferTarget,
    /// True for hotplug policy member moves; false for command transfers.
    pub(crate) policy: bool,
    pub(crate) source_output: OutputId,
    pub(crate) source_workspace: String,
}

/// Retained explicit float-placement target: one outstanding authorized
/// frame per window, keyed by the stable command operation identity
/// (`op-<cmdSeq>`). Sourced from the observed frame/context at command
/// time plus the engine-committed effective rect. The window row carries
/// the operation id explicitly; the adapter writes geometry ONLY under
/// that token, exactly once. Settles on observation confirmation (observed
/// frame equals the target), proven refusal/supersession feedback, explicit
/// close, a newer float intent for the same window, or generation reset.
/// An observed frame that matches neither the source nor the target proves
/// newer native input and supersedes without a fight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedFloat {
    pub(crate) operation_id: String,
    pub(crate) window: WindowId,
    pub(crate) target: crate::geometry::Rect,
    pub(crate) source: Option<crate::geometry::Rect>,
    pub(crate) output: OutputId,
    pub(crate) workspace: String,
}

/// Pending logical workspace creation: at most one per output, stable
/// across snapshots until binding or cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingCreation {
    pub(crate) logical_id: String,
    pub(crate) output: OutputId,
}

/// Pending numbered-send intent against an unbound logical workspace: the
/// trailing-create path of [`crate::world_report::WorldCommand::SendNumbered`]
/// minted `logical_id` and the engine send starts once the binding is
/// reported. Stable operation identity, no view target, never the token as
/// a native id. Drained in report order once bound; dropped when the window
/// closes or moves elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingSend {
    pub(crate) operation_id: String,
    pub(crate) logical_id: String,
    pub(crate) output: OutputId,
    pub(crate) window: WindowId,
    pub(crate) follow: bool,
    pub(crate) source_output: OutputId,
    pub(crate) source_workspace: String,
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
    last_reply: Option<WorldReply>,
    /// Monotonic command high-watermark per (incarnation, generation):
    /// any `command_seq` at or below it is already consumed, so eviction
    /// can never resurrect a sequence. Commandless reports never move it.
    pub(crate) command_highwater: Option<u64>,
    pub(crate) retained: Vec<RetainedTransfer>,
    /// Outstanding explicit float-placement targets, at most one per window.
    /// Bounded by the window inventory itself.
    pub(crate) retained_floats: Vec<RetainedFloat>,
    pub(crate) pending_creations: Vec<PendingCreation>,
    pub(crate) creation_counter: u64,
    last_views: BTreeMap<OutputId, String>,
    /// Planner-owned scoped workspace assignment (D2): per-output lists
    /// for local/global-unique, rebuilt every accepted report through
    /// [`rebuild_local_mapping`]/[`rebuild_global_mapping`].
    pub(crate) mapping: BTreeMap<String, Vec<String>>,
    /// Planner-owned shared order (D2): the live native order itself.
    pub(crate) shared_order: Vec<String>,
    /// Planner-owned previous-view history (D2): per-output entries plus
    /// one shared entry, observed from verified views only.
    pub(crate) history: ScopeHistory,
    /// Planner-owned hotplug displacement (D2, local/global-unique only):
    /// removed origins mapped to their preserved workspace units plus the
    /// hosting survivor. Shared mode never displaces.
    pub(crate) displaced: BTreeMap<String, crate::workspace_scope::DisplacedEntry>,
    /// Last observed native output facts by stable output key (geometry
    /// plus active-screen primary). Updated for every live output on
    /// every accepted report; removed origins keep their disconnect-time
    /// facts while a displacement record needs placement. Never a window
    /// frame proxy.
    pub(crate) output_facts: BTreeMap<String, crate::workspace_scope::OutputFacts>,
    /// Core-owned live output topology: validated FULL native rectangles by
    /// stable output id, retained from the last accepted report claiming
    /// complete topology in EVERY workspace mode (never tied to hotplug
    /// displacement: shared mode retains exactly like local modes).
    /// Directional world commands resolve against this map through the
    /// shared strict unique-reciprocal helper, never the carried adapter
    /// adjacency. A report that is incomplete, partial, or carries a
    /// missing/invalid/duplicate FULL rect clears the map, so
    /// topology-dependent crossing, migration, and remap fence narrowly
    /// until the next complete report instead of actuating on stale edges.
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
    /// Floating scopes for the report currently being staged: derived from
    /// [`WorldEngine::tiling`] at the start of every accepted run, read by
    /// the command gates and publishing.
    pub(crate) floating_scopes: BTreeSet<DomainKey>,
    /// Pending numbered-send intents against unbound logical workspaces.
    pub(crate) pending_sends: Vec<PendingSend>,
    /// Per-scope sweep failure classification for the report currently
    /// being staged: [`WorldEngine::classify_sweep_hold`] reasons for scope
    /// keys in the staged held set. Cleared at the start of every accepted
    /// run and published by [`WorldEngine::publish`]; never survives across
    /// reports, incarnations, or generations.
    pub(crate) hold_reasons: BTreeMap<DomainKey, HoldReason>,
    /// Minimize slots to forget after the sweep (proven-refusal cleanup:
    /// converge may re-store the slot during the unshielded sweep, so the
    /// forget runs after, never before).
    pending_forget: Vec<(DomainKey, WindowId)>,
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
    admission_focus_spent: BTreeSet<String>,
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
            pending_creations: Vec::new(),
            creation_counter: 0,
            last_views: BTreeMap::new(),
            mapping: BTreeMap::new(),
            shared_order: Vec::new(),
            history: ScopeHistory::new(),
            displaced: BTreeMap::new(),
            output_facts: BTreeMap::new(),
            live_topology: BTreeMap::new(),
            tiling: BTreeMap::new(),
            floating_scopes: BTreeSet::new(),
            pending_sends: Vec::new(),
            hold_reasons: BTreeMap::new(),
            pending_forget: Vec::new(),
            command_result: None,
            admission_focus_spent: BTreeSet::new(),
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
        self.pending_creations.clear();
        self.pending_sends.clear();
        self.tiling.clear();
        self.floating_scopes.clear();
        self.last_views.clear();
        self.mapping.clear();
        self.shared_order.clear();
        self.history = ScopeHistory::new();
        self.displaced.clear();
        self.output_facts.clear();
        self.live_topology.clear();
        self.command_result = None;
        self.last_adapter_incarnation = Some(adapter_incarnation.to_owned());
    }

    /// Borrow the retained engine (read-only inspection for tests/callers).
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Outstanding retained transfer/view targets.
    #[must_use]
    pub fn retained_transfers(&self) -> Vec<TransferTarget> {
        self.retained.iter().map(|r| r.target.clone()).collect()
    }

    /// Latest published global revision.
    #[must_use]
    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Retain (or fence) the core-owned live output topology from one
    /// accepted report, before any command planning on that same report.
    /// A report claiming complete inventory with every topology entry
    /// complete and carrying a valid FULL rectangle replaces the map;
    /// anything else (partial inventory, incomplete entries, missing or
    /// invalid rectangles, duplicate output ids) clears it, so
    /// topology-dependent operations fence until the next complete
    /// report. Never infers geometry from work areas or window frames.
    pub(crate) fn retain_live_topology(&mut self, report: &WorldReport) {
        let mut fresh: BTreeMap<String, Rect> = BTreeMap::new();
        let mut complete = report.inventory_complete && !report.topology.is_empty();
        for topology in &report.topology {
            if !topology.complete {
                complete = false;
                break;
            }
            let Some(rect) = topology.geometry else {
                complete = false;
                break;
            };
            if topology.output.0.is_empty()
                || !crate::bounds::valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
                || fresh.insert(topology.output.0.clone(), rect).is_some()
            {
                complete = false;
                break;
            }
        }
        if complete {
            self.live_topology = fresh;
        } else {
            self.live_topology.clear();
        }
    }

    /// Live FULL rectangles as shared selector entries, in stable output
    /// order. Empty when no complete topology is retained.
    pub(crate) fn live_topology_entries(&self) -> Vec<crate::output_selection::OutputEntry> {
        self.live_topology
            .iter()
            .map(|(id, rect)| crate::output_selection::OutputEntry {
                id: OutputId(id.clone()),
                rect: *rect,
            })
            .collect()
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
        let (retained_len, pending_len) = if is_new_incarnation {
            (0, 0)
        } else {
            (
                self.retained.len() + self.retained_floats.len(),
                self.pending_creations.len(),
            )
        };
        if reply_upper_bound(
            report,
            retained_len,
            self.engine.domain_keys().len(),
            pending_len,
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

    /// Run one accepted report on the staged candidate: settle retained
    /// state, sweep every readable domain, execute at most one command, and
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
        // Observation index for shielding/settlement, built from the raw
        // report before any mutation.
        let mut by_id: BTreeMap<&WindowId, &EngineWindow> = BTreeMap::new();
        for domain in &report.domains {
            for window in &domain.windows {
                by_id.insert(&window.window, window);
            }
        }
        self.settle_retained(report);
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
        // Seed planner-owned per-workspace tiling for newly observed
        // natives from the reported saved default: startup seeds all live
        // workspaces; live default changes seed only subsequently observed
        // ones. Keys already present (manual toggles, earlier seeds) never
        // reseed, and keys for no-longer-live natives (outside retained
        // pinned/displaced bindings) prune so a reused native id reseeds
        // instead of inheriting a stale mode.
        self.seed_workspace_tiling(report);
        // Floating workspaces skip convergence entirely: no session is
        // created or converged there, and no frame writes publish for them.
        self.floating_scopes = report
            .domains
            .iter()
            .map(|domain| domain.domain.key())
            .filter(|key| {
                self.tiling
                    .get(key)
                    .is_some_and(|mode| *mode == TilingMode::Floating)
            })
            .collect();
        // Planner-owned workspace mapping/history converge before the sweep
        // so lifecycle rows and selection rings see this report's topology.
        self.converge_workspace_policy(report);
        // Core-owned live output topology retains before the sweep so the
        // same report's commands resolve against its own geometry.
        self.retain_live_topology(report);
        // Retained membership before the sweep, per swept domain: the
        // admission-focus pass diffs this against converged state to find
        // genuinely new lifetimes. Only domains with a retained session
        // count; fresh seeds (startup, restart, disable/enable
        // re-adoption) never mint admission intents.
        let mut known_before: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for domain in &report.domains {
            if !domain.readable {
                continue;
            }
            let key = domain.domain.key();
            if self.floating_scopes.contains(&key) {
                continue;
            }
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            let mut known = BTreeSet::new();
            for link in session.snapshot().windows {
                if link.output == key.output && link.workspace == key.workspace {
                    known.insert(link.window.0.clone());
                }
            }
            for observed in session.exception_observed() {
                if observed.output == key.output && observed.workspace == key.workspace {
                    known.insert(observed.window.0.clone());
                }
            }
            for (window, output, workspace) in session.minimized_entries() {
                if output == key.output && workspace == key.workspace {
                    known.insert(window.0.clone());
                }
            }
            known_before.insert(key, known);
        }
        // Reconcile every readable tiled domain; hold the rest explicitly.
        // Floating domains skip convergence (untouched, never held).
        // Hold reasons are per-report staging only: cleared here so a
        // recovered scope never inherits a previous hold.
        self.hold_reasons.clear();
        let mut held_scopes: BTreeSet<DomainKey> = BTreeSet::new();
        for domain in &report.domains {
            if !domain.readable {
                continue;
            }
            let key = domain.domain.key();
            if self.floating_scopes.contains(&key) {
                continue;
            }
            self.sweep_domain(
                domain,
                &by_id,
                owner,
                correlation,
                &mut held_scopes,
                report.gesture_barrier,
            );
        }
        for (key, window) in std::mem::take(&mut self.pending_forget) {
            self.engine.forget_minimized_slot(&key, &window);
        }
        // Fresh-admission focus intent for this report's sweep, if any.
        // Computed before pending sends and command execution (neither
        // mints newcomers) so every publish below shares it.
        let admission_effects = self.take_admission_focus(report, &known_before, &held_scopes);
        // Pending numbered sends against newly bound workspaces start now:
        // the binding resolved the logical token to a native id, so the
        // engine send runs against the observed target domain. Unbound
        // intents persist without any view switch or token-as-native write.
        self.drain_pending_sends(report, &by_id, &held_scopes);
        // Command path: consumed sequences reconcile only, never re-execute.
        // A consumed command never re-executes, so its result never
        // re-emits either: consumed replies carry no result.
        self.command_result = None;
        if consumed {
            let reply = self.publish(report, true, admission_effects, None, &held_scopes);
            return self.finish(report, digest, reply);
        }
        if let Some(command) = report.command.as_ref() {
            let command_seq = report.command_seq.expect("validated pair");
            let was_floating = self.floating_scopes.clone();
            match self.execute_command(report, command_seq, command, &by_id, &held_scopes) {
                CommandApplied::Effect(effect) => {
                    self.resweep_retiled(
                        report,
                        &by_id,
                        owner,
                        correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    let result = self.command_result.take();
                    // Admission intent first: an explicit command focuses
                    // after it, so user intent wins when both coincide.
                    let mut effects = admission_effects;
                    effects.push(effect);
                    let reply = self.publish(report, false, effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::NoEffect => {
                    self.resweep_retiled(
                        report,
                        &by_id,
                        owner,
                        correlation,
                        &mut held_scopes,
                        &was_floating,
                    );
                    let result = self.command_result.take();
                    let reply =
                        self.publish(report, false, admission_effects, result, &held_scopes);
                    return self.finish(report, digest, reply);
                }
                CommandApplied::Rejected { kind, message } => {
                    return WorldOutcome::Rejected { kind, message };
                }
            }
        }
        let reply = self.publish(report, false, admission_effects, None, &held_scopes);
        self.finish(report, digest, reply)
    }

    /// Publish one globally monotonic revision and record watermarks on the
    /// staged candidate. Every accepted report advances the revision, even a
    /// semantic no-op. The revision cannot wrap: exhaustion is refused
    /// before staging.
    fn finish(&mut self, report: &WorldReport, digest: u64, mut reply: WorldReply) -> WorldOutcome {
        self.state_revision += 1;
        reply.state_revision = self.state_revision;
        if report.observation_seq > self.last_observation_seq || !self.has_last {
            self.last_observation_seq = report.observation_seq;
        }
        self.has_last = true;
        self.last_request_seq = report.request_seq;
        self.last_digest = digest;
        self.last_report = Some(report.clone());
        for topology in &report.topology {
            self.last_views
                .insert(topology.output.clone(), topology.current_native.clone());
        }
        if reply.command_consumed {
            WorldOutcome::CommandConsumed(reply)
        } else {
            WorldOutcome::Accepted(reply)
        }
    }

    /// Settle outstanding transfers/views against the raw observation and
    /// explicit attempt feedback before the sweep.
    fn settle_retained(&mut self, report: &WorldReport) {
        let mut keep: Vec<RetainedTransfer> = Vec::new();
        for retained in std::mem::take(&mut self.retained) {
            let feedback = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.target.operation_id);
            let refused = matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::RefusedProven)
            );
            if refused && let TransferKind::Window(window) = &retained.target.kind {
                // Deferred until after the sweep: the unshielded sweep may
                // re-store the slot while converging back to observed.
                self.pending_forget.push((
                    DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    },
                    window.clone(),
                ));
            }
            match &retained.target.kind {
                TransferKind::Window(window) => {
                    let homing = observed_homing(report, window);
                    let closed = observed_closed(report, window);
                    let source_key = DomainKey {
                        output: retained.source_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.source_workspace.clone(),
                        ),
                    };
                    let target_key = DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    };
                    if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
                    ) {
                        // Dropped above; refusal slot cleanup already queued.
                    } else if homing.as_ref() == Some(&target_key) {
                        // Confirmed by observation.
                    } else if closed {
                        // Explicit close settles; the sweep forgets the slot.
                    } else if let Some(key) = homing {
                        if key != source_key {
                            // Newer external input moved it elsewhere: newer
                            // input wins.
                        } else if matches!(
                            feedback.map(|f| f.outcome),
                            Some(AttemptOutcome::PossiblyExecuted)
                        ) {
                            let mut held = retained;
                            held.target.held = true;
                            keep.push(held);
                        } else {
                            keep.push(retained);
                        }
                    } else if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::PossiblyExecuted)
                    ) {
                        let mut held = retained;
                        held.target.held = true;
                        keep.push(held);
                    } else {
                        // Absent everywhere and not closed: never infer a
                        // close; retain across the gap.
                        keep.push(retained);
                    }
                }
                TransferKind::View => {
                    let current = current_view(report, &retained.target.target_output);
                    if current == Some(retained.target.target_workspace.as_str()) {
                        // View confirmed.
                    } else if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
                    ) {
                        // Settle to observed.
                    } else {
                        let last = self
                            .last_views
                            .get(&retained.target.target_output)
                            .map(String::as_str);
                        if current != last {
                            // External view input supersedes.
                        } else {
                            keep.push(retained);
                        }
                    }
                }
                TransferKind::Migration => {
                    if migration_confirmed(report, &retained.source_output, &retained.target) {
                        // Topology shows the workspace under the target
                        // scope and no longer under the source.
                    } else if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
                    ) {
                        // Settle to observed; the mapping reconverges below.
                    } else {
                        keep.push(retained);
                    }
                }
            }
        }
        self.retained = keep;
        self.settle_floats(report);
    }

    /// Settle outstanding explicit float-placement targets against the raw
    /// observation and explicit property feedback before the sweep. Accepted
    /// (`Applied`) feedback is recorded by the adapter's read-back proof;
    /// here only the terminal outcomes and observation proofs settle:
    /// confirmation (observed frame equals the target), proven refusal or
    /// external supersession feedback, explicit close, or a native frame
    /// that matches neither the command-time source nor the target (newer
    /// input wins, never fought). Possibly-executed holds the target
    /// without re-actuation; absence without a close retains across the gap.
    fn settle_floats(&mut self, report: &WorldReport) {
        let mut observed: BTreeMap<&WindowId, &crate::seed::EngineWindow> = BTreeMap::new();
        for domain in &report.domains {
            for window in &domain.windows {
                observed.insert(&window.window, window);
            }
        }
        let mut keep: Vec<RetainedFloat> = Vec::new();
        for retained in std::mem::take(&mut self.retained_floats) {
            let feedback = report
                .feedback
                .iter()
                .find(|f| f.operation_id == retained.operation_id);
            if matches!(
                feedback.map(|f| f.outcome),
                Some(AttemptOutcome::RefusedProven | AttemptOutcome::SupersededExternal)
            ) {
                continue;
            }
            if observed_closed(report, &retained.window) {
                continue;
            }
            match observed.get(&retained.window) {
                None => {
                    // Absent everywhere and not closed: never infer a
                    // close; retain across the gap.
                    keep.push(retained);
                }
                Some(current) => {
                    if current.rect == retained.target {
                        // Confirmed by observation.
                    } else if matches!(
                        feedback.map(|f| f.outcome),
                        Some(AttemptOutcome::PossiblyExecuted)
                    ) {
                        // Ambiguous actuation (a clamp/partial setter can
                        // leave a frame matching neither source nor
                        // target): hold the target without re-actuation
                        // until confirmation, proven refusal, explicit
                        // external supersession, or close — exactly like a
                        // possibly-executed transfer. Never mistake the
                        // ambiguity for newer input.
                        keep.push(retained);
                    } else if retained.source.is_some_and(|source| current.rect != source) {
                        // Native frame matches neither the command-time
                        // source nor the target: newer external input
                        // (manual move/resize) wins; the stale target drops
                        // rather than fighting it.
                    } else {
                        keep.push(retained);
                    }
                }
            }
        }
        self.retained_floats = keep;
    }

    /// At-most-once fresh-admission focus intent (D4): the genuinely new
    /// lifetime admitted by this report's sweep, tiled or floated alike,
    /// carries one Focus effect even when the incoming native report still
    /// focuses the previous tile. The adapter owns native activation
    /// through the existing one-shot machinery (`issued_revision`
    /// watermark, spent-before-dispatch, lost/stale drops); no persistent
    /// focus desired exists.
    ///
    /// At most one candidate per report: the last newcomer in report order
    /// (new birth order; the D2 fallback anchor is retained state, never a
    /// candidate). Fresh seeds emit nothing (no retained session means
    /// startup, restart, or disable/enable re-adoption, where the observed
    /// native focus stays authoritative), except the first fresh map into
    /// a previously reported empty readable controlled tiled domain: the
    /// prior accepted report proves the empty roster there, so the newly
    /// arrived lifetime focuses once. A workspace with no prior roster, a
    /// recovered unreadable scope, or a prior held/untouched scope never
    /// infers birth by absence. Re-observation, shrink evictions
    /// (D7 victims are retained), closes, gap/bounds edits, settings Apply,
    /// explicit tile retries, held scopes, and floating scopes emit
    /// nothing: only a retained domain gaining an unknown lifetime.
    /// A mid-gesture newcomer spends without emitting (never replayed),
    /// and an unexplained fit-excluded newcomer (game/overlay with no
    /// floating, fullscreen, or maximized origin) never takes automatic
    /// activation; normalized ordinary floats stay eligible.
    /// D01 native activation ownership: a newcomer admitted behind a
    /// fullscreen/maximized sibling never takes automatic activation
    /// either (the host owns activation away from a protected overlay);
    /// the suppressed intent spends without emitting, never retried.
    /// Windows under an outstanding transfer never count (cross-domain
    /// arrivals settle through the transfer, never a focus steal). Spent
    /// intents prune on close, so a reused id focuses again as a fresh
    /// newcomer. The `af-<window>` id marks admission intent so the adapter
    /// applies its float-tolerant guard only there; diagnostic float
    /// origin is never consulted.
    fn take_admission_focus(
        &mut self,
        report: &WorldReport,
        known_before: &BTreeMap<DomainKey, BTreeSet<String>>,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> Vec<OneShotEffect> {
        let mut known_now: BTreeSet<String> = BTreeSet::new();
        let mut current: BTreeMap<DomainKey, BTreeSet<String>> = BTreeMap::new();
        for key in self.engine.domain_keys() {
            let Some(session) = self.engine.session(&key) else {
                continue;
            };
            let mut known = BTreeSet::new();
            for link in session.snapshot().windows {
                known.insert(link.window.0.clone());
                known_now.insert(link.window.0.clone());
            }
            for observed in session.exception_observed() {
                known.insert(observed.window.0.clone());
                known_now.insert(observed.window.0.clone());
            }
            for (window, _, _) in session.minimized_entries() {
                known_now.insert(window.0.clone());
            }
            current.insert(key, known);
        }
        self.admission_focus_spent
            .retain(|id| known_now.contains(id));
        let transferred: BTreeSet<&str> = self
            .retained
            .iter()
            .filter_map(|retained| match &retained.target.kind {
                TransferKind::Window(window) => Some(window.0.as_str()),
                TransferKind::View | TransferKind::Migration => None,
            })
            .collect();
        let mut newcomer: Option<(WindowId, DomainKey)> = None;
        for domain in &report.domains {
            let key = domain.domain.key();
            if !domain.readable || self.floating_scopes.contains(&key) || held_scopes.contains(&key)
            {
                continue;
            }
            // A domain with no retained session is a fresh seed (startup,
            // restart, disable/enable re-adoption) with one exception: a
            // previously reported empty readable controlled tiled domain
            // proves its empty roster through the prior accepted report,
            // so the first fresh map there still counts as admission.
            let known = known_before.get(&key);
            if known.is_none() && !self.prior_empty_controlled(&key) {
                continue;
            }
            let empty = BTreeSet::new();
            let before: &BTreeSet<String> = known.unwrap_or(&empty);
            let converged = current.get(&key);
            for window in &domain.windows {
                let id = window.window.0.as_str();
                if before.contains(id) || transferred.contains(id) {
                    continue;
                }
                // Safety priority: an unexplained fit-excluded newcomer
                // (game/overlay with no floating, fullscreen, or
                // maximized origin) never takes automatic activation,
                // mirroring the send path's no-automatic-permission
                // refusal. A normalized ordinary float (floating with an
                // origin) stays eligible, capacity floats alike; only
                // the unsafe shape is skipped, never the latest
                // eligible newcomer.
                if window.fit_excluded
                    && !window.floating
                    && !window.fullscreen
                    && !window.maximized
                {
                    continue;
                }
                if converged.is_some_and(|known| known.contains(id)) {
                    newcomer = Some((window.window.clone(), key.clone()));
                }
            }
        }
        let Some((target, target_key)) = newcomer else {
            return Vec::new();
        };
        // D01 native activation ownership: never force activation away
        // from a protected overlay. A fullscreen/maximized sibling in the
        // newcomer's own domain owns host activation, denial, and
        // explicit switching; the automatic intent spends without
        // emitting and is never retried. Explicit user focus/send
        // shortcuts ride commands, never this path.
        if report.domains.iter().any(|domain| {
            domain.domain.key() == target_key
                && domain.windows.iter().any(|window| {
                    window.window != target && (window.fullscreen || window.maximized)
                })
        }) {
            self.admission_focus_spent.insert(target.0.clone());
            return Vec::new();
        }
        // The gesture barrier spends honestly: a newcomer arriving
        // mid-gesture drops its activation here and never replays it on
        // a later report (safety over liveness; the at-most-once
        // watermark still records it as spent).
        if report.gesture_barrier {
            self.admission_focus_spent.insert(target.0.clone());
            return Vec::new();
        }
        if !self.admission_focus_spent.insert(target.0.clone()) {
            return Vec::new();
        }
        vec![OneShotEffect {
            id: format!("af-{}", target.0),
            kind: EffectKind::Focus,
            target,
            issued_revision: self.state_revision + 1,
        }]
    }

    /// Prior-report proof that `key` was a readable, controlled, tiled,
    /// empty domain: the last accepted report carried the domain readable
    /// with no windows, and the last accepted reply published it
    /// controlled. Covers the first fresh map into a previously reported
    /// empty domain, which carries no retained session to diff against.
    /// Anything else (no prior report, prior domain absent/unreadable,
    /// prior roster non-empty, prior held/untouched scope) is not birth
    /// evidence: truly unknown rosters never infer admission by absence.
    /// Restart/incarnation resets clear the retained reports, so fresh
    /// adoption stays quiet; disable/enable adoption replays a prior
    /// untouched scope, never a controlled empty one.
    fn prior_empty_controlled(&self, key: &DomainKey) -> bool {
        let Some(last) = self.last_report.as_ref() else {
            return false;
        };
        let Some(prior) = last
            .domains
            .iter()
            .find(|domain| domain.domain.key() == *key)
        else {
            return false;
        };
        if !prior.readable || !prior.windows.is_empty() {
            return false;
        }
        let Some(reply) = self.last_reply.as_ref() else {
            return false;
        };
        let scope = format!("{}/{}", key.output.0, key.workspace.0);
        reply
            .scopes
            .iter()
            .any(|status| status.scope == scope && status.authority == ScopeAuthority::Controlled)
    }

    /// Reconcile one readable domain through the retained engine sweep.
    /// Failures hold the scope explicitly; floating callers never reach
    /// here. A native gesture (`gesture_barrier`) without an explicit
    /// Session drag defers geometric capacity repair (D7): when the
    /// carried bounds/gaps changed, membership still converges at the
    /// retained geometry, but gap adoption and victim repair wait for the
    /// first report after the gesture clears. The scope holds as
    /// [`HoldReason::GestureBarrier`] with topology/order retained, so no
    /// float intent persists mid-gesture and no classification runs.
    /// Unchanged shapes converge exactly as without the barrier.
    pub(crate) fn sweep_domain(
        &mut self,
        domain: &crate::world_report::DomainReport,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
        gesture_barrier: bool,
    ) {
        let key = domain.domain.key();
        // Legacy parity: the Engine retains the outer-inset work area.
        // Inset exhaustion holds geometrically (capacity B), never seeds
        // an empty domain or drops retained topology. This protected hold
        // stands even mid-gesture: the barrier defers repair, never
        // exhaustion.
        let Some(inset) = inset_world_domain(&domain.domain, domain.outer_gap) else {
            self.hold_reasons
                .insert(key.clone(), HoldReason::ConstraintClamp);
            held_scopes.insert(key);
            return;
        };
        if gesture_barrier
            && self.defer_gesture_repair(domain, &inset, by_id, owner, correlation, held_scopes)
        {
            return;
        }
        let windows = self.shielded_windows(&key, &domain.windows, by_id);
        let accepted = self
            .engine
            .session(&key)
            .map(|s| (s.accepted_revision(), s.accepted_fingerprint()));
        let (revision, fingerprint) = accepted.unwrap_or((0, 0));
        // Gap/bounds changes adopt through the topology-preserving
        // update-gaps path (legacy parity); unchanged shapes reconcile.
        // Unknown sessions reconcile (fresh seed), never update-gaps
        // (which refuses unknown domains so the fresh route seeds them).
        let command = match self.engine.session(&key) {
            Some(session) => {
                let retained_gap = session
                    .domains()
                    .iter()
                    .find(|d| d.key() == key)
                    .map(|d| (d.bounds, d.gap));
                let retained_outer = self.engine.outer_gap(&key);
                match (retained_gap, retained_outer) {
                    (Some((bounds, gap)), Some(outer))
                        if bounds == inset.bounds
                            && gap == inset.gap
                            && outer == domain.outer_gap =>
                    {
                        CoreCommand::Reconcile
                    }
                    (Some(_), Some(_)) => CoreCommand::UpdateGaps,
                    _ => CoreCommand::Reconcile,
                }
            }
            None => CoreCommand::Reconcile,
        };
        let event = CoreEvent {
            owner: owner.clone(),
            generation: self.core_generation.clone(),
            correlation: correlation.clone(),
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&inset),
            domain_key: key.clone(),
            outer_gap: domain.outer_gap,
            focused_window: domain.focused.clone(),
            windows,
            closed_windows: domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_)
            | CoreReply::Tiled(_)
            | CoreReply::Released
            | CoreReply::NoGroup { .. } => {}
            refused => {
                // Single-policy classification (capacity B): the staged
                // reason publishes with the held scope and its rows.
                self.hold_reasons
                    .insert(key.clone(), Self::classify_sweep_hold(&refused));
                held_scopes.insert(key);
            }
        }
    }

    /// Gesture repair deferral for one domain (D7): true when the barrier
    /// owned this domain (the caller returns without sweeping further).
    /// Convergence-only at the retained bounds/gaps when those changed:
    /// membership still reconciles through the ordinary [`Engine::handle`]
    /// path at retained geometry (a `Reconcile` there never adopts gaps or
    /// repairs), then the scope holds as [`HoldReason::GestureBarrier`]
    /// with retained topology, order, focus, and float state untouched.
    /// Unchanged shapes return false so the ordinary sweep converges them
    /// fully (the barrier only withholds frame writes at publish). Fresh
    /// seeds (no retained session) also return false. A failed
    /// convergence keeps its classified hold: the barrier postpones
    /// repair, never real failures.
    fn defer_gesture_repair(
        &mut self,
        domain: &crate::world_report::DomainReport,
        inset: &OutputDomain,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
    ) -> bool {
        let key = domain.domain.key();
        let retained = self.engine.session(&key).and_then(|session| {
            let geometry = session
                .domains()
                .iter()
                .find(|d| d.key() == key)
                .map(|d| (d.bounds, d.gap))?;
            let outer = self.engine.outer_gap(&key)?;
            Some((
                geometry.0,
                geometry.1,
                outer,
                session.accepted_revision(),
                session.accepted_fingerprint(),
            ))
        });
        let Some((bounds, gap, outer, revision, fingerprint)) = retained else {
            return false;
        };
        if bounds == inset.bounds && gap == inset.gap && outer == domain.outer_gap {
            return false;
        }
        let windows = self.shielded_windows(&key, &domain.windows, by_id);
        let retained_domain = OutputDomain {
            id: key.output.clone(),
            workspace: key.workspace.clone(),
            bounds,
            gap,
            adjacent: BTreeMap::new(),
        };
        let event = CoreEvent {
            owner: owner.clone(),
            generation: self.core_generation.clone(),
            correlation: correlation.clone(),
            revision,
            fingerprint,
            domain: Engine::canonical_component_domain(&retained_domain),
            domain_key: key.clone(),
            outer_gap: outer,
            focused_window: domain.focused.clone(),
            windows,
            closed_windows: domain.closed.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: Vec::new(),
            command: CoreCommand::Reconcile,
        };
        match self.engine.handle(&event) {
            CoreReply::Projection(_)
            | CoreReply::Tiled(_)
            | CoreReply::Released
            | CoreReply::NoGroup { .. } => {}
            refused => {
                // Single-policy classification (capacity B): the staged
                // reason publishes with the held scope and its rows.
                self.hold_reasons
                    .insert(key.clone(), Self::classify_sweep_hold(&refused));
                held_scopes.insert(key);
                return true;
            }
        }
        self.hold_reasons
            .insert(key.clone(), HoldReason::GestureBarrier);
        held_scopes.insert(key);
        true
    }

    /// Re-sweep domains a command just re-tiled: the pre-command floating
    /// set skipped them, so without this pass a floating-to-tiled flip
    /// would publish no session rows until the next report. Only domains
    /// floating before the command and tiled after sweep here, so command
    /// releases and ordinary failures are never resurrected.
    pub(crate) fn resweep_retiled(
        &mut self,
        report: &WorldReport,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        owner: &OwnerId,
        correlation: &CorrelationId,
        held_scopes: &mut BTreeSet<DomainKey>,
        was_floating: &BTreeSet<DomainKey>,
    ) {
        let gesture_barrier = report.gesture_barrier;
        for domain in &report.domains {
            if !domain.readable {
                continue;
            }
            let key = domain.domain.key();
            if !was_floating.contains(&key)
                || self.floating_scopes.contains(&key)
                || held_scopes.contains(&key)
                || self.engine.contains(&key)
            {
                continue;
            }
            self.sweep_domain(
                domain,
                by_id,
                owner,
                correlation,
                held_scopes,
                gesture_barrier,
            );
        }
    }

    /// Start pending numbered sends whose logical token just bound to a
    /// native id. Unbound intents persist across the gap (never inferred
    /// closed, never view-switching, never the token as a native id);
    /// explicit closes and external moves settle them. Deterministic engine
    /// refusals settle; only unobservable targets (unbound, unreadable, or
    /// held) keep pending.
    pub(crate) fn drain_pending_sends(
        &mut self,
        report: &WorldReport,
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
        held_scopes: &BTreeSet<DomainKey>,
    ) {
        let pending = std::mem::take(&mut self.pending_sends);
        for intent in pending {
            let Some(native) = report
                .bindings
                .iter()
                .find(|binding| {
                    binding.logical_id == intent.logical_id && binding.output == intent.output
                })
                .map(|binding| binding.native_id.clone())
            else {
                self.pending_sends.push(intent);
                continue;
            };
            if observed_closed(report, &intent.window) {
                continue;
            }
            let source_key = DomainKey {
                output: intent.source_output.clone(),
                workspace: WorkspaceId(intent.source_workspace.clone()),
            };
            match observed_homing(report, &intent.window) {
                None => {
                    // Absent everywhere and not closed: retain across gap.
                    self.pending_sends.push(intent);
                    continue;
                }
                Some(key) if key != source_key => continue,
                Some(_) => {}
            }
            if held_scopes.contains(&source_key) {
                self.pending_sends.push(intent);
                continue;
            }
            if self.retained.iter().any(|r| match &r.target.kind {
                TransferKind::Window(w) => w == &intent.window,
                TransferKind::View | TransferKind::Migration => false,
            }) {
                continue;
            }
            let target_key = DomainKey {
                output: intent.output.clone(),
                workspace: WorkspaceId(native.clone()),
            };
            if !report
                .domains
                .iter()
                .any(|d| d.readable && d.domain.key() == target_key)
                || held_scopes.contains(&target_key)
            {
                self.pending_sends.push(intent);
                continue;
            }
            let window = intent.window.clone();
            if let Ok(retained) = self.execute_send(
                report,
                &intent.operation_id,
                &window,
                &intent.output,
                &native,
                intent.follow,
                by_id,
                held_scopes,
            ) {
                self.retained.push(retained);
            }
        }
    }

    /// Observation presented to convergence for one domain: outstanding
    /// transfer windows are stripped from their source and presented at
    /// their target so committed retained topology survives actuation lag.
    /// Windows with an outstanding explicit float-placement target are
    /// presented as floating so convergence retains the committed
    /// exception (and its exact geometry) instead of re-admitting them
    /// tiled from a stale non-floating observation: a lost placement
    /// reply must not dissolve the target it never delivered.
    pub(crate) fn shielded_windows(
        &self,
        key: &DomainKey,
        windows: &[EngineWindow],
        by_id: &BTreeMap<&WindowId, &EngineWindow>,
    ) -> Vec<EngineWindow> {
        let mut out: Vec<EngineWindow> = windows
            .iter()
            .filter(|window| {
                !self.retained.iter().any(|r| match &r.target.kind {
                    TransferKind::Window(w) => {
                        w.0 == window.window.0
                            && r.source_output == key.output
                            && r.source_workspace == key.workspace.0
                    }
                    TransferKind::View | TransferKind::Migration => false,
                })
            })
            .cloned()
            .collect();
        for entry in &mut out {
            if self
                .retained_floats
                .iter()
                .any(|held| held.window == entry.window)
            {
                entry.floating = true;
            }
        }
        for retained in &self.retained {
            if let TransferKind::Window(window) = &retained.target.kind
                && retained.target.target_output == key.output
                && retained.target.target_workspace == key.workspace.0
                && !out.iter().any(|entry| entry.window == *window)
                && let Some(source) = by_id.get(window)
            {
                let mut injected = (*source).clone();
                injected.output = key.output.clone();
                injected.workspace = key.workspace.clone();
                out.push(injected);
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "world_engine_tests.rs"]
mod tests;
