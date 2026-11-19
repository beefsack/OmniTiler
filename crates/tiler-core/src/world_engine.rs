//! Deterministic world transition over the stateful Engine (D1-D5).
//!
//! [`WorldEngine`] retains the [`crate::engine::Engine`] (D1=A) and owns the
//! logical workspace model outright: stable core workspace ids, per-screen
//! rings, views, members, history, lifecycle, and trailing. It accepts one
//! complete [`WorldReportV4`](crate::world_report::WorldReportV4) per call — screens (FULL rectangles plus work
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
//! reply is then measured with the independent core v4 byte-count twin
//! [`reply_exact_bytes_v4`](crate::world_sizing_v4::reply_exact_bytes_v4)
//! before commit. Overflow discards the staged candidate and leaves
//! sessions, retained transfers, ordering, and command consumption exactly
//! as before: only reports that pass every cheap gate pay for one engine
//! clone, and only the byte-counted reply commits. The protocol
//! `encode_reply_v4` writer tests pin the arithmetic independently
//! (roundtrips and immutable literal bytes); core builds no second writer.

use std::collections::{BTreeMap, BTreeSet};

use crate::connection::{
    ApplicationIdentity, HandshakeReply, HandshakeRequest, HostCapabilities, ProtocolKind,
    SettingsReply, SettingsRequest,
};
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::session::{DomainKey, OutputDomain};
use crate::world_connection_state::WorldConnectionState;
pub(crate) use crate::world_diagnostics::{
    core_trace_enabled, diag_id, emit_focus_trace, emit_resize_refusal, format_bounded_failed,
};
#[cfg(test)]
pub(crate) use crate::world_diagnostics::{
    with_drag_preview_trace_capture, with_focus_trace_capture, with_focus_trace_disabled,
    with_retained_state_trace_capture, with_settlement_trace_capture,
};
use crate::world_layout::WorldLayout;
use crate::world_report::{
    AuthorityScope, CommandResult, HoldReason, ScopeAuthority, ScopeStatus, TilingMode,
    TransferKind, TransferTarget, WindowDesire, WorkspaceDesire, WorldCommand, WorldReplyV4,
    WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
pub(crate) use crate::world_staged::{
    EventIssue, RawRouting, StagedDomain, StagedObservation, StagedReport,
};
use crate::world_transaction::WorldTransaction;

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

pub(crate) const MSG_OWNER: &str = "owner is invalid";
pub(crate) const MSG_CORRELATION: &str = "correlation id is invalid";
pub(crate) const MSG_GENERATION: &str = "generation does not match the core generation";
pub(crate) const MSG_OVERSIZED: &str = "request exceeds size bound";
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

pub(crate) use crate::world_settlement_state::{
    RetainedBornClear, RetainedClear, RetainedFloat, RetainedNativeCreation, RetainedTransfer,
};

/// Public bounded outcome of one [`WorldEngine::handle_report`] call: the
/// genuine v4 result.
pub type WorldOutcome = V4RunOutcome;

pub use crate::world_admit_v4::{
    AdmissionGateDecision, CoreAdmissionGate, ExactInnerCheck, ExactInnerDecision,
    FutureReserveGateFacts, PreflightGateFacts, ReplayResidualToken, RetainedResidual,
};

/// Unforgeable borrowed proof that `reply` passed mandatory core semantic and
/// host-capability validation at final prepare. Constructible only inside
/// `tiler-core` after `validate_prepared_reply_v4` succeeds.
#[derive(Debug, Clone, Copy)]
pub struct ValidatedReply<'a>(&'a WorldReplyV4);

impl<'a> ValidatedReply<'a> {
    pub(crate) fn new_validated(reply: &'a WorldReplyV4) -> Self {
        Self(reply)
    }

    #[must_use]
    pub fn reply(&self) -> &'a WorldReplyV4 {
        self.0
    }
}

/// Borrowed pre-validation candidate view passed to
/// [`CoreAdmissionGate::check_wake_precheck`].
pub struct WakePrecheckCandidate<'a> {
    pub reply: &'a WorldReplyV4,
    pub active_gesture_id: Option<u64>,
    pub dispositions: &'a [crate::world_report::BatchDisposition],
}

