#![forbid(unsafe_code)]
//! Unit A boundary: production callers land in Unit B, so non-test builds
//! would flag every constructor as dead. Tests own the coverage meanwhile.
#![cfg_attr(not(test), allow(dead_code))]

//! World reply ordering and sequencing primitives (Unit A, additive, offline).
//!
//! Thread-safe adapter-side half of the world request/reply boundary; the
//! core-owned half lives in [`WorldEngine`](tiler_core::world_engine::WorldEngine).
//! No actuation here: this module mints sequences, tracks the single
//! outstanding flight, and gates incoming replies. Unit B consumes
//! [`WorldSequencer::accept_reply`] plus [`WorldSequencer::take_unspent_effects`]
//! on the core-owned engine thread before any native dispatch.
//!
//! Contract (mirrors the KDE `WorldAdapterState` rules in TS form):
//! - The observation watermark ([`ObservationClock`]) is an atomic counter
//!   advanced synchronously on EVERY invalidating native input, including
//!   while a request is outstanding and while a gesture barrier is active.
//!   Merely numbering requests is insufficient: a reply whose
//!   `based_on_observation_seq` no longer equals the CURRENT watermark is
//!   stale, even when its request id matches the flight.
//! - Request and command sequences are distinct high-watermarks, never
//!   conflated. Request ids are `req-<request_seq>`; command identity is
//!   `op-<command_seq>`.
//! - Single-flight, non-queuing: [`try_begin_request`](WorldSequencer::try_begin_request)
//!   refuses while a flight is outstanding. Transport loss clears the flight
//!   but NEVER unspends a claimed command sequence (at-most-once).
//! - A reply applies only when it names the CURRENT bound generation, is
//!   ready, carries a strictly newer `state_revision`, names the outstanding
//!   flight request, and still matches the current watermark. Anything else
//!   refuses with a static reason and changes no state.
//! - One-shot effects are spent BEFORE first native dispatch
//!   ([`take_unspent_effects`](WorldSequencer::take_unspent_effects) marks
//!   each returned effect spent): duplicates and later unrelated replies
//!   carrying the same effect id never reapply.
//! - A new bound generation (re-bootstrap) resets flights, ordering,
//!   spent sets, close-focus accounting, and both counters, exactly like
//!   the core-side incarnation reset. Native windows are untouched (there
//!   are none owned here).
//! - Close focus observes only the coherent LATEST envelope: a stable
//!   request may first arrive on a later envelope (the issuing publication
//!   is skipped, never inferred). One bounded current record plus a
//!   monotonic retired/spent revision fence gates replays; a null envelope
//!   retires the current record so old cached replays refuse as stale. The
//!   record pins the bound handshake session, so a new handshake reusing
//!   the same id/revision never aliases the old one.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tiler_core::directional::WindowId;
#[cfg(test)]
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    CloseFocusRequest, EffectCause, EffectKind, OneShotEffect, WorldCommand, WorldReplyV4,
};

/// Thread-safe observation watermark. `invalidate` advances synchronously on
/// every invalidating input (plain atomics: never blocks, even mid-flight);
/// `take_dirty` drains the dispatch hint. Saturates at `u64::MAX` instead
/// of wrapping: exhaustion re-bootstraps under a new core generation.
#[derive(Debug, Default)]
pub(crate) struct ObservationClock {
    current: AtomicU64,
    dirty: AtomicBool,
}

impl ObservationClock {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Current watermark (the `observation_seq` the next report carries).
    pub(crate) fn get(&self) -> u64 {
        self.current.load(Ordering::SeqCst)
    }

    /// Advance on one invalidating input; returns the new watermark.
    /// Lock-free read-modify-write (never a lost advance under
    /// concurrent observers).
    pub(crate) fn invalidate(&self) -> u64 {
        let prev = self
            .current
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                Some(value.saturating_add(1))
            })
            .expect("closure never refuses");
        self.dirty.store(true, Ordering::SeqCst);
        prev.saturating_add(1)
    }

    /// True when invalidating input arrived since the last drain.
    pub(crate) fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::SeqCst)
    }
}

/// One outstanding engine request: the exact identity a reply must echo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Flight {
    pub request_seq: u64,
    pub observation_seq: u64,
    pub command_seq: Option<u64>,
    pub request_id: String,
}

/// Ticket minted for one report dispatch (owns the borrowed report
/// identity strings Unit B stamps into [`ReportInput`](crate::world_builder::ReportInput)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestTicket {
    pub observation_seq: u64,
    pub request_seq: u64,
    pub command_seq: Option<u64>,
    pub request_id: String,
    pub command: Option<WorldCommand>,
}

