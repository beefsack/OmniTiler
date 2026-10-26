#![forbid(unsafe_code)]
//! In-process adapter sequencing boundary for the runtime world owner.
//!
//! Assembles the
//! typed [`WorldReport`](tiler_core::world_report::WorldReport) envelope from
//! the accepted source-first [`capture_world`](super::world_capture::capture_world)
//! pieces and sequences it through the real [`WorldEngine`](tiler_core::world_engine::WorldEngine)
//! with the Windows constructor profile (`fixed_send_admission` on, core
//! maximized focus fence off). The runtime world owner uses this envelope
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
//! - Dirty means an outstanding OBSERVATION, CLOSE, FRAME, MEMBER, VIEW,
//!   or CLEAR obligation, never the watermark alone: an accepted report
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
//!   under the Windows profile. Counters restart because the core namespaces
//!   ordering per generation; they still never wrap within one. Native
//!   lifetime/close facts are untouched: the owner is not mutably reachable
//!   here. Old arbitrary replies cannot reset: only this call changes the
//!   bound generation.

use super::world_capture::{CapturedWorld, OwnerInputs, capture_world};
use super::{MonitorArea, ObservedWindow, PendingClosed, RetainedRow};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;
use tiler_core::ids::{GenerationId, OwnerId};
use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use tiler_core::world_report::{
    ClearAttempt, FrameAttempt, MemberAttempt, ScreenView, ViewAttempt, WORLD_CONTRACT_VERSION,
    WorldCommand, WorldReply, WorldReport,
};

/// Windows carrier profile for the future world engine: automatic
/// fixed-float sends admitted (D16), core maximized directional-focus fence
/// off (native adapter fences remain owner-side).
const fn windows_engine_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: true,
        maximized_focus_fence: false,
    }
}

/// Product owner identity carried on every sequenced report.
const WINDOWS_OWNER: &str = "tiler-windows";

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
    delivered_clears: Vec<ClearAttempt>,
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
    pub(super) clears_acked: Vec<(String, String)>,
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
    /// Latest-wins raw maximize-clear attempts per operation window,
    /// drained into the next report (same ack semantics as members).
    pending_clears: BTreeMap<(String, String), ClearAttempt>,
    last_desired: Option<WorldReply>,
    last_accepted_obs: u64,
    last_accepted_rev: u64,
    last_accepted_req: String,
    has_accepted: bool,
    observation_only_catchup: bool,
}

impl WorldSequencer {
    /// Authenticated bootstrap: binds the explicit adapter/core identities
    /// and constructs the Windows-profile engine. `None` on any malformed
    /// identity. The first full observation is immediately dirty; readiness
    /// arrives with the first applied acknowledgement.
    #[must_use]
    pub(super) fn bootstrap(adapter_incarnation: &str, core_generation: &str) -> Option<Self> {
        OwnerId::parse(adapter_incarnation)?;
        GenerationId::parse(core_generation)?;
        let engine = WorldEngine::new_with_config(core_generation, windows_engine_config())?;
        Some(Self {
            adapter_incarnation: adapter_incarnation.to_owned(),
            core_generation: core_generation.to_owned(),
            engine,
            current_obs: 0,
            obs_dirty: true,
            next_request_seq: 1,
            next_command_seq: 1,
            flight: None,
            pending_frames: BTreeMap::new(),
            pending_members: BTreeMap::new(),
            pending_views: BTreeMap::new(),
            pending_clears: BTreeMap::new(),
            last_desired: None,
            last_accepted_obs: 0,
            last_accepted_rev: 0,
            last_accepted_req: String::new(),
            has_accepted: false,
            observation_only_catchup: false,
        })
    }

