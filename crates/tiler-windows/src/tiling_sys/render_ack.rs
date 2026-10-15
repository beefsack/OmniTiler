use super::*;

use super::render_bridge::{PostedNote, render_hide_reason, render_record_applied};

pub(super) fn poll_render_carrier(state: &mut TileLoop, carrier: RenderCarrier, now: Instant) {
    let ack = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.channels.take_ack(carrier));
    let Some(ack) = ack else {
        return;
    };
    // Applied mirror first, independently of log pairing: visibility and the
    // stop snapshot stay truthful even when the posted note aged out.
    render_record_applied(state, carrier, &ack);
    // Bounded routine-frame gate: one decision shared by the trace
    // `render-applied` proof and the companion status line, so an aggregated
    // frame emits neither (no duplicates). Failures never aggregate (including
    // repeats); a changed transition key logs immediately; only a routine
    // successful repaint (Shown/Redrew/Unchanged with applied content, or
    // Moved when tracing) under the same key aggregates. Alternating routine
    // outcome names under one key still aggregate: the key carries no outcome.
    if !render_ack_is_failure(&ack)
        && let Some(applied) = ack.applied.as_ref()
    {
        let routine = match &ack.outcome {
            RenderOutcome::Shown | RenderOutcome::Redrew | RenderOutcome::Unchanged => true,
            RenderOutcome::Moved if state.trace => true,
            _ => false,
        };
        if routine {
            let key = render_ack_transition_key(state, carrier, &ack);
            if state.render_frames.transition_key(carrier) == Some(key.as_str()) {
                let checksum_hex = format!("{:016x}", ack.checksum);
                state.render_frames.observe_routine(
                    carrier,
                    now,
                    Some(ack.queue_age_us),
                    Some(ack.span_us),
                    RenderFrameSample {
                        outcome: ack.outcome.name(),
                        target: applied.target.clone(),
                        outer: applied.outer,
                        color_hex: applied.color_hex.clone(),
                        checksum_hex,
                        generation: Some(ack.generation),
                        safety: Some(ack.safety),
                        hidden_proven: ack.hidden_proven,
                        identity_proven: ack.identity_proven,
                        source: "ack",
                    },
                );
                render_prune_notes(state, carrier, ack.generation);
                return;
            }
            state.render_frames.note_transition(carrier, key);
        } else {
            let key = render_ack_transition_key(state, carrier, &ack);
            state.render_frames.note_transition(carrier, key);
        }
    } else {
        let key = render_ack_transition_key(state, carrier, &ack);
        state.render_frames.note_transition(carrier, key);
    }
    // Trace-only compact proof for immediate transitions and failures only:
    // routine successful repaints aggregate into the per-carrier window above
    // instead of logging per ack. Non-trace logs stay change-gated in the
    // existing vocabulary below. No renderer/input logging, no raw input/app
    // content, no window identifiers.
    if state.trace {
        let (outcome_name, reason) = match &ack.outcome {
            RenderOutcome::Refused(reason) => ("refused", Some(*reason)),
            RenderOutcome::Unknown(reason) => ("unknown", Some(*reason)),
            outcome => (outcome.name(), None),
        };
        let mut event = serde_json::json!({
            "event": "render-applied",
            "tick": state.tick,
            "carrier": carrier.name(),
            "generation": ack.generation,
            "safety": ack.safety,
            "outcome": outcome_name,
            "hidden_proven": ack.hidden_proven,
            "id_proven": ack.identity_proven,
            "queue_age_us": ack.queue_age_us,
            "span_us": ack.span_us,
            "checksum": format!("{:016x}", ack.checksum),
        });
        if let Some(reason) = reason {
            event["reason"] = serde_json::Value::from(reason);
        }
        if let Some(applied) = ack.applied.as_ref() {
            event["color"] = serde_json::Value::from(applied.color_hex.clone());
            event["rect"] = serde_json::json!(applied.outer);
        }
        if let Some(error) = ack.last_error.as_ref() {
            event["last_error"] = serde_json::Value::from(error.clone());
        }
        let log_path = state.log_path.clone();
        log_json_at(&log_path, event);
    }
    let checksum_hex = format!("{:016x}", ack.checksum);
    match carrier {
        RenderCarrier::Border => poll_border_ack(state, &ack, &checksum_hex),
        RenderCarrier::Underlay => poll_underlay_ack(state, &ack, &checksum_hex),
        RenderCarrier::Preview => poll_preview_ack(state, &ack, &checksum_hex),
    }
}