/// Freshness verdict for one reply. `Applied` carries the accepted snapshot
/// (Unit B reconciles/actuates from it); `Refused` names the static reason
/// and changes no sequencer state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReplyVerdict {
    Applied(Box<WorldReplyV4>),
    Refused { reason: &'static str },
}

#[derive(Debug)]
struct SequencerState {
    generation: String,
    next_request: u64,
    next_command: u64,
    flight: Option<Flight>,
    last_revision: u64,
    has_accepted: bool,
    spent_effects: BTreeSet<String>,
    spent_commands: BTreeSet<u64>,
    last_reply: Option<WorldReplyV4>,
    close_current: Option<CloseFocusRecord>,
    close_retired_rev: u64,
}

/// One bounded close-focus record: the exact latest envelope tuple plus its
/// spend bit. At most one lives here; older tuples retire into the monotonic
/// `close_retired_rev` fence. The session legs pin the handshake that minted
/// the record: a new handshake reusing the same `close-focus-<rev>` id can
/// never exact-match an old-session record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloseFocusRecord {
    pub id: String,
    pub target: WindowId,
    pub issued_revision: u64,
    pub adapter_incarnation: String,
    pub core_generation: String,
    pub spent: bool,
}

/// Freshness verdict for one observed close-focus envelope. `Fresh` names
/// the now-current unspent record (the caller spends before dispatch);
/// `Stale` refuses an old or same-revision-mismatched replay;
/// `DuplicateSpent` silences a re-observed spent record; `Absent` retires
/// the current record on a null envelope (no close intent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseEnvelope {
    Absent,
    Stale,
    DuplicateSpent,
    Fresh,
}

/// Adapter-side world sequencing. `Send + Sync` so AX/input observers can
/// invalidate the watermark while the engine thread owns dispatch. All
/// interior state sits behind one small mutex; the watermark/dirty atomics
/// stay lock-free so invalidation never blocks on dispatch.
///
/// The clock itself is shared ownership ([`Arc`]): every invalidating
/// input submitter holds the same clock (via [`Shared`](crate::runtime::Shared)),
/// so input arriving while a request is outstanding advances the CURRENT
/// watermark and the in-flight reply refuses as stale instead of actuating
/// over newer input. The flight/effects state stays engine-thread-owned.
#[derive(Debug)]
pub(crate) struct WorldSequencer {
    adapter_incarnation: String,
    clock: Arc<ObservationClock>,
    state: Mutex<SequencerState>,
}

impl WorldSequencer {
    /// Fresh sequencer bound to one core generation. Counters start at 1
    /// (0 is the pre-bootstrap observation baseline).
    pub(crate) fn new(adapter_incarnation: &str, core_generation: &str) -> Self {
        Self::with_clock(
            adapter_incarnation,
            core_generation,
            Arc::new(ObservationClock::new()),
        )
    }

