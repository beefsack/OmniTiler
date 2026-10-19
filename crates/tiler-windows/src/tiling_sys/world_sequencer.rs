#![forbid(unsafe_code)]
//! Future in-process adapter sequencing boundary (W1, offline only).
//!
//! Test-only scaffolding for the W1 observation/ordering slice: assembles the
//! typed [`WorldReport`](tiler_core::world_report::WorldReport) envelope from
//! the accepted source-first [`capture_world`](super::world_capture::capture_world)
//! pieces and sequences it through the real [`WorldEngine`](tiler_core::world_engine::WorldEngine)
//! with the Windows constructor profile (`fixed_send_admission` on, core
//! maximized focus fence off). W1 and W2 stay offline until the W3 atomic
//! runtime cutover; this module never wires into production, never touches
//! the running [`TileLoop`](super::TileLoop) owner path, and performs no
//! actuation, JSON transport, polling, or timers.
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
//! - Dirty means an outstanding OBSERVATION, CLOSE, or FEEDBACK obligation,
//!   never the watermark alone: an accepted report whose watermark is current
//!   still stays dirty while newer feedback or an absent-domain close remains
//!   queued, with no extra native invalidation required to notice.
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
//! - Feedback is a sequencer-owned pending map keyed by operation id (no
//!   append-only retry ledger). Acknowledgement removes only entries exactly
//!   equal to what the flight delivered, so a newer update for the same
//!   operation survives its predecessor's ack. Generation reset clears the
//!   map: command sequences restart under the fresh generation, so stale
//!   feedback would collide with reused operation identities.
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
//!   flight/ordering/desired/command/feedback state, rebuilding the engine
//!   under the Windows profile. Counters restart because the core namespaces
//!   ordering per generation; they still never wrap within one. Native
//!   lifetime/close facts are untouched: the owner is not mutably reachable
//!   here. Old arbitrary replies cannot reset: only this call changes the
//!   bound generation.

use super::world_capture::{CapturedWorld, capture_world};
use super::{MonitorArea, ObservedWindow, PendingClosed, RetainedRow, TileLoop};
use std::collections::{BTreeMap, BTreeSet};
use tiler_core::ids::{GenerationId, OwnerId};
use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, WORLD_CONTRACT_VERSION, WorldCommand, WorldReply, WorldReport,
};

/// Windows carrier profile for the future world engine: automatic
/// fixed-float sends admitted (D16), core maximized directional-focus fence
/// off (native adapter fences remain owner-side).
const fn windows_engine_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: true,
        maximized_focus_fence: false,
        aligned_per_output_workspaces: false,
    }
}

/// Product owner identity carried on every sequenced report.
const WINDOWS_OWNER: &str = "tiler-windows";

/// One outstanding sequenced report: the exact facts the core incorporation
/// acknowledgement must settle. Closes and feedback are snapshots of what the
/// dispatched report carried, never a re-read of newer state. Close entries
/// pair each carried token with its native homing read at dispatch time, so
/// the caller can prune if and only if its source entry still matches.
#[derive(Debug, Clone)]
struct SequencerFlight {
    observation_seq: u64,
    command_seq: Option<u64>,
    request_id: String,
    delivered_closed: Vec<(String, PendingClosed)>,
    delivered_feedback: Vec<AttemptFeedback>,
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
    pub(super) feedback_acked: Vec<String>,
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
    pending_feedback: BTreeMap<String, AttemptFeedback>,
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
            pending_feedback: BTreeMap::new(),
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

    /// Stage per-operation actuation feedback for the next report, replacing
    /// any entry for the same operation id. Marks feedback-dirty; the
    /// acknowledgement path decides exact removal.
    pub(super) fn note_feedback(&mut self, feedback: AttemptFeedback) {
        self.pending_feedback
            .insert(feedback.operation_id.clone(), feedback);
    }

