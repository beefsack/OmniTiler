#![forbid(unsafe_code)]
//! Bounded trace-only health diagnostics for the input path.
//!
//! Activity-only windowed summaries (at most one per second, emitted on
//! existing owner turns only: no new wakes, timers, or scheduling) plus an
//! immediate first outlier per stream (repeats, including changing outlier
//! signatures, throttled to at most one per second per stream). Every
//! observation is recorded into the window aggregates before any outlier
//! decision, so an immediate record never discards summary data.
//!
//! Honest scope (mirrors the per-turn logs this replaces): snap age is
//! the owner-measured take-time age of a taken batch; pump age is the
//! pump-measured oldest queued item age at its last offer (offer-time
//! queue age, never pump-to-owner transport); heartbeat is the last
//! observed gap between offers and proves liveness only, never alerts;
//! the native event-timestamp vs delivery gap rides summaries as evidence
//! only, never as an alert; owner busy covers non-wait owner work after
//! the pump only (excludes the pump wait, callbacks delivered inside it,
//! and the sample write itself).
//! Summaries count observed samples plus cumulative pump turns and their
//! delta; they never claim every pump turn was individually sampled.
//! Windows with no activity emit nothing. Aggregates reset on emission.
//! No percentiles are tracked. All clocks are caller-supplied (`Instant`
//! parameters) so tests stay deterministic with injected time.

use std::time::{Duration, Instant};

/// Snap classifier-queue age outlier threshold, microseconds.
pub const SNAP_AGE_OUTLIER_US: u64 = 50_000;
/// Pump offer-time queue age outlier threshold, microseconds: the oldest
/// queued item age stamped by the pump at its last offer (see
/// [`crate::input_handoff::InputDiag::queue_age_us`]), never
/// pump-to-owner transport.
pub const PUMP_AGE_OUTLIER_US: u64 = 50_000;
/// Pump dispatch-span outlier threshold, microseconds.
pub const DISPATCH_OUTLIER_US: u64 = 10_000;
/// Owner non-wait work outlier threshold, microseconds.
pub const OWNER_BUSY_OUTLIER_US: u64 = 50_000;
/// Summary/outlier-repeat window: at most one summary and one repeated
/// outlier per stream per second, apart from the final marked run flush.
pub const SUMMARY_WINDOW: Duration = Duration::from_secs(1);

/// Honest scope strings (kept identical to the per-turn logs replaced).
pub const INPUT_SCOPE: &str = "classifier-queue-plus-pump-transport-only";
pub const INPUT_EXCLUDES: &str = "native-delivery-latency";
pub const OWNER_SCOPE: &str = "owner-non-wait-work-after-pump-only";
pub const OWNER_EXCLUDES: &str = "pump-wait-plus-callback-delivery-plus-sample-write";

/// One owner-side input observation. Freshness differs per field:
/// `snap_age_us` is `Some` only when a batch was taken this turn
/// (owner-measured take-time age); `pump_age_us` is `Some` only when the
/// pump published a fresh offer since the last observation
/// (offer-time queue age); `None` skips age maxima and age outliers so
/// stale ages can never false-alert. `dispatch_us` and
/// `delivery_gap_ms` are the last published turn samples; `heartbeat_us`
/// and `pending` are last-observed slot values (the heartbeat persists
/// between offers); `pump_turns` is the cumulative pump turn count.
#[derive(Debug, Clone, Copy)]
pub struct InputSample {
    pub snap_age_us: Option<u64>,
    pub pump_age_us: Option<u64>,
    pub dispatch_us: u64,
    pub delivery_gap_ms: u32,
    pub heartbeat_us: u64,
    pub pending: usize,
    pub pump_turns: u64,
}

/// One owner non-wait work sample (busy measured pre-log exclusive).
#[derive(Debug, Clone, Copy)]
pub struct OwnerSample {
    pub busy_us: u64,
    pub border_refresh_us: u64,
    pub batch_events: u64,
    pub wakes: u64,
    pub starts: u64,
    pub ends: u64,
    pub gesture_held: bool,
    pub seq: u64,
    pub tick_at_start: u64,
}