/// Find the posted note pairing one ack generation (bounded history; the
/// latest-wins slot plus the renderer quantum keep realistic lag near zero).
/// `None` means the pairing is gone: log nothing rather than fabricate.
fn render_note_for(
    state: &TileLoop,
    carrier: RenderCarrier,
    generation: u64,
) -> Option<PostedNote> {
    state.render_bridge.as_ref()?.posted[carrier.index()]
        .iter()
        .find(|(posted_gen, _)| *posted_gen == generation)
        .map(|(_, note)| note.clone())
}

/// Prune posted notes at or below one acknowledged generation.
fn render_prune_notes(state: &mut TileLoop, carrier: RenderCarrier, generation: u64) {
    if let Some(bridge) = state.render_bridge.as_mut() {
        let history = &mut bridge.posted[carrier.index()];
        history.retain(|(posted_gen, _)| *posted_gen > generation);
    }
}

/// Summary window for routine render-frame diagnostics: at most one summary
/// per second per carrier, emitted on existing owner turns only (no new wakes,
/// timers, or scheduling). All clocks are caller-supplied (`Instant`
/// parameters) so tests stay deterministic with injected time.
pub(super) const RENDER_SUMMARY_WINDOW: Duration = Duration::from_secs(1);

/// Honest scope strings for the routine-frame summaries.
pub(super) const RENDER_FRAME_SCOPE: &str = "routine-successful-repaints-only";
const RENDER_FRAME_EXCLUDES: &str = "transitions-failures-unknown-refused";

/// Latest-applied sample carried in a routine-frame summary. Content truth
/// (generation/safety/proof/outcome/geometry/colour/target/checksum) rides the
/// ack applied payload, never desired state; the summary labels it a sample
/// and maxima come from actual acks only. Same-thread repaints carry no ack
/// transport (`source` is `"same-thread"`, generation/safety/queue/span are
/// null there); acked repaints carry `source: "ack"` with real values.
#[derive(Debug, Clone, Default)]
pub(super) struct RenderFrameSample {
    pub(super) outcome: &'static str,
    pub(super) target: String,
    pub(super) outer: [i32; 4],
    pub(super) color_hex: String,
    pub(super) checksum_hex: String,
    pub(super) generation: Option<u64>,
    pub(super) safety: Option<u64>,
    pub(super) hidden_proven: bool,
    pub(super) identity_proven: bool,
    pub(super) source: &'static str,
}

#[derive(Debug, Default)]
struct RenderCarrierWin {
    observed: u64,
    /// Maxima over actual ack transport only; `None` when the window holds
    /// same-thread samples alone (never zero-filled: zero is a measurement).
    queue_max_us: Option<u64>,
    span_max_us: Option<u64>,
    sample: RenderFrameSample,
    window_start: Option<Instant>,
    last_emit: Option<Instant>,
    /// Last immediate transition key (`shown:{target}:s{safety}:h{proven}:i{proven}`,
    /// `hidden:{reason}:...`, `refused:{reason}`, `unknown:{reason}`,
    /// `failure:{detail}`). Routine repaints with the same key aggregate; a
    /// changed key logs immediately.
    transition_key: Option<String>,
}

/// Bounded per-carrier routine-frame aggregator. Plain struct, no interior
/// mutability. Windows with no activity emit nothing. Aggregates reset on
/// emission. Failures and transitions never enter here: callers log those
/// immediately and only route routine successful repaints (Shown/Redrew/
/// Unchanged/Moved with no failure) through [`Self::observe_routine`].
#[derive(Debug, Default)]
pub(super) struct RenderFrameHealth {
    wins: [RenderCarrierWin; 3],
}

