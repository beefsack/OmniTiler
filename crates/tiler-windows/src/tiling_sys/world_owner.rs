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
//! the last applied [`WorldReplyV4`](tiler_core::world_report::WorldReplyV4) is a
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
//! v4 core owns cross-output capacity handling: an exhausted target holds
//! its scope (`ConstraintClamp`) instead of floating a transferred row, so
//! production delivery runs the report once through the sequenced engine
//! with no adapter transfer-remap gate.
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
use tiler_core::world_report::{WorldCommand, WorldReplyV4, WorldReportV4};

/// Single runtime world policy owner: bootstraps and sequences online.
/// All production lifecycle/history/tiling/scope/select/send/migration
/// policy routes through the sequenced [`WorldEngine`]; the host diff
/// applies every accepted reply through [`LiveNative`](super::world_native::LiveNative).
pub(super) struct WorldOwner {
    seq: WorldSequencer,
    diff: HostDiff,
    last_reply: Option<WorldReplyV4>,
    /// Bounded accepted trace-proof row for the current `last_reply`
    /// (TRACE-ONLY): the exact `world-apply` projection built from the
    /// accepted reply plus its dispatched report at accept time, even when
    /// the ordinary world log stayed silent. Replaced together with
    /// `last_reply` on every fresh Applied ack under trace; stale, ignored
    /// and duplicate outcomes retain the fallback snapshot. The pager pump
    /// pairs this row with its payload emission only when its
    /// (request, observation, revision) still names the snapshot actually
    /// rendered, so later state can never relabel captured proof. Never a
    /// second policy, never regenerated from current caches.
    last_proof: Option<serde_json::Value>,
    has_accepted: bool,
}

/// Owned core projections for one diff pass (homing, rings, views).
pub(super) type CoreProjections = (
    HashMap<String, (String, String)>,
    HashMap<String, Vec<String>>,
    HashMap<String, String>,
);

impl WorldOwner {
    /// Test/offline bootstrap from explicit id strings (for struct-literal
    /// construction sites where the owner state does not exist yet). Keeps
    /// the historical empty initial settings snapshot; production startup
    /// uses [`WorldOwner::bootstrap_ids_with_settings`].
    #[cfg(test)]
    pub(super) fn bootstrap_ids(owner: &str, generation: &str) -> Option<Self> {
        Self::bootstrap_ids_with_settings(owner, generation, &[])
    }

    /// Production bootstrap with the validated initial raw settings snapshot
    /// (explicit CLI switches already applied) so restart-only `mode` and
    /// every other core key adopt at revision 1.
    pub(super) fn bootstrap_ids_with_settings(
        owner: &str,
        generation: &str,
        values: &[(String, tiler_core::connection::RawSettingValue)],
    ) -> Option<Self> {
        Some(Self::from_sequencer(
            WorldSequencer::bootstrap_with_settings(owner, generation, values)?,
            owner,
            generation,
        ))
    }

    /// Test-only bootstrap whose handshake declares a synthetic owner
    /// identity; live fixture windows remain owned by the test process.
    #[cfg(test)]
    pub(super) fn bootstrap_ids_with_identity(
        owner: &str,
        generation: &str,
        identity: tiler_core::connection::ApplicationIdentity,
    ) -> Option<Self> {
        Some(Self::from_sequencer(
            WorldSequencer::bootstrap_with_identity(owner, generation, identity)?,
            owner,
            generation,
        ))
    }