#[derive(Debug, Default)]
struct InputWin {
    samples: u64,
    batches: u64,
    turns_start: u64,
    turns_end: u64,
    snap_max: u64,
    pump_max: u64,
    dispatch_max: u64,
    delivery_gap_max: u32,
    heartbeat_max: u64,
    pending_peak: usize,
    outliers: u64,
    last_outlier: Option<Instant>,
}

#[derive(Debug, Default)]
struct OwnerWin {
    samples: u64,
    busy_max: u64,
    border_max: u64,
    batch_total: u64,
    outliers: u64,
    last_outlier: Option<Instant>,
}

/// Windowed health aggregator. Plain struct, no interior mutability:
/// the owner-work guard holds the only `&mut` borrow during a turn,
/// so no `RefCell` borrow tracking or shared-ownership machinery.
#[derive(Debug, Default)]
pub struct Health {
    window_start: Option<Instant>,
    last_emit: Option<Instant>,
    input: InputWin,
    owner: OwnerWin,
}

impl Health {
    /// New empty aggregator (no activity, emits nothing).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn start_window(&mut self, now: Instant) {
        if self.window_start.is_none() {
            self.window_start = Some(now);
        }
    }

    fn repeat_due(last: Option<Instant>, now: Instant) -> bool {
        last.map(|prev| now.saturating_duration_since(prev) >= SUMMARY_WINDOW)
            .unwrap_or(true)
    }

    fn window_due(&self, now: Instant) -> bool {
        self.window_start
            .map(|start| now.saturating_duration_since(start) >= SUMMARY_WINDOW)
            .unwrap_or(false)
    }

    fn has_activity(&self) -> bool {
        self.input.samples > 0 || self.owner.samples > 0
    }

    /// Record one input observation; returns the immediate outlier line
    /// on the first breach (repeats throttled per stream). Breaches are
    /// always counted into the window total, throttled or not.
    pub fn observe_input(
        &mut self,
        now: Instant,
        sample: InputSample,
    ) -> Option<serde_json::Value> {
        self.start_window(now);
        let w = &mut self.input;
        w.samples = w.samples.saturating_add(1);
        if w.samples == 1 {
            w.turns_start = sample.pump_turns;
        }
        w.turns_end = sample.pump_turns;
        if sample.snap_age_us.is_some() {
            w.batches = w.batches.saturating_add(1);
        }
        if let Some(age) = sample.snap_age_us {
            w.snap_max = w.snap_max.max(age);
        }
        if let Some(age) = sample.pump_age_us {
            w.pump_max = w.pump_max.max(age);
        }
        w.dispatch_max = w.dispatch_max.max(sample.dispatch_us);
        w.delivery_gap_max = w.delivery_gap_max.max(sample.delivery_gap_ms);
        w.heartbeat_max = w.heartbeat_max.max(sample.heartbeat_us);
        w.pending_peak = w.pending_peak.max(sample.pending);
        let mut breached = serde_json::Map::new();
        if let Some(age) = sample.snap_age_us
            && age > SNAP_AGE_OUTLIER_US
        {
            breached.insert("snap_queue_age_us".to_owned(), age.into());
            breached.insert("snap_threshold_us".to_owned(), SNAP_AGE_OUTLIER_US.into());
        }
        if let Some(age) = sample.pump_age_us
            && age > PUMP_AGE_OUTLIER_US
        {
            breached.insert("pump_queue_age_us".to_owned(), age.into());
            breached.insert("pump_threshold_us".to_owned(), PUMP_AGE_OUTLIER_US.into());
        }
        if sample.dispatch_us > DISPATCH_OUTLIER_US {
            breached.insert("dispatch_us".to_owned(), sample.dispatch_us.into());
            breached.insert(
                "dispatch_threshold_us".to_owned(),
                DISPATCH_OUTLIER_US.into(),
            );
        }
        if breached.is_empty() {
            return None;
        }
        w.outliers = w.outliers.saturating_add(1);
        if !Self::repeat_due(w.last_outlier, now) {
            return None;
        }
        w.last_outlier = Some(now);
        Some(serde_json::json!({
            "event": "input-queue-outlier",
            "metrics": breached,
            "stream_outliers_total": w.outliers,
            "scope": INPUT_SCOPE,
            "excludes": INPUT_EXCLUDES,
        }))
    }

    /// Record one owner-work sample; outlier rules mirror
    /// [`Self::observe_input`].
    pub fn observe_owner(
        &mut self,
        now: Instant,
        sample: OwnerSample,
    ) -> Option<serde_json::Value> {
        self.start_window(now);
        let w = &mut self.owner;
        w.samples = w.samples.saturating_add(1);
        w.busy_max = w.busy_max.max(sample.busy_us);
        w.border_max = w.border_max.max(sample.border_refresh_us);
        w.batch_total = w.batch_total.saturating_add(sample.batch_events);
        if sample.busy_us <= OWNER_BUSY_OUTLIER_US {
            return None;
        }
        w.outliers = w.outliers.saturating_add(1);
        if !Self::repeat_due(w.last_outlier, now) {
            return None;
        }
        w.last_outlier = Some(now);
        Some(serde_json::json!({
            "event": "owner-work-outlier",
            "seq": sample.seq,
            "tick_at_start": sample.tick_at_start,
            "busy_us": sample.busy_us,
            "threshold_us": OWNER_BUSY_OUTLIER_US,
            "border_refresh_us": sample.border_refresh_us,
            "batch_events": sample.batch_events,
            "wakes": sample.wakes,
            "starts": sample.starts,
            "ends": sample.ends,
            "gesture_held": sample.gesture_held,
            "stream_outliers_total": w.outliers,
            "scope": OWNER_SCOPE,
            "excludes": OWNER_EXCLUDES,
        }))
    }

    fn window_us(&self, now: Instant) -> u64 {
        self.window_start
            .map(|start| {
                now.saturating_duration_since(start)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64
            })
            .unwrap_or(0)
    }

    /// Emit per-stream summaries for streams with activity and reset
    /// the window aggregates. `flushed` marks trailing/end-of-run
    /// lines; periodic lines carry no marker. Outlier throttle
    /// timestamps survive the reset: a flush must never re-arm an
    /// immediate outlier inside the second (only aggregate counts and
    /// maxima reset).
    fn emit(&mut self, now: Instant, flushed: Option<&str>) -> Vec<serde_json::Value> {
        let window_us = self.window_us(now);
        let mut out = Vec::new();
        if self.input.samples > 0 {
            let mut line = serde_json::json!({
                "event": "input-queue-summary",
                "window_us": window_us,
                "observed": self.input.samples,
                "batches": self.input.batches,
                "pump_turns_start": self.input.turns_start,
                "pump_turns_end": self.input.turns_end,
                "pump_turns_delta": self.input.turns_end.saturating_sub(self.input.turns_start),
                "snap_queue_age_max_us": self.input.snap_max,
                "pump_queue_age_max_us": self.input.pump_max,
                "dispatch_max_us": self.input.dispatch_max,
                "delivery_gap_max_ms": self.input.delivery_gap_max,
                "heartbeat_max_us": self.input.heartbeat_max,
                "pending_peak": self.input.pending_peak,
                "outliers": self.input.outliers,
                "scope": INPUT_SCOPE,
                "excludes": INPUT_EXCLUDES,
                "note": "observed-samples-only-not-every-pump-turn",
            });
            if let Some(flushed) = flushed {
                line["flushed"] = flushed.into();
            }
            out.push(line);
        }
        if self.owner.samples > 0 {
            let mut line = serde_json::json!({
                "event": "owner-work-summary",
                "window_us": window_us,
                "samples": self.owner.samples,
                "busy_max_us": self.owner.busy_max,
                "border_refresh_max_us": self.owner.border_max,
                "batch_events_total": self.owner.batch_total,
                "outliers": self.owner.outliers,
                "scope": OWNER_SCOPE,
                "excludes": OWNER_EXCLUDES,
            });
            if let Some(flushed) = flushed {
                line["flushed"] = flushed.into();
            }
            out.push(line);
        }
        // Retain per-stream outlier throttle timestamps across the
        // aggregate reset (see `emit` docs).
        let input_outlier = self.input.last_outlier;
        let owner_outlier = self.owner.last_outlier;
        self.input = InputWin::default();
        self.owner = OwnerWin::default();
        self.input.last_outlier = input_outlier;
        self.owner.last_outlier = owner_outlier;
        self.window_start = None;
        self.last_emit = Some(now);
        out
    }

    /// Periodic summaries, due only: activity plus a full window
    /// elapsed. Quiet windows emit nothing.
    #[must_use]
    pub fn take_due(&mut self, now: Instant) -> Vec<serde_json::Value> {
        if !self.has_activity() || !self.window_due(now) {
            return Vec::new();
        }
        self.emit(now, None)
    }

    /// Trailing flush for idle maintenance turns: pending activity
    /// below the window age still emits (marked), throttled to one
    /// per second from the last emission so quiet-adjacent turns
    /// cannot exceed the rate. Suppressed windows keep their data
    /// for the next due or end-of-run flush: nothing is discarded.
    #[must_use]
    pub fn take_trailing(&mut self, now: Instant) -> Vec<serde_json::Value> {
        if !self.has_activity() || !Self::repeat_due(self.last_emit, now) {
            return Vec::new();
        }
        self.emit(now, Some("maintenance-trailing"))
    }

    /// Pending end-of-run summary: emits any retained activity
    /// regardless of the throttle (at most one call per run).
    #[must_use]
    pub fn take_end_of_run(&mut self, now: Instant) -> Vec<serde_json::Value> {
        if !self.has_activity() {
            return Vec::new();
        }
        self.emit(now, Some("end-of-run"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn age_only(snap_age_us: u64, pump_age_us: u64, turns: u64) -> InputSample {
        InputSample {
            snap_age_us: Some(snap_age_us),
            pump_age_us: Some(pump_age_us),
            dispatch_us: 100,
            delivery_gap_ms: 2,
            heartbeat_us: 500,
            pending: 0,
            pump_turns: turns,
        }
    }

    fn owner(busy_us: u64) -> OwnerSample {
        OwnerSample {
            busy_us,
            border_refresh_us: 7,
            batch_events: 3,
            wakes: 1,
            starts: 0,
            ends: 0,
            gesture_held: false,
            seq: 9,
            tick_at_start: 4,
        }
    }

    #[test]
    fn high_rate_flood_aggregates_with_one_immediate_outlier() {
        let mut health = Health::new();
        let start = t0();
        let mut immediates = 0;
        for turn in 0..500u64 {
            let sample = age_only(60_000 + turn, 55_000, 100 + turn);
            if health.observe_input(start, sample).is_some() {
                immediates += 1;
            }
        }
        assert_eq!(immediates, 1, "flood inside one window emits one outlier");
        assert_eq!(health.input.samples, 500);
        assert_eq!(health.input.outliers, 500, "throttled breaches still count");
        assert_eq!(health.input.snap_max, 60_499);
        assert_eq!(health.input.pump_max, 55_000);
        let lines = health.take_due(start + SUMMARY_WINDOW);
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line["event"], "input-queue-summary");
        assert_eq!(line["observed"], 500);
        assert_eq!(line["batches"], 500);
        assert_eq!(line["pump_turns_start"], 100);
        assert_eq!(line["pump_turns_end"], 599);
        assert_eq!(line["pump_turns_delta"], 499);
        assert_eq!(line["outliers"], 500);
        assert!(line.get("flushed").is_none());
        assert!(health.take_due(start + 2 * SUMMARY_WINDOW).is_empty());
    }

    #[test]
    fn thresholds_fire_strictly_above_boundary() {
        let mut health = Health::new();
        let start = t0();
        assert!(
            health
                .observe_input(start, age_only(50_000, 50_000, 1))
                .is_none()
        );
        assert_eq!(health.input.outliers, 0);
        let line = health
            .observe_input(start, age_only(50_001, 50_000, 2))
            .expect("snap age above boundary alerts");
        assert_eq!(line["event"], "input-queue-outlier");
        assert_eq!(line["metrics"]["snap_queue_age_us"], 50_001);
        let mut health = Health::new();
        let dispatch = InputSample {
            snap_age_us: None,
            pump_age_us: None,
            dispatch_us: 10_000,
            delivery_gap_ms: 0,
            heartbeat_us: 0,
            pending: 0,
            pump_turns: 3,
        };
        assert!(health.observe_input(start, dispatch).is_none());
        let dispatch = InputSample {
            dispatch_us: 10_001,
            ..dispatch
        };
        assert!(health.observe_input(start, dispatch).is_some());
        let mut health = Health::new();
        assert!(health.observe_owner(start, owner(50_000)).is_none());
        assert!(health.observe_owner(start, owner(50_001)).is_some());
    }

    #[test]
    fn repeated_outliers_throttled_per_stream_despite_changing_signature() {
        let mut health = Health::new();
        let start = t0();
        let first = health
            .observe_input(start, age_only(60_000, 1_000, 1))
            .expect("first breach is immediate");
        assert_eq!(first["metrics"]["snap_queue_age_us"], 60_000);
        let dispatch_only = InputSample {
            snap_age_us: Some(10),
            pump_age_us: Some(10),
            dispatch_us: 20_000,
            delivery_gap_ms: 0,
            heartbeat_us: 0,
            pending: 0,
            pump_turns: 2,
        };
        assert!(
            health
                .observe_input(start + Duration::from_millis(500), dispatch_only)
                .is_none(),
            "changing outlier signature still throttled within the window"
        );
        assert_eq!(health.input.outliers, 2);
        let later = health
            .observe_input(start + SUMMARY_WINDOW, age_only(1_000, 70_000, 3))
            .expect("repeat after the window is immediate");
        assert_eq!(later["metrics"]["pump_queue_age_us"], 70_000);
        assert_eq!(later["stream_outliers_total"], 3);
    }

    #[test]
    fn outlier_streams_throttle_independently() {
        let mut health = Health::new();
        let start = t0();
        assert!(
            health
                .observe_input(start, age_only(60_000, 60_000, 1))
                .is_some()
        );
        assert!(health.observe_owner(start, owner(60_000)).is_some());
        assert!(
            health
                .observe_input(start, age_only(61_000, 61_000, 2))
                .is_none()
        );
        assert!(health.observe_owner(start, owner(61_000)).is_none());
    }

    #[test]
    fn outlier_throttle_survives_flush_resets_on_all_streams() {
        // A flush (due, trailing, or end-of-run) resets aggregates but
        // must retain per-stream outlier throttle timestamps: a breach
        // just before the flush, then one immediately after, emits exactly
        // one immediate per second, while throttled breaches still count.
        #[derive(Clone, Copy, Debug)]
        enum Stream {
            Input,
            Owner,
        }
        fn breach(health: &mut Health, stream: Stream, now: Instant) -> bool {
            match stream {
                Stream::Input => health
                    .observe_input(now, age_only(60_000, 60_000, 1))
                    .is_some(),
                Stream::Owner => health.observe_owner(now, owner(60_000)).is_some(),
            }
        }
        fn calm(health: &mut Health, stream: Stream, now: Instant) {
            match stream {
                Stream::Input => {
                    health.observe_input(now, age_only(100, 100, 0));
                }
                Stream::Owner => {
                    health.observe_owner(now, owner(100));
                }
            }
        }
        // 0 = due, 1 = trailing, 2 = end-of-run.
        fn flush(health: &mut Health, kind: u8, now: Instant) -> usize {
            match kind {
                0 => health.take_due(now).len(),
                1 => health.take_trailing(now).len(),
                _ => health.take_end_of_run(now).len(),
            }
        }
        for stream in [Stream::Input, Stream::Owner] {
            // Due flush just after a late-window outlier: the calm
            // observation opens the window so the flush lands ~1ms after
            // the outlier.
            let mut health = Health::new();
            let start = t0();
            calm(&mut health, stream, start);
            assert!(
                breach(&mut health, stream, start + SUMMARY_WINDOW),
                "{stream:?}: first breach is immediate"
            );
            assert_eq!(
                flush(
                    &mut health,
                    0,
                    start + SUMMARY_WINDOW + Duration::from_millis(1)
                ),
                1,
                "{stream:?}: due flush emits"
            );
            assert!(
                !breach(
                    &mut health,
                    stream,
                    start + SUMMARY_WINDOW + Duration::from_millis(2)
                ),
                "{stream:?}: breach inside the second after a flush stays suppressed"
            );
            assert!(
                breach(&mut health, stream, start + 2 * SUMMARY_WINDOW),
                "{stream:?}: breach at the 1s boundary is immediate"
            );
            // Trailing and end-of-run flushes.
            for kind in [1u8, 2] {
                let mut health = Health::new();
                let start = t0();
                assert!(
                    breach(&mut health, stream, start),
                    "{stream:?}: first breach is immediate"
                );
                assert_eq!(
                    flush(&mut health, kind, start + Duration::from_millis(100)),
                    1,
                    "{stream:?}: flush kind {kind} emits"
                );
                assert!(
                    !breach(&mut health, stream, start + Duration::from_millis(200)),
                    "{stream:?}: breach inside the second after flush kind {kind} stays suppressed"
                );
                assert!(
                    breach(&mut health, stream, start + SUMMARY_WINDOW),
                    "{stream:?}: breach at the 1s boundary after flush kind {kind} is immediate"
                );
                let retained = health.take_due(start + 2 * SUMMARY_WINDOW);
                assert_eq!(retained.len(), 1, "{stream:?}: totals survive");
                assert_eq!(
                    retained[0]["outliers"], 2,
                    "{stream:?}: throttled breach counted"
                );
            }
        }
    }

    #[test]
    fn no_batch_turn_samples_preserve_dispatch_outliers_without_age_alerts() {
        // Sustained no-batch pump turns (stale ages, fresh dispatch):
        // dispatch breaches alert and count, stale ages never do, and
        // the summary carries the turn evidence with zero age maxima.
        let mut health = Health::new();
        let start = t0();
        let turn = |dispatch_us: u64, turns: u64| InputSample {
            snap_age_us: None,
            pump_age_us: None,
            dispatch_us,
            delivery_gap_ms: 3,
            heartbeat_us: 400,
            pending: 1,
            pump_turns: turns,
        };
        assert!(
            health.observe_input(start, turn(20_000, 50)).is_some(),
            "fresh dispatch breach on a batch-less turn is immediate"
        );
        assert!(
            health
                .observe_input(start + Duration::from_millis(100), turn(25_000, 51))
                .is_none(),
            "repeat inside the second is throttled but counted"
        );
        let lines = health.take_due(start + SUMMARY_WINDOW);
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line["observed"], 2);
        assert_eq!(line["batches"], 0);
        assert_eq!(line["dispatch_max_us"], 25_000);
        assert_eq!(line["outliers"], 2);
        assert_eq!(line["snap_queue_age_max_us"], 0);
        assert_eq!(line["pump_queue_age_max_us"], 0);
        assert_eq!(line["pump_turns_start"], 50);
        assert_eq!(line["pump_turns_end"], 51);
    }

    #[test]
    fn quiet_windows_emit_nothing() {
        let mut health = Health::new();
        let start = t0();
        assert!(health.take_due(start + SUMMARY_WINDOW).is_empty());
        assert!(health.take_trailing(start).is_empty());
        assert!(health.take_end_of_run(start).is_empty());
    }

    #[test]
    fn trailing_flush_marks_and_resets_without_discarding_on_throttle() {
        let mut health = Health::new();
        let start = t0();
        health.observe_input(start, age_only(1_000, 2_000, 7));
        let lines = health.take_trailing(start + Duration::from_millis(100));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["flushed"], "maintenance-trailing");
        assert_eq!(lines[0]["pump_turns_start"], 7);
        assert!(health.take_due(start + SUMMARY_WINDOW).is_empty(), "reset");

        health.observe_input(start + SUMMARY_WINDOW, age_only(3_000, 4_000, 9));
        assert!(!health.take_due(start + 2 * SUMMARY_WINDOW).is_empty());
        health.observe_owner(start + 2 * SUMMARY_WINDOW, owner(100));
        assert!(
            health
                .take_trailing(start + 2 * SUMMARY_WINDOW + Duration::from_millis(100))
                .is_empty(),
            "trailing inside one second of the last emit retains data"
        );
        let lines = health.take_due(start + 3 * SUMMARY_WINDOW);
        assert_eq!(lines.len(), 1, "retained owner sample still summarizes");
        assert_eq!(lines[0]["event"], "owner-work-summary");
    }

    #[test]
    fn end_of_run_flushes_pending_regardless_of_throttle() {
        let mut health = Health::new();
        let start = t0();
        health.observe_input(start, age_only(1_000, 2_000, 7));
        let lines = health.take_end_of_run(start + Duration::from_millis(10));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["event"], "input-queue-summary");
        assert_eq!(lines[0]["flushed"], "end-of-run");
        assert!(
            health
                .take_end_of_run(start + Duration::from_millis(20))
                .is_empty()
        );
    }

    #[test]
    fn maxima_reset_between_windows() {
        let mut health = Health::new();
        let start = t0();
        health.observe_input(start, age_only(60_000, 60_000, 1));
        assert_eq!(health.take_due(start + SUMMARY_WINDOW).len(), 1);
        health.observe_input(start + SUMMARY_WINDOW, age_only(1_000, 2_000, 2));
        let lines = health.take_due(start + 2 * SUMMARY_WINDOW);
        assert_eq!(lines[0]["snap_queue_age_max_us"], 1_000);
        assert_eq!(lines[0]["pump_queue_age_max_us"], 2_000);
        assert_eq!(lines[0]["outliers"], 0);
    }

    #[test]
    fn heartbeat_and_delivery_gap_are_summary_evidence_only() {
        let mut health = Health::new();
        let start = t0();
        let sample = InputSample {
            snap_age_us: Some(100),
            pump_age_us: Some(200),
            dispatch_us: 300,
            delivery_gap_ms: u32::MAX,
            heartbeat_us: u64::MAX,
            pending: 5,
            pump_turns: 11,
        };
        assert!(
            health.observe_input(start, sample).is_none(),
            "liveness/gap never alert"
        );
        health.observe_input(
            start,
            InputSample {
                pending: 9,
                pump_turns: 12,
                ..sample
            },
        );
        let lines = health.take_due(start + SUMMARY_WINDOW);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["delivery_gap_max_ms"], u32::MAX);
        assert_eq!(lines[0]["heartbeat_max_us"], u64::MAX);
        assert_eq!(lines[0]["pending_peak"], 9);
        assert_eq!(lines[0]["outliers"], 0);
    }

    #[test]
    fn stale_ages_never_alert_or_pollute_maxima() {
        let mut health = Health::new();
        let start = t0();
        let sample = InputSample {
            snap_age_us: None,
            pump_age_us: None,
            dispatch_us: 100,
            delivery_gap_ms: 0,
            heartbeat_us: 0,
            pending: 0,
            pump_turns: 4,
        };
        assert!(health.observe_input(start, sample).is_none());
        assert_eq!(health.input.samples, 1);
        assert_eq!(health.input.batches, 0);
        let lines = health.take_due(start + SUMMARY_WINDOW);
        assert_eq!(lines[0]["snap_queue_age_max_us"], 0);
        assert_eq!(lines[0]["pump_queue_age_max_us"], 0);
    }

    #[test]
    fn summary_and_outlier_lines_carry_honest_scopes_and_no_identity() {
        let mut health = Health::new();
        let start = t0();
        let outlier = health
            .observe_input(start, age_only(60_000, 70_000, 1))
            .expect("outlier");
        health.observe_owner(start, owner(60_000));
        let mut lines = health.take_due(start + SUMMARY_WINDOW);
        lines.push(outlier);
        assert_eq!(lines.len(), 3);
        for line in &lines {
            let text = line.to_string().to_lowercase();
            for banned in ["hwnd", "pid", "exe", "title", "token"] {
                assert!(!text.contains(banned), "no identity in {line}");
            }
        }
        let summary = lines
            .iter()
            .find(|l| l["event"] == "input-queue-summary")
            .expect("input summary");
        assert_eq!(summary["scope"], INPUT_SCOPE);
        assert_eq!(summary["excludes"], INPUT_EXCLUDES);
        assert_eq!(summary["note"], "observed-samples-only-not-every-pump-turn");
        let owner_summary = lines
            .iter()
            .find(|l| l["event"] == "owner-work-summary")
            .expect("owner summary");
        assert_eq!(owner_summary["scope"], OWNER_SCOPE);
    }
}