impl RenderFrameHealth {
    fn window_due(win: &RenderCarrierWin, now: Instant) -> bool {
        win.window_start
            .map(|start| now.saturating_duration_since(start) >= RENDER_SUMMARY_WINDOW)
            .unwrap_or(false)
    }

    fn repeat_due(last: Option<Instant>, now: Instant) -> bool {
        last.map(|prev| now.saturating_duration_since(prev) >= RENDER_SUMMARY_WINDOW)
            .unwrap_or(true)
    }

    /// Last immediate transition key for one carrier.
    fn transition_key(&self, carrier: RenderCarrier) -> Option<&str> {
        self.wins[carrier.index()].transition_key.as_deref()
    }

    /// Record one immediate transition (visibility/target/outcome/reason
    /// change or failure). Only updates the key so the next same-key routine
    /// repaint aggregates; emits nothing itself.
    pub(super) fn note_transition(&mut self, carrier: RenderCarrier, key: String) {
        self.wins[carrier.index()].transition_key = Some(key);
    }

    /// Record one routine successful repaint into the window aggregates.
    /// `queue_us`/`span_us` are `Some` only for acked repaints; same-thread
    /// samples pass `None` so maxima cover actual ack transport only and are
    /// never zero-filled (zero is a measurement).
    pub(super) fn observe_routine(
        &mut self,
        carrier: RenderCarrier,
        now: Instant,
        queue_us: Option<u64>,
        span_us: Option<u64>,
        sample: RenderFrameSample,
    ) {
        let win = &mut self.wins[carrier.index()];
        if win.window_start.is_none() {
            win.window_start = Some(now);
        }
        win.observed = win.observed.saturating_add(1);
        if let Some(queue_us) = queue_us {
            win.queue_max_us = Some(win.queue_max_us.unwrap_or(0).max(queue_us));
        }
        if let Some(span_us) = span_us {
            win.span_max_us = Some(win.span_max_us.unwrap_or(0).max(span_us));
        }
        win.sample = sample;
    }

    fn emit_for(
        win: &mut RenderCarrierWin,
        carrier: RenderCarrier,
        now: Instant,
        flushed: Option<&str>,
    ) -> Option<serde_json::Value> {
        if win.observed == 0 {
            return None;
        }
        let window_us = win
            .window_start
            .map(|start| {
                now.saturating_duration_since(start)
                    .as_micros()
                    .min(u128::from(u64::MAX)) as u64
            })
            .unwrap_or(0);
        let mut line = serde_json::json!({
            "event": "render-frame-summary",
            "carrier": carrier.name(),
            "window_us": window_us,
            "observed": win.observed,
            "queue_max_us": win.queue_max_us,
            "span_max_us": win.span_max_us,
            "sample": {
                "outcome": win.sample.outcome,
                "target": win.sample.target,
                "rect": win.sample.outer,
                "color": win.sample.color_hex,
                "dib_checksum": win.sample.checksum_hex,
                "generation": win.sample.generation,
                "safety": win.sample.safety,
                "hidden_proven": win.sample.hidden_proven,
                "id_proven": win.sample.identity_proven,
                "source": win.sample.source,
            },
            "scope": RENDER_FRAME_SCOPE,
            "excludes": RENDER_FRAME_EXCLUDES,
        });
        if let Some(flushed) = flushed {
            line["flushed"] = flushed.into();
        }
        win.observed = 0;
        win.queue_max_us = None;
        win.span_max_us = None;
        win.sample = RenderFrameSample::default();
        win.window_start = None;
        win.last_emit = Some(now);
        Some(line)
    }