/// Borrowed format-neutral candidate view passed to the caller's final
/// prepare callback immediately before candidate adoption, carrying an
/// unforgeable [`ValidatedReply`] proof.
pub struct PreparedCandidate<'a> {
    pub reply: ValidatedReply<'a>,
    pub active_gesture_id: Option<u64>,
    pub dispositions: &'a [crate::world_report::BatchDisposition],
}

/// Single-transition return contract: deliverable publication outcome,
/// world incorporation verdict, and sequence-keyed dispositions for supplied
/// batch identities.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionResult {
    pub outcome: V4RunOutcome,
    pub incorporation: crate::world_report::ReportIncorporation,
    pub dispositions: Vec<crate::world_report::BatchDisposition>,
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

/// Stateful world boundary owning its layout storage.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AdmittedReplayV4 {
    pub(crate) request_seq: u64,
    pub(crate) digest: u64,
    pub(crate) observation_seq: u64,
    pub(crate) report: WorldReportV4,
    pub(crate) residual: Option<RetainedResidual>,
    pub(crate) reply: WorldReplyV4,
}

#[derive(Debug, Clone)]
pub(crate) struct WorldState {
    pub(crate) layout: WorldLayout,
    pub(crate) ready_latched: bool,
    pub(crate) state_revision: u64,
    /// Retained prior-accepted observation/publication facts for shared
    /// lanes: previous raw focus plus the actually published
    /// controlled-empty workspace set. Written at `finish_v4` from the
    /// accepted candidate; cleared with every fresh engine, cloned with
    /// every staged candidate, never touched by rejected candidates.
    pub(crate) prior_context: PriorAcceptedContext,
    /// Monotonic command high-watermark per (incarnation, generation):
    /// any `command_seq` at or below it is already consumed, so eviction
    /// can never resurrect a sequence. Commandless reports never move it.
    pub(crate) command_highwater: Option<u64>,
    pub(crate) workspace: crate::world_workspace_state::WorkspaceState,
    pub(crate) settlement: crate::world_settlement_state::SettlementState,
    pub(crate) focus: crate::world_focus_state::FocusState,
    pub(crate) gesture: crate::world_gesture_state::GestureState,
    pub(crate) runtime: crate::world_runtime_state::RuntimeState,
    pub(crate) intent: crate::world_intent_state::IntentState,
    pub(crate) presentation: crate::world_presentation_state::PresentationState,
    /// Floating scopes for the report currently being staged: derived from
    /// [`WorkspaceState::tiling`] at the start of every accepted run, read by
    /// the command gates and publishing.
    pub(crate) floating_scopes: BTreeSet<DomainKey>,
    /// Per-scope sweep failure classification for the report currently
    /// being staged.
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
    /// Correlated per-command outcome staged by the command path below and
    /// consumed once by [`crate::world_engine::WorldEngine::publish_v4`] for the reply being built.
    /// `None` for observation-only replies and for operations whose result
    /// the snapshot already carries (move/resize/tiled-admit/focus/send).
    /// Cleared at the start of every staged run so a previous command's
    /// result never leaks into an unrelated reply.
    pub(crate) command_result: Option<CommandResult>,
    /// Staged logical observation for the report currently being staged:
    /// screens, one domain per known logical workspace, view echoes, raw
    /// facts, homing, and the carried gesture barrier. Built once per
    /// accepted run and read by settling, sweeping, commands, and
    /// publishing; never survives across runs.
    pub(crate) staged: StagedReport,
}

