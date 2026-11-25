#![forbid(unsafe_code)]
//! In-process adapter sequencing boundary for the runtime world owner.
//!
//! Assembles the
//! typed [`WorldReportV4`](tiler_core::world_report::WorldReportV4) envelope from
//! the accepted source-first [`capture_world`](super::world_capture::capture_world)
//! pieces and sequences it through the real [`WorldEngine`](tiler_core::world_engine::WorldEngine)
//! via the single shared [`WorldEngine::new`](tiler_core::world_engine::WorldEngine::new)
//! constructor. The runtime world owner uses this envelope
//! for observation and command dispatch; native actuation remains outside
//! the sequencer. No JSON transport, polling, or timers.
//!
//! Sequencing rules (shared
//! [ordering/bootstrap/ack](../../../../docs/changes/archive/core-full-state-replies.md)
//! contract, in-process shape):
//! - Bootstrap binds explicit adapter/core identities through authenticated
//!   construction; a delayed arbitrary reply can neither bind nor reset the
//!   generation. Readiness is the first applied acknowledgement.
//! - Single flight. Invalidation advances the current observation watermark
//!   synchronously before dispatch, including during a flight. Dispatch never
//!   clears the observation obligation: dirty-until-ack leaves it set until
//!   an accepted current acknowledgement arrives, so `is_dirty` sees the
//!   outstanding flight observation until the actual ack. No retries, no
//!   polling; dirtiness is only readiness for event-driven catch-up.
//! - Dirty means an outstanding OBSERVATION, CLOSE, FRAME, MEMBER, or VIEW
//!   obligation, never the watermark alone: an accepted report
//!   whose watermark is current still stays dirty while newer raw attempts
//!   or an absent-domain close remains queued, with no extra native
//!   invalidation required to notice.
//! - Closes stay native-owned: the native `PendingClosed` map is the SOLE
//!   close authority and there is no sequencer-side ack shadow history.
//!   Dispatch snapshots the exact captured `(token, homing)` pairs it
//!   carries; the ack receipt returns exactly those pairs and the caller
//!   prunes its own map entry if and only if the current source entry still
//!   exactly equals the delivered one (see [`WorldSequencer::apply_close_receipt`]).
//!   Receipts are bound to the incorporating adapter/core generation: one
//!   held across a reset prunes nothing, so the fresh generation's close
//!   obligation stays dirty and redelivers.
//!   `is_dirty` correctly stays true until the native queue applies the
//!   receipt; nothing here hides native obligations.
//! - A reply whose observation echo differs from the dispatched flight's is
//!   ignored with the flight kept, before any obligation is acknowledged.
//!   A stale ready reply with matching flight correlation acknowledges
//!   incorporation (receipt) but never applies state/effects and always
//!   schedules observation-only catch-up. Foreign-generation,
//!   foreign-adapter, and wrong-request replies apply and acknowledge nothing
//!   and never consume a newer flight.
//! - Explicit commands are caller-supplied per dispatch and refused while
//!   busy; while observation-only catch-up is pending, a supplied command is
//!   refused as [`DispatchError::CatchupRequired`] without consuming
//!   counters, flight, or flags, so the caller dispatches the catch-up
//!   observation first and resubmits the command after. Commands never
//!   silently coalesce and a lost/unaccepted command is never retried
//!   automatically. Counter exhaustion refuses dispatch and requires a
//!   generation reset; counters never wrap in place.
//! - Rejection/loss is correlated to the exact outstanding flight
//!   (generation plus request id): a delayed old rejection can never clear a
//!   newer flight after catch-up or a generation reset.
//! - Generation reset requires an explicit NEW identity (same or malformed
//!   resets are side-effect-free no-ops) and clears sequencer
//!   flight/ordering/desired/command/raw-attempt state, rebuilding the engine
//!   via the shared constructor. Counters restart because the core namespaces
//!   ordering per generation; they still never wrap within one. Native
//!   lifetime/close facts are untouched: the owner is not mutably reachable
//!   here. Old arbitrary replies cannot reset: only this call changes the
//!   bound generation.

use super::world_capture::{CapturedWorld, OwnerInputs, capture_world};
use super::{MonitorArea, ObservedWindow, PendingClosed, RetainedRow};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use tiler_core::connection::{RawSettingValue, SettingsReply, SettingsStatus};
use tiler_core::ids::{GenerationId, OwnerId};
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    BatchDisposition, BatchSettlement, CommandDisposition, CommandResult, FrameAttempt,
    MemberAttempt, PresentationRowV4, ViewAttempt, VisibilityAttempt, WORLD_CONTRACT_VERSION_V4,
    WorldCommand, WorldReplyV4, WorldReportV4,
};

/// One outstanding sequenced report: the exact facts the core incorporation
/// acknowledgement must settle. Closes are snapshots of what the
/// dispatched report carried, never a re-read of newer state. Close entries
/// pair each carried token with its native homing read at dispatch time, so
/// the caller can prune if and only if its source entry still matches.
#[derive(Debug, Clone)]
struct SequencerFlight {
    observation_seq: u64,
    // Consumed by the 2b owner cutover for command correlation; the core
    // already namespaces command consumption per incarnation.
    #[allow(dead_code)]
    command_seq: Option<u64>,
    request_id: String,
    delivered_closed: Vec<(String, PendingClosed)>,
    delivered_frames: Vec<FrameAttempt>,
    delivered_members: Vec<MemberAttempt>,
    delivered_views: Vec<ViewAttempt>,
    delivered_visibility: Vec<VisibilityAttempt>,
}