    /// Periodic summaries, due only: activity plus a full window elapsed per
    /// carrier. Quiet carriers emit nothing. Per-carrier independence: one
    /// carrier's flood never suppresses another's summary.
    pub(super) fn take_due(&mut self, now: Instant) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for carrier in RenderCarrier::all() {
            let due = Self::window_due(&self.wins[carrier.index()], now);
            if due
                && let Some(line) =
                    Self::emit_for(&mut self.wins[carrier.index()], carrier, now, None)
            {
                out.push(line);
            }
        }
        out
    }

    /// Trailing flush for idle maintenance turns: pending activity below the
    /// window age still emits (marked), throttled to one per second per
    /// carrier from that carrier's last emission. Suppressed carriers keep
    /// their data for the next due or end-of-run flush.
    pub(super) fn take_trailing(&mut self, now: Instant) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for carrier in RenderCarrier::all() {
            let due = Self::repeat_due(self.wins[carrier.index()].last_emit, now);
            if due
                && let Some(line) = Self::emit_for(
                    &mut self.wins[carrier.index()],
                    carrier,
                    now,
                    Some("maintenance-trailing"),
                )
            {
                out.push(line);
            }
        }
        out
    }

    /// Pending end-of-run summary: emits any retained activity regardless of
    /// the throttle (at most one call per run).
    pub(super) fn take_end_of_run(&mut self, now: Instant) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for carrier in RenderCarrier::all() {
            if let Some(line) = Self::emit_for(
                &mut self.wins[carrier.index()],
                carrier,
                now,
                Some("end-of-run"),
            ) {
                out.push(line);
            }
        }
        out
    }
}

/// Transition key for one bridged ack: visibility/outcome/target/reason plus
/// the safety epoch and proof flags, without geometry. Geometry-only churn
/// keeps the same key and aggregates, while a target/visibility/reason change
/// or a safety-epoch/proof-flag change logs immediately. A steady
/// `identity_proven: false` keeps its key (not a per-frame failure), but any
/// proof-flag transition is immediate. Failures map to a `failure:{detail}`
/// key but always log immediately regardless (failures never aggregate,
/// including repeats).
fn render_ack_transition_key(
    state: &TileLoop,
    carrier: RenderCarrier,
    ack: &crate::render_handoff::CarrierAck,
) -> String {
    if let Some(error) = ack.last_error.as_ref() {
        return format!("failure:{error}");
    }
    match &ack.outcome {
        RenderOutcome::Hidden => {
            format!(
                "hidden:{}:s{}:h{}",
                render_hide_reason(state, carrier, ack),
                ack.safety,
                ack.hidden_proven
            )
        }
        RenderOutcome::Refused(reason) => format!("refused:{reason}"),
        RenderOutcome::Unknown(reason) => format!("unknown:{reason}"),
        _ => {
            let target = ack
                .applied
                .as_ref()
                .map(|applied| applied.target.clone())
                .unwrap_or_default();
            format!(
                "shown:{target}:s{}:h{}:i{}",
                ack.safety, ack.hidden_proven, ack.identity_proven
            )
        }
    }
}

/// Whether one bridged ack is a failure that must log immediately (failures
/// never aggregate, including repeated identical failures).
fn render_ack_is_failure(ack: &crate::render_handoff::CarrierAck) -> bool {
    if ack.last_error.is_some() {
        return true;
    }
    match &ack.outcome {
        RenderOutcome::Refused(_) | RenderOutcome::Unknown(_) => true,
        RenderOutcome::Hidden if !ack.hidden_proven => true,
        _ => false,
    }
}

/// Same-thread routine gate for the overlay show paths (no ack transport, so
/// the sample carries `source: "same-thread"` with null generation/safety/
/// queue/span and unproven flags): returns true when the caller must suppress
/// its immediate status line because the frame aggregated into the per-carrier
/// ~1s window. A changed `shown:{target}` key records the transition and
/// returns false (the caller logs immediately per its existing gates); an
/// unchanged key with `would_log` true aggregates and returns true; an
/// unchanged key with `would_log` false stays silent with no aggregation
/// (preserving steady-state silence). Failures and hides never reach here:
/// callers keep their existing immediate failure/hide branches untouched, and
/// those branches record their own `hidden:`/`failure:` transition keys (see
/// `note_render_hidden`) so a later show under the same target still logs its
/// lifecycle instead of aggregating against a stale show key.
pub(super) fn render_same_thread_gate(
    state: &mut TileLoop,
    carrier: RenderCarrier,
    target: &str,
    now: Instant,
    would_log: bool,
    sample: RenderFrameSample,
) -> bool {
    let key = format!("shown:{target}");
    if state.render_frames.transition_key(carrier) != Some(key.as_str()) {
        state.render_frames.note_transition(carrier, key);
        return false;
    }
    if !would_log {
        return false;
    }
    state
        .render_frames
        .observe_routine(carrier, now, None, None, sample);
    true
}