    /// Fresh sequencer sharing an externally-owned watermark clock (the
    /// production route: submitters and the engine thread advance and read
    /// the same current input mark).
    pub(crate) fn with_clock(
        adapter_incarnation: &str,
        core_generation: &str,
        clock: Arc<ObservationClock>,
    ) -> Self {
        Self {
            adapter_incarnation: adapter_incarnation.to_owned(),
            clock,
            state: Mutex::new(SequencerState {
                generation: core_generation.to_owned(),
                next_request: 1,
                next_command: 1,
                flight: None,
                last_revision: 0,
                has_accepted: false,
                spent_effects: BTreeSet::new(),
                spent_commands: BTreeSet::new(),
                last_reply: None,
                close_current: None,
                close_retired_rev: 0,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, SequencerState> {
        // Adapter-side dispatch state is fully reconstructible via
        // re-bootstrap; a poisoned mutex recovers in place rather than
        // wedging the engine thread. Never native actuation here.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Advance the observation watermark on invalidating native input.
    /// Lock-free: always advances, even with a request outstanding.
    pub(crate) fn invalidate(&self) -> u64 {
        self.clock.invalidate()
    }

    /// Current observation watermark.
    pub(crate) fn observation_seq(&self) -> u64 {
        self.clock.get()
    }

    /// Drain the dispatch hint.
    pub(crate) fn take_dirty(&self) -> bool {
        self.clock.take_dirty()
    }

    /// Bound core generation (replies naming anything else refuse).
    pub(crate) fn bound_generation(&self) -> String {
        self.lock().generation.clone()
    }

    /// Outstanding flight, if any.
    pub(crate) fn flight(&self) -> Option<Flight> {
        self.lock().flight.clone()
    }

    /// Last accepted world revision (0 before the first accept).
    pub(crate) fn last_revision(&self) -> u64 {
        self.lock().last_revision
    }

    /// Last applied reply snapshot, if any.
    pub(crate) fn last_reply(&self) -> Option<WorldReplyV4> {
        self.lock().last_reply.clone()
    }

    /// True once `seq` was claimed for a command (at-most-once dispatch).
    pub(crate) fn command_spent(&self, seq: u64) -> bool {
        self.lock().spent_commands.contains(&seq)
    }

    /// Mint one report ticket and open its flight. Captures the CURRENT
    /// watermark (never a stale dispatch copy). `None` while a flight is
    /// outstanding (single-flight, non-queuing) or when a counter is
    /// exhausted (re-bootstrap under a new generation instead of wrapping).
    /// A carried command claims its sequence immediately: even if the
    /// flight is later lost, that sequence never dispatches again.
    pub(crate) fn try_begin_request(&self, command: Option<WorldCommand>) -> Option<RequestTicket> {
        let mut state = self.lock();
        if state.flight.is_some() {
            return None;
        }
        if state.next_request == u64::MAX || (command.is_some() && state.next_command == u64::MAX) {
            return None;
        }
        let request_seq = state.next_request;
        state.next_request = state.next_request.saturating_add(1);
        let command_seq = command.as_ref().map(|_| {
            let seq = state.next_command;
            state.next_command = state.next_command.saturating_add(1);
            state.spent_commands.insert(seq);
            seq
        });
        let ticket = RequestTicket {
            observation_seq: self.clock.get(),
            request_seq,
            command_seq,
            request_id: format!("req-{request_seq}"),
            command,
        };
        state.flight = Some(Flight {
            request_seq: ticket.request_seq,
            observation_seq: ticket.observation_seq,
            command_seq: ticket.command_seq,
            request_id: ticket.request_id.clone(),
        });
        Some(ticket)
    }

    /// Drop the outstanding flight after transport loss or an explicit
    /// supersede. Claimed command sequences stay spent (never replayed);
    /// the next native event redispatches observation-only.
    pub(crate) fn clear_flight(&self) {
        self.lock().flight = None;
        self.clock.dirty.store(true, Ordering::SeqCst);
    }

    /// Gate one world reply. Applies only a current-generation, ready,
    /// newer-revision reply that names the outstanding flight request and
    /// still matches the CURRENT watermark. On apply: records the revision,
    /// clears the flight, and retains the snapshot. Consumed-command replies
    /// (`command_consumed`) still apply as observation (the core already
    /// advanced their revision; the command never re-executes here because
    /// command identity is claimed once at dispatch).
    pub(crate) fn accept_reply(&self, reply: &WorldReplyV4) -> ReplyVerdict {
        let mut state = self.lock();
        if reply.core_generation != state.generation {
            return ReplyVerdict::Refused {
                reason: "generation-mismatch",
            };
        }
        if reply.adapter_incarnation != self.adapter_incarnation {
            return ReplyVerdict::Refused {
                reason: "adapter-mismatch",
            };
        }
        let Some(flight) = state.flight.clone() else {
            return ReplyVerdict::Refused {
                reason: "no-flight",
            };
        };
        if reply.request_id != flight.request_id {
            return ReplyVerdict::Refused {
                reason: "stale-request",
            };
        }
        if !reply.ready {
            return ReplyVerdict::Refused {
                reason: "not-ready",
            };
        }
        if state.has_accepted && reply.state_revision <= state.last_revision {
            return ReplyVerdict::Refused {
                reason: "stale-revision",
            };
        }
        if reply.based_on_observation_seq != self.clock.get()
            || reply.based_on_observation_seq != flight.observation_seq
        {
            // Superseded by newer local input (or a watermark that moved
            // mid-flight): observation-only catchup, never actuation.
            return ReplyVerdict::Refused {
                reason: "stale-observation",
            };
        }
        state.last_revision = reply.state_revision;
        state.has_accepted = true;
        state.flight = None;
        state.last_reply = Some(reply.clone());
        ReplyVerdict::Applied(Box::new(reply.clone()))
    }

    /// Spend one one-shot effect id before its first native dispatch.
    /// Returns false when already spent (the dispatch must be skipped).
    pub(crate) fn spend_effect(&self, id: &str) -> bool {
        self.lock().spent_effects.insert(id.to_owned())
    }

    /// Filter one accepted reply's effects to the never-spent ones, spending
    /// each returned id. Unit B calls this BEFORE dispatch so duplicates and
    /// repeated diffs never reapply.
    pub(crate) fn take_unspent_effects(&self, reply: &WorldReplyV4) -> Vec<OneShotEffect> {
        let mut state = self.lock();
        reply
            .effects
            .iter()
            .filter(|effect| state.spent_effects.insert(effect.id.clone()))
            .cloned()
            .collect()
    }

    /// Re-bootstrap under a new core generation: resets flights, ordering,
    /// spent sets, close-focus accounting, and both counters. The watermark clock is left alone
    /// (it names local input recency, not core state).
    pub(crate) fn bind_generation(&self, core_generation: &str) {
        let mut state = self.lock();
        state.generation = core_generation.to_owned();
        state.next_request = 1;
        state.next_command = 1;
        state.flight = None;
        state.last_revision = 0;
        state.has_accepted = false;
        state.spent_effects.clear();
        state.spent_commands.clear();
        state.last_reply = None;
        state.close_current = None;
        state.close_retired_rev = 0;
    }

    /// Observe one coherent latest close-focus envelope. `None` retires the
    /// current record into the fence (no close intent: never actuate, and
    /// old cached replays later refuse as stale). An exact
    /// `(session, id, target, issued_revision)` match returns `Fresh`
    /// (unspent) or `DuplicateSpent` (spent). A request naming a different
    /// session than the bound handshake refuses as `Stale` (handshake alias:
    /// a new handshake reusing the same id/revision never adopts over the
    /// old session, and an old cached replay never adopts on the new
    /// bound sequencer). Any other same-session tuple adopts only when
    /// strictly newer than both the current record and the fence (stable
    /// requests may first arrive on a later envelope); same-revision
    /// mismatches and older revisions refuse as `Stale`.
    pub(crate) fn observe_close_envelope(
        &self,
        request: Option<&CloseFocusRequest>,
        adapter_incarnation: &str,
        core_generation: &str,
    ) -> CloseEnvelope {
        let mut state = self.lock();
        let Some(req) = request else {
            if let Some(prev) = state.close_current.take() {
                if prev.issued_revision > state.close_retired_rev {
                    state.close_retired_rev = prev.issued_revision;
                }
            }
            return CloseEnvelope::Absent;
        };
        // Session alias fence: only the bound handshake's envelopes adopt.
        // `bind_generation` resets the close accounting, so a same-rev
        // reuse on the new bound sequencer arrives with matching session
        // legs and adopts; an old-session cached replay on the new bound
        // sequencer refuses here without needing the revision fence.
        if adapter_incarnation != self.adapter_incarnation || core_generation != state.generation {
            return CloseEnvelope::Stale;
        }
        if let Some(current) = state.close_current.as_ref() {
            if current.id == req.id
                && current.target == req.target
                && current.issued_revision == req.issued_revision
                && current.adapter_incarnation == adapter_incarnation
                && current.core_generation == core_generation
            {
                if current.spent {
                    return CloseEnvelope::DuplicateSpent;
                }
                return CloseEnvelope::Fresh;
            }
            if req.issued_revision <= state.close_retired_rev {
                return CloseEnvelope::Stale;
            }
            if req.issued_revision <= current.issued_revision {
                return CloseEnvelope::Stale;
            }
            let old = state
                .close_current
                .replace(CloseFocusRecord {
                    id: req.id.clone(),
                    target: req.target.clone(),
                    issued_revision: req.issued_revision,
                    adapter_incarnation: adapter_incarnation.to_owned(),
                    core_generation: core_generation.to_owned(),
                    spent: false,
                })
                .expect("current checked above");
            if old.issued_revision > state.close_retired_rev {
                state.close_retired_rev = old.issued_revision;
            }
            return CloseEnvelope::Fresh;
        }
        if req.issued_revision <= state.close_retired_rev {
            return CloseEnvelope::Stale;
        }
        state.close_current = Some(CloseFocusRecord {
            id: req.id.clone(),
            target: req.target.clone(),
            issued_revision: req.issued_revision,
            adapter_incarnation: adapter_incarnation.to_owned(),
            core_generation: core_generation.to_owned(),
            spent: false,
        });
        CloseEnvelope::Fresh
    }

    /// Spend the current close-focus record by exact id. Marks spent and
    /// raises the fence to the record revision; false on no current
    /// record, id mismatch, or an already-spent record (duplicate spend
    /// blocked). Same-generation reconnects preserve the spent bit (only
    /// `bind_generation` or a null/newer envelope retires it).
    pub(crate) fn spend_close(&self, id: &str) -> bool {
        let mut state = self.lock();
        let Some(current) = state.close_current.as_mut() else {
            return false;
        };
        if current.id != id || current.spent {
            return false;
        }
        current.spent = true;
        let revision = current.issued_revision;
        if revision > state.close_retired_rev {
            state.close_retired_rev = revision;
        }
        true
    }

    /// Whether the given close identity is still the current record: exact
    /// `(id, target, issued_revision, session legs)` match against the live
    /// record. A null envelope, a newer publication, or a session alias
    /// moves or retires the record, so a delayed owner result naming
    /// anything else is stale and must never stage. Spend state is
    /// ignored: the dispatched request spends at dispatch, before its
    /// result arrives.
    pub(crate) fn is_current_close_request(
        &self,
        id: &str,
        target: &tiler_core::directional::WindowId,
        issued_revision: u64,
        adapter_incarnation: &str,
        core_generation: &str,
    ) -> bool {
        let state = self.lock();
        state.close_current.as_ref().is_some_and(|current| {
            current.id == id
                && current.target == *target
                && current.issued_revision == issued_revision
                && current.adapter_incarnation == adapter_incarnation
                && current.core_generation == core_generation
        })
    }
}

/// Mint the stable command operation identity for one command sequence.
#[must_use]
pub(crate) fn command_operation_id(command_seq: u64) -> String {
    format!("op-{command_seq}")
}

/// Mint the stable one-shot effect identity for one focus dispatch.
#[must_use]
pub(crate) fn focus_effect_id(command_seq: u64) -> String {
    format!("effect-focus-{command_seq}")
}

/// Build the at-most-once focus effect the core emits for a focus command
/// (test/fixture helper sharing the production id shape).
#[must_use]
pub(crate) fn focus_effect(command_seq: u64, window: &str, issued_revision: u64) -> OneShotEffect {
    OneShotEffect {
        id: focus_effect_id(command_seq),
        kind: EffectKind::Focus,
        target: WindowId(window.to_owned()),
        issued_revision,
        cause: EffectCause::ExplicitCommand,
        command_seq: Some(command_seq),
        await_shown: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiler_core::world_report::WORLD_CONTRACT_VERSION_V4;

    const INCARNATION: &str = "adapter-1";
    const GENERATION: &str = "gen-1";

    fn empty_presentation() -> tiler_core::world_report::WorldPresentationV4 {
        tiler_core::world_report::WorldPresentationV4 {
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
            underlay: tiler_core::world_presentation::PresentationUnderlay::Hidden {
                base_revision: None,
                reason: tiler_core::world_presentation::GroupHideReason::NoGroup,
            },
            pager: Vec::new(),
        }
    }

    fn reply(
        request_id: &str,
        obs: u64,
        revision: u64,
        ready: bool,
        generation: &str,
    ) -> WorldReplyV4 {
        WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: INCARNATION.to_owned(),
            core_generation: generation.to_owned(),
            request_id: request_id.to_owned(),
            based_on_observation_seq: obs,
            state_revision: revision,
            ready,
            command_consumed: false,
            scopes: Vec::new(),
            workspaces: Vec::new(),
            outputs: Vec::new(),
            windows: Vec::new(),
            transfers: Vec::new(),
            effects: Vec::new(),
            close_focus: None,
            command_result: None,
            gesture_settled: None,
            operation_settled: Vec::new(),
            native_creations: Vec::new(),
            cursor_warp: None,
            presentation: empty_presentation(),
            intent_writes: Vec::new(),
            switch_settled: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn clock_advances_during_outstanding_flight() {
        // The load-bearing watermark rule: invalidating input moves the
        // CURRENT watermark even while a request is outstanding, so the
        // in-flight reply later refuses as stale-observation instead of
        // actuating over newer input.
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let ticket = sequencer.try_begin_request(None).expect("first flight");
        assert_eq!(ticket.observation_seq, 0);
        assert_eq!(sequencer.invalidate(), 1);
        assert_eq!(sequencer.invalidate(), 2);
        assert!(sequencer.take_dirty());
        assert!(!sequencer.take_dirty());
        // The flight reply names dispatch-time obs 0 against watermark 2.
        match sequencer.accept_reply(&reply(&ticket.request_id, 0, 1, true, GENERATION)) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "stale-observation"),
            ReplyVerdict::Applied(_) => panic!("stale watermark must not apply"),
        }
        assert!(
            sequencer.flight().is_some(),
            "refusal keeps no-flight state clean"
        );
        // Fresh observation-only catchup after clearing applies.
        sequencer.clear_flight();
        let catchup = sequencer.try_begin_request(None).expect("catchup flight");
        assert_eq!(catchup.observation_seq, 2);
        match sequencer.accept_reply(&reply(&catchup.request_id, 2, 1, true, GENERATION)) {
            ReplyVerdict::Applied(applied) => assert_eq!(applied.state_revision, 1),
            ReplyVerdict::Refused { reason } => panic!("catchup must apply: {reason}"),
        }
        assert_eq!(sequencer.last_revision(), 1);
    }

    #[test]
    fn single_flight_refuses_overlap_until_cleared() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let first = sequencer.try_begin_request(None).expect("first flight");
        assert!(sequencer.try_begin_request(None).is_none(), "no queueing");
        sequencer.clear_flight();
        let second = sequencer.try_begin_request(None).expect("after clear");
        assert_eq!(
            second.request_seq,
            first.request_seq + 1,
            "requests stay monotonic"
        );
        assert_ne!(second.request_id, first.request_id);
    }

    #[test]
    fn accept_applies_only_current_ready_newer_matching() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        // No flight: nothing applies.
        match sequencer.accept_reply(&reply("req-9", 0, 1, true, GENERATION)) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "no-flight"),
            ReplyVerdict::Applied(_) => panic!("flightless reply must not apply"),
        }
        let ticket = sequencer.try_begin_request(None).expect("flight");
        // Wrong generation / adapter / request / readiness each refuse.
        for (candidate, reason) in [
            (
                reply(&ticket.request_id, ticket.observation_seq, 1, true, "gen-2"),
                "generation-mismatch",
            ),
            (
                reply(
                    &ticket.request_id,
                    ticket.observation_seq,
                    1,
                    false,
                    GENERATION,
                ),
                "not-ready",
            ),
            (
                reply("req-999", ticket.observation_seq, 1, true, GENERATION),
                "stale-request",
            ),
        ] {
            match sequencer.accept_reply(&candidate) {
                ReplyVerdict::Refused { reason: got } => assert_eq!(got, reason),
                ReplyVerdict::Applied(_) => panic!("{reason} must refuse"),
            }
        }
        let mut foreign_adapter = reply(
            &ticket.request_id,
            ticket.observation_seq,
            1,
            true,
            GENERATION,
        );
        foreign_adapter.adapter_incarnation = "adapter-2".to_owned();
        match sequencer.accept_reply(&foreign_adapter) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "adapter-mismatch"),
            ReplyVerdict::Applied(_) => panic!("foreign adapter must not apply"),
        }
        // Happy path applies and clears the flight.
        match sequencer.accept_reply(&reply(
            &ticket.request_id,
            ticket.observation_seq,
            1,
            true,
            GENERATION,
        )) {
            ReplyVerdict::Applied(applied) => {
                assert_eq!(applied.request_id, ticket.request_id);
            }
            ReplyVerdict::Refused { reason } => panic!("happy path must apply: {reason}"),
        }
        assert!(sequencer.flight().is_none());
        assert!(sequencer.last_reply().is_some());
        // Same-or-older revisions never reapply, even with a fresh flight.
        let next = sequencer.try_begin_request(None).expect("next flight");
        match sequencer.accept_reply(&reply(
            &next.request_id,
            next.observation_seq,
            1,
            true,
            GENERATION,
        )) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "stale-revision"),
            ReplyVerdict::Applied(_) => panic!("equal revision must not reapply"),
        }
        match sequencer.accept_reply(&reply(
            &next.request_id,
            next.observation_seq,
            2,
            true,
            GENERATION,
        )) {
            ReplyVerdict::Applied(applied) => assert_eq!(applied.state_revision, 2),
            ReplyVerdict::Refused { reason } => panic!("newer revision must apply: {reason}"),
        }
    }

    #[test]
    fn effects_spend_before_dispatch_at_most_once() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let mut answered = reply("req-1", 0, 1, true, GENERATION);
        answered.effects = vec![focus_effect(1, "1001_1", 1), focus_effect(2, "1001_2", 1)];
        let first = sequencer.take_unspent_effects(&answered);
        assert_eq!(first.len(), 2, "first dispatch spends both");
        assert!(sequencer.take_unspent_effects(&answered).is_empty());
        // A duplicate reply carrying one old plus one new effect dispatches
        // only the new one.
        let mut later = answered.clone();
        later.effects.push(focus_effect(3, "1001_3", 2));
        let third = sequencer.take_unspent_effects(&later);
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].id, focus_effect_id(3));
        // Direct spend refuses repeats too.
        assert!(!sequencer.spend_effect(&focus_effect_id(3)));
        assert!(sequencer.spend_effect("effect-focus-9"));
    }

    #[test]
    fn commands_claim_once_and_survive_transport_loss() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let action = || WorldCommand::SelectTrailing {
            output: Some(tiler_core::directional::OutputId(
                "macos-display-1".to_owned(),
            )),
        };
        let ticket = sequencer.try_begin_request(Some(action())).expect("flight");
        let seq = ticket.command_seq.expect("command claims a sequence");
        assert_eq!(ticket.request_id, format!("req-{}", ticket.request_seq));
        assert!(sequencer.command_spent(seq), "claimed at dispatch");
        // Transport loss drops the flight but never unspends the command:
        // the redispatch is observation-only with a fresh request id.
        sequencer.clear_flight();
        assert!(
            sequencer.command_spent(seq),
            "lost commands never resurrect"
        );
        let retry = sequencer
            .try_begin_request(None)
            .expect("observation-only retry");
        assert!(retry.command_seq.is_none());
        // Identity helpers share the production shape.
        assert_eq!(command_operation_id(seq), format!("op-{seq}"));
        assert_eq!(focus_effect_id(seq), format!("effect-focus-{seq}"));
    }

    fn close_request(id: &str, target: &str, issued_revision: u64) -> CloseFocusRequest {
        CloseFocusRequest {
            id: id.to_owned(),
            target: WindowId(target.to_owned()),
            issued_revision,
            cause: tiler_core::world_report::CloseFocusCause::PositiveClose,
            output: tiler_core::directional::OutputId("macos-display-1".to_owned()),
            workspace: "space-11".to_owned(),
        }
    }

    #[test]
    fn close_latest_only_skips_issuing_publication() {
        // Stable request first arrives on a later envelope: the issuing
        // publication was skipped, never inferred — the later envelope
        // adopts as Fresh.
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-7", "1_2", 7)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Fresh
        );
        assert!(sequencer.spend_close("close-focus-7"));
    }

    #[test]
    fn close_spend_duplicate_blocked_and_reemit_silent() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let req = close_request("close-focus-3", "1_2", 3);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
        assert!(sequencer.spend_close(&req.id));
        assert!(!sequencer.spend_close(&req.id), "duplicate spend blocked");
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, GENERATION),
            CloseEnvelope::DuplicateSpent
        );
        // Re-emission of the spent record stays silent: no second spend.
        assert!(!sequencer.spend_close(&req.id));
    }

    #[test]
    fn close_null_retires_and_blocks_old_cached_replay() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let req = close_request("close-focus-4", "1_2", 4);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
        assert_eq!(
            sequencer.observe_close_envelope(None, INCARNATION, GENERATION),
            CloseEnvelope::Absent
        );
        // Old cached replay after null refuses as stale.
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, GENERATION),
            CloseEnvelope::Stale
        );
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-3", "1_2", 3)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Stale
        );
        // Strictly newer envelope adopts.
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-5", "1_3", 5)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Fresh
        );
    }

    #[test]
    fn close_same_revision_mismatch_refused() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let first = close_request("close-focus-5", "1_2", 5);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&first), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
        // Same revision, different id/target: exact-tuple mismatch refuses.
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-5x", "1_2", 5)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Stale
        );
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-5", "1_9", 5)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Stale
        );
        // The original record still stands unspent.
        assert_eq!(
            sequencer.observe_close_envelope(Some(&first), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
    }

    #[test]
    fn close_fence_monotonic_across_adoptions_and_bind_resets() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let first = close_request("close-focus-2", "1_2", 2);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&first), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
        assert!(sequencer.spend_close(&first.id));
        let second = close_request("close-focus-6", "1_3", 6);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&second), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
        // Retired revisions never reapply, even unspent-looking replays.
        assert_eq!(
            sequencer.observe_close_envelope(Some(&first), INCARNATION, GENERATION),
            CloseEnvelope::Stale
        );
        assert_eq!(
            sequencer.observe_close_envelope(
                Some(&close_request("close-focus-5", "1_2", 5)),
                INCARNATION,
                GENERATION
            ),
            CloseEnvelope::Stale
        );
        // Fresh handshake retires close accounting.
        sequencer.bind_generation("gen-2");
        assert_eq!(
            sequencer.observe_close_envelope(Some(&first), INCARNATION, "gen-2"),
            CloseEnvelope::Fresh,
            "new generation adopts without the old fence"
        );
    }

    #[test]
    fn close_session_alias_never_adopts() {
        // Handshake alias: the same id/revision on a foreign session never
        // adopts, and an old-session cached replay on the bound sequencer
        // refuses — the revision fence alone cannot separate them.
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let req = close_request("close-focus-4", "1_2", 4);
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), "adapter-2", GENERATION),
            CloseEnvelope::Stale,
            "foreign incarnation never adopts"
        );
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, "gen-2"),
            CloseEnvelope::Stale,
            "foreign generation never adopts"
        );
        // Bound session still adopts after the alias refusals.
        assert_eq!(
            sequencer.observe_close_envelope(Some(&req), INCARNATION, GENERATION),
            CloseEnvelope::Fresh
        );
    }

    #[test]
    fn bind_generation_resets_ordering_for_rebootstrap() {
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let ticket = sequencer.try_begin_request(None).expect("flight");
        match sequencer.accept_reply(&reply(
            &ticket.request_id,
            ticket.observation_seq,
            4,
            true,
            GENERATION,
        )) {
            ReplyVerdict::Applied(_) => {}
            ReplyVerdict::Refused { reason } => panic!("must apply: {reason}"),
        }
        sequencer.bind_generation("gen-2");
        assert_eq!(sequencer.bound_generation(), "gen-2");
        assert_eq!(sequencer.last_revision(), 0);
        assert!(sequencer.flight().is_none());
        assert!(sequencer.last_reply().is_none());
        // Old-generation replies refuse after the reset.
        match sequencer.accept_reply(&reply(
            &ticket.request_id,
            ticket.observation_seq,
            5,
            true,
            GENERATION,
        )) {
            ReplyVerdict::Refused { reason } => assert_eq!(reason, "generation-mismatch"),
            ReplyVerdict::Applied(_) => panic!("old generation must not apply"),
        }
        // Counters restart under the new generation.
        let fresh = sequencer.try_begin_request(None).expect("fresh flight");
        assert_eq!(fresh.request_seq, 1);
        assert_eq!(fresh.request_id, "req-1");
    }

    #[test]
    fn bootstrap_carries_no_desired_state() {
        // Pre-handshake engine publishes no desired native writes: an
        // unbound engine (no handshake/settings) refuses the report at the
        // owner gate, so nothing is admitted and no authority is published.
        let mut unbound = WorldEngine::new(GENERATION).expect("valid generation");
        let report = tiler_core::world_report::WorldReportV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: INCARNATION.to_owned(),
            core_generation: GENERATION.to_owned(),
            observation_seq: 0,
            request_seq: 1,
            request_id: "req-1".to_owned(),
            command_seq: None,
            command: None,
            screens: Vec::new(),
            held_screens: Vec::new(),
            windows: Vec::new(),
            held_windows: Vec::new(),
            closed: Vec::new(),
            native: Vec::new(),
            held_native: Vec::new(),
            cursor: None,
            cursor_hit: None,
            focused_global: String::new(),
            foreground_known: false,
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            primary_output: None,
            inventory_complete: false,
            gesture_barrier: false,
            gesture: None,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            held_attempts: Vec::new(),
            state_attempts: Vec::new(),
            focus_feedback: Vec::new(),
            presentation: tiler_core::world_report::PresentationRowV4::Valid(
                tiler_core::world_report::PresentationIngressV4::default(),
            ),
            intent_records: Vec::new(),
            renderer_input: None,
        };
        match unbound.handle_report(&report, &[]).0 {
            tiler_core::world_engine::WorldOutcome::Rejected { .. } => {}
            tiler_core::world_engine::WorldOutcome::Accepted(_)
            | tiler_core::world_engine::WorldOutcome::Duplicate(_)
            | tiler_core::world_engine::WorldOutcome::CommandConsumed(_) => {
                panic!("pre-handshake report must not be admitted")
            }
        }
        // Adapter-side readiness gate mirrors the same invariant: a
        // not-ready bootstrap-shaped reply carries no desired state and
        // never applies, so no native write is authorized pre-readiness.
        let bootstrap = reply("req-bootstrap", 0, 0, false, GENERATION);
        assert!(!bootstrap.ready);
        assert!(bootstrap.windows.is_empty() && bootstrap.transfers.is_empty());
        assert_eq!(bootstrap.core_generation, GENERATION);
        assert!(bootstrap.presentation.pager.is_empty());
        assert!(!bootstrap.presentation.border.visible);
        let sequencer = WorldSequencer::new(INCARNATION, GENERATION);
        let ticket = sequencer.try_begin_request(None).expect("flight");
        assert_eq!(ticket.request_id, "req-1");
        match sequencer.accept_reply(&bootstrap) {
            ReplyVerdict::Refused { reason } => {
                // Stale-request (bootstrap id unbound to the flight) or
                // not-ready both gate pre-readiness writes; either preserves
                // the no-writes invariant.
                assert!(
                    reason == "stale-request" || reason == "not-ready",
                    "unexpected gate: {reason}"
                );
            }
            ReplyVerdict::Applied(_) => panic!("not-ready bootstrap must not apply"),
        }
        assert!(sequencer.last_reply().is_none());
    }

    #[test]
    fn clock_is_thread_safe_and_saturates() {
        // Watermark invalidation from many threads never loses an advance
        // and never wraps: the exact property Unit B relies on when AX and
        // input observers fire during an outstanding engine request.
        use std::sync::Arc;
        let sequencer = Arc::new(WorldSequencer::new(INCARNATION, GENERATION));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let peer = Arc::clone(&sequencer);
            handles.push(std::thread::spawn(move || {
                for _ in 0..250 {
                    peer.invalidate();
                }
            }));
        }
        for handle in handles {
            handle.join().expect("worker joins");
        }
        assert_eq!(sequencer.observation_seq(), 8 * 250);
        assert!(sequencer.take_dirty());
    }
}