/// Exact incorporation receipt for the native owner to prune against. Close
/// pairs are `(token, homing)` exactly as captured at dispatch; the caller
/// removes its source entry only on exact equality (see
/// [`WorldSequencer::apply_close_receipt`]). The receipt is bound to the
/// adapter/core generation namespace that incorporated it: a receipt held
/// across a reset prunes nothing under the fresh generation. Derived only
/// from the acknowledged flight's delivered facts, never from a re-read of
/// newer state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AckReceipt {
    pub(super) adapter_incarnation: String,
    pub(super) core_generation: String,
    pub(super) request_id: String,
    pub(super) closed_acked: Vec<(String, PendingClosed)>,
    pub(super) frames_acked: Vec<String>,
    pub(super) members_acked: Vec<(String, String)>,
    pub(super) views_acked: Vec<String>,
}

/// Outcome of offering one core reply to the sequencer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AckOutcome {
    /// Current observation, newer revision: obligations acked, desired
    /// snapshot applied, catch-up cleared.
    Applied(AckReceipt),
    /// Matching correlation but superseded observation (or a not-ready
    /// reply): incorporation acked where it happened, desired snapshot and
    /// effects NOT applied, observation-only catch-up scheduled.
    Stale(AckReceipt),
    /// No flight, foreign generation/adapter, or wrong request: nothing
    /// applied, nothing acknowledged, the outstanding flight (if any) kept.
    Ignored { reason: &'static str },
}

/// Why a dispatch was refused. Commands are never queued: the caller retries
/// with current input on the next native event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DispatchError {
    /// A flight is outstanding; single-flight only.
    Busy,
    /// Observation-only catch-up is pending and the caller supplied an
    /// explicit command. Nothing was consumed (no counters, no flight, no
    /// flags): dispatch the catch-up observation first, then resubmit the
    /// command. The superseded command is never replayed.
    CatchupRequired,
    /// A request/command counter or the observation watermark reached
    /// `u64::MAX`: reset the generation (fresh bootstrap namespace), never
    /// wrap in place.
    CountersExhausted,
}

/// Future in-process adapter sequencing boundary. Owns the offline
/// [`WorldEngine`] plus adapter-side ordering state; reads the native owner
/// only through shared references.
#[derive(Debug)]
pub(super) struct WorldSequencer {
    adapter_incarnation: String,
    core_generation: String,
    engine: WorldEngine,
    /// Last settings revision accepted by the core (connect applies 1).
    settings_revision: u64,
    /// Last accepted raw settings snapshot. Replayed at revision 1 on a
    /// generation reset so restart-only values (mode) adopt under the fresh
    /// generation instead of reverting to handshake defaults.
    last_settings_values: Vec<(String, RawSettingValue)>,
    /// Initial (revision 1) settings reply from bootstrap, retained until the
    /// runtime logs its status and diagnostics. `None` once taken.
    initial_settings_reply: Option<SettingsReply>,
    current_obs: u64,
    obs_dirty: bool,
    next_request_seq: u64,
    next_command_seq: u64,
    flight: Option<SequencerFlight>,
    /// Latest-wins mechanical frame attempts per window lifetime, drained
    /// into the next report. Stamped at the write with the tick's monotonic
    /// clock; the core consumes each stamp idempotently, so redelivery
    /// after a lost acknowledgement never renews backoff.
    pending_frames: BTreeMap<String, FrameAttempt>,
    /// Latest-wins raw membership attempts per operation window, drained
    /// into the next report. The core derives window/migration completion
    /// from these plus the staged observation; the acknowledgement removes
    /// only exactly-equal entries, so a newer update for the same operation
    /// window survives its predecessor's ack.
    pending_members: BTreeMap<(String, String), MemberAttempt>,
    /// Latest-wins raw view attempts per owning view phase, drained into
    /// the next report (same ack semantics as members).
    pending_views: BTreeMap<String, ViewAttempt>,
    /// Latest-wins visibility readbacks per window lifetime, drained into
    /// the next report (same ack semantics as frames).
    pending_visibility: BTreeMap<String, VisibilityAttempt>,
    last_desired: Option<WorldReplyV4>,
    /// Last delivery's per-command settlement rows, index-aligned with the
    /// commands the core executed (inline command first, then any queued
    /// batch). Retained for correlated command results; the reply carries no
    /// `command_result` in v4.
    last_settlements: Vec<BatchSettlement>,
    last_command_seq: Option<u64>,
    last_accepted_obs: u64,
    last_accepted_rev: u64,
    last_accepted_req: String,
    has_accepted: bool,
    observation_only_catchup: bool,
}

impl WorldSequencer {
    /// Test/offline bootstrap: binds the explicit adapter/core identities
    /// and constructs the shared engine. `None` on any malformed
    /// identity. The first full observation is immediately dirty; readiness
    /// arrives with the first applied acknowledgement.
    ///
    /// This entry keeps the historical empty initial snapshot (core
    /// defaults plus `Missing` diagnostics) for offline fixtures. Production
    /// startup uses [`WorldSequencer::bootstrap_with_settings`] so
    /// restart-only `mode` and every other core key adopt at revision 1.
    #[cfg(test)]
    #[must_use]
    pub(super) fn bootstrap(adapter_incarnation: &str, core_generation: &str) -> Option<Self> {
        Self::bootstrap_inner(adapter_incarnation, core_generation, &[], None)
    }