    /// Discovery reply: generation/capabilities with `ready=false` and no
    /// applicable desired state, straight from the bound engine.
    #[must_use]
    #[cfg(test)]
    pub(super) fn bootstrap_reply(&self, request_id: &str) -> WorldReply {
        self.engine
            .bootstrap_reply(&self.adapter_incarnation, request_id)
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

    /// Stage one raw view attempt for the next report, replacing any entry
    /// for the same owning view phase (same ack semantics as frames).
    pub(super) fn note_view_attempt(&mut self, attempt: ViewAttempt) {
        self.pending_views
            .insert(attempt.operation_id.clone(), attempt);
    }

    /// Stage one raw maximize-clear attempt for the next report, replacing
    /// any entry for the same operation window (same ack semantics as
    /// members).
    pub(super) fn note_clear_attempt(&mut self, attempt: ClearAttempt) {
        self.pending_clears.insert(
            (attempt.operation_id.clone(), attempt.window.0.clone()),
            attempt,
        );
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

    /// Dirty means an outstanding OBSERVATION, CLOSE, FRAME,
    /// MEMBER, VIEW, or CLEAR obligation. Event-driven catch-up wants the next
    /// trigger while any holds; nothing here polls or spins.
    #[must_use]
    #[cfg(test)]
    pub(super) fn is_dirty(&self, captured: &CapturedWorld) -> bool {
        self.obs_dirty
            || !self.pending_frames.is_empty()
            || !self.pending_members.is_empty()
            || !self.pending_views.is_empty()
            || !self.pending_clears.is_empty()
            || !self.outstanding_closes(captured).is_empty()
    }

    /// Typed envelope over accepted capture pieces. Core size/validation
    /// gates stay core-side via [`WorldEngine::handle_report`]; the adapter
    /// never pre-empts them and never encodes JSON transport.
    ///
    /// View echoes are the adapter's observed applied logical views
    /// (facade readback), never the sequencer's desired cache: in the
    /// absence of an observation the report carries empty (unknown) views
    /// and the core retains the pending flight. Copying the last desired
    /// views back as proof would self-echo desire as observation.
    #[must_use]
    pub(super) fn assemble(
        &mut self,
        captured: &CapturedWorld,
        request_seq: u64,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
        monotonic_ms: u64,
    ) -> WorldReport {
        let frames: Vec<FrameAttempt> = self.pending_frames.values().cloned().collect();
        let mut member_attempts: Vec<MemberAttempt> =
            self.pending_members.values().cloned().collect();
        member_attempts
            .sort_by(|a, b| (&a.operation_id, &a.window.0).cmp(&(&b.operation_id, &b.window.0)));
        let mut view_attempts: Vec<ViewAttempt> = self.pending_views.values().cloned().collect();
        view_attempts.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
        let mut clear_attempts: Vec<ClearAttempt> = self.pending_clears.values().cloned().collect();
        clear_attempts
            .sort_by(|a, b| (&a.operation_id, &a.window.0).cmp(&(&b.operation_id, &b.window.0)));
        // Observed applied views from capture (unknown when the facade has
        // no readback); never the last desired snapshot.
        let views: Vec<ScreenView> = captured.observed_views.clone();
        // No restoration ingress exists: every report is a fresh-observation
        // report and the core mints rings/views from the live screens.
        // Restart truth rides the native marker/recovery lanes, which
        // capture consumes as ordinary facts/lanes on the first observation.
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: WINDOWS_OWNER.to_owned(),
            adapter_incarnation: self.adapter_incarnation.clone(),
            core_generation: self.core_generation.clone(),
            observation_seq: self.current_obs,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq,
            command,
            screens: captured.screens.clone(),
            windows: captured.windows.clone(),
            views,
            closed: captured.closed.clone(),
            workspace_mode: captured.workspace_mode,
            host_owned_workspace_domains: false,
            equal_workspace_count: captured.equal_workspace_count,
            // The Windows adapter never opts into automatic birth clears:
            // the retained admission preamble owns the birth route and the
            // core must not mint a second autonomous lane beside it.
            birth_clear: false,
            native: Vec::new(),
            fixed_size_predicate: captured.fixed_size_predicate,
            migration_source_refill: captured.migration_source_refill,
            anchor_fallback: Default::default(),
            no_space_result: Default::default(),
            born_maximized: captured.born_maximized,
            switch_target: captured.switch_target,
            fresh_placement: captured.fresh_placement,
            focus_follows_cursor: captured.focus_follows_cursor,
            cursor_warp: captured.cursor_warp,
            cursor: captured.cursor,
            cursor_hit: captured.cursor_hit.clone(),
            action_foreground: captured.action_foreground.clone(),
            action_cached: captured.action_cached.clone(),
            action_outputs: captured.action_outputs.clone(),
            default_tiled: captured.default_tiled,
            focused_global: captured.focused_global.clone(),
            inventory_complete: captured.inventory_complete,
            gesture_barrier: captured.gesture_barrier,
            gesture: captured.gesture.clone(),
            monotonic_ms,
            frames,
            member_attempts,
            view_attempts,
            clear_attempts,
            creation_attempts: Vec::new(),
            presentation: captured.presentation.clone(),
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
    ) -> Result<WorldReport, DispatchError> {
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
            delivered_clears: report.clear_attempts.clone(),
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
    pub(super) fn deliver(&mut self, report: &WorldReport) -> WorldOutcome {
        self.engine.handle_report(report)
    }

    /// Offer one core reply: acknowledge exactly the delivered facts the core
    /// incorporated, apply desired state only when current, schedule
    /// observation-only catch-up otherwise. Mismatched bindings never consume
    /// the flight.
    pub(super) fn ack(&mut self, reply: &WorldReply) -> AckOutcome {
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
                clears_acked: Vec::new(),
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

    /// Generation reset under an explicit NEW identity: clears sequencer
    /// flight/ordering/desired/command state plus pending raw attempts, and
    /// rebuilds the engine under the Windows profile. Attempt maps clear
    /// because command sequences restart: a stale entry would collide with a
    /// reused operation identity under the fresh generation. Counters
    /// restart because the core namespaces ordering per generation; they
    /// still never wrap within one. Same-generation and malformed resets are
    /// side-effect-free no-ops (`false`). Native lifetime/close facts are
    /// untouched: the owner is not mutably reachable here. Old arbitrary
    /// replies cannot reset: only this call changes the bound generation.
    /// (No caller yet: the 2b owner cutover drives resets on identity
    /// rotation. Retained as the sole reset path, never a second one.)
    #[allow(dead_code)]
    pub(super) fn reset_generation(&mut self, core_generation: &str) -> bool {
        if core_generation == self.core_generation {
            return false;
        }
        let Some(engine) = WorldEngine::new_with_config(core_generation, windows_engine_config())
        else {
            return false;
        };
        self.engine = engine;
        self.core_generation = core_generation.to_owned();
        self.flight = None;
        self.pending_frames.clear();
        self.pending_members.clear();
        self.pending_views.clear();
        self.pending_clears.clear();
        self.last_desired = None;
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
        let mut clears_acked = Vec::new();
        for delivered in &flight.delivered_clears {
            let key = (delivered.operation_id.clone(), delivered.window.0.clone());
            if self
                .pending_clears
                .get(&key)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_clears.remove(&key);
                clears_acked.push(key);
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
            clears_acked,
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

    #[must_use]
    #[cfg(test)]
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
    ) -> Result<WorldReport, DispatchError> {
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

    /// Evaluate one report on a cloned engine candidate without adopting.
    /// Returns the outcome plus the evaluated candidate: the caller adopts
    /// the candidate exactly once via [`WorldSequencer::adopt_or_deliver`]
    /// (accepted/consumed/duplicate), never re-running the report.
    /// Rejection preserves the committed engine.
    #[must_use]
    pub(super) fn deliver_staged_candidate(
        &self,
        report: &WorldReport,
    ) -> (WorldOutcome, WorldEngine) {
        let mut candidate = self.engine.clone();
        let outcome = candidate.handle_report(report);
        (outcome, candidate)
    }

    /// Adopt a previously staged candidate outcome when it is not a
    /// rejection. Single evaluation: the candidate already ran the report,
    /// so no second `deliver` runs here. Rejection preserves the committed
    /// engine; accepted/consumed/duplicate adopt the candidate exactly once.
    pub(super) fn adopt_or_deliver(&mut self, staged: (WorldOutcome, WorldEngine)) -> WorldOutcome {
        let (outcome, candidate) = staged;
        match outcome {
            WorldOutcome::Rejected { .. } => outcome,
            _ => {
                self.engine = candidate;
                outcome
            }
        }
    }

    /// Production staged delivery (T2): an affected tiled-source cross-output
    /// Window transfer whose staged row floats refuses as `float-no-remap`
    /// with the committed world unchanged. Single evaluation: the exact
    /// report runs once on a cloned candidate; accepted/consumed/duplicate
    /// adopt the candidate, rejection preserves the committed engine.
    /// Unaffected commands run the direct path. Never reads `FloatOrigin`.
    pub(super) fn deliver_production(&mut self, report: &WorldReport) -> WorldOutcome {
        let Some(mover) = Self::staged_mover(report) else {
            return self.deliver(report);
        };
        let (staged, candidate) = self.deliver_staged_candidate(report);
        let declined = matches!(
            &staged,
            WorldOutcome::Accepted(reply) if Self::staged_declines(&mover, report, reply)
        );
        if declined {
            return WorldOutcome::Rejected {
                kind: "float-no-remap",
                message: "cross-output capacity float without remap",
            };
        }
        self.adopt_or_deliver((staged, candidate))
    }

    /// Affected mover for staging, bounded to the T2 scope: direct output
    /// sends, directional sends and moves, plus the equivalent Operation
    /// SendToOutput / cross-output Move. Every other command kind runs the
    /// direct path with no staging; no claim is made here about whether
    /// those other routes can cross outputs.
    fn staged_mover(report: &WorldReport) -> Option<String> {
        match &report.command {
            Some(WorldCommand::Send { window, .. })
            | Some(WorldCommand::SendDirectional { window, .. })
            | Some(WorldCommand::MoveDirectional { window, .. }) => Some(window.0.clone()),
            Some(WorldCommand::Operation(command)) => match command {
                tiler_core::boundary::CoreCommand::SendToOutput { window, .. } => {
                    Some(window.clone())
                }
                tiler_core::boundary::CoreCommand::Move {
                    window,
                    cross_output_transfer,
                    ..
                } if *cross_output_transfer => Some(window.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Functional decline facts only for the pinned mover: report-time tiled
    /// source, cross-output Window transfer under `op-<command_seq>`,
    /// floating staged row. Same-output transfers never decline. Never reads
    /// `float_origin`.
    fn staged_declines(mover: &str, report: &WorldReport, reply: &WorldReply) -> bool {
        let Some(source) = report.windows.iter().find(|fact| fact.window.0 == mover) else {
            return false;
        };
        if source.floating {
            return false;
        }
        let Some(seq) = report.command_seq else {
            return false;
        };
        let operation_id = format!("op-{seq}");
        let Some(transfer) = reply.transfers.iter().find(|target| {
            matches!(
                &target.kind,
                tiler_core::world_report::TransferKind::Window(window) if window.0 == mover
            )
        }) else {
            return false;
        };
        if transfer.target_output == source.output || transfer.operation_id != operation_id {
            return false;
        }
        reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == mover && row.floating)
    }

    #[must_use]
    #[cfg(test)]
    pub(super) fn last_desired(&self) -> Option<&WorldReply> {
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

/// Offline T2 staged delivery (test-only proof; never runtime).
///
/// Thin branch-indicator wrapper over the production
/// [`WorldSequencer::deliver_production`] path: returns the production
/// outcome plus whether the affected-command branch ran. No second staging
/// route exists; the production path owns the single evaluation.
#[cfg(test)]
#[allow(dead_code)]
impl WorldSequencer {
    pub(super) fn deliver_staged(&mut self, report: &WorldReport) -> (WorldOutcome, bool) {
        let staged = Self::staged_mover(report).is_some();
        (self.deliver_production(report), staged)
    }

    /// Test-only read proof: retained-engine revision, transfers, and full
    /// engine debug (session snapshots included) for before/after compare.
    pub(super) fn test_revision(&self) -> u64 {
        self.engine.state_revision()
    }

    pub(super) fn test_retained(&self) -> Vec<tiler_core::world_report::TransferTarget> {
        self.engine.retained_transfers()
    }

    pub(super) fn test_engine_debug(&self) -> String {
        format!("{:?}", self.engine)
    }
}
