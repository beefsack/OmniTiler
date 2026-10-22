#![forbid(unsafe_code)]
//! Single runtime WorldEngine owner (stage 2b, standby: unwired).
//!
//! `TileLoop` owns exactly one [`WorldOwner`]; the owner wraps the adapted
//! [`WorldSequencer`](super::world_sequencer::WorldSequencer) (in-process
//! holder over [`WorldEngine`](tiler_core::world_engine::WorldEngine)).
//! Standby status: the owner bootstraps and sequences offline (holder
//! boundary tests), but no production tick or command route dispatches
//! through it yet and it actuates nothing: the legacy Engine path remains
//! the single active runtime owner until the follow-up cutover unit wires
//! all tick/command routes and deletes the legacy path. No second active
//! owner exists at any point.
//!
//! Native binding/claim/identity/lifetime caches stay adapter-mechanical;
//! the last applied [`WorldReply`](tiler_core::world_report::WorldReply) is a
//! read-only presentation projection, never a second policy.
//!
//! Durable restart: this carrier persists no logical rings. Every report
//! is a fresh-observation report: the core mints rings/views from the live
//! screens, remaining windows admit as genuinely new through the ordinary
//! path, and restart truth rides the native marker/recovery lanes
//! (sticky/float/tile-override window markers plus the hide-claim ledger
//! and the visible-membership lifetime tags), which capture consumes as
//! ordinary facts/lanes on the first observation.
//!
//! T2 staged refusal (production): an affected tiled-source cross-output
//! Window transfer whose staged reply floats the mover refuses as
//! `float-no-remap` with the source world unchanged. The check reads only
//! functional decline facts (report-time tiled source, cross-output Window
//! transfer under the command operation id, floating staged row); it never
//! branches on `FloatOrigin`.
//!
//! Borrow split: every dispatch entry takes the field-input seam
//! (`OwnerInputs`), never `&TileLoop`, so the runtime can hold
//! `&mut WorldOwner` alongside shared owner inputs with disjoint borrows.

use super::world_capture::OwnerInputs;
use super::world_diff::{FreshHost, HostDiff, apply_world_reply};
use super::world_sequencer::{AckOutcome, AckReceipt, DispatchError, WorldSequencer};
use super::{MonitorArea, ObservedWindow, RetainedRow};
use std::collections::{BTreeMap, HashMap};
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{AttemptFeedback, WorldCommand, WorldReply, WorldReport};

/// Single runtime world policy owner: bootstraps and sequences online.
/// All production lifecycle/history/tiling/scope/select/send/migration
/// policy routes through the sequenced [`WorldEngine`]; the host diff
/// applies every accepted reply through [`LiveNative`](super::world_native::LiveNative).
pub(super) struct WorldOwner {
    seq: WorldSequencer,
    diff: HostDiff,
    last_reply: Option<WorldReply>,
    has_accepted: bool,
}

/// Owned core projections for one diff pass (homing, rings, views).
pub(super) type CoreProjections = (
    HashMap<String, (String, String)>,
    HashMap<String, Vec<String>>,
    HashMap<String, String>,
);

impl WorldOwner {
    /// Authenticated bootstrap from explicit id strings (for struct-literal
    /// construction sites where the owner state does not exist yet).
    pub(super) fn bootstrap_ids(owner: &str, generation: &str) -> Option<Self> {
        let seq = WorldSequencer::bootstrap(owner, generation)?;
        let diff = HostDiff::new(owner, generation);
        Some(Self {
            seq,
            diff,
            last_reply: None,
            has_accepted: false,
        })
    }

    #[cfg(test)]
    pub(super) fn is_ready(&self) -> bool {
        self.seq.is_ready()
    }

    /// Borrow the sequenced engine (read-only core logical queries for
    /// routing: rings/views/members/homing; never a second policy).
    pub(super) fn engine(&self) -> &WorldEngine {
        self.seq.engine_ref()
    }

    /// Owned core projections for one diff pass (homing, rings, views).
    pub(super) fn core_projections(&self) -> CoreProjections {
        let engine = self.engine();
        let mut homing = HashMap::new();
        for window in engine.logical_members_all() {
            if let Some((output, workspace)) = engine.adapter_homing(&window) {
                homing.insert(window.0, (output, workspace));
            }
        }
        let mut rings = HashMap::new();
        for output in engine.logical_outputs() {
            rings.insert(output.clone(), engine.logical_ring(&output));
        }
        let mut views = HashMap::new();
        for output in engine.logical_outputs() {
            if let Some(view) = engine.logical_view(&output) {
                views.insert(output, view);
            }
        }
        (homing, rings, views)
    }

    /// Sync the diff freshness gate to the current observation watermark
    /// before one apply pass.
    pub(super) fn sync_diff_gate(&mut self) {
        let obs = self.seq.current_obs();
        self.diff.gate.current_obs = obs;
    }

    /// Record one applied core revision on the diff gate (host results never
    /// move this watermark).
    pub(super) fn note_diff_applied(&mut self, reply: &WorldReply) {
        self.diff.gate.note_applied(reply);
    }

    /// Run one diff pass for an accepted reply. Returns stats plus feedback
    /// rows for the next report. Attempted frame writes append mechanical
    /// rows to `frame_out` for the next report (stamped with `attempt_ms`).
    pub(super) fn apply_accepted(
        &mut self,
        reply: &WorldReply,
        fresh: &FreshHost,
        ops: &mut dyn super::world_diff::NativeOps,
        frame_out: &mut Vec<tiler_core::world_report::FrameAttempt>,
        attempt_ms: u64,
    ) -> (super::world_diff::ApplyStats, Vec<AttemptFeedback>) {
        apply_world_reply(&mut self.diff, reply, fresh, ops, frame_out, attempt_ms)
    }