    /// Production bootstrap with the validated initial raw settings snapshot
    /// (explicit CLI switches already applied): the genuine handshake binds
    /// the generation and revision 1 accepts the full snapshot, so
    /// restart-only keys adopt at startup rather than latching handshake
    /// defaults until a later revision core would refuse. The accepted reply
    /// (status + diagnostics) is retained for the caller to log.
    #[must_use]
    pub(super) fn bootstrap_with_settings(
        adapter_incarnation: &str,
        core_generation: &str,
        values: &[(String, RawSettingValue)],
    ) -> Option<Self> {
        Self::bootstrap_inner(adapter_incarnation, core_generation, values, None)
    }

    /// Test-only bootstrap with an explicit owner identity, so fixtures owned
    /// by the test process are not excluded as own-process windows.
    #[cfg(test)]
    #[must_use]
    pub(super) fn bootstrap_with_identity(
        adapter_incarnation: &str,
        core_generation: &str,
        identity: tiler_core::connection::ApplicationIdentity,
    ) -> Option<Self> {
        Self::bootstrap_inner(adapter_incarnation, core_generation, &[], Some(identity))
    }

    fn bootstrap_inner(
        adapter_incarnation: &str,
        core_generation: &str,
        values: &[(String, RawSettingValue)],
        #[allow(unused_variables)] identity: Option<tiler_core::connection::ApplicationIdentity>,
    ) -> Option<Self> {
        OwnerId::parse(adapter_incarnation)?;
        GenerationId::parse(core_generation)?;
        let mut engine = WorldEngine::new(core_generation)?;
        // v4 binds through the genuine connection path: one handshake plus
        // the initial revisioned settings exchange. The core mints the
        // generation; a fresh engine's first connection mints exactly the
        // seed, so a different generation is an identity mismatch and
        // refuses bootstrap (no second engine owner is ever created).
        #[cfg(test)]
        let outcome = match identity {
            Some(identity) => super::settings::connect_with_identity(
                &mut engine,
                adapter_incarnation,
                values,
                identity,
            )?,
            None => super::settings::connect(&mut engine, adapter_incarnation, values)?,
        };
        #[cfg(not(test))]
        let outcome = super::settings::connect(&mut engine, adapter_incarnation, values)?;
        if outcome.generation != core_generation {
            return None;
        }
        Some(Self {
            adapter_incarnation: adapter_incarnation.to_owned(),
            core_generation: core_generation.to_owned(),
            engine,
            settings_revision: 2,
            last_settings_values: values.to_vec(),
            initial_settings_reply: Some(outcome.settings),
            current_obs: 0,
            obs_dirty: true,
            next_request_seq: 1,
            next_command_seq: 1,
            flight: None,
            pending_frames: BTreeMap::new(),
            pending_members: BTreeMap::new(),
            pending_views: BTreeMap::new(),
            pending_visibility: BTreeMap::new(),
            last_desired: None,
            last_settlements: Vec::new(),
            last_command_seq: None,
            last_accepted_obs: 0,
            last_accepted_rev: 0,
            last_accepted_req: String::new(),
            has_accepted: false,
            observation_only_catchup: false,
        })
    }

    /// Apply one full raw catalog snapshot at the next revision through the
    /// genuine typed connection route. Accepted revisions advance and become
    /// the reset replay snapshot; stale/reused refusals keep both the last
    /// good state and revision. Returns the core reply (including every
    /// diagnostic), or `None` when the request was unbound.
    pub(super) fn apply_settings_values(
        &mut self,
        values: &[(String, RawSettingValue)],
    ) -> Option<SettingsReply> {
        let revision = self.settings_revision;
        let reply = self.apply_settings_at(revision, values)?;
        if reply.status == SettingsStatus::Accepted {
            self.settings_revision += 1;
            self.last_settings_values = values.to_vec();
        }
        Some(reply)
    }

    /// One typed settings exchange at an explicit revision. Shared by the
    /// monotonic live path and the stale/reused proof seam; only an
    /// `Accepted` reply advances owner state (see `apply_settings_values`).
    fn apply_settings_at(
        &mut self,
        revision: u64,
        values: &[(String, RawSettingValue)],
    ) -> Option<SettingsReply> {
        super::settings::apply_settings_revision(
            &mut self.engine,
            &self.adapter_incarnation,
            &self.core_generation,
            revision,
            values,
        )
    }

    /// Test-only explicit-revision exchange for refused-revision proof
    /// (older revision -> `Stale`, equal revision with different content ->
    /// `RevisionReused`). Never advances the live revision: callers observe
    /// `settings_revision` unchanged.
    #[cfg(test)]
    pub(super) fn apply_settings_at_for_test(
        &mut self,
        revision: u64,
        values: &[(String, RawSettingValue)],
    ) -> Option<SettingsReply> {
        self.apply_settings_at(revision, values)
    }

