//! Owned native-settlement substate for [`crate::world_engine::WorldState`].
//!
//! Holds correlated native transfer, float, state, clear, born, creation, and
//! frame evidence, native workspace bindings, visibility/selection evidence,
//! and retained operation/switch outcomes. Logical workspace facts live
//! separately in [`crate::world_workspace_state::WorkspaceState`], and runtime
//! composed maximize lives separately on `WorldState`.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_attempts::{ConsumedFrameWatermark, PropertyAttempt};
use crate::world_engine::{core_trace_enabled, diag_id, emit_focus_trace, format_bounded_failed};
use crate::world_frames::{FrameBurst, FrameMemory, LearnedMin};
use crate::world_report::{
    AttemptOutcome, NativeCreationAttempt, OperationEvent, OperationKind, StateProperty,
    SwitchEvent, TransferKind, TransferTarget, WorldReplyV4,
};
use crate::world_selection::{
    PendingSelection, RetainedSelection, SelectionScope, SelectionTarget,
};
use crate::world_state::RetainedStateBinding;

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
    /// transfer kind.
    pub(crate) members: Vec<WindowId>,
    /// Expected source-refill logical view for a Migration, stored at commit
    /// so settlement survives early refill-child settlement.
    pub(crate) migration_refill: Option<String>,
    /// Explicit parent operation id for a migration refill child (`View`
    /// transfer), correlated in core without parsing token spelling.
    pub(crate) migration_parent_op: Option<String>,
    /// True when a Migration committed with zero members (originally empty).
    pub(crate) migration_initial_empty: bool,
    /// Source ring index of the migrated id at commit, for refusal
    /// rollback position restore (never wraps; clamped on reinsert).
    pub(crate) migration_source_index: Option<usize>,
    /// Core views at commit for refusal rollback reconciliation.
    pub(crate) migration_source_view: Option<String>,
    pub(crate) migration_target_view: Option<String>,
    /// Latched terminal migration outcome (proven refusal or external
    /// supersession) retained under the operation identity after its raw
    /// attempt row is acknowledged.
    pub(crate) migration_terminal: Option<AttemptOutcome>,
    /// Issuing selection revision for a native-free migration binding
    /// (`pending.rev` at commit, `None` on the legacy host-owned route).
    pub(crate) migration_selection_rev: Option<u64>,
    /// Latest exact-correlated canonical membership evidence staged BEFORE any
    /// readability hold (window transfer) or per-member map (migration parent).
    pub(crate) pending_member: Option<PropertyAttempt>,
    /// Superseded published window-transfer correlations plus canonical
    /// evidence, never native authority.
    pub(crate) member_predecessors: Vec<(TransferTarget, Option<PropertyAttempt>)>,
    /// Per-member latest canonical evidence for a migration parent, keyed by
    /// member window.
    pub(crate) pending_members: BTreeMap<WindowId, PropertyAttempt>,
    /// Latest exact-correlated canonical view evidence for the owning phase.
    pub(crate) pending_view: Option<PropertyAttempt>,
    /// Frozen detached destination of one eligible standalone explicit
    /// selection view flight: the exact target surface plus the intended
    /// member the adapter was granted. `None` on every other flight
    /// (policy, swap, refill, follow, migration, capability-false host).
    pub(crate) detached_destination: Option<crate::world_report::DetachedDestination>,
}

/// Retained explicit float-placement target: one outstanding authorized
/// frame per window, keyed by the stable command operation identity
/// (`op-<cmdSeq>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedFloat {
    pub(crate) operation_id: String,
    pub(crate) command_seq: u64,
    pub(crate) window: WindowId,
    pub(crate) target: Rect,
    pub(crate) source: Option<Rect>,
    pub(crate) output: OutputId,
    pub(crate) workspace: String,
    pub(crate) ambiguous: bool,
    pub(crate) pending: Option<PropertyAttempt>,
}