/// Record a same-thread hide or present-failure transition in the diagnostic
/// key state before its log line. Product behaviour is untouched (the overlay
/// hide and signature update already happened in the caller); this only keeps
/// the routine gate honest so a later show under the same target logs its
/// lifecycle instead of aggregating against a stale show key.
pub(super) fn note_render_hidden(state: &mut TileLoop, carrier: RenderCarrier, key: String) {
    state.render_frames.note_transition(carrier, key);
}

/// Same-thread routine sample: no ack transport exists on the inline overlay
/// path, so generation/safety/queue/span stay null and proof flags stay
/// unproven in the summary sample (`source: "same-thread"`).
pub(super) fn same_thread_sample(
    outcome: &'static str,
    target: String,
    outer: [i32; 4],
    color_hex: String,
    checksum_hex: String,
) -> RenderFrameSample {
    RenderFrameSample {
        outcome,
        target,
        outer,
        color_hex,
        checksum_hex,
        generation: None,
        safety: None,
        hidden_proven: false,
        identity_proven: false,
        source: "same-thread",
    }
}

fn poll_border_ack(
    state: &mut TileLoop,
    ack: &crate::render_handoff::CarrierAck,
    checksum_hex: &str,
) {
    let tick = state.tick;
    let trace = state.trace;
    let log_path = state.log_path.clone();
    match &ack.outcome {
        RenderOutcome::Hidden if !ack.hidden_proven => {
            // Attempted but unverified: unknown visibility, never a hidden
            // claim. The hide stays pending renderer-side for its retry.
            render_prune_notes(state, RenderCarrier::Border, ack.generation);
            let reason = render_hide_reason(state, RenderCarrier::Border, ack);
            let signature = format!("unknown:{reason}");
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Hidden => {
            let reason = render_hide_reason(state, RenderCarrier::Border, ack);
            render_prune_notes(state, RenderCarrier::Border, ack.generation);
            let signature = format!("hidden:{reason}");
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if changed || trace {
                let mut event = serde_json::json!({
                    "event": "active-border",
                    "tick": tick,
                    "outcome": "hidden",
                    "reason": reason,
                });
                if let Some(error) = ack.last_error.as_ref() {
                    event["last_error"] = serde_json::Value::from(error.clone());
                }
                log_json_at(&log_path, event);
            }
        }
        RenderOutcome::Refused(reason) => {
            render_prune_notes(state, RenderCarrier::Border, ack.generation);
            let signature = format!("hidden:{reason}");
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": tick,
                        "outcome": "hidden",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Unknown(reason) => {
            render_prune_notes(state, RenderCarrier::Border, ack.generation);
            let signature = format!("unknown:{reason}");
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "active-border",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        outcome => {
            // Content truth rides the ack applied payload (fresh from the
            // presented frame, accent-resolved): a missing note pairing
            // logs nothing rather than an older payload as new.
            let Some(applied) = ack.applied.clone() else {
                return;
            };
            render_prune_notes(state, RenderCarrier::Border, ack.generation);
            let style = applied.style_px.unwrap_or([0, 0, 0]);
            // Same change-only signature as the same-thread line: the
            // presented colour rides it so a theme change that repaints
            // the same geometry still logs once.
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                applied.target,
                applied.outer[0],
                applied.outer[1],
                applied.outer[2],
                applied.outer[3],
                style[0],
                style[1],
                applied.topmost,
                applied.color_hex
            );
            let changed = state.border_last.as_deref() != Some(signature.as_str());
            if changed {
                state.border_last = Some(signature);
            }
            let outcome_name = outcome.name();
            if outcome_name == "moved" {
                if trace {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "active-border",
                            "tick": tick,
                            "outcome": "moved",
                            "target": applied.target,
                            "outer": applied.outer,
                            "style_px": style,
                            "span_us": ack.span_us,
                        }),
                    );
                }
                return;
            }
            if outcome_name == "unchanged" {
                if trace && changed {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "active-border",
                            "tick": tick,
                            "outcome": "unchanged",
                            "target": applied.target,
                        }),
                    );
                }
                return;
            }
            if !(changed || trace) {
                return;
            }
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "active-border",
                    "tick": tick,
                    "outcome": outcome_name,
                    "target": applied.target,
                    "outer": applied.outer,
                    "style_px": style,
                    "color": applied.color_hex,
                    "dib_checksum": checksum_hex,
                    "span_us": ack.span_us,
                    "id_proven": ack.identity_proven,
                }),
            );
        }
    }
}