    /// Take the retained initial (revision 1) settings reply for logging.
    /// `None` until bootstrap succeeds and once already taken.
    pub(super) fn take_initial_settings_reply(&mut self) -> Option<SettingsReply> {
        self.initial_settings_reply.take()
    }

    /// Last revision accepted by the core (2 immediately after bootstrap).
    #[must_use]
    #[cfg(test)]
    pub(super) fn settings_revision(&self) -> u64 {
        self.settings_revision
    }

    /// Test-only live-settings revision: apply the supplied raw catalog
    /// values through the genuine connection route so offline fixtures drive
    /// core policy (switch target, fresh placement) that v4 carries out of
    /// band instead of in the report.
    #[cfg(test)]
    pub(super) fn apply_test_settings(
        &mut self,
        values: &[(&str, tiler_core::connection::RawSettingValue)],
    ) -> bool {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<Vec<_>>();
        self.apply_settings_values(&values)
            .is_some_and(|reply| reply.status == SettingsStatus::Accepted)
    }

    /// Discovery reply for offline tests: the bound generation with
    /// `ready=false` and no applicable desired state. The production
    /// discovery answer is the handshake reply; this keeps the offline
    /// sequencer tests able to prove pre-readiness without a second engine
    /// owner or a fabricated desired snapshot.
    #[must_use]
    #[cfg(test)]
    pub(super) fn bootstrap_reply(&self, request_id: &str) -> WorldReplyV4 {
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: self.adapter_incarnation.clone(),
            core_generation: self.core_generation.clone(),
            request_id: request_id.to_owned(),
            based_on_observation_seq: 0,
            state_revision: 0,
            ready: false,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: tiler_core::world_report::WorldPresentationV4 {
                foreground: String::new(),
                rev: 0,
                suspended_output: None,
                border: tiler_core::world_report::BorderPresentationV4 {
                    visible: false,
                    reason: None,
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: Default::default(),
                pager: Vec::new(),
            },
            intent_desired: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Synchronous invalidation: advance the CURRENT watermark before any
    /// debounce/dispatch, including during a flight. Holds at `u64::MAX`
    /// instead of wrapping; dispatch then refuses until a generation reset.
    pub(super) fn invalidate(&mut self) {
        if let Some(next) = self.current_obs.checked_add(1) {
            self.current_obs = next;
        }
        self.obs_dirty = true;
    }

    /// Stage one mechanical frame attempt for the next report, replacing any
    /// entry for the same window lifetime. The acknowledgement path removes
    /// only entries exactly equal to what the flight delivered.
    pub(super) fn note_frame_attempt(&mut self, attempt: FrameAttempt) {
        self.pending_frames
            .insert(attempt.window.0.clone(), attempt);
    }

    /// Stage one raw membership attempt for the next report, replacing any
    /// entry for the same operation window (same ack semantics as frames).
    pub(super) fn note_member_attempt(&mut self, attempt: MemberAttempt) {
        self.pending_members.insert(
            (attempt.operation_id.clone(), attempt.window.0.clone()),
            attempt,
        );
    }

    /// Host-owned workspace domains (native desktops author views). Windows
    /// binds app-owned logical workspaces, so native view attempts are not a
    /// supported evidence lane and must never ride a report.
    #[must_use]
    pub(super) fn host_owned_workspace_domains(&self) -> bool {
        self.engine
            .capabilities()
            .is_some_and(|caps| caps.host_owned)
    }

    /// Stage one raw view attempt for the next report, replacing any entry
    /// for the same owning view phase (same ack semantics as frames).
    pub(super) fn note_view_attempt(&mut self, attempt: ViewAttempt) {
        self.pending_views
            .insert(attempt.operation_id.clone(), attempt);
    }

    /// Stage one raw visibility attempt for the next report, replacing any
    /// entry for the same window lifetime (same ack semantics as frames).
    pub(super) fn note_visibility_attempt(&mut self, attempt: VisibilityAttempt) {
        self.pending_visibility
            .insert(attempt.window.0.clone(), attempt);
    }

    /// Closes the current capture still obliges: every carried close plus
    /// every unhomed (absent-workspace, uncarried) close. The native map is
    /// the sole authority, so obligations read straight from the capture with
    /// no shadow ack history: they clear only when the native queue applies
    /// an ack receipt. Unhomed facts stay dirty with no extra invalidation.
    #[must_use]
    #[cfg(test)]
    pub(super) fn outstanding_closes(&self, captured: &CapturedWorld) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for id in &captured.closed {
            out.insert(id.0.clone());
        }
        for token in &captured.unhomed_closed {
            out.insert(token.clone());
        }
        out.into_iter().collect()
    }

    /// Dirty means an outstanding OBSERVATION, CLOSE, FRAME, MEMBER, VIEW,
    /// or STATE/VISIBILITY obligation. Event-driven catch-up wants the next
    /// trigger while any holds; nothing here polls or spins.
    #[must_use]
    #[cfg(test)]
    pub(super) fn is_dirty(&self, captured: &CapturedWorld) -> bool {
        self.obs_dirty
            || !self.pending_frames.is_empty()
            || !self.pending_members.is_empty()
            || !self.pending_views.is_empty()
            || !self.pending_visibility.is_empty()
            || !self.outstanding_closes(captured).is_empty()
    }

    /// Typed v4 envelope over accepted capture pieces. Core size/validation
    /// gates stay core-side via the checked admission route; the adapter
    /// never pre-empts them and never encodes JSON transport.
    ///
    /// The report carries no settings or capability projection in v4: the
    /// engine's bound connection state owns both, and the handshake/settings
    /// route set them once. Windows has no native domains, state setter lane,
    /// focus-feedback producer, intent storage, or renderer; visibility
    /// attempts ride only target-matching apply readbacks. Capture holds unreadable
    /// per-output scale inside `held_screens`; failing window facts stay
    /// inside `windows` with `rect: None` plus an incomplete inventory, so
    /// `held_windows` is empty. No restoration ingress exists: every report
    /// is a fresh-observation report and the core mints rings/views from the
    /// live screens.
    #[must_use]
    pub(super) fn assemble(
        &mut self,
        captured: &CapturedWorld,
        request_seq: u64,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
        monotonic_ms: u64,
    ) -> WorldReportV4 {
        let frames: Vec<FrameAttempt> = self.pending_frames.values().cloned().collect();
        let mut member_attempts: Vec<MemberAttempt> =
            self.pending_members.values().cloned().collect();
        member_attempts
            .sort_by(|a, b| (&a.operation_id, &a.window.0).cmp(&(&b.operation_id, &b.window.0)));
        let mut view_attempts: Vec<ViewAttempt> = if self.host_owned_workspace_domains() {
            self.pending_views.values().cloned().collect()
        } else {
            // Native-free views settle from the frozen selection binding;
            // the echo/attempt lane is unsupported evidence here and would
            // be rejected as native-without-capability.
            Vec::new()
        };
        view_attempts.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        let visibility_attempts: Vec<VisibilityAttempt> =
            self.pending_visibility.values().cloned().collect();
        WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: self.adapter_incarnation.clone(),
            core_generation: self.core_generation.clone(),
            observation_seq: self.current_obs,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq,
            command,
            screens: captured.screens.clone(),
            held_screens: captured.held_screens.clone(),
            windows: captured.windows.clone(),
            held_windows: Vec::new(),
            closed: captured.closed.clone(),
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: captured.cursor,
            cursor_hit: captured.cursor_hit.clone(),
            focused_global: captured.focused_global.clone(),
            foreground_known: captured.foreground_known,
            focus_changes: captured.focus_changes.clone(),
            action_foreground: captured.action_foreground.clone(),
            action_cached: captured.action_cached.clone(),
            action_outputs: captured.action_outputs.clone(),
            primary_output: captured.primary_output.clone(),
            inventory_complete: captured.inventory_complete,
            gesture_barrier: captured.gesture_barrier,
            gesture: captured.gesture.clone(),
            monotonic_ms,
            frames,
            member_attempts,
            view_attempts,
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            visibility_attempts,
            focus_feedback: Vec::new(),
            presentation: PresentationRowV4::Valid(captured.presentation.clone()),
            intent_records: Vec::new(),
            renderer_input: None,
        }
    }