#[derive(Debug, Clone)]
pub struct WorldEngine {
    pub(crate) connection: WorldConnectionState,
    pub(crate) replay: Option<AdmittedReplayV4>,
    pub(crate) world: WorldState,
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
        mut values: Vec<(String, crate::connection::RawSettingValue)>,
    ) -> Option<Self> {
        let mut engine = Self::new(generation_seed)?;
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
            adapter: self.connection.adapter.clone()?,
            generation: self.connection.core_generation_str.clone(),
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
        GenerationId::parse(core_generation)?;
        Some(Self {
            connection: WorldConnectionState::new(core_generation),
            replay: None,
            world: WorldState::new(),
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
        let next_number = if self.connection.adapter.is_none() {
            self.connection.connection_number
        } else {
            let Some(number) = self.connection.connection_number.checked_add(1) else {
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
            self.connection.generation_seed.clone()
        } else {
            let suffixed = format!("{}-{next_number}", self.connection.generation_seed);
            if suffixed.len() <= crate::ids::MAX_GENERATION_LEN {
                suffixed
            } else {
                // Bounded within-engine rebind mint when appending to a long
                // seed would exceed MAX_GENERATION_LEN (64): not a global
                // cross-engine identity. Uniqueness fence: hitting this branch
                // means `generation_seed` was too long to fit `-{next_number}`,
                // whereas `rebind-{next_number}` is at most 27 bytes, so it
                // cannot collide with the initial seed, and strictly monotonic
                // `next_number` prevents collision with prior rebinds inside
                // this engine.
                format!("rebind-{next_number}")
            }
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
        let Some(mut fresh) = Self::new(&generation) else {
            return reply(
                request.adapter.clone(),
                request.request.clone(),
                None,
                false,
                Some(ProtocolKind::RevisionExhausted),
            );
        };
        fresh
            .connection
            .generation_seed
            .clone_from(&self.connection.generation_seed);
        fresh.connection.connection_number = next_number;
        fresh.connection.adapter = Some(request.adapter.clone());
        fresh.connection.connection_owner = Some(owner);
        // A new world generation starts with empty layout storage and a fresh
        // group allocator.
        fresh.connection.application_identity = Some(request.application_identity.clone());
        fresh.connection.capabilities = Some(request.capabilities.clone());
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
            || self.connection.adapter.as_deref() != Some(request.adapter.as_str())
            || self.connection.core_generation_str != request.generation
        {
            return None;
        }
        let caps = self.connection.capabilities.as_ref()?;
        let reply = self.connection.settings.process(request, caps);
        if reply.status == crate::connection::SettingsStatus::Accepted {
            self.connection.initial_settings_accepted = true;
            self.world
                .layout
                .set_fixed_size_predicate(self.connection.settings.fixed_size_predicate);
            self.world
                .layout
                .set_fresh_placement(self.connection.settings.fresh_placement);
        }
        Some(reply)
    }

    #[must_use]
    pub fn capabilities(&self) -> Option<&HostCapabilities> {
        self.connection.capabilities()
    }

    #[must_use]
    pub fn application_identity(&self) -> Option<&ApplicationIdentity> {
        self.connection.application_identity()
    }

    #[must_use]
    pub fn settings(&self) -> &crate::settings_state::SettingsState {
        self.connection.settings()
    }

    /// Test-only accessor returning window IDs of normalized visibility
    /// attempts staged on the latest observation.
    pub fn staged_visibility_attempt_windows_for_test(&self) -> Vec<WindowId> {
        self.world
            .staged
            .observation
            .property_attempts
            .iter()
            .filter_map(|attempt| attempt.as_visibility().map(|vis| vis.window.clone()))
            .collect()
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

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.world.ready_latched
    }

    #[must_use]
    pub fn core_generation(&self) -> &str {
        &self.connection.core_generation_str
    }

    /// Currently active retained gesture id, if any.
    #[must_use]
    pub fn active_gesture_id(&self) -> Option<u64> {
        self.world
            .gesture
            .retained
            .as_ref()
            .map(|retained| retained.id)
    }

    /// Retained cursor-first switch target from the latest accepted
    /// report (cursor-output default).
    #[must_use]
    pub fn switch_target(&self) -> crate::cursor_targeting::SwitchTarget {
        self.connection.switch_target_setting()
    }

    /// Retained fresh-window placement from the latest accepted report
    /// (cursor-output/last-focused default).
    #[must_use]
    pub fn fresh_placement(&self) -> crate::cursor_targeting::FreshPlacement {
        self.connection.fresh_placement_setting()
    }

    /// Retained focus-follows-cursor setting (default off); the stage-4b
    /// core policy in [`crate::world_cursor`] actuates it.
    #[must_use]
    pub fn focus_follows_cursor(&self) -> bool {
        self.connection.focus_follows_cursor_setting()
    }

    /// Retained cursor-warp setting (default off); the stage-4b core
    /// policy in [`crate::world_cursor`] actuates it.
    #[must_use]
    pub fn cursor_warp(&self) -> bool {
        self.connection.cursor_warp_setting()
    }

    /// Normalized cursor retained from the latest accepted report, `None`
    /// when the adapter could not read it.
    #[must_use]
    pub fn last_cursor(&self) -> Option<(i32, i32)> {
        self.world.focus.last_cursor
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
        let trace = &self.world.focus.fresh_focus.trace;
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
        self.world
            .settlement
            .retained
            .iter()
            .map(|r| r.target.clone())
            .collect()
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
        self.world.stay_source_focus(operation_id, source)
    }

    /// Latest published global revision.
    #[must_use]
    pub fn state_revision(&self) -> u64 {
        self.world.state_revision
    }

    /// Adapter routing projection (read-only, never a second policy):
    /// ordered logical outputs known to the core.
    #[must_use]
    pub fn logical_outputs(&self) -> Vec<String> {
        let mut out: Vec<String> = self.world.workspace.mapping.keys().cloned().collect();
        out.sort();
        out
    }

    /// Adapter routing projection: ordered logical ring for one output.
    #[must_use]
    pub fn logical_ring(&self, output: &str) -> Vec<String> {
        self.world
            .workspace
            .mapping
            .get(output)
            .cloned()
            .unwrap_or_default()
    }

    /// Adapter routing projection: retained logical view for one output.
    #[must_use]
    pub fn logical_view(&self, output: &str) -> Option<String> {
        self.world.workspace.views.get(output).cloned()
    }

    /// Adapter routing projection: retained tiling mode (absent seeds tiled).
    #[must_use]
    pub fn logical_tiling(&self, output: &str, workspace: &str) -> TilingMode {
        let key = DomainKey {
            output: OutputId(output.to_owned()),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        self.world
            .workspace
            .tiling
            .get(&key)
            .copied()
            .unwrap_or(TilingMode::Tiled)
    }

    /// Adapter routing projection: members homed to one logical workspace.
    #[must_use]
    pub fn logical_members(&self, output: &str, workspace: &str) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = self
            .world
            .workspace
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
        self.world
            .workspace
            .members
            .get(window)
            .map(|state| (state.home.output.0.clone(), state.home.workspace.0.clone()))
    }

    /// Adapter routing projection: all known lifetimes (sorted).
    #[must_use]
    pub fn logical_members_all(&self) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = self.world.workspace.members.keys().cloned().collect();
        out.sort();
        out
    }

    /// Adapter routing projection: retained previous-view history per output
    /// (observed views only, never desires).
    #[must_use]
    pub fn logical_previous(&self, output: &str) -> Option<String> {
        self.world.workspace.history.previous(output)
    }

    /// Accept one complete v4 report plus the parked FIFO command batch
    /// drained inside the same capture/run/publication. This is the sole
    /// public report ingress: the caller supplies the typed v4 report and
    /// the ordered `(command sequence, command)` pairs, and the admitted
    /// boundary owns admission (bound owner plus request correlation)
    /// before delegating to the internal v4 run. An empty `queued` preserves
    /// the existing inline report-command semantics. Returns the single
    /// admission outcome plus one per-command settlement index-aligned with
    /// `queued`. Any refusal adopts nothing, settles nothing, and returns
    /// empty settlements so the caller reparks the whole batch. Missing
    /// bound owner refuses as owner-invalid; an invalid request correlation
    /// refuses as correlation-invalid.
    ///
    /// Structural admission only (no wire/reply size gating); available only
    /// in tests or explicit `structural-bench` builds.
    #[cfg(any(test, feature = "structural-bench"))]
    pub fn handle_report(
        &mut self,
        report: &WorldReportV4,
        queued: &[(u64, WorldCommand)],
    ) -> (V4RunOutcome, Vec<crate::world_report::BatchSettlement>) {
        self.handle_report_with_prepare(report, queued, |_| Ok::<(), std::convert::Infallible>(()))
            .unwrap_or_else(|e| match e {})
    }

    /// Same as [`WorldEngine::handle_report`], except `prepare` runs once on
    /// the borrowed candidate immediately before candidate adoption.
    /// Available only in tests or explicit `structural-bench` builds.
    #[cfg(any(test, feature = "structural-bench"))]
    pub fn handle_report_with_prepare<E>(
        &mut self,
        report: &WorldReportV4,
        queued: &[(u64, WorldCommand)],
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<(V4RunOutcome, Vec<crate::world_report::BatchSettlement>), E> {
        let result = self.handle_report_with_gate_and_prepare(
            report,
            queued,
            &crate::world_admit_v4::StructuralCoreAdmissionGate,
            prepare,
        )?;
        Ok((
            result.outcome,
            crate::world_admit_v4::dispositions_to_legacy_settlements(&result.dispositions, queued),
        ))
    }

    /// Sole production report transition taking an explicit
    /// [`CoreAdmissionGate`] alongside the final `prepare` callback.
    pub fn handle_report_with_gate_and_prepare<E, G: CoreAdmissionGate + ?Sized>(
        &mut self,
        report: &WorldReportV4,
        queued: &[(u64, WorldCommand)],
        gate: &G,
        prepare: impl FnOnce(PreparedCandidate<'_>) -> Result<(), E>,
    ) -> Result<TransitionResult, E> {
        let Some(owner) = self.connection.connection_owner.clone() else {
            return Ok(TransitionResult {
                outcome: V4RunOutcome::Rejected {
                    kind: KIND_OWNER,
                    message: MSG_OWNER,
                },
                incorporation: crate::world_report::ReportIncorporation::NotAdmitted,
                dispositions: Vec::new(),
            });
        };
        let correlation = match CorrelationId::parse(&report.request_id) {
            Some(correlation) => correlation,
            None => {
                return Ok(TransitionResult {
                    outcome: V4RunOutcome::Rejected {
                        kind: KIND_CORRELATION,
                        message: MSG_CORRELATION,
                    },
                    incorporation: crate::world_report::ReportIncorporation::NotAdmitted,
                    dispositions: Vec::new(),
                });
            }
        };
        self.handle_report_v4_batched_with_gate_and_prepare(
            report,
            &EventIssue { owner, correlation },
            queued,
            gate,
            prepare,
        )
    }
}

/// Test-only bridges from the public engine to its internal transaction and
/// replay cache. Production callers use the real report boundary instead.
#[cfg(test)]
impl WorldEngine {
    /// Run one closure against a fresh transaction over this engine's live
    /// state. Test-only: mirrors the transaction the report boundary builds.
    pub(crate) fn tx<T>(&mut self, f: impl FnOnce(&mut WorldTransaction<'_>) -> T) -> T {
        let prev_transfers = self
            .replay
            .as_ref()
            .map(|replay| replay.reply.transfers.as_slice())
            .unwrap_or(&[]);
        let mut tx = WorldTransaction::new(&self.connection, prev_transfers, &mut self.world);
        f(&mut tx)
    }

    pub(crate) fn has_last_v4(&self) -> bool {
        self.replay.is_some()
    }

    pub(crate) fn last_request_seq_v4(&self) -> u64 {
        self.replay.as_ref().map_or(0, |r| r.request_seq)
    }

    pub(crate) fn last_report_v4(&self) -> Option<WorldReportV4> {
        self.replay.as_ref().map(|r| r.report.clone())
    }

    pub(crate) fn last_reply_v4(&self) -> Option<WorldReplyV4> {
        self.replay.as_ref().map(|r| r.reply.clone())
    }

    pub(crate) fn set_last_digest_v4(&mut self, digest: u64) {
        if let Some(replay) = &mut self.replay {
            replay.digest = digest;
        }
    }

    pub(crate) fn clear_replay(&mut self) {
        self.replay = None;
    }

    /// Read-only transaction helper for tests whose subject is an immutable
    /// engine reference; clones so the observed state is not mutated.
    pub(crate) fn tx_ro<T>(&self, f: impl FnOnce(&WorldTransaction<'_>) -> T) -> T {
        let mut world = self.world.clone();
        let prev_transfers = self
            .replay
            .as_ref()
            .map(|replay| replay.reply.transfers.as_slice())
            .unwrap_or(&[]);
        let tx = WorldTransaction::new(&self.connection, prev_transfers, &mut world);
        f(&tx)
    }

    /// Test-only adapter for the read-only cursor pause predicate.
    pub(crate) fn cursor_game_paused(&self) -> bool {
        self.tx_ro(crate::world_cursor::cursor_game_paused)
    }

    /// Test-only adapter for the unproven-sticky swap source predicate.
    pub(crate) fn swap_source_unknown_sticky(&self, source: &crate::session::DomainKey) -> bool {
        self.tx_ro(|tx| WorldTransaction::swap_source_unknown_sticky(tx, source))
    }
}

impl WorldState {
    #[must_use]
    pub(crate) fn new() -> Self {
        let mut layout = WorldLayout::new();
        // Fixed-size admission stays enabled for every world carrier.
        layout.set_fixed_size_admission(true);
        Self {
            layout,
            ready_latched: false,
            state_revision: 0,
            prior_context: PriorAcceptedContext::default(),
            command_highwater: None,
            workspace: crate::world_workspace_state::WorkspaceState::new(),
            settlement: crate::world_settlement_state::SettlementState::new(),
            focus: crate::world_focus_state::FocusState::new(),
            gesture: crate::world_gesture_state::GestureState::new(),
            runtime: crate::world_runtime_state::RuntimeState::new(),
            intent: crate::world_intent_state::IntentState::new(),
            presentation: crate::world_presentation_state::PresentationState::new(),
            floating_scopes: BTreeSet::new(),
            hold_reasons: BTreeMap::new(),
            placement_bootstrapped: false,
            command_result: None,
            staged: StagedReport::default(),
        }
    }

    pub(crate) fn stay_source_focus(
        &self,
        operation_id: &str,
        source: &DomainKey,
    ) -> StaySourceFocus {
        let Some(retained) = self
            .settlement
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
        if !self.layout.contains_domain(source) {
            return StaySourceFocus::Empty;
        };
        if let Some(leaf) = self.layout.focus_leaf(source) {
            let Some(resident) = self.layout.window_for_leaf(source, leaf) else {
                return StaySourceFocus::Unknown;
            };
            if resident == mover {
                return StaySourceFocus::Unknown;
            }
            return StaySourceFocus::Resident(resident);
        }
        let mut residents = self.layout.resident_windows(source);
        if residents.contains(&mover) {
            return StaySourceFocus::Unknown;
        }
        match residents.len() {
            0 => StaySourceFocus::Empty,
            1 => StaySourceFocus::Resident(residents.remove(0)),
            _ => StaySourceFocus::Unknown,
        }
    }

    pub(crate) fn layout(&self) -> &WorldLayout {
        &self.layout
    }

    pub(crate) fn engine_knows_window_any(&self, window: &WindowId) -> bool {
        self.workspace.members.contains_key(window) || self.layout.knows_window_any(window)
    }

    pub(crate) fn is_tiled_window_any(&self, window: &WindowId) -> bool {
        self.layout.is_tiled_window_any(window)
    }

    pub(crate) fn is_exception_window_any(&self, window: &WindowId) -> bool {
        self.layout.is_exception_window_any(window)
    }

    pub(crate) fn retained_publish_footprint(&self) -> usize {
        self.layout
            .retained_bound_count()
            .saturating_add(self.settlement.retained_transfer_footprint())
    }

    pub(crate) fn intent_desired_new_rows(
        &self,
        inline: Option<&crate::world_report::WorldCommand>,
        queued: &[(u64, crate::world_report::WorldCommand)],
    ) -> usize {
        use crate::world_report::is_intent_save_command_v4;
        use std::collections::BTreeSet;
        let mut distinct: BTreeSet<String> = BTreeSet::new();
        let mut implicit_new: usize = 0;
        let mut inline_key: Option<String> = None;
        if let Some(command) = inline
            && is_intent_save_command_v4(command)
        {
            if crate::world_report::is_current_focus_command(command) {
                implicit_new = implicit_new.saturating_add(1);
            } else {
                let key = match command {
                    crate::world_report::WorldCommand::SetSticky {
                        window: crate::directional::WindowSubject::Window(window),
                        ..
                    }
                    | crate::world_report::WorldCommand::ToggleSticky {
                        window: crate::directional::WindowSubject::Window(window),
                    }
                    | crate::world_report::WorldCommand::Operation(
                        crate::boundary::CoreCommand::ToggleFloat {
                            window: crate::directional::WindowSubject::Window(window),
                            ..
                        },
                    ) => Some(window.0.clone()),
                    _ => None,
                };
                if let Some(key) = key {
                    inline_key = Some(key.clone());
                    distinct.insert(key);
                }
            }
        }
        for (_, command) in queued {
            if !is_intent_save_command_v4(command) {
                continue;
            }
            if crate::world_report::is_current_focus_command(command) {
                implicit_new = implicit_new.saturating_add(1);
                continue;
            }
            match command {
                crate::world_report::WorldCommand::SetSticky {
                    window: crate::directional::WindowSubject::Window(window),
                    ..
                }
                | crate::world_report::WorldCommand::ToggleSticky {
                    window: crate::directional::WindowSubject::Window(window),
                }
                | crate::world_report::WorldCommand::Operation(
                    crate::boundary::CoreCommand::ToggleFloat {
                        window: crate::directional::WindowSubject::Window(window),
                        ..
                    },
                ) => {
                    distinct.insert(window.0.clone());
                }
                _ => {}
            }
        }
        if let Some(inline) = inline_key {
            distinct.remove(&inline);
            let inline_new = usize::from(
                !self
                    .intent
                    .intent_desired
                    .contains_key(&crate::directional::WindowId(inline.clone())),
            );
            let batch_new = distinct
                .iter()
                .filter(|id| {
                    !self
                        .intent
                        .intent_desired
                        .contains_key(&crate::directional::WindowId((*id).clone()))
                })
                .count();
            return inline_new
                .saturating_add(batch_new)
                .saturating_add(implicit_new);
        }
        let batch_new = distinct
            .iter()
            .filter(|id| {
                !self
                    .intent
                    .intent_desired
                    .contains_key(&crate::directional::WindowId((*id).clone()))
            })
            .count();
        batch_new.saturating_add(implicit_new)
    }
}

impl WorldTransaction<'_> {
    pub(crate) fn is_ready(&self) -> bool {
        self.connection.initial_settings_accepted && self.world.ready_latched
    }

    pub(crate) fn logical_outputs(&self) -> Vec<String> {
        let mut out: Vec<String> = self.world.workspace.mapping.keys().cloned().collect();
        out.sort();
        out
    }

    pub(crate) fn logical_ring(&self, output: &str) -> Vec<String> {
        self.world
            .workspace
            .mapping
            .get(output)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn logical_view(&self, output: &str) -> Option<String> {
        self.world.workspace.views.get(output).cloned()
    }

    pub(crate) fn logical_tiling(&self, output: &str, workspace: &str) -> TilingMode {
        let key = DomainKey {
            output: OutputId(output.to_owned()),
            workspace: crate::directional::WorkspaceId(workspace.to_owned()),
        };
        self.world
            .workspace
            .tiling
            .get(&key)
            .copied()
            .unwrap_or(TilingMode::Tiled)
    }

    pub(crate) fn logical_members(&self, output: &str, workspace: &str) -> Vec<WindowId> {
        let mut out: Vec<WindowId> = self
            .world
            .workspace
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

    pub(crate) fn adapter_homing(&self, window: &WindowId) -> Option<(String, String)> {
        self.world
            .workspace
            .members
            .get(window)
            .map(|state| (state.home.output.0.clone(), state.home.workspace.0.clone()))
    }
}