    pub(super) fn last_reply(&self) -> Option<&WorldReply> {
        self.last_reply.as_ref()
    }

    /// Synchronous invalidation before dispatch (dirty-until-ack).
    pub(super) fn invalidate(&mut self) {
        self.seq.invalidate();
    }

    /// Stage per-operation actuation feedback for the next report.
    pub(super) fn note_feedback(&mut self, feedback: AttemptFeedback) {
        self.seq.note_feedback(feedback);
    }

    /// Stage one mechanical frame attempt for the next report.
    pub(super) fn note_frame_attempt(&mut self, attempt: tiler_core::world_report::FrameAttempt) {
        self.seq.note_frame_attempt(attempt);
    }

    /// No durable restoration ingress exists.
    /// Capture source-first and sequence one typed report. Read-only over
    /// the inputs; the seam (never `&TileLoop`) keeps the `&mut WorldOwner`
    /// borrow split. Capture carries `None`: the core mints rings/views from
    /// the live screens, and restart truth rides ordinary marker facts.
    pub(super) fn try_dispatch_observation(
        &mut self,
        inputs: &OwnerInputs<'_>,
        observed: Option<&[ObservedWindow]>,
        retained: &[RetainedRow],
        areas: &[MonitorArea],
        foreground_hwnd: u64,
        monotonic_ms: u64,
    ) -> Result<WorldReport, DispatchError> {
        self.seq.try_dispatch(
            inputs,
            observed,
            retained,
            areas,
            foreground_hwnd,
            monotonic_ms,
            None,
        )
    }

    /// Capture plus one explicit command, refused while busy or during
    /// observation-only catch-up (caller dispatches the catch-up first).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_dispatch_command(
        &mut self,
        inputs: &OwnerInputs<'_>,
        observed: Option<&[ObservedWindow]>,
        retained: &[RetainedRow],
        areas: &[MonitorArea],
        foreground_hwnd: u64,
        monotonic_ms: u64,
        command: WorldCommand,
    ) -> Result<WorldReport, DispatchError> {
        self.seq.try_dispatch_with_command(
            inputs,
            observed,
            retained,
            areas,
            foreground_hwnd,
            monotonic_ms,
            command,
        )
    }

    /// Production staged delivery for affected commands (T2): single
    /// evaluation through the sequencer-owned path (tiled-source cross-output
    /// Window transfer whose staged row floats refuses as `float-no-remap`
    /// with the source world unchanged). Unaffected commands and
    /// `CommandConsumed` run the direct path. Never reads `FloatOrigin`.
    pub(super) fn deliver_production(&mut self, report: &WorldReport) -> WorldOutcome {
        self.seq.deliver_production(report)
    }

    /// Offer one core reply: acknowledge obligations, apply desired state
    /// only when current, prune native closes on receipt. Returns the ack
    /// outcome plus pruned close tokens for the caller ledger.
    pub(super) fn ack(
        &mut self,
        reply: &WorldReply,
        pending: &mut BTreeMap<String, super::PendingClosed>,
    ) -> (AckOutcome, Vec<String>) {
        let outcome = self.seq.ack(reply);
        match &outcome {
            AckOutcome::Applied(receipt) => {
                self.apply_receipt(reply, receipt, pending);
                let pruned = self.seq.apply_close_receipt(pending, receipt);
                (outcome, pruned)
            }
            AckOutcome::Stale(receipt) => {
                let pruned = self.seq.apply_close_receipt(pending, receipt);
                (outcome, pruned)
            }
            AckOutcome::Ignored { .. } => (outcome, Vec::new()),
        }
    }

    fn apply_receipt(
        &mut self,
        reply: &WorldReply,
        _receipt: &AckReceipt,
        _pending: &BTreeMap<String, super::PendingClosed>,
    ) {
        self.last_reply = Some(reply.clone());
        self.has_accepted = true;
    }

    /// Release one outstanding flight after a proven refusal/loss without
    /// acknowledgement or retry; schedules observation-only catch-up.
    pub(super) fn note_rejected_or_lost(&mut self, request_id: &str) -> bool {
        let generation = self.seq.core_generation().to_owned();
        self.seq.note_rejected_or_lost(&generation, request_id)
    }
    /// Read-only reply-derived presentation projection (pager/border only):
    /// logical views per output. Never a second policy.
    #[cfg(test)]
    pub(super) fn presentation(&self) -> Option<WorldPresentation> {
        let reply = self.last_reply.as_ref()?;
        let mut views: BTreeMap<String, String> = BTreeMap::new();
        for output in &reply.outputs {
            if let Some(view) = output.logical_view.clone() {
                views.insert(output.output.0.clone(), view);
            }
        }
        Some(WorldPresentation {
            views,
            revision: reply.state_revision,
        })
    }
}

#[cfg(test)]
#[path = "world_owner_tests.rs"]
mod world_owner_tests;

/// Read-only presentation snapshot derived from the last applied reply.
#[derive(Debug, Clone, Default)]
#[cfg(test)]
pub(super) struct WorldPresentation {
    pub(super) views: BTreeMap<String, String>,
    pub(super) revision: u64,
}