/// Retained host-owned native creation: one outstanding core-owned
/// authorization for the adapter to create exactly one native desktop for
/// one logical workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedNativeCreation {
    pub(crate) operation_id: String,
    pub(crate) logical_workspace: String,
    pub(crate) target_output: OutputId,
    pub(crate) order_before: Vec<String>,
    pub(crate) minted_ms: u64,
    pub(crate) pending: Option<NativeCreationAttempt>,
    pub(crate) ambiguous: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SettlementState {
    pub(crate) retained: Vec<RetainedTransfer>,
    pub(crate) retained_floats: Vec<RetainedFloat>,
    pub(crate) retained_states: BTreeMap<(WindowId, StateProperty), RetainedStateBinding>,
    pub(crate) born_seq: u64,
    pub(crate) born_attempted: BTreeSet<WindowId>,
    pub(crate) born_held_fullscreen: BTreeSet<WindowId>,
    pub(crate) born_floating_first: BTreeSet<WindowId>,
    pub(crate) born_seen_nonfullscreen: BTreeSet<WindowId>,
    pub(crate) born_keep_overlay: BTreeSet<WindowId>,
    pub(crate) frames: BTreeMap<WindowId, FrameMemory>,
    /// Per-window `attempt_id` replay watermark and latest qualifying timed
    /// float self-echo target.
    pub(crate) consumed_frames: BTreeMap<WindowId, ConsumedFrameWatermark>,
    pub(crate) frame_bursts: BTreeMap<WindowId, FrameBurst>,
    pub(crate) learned_mins: BTreeMap<WindowId, LearnedMin>,
    pub(crate) native_creation_seq: u64,
    pub(crate) retained_creations: Vec<RetainedNativeCreation>,
    pub(crate) pending_source_releases: Vec<(String, String)>,
    /// Typed coherent physical view last retained per output, for selector
    /// echo comparison and detached return proof. Detached entries name the
    /// physical surface; native entries name the native desktop (translated
    /// to a logical workspace by consumers when needed).
    pub(crate) last_views: BTreeMap<OutputId, crate::world_report::ViewRef>,
    pub(crate) native_map: BTreeMap<(String, String), String>,
    pub(crate) retired_natives: BTreeMap<(String, String), String>,
    pub(crate) shared_roster: crate::world_roster::RosterState,
    pub(crate) pending_forget: Vec<(DomainKey, WindowId)>,
    pub(crate) operation_settled: BTreeMap<(WindowId, OperationKind), OperationEvent>,
    pub(crate) staged_dropped_operations: BTreeMap<(WindowId, OperationKind), OperationEvent>,
    pub(crate) switch_settled: BTreeMap<OutputId, SwitchEvent>,
    pub(crate) staged_dropped_switches: BTreeMap<OutputId, SwitchEvent>,
    pub(crate) pending_selection: Option<PendingSelection>,
    pub(crate) pending_policy_selections: Vec<PendingSelection>,
    pub(crate) selection_targets: BTreeMap<WindowId, SelectionTarget>,
    pub(crate) retained_selection: BTreeMap<OutputId, RetainedSelection>,
    pub(crate) selection_scopes: BTreeMap<OutputId, SelectionScope>,
}

impl Default for SettlementState {
    fn default() -> Self {
        Self::new()
    }
}