    fn from_sequencer(seq: WorldSequencer, owner: &str, generation: &str) -> Self {
        let diff = HostDiff::new(owner, generation);
        Self {
            seq,
            diff,
            last_reply: None,
            last_proof: None,
            has_accepted: false,
        }
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

    /// Push the validated settings document into core policy through the
    /// bound connection. One full snapshot per revision; explicit startup
    /// CLI switches override the saved base. Returns the core reply
    /// (status + every diagnostic) so the caller can log acceptance,
    /// refusal, stale/reused revisions, and `restart-required`; `None` when
    /// the connection is unbound. Refusal leaves the last accepted core
    /// state untouched.
    pub(super) fn apply_core_settings(
        &mut self,
        settings: &crate::settings::Settings,
        cli: &crate::tiling::CliOverrides,
    ) -> Option<tiler_core::connection::SettingsReply> {
        let values = crate::settings::core_setting_values(settings, cli);
        self.seq.apply_settings_values(&values)
    }

    /// Take the retained initial (revision 1) settings reply for startup
    /// logging. `None` once already taken.
    pub(super) fn take_initial_settings_reply(
        &mut self,
    ) -> Option<tiler_core::connection::SettingsReply> {
        self.seq.take_initial_settings_reply()
    }

    /// Test-only live-settings revision through the bound connection.
    #[cfg(test)]
    pub(super) fn apply_test_settings(
        &mut self,
        values: &[(&str, tiler_core::connection::RawSettingValue)],
    ) -> bool {
        self.seq.apply_test_settings(values)
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
    pub(super) fn note_diff_applied(&mut self, reply: &WorldReplyV4) {
        self.diff.gate.note_applied(reply);
    }

    #[allow(dead_code)]
    pub(super) fn push_foreground_events(&mut self, hwnds: Vec<u64>) {
        self.diff.push_foreground_events(hwnds);
    }

    /// Record one drained foreground pass with delivery/owner timestamps
    /// for the in-flight focus join. Production only; offline tests keep
    /// the HWND-only entry above. `omitted` carries the drain's own
    /// detail-cap excess so the sidecar omission stays exact.
    pub(super) fn push_foreground_drain(
        &mut self,
        hwnds: Vec<u64>,
        meta: Vec<super::observe::ForegroundEdgeMeta>,
        omitted: usize,
    ) {
        self.diff.push_foreground_drain_traced(hwnds, meta, omitted);
    }

    /// True while drained foreground edges await apply-entry resolution.
    /// A key-up-only turn owes one observation pass while this holds.
    pub(super) fn has_pending_foreground(&self) -> bool {
        !self.diff.pending_foreground.is_empty()
    }

    /// World-tick entry watermark. Advanced by invalidate before dispatch
    /// fences, so Busy/CountersExhausted still move it. Unchanged fences.
    pub(super) fn current_obs(&self) -> u64 {
        self.seq.current_obs()
    }

    /// Trace-only hover-change oracle: true when `(cursor-hit, focused,
    /// ffc-enabled)` differs from the last emitted `world-apply` row.
    /// Pure comparison over already-captured values: no native reads.
    pub(super) fn hover_trace_changed(&self, hit: Option<&str>, focused: &str, ffc: bool) -> bool {
        self.diff.effects.hover_trace_changed(hit, focused, ffc)
    }

    /// Record the hover triple of an emitted row. Call only when a
    /// `world-apply` row is actually emitted, never on quiet ticks.
    pub(super) fn note_hover_traced(&mut self, hit: Option<String>, focused: String, ffc: bool) {
        self.diff.effects.note_hover_traced(hit, focused, ffc);
    }

    /// Run one diff pass for an accepted reply. Returns stats; attempted
    /// writes append mechanical rows to `frame_out`, `member_out`,
    /// `view_out` for the next report (each stamped with
    /// `attempt_ms`).
    /// No verdicts are produced: the core derives every completion from
    /// the raw attempts plus staged observation. The arguments are the
    /// disjoint pass products; bundling would hide the borrow split the
    /// tick relies on.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_accepted(
        &mut self,
        reply: &WorldReplyV4,
        fresh: &FreshHost,
        ops: &mut dyn super::world_diff::NativeOps,
        frame_out: &mut Vec<tiler_core::world_report::FrameAttempt>,
        member_out: &mut Vec<tiler_core::world_report::MemberAttempt>,
        view_out: &mut Vec<tiler_core::world_report::ViewAttempt>,
        attempt_ms: u64,
    ) -> super::world_diff::ApplyStats {
        let selection_focus_authority = self.seq.selection_focus_authority(reply);
        self.diff.arm_selection_focus(selection_focus_authority);
        let stats = apply_world_reply(
            &mut self.diff,
            reply,
            fresh,
            ops,
            frame_out,
            member_out,
            view_out,
            attempt_ms,
        );
        for attempt in self.diff.take_visibility_attempts() {
            self.seq.note_visibility_attempt(attempt);
        }
        for feedback in self.diff.take_focus_feedback() {
            self.seq.note_focus_feedback(feedback);
        }
        stats
    }

    pub(super) fn last_reply(&self) -> Option<&WorldReplyV4> {
        self.last_reply.as_ref()
    }

    /// Correlated result of the most recent delivered report command: the
    /// sequencer-owned settlement row for that command sequence. `None`
    /// when no command executed (observation tick, duplicate,
    /// already-consumed, or rejected).
    pub(super) fn last_command_result(&self) -> Option<&tiler_core::world_report::CommandResult> {
        self.seq.last_command_result()
    }

    /// Test-only: the desktop-exception authority the owner derives for one
    /// reply from the sequencer's actual latest accepted command state.
    /// Production arms the diff through the same sequencer call.
    #[cfg(test)]
    pub(super) fn selection_focus_authority(&self, reply: &WorldReplyV4) -> bool {
        self.seq.selection_focus_authority(reply)
    }

    /// Borrow the bounded accepted trace-proof row for the current
    /// snapshot (`None` until a fresh accept captures one under trace).
    pub(super) fn trace_proof(&self) -> Option<&serde_json::Value> {
        self.last_proof.as_ref()
    }

    /// Store (`Some`) or clear (`None`) the bounded accepted trace-proof
    /// row. Call only in the Applied ack arm for a fresh (non-duplicate)
    /// delivery, next to `note_diff_applied`, so the proof and
    /// `last_reply` advance together; stale, ignored and duplicate
    /// outcomes must not call this and retain the fallback snapshot.
    pub(super) fn note_trace_proof(&mut self, row: Option<serde_json::Value>) {
        self.last_proof = row;
    }

    /// Synchronous invalidation before dispatch (dirty-until-ack).
    pub(super) fn invalidate(&mut self) {
        self.seq.invalidate();
    }

    /// Stage one mechanical frame attempt for the next report.
    pub(super) fn note_frame_attempt(&mut self, attempt: tiler_core::world_report::FrameAttempt) {
        self.seq.note_frame_attempt(attempt);
    }

    /// Stage one raw membership attempt for the next report.
    pub(super) fn note_member_attempt(&mut self, attempt: tiler_core::world_report::MemberAttempt) {
        self.seq.note_member_attempt(attempt);
    }

    /// Stage one raw view attempt for the next report.
    pub(super) fn note_view_attempt(&mut self, attempt: tiler_core::world_report::ViewAttempt) {
        self.seq.note_view_attempt(attempt);
    }

    /// Host-owned workspace domains capability (see
    /// [`WorldSequencer::host_owned_workspace_domains`]).
    #[must_use]
    pub(super) fn host_owned_workspace_domains(&self) -> bool {
        self.seq.host_owned_workspace_domains()
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
    ) -> Result<WorldReportV4, DispatchError> {
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
    ) -> Result<WorldReportV4, DispatchError> {
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

    /// Production delivery: one report through the sequenced engine. v4
    /// core owns clamped targets; the adapter adds no transfer-remap gate.
    pub(super) fn deliver_production(&mut self, report: &WorldReportV4) -> WorldOutcome {
        let outcome = self.seq.deliver_production(report);
        // Refresh after every core delivery, before native code can pump.
        // Diagnostic reads only; neither value controls core or actuation.
        if super::observe::switch_diag_enabled() {
            super::observe::publish_journey_snapshot(
                self.seq.next_command_seq().saturating_sub(1),
                self.engine().state_revision(),
            );
        }
        outcome
    }

    /// Offer one core reply: acknowledge obligations, apply desired state
    /// only when current, prune native closes on receipt. Returns the ack
    /// outcome plus pruned close tokens for the caller ledger.
    pub(super) fn ack(
        &mut self,
        reply: &WorldReplyV4,
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
        reply: &WorldReplyV4,
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

    /// Read-only fence inspection (production): whether observation-only
    /// catch-up is currently owed. No production mutation.
    pub(super) fn catchup_required(&self) -> bool {
        self.seq.catchup_required()
    }

    /// Force observation-only catch-up (test-only seam): arms the fence a
    /// stale ack or rejection would set. No production caller.
    #[cfg(test)]
    pub(super) fn arm_catchup_for_test(&mut self) {
        self.seq.arm_catchup_for_test();
    }

    /// Next command sequence the sequencer will assign. Advances exactly
    /// once per dispatched explicit command; observation-only catch-up never
    /// moves it. Plain `u64` read (no lock): test command-ordering proof
    /// plus the trace-only journey diagnostic's sampled/published snapshot.
    #[must_use]
    pub(super) fn next_command_seq(&self) -> u64 {
        self.seq.next_command_seq()
    }
}

#[cfg(test)]
#[path = "world_owner_tests.rs"]
mod world_owner_tests;