    /// Capture source-first from the field-input seam and sequence one typed
    /// report: at most one flight; a caller-supplied explicit command rides
    /// exactly once under the next command sequence, or is refused while
    /// busy. While observation-only catch-up is pending, a supplied command
    /// is refused as [`DispatchError::CatchupRequired`] with nothing
    /// consumed, so the superseded command can never replay: the caller
    /// dispatches the catch-up first and resubmits after. Dirtiness stays
    /// set through dispatch (dirty-until-ack); only an accepted current
    /// acknowledgement clears the observation obligation. Read-only over
    /// the owner inputs: native ledgers and maps cannot mutate through `&`.
    /// The seam (never `&TileLoop`) keeps the runtime borrow split: the
    /// caller holds `&mut WorldOwner` alongside these shared inputs.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dispatch(
        &mut self,
        inputs: &OwnerInputs<'_>,
        observed: Option<&[ObservedWindow]>,
        retained: &[RetainedRow],
        areas: &[MonitorArea],
        foreground_hwnd: u64,
        monotonic_ms: u64,
        command: Option<WorldCommand>,
    ) -> Result<WorldReportV4, DispatchError> {
        if self.flight.is_some() {
            return Err(DispatchError::Busy);
        }
        if self.observation_only_catchup && command.is_some() {
            return Err(DispatchError::CatchupRequired);
        }
        // An exhausted watermark can never sequence another report: holding
        // at max must not silently reuse it.
        if self.current_obs == u64::MAX || self.next_request_seq == u64::MAX {
            return Err(DispatchError::CountersExhausted);
        }
        let command_seq = if command.is_some() {
            if self.next_command_seq == u64::MAX {
                return Err(DispatchError::CountersExhausted);
            }
            Some(self.next_command_seq)
        } else {
            None
        };
        let captured = capture_world(inputs, observed, retained, areas, foreground_hwnd);
        // Attempts for explicitly closed lifetimes never ride: the core
        // prunes them on the close itself.
        for id in &captured.closed {
            self.pending_frames.remove(&id.0);
            self.pending_members
                .retain(|(_, window), _| window != &id.0);
        }
        let request_seq = self.next_request_seq;
        let report = self.assemble(&captured, request_seq, command_seq, command, monotonic_ms);
        // Snapshot the exact native homing for every carried close, read
        // from the same owner inputs as the capture. Entries without a
        // source record ride the report but settle no receipt pair.
        let mut delivered_closed: Vec<(String, PendingClosed)> = Vec::new();
        for id in &report.closed {
            if let Some(homing) = inputs.pending_closed.get(&id.0) {
                delivered_closed.push((id.0.clone(), homing.clone()));
            }
        }
        delivered_closed.sort_by(|a, b| a.0.cmp(&b.0));
        self.flight = Some(SequencerFlight {
            observation_seq: self.current_obs,
            command_seq,
            request_id: report.request_id.clone(),
            delivered_closed,
            delivered_frames: report.frames.clone(),
            delivered_members: report.member_attempts.clone(),
            delivered_views: report.view_attempts.clone(),
            delivered_visibility: report.visibility_attempts.clone(),
        });
        self.next_request_seq += 1;
        if command_seq.is_some() {
            self.next_command_seq += 1;
        }
        self.observation_only_catchup = false;
        Ok(report)
    }

    /// In-process delivery into the sequenced engine (the offline stand-in
    /// for the transport round trip). Core gates -- size, validation,
    /// generation, freshness, command consumption -- run here, unmodified.
    /// The v4 reply carries no command result: the per-command settlement is
    /// retained from the returned dispositions.
    pub(super) fn deliver(&mut self, report: &WorldReportV4) -> WorldOutcome {
        let transition =
            tiler_protocol::world_admission::handle_report_checked(&mut self.engine, report, &[]);
        self.last_command_seq = report.command_seq;
        self.last_settlements = Self::settlements(&transition.dispositions);
        transition.outcome
    }

    /// Correlated result for the inline report command: v4 executes the
    /// inline command first, so its settlement is the first executed row.
    /// `None` when the command was not executed (observation-only,
    /// already-consumed, or a rejected report).
    #[must_use]
    pub(super) fn last_command_result(&self) -> Option<&CommandResult> {
        let seq = self.last_command_seq?;
        self.last_settlements
            .iter()
            .find(|row| row.seq == seq)
            .and_then(|row| row.result.as_ref())
    }

    /// Offer one core reply: acknowledge exactly the delivered facts the core
    /// incorporated, apply desired state only when current, schedule
    /// observation-only catch-up otherwise. Mismatched bindings never consume
    /// the flight.
    pub(super) fn ack(&mut self, reply: &WorldReplyV4) -> AckOutcome {
        let Some(flight) = self.flight.clone() else {
            return AckOutcome::Ignored {
                reason: "no-flight",
            };
        };
        if reply.core_generation != self.core_generation {
            return AckOutcome::Ignored {
                reason: "foreign-generation",
            };
        }
        if reply.adapter_incarnation != self.adapter_incarnation {
            return AckOutcome::Ignored {
                reason: "foreign-adapter",
            };
        }
        if reply.request_id != flight.request_id {
            return AckOutcome::Ignored {
                reason: "wrong-request",
            };
        }
        // The echo must name the dispatched flight's own observation before
        // anything is acknowledged: a reply for another watermark keeps the
        // flight with closes and raw attempts retained, even when the local
        // watermark has since advanced past both.
        if reply.based_on_observation_seq != flight.observation_seq {
            return AckOutcome::Ignored {
                reason: "obs-mismatch",
            };
        }
        if !reply.ready {
            // No incorporation happened: release the flight token, ack
            // nothing, catch up observation-only.
            self.flight = None;
            self.observation_only_catchup = true;
            self.obs_dirty = true;
            return AckOutcome::Stale(AckReceipt {
                adapter_incarnation: self.adapter_incarnation.clone(),
                core_generation: self.core_generation.clone(),
                request_id: flight.request_id,
                closed_acked: Vec::new(),
                frames_acked: Vec::new(),
                members_acked: Vec::new(),
                views_acked: Vec::new(),
            });
        }
        let revision_new = !self.has_accepted || reply.state_revision > self.last_accepted_rev;
        let observation_current = reply.based_on_observation_seq == self.current_obs
            && reply.based_on_observation_seq == flight.observation_seq;
        let receipt = self.ack_flight_obligations(&flight);
        self.flight = None;
        if revision_new && observation_current {
            self.has_accepted = true;
            self.last_accepted_rev = reply.state_revision;
            self.last_accepted_obs = reply.based_on_observation_seq;
            self.last_accepted_req = reply.request_id.clone();
            self.last_desired = Some(reply.clone());
            // Desired views are never fed back as the next report's
            // echoes: echoes carry observed applied state only, so the
            // ack updates no view cache here.
            self.obs_dirty = false;
            self.observation_only_catchup = false;
            AckOutcome::Applied(receipt)
        } else {
            // Stale but correlated: incorporation acknowledged, state and
            // effects deliberately not applied, catch-up scheduled.
            self.observation_only_catchup = true;
            self.obs_dirty = true;
            AckOutcome::Stale(receipt)
        }
    }

    /// Rejection, busy, timeout, or transport loss correlated to the exact
    /// outstanding flight: the matching generation plus request id releases
    /// the flight, retains dirty work (observation, closes, raw attempts), and
    /// schedules observation-only catch-up. A delayed old rejection (another
    /// request, or another generation reusing the same request id) is a
    /// side-effect-free no-op and can never clear a newer flight. Never an
    /// acknowledgement, never a retry. Returns whether a flight was
    /// released.
    #[must_use]
    pub(super) fn note_rejected_or_lost(
        &mut self,
        core_generation: &str,
        request_id: &str,
    ) -> bool {
        let correlated = self.flight.as_ref().is_some_and(|flight| {
            flight.request_id == request_id && self.core_generation == core_generation
        });
        if !correlated {
            return false;
        }
        self.flight = None;
        self.observation_only_catchup = true;
        self.obs_dirty = true;
        true
    }

    /// Read-only fence inspection (production): whether observation-only
    /// catch-up is currently owed. Lets the superseded command leg service
    /// the fence before its single dispatch without arming, clearing, or
    /// consuming anything.
    #[must_use]
    pub(super) fn catchup_required(&self) -> bool {
        self.observation_only_catchup
    }

    /// Force observation-only catch-up (test-only): arms the fence a stale
    /// ack or rejection would set, so latest-only tests can prove a
    /// superseded burst stays native-free through the fence without
    /// fabricating engine divergence. No production caller.
    #[cfg(test)]
    pub(super) fn arm_catchup_for_test(&mut self) {
        self.observation_only_catchup = true;
        self.obs_dirty = true;
    }

    /// Generation reset under an explicit NEW identity: clears sequencer
    /// flight/ordering/desired/command state plus pending raw attempts, and
    /// rebuilds the shared engine. Attempt maps clear
    /// because command sequences restart: a stale entry would collide with a
    /// reused operation identity under the fresh generation. Counters
    /// restart because the core namespaces ordering per generation; they
    /// still never wrap within one. Same-generation and malformed resets are
    /// side-effect-free no-ops (`false`). Native lifetime/close facts are
    /// untouched: the owner is not mutably reachable here. Old arbitrary
    /// replies cannot reset: only this call changes the bound generation.
    /// The last accepted raw settings snapshot is replayed at revision 1
    /// under the fresh generation (restart-only `mode` re-adopts instead of
    /// reverting to handshake defaults), and its reply is retained for
    /// logging.
    /// (No caller yet: the 2b owner cutover drives resets on identity
    /// rotation. Retained as the sole reset path, never a second one.)
    #[allow(dead_code)]
    pub(super) fn reset_generation(&mut self, core_generation: &str) -> bool {
        if core_generation == self.core_generation {
            return false;
        }
        let Some(mut engine) = WorldEngine::new(core_generation) else {
            return false;
        };
        // A rebuilt engine is unbound: it must repeat the genuine v4
        // handshake/settings connection before accepting any report. A
        // refused connection leaves the old generation fully intact.
        let values = self.last_settings_values.clone();
        let Some(connected) =
            super::settings::connect(&mut engine, &self.adapter_incarnation, &values)
        else {
            return false;
        };
        if connected.generation != core_generation {
            return false;
        }
        self.engine = engine;
        self.core_generation = core_generation.to_owned();
        self.settings_revision = 2;
        self.initial_settings_reply = Some(connected.settings);
        self.flight = None;
        self.pending_frames.clear();
        self.pending_members.clear();
        self.pending_views.clear();
        self.pending_visibility.clear();
        self.last_desired = None;
        self.last_settlements.clear();
        self.last_command_seq = None;
        self.has_accepted = false;
        self.last_accepted_obs = 0;
        self.last_accepted_rev = 0;
        self.last_accepted_req.clear();
        self.current_obs = 0;
        self.next_request_seq = 1;
        self.next_command_seq = 1;
        self.observation_only_catchup = false;
        self.obs_dirty = true;
        true
    }

    /// Acknowledge exactly the flight's delivered facts: every carried
    /// `(token, homing)` close pair (read from the flight snapshot, never
    /// re-read from a newer report) plus pending frame-attempt,
    /// member-attempt, and view-attempt entries exactly equal to the
    /// delivered value. A newer update for the same operation, window, or
    /// view phase survives and stays dirty.
    fn ack_flight_obligations(&mut self, flight: &SequencerFlight) -> AckReceipt {
        let mut closed_acked = Vec::new();
        for (token, homing) in &flight.delivered_closed {
            closed_acked.push((token.clone(), homing.clone()));
        }
        let mut frames_acked = Vec::new();
        for delivered in &flight.delivered_frames {
            if self
                .pending_frames
                .get(&delivered.window.0)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_frames.remove(&delivered.window.0);
                frames_acked.push(delivered.window.0.clone());
            }
        }
        let mut members_acked = Vec::new();
        for delivered in &flight.delivered_members {
            let key = (delivered.operation_id.clone(), delivered.window.0.clone());
            if self
                .pending_members
                .get(&key)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_members.remove(&key);
                members_acked.push(key);
            }
        }
        let mut views_acked = Vec::new();
        for delivered in &flight.delivered_views {
            if self
                .pending_views
                .get(&delivered.operation_id)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_views.remove(&delivered.operation_id);
                views_acked.push(delivered.operation_id.clone());
            }
        }
        for delivered in &flight.delivered_visibility {
            if self
                .pending_visibility
                .get(&delivered.window.0)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_visibility.remove(&delivered.window.0);
            }
        }
        AckReceipt {
            adapter_incarnation: self.adapter_incarnation.clone(),
            core_generation: self.core_generation.clone(),
            request_id: flight.request_id.clone(),
            closed_acked,
            frames_acked,
            members_acked,
            views_acked,
        }
    }

    /// Apply an ack receipt to the native close queue (offline caller-side
    /// simulation of the production prune path): remove an entry if and only
    /// if the receipt's namespace still matches this sequencer's current
    /// adapter/core generation AND the current source entry still exactly
    /// equals the delivered `(token, homing)` pair. A receipt held across a
    /// generation reset therefore prunes nothing: the fresh generation has
    /// not incorporated the close, so the source obligation stays dirty and
    /// redelivers. Likewise a newer close or re-homing under the same token
    /// survives an old ack. Returns the pruned tokens. Touches only the
    /// passed map entries, never the desktop.
    #[must_use]
    pub(super) fn apply_close_receipt(
        &self,
        pending: &mut BTreeMap<String, PendingClosed>,
        receipt: &AckReceipt,
    ) -> Vec<String> {
        if receipt.adapter_incarnation != self.adapter_incarnation
            || receipt.core_generation != self.core_generation
        {
            return Vec::new();
        }
        let mut pruned = Vec::new();
        for (token, homing) in &receipt.closed_acked {
            if pending.get(token).is_some_and(|current| current == homing) {
                pending.remove(token);
                pruned.push(token.clone());
            }
        }
        pruned
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn adapter_incarnation(&self) -> &str {
        &self.adapter_incarnation
    }

    #[must_use]
    pub(super) fn core_generation(&self) -> &str {
        &self.core_generation
    }

    #[must_use]
    pub(super) fn current_obs(&self) -> u64 {
        self.current_obs
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn next_request_seq(&self) -> u64 {
        self.next_request_seq
    }

    /// Next command sequence the sequencer will assign. Advances exactly
    /// once per dispatched explicit command (core-only superseded dispatches
    /// included); observation-only catch-up never moves it. Plain `u64`
    /// read (no core lock): test command-ordering proof plus the trace-only
    /// journey diagnostic's sampled/published delivery snapshot.
    #[must_use]
    pub(super) fn next_command_seq(&self) -> u64 {
        self.next_command_seq
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn has_flight(&self) -> bool {
        self.flight.is_some()
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn in_catchup(&self) -> bool {
        self.observation_only_catchup
    }

    /// Readiness: bound identities plus at least one applied acknowledgement.
    #[must_use]
    #[cfg(test)]
    pub(super) fn is_ready(&self) -> bool {
        self.has_accepted
    }

    /// Borrow the sequenced engine (read-only inspection for the runtime
    /// owner; mutation flows through [`WorldSequencer::deliver`]).
    #[must_use]
    pub(super) fn engine_ref(&self) -> &WorldEngine {
        &self.engine
    }

    /// Capture plus one explicit command (single-flight). Thin wrapper over
    /// [`WorldSequencer::try_dispatch`] for the runtime owner call site.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dispatch_with_command(
        &mut self,
        inputs: &OwnerInputs<'_>,
        observed: Option<&[ObservedWindow]>,
        retained: &[RetainedRow],
        areas: &[MonitorArea],
        foreground_hwnd: u64,
        monotonic_ms: u64,
        command: WorldCommand,
    ) -> Result<WorldReportV4, DispatchError> {
        self.try_dispatch(
            inputs,
            observed,
            retained,
            areas,
            foreground_hwnd,
            monotonic_ms,
            Some(command),
        )
    }

    /// Translate sequence-keyed dispositions into the settlement rows the
    /// adapter retains. `Refused` settles as `CommandResult::Rejected` (the
    /// failed command), `Executed(None)` settles ready with no result, and
    /// `AlreadyConsumed`/`NotRun` never executed so they retain no row.
    fn settlements(dispositions: &[BatchDisposition]) -> Vec<BatchSettlement> {
        dispositions
            .iter()
            .filter_map(|settled| {
                let result = match &settled.disposition {
                    CommandDisposition::Executed(result) => result.clone(),
                    CommandDisposition::Refused(kind) => {
                        Some(CommandResult::Rejected { kind: *kind })
                    }
                    CommandDisposition::AlreadyConsumed | CommandDisposition::NotRun => None,
                };
                if matches!(
                    settled.disposition,
                    CommandDisposition::AlreadyConsumed | CommandDisposition::NotRun
                ) {
                    None
                } else {
                    Some(BatchSettlement {
                        seq: settled.seq,
                        result,
                    })
                }
            })
            .collect()
    }

    /// Production delivery: run one report through the sequenced engine and
    /// retain its command settlement. v4 core owns cross-output capacity
    /// handling (a clamped target holds the scope instead of floating a
    /// transferred row), so no adapter-side transfer remap gate exists.
    pub(super) fn deliver_production(&mut self, report: &WorldReportV4) -> WorldOutcome {
        self.deliver(report)
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn last_desired(&self) -> Option<&WorldReplyV4> {
        self.last_desired.as_ref()
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn pending_frames_len(&self) -> usize {
        self.pending_frames.len()
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn flight_command_seq(&self) -> Option<u64> {
        self.flight.as_ref().and_then(|flight| flight.command_seq)
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn last_accepted_obs(&self) -> u64 {
        self.last_accepted_obs
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn last_accepted_rev(&self) -> u64 {
        self.last_accepted_rev
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn last_accepted_req(&self) -> &str {
        &self.last_accepted_req
    }
}