impl SettlementState {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            retained: Vec::new(),
            retained_floats: Vec::new(),
            retained_states: BTreeMap::new(),
            born_seq: 0,
            born_attempted: BTreeSet::new(),
            born_held_fullscreen: BTreeSet::new(),
            born_floating_first: BTreeSet::new(),
            born_seen_nonfullscreen: BTreeSet::new(),
            born_keep_overlay: BTreeSet::new(),
            frames: BTreeMap::new(),
            consumed_frames: BTreeMap::new(),
            frame_bursts: BTreeMap::new(),
            learned_mins: BTreeMap::new(),
            native_creation_seq: 0,
            retained_creations: Vec::new(),
            pending_source_releases: Vec::new(),
            last_views: BTreeMap::new(),
            native_map: BTreeMap::new(),
            retired_natives: BTreeMap::new(),
            shared_roster: crate::world_roster::RosterState::default(),
            pending_forget: Vec::new(),
            operation_settled: BTreeMap::new(),
            staged_dropped_operations: BTreeMap::new(),
            switch_settled: BTreeMap::new(),
            staged_dropped_switches: BTreeMap::new(),
            pending_selection: None,
            pending_policy_selections: Vec::new(),
            selection_targets: BTreeMap::new(),
            retained_selection: BTreeMap::new(),
            selection_scopes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub(crate) fn retained_transfer_footprint(&self) -> usize {
        let mut total: usize = 0;
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

    #[must_use]
    pub(crate) fn pending_operation_id(
        &self,
        window: &WindowId,
        kind: OperationKind,
    ) -> Option<&str> {
        match kind {
            OperationKind::Clear => self
                .retained_states
                .get(&(window.clone(), StateProperty::Maximized))
                .and_then(|binding| match binding.clear_meta.as_ref() {
                    Some(crate::world_state::ClearProvenance::Directional(meta)) => {
                        Some(meta.operation_id.as_str())
                    }
                    Some(crate::world_state::ClearProvenance::Born(meta)) => {
                        Some(meta.operation_id.as_str())
                    }
                    None => None,
                }),
            OperationKind::Float => self
                .retained_floats
                .iter()
                .find(|retained| &retained.window == window)
                .map(|retained| retained.operation_id.as_str())
                .or_else(|| {
                    self.retained_states
                        .get(&(window.clone(), StateProperty::Maximized))
                        .and_then(|binding| match binding.clear_meta.as_ref() {
                            Some(crate::world_state::ClearProvenance::Directional(meta))
                                if meta.float_route =>
                            {
                                Some(meta.operation_id.as_str())
                            }
                            _ => None,
                        })
                }),
        }
    }

    /// Emit and clear trace-only Q2 dropped outcome diagnostics at actual
    /// final publication/adoption, correlated to the final published reply's
    /// `(gen, req, rev, obs)` (`OMNITILER_TRACE=1`, zero cost when disabled).
    pub(crate) fn emit_staged_dropped_settlement_trace(&mut self, reply: &WorldReplyV4) {
        if !core_trace_enabled() {
            self.staged_dropped_operations.clear();
            self.staged_dropped_switches.clear();
            return;
        }
        let gen_str = diag_id(&reply.core_generation);
        let req_str = diag_id(&reply.request_id);
        let rev = reply.state_revision;
        let obs = reply.based_on_observation_seq;
        for event in self.staged_dropped_operations.values() {
            emit_focus_trace(format!(
                "omnitiler:core:trace ev=operation-settled-dropped gen={} req={} rev={} obs={} win={} kind={} op={} outcome={} cause={}",
                gen_str,
                req_str,
                rev,
                obs,
                diag_id(&event.window.0),
                event.kind.as_str(),
                diag_id(&event.operation_id),
                event.outcome.as_str(),
                event.cause.as_deref().map_or("-", diag_id),
            ));
        }
        for event in self.staged_dropped_switches.values() {
            let (failed_shown, omitted) = format_bounded_failed(&event.failed);
            if omitted > 0 {
                emit_focus_trace(format!(
                    "omnitiler:core:trace ev=switch-settled-dropped gen={} req={} rev={} obs={} out={} op={} outcome={} focus={} fallback={} failed={} failedOmitted={}",
                    gen_str,
                    req_str,
                    rev,
                    obs,
                    diag_id(&event.output.0),
                    diag_id(&event.operation_id),
                    event.outcome.as_str(),
                    event.focus.as_str(),
                    event.fallback,
                    failed_shown,
                    omitted,
                ));
            } else {
                emit_focus_trace(format!(
                    "omnitiler:core:trace ev=switch-settled-dropped gen={} req={} rev={} obs={} out={} op={} outcome={} focus={} fallback={} failed={}",
                    gen_str,
                    req_str,
                    rev,
                    obs,
                    diag_id(&event.output.0),
                    diag_id(&event.operation_id),
                    event.outcome.as_str(),
                    event.focus.as_str(),
                    event.fallback,
                    failed_shown,
                ));
            }
        }
        self.staged_dropped_operations.clear();
        self.staged_dropped_switches.clear();
    }
}
