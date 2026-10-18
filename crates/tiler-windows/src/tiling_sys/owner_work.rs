#![forbid(unsafe_code)]
use super::*;

use super::trace::log_json_at;

/// Trace-only non-wait owner-work timing sample for one pump iteration,
/// aggregated into the bounded [`Health`] window instead of per-wake file
/// writes. Created immediately after `pump_wait` returns (so `busy_us`
/// excludes the wait plus any hook callbacks delivered inside the pump)
/// and scoped to the iteration: every `continue`/error path drops through
/// [`OwnerWorkGuard::commit_owner`] in `Drop`. `Drop` emits at most one
/// throttled outlier line via the existing infallible `log_json_at` and
/// never logs from a callback; periodic summaries flush on existing owner
/// turns through [`OwnerWorkGuard::finish_turn`]. Fields are closed
/// aggregates only (sequence, tick, microsecond offsets, batch counts,
/// held flag): no HWNDs, PIDs, input content, or absolute clocks. Quiet
/// short-poll iterations stay disabled and drop silently; the caller arms
/// only active wakes.
///
/// The guard holds the iteration's only `&mut Health` borrow as a plain
/// field (never `RefCell`, never shared ownership): input observations
/// route through the guard mid-turn while `state` stays freely mutable,
/// so no borrow is held across the rest of the loop.
pub(super) struct OwnerWorkGuard<'h> {
    enabled: bool,
    log_path: Option<std::path::PathBuf>,
    seq: u64,
    tick_at_start: u64,
    start: Option<Instant>,
    border_refresh_us: u64,
    batch_events: u64,
    wakes: u64,
    starts: u64,
    ends: u64,
    gesture_held: bool,
    health: Option<&'h mut Health>,
}

impl<'h> OwnerWorkGuard<'h> {
    /// Starts the non-wait clock. Timer creation, path clone, and sequence
    /// accounting happen only when `trace` holds, so default runs create
    /// no timers or log records (callers gate border
    /// timing on [`OwnerWorkGuard::timing_active`]). The guard starts
    /// disabled; the caller arms it after the wake gate. `health` is
    /// `Some` only when tracing: `None` keeps the guard fully inert.
    pub(super) fn begin(
        trace: bool,
        log_path: &Path,
        seq: u64,
        tick: u64,
        health: Option<&'h mut Health>,
    ) -> Self {
        if !trace {
            return Self {
                enabled: false,
                log_path: None,
                seq: 0,
                tick_at_start: tick,
                start: None,
                border_refresh_us: 0,
                batch_events: 0,
                wakes: 0,
                starts: 0,
                ends: 0,
                gesture_held: false,
                health: None,
            };
        }
        Self {
            enabled: false,
            log_path: Some(log_path.to_path_buf()),
            seq,
            tick_at_start: tick,
            start: Some(Instant::now()),
            border_refresh_us: 0,
            batch_events: 0,
            wakes: 0,
            starts: 0,
            ends: 0,
            gesture_held: false,
            health,
        }
    }

    /// Arms the one sample for an active wake. No-op unless tracing: quiet
    /// iterations never enable, so their `continue` drops silently.
    pub(super) fn arm(
        &mut self,
        batch_events: u64,
        wakes: u64,
        starts: u64,
        ends: u64,
        gesture_held: bool,
    ) {
        if self.health.is_none() {
            return;
        }
        self.batch_events = batch_events;
        self.wakes = wakes;
        self.starts = starts;
        self.ends = ends;
        self.gesture_held = gesture_held;
        self.enabled = true;
    }

    /// Whether border-refresh timing should run (trace plus armed). Callers
    /// must not create an `Instant` per refresh when this is false.
    pub(super) fn timing_active(&self) -> bool {
        self.enabled
    }

    pub(super) fn add_border_refresh(&mut self, elapsed: Duration) {
        if !self.enabled {
            return;
        }
        self.border_refresh_us = self
            .border_refresh_us
            .saturating_add(elapsed.as_micros().try_into().unwrap_or(u64::MAX));
    }