    /// Closes the current capture still obliges: every carried domain close
    /// plus every unhomed (absent-domain, uncarried) close. The native map is
    /// the sole authority, so obligations read straight from the capture with
    /// no shadow ack history: they clear only when the native queue applies
    /// an ack receipt. Unhomed facts stay dirty with no extra invalidation.
    #[must_use]
    pub(super) fn outstanding_closes(&self, captured: &CapturedWorld) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for domain in &captured.domains {
            for id in &domain.closed {
                out.insert(id.0.clone());
            }
        }
        for token in &captured.unhomed_closed {
            out.insert(token.clone());
        }
        out.into_iter().collect()
    }

    /// Dirty means an outstanding OBSERVATION, CLOSE, or FEEDBACK obligation.
    /// Event-driven catch-up wants the next trigger while any holds; nothing
    /// here polls or spins.
    #[must_use]
    pub(super) fn is_dirty(&self, captured: &CapturedWorld) -> bool {
        self.obs_dirty
            || !self.pending_feedback.is_empty()
            || !self.outstanding_closes(captured).is_empty()
    }

    /// Typed envelope over accepted capture pieces. Core size/validation
    /// gates stay core-side via [`WorldEngine::handle_report`]; the adapter
    /// never pre-empts them and never encodes JSON transport.
    #[must_use]
    pub(super) fn assemble(
        &self,
        captured: &CapturedWorld,
        request_seq: u64,
        command_seq: Option<u64>,
        command: Option<WorldCommand>,
    ) -> WorldReport {
        let mut feedback: Vec<AttemptFeedback> = self.pending_feedback.values().cloned().collect();
        feedback.sort_by(|a, b| a.operation_id.cmp(&b.operation_id));
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
            workspace_mode: captured.mode,
            topology: captured.topology.clone(),
            bindings: captured.bindings.clone(),
            domains: captured.domains.clone(),
            minimized: captured.minimized.clone(),
            fixed_size_predicate: captured.fixed_size_predicate,
            migration_source_refill: captured.migration_source_refill,
            anchor_fallback: Default::default(),
            no_space_result: Default::default(),
            default_tiled: captured.default_tiled,
            focused_global: captured.focused_global.clone(),
            inventory_complete: captured.inventory_complete,
            gesture_barrier: captured.gesture_barrier,
            feedback,
        }
    }

    /// Capture source-first from the live owner tables and sequence one typed
    /// report: at most one flight; a caller-supplied explicit command rides
    /// exactly once under the next command sequence, or is refused while
    /// busy. While observation-only catch-up is pending, a supplied command
    /// is refused as [`DispatchError::CatchupRequired`] with nothing
    /// consumed, so the superseded command can never replay: the caller
    /// dispatches the catch-up first and resubmits after. Dirtiness stays
    /// set through dispatch (dirty-until-ack); only an accepted current
    /// acknowledgement clears the observation obligation. Read-only over
    /// the owner: native ledgers and maps cannot mutate through `&TileLoop`.
    pub(super) fn try_dispatch(
        &mut self,
        state: &TileLoop,
        observed: Option<&[ObservedWindow]>,
        retained: &[RetainedRow],
        areas: &[MonitorArea],
        foreground_hwnd: u64,
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
        let captured = capture_world(state, observed, retained, areas, foreground_hwnd);
        let request_seq = self.next_request_seq;
        let report = self.assemble(&captured, request_seq, command_seq, command);
        // Snapshot the exact native homing for every carried close, read
        // from the same owner state as the capture. Entries without a
        // source record ride the report but settle no receipt pair.
        let mut delivered_closed: Vec<(String, PendingClosed)> = Vec::new();
        for domain in &report.domains {
            for id in &domain.closed {
                if let Some(homing) = state.pending_closed.get(&id.0) {
                    delivered_closed.push((id.0.clone(), homing.clone()));
                }
            }
        }
        delivered_closed.sort_by(|a, b| a.0.cmp(&b.0));
        self.flight = Some(SequencerFlight {
            observation_seq: self.current_obs,
            command_seq,
            request_id: report.request_id.clone(),
            delivered_closed,
            delivered_feedback: report.feedback.clone(),
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
        // flight with closes and feedback retained, even when the local
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
                feedback_acked: Vec::new(),
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
    /// the flight, retains dirty work (observation, closes, feedback), and
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
    /// flight/ordering/desired/command state plus pending feedback, and
    /// rebuilds the engine under the Windows profile. Feedback must clear
    /// because command sequences restart: a stale entry would collide with a
    /// reused operation identity under the fresh generation. Counters
    /// restart because the core namespaces ordering per generation; they
    /// still never wrap within one. Same-generation and malformed resets are
    /// side-effect-free no-ops (`false`). Native lifetime/close facts are
    /// untouched: the owner is not mutably reachable here. Old arbitrary
    /// replies cannot reset: only this call changes the bound generation.
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
        self.pending_feedback.clear();
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
    /// re-read from a newer report) plus pending feedback entries exactly
    /// equal to the delivered value. A newer update for the same operation
    /// survives and stays dirty.
    fn ack_flight_obligations(&mut self, flight: &SequencerFlight) -> AckReceipt {
        let mut closed_acked = Vec::new();
        for (token, homing) in &flight.delivered_closed {
            closed_acked.push((token.clone(), homing.clone()));
        }
        let mut feedback_acked = Vec::new();
        for delivered in &flight.delivered_feedback {
            if self
                .pending_feedback
                .get(&delivered.operation_id)
                .is_some_and(|pending| pending == delivered)
            {
                self.pending_feedback.remove(&delivered.operation_id);
                feedback_acked.push(delivered.operation_id.clone());
            }
        }
        AckReceipt {
            adapter_incarnation: self.adapter_incarnation.clone(),
            core_generation: self.core_generation.clone(),
            request_id: flight.request_id.clone(),
            closed_acked,
            feedback_acked,
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
    pub(super) fn next_request_seq(&self) -> u64 {
        self.next_request_seq
    }

    #[must_use]
    pub(super) fn next_command_seq(&self) -> u64 {
        self.next_command_seq
    }

    #[must_use]
    pub(super) fn has_flight(&self) -> bool {
        self.flight.is_some()
    }

    #[must_use]
    pub(super) fn in_catchup(&self) -> bool {
        self.observation_only_catchup
    }

    /// Readiness: bound identities plus at least one applied acknowledgement.
    #[must_use]
    pub(super) fn is_ready(&self) -> bool {
        self.has_accepted
    }

    #[must_use]
    pub(super) fn last_desired(&self) -> Option<&WorldReply> {
        self.last_desired.as_ref()
    }

    #[must_use]
    pub(super) fn pending_feedback_len(&self) -> usize {
        self.pending_feedback.len()
    }

    #[must_use]
    pub(super) fn flight_command_seq(&self) -> Option<u64> {
        self.flight.as_ref().and_then(|flight| flight.command_seq)
    }

    #[must_use]
    pub(super) fn last_accepted_obs(&self) -> u64 {
        self.last_accepted_obs
    }

    #[must_use]
    pub(super) fn last_accepted_rev(&self) -> u64 {
        self.last_accepted_rev
    }

    #[must_use]
    pub(super) fn last_accepted_req(&self) -> &str {
        &self.last_accepted_req
    }

    #[must_use]
    pub(super) fn has_pending_feedback(&self) -> bool {
        !self.pending_feedback.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::rmax03_adapter_tests::test_state;
    use super::*;
    use crate::model::ProcessIdentity;
    use crate::tiling::FrameInsets;
    use crate::workspace::WindowKey;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::world_report::{
        AttemptKind, AttemptOutcome, HoldReason, validate_report_reason,
    };

    const DEV1: &str = "\\\\.\\DISPLAY1";
    const DEV2: &str = "\\\\.\\DISPLAY2";
    const EXE: &str = "C:\\Apps\\app.exe";
    const ADAPTER: &str = "test-windows-adapter-1";

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    fn win_key(hwnd: u64, pid: u32, creation: &str) -> WindowKey {
        WindowKey {
            hwnd,
            pid,
            creation: creation.to_owned(),
        }
    }

    fn plain_facts() -> crate::tiling::WindowFacts {
        crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }
    }

    fn observed_window(key: &WindowKey, token: &str, vis: Rect) -> ObservedWindow {
        let outer = Rect {
            x: vis.x - 8,
            y: vis.y - 8,
            w: vis.w + 16,
            h: vis.h + 16,
        };
        ObservedWindow {
            hwnd: key.hwnd,
            token: token.to_owned(),
            outer,
            visible: vis,
            insets: FrameInsets::measure(outer, vis),
            identity: crate::tiling::ObservedTarget {
                hwnd: key.hwnd,
                pid: key.pid,
                process_creation: key.creation.clone(),
                exe_path: EXE.to_owned(),
                user_sid: "S-1-5-21-1000".to_owned(),
                session_id: 1,
                tag: String::new(),
            },
            facts: plain_facts(),
        }
    }

    fn process_identity(key: &WindowKey) -> ProcessIdentity {
        ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: EXE.to_owned(),
        }
    }

    fn two_areas() -> Vec<MonitorArea> {
        vec![
            MonitorArea {
                device: DEV1.to_owned(),
                work: rect(0, 0, 1920, 1040),
                full: rect(0, 0, 1920, 1080),
            },
            MonitorArea {
                device: DEV2.to_owned(),
                work: rect(1920, 0, 1920, 1040),
                full: rect(1920, 0, 1920, 1080),
            },
        ]
    }

    fn synced_state() -> TileLoop {
        let mut state = test_state();
        state.workspaces.ensure_output(DEV1);
        state.workspaces.ensure_output(DEV2);
        state
    }

    fn ws_id(state: &TileLoop, dev: &str, index: usize) -> String {
        state.workspaces.workspace_ids(dev)[index].clone()
    }

    fn admit(
        state: &mut TileLoop,
        key: &WindowKey,
        dev: &str,
        ws: &str,
        token: &str,
        hidden: bool,
    ) {
        assert!(state.workspaces.assign(key.clone(), dev, ws, hidden));
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state
            .member_identity
            .insert(key.clone(), process_identity(key));
    }

    fn bootstrapped() -> WorldSequencer {
        WorldSequencer::bootstrap(ADAPTER, "gen-1").expect("valid identities")
    }

    fn transfer_feedback(op: &str, outcome: AttemptOutcome) -> AttemptFeedback {
        AttemptFeedback {
            operation_id: op.to_owned(),
            kind: AttemptKind::Transfer,
            outcome,
            hold: None,
        }
    }

    fn focus_command(window: &str) -> WorldCommand {
        WorldCommand::Focus {
            window: WindowId(window.to_owned()),
            direction: "left".to_owned(),
        }
    }

    fn send_command(window: &str, output: &str, workspace: &str) -> WorldCommand {
        WorldCommand::Send {
            window: WindowId(window.to_owned()),
            target_output: OutputId(output.to_owned()),
            target_workspace: workspace.to_owned(),
            follow: false,
        }
    }

    /// Two live members with verified fresh frames; returns the sequencer,
    /// owner, and current-tick products.
    fn two_window_rig() -> (
        WorldSequencer,
        TileLoop,
        Vec<ObservedWindow>,
        Vec<MonitorArea>,
    ) {
        let mut state = synced_state();
        let ws1 = ws_id(&state, DEV1, 0);
        let k1 = win_key(1001, 2001, "proc-1");
        let k2 = win_key(1002, 2002, "proc-2");
        admit(&mut state, &k1, DEV1, &ws1, "w1", false);
        admit(&mut state, &k2, DEV1, &ws1, "w2", false);
        let observed = vec![
            observed_window(&k1, "w1", rect(100, 100, 800, 600)),
            observed_window(&k2, "w2", rect(110, 110, 800, 600)),
        ];
        (bootstrapped(), state, observed, two_areas())
    }

    /// Native ledger fingerprint the scaffold must never mutate.
    fn native_fingerprint(state: &TileLoop) -> (Vec<String>, usize, usize, usize) {
        let mut closed: Vec<String> = state.pending_closed.keys().cloned().collect();
        closed.sort();
        (
            closed,
            state.member_tokens.len(),
            state.member_tags.len(),
            state.member_identity.len(),
        )
    }

    fn accepted_reply(seq: &mut WorldSequencer, report: &WorldReport) -> WorldReply {
        match seq.deliver(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn bootstrap_binds_identities_and_first_report_is_dirty() {
        assert!(WorldSequencer::bootstrap("has space", "gen-1").is_none());
        assert!(WorldSequencer::bootstrap(ADAPTER, "GEN-1").is_none());
        assert!(WorldSequencer::bootstrap(ADAPTER, "").is_none());
        let seq = bootstrapped();
        assert_eq!(seq.adapter_incarnation(), ADAPTER);
        assert_eq!(seq.core_generation(), "gen-1");
        assert!(!seq.is_ready());
        assert!(!seq.has_flight());
        // Discovery carries generation/capabilities with ready=false and no
        // applicable desired state.
        let discovery = seq.bootstrap_reply("disc-1");
        assert!(!discovery.ready);
        assert_eq!(discovery.core_generation, "gen-1");
        assert_eq!(discovery.adapter_incarnation, ADAPTER);
        assert!(discovery.windows.is_empty());
        // Bootstrap is immediately dirty: the first full observation is owed.
        let state = synced_state();
        let captured = capture_world(&state, Some(&[]), &[], &two_areas(), 0);
        assert!(seq.is_dirty(&captured));
    }

    #[test]
    fn generation_handshake_reset_and_no_native_mutation() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );
        let before = native_fingerprint(&state);
        // Invalidation advances before dispatch and the report binds the
        // explicit current identities.
        assert_eq!(seq.current_obs(), 0);
        seq.invalidate();
        assert_eq!(seq.current_obs(), 1);
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(validate_report_reason(&report), None);
        assert_eq!(report.owner, "tiler-windows");
        assert_eq!(report.adapter_incarnation, ADAPTER);
        assert_eq!(report.core_generation, "gen-1");
        assert_eq!(report.observation_seq, 1);
        assert_eq!(report.request_seq, 1);
        assert_eq!(report.request_id, "req-1");
        assert!(seq.has_flight());
        // Single flight: a second dispatch while outstanding is busy, and no
        // quiet queue forms behind it.
        assert_eq!(
            seq.try_dispatch(&state, Some(&observed), &[], &areas, 0, None),
            Err(DispatchError::Busy)
        );
        let reply = accepted_reply(&mut seq, &report);
        assert_eq!(reply.based_on_observation_seq, 1);
        let homing = PendingClosed {
            output: DEV1.to_owned(),
            workspace: ws1.clone(),
        };
        let receipt = match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(receipt.request_id, "req-1");
                // The receipt carries the exact captured (token, homing)
                // pair, never a bare token, bound to this namespace.
                assert_eq!(receipt.adapter_incarnation, ADAPTER);
                assert_eq!(receipt.core_generation, "gen-1");
                assert_eq!(receipt.closed_acked, vec![("w9".to_owned(), homing)]);
                receipt
            }
            other => panic!("expected applied, got {other:?}"),
        };
        assert!(seq.is_ready());
        assert_eq!(seq.last_desired().expect("desired").state_revision, 1);
        assert_eq!(seq.last_accepted_obs(), 1);
        assert_eq!(seq.last_accepted_rev(), 1);
        assert_eq!(seq.last_accepted_req(), "req-1");
        // The scaffold alone prunes nothing: the close stays dirty until the
        // native caller applies the receipt to its own queue.
        assert_eq!(native_fingerprint(&state), before);
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&captured), vec!["w9".to_owned()]);
        assert!(seq.is_dirty(&captured));
        // Caller-side receipt application prunes exactly the delivered pair,
        // and repeated post-prune reports carry no close.
        let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
        assert_eq!(pruned, vec!["w9".to_owned()]);
        assert!(state.pending_closed.is_empty());
        let clean = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert!(seq.outstanding_closes(&clean).is_empty());
        assert!(!seq.is_dirty(&clean));
        // Generation reset clears flight/ordering/desired/command/feedback
        // state and restarts counters under the fresh namespace; the pruned
        // native close does not re-deliver.
        assert!(seq.reset_generation("gen-2"));
        assert!(!seq.has_flight());
        assert!(seq.last_desired().is_none());
        assert!(!seq.is_ready());
        assert_eq!(seq.core_generation(), "gen-2");
        // The pruned native close does not re-deliver, but the reset itself
        // owes a fresh full observation.
        let recaptured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert!(seq.outstanding_closes(&recaptured).is_empty());
        assert!(seq.is_dirty(&recaptured));
        // Same-generation and malformed resets are side-effect-free no-ops.
        assert!(!seq.reset_generation("gen-2"));
        assert_eq!(seq.core_generation(), "gen-2");
        assert_eq!(seq.next_request_seq(), 1);
        assert_eq!(seq.current_obs(), 0);
        // An old arbitrary reply can neither apply nor reset the generation.
        // The delayed old req-1 (gen-1) cannot match the new gen-2 req-1.
        seq.invalidate();
        let report2 = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch under gen-2");
        assert_eq!(report2.request_seq, 1);
        assert_eq!(report2.request_id, "req-1");
        assert_eq!(report2.core_generation, "gen-2");
        assert!(
            report2
                .domains
                .iter()
                .all(|domain| domain.closed.is_empty())
        );
        assert!(!seq.reset_generation("gen-2"));
        assert!(seq.has_flight());
        assert_eq!(seq.next_request_seq(), 2);
        assert!(!seq.reset_generation("GEN-BAD"));
        assert_eq!(seq.core_generation(), "gen-2");
        match seq.ack(&reply) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "foreign-generation"),
            other => panic!("expected foreign-generation ignore, got {other:?}"),
        }
        assert_eq!(seq.core_generation(), "gen-2");
        assert!(seq.has_flight());
        let reply2 = accepted_reply(&mut seq, &report2);
        match seq.ack(&reply2) {
            AckOutcome::Applied(receipt) => {
                assert!(receipt.closed_acked.is_empty());
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
        assert!(state.pending_closed.is_empty());
        assert_eq!(
            (
                state.member_tokens.len(),
                state.member_tags.len(),
                state.member_identity.len()
            ),
            (before.1, before.2, before.3)
        );
    }

    #[test]
    fn stale_watermark_revision_and_request_never_consume_flight() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let report1 = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        // Racing native input advances the watermark while the flight is out.
        seq.invalidate();
        assert_eq!(seq.current_obs(), 2);
        let reply1 = accepted_reply(&mut seq, &report1);
        // Stale but correlated: incorporation acknowledged, state/effects
        // NOT applied, catch-up scheduled.
        match seq.ack(&reply1) {
            AckOutcome::Stale(_) => {}
            other => panic!("expected stale, got {other:?}"),
        }
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        assert!(seq.last_desired().is_none());
        assert!(!seq.is_ready());
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert!(seq.is_dirty(&captured));
        // Catch-up refuses a NEW explicit command without consuming
        // anything: no counters, no flight, no flags. The caller dispatches
        // the catch-up observation first and resubmits after; the superseded
        // command never replays.
        assert_eq!(
            seq.try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            ),
            Err(DispatchError::CatchupRequired)
        );
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        assert_eq!(seq.next_request_seq(), 2);
        assert_eq!(seq.next_command_seq(), 1);
        let catchup = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up dispatch");
        assert!(catchup.command.is_none());
        assert!(catchup.command_seq.is_none());
        assert_eq!(catchup.observation_seq, 2);
        // Replies naming another request, a foreign generation, or a foreign
        // adapter consume nothing and keep the flight. These mutated clones
        // are never delivered; only the true reply below reaches the core.
        let reply2 = accepted_reply(&mut seq, &catchup);
        let mut wrong = reply2.clone();
        wrong.request_id = "req-999".to_owned();
        match seq.ack(&wrong) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "wrong-request"),
            other => panic!("expected wrong-request ignore, got {other:?}"),
        }
        assert!(seq.has_flight());
        // A foreign-generation reply likewise acks nothing.
        let mut foreign = reply2.clone();
        foreign.core_generation = "gen-9".to_owned();
        match seq.ack(&foreign) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "foreign-generation"),
            other => panic!("expected foreign-generation ignore, got {other:?}"),
        }
        assert!(seq.has_flight());
        // A foreign-adapter reply is equally powerless.
        let mut alien = reply2.clone();
        alien.adapter_incarnation = "other-adapter-1".to_owned();
        match seq.ack(&alien) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "foreign-adapter"),
            other => panic!("expected foreign-adapter ignore, got {other:?}"),
        }
        assert!(seq.has_flight());
        // The true reply applies and clears catch-up.
        match seq.ack(&reply2) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(!seq.in_catchup());
        assert!(seq.is_ready());
        // With no flight, stray replies are ignored, never applied.
        match seq.ack(&reply2) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "no-flight"),
            other => panic!("expected no-flight ignore, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_and_consumed_commands_run_at_most_once() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let report = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("dispatch");
        assert_eq!(report.command_seq, Some(1));
        assert_eq!(validate_report_reason(&report), None);
        let reply = accepted_reply(&mut seq, &report);
        assert_eq!(reply.effects.len(), 1);
        assert_eq!(reply.effects[0].id, "fx-1");
        assert_eq!(reply.effects[0].target.0, "w1");
        let revision = reply.state_revision;
        // An identical redelivery duplicates at the same revision: no new
        // revision, no second effect.
        match seq.deliver(&report) {
            WorldOutcome::Duplicate(dup) => assert_eq!(dup.state_revision, revision),
            other => panic!("expected duplicate, got {other:?}"),
        }
        match seq.ack(&reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // The next fresh command sequence executes exactly once.
        seq.invalidate();
        let fresh = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("dispatch");
        assert_eq!(fresh.command_seq, Some(2));
        let fresh_reply = accepted_reply(&mut seq, &fresh);
        assert_eq!(fresh_reply.effects.len(), 1);
        assert_eq!(fresh_reply.effects[0].id, "fx-2");
        match seq.ack(&fresh_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // A fresh report carrying the consumed command sequence reconciles
        // observation but never re-executes the command. Hand-assembled under
        // the next request sequence (the sequencer itself never reuses one),
        // with no flight to acknowledge afterwards.
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        let consumed = seq.assemble(
            &captured,
            seq.next_request_seq(),
            Some(1),
            Some(focus_command("w1")),
        );
        match seq.deliver(&consumed) {
            WorldOutcome::CommandConsumed(consumed_reply) => {
                assert!(consumed_reply.command_consumed);
                assert!(consumed_reply.effects.is_empty());
            }
            other => panic!("expected command-consumed, got {other:?}"),
        }
    }

    #[test]
    fn newer_explicit_command_is_never_replayed() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let first = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("dispatch");
        assert_eq!(seq.flight_command_seq(), Some(1));
        // Racing input supersedes the observation before the reply lands.
        seq.invalidate();
        let stale_reply = accepted_reply(&mut seq, &first);
        assert_eq!(stale_reply.effects.len(), 1);
        assert_eq!(stale_reply.effects[0].id, "fx-1");
        match seq.ack(&stale_reply) {
            AckOutcome::Stale(_) => {}
            other => panic!("expected stale, got {other:?}"),
        }
        // The stale effect is never applied as desired state.
        assert!(seq.last_desired().is_none());
        // Catch-up refuses the newer explicit command with nothing
        // consumed; the caller observes first, resubmits after.
        assert_eq!(
            seq.try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w2")),
            ),
            Err(DispatchError::CatchupRequired)
        );
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        assert_eq!(seq.next_request_seq(), 2);
        assert_eq!(seq.next_command_seq(), 2);
        let catchup = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up");
        assert!(catchup.command.is_none());
        let catchup_reply = accepted_reply(&mut seq, &catchup);
        assert!(catchup_reply.effects.is_empty());
        match seq.ack(&catchup_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // The caller resupplies the newer explicit command under a fresh
        // sequence; it executes exactly once.
        seq.invalidate();
        let second = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w2")),
            )
            .expect("dispatch");
        assert_eq!(second.command_seq, Some(2));
        let second_reply = accepted_reply(&mut seq, &second);
        assert_eq!(second_reply.effects.len(), 1);
        assert_eq!(second_reply.effects[0].id, "fx-2");
        assert_eq!(second_reply.effects[0].target.0, "w2");
        match seq.ack(&second_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // Across every applied desired snapshot, fx-1 never appears and
        // fx-2 appears exactly once: the superseded command never replayed.
        let applied = seq.last_desired().expect("desired");
        assert!(applied.effects.iter().all(|fx| fx.id != "fx-1"));
    }

    #[test]
    fn lost_reply_causes_observation_catchup_without_command_retry() {
        let (mut seq, state, observed, areas) = two_window_rig();
        let before = native_fingerprint(&state);
        seq.invalidate();
        let report = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("dispatch");
        // Transport loss: no reply ever arrives. The correlated rejection
        // releases the flight, retains dirty work, acknowledges nothing.
        let req_id = report.request_id.clone();
        assert!(seq.note_rejected_or_lost("gen-1", &req_id));
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        assert!(seq.last_desired().is_none());
        // A fresh explicit command during catch-up is refused without
        // consuming anything: the lost command is not retried automatically
        // and the new one waits for resubmission after catch-up.
        assert_eq!(
            seq.try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            ),
            Err(DispatchError::CatchupRequired)
        );
        assert_eq!(seq.next_request_seq(), 2);
        assert_eq!(seq.next_command_seq(), 2);
        let catchup = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up");
        assert!(catchup.command.is_none());
        assert_eq!(catchup.request_id, "req-2");
        drop(report);
        let reply = accepted_reply(&mut seq, &catchup);
        assert!(reply.effects.is_empty());
        match seq.ack(&reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // The caller resubmits explicitly afterwards; it runs exactly once.
        seq.invalidate();
        let resubmitted = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("resubmit");
        assert_eq!(resubmitted.command_seq, Some(2));
        let resubmitted_reply = accepted_reply(&mut seq, &resubmitted);
        assert_eq!(resubmitted_reply.effects.len(), 1);
        assert_eq!(resubmitted_reply.effects[0].id, "fx-2");
        match seq.ack(&resubmitted_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        assert_eq!(native_fingerprint(&state), before);
    }

    #[test]
    fn counter_exhaustion_requires_generation_reset_without_wrap() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let first = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(first.request_seq, 1);
        let reply = accepted_reply(&mut seq, &first);
        match seq.ack(&reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // Drive the request counter to exhaustion inside this generation.
        seq.next_request_seq = u64::MAX;
        assert_eq!(
            seq.try_dispatch(&state, Some(&observed), &[], &areas, 0, None),
            Err(DispatchError::CountersExhausted)
        );
        // No wrap: the counter still reads max and invalidation holds there.
        assert_eq!(seq.next_request_seq(), u64::MAX);
        seq.current_obs = u64::MAX;
        seq.invalidate();
        assert_eq!(seq.current_obs(), u64::MAX);
        // An exhausted watermark refuses dispatch too: holding at max must
        // not silently reuse it for a later report.
        seq.next_request_seq = 2;
        assert_eq!(
            seq.try_dispatch(&state, Some(&observed), &[], &areas, 0, None),
            Err(DispatchError::CountersExhausted)
        );
        // Only a generation reset (fresh bootstrap namespace) recovers.
        assert!(seq.reset_generation("gen-2"));
        assert_eq!(seq.next_request_seq(), 1);
        assert_eq!(seq.next_command_seq(), 1);
        assert_eq!(seq.current_obs(), 0);
        seq.invalidate();
        let recovered = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch after reset");
        assert_eq!(recovered.request_seq, 1);
        assert_eq!(recovered.core_generation, "gen-2");
        let recovered_reply = accepted_reply(&mut seq, &recovered);
        match seq.ack(&recovered_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
    }

    #[test]
    fn updated_feedback_after_dispatch_survives_old_ack() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.note_feedback(transfer_feedback("op-1", AttemptOutcome::PossiblyExecuted));
        seq.invalidate();
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(report.feedback.len(), 1);
        assert_eq!(report.feedback[0].outcome, AttemptOutcome::PossiblyExecuted);
        // A partial attempt resolves AFTER dispatch: PossiblyExecuted moves
        // to Applied with no new native invalidation.
        seq.note_feedback(transfer_feedback("op-1", AttemptOutcome::Applied));
        assert_eq!(seq.pending_feedback_len(), 1);
        let reply = accepted_reply(&mut seq, &report);
        match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => {
                // Exact version/value compare: the delivered value no longer
                // matches pending, so nothing is removed.
                assert!(receipt.feedback_acked.is_empty());
            }
            other => panic!("expected applied, got {other:?}"),
        }
        // The newer feedback stays dirty without any manual invalidation:
        // the observation watermark is current, the obligation is not.
        assert!(!seq.obs_dirty);
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert!(seq.is_dirty(&captured));
        // The next report carries the resolved outcome, which then acks.
        let follow = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(follow.feedback.len(), 1);
        assert_eq!(follow.feedback[0].outcome, AttemptOutcome::Applied);
        let follow_reply = accepted_reply(&mut seq, &follow);
        match seq.ack(&follow_reply) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(receipt.feedback_acked, vec!["op-1".to_owned()]);
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(!seq.has_pending_feedback());
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
    }

    #[test]
    fn absent_domain_close_stays_dirty_then_attaches_when_domain_returns() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        // Queue a verified close homed on an unknown domain: uncarried, but
        // the caller retains it.
        state.pending_closed.insert(
            "w7".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: "ws-9".to_owned(),
            },
        );
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(captured.unhomed_closed, vec!["w7".to_owned()]);
        seq.invalidate();
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert!(report.domains.iter().all(|domain| domain.closed.is_empty()));
        let reply = accepted_reply(&mut seq, &report);
        match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => assert!(receipt.closed_acked.is_empty()),
            other => panic!("expected applied, got {other:?}"),
        }
        // The unrelated ack changes nothing for the absent-domain close: it
        // stays dirty with no extra native invalidation.
        assert!(!seq.obs_dirty);
        let recaptured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&recaptured), vec!["w7".to_owned()]);
        assert!(seq.is_dirty(&recaptured));
        // The domain returns: the same close attaches, delivers exactly on
        // the next flight, and is retained until the caller applies the
        // receipt to its own queue.
        assert!(state.workspaces.ensure_workspace_id(DEV1, "ws-9"));
        seq.invalidate();
        let homed = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        let carriers: Vec<String> = homed
            .domains
            .iter()
            .flat_map(|domain| domain.closed.iter().map(|id| id.0.clone()))
            .collect();
        assert_eq!(carriers, vec!["w7".to_owned()]);
        let homed_reply = accepted_reply(&mut seq, &homed);
        match seq.ack(&homed_reply) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(
                    receipt.closed_acked,
                    vec![(
                        "w7".to_owned(),
                        PendingClosed {
                            output: DEV1.to_owned(),
                            workspace: "ws-9".to_owned(),
                        }
                    )]
                );
                let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
                assert_eq!(pruned, vec!["w7".to_owned()]);
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
        assert!(state.pending_closed.is_empty());
    }

    #[test]
    fn close_added_during_flight_stays_pending_until_its_own_ack() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        seq.invalidate();
        let first = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        // A close queues while the flight is out; the dispatched snapshot
        // cannot carry it.
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );
        assert!(first.domains.iter().all(|domain| domain.closed.is_empty()));
        let reply = accepted_reply(&mut seq, &first);
        // The ack settles only exact delivered facts: the newer close is not
        // swept in.
        match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => assert!(receipt.closed_acked.is_empty()),
            other => panic!("expected applied, got {other:?}"),
        }
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&captured), vec!["w9".to_owned()]);
        assert!(seq.is_dirty(&captured));
        // The next flight delivers it exactly; the caller-side receipt
        // application then clears the native obligation.
        let second = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        let carriers: Vec<String> = second
            .domains
            .iter()
            .flat_map(|domain| domain.closed.iter().map(|id| id.0.clone()))
            .collect();
        assert_eq!(carriers, vec!["w9".to_owned()]);
        let reply2 = accepted_reply(&mut seq, &second);
        match seq.ack(&reply2) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(
                    receipt.closed_acked,
                    vec![(
                        "w9".to_owned(),
                        PendingClosed {
                            output: DEV1.to_owned(),
                            workspace: ws1.clone(),
                        }
                    )]
                );
                let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
                assert_eq!(pruned, vec!["w9".to_owned()]);
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
    }

    #[test]
    fn rehomed_close_under_same_token_survives_old_ack() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        let ws2 = ws_id(&state, DEV1, 1);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );
        seq.invalidate();
        let first = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert!(
            first
                .domains
                .iter()
                .any(|domain| domain.closed.iter().any(|id| id.0 == "w9"))
        );
        // Newer native close information re-homes the same token before the
        // ack lands; the dispatched snapshot still names the old homing.
        state
            .pending_closed
            .get_mut("w9")
            .expect("queued")
            .workspace = ws2.clone();
        let reply = accepted_reply(&mut seq, &first);
        match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(
                    receipt.closed_acked,
                    vec![(
                        "w9".to_owned(),
                        PendingClosed {
                            output: DEV1.to_owned(),
                            workspace: ws1.clone(),
                        }
                    )]
                );
                // Exact-equality prune: the re-homed entry survives.
                let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
                assert!(pruned.is_empty());
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert_eq!(
            state.pending_closed.get("w9"),
            Some(&PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws2.clone(),
            })
        );
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&captured), vec!["w9".to_owned()]);
        assert!(seq.is_dirty(&captured));
        // The next flight delivers the current homing, which then prunes.
        let second = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        let homed: Vec<String> = second
            .domains
            .iter()
            .filter(|domain| domain.closed.iter().any(|id| id.0 == "w9"))
            .map(|domain| domain.domain.workspace.0.clone())
            .collect();
        assert_eq!(homed.len(), 1);
        let reply2 = accepted_reply(&mut seq, &second);
        match seq.ack(&reply2) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(
                    receipt.closed_acked,
                    vec![(
                        "w9".to_owned(),
                        PendingClosed {
                            output: DEV1.to_owned(),
                            workspace: ws2.clone(),
                        }
                    )]
                );
                let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
                assert_eq!(pruned, vec!["w9".to_owned()]);
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(state.pending_closed.is_empty());
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
    }

    #[test]
    fn deferred_receipt_cannot_prune_across_generation() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1.clone(),
            },
        );
        seq.invalidate();
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        let reply = accepted_reply(&mut seq, &report);
        let receipt = match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => receipt,
            other => panic!("expected applied, got {other:?}"),
        };
        assert_eq!(receipt.core_generation, "gen-1");
        // The generation resets before the caller applies the receipt: the
        // fresh engine has not incorporated the close.
        assert!(seq.reset_generation("gen-2"));
        // A malformed reset is a side-effect-free no-op: still gen-2.
        assert!(!seq.reset_generation("GEN-BAD"));
        assert_eq!(seq.core_generation(), "gen-2");
        // The deferred old-namespace receipt prunes nothing.
        let pruned = seq.apply_close_receipt(&mut state.pending_closed, &receipt);
        assert!(pruned.is_empty());
        assert!(state.pending_closed.contains_key("w9"));
        // The fresh-generation close obligation remains dirty and the next
        // report carries it again.
        let recaptured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&recaptured), vec!["w9".to_owned()]);
        assert!(seq.is_dirty(&recaptured));
        seq.invalidate();
        let fresh = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(fresh.core_generation, "gen-2");
        assert!(
            fresh
                .domains
                .iter()
                .any(|domain| domain.closed.iter().any(|id| id.0 == "w9"))
        );
        let fresh_reply = accepted_reply(&mut seq, &fresh);
        match seq.ack(&fresh_reply) {
            AckOutcome::Applied(fresh_receipt) => {
                assert_eq!(fresh_receipt.core_generation, "gen-2");
                // Only the current-namespace receipt prunes.
                let pruned = seq.apply_close_receipt(&mut state.pending_closed, &fresh_receipt);
                assert_eq!(pruned, vec!["w9".to_owned()]);
            }
            other => panic!("expected applied, got {other:?}"),
        }
        assert!(state.pending_closed.is_empty());
        assert!(!seq.is_dirty(&capture_world(&state, Some(&observed), &[], &areas, 0)));
    }

    #[test]
    fn core_rejection_is_never_an_acknowledgement() {
        let (mut seq, state, observed, areas) = two_window_rig();
        let before = native_fingerprint(&state);
        seq.invalidate();
        let report = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w1")),
            )
            .expect("dispatch");
        // A caller-supplied command offered while busy is refused outright:
        // explicit commands never silently coalesce into a queue.
        assert_eq!(
            seq.try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                0,
                Some(focus_command("w2")),
            ),
            Err(DispatchError::Busy)
        );
        // Corrupt the generation binding: the core rejects before any
        // mutation, so there is nothing to acknowledge.
        let mut foreign = report.clone();
        foreign.core_generation = "gen-9".to_owned();
        match seq.deliver(&foreign) {
            WorldOutcome::Rejected { .. } => {}
            other => panic!("expected rejected, got {other:?}"),
        }
        // A malformed correlation is likewise refused, never incorporated.
        let mut bad_id = report.clone();
        bad_id.request_id = String::new();
        match seq.deliver(&bad_id) {
            WorldOutcome::Rejected { .. } => {}
            other => panic!("expected rejected, got {other:?}"),
        }
        // The correlated rejection releases exactly this flight and nothing
        // else; the refused command never ran and is never replayed.
        assert!(seq.note_rejected_or_lost("gen-1", &report.request_id));
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        assert!(seq.last_desired().is_none());
        assert!(!seq.has_pending_feedback());
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert!(seq.is_dirty(&captured));
        // The refused command never ran and is never replayed: catch-up is
        // observation-only.
        let catchup = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up");
        assert!(catchup.command.is_none());
        let reply = accepted_reply(&mut seq, &catchup);
        assert!(reply.effects.is_empty());
        assert_eq!(native_fingerprint(&state), before);
    }

    #[test]
    fn delayed_rejection_never_clears_newer_flight() {
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let first = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(first.request_id, "req-1");
        assert!(seq.note_rejected_or_lost("gen-1", "req-1"));
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        // A newer flight is out: the delayed old rejection is a no-op.
        let second = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up");
        assert_eq!(second.request_id, "req-2");
        assert!(!seq.note_rejected_or_lost("gen-1", "req-1"));
        assert!(seq.has_flight());
        assert!(!seq.in_catchup());
        let reply = accepted_reply(&mut seq, &second);
        match seq.ack(&reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // After a generation reset the request namespace restarts, so an
        // old-generation rejection must not match the new req-1 flight.
        assert!(seq.reset_generation("gen-2"));
        seq.invalidate();
        let third = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(third.request_id, "req-1");
        assert!(!seq.note_rejected_or_lost("gen-1", "req-1"));
        assert!(!seq.note_rejected_or_lost("gen-2", "req-9"));
        assert!(seq.has_flight());
        assert!(!seq.in_catchup());
        assert!(seq.note_rejected_or_lost("gen-2", "req-1"));
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
    }

    #[test]
    fn sequencer_never_mutates_native_ledgers_or_maps() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1,
            },
        );
        seq.note_feedback(transfer_feedback("op-9", AttemptOutcome::PossiblyExecuted));
        let before = native_fingerprint(&state);
        // Full lifecycle: dispatch, stale ack, catch-up, applied ack,
        // rejection, generation reset. Every sequencer method takes the
        // owner by shared reference, so the fingerprint must hold.
        seq.invalidate();
        let first = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        seq.invalidate();
        let stale = accepted_reply(&mut seq, &first);
        match seq.ack(&stale) {
            AckOutcome::Stale(_) => {}
            other => panic!("expected stale, got {other:?}"),
        }
        let catchup = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("catch-up");
        let applied = accepted_reply(&mut seq, &catchup);
        match seq.ack(&applied) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // Correlated rejection then generation reset: still only shared
        // references to the owner, so the fingerprint must hold. The reset
        // clears staged feedback, which is sequencer-side and outside the
        // native fingerprint.
        let third = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert!(seq.note_rejected_or_lost("gen-1", &third.request_id));
        assert!(seq.reset_generation("gen-9"));
        assert_eq!(native_fingerprint(&state), before);
        // Hint and snapshot caches capture reads are equally untouched.
        assert!(state.hint_logged.is_empty());
        assert!(state.member_rects.is_empty());
        assert!(state.float_rects.is_empty());
    }

    #[test]
    fn wrong_echo_and_stale_revision_never_apply() {
        let (mut seq, mut state, observed, areas) = two_window_rig();
        let ws1 = ws_id(&state, DEV1, 0);
        state.pending_closed.insert(
            "w9".to_owned(),
            PendingClosed {
                output: DEV1.to_owned(),
                workspace: ws1,
            },
        );
        seq.note_feedback(transfer_feedback("op-9", AttemptOutcome::PossiblyExecuted));
        seq.invalidate();
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(report.feedback.len(), 1);
        let reply = accepted_reply(&mut seq, &report);
        // A reply echoing another watermark keeps the flight with closes
        // and feedback retained: nothing applied, nothing acknowledged.
        let mut wrong_echo = reply.clone();
        wrong_echo.based_on_observation_seq = 999;
        match seq.ack(&wrong_echo) {
            AckOutcome::Ignored { reason } => assert_eq!(reason, "obs-mismatch"),
            other => panic!("expected obs-mismatch ignore, got {other:?}"),
        }
        assert!(seq.has_flight());
        assert!(!seq.in_catchup());
        assert!(seq.has_pending_feedback());
        let captured = capture_world(&state, Some(&observed), &[], &areas, 0);
        assert_eq!(seq.outstanding_closes(&captured), vec!["w9".to_owned()]);
        // The true reply applies and acks exactly the delivered facts.
        match seq.ack(&reply) {
            AckOutcome::Applied(receipt) => {
                assert_eq!(receipt.feedback_acked, vec!["op-9".to_owned()]);
                assert_eq!(receipt.closed_acked.len(), 1);
                assert_eq!(&receipt.closed_acked[0].0, "w9");
            }
            other => panic!("expected applied, got {other:?}"),
        }
        // A revision-stale reply for the current flight never grants
        // applicability either: incorporation receipt, no state, catch-up.
        seq.invalidate();
        let next = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        let fresh = accepted_reply(&mut seq, &next);
        assert!(fresh.state_revision > reply.state_revision);
        let mut stale_rev = fresh.clone();
        stale_rev.state_revision = reply.state_revision;
        match seq.ack(&stale_rev) {
            AckOutcome::Stale(_) => {}
            other => panic!("expected stale, got {other:?}"),
        }
        assert!(!seq.has_flight());
        assert!(seq.in_catchup());
        let desired = seq.last_desired().expect("desired");
        assert_eq!(desired.request_id, "req-1");
        assert_eq!(desired.state_revision, reply.state_revision);
    }

    #[test]
    fn generation_reset_clears_feedback_before_command_reuse() {
        let (mut seq, state, observed, areas) = two_window_rig();
        // Same-output workspace send: the empty second workspace accepts.
        let target_ws = ws_id(&state, DEV1, 1);
        // Generation one: an explicit send retains transfer op-1. The send
        // needs an observed focused window, so the foreground names w1.
        seq.invalidate();
        let send = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                1001,
                Some(send_command("w1", "o1", &target_ws)),
            )
            .expect("dispatch");
        assert_eq!(send.command_seq, Some(1));
        let send_reply = accepted_reply(&mut seq, &send);
        let retained: Vec<String> = seq
            .engine
            .retained_transfers()
            .iter()
            .map(|target| target.operation_id.clone())
            .collect();
        assert_eq!(retained, vec!["op-1".to_owned()]);
        match seq.ack(&send_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // A late refusal for the old generation is staged but must not
        // survive the reset: command sequences restart at one, so it would
        // collide with the fresh op-1.
        seq.note_feedback(transfer_feedback("op-1", AttemptOutcome::RefusedProven));
        assert!(seq.reset_generation("gen-2"));
        assert!(!seq.has_pending_feedback());
        // Generation two reuses op-1 for its own first send; with the stale
        // refusal cleared, the fresh transfer retains.
        seq.invalidate();
        let resend = seq
            .try_dispatch(
                &state,
                Some(&observed),
                &[],
                &areas,
                1001,
                Some(send_command("w1", "o1", &target_ws)),
            )
            .expect("dispatch");
        assert_eq!(resend.command_seq, Some(1));
        assert!(resend.feedback.is_empty());
        let resend_reply = accepted_reply(&mut seq, &resend);
        let retained: Vec<String> = seq
            .engine
            .retained_transfers()
            .iter()
            .map(|target| target.operation_id.clone())
            .collect();
        assert_eq!(retained, vec!["op-1".to_owned()]);
        match seq.ack(&resend_reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        // The collision is real, not hypothetical: redelivering the old
        // refusal settles (kills) the fresh same-identity transfer.
        seq.note_feedback(transfer_feedback("op-1", AttemptOutcome::RefusedProven));
        let probe = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(probe.feedback.len(), 1);
        match seq.deliver(&probe) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
        assert!(seq.engine.retained_transfers().is_empty());
    }

    #[test]
    fn envelope_preserves_windows_profile_and_core_gates() {
        // The sequencer assembles typed reports only; the core's own
        // size/validation gates still decide, with no JSON transport.
        let (mut seq, state, observed, areas) = two_window_rig();
        seq.invalidate();
        let report = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(validate_report_reason(&report), None);
        assert_eq!(report.contract_version, WORLD_CONTRACT_VERSION);
        // Monotonic counters across dispatches within the generation.
        let reply = accepted_reply(&mut seq, &report);
        match seq.ack(&reply) {
            AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        }
        seq.invalidate();
        let second = seq
            .try_dispatch(&state, Some(&observed), &[], &areas, 0, None)
            .expect("dispatch");
        assert_eq!(second.request_seq, 2);
        assert_eq!(second.request_id, "req-2");
        assert_eq!(second.observation_seq, 2);
        assert!(second.request_seq > report.request_seq);
        assert!(second.observation_seq > report.observation_seq);
        // Hold reasons ride through untouched: an unreadable capture still
        // validates and the core holds rather than authorizes.
        let unreadable = capture_world(&state, None, &[], &areas, 0);
        assert!(!unreadable.inventory_complete);
        let held_report = seq.assemble(&unreadable, 99, None, None);
        assert_eq!(validate_report_reason(&held_report), None);
        assert!(
            held_report
                .domains
                .iter()
                .all(|domain| domain.hold == Some(HoldReason::IncompleteInventory))
        );
    }
}