fn poll_underlay_ack(
    state: &mut TileLoop,
    ack: &crate::render_handoff::CarrierAck,
    checksum_hex: &str,
) {
    let tick = state.tick;
    let trace = state.trace;
    let log_path = state.log_path.clone();
    match &ack.outcome {
        RenderOutcome::Hidden if !ack.hidden_proven => {
            render_prune_notes(state, RenderCarrier::Underlay, ack.generation);
            let reason = render_hide_reason(state, RenderCarrier::Underlay, ack);
            let signature = format!("unknown:{reason}");
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Hidden | RenderOutcome::Refused(_) => {
            let reason = match &ack.outcome {
                RenderOutcome::Refused(reason) => (*reason).to_owned(),
                _ => render_hide_reason(state, RenderCarrier::Underlay, ack),
            };
            render_prune_notes(state, RenderCarrier::Underlay, ack.generation);
            let signature = format!("hidden:{reason}");
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": tick,
                        "outcome": "hidden",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Unknown(reason) => {
            render_prune_notes(state, RenderCarrier::Underlay, ack.generation);
            let signature = format!("unknown:{reason}");
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "group-underlay",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        outcome => {
            let Some(applied) = ack.applied.clone() else {
                return;
            };
            // Diagnostics extras still ride the note pairing; content truth
            // rides the ack applied payload.
            let note = render_note_for(state, RenderCarrier::Underlay, ack.generation);
            render_prune_notes(state, RenderCarrier::Underlay, ack.generation);
            let signature = format!(
                "shown:{}:{}:{}:{}:{}:{}:{}:{}",
                applied.target,
                applied.outer[0],
                applied.outer[1],
                applied.outer[2],
                applied.outer[3],
                applied.topmost,
                applied.color_hex,
                checksum_hex
            );
            let changed = state.underlay_last.as_deref() != Some(signature.as_str());
            if changed {
                state.underlay_last = Some(signature);
            }
            let outcome_name = outcome.name();
            if outcome_name == "moved" {
                if trace {
                    let mut event = serde_json::json!({
                        "event": "group-underlay",
                        "tick": tick,
                        "outcome": "moved",
                        "target": applied.target,
                        "outer": applied.outer,
                        "span_us": ack.span_us,
                    });
                    if let Some(fields) = note
                        .as_ref()
                        .and_then(|note| note.debug.as_ref())
                        .and_then(|d| d.as_object())
                    {
                        for (key, value) in fields {
                            event[key] = value.clone();
                        }
                    }
                    log_json_at(&log_path, event);
                }
                return;
            }
            if outcome_name == "unchanged" {
                if trace && changed {
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "group-underlay",
                            "tick": tick,
                            "outcome": "unchanged",
                            "target": applied.target,
                        }),
                    );
                }
                return;
            }
            if !(changed || trace) {
                return;
            }
            let mut event = serde_json::json!({
                "event": "group-underlay",
                "tick": tick,
                "outcome": outcome_name,
                "target": applied.target,
                "outer": applied.outer,
                "color": applied.color_hex,
                "members": applied.members,
                "dib_checksum": checksum_hex,
                "span_us": ack.span_us,
                "id_proven": ack.identity_proven,
            });
            if let Some(fields) = note
                .as_ref()
                .and_then(|note| note.debug.as_ref())
                .and_then(|d| d.as_object())
            {
                for (key, value) in fields {
                    event[key] = value.clone();
                }
            }
            log_json_at(&log_path, event);
        }
    }
}