    /// Records one input observation into the window; emits the immediate
    /// outlier line when the health window reports one. `snap_age_us` is
    /// `Some` only when a batch was taken this turn (owner-measured
    /// take-time age); `pump_age_us` is `Some` only when the pump
    /// published a fresh offer since the last observation (offer-time
    /// queue age, never transport). Stale ages stay `None` and can never
    /// false-alert. Dispatch/delivery-gap are last published turn
    /// samples; heartbeat/pending are last-observed values. No-op when
    /// tracing is off.
    pub(super) fn observe_input(
        &mut self,
        diag: &crate::input_handoff::InputDiag,
        snap_age_us: Option<u64>,
        pump_age_us: Option<u64>,
    ) {
        let (Some(health), Some(log_path)) = (self.health.as_mut(), self.log_path.as_ref()) else {
            return;
        };
        let sample = InputSample {
            snap_age_us,
            pump_age_us,
            dispatch_us: diag.dispatch_us,
            delivery_gap_ms: diag.delivery_gap_ms,
            heartbeat_us: diag.heartbeat_us,
            pending: diag.pending_snap,
            pump_turns: diag.turns,
        };
        if let Some(line) = health.observe_input(Instant::now(), sample) {
            log_json_at(log_path, line);
        }
    }

    /// Commits the armed owner sample into the window (measuring the
    /// pre-log exclusive span first) and disarms. Covers the suspend
    /// `continue` and any error path when reached explicitly; `Drop`
    /// covers the rest.
    fn commit_owner(&mut self) {
        if !self.enabled {
            return;
        }
        self.enabled = false;
        let (Some(health), Some(log_path), Some(start)) =
            (self.health.as_mut(), self.log_path.as_ref(), self.start)
        else {
            return;
        };
        // `busy_us` covers owner work after the pump only. The write cost
        // of any sample itself is not included (elapsed is captured before
        // the append). Delivered callback time inside `pump_wait` is not
        // covered either; neither is the quiet stop probe (quiet
        // iterations emit no sample at all).
        let now = Instant::now();
        let busy_us: u64 = now
            .duration_since(start)
            .as_micros()
            .try_into()
            .unwrap_or(u64::MAX);
        let sample = OwnerSample {
            busy_us,
            border_refresh_us: self.border_refresh_us,
            batch_events: self.batch_events,
            wakes: self.wakes,
            starts: self.starts,
            ends: self.ends,
            gesture_held: self.gesture_held,
            seq: self.seq,
            tick_at_start: self.tick_at_start,
        };
        if let Some(line) = health.observe_owner(now, sample) {
            log_json_at(log_path, line);
        }
    }

    /// Flushes due window summaries (activity plus a full window) via the
    /// existing infallible writer. Quiet windows emit nothing.
    fn poll_due(&mut self, now: Instant) {
        let (Some(health), Some(log_path)) = (self.health.as_mut(), self.log_path.as_ref()) else {
            return;
        };
        for line in health.take_due(now) {
            log_json_at(log_path, line);
        }
    }

    /// Trailing flush for idle maintenance turns (marked lines,
    /// throttled): pending activity below the window age still emits so
    /// evidence never strands behind idle. Suppressed windows keep
    /// their data for the next due or end-of-run flush.
    fn poll_trailing(&mut self, now: Instant) {
        let (Some(health), Some(log_path)) = (self.health.as_mut(), self.log_path.as_ref()) else {
            return;
        };
        for line in health.take_trailing(now) {
            log_json_at(log_path, line);
        }
    }

    /// End-of-turn commit: records the armed owner sample, then flushes
    /// due summaries; on maintenance-quiet turns also flushes trailing
    /// activity. After this the guard drops silently.
    pub(super) fn finish_turn(&mut self, maintenance_quiet: bool) {
        self.commit_owner();
        let now = Instant::now();
        self.poll_due(now);
        if maintenance_quiet {
            self.poll_trailing(now);
        }
    }
}

impl Drop for OwnerWorkGuard<'_> {
    fn drop(&mut self) {
        self.commit_owner();
    }
}