fn poll_preview_ack(
    state: &mut TileLoop,
    ack: &crate::render_handoff::CarrierAck,
    checksum_hex: &str,
) {
    let tick = state.tick;
    let trace = state.trace;
    let log_path = state.log_path.clone();
    match &ack.outcome {
        RenderOutcome::Hidden if !ack.hidden_proven => {
            render_prune_notes(state, RenderCarrier::Preview, ack.generation);
            let reason = render_hide_reason(state, RenderCarrier::Preview, ack);
            let signature = format!("unknown:{reason}");
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Hidden | RenderOutcome::Refused(_) => {
            let reason = match &ack.outcome {
                RenderOutcome::Refused(reason) => (*reason).to_owned(),
                _ => render_hide_reason(state, RenderCarrier::Preview, ack),
            };
            render_prune_notes(state, RenderCarrier::Preview, ack.generation);
            let signature = format!("hidden:{reason}");
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": tick,
                        "outcome": "hidden",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        RenderOutcome::Unknown(reason) => {
            render_prune_notes(state, RenderCarrier::Preview, ack.generation);
            let signature = format!("unknown:{reason}");
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": tick,
                        "outcome": "unknown",
                        "reason": reason,
                        "last_error": ack.last_error,
                    }),
                );
            }
        }
        _ => {
            let Some(applied) = ack.applied.clone() else {
                return;
            };
            let note = render_note_for(state, RenderCarrier::Preview, ack.generation);
            render_prune_notes(state, RenderCarrier::Preview, ack.generation);
            let signature = format!(
                "shown:{}:{},{},{},{}:{checksum_hex}",
                applied.target,
                applied.outer[0],
                applied.outer[1],
                applied.outer[2],
                applied.outer[3]
            );
            let changed = state.preview_last.as_deref() != Some(signature.as_str());
            if changed {
                state.preview_last = Some(signature);
            }
            if changed || trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "drag-preview",
                        "tick": tick,
                        "correlation": note.as_ref().map(|note| note.correlation.clone()).unwrap_or_default(),
                        "outcome": ack.outcome.name(),
                        "window": applied.target,
                        "rect": applied.outer,
                        "hover_prior": note.as_ref().map(|note| note.prior.clone()).unwrap_or_default(),
                        "span_us": ack.span_us,
                        "id_proven": ack.identity_proven,
                    }),
                );
            }
        }
    }
}

/// Read-only applied snapshot for one renderer carrier at graceful stop:
/// the last acknowledged applied content plus its ack generation, outcome
/// and owned-pixel checksum. Hidden when the carrier reads hidden or never
/// presented; explicitly unknown when the last ack proved nothing (so
/// inspection never claims desired-or-stale pixels). Feeds the `*-end`
/// lines. The payload shape mirrors the carrier `snapshot()` vocabulary
/// (visibility, target, geometry, colour) without foreign identifiers, plus
/// the ack evidence.
pub(super) fn render_end_snapshot(state: &TileLoop, carrier: RenderCarrier) -> serde_json::Value {
    let bridge = state.render_bridge.as_ref();
    let index = carrier.index();
    let (visible, known, ack) = match bridge {
        Some(bridge) => (
            bridge.applied_visible[index],
            bridge.applied_known[index],
            bridge.applied_ack[index].clone(),
        ),
        None => return serde_json::json!({"visible": false}),
    };
    if !known {
        return serde_json::json!({"visibility": "unknown", "visible": visible});
    }
    if !visible {
        return serde_json::json!({"visible": false});
    }
    match ack.and_then(|ack| {
        ack.applied.map(|applied| {
            (
                ack.generation,
                ack.outcome.name().to_owned(),
                ack.checksum,
                applied,
            )
        })
    }) {
        Some((generation, outcome, checksum, applied)) => serde_json::json!({
            "visible": true,
            "target": applied.target,
            "outer": applied.outer,
            "color": applied.color_hex,
            "generation": generation,
            "outcome": outcome,
            "dib_checksum": format!("{:016x}", checksum),
        }),
        None => serde_json::json!({"visible": false}),
    }
}
