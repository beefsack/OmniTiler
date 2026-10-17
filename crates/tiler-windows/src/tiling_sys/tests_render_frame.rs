#![forbid(unsafe_code)]
use super::{
    RENDER_SUMMARY_WINDOW, RenderBridge, RenderCarrier, RenderFrameHealth, RenderFrameSample,
    render_same_thread_gate,
};
use crate::render_handoff::{AppliedContent, CarrierAck, RenderChannels, RenderOutcome};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn temp_log(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tiler-render-frame-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn log_values(path: &std::path::Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line parses"))
        .collect()
}

fn sample_for(target: &str, x: i32, outcome: &'static str) -> RenderFrameSample {
    RenderFrameSample {
        outcome,
        target: target.to_owned(),
        outer: [x, 0, 100, 100],
        color_hex: "#ff0000".to_owned(),
        checksum_hex: "0123456789abcdef".to_owned(),
        generation: Some(7),
        safety: Some(1),
        hidden_proven: false,
        identity_proven: true,
        source: "ack",
    }
}

/// Test ack builder: `proof` aggregates the safety epoch plus the
/// identity-proof flag (one tuple, keeping the helper under the arg
/// limit); geometry/transport stay positional like the callers read.
fn ack_ex(
    generation: u64,
    target: &str,
    x: i32,
    outcome: RenderOutcome,
    proof: (u64, bool),
    queue_us: u64,
    span_us: u64,
) -> CarrierAck {
    CarrierAck {
        generation,
        safety: proof.0,
        outcome,
        checksum: 0xabcd,
        span_us,
        queue_age_us: queue_us,
        last_error: None,
        hidden_proven: false,
        identity_proven: proof.1,
        applied: Some(AppliedContent::new(
            target,
            [x, 0, 100, 100],
            "#ff0000".to_owned(),
            Some([2, 2, 2]),
            true,
            1,
        )),
    }
}

fn shown_ack(generation: u64, target: &str, x: i32, queue_us: u64, span_us: u64) -> CarrierAck {
    ack_ex(
        generation,
        target,
        x,
        RenderOutcome::Shown,
        (1, true),
        queue_us,
        span_us,
    )
}

fn refused_ack(generation: u64, reason: &'static str) -> CarrierAck {
    CarrierAck {
        generation,
        safety: 1,
        outcome: RenderOutcome::Refused(reason),
        checksum: 0,
        span_us: 3,
        queue_age_us: 0,
        last_error: None,
        hidden_proven: false,
        identity_proven: false,
        applied: None,
    }
}

/// Offline owner fixture with a bridged (channel-only, no thread) renderer
/// and a real temp log-file sink.
fn bridged_state(log_path: &std::path::Path, trace: bool) -> super::TileLoop {
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = log_path.to_owned();
    state.trace = trace;
    state.render_bridge = Some(RenderBridge {
        channels: Arc::new(RenderChannels::new()),
        handle: None,
        gens: [0, 0, 0],
        hide_posted: [false, false, false],
        hide_reason: [None, None, None],
        hidden_ack: [None, None, None],
        posted: std::array::from_fn(|_| VecDeque::new()),
        applied_visible: [false, false, false],
        applied_note: [None, None, None],
        applied_ack: [None, None, None],
        applied_known: [false, false, false],
        admission_open: true,
        pending: None,
    });
    state
}

fn post_ack(state: &super::TileLoop, carrier: RenderCarrier, ack: CarrierAck) {
    state
        .render_bridge
        .as_ref()
        .expect("bridge")
        .channels
        .post_ack(carrier, ack);
}

#[test]
fn struct_flood_aggregates_with_maxima_and_latest_sample() {
    let mut health = RenderFrameHealth::default();
    let start = Instant::now();
    health.note_transition(RenderCarrier::Border, "shown:win-a".to_owned());
    for i in 0..3000u64 {
        health.observe_routine(
            RenderCarrier::Border,
            start,
            Some(100 + (i % 7)),
            Some(5 + (i % 3)),
            sample_for("win-a", i as i32, "shown"),
        );
    }
    // No window elapsed: nothing due yet.
    assert!(
        health
            .take_due(start + Duration::from_millis(999))
            .is_empty()
    );
    let lines = health.take_due(start + RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert_eq!(line["event"], "render-frame-summary");
    assert_eq!(line["carrier"], "border");
    assert_eq!(line["observed"], 3000);
    assert_eq!(line["queue_max_us"], 106);
    assert_eq!(line["span_max_us"], 7);
    assert_eq!(line["sample"]["outcome"], "shown");
    assert_eq!(line["sample"]["target"], "win-a");
    assert_eq!(
        line["sample"]["rect"],
        serde_json::json!([2999, 0, 100, 100])
    );
    assert!(line.get("flushed").is_none());
    let text = line.to_string().to_lowercase();
    for banned in ["hwnd", "pid", "exe", "title"] {
        assert!(!text.contains(banned), "no identity in {line}");
    }
}

#[test]
fn struct_per_carrier_independence_and_reset() {
    let mut health = RenderFrameHealth::default();
    let start = Instant::now();
    for carrier in RenderCarrier::all() {
        health.note_transition(carrier, "shown:win-a".to_owned());
    }
    for i in 0..500u64 {
        health.observe_routine(
            RenderCarrier::Border,
            start,
            Some(i),
            Some(1),
            sample_for("win-a", 0, "shown"),
        );
        health.observe_routine(
            RenderCarrier::Preview,
            start,
            Some(2 * i),
            Some(2),
            sample_for("mover", i as i32, "moved"),
        );
    }
    // Underlay idle: only the two active carriers summarize.
    let lines = health.take_due(start + RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let border = lines
        .iter()
        .find(|l| l["carrier"] == "border")
        .expect("border");
    let preview = lines
        .iter()
        .find(|l| l["carrier"] == "preview")
        .expect("preview");
    assert_eq!(border["observed"], 500);
    assert_eq!(border["queue_max_us"], 499);
    assert_eq!(preview["observed"], 500);
    assert_eq!(preview["queue_max_us"], 998);
    // Maxima reset between windows.
    health.observe_routine(
        RenderCarrier::Border,
        start + RENDER_SUMMARY_WINDOW,
        Some(11),
        Some(1),
        sample_for("win-a", 0, "shown"),
    );
    let lines = health.take_due(start + 2 * RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["observed"], 1);
    assert_eq!(lines[0]["queue_max_us"], 11);
}

#[test]
fn struct_no_activity_silence_and_flushes() {
    let mut health = RenderFrameHealth::default();
    let start = Instant::now();
    assert!(health.take_due(start + RENDER_SUMMARY_WINDOW).is_empty());
    assert!(health.take_trailing(start).is_empty());
    assert!(health.take_end_of_run(start).is_empty());

    health.note_transition(RenderCarrier::Underlay, "shown:g".to_owned());
    health.observe_routine(
        RenderCarrier::Underlay,
        start,
        Some(9),
        Some(4),
        sample_for("g", 1, "redrew"),
    );
    let lines = health.take_trailing(start + Duration::from_millis(100));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["flushed"], "maintenance-trailing");
    assert_eq!(lines[0]["observed"], 1);
    assert!(
        health
            .take_end_of_run(start + Duration::from_millis(200))
            .is_empty()
    );

    // Throttled trailing retains data for the next due flush.
    health.observe_routine(
        RenderCarrier::Underlay,
        start + RENDER_SUMMARY_WINDOW,
        Some(30),
        Some(6),
        sample_for("g", 2, "redrew"),
    );
    assert!(
        health
            .take_trailing(start + RENDER_SUMMARY_WINDOW + Duration::from_millis(50))
            .is_empty(),
        "trailing inside one second of the last emit retains data"
    );
    let lines = health.take_due(start + 2 * RENDER_SUMMARY_WINDOW + Duration::from_millis(50));
    assert_eq!(lines.len(), 1, "retained sample still summarizes");
    assert_eq!(lines[0]["observed"], 1);
    assert_eq!(lines[0]["queue_max_us"], 30);

    // End-of-run flushes pending regardless of throttle.
    health.observe_routine(
        RenderCarrier::Underlay,
        start + 2 * RENDER_SUMMARY_WINDOW,
        Some(7),
        Some(1),
        sample_for("g", 3, "redrew"),
    );
    let lines = health.take_end_of_run(start + 2 * RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["flushed"], "end-of-run");
    assert!(
        health
            .take_end_of_run(start + 2 * RENDER_SUMMARY_WINDOW)
            .is_empty()
    );
}

#[test]
fn same_thread_gate_transition_aggregates_and_silence() {
    let mut state = super::rmax03_adapter_tests::test_state();
    let now = Instant::now();
    // New target: transition, caller logs immediately.
    assert!(!render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        true,
        sample_for("win-a", 0, "shown"),
    ));
    // Same target with would-log: aggregate, caller suppresses.
    assert!(render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        true,
        sample_for("win-a", 5, "shown"),
    ));
    // Same target but silent under old rules: no aggregation.
    assert!(!render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        false,
        sample_for("win-a", 6, "shown"),
    ));
    let lines = state.render_frames.take_due(now + RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["observed"], 1);
    assert_eq!(
        lines[0]["sample"]["rect"],
        serde_json::json!([5, 0, 100, 100])
    );
    // New target again: transition, caller logs immediately.
    assert!(!render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-b",
        now,
        true,
        sample_for("win-b", 0, "shown"),
    ));
}

#[test]
fn file_sink_thousands_of_frames_bound_with_transition_and_failure() {
    let log_path = temp_log("flood");
    let mut state = bridged_state(&log_path, true);
    let start = Instant::now();
    // 3000 routine same-target frames with different rects: the first is
    // the visibility transition (immediate proof + status), the rest
    // aggregate into one ~1s summary.
    for i in 0..3000u64 {
        post_ack(
            &state,
            RenderCarrier::Border,
            shown_ack(i + 1, "win-a", i as i32, 100 + (i % 5), 5 + (i % 3)),
        );
        super::poll_render_acks(&mut state, start);
    }
    let lines = log_values(&log_path);
    let applied: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-applied")
        .collect();
    let status: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "active-border")
        .collect();
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    assert_eq!(
        applied.len(),
        1,
        "only the transition keeps render-applied: {lines:?}"
    );
    assert_eq!(applied[0]["outcome"], "shown");
    assert_eq!(status.len(), 1, "only the transition keeps a status line");
    assert_eq!(status[0]["outcome"], "shown");
    assert!(summaries.is_empty(), "window not elapsed yet");
    // Target transition logs immediately, not into the window.
    post_ack(
        &state,
        RenderCarrier::Border,
        shown_ack(4001, "win-b", 0, 50, 5),
    );
    super::poll_render_acks(&mut state, start);
    // Repeated identical failures both log immediately (never aggregated).
    post_ack(
        &state,
        RenderCarrier::Border,
        refused_ack(4002, "scope-excluded"),
    );
    super::poll_render_acks(&mut state, start);
    post_ack(
        &state,
        RenderCarrier::Border,
        refused_ack(4003, "scope-excluded"),
    );
    super::poll_render_acks(&mut state, start);
    let lines = log_values(&log_path);
    let status: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "active-border")
        .collect();
    assert_eq!(
        status.len(),
        4,
        "transition + 2 failures + first: {status:?}"
    );
    assert_eq!(status[1]["outcome"], "shown");
    assert_eq!(status[1]["target"], "win-b");
    assert_eq!(status[2]["outcome"], "hidden");
    assert_eq!(status[2]["reason"], "scope-excluded");
    assert_eq!(status[3]["outcome"], "hidden");
    // Window elapse flushes the routine backlog as one summary.
    super::poll_render_acks(&mut state, start + RENDER_SUMMARY_WINDOW);
    let lines = log_values(&log_path);
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    assert_eq!(summaries.len(), 1, "{lines:?}");
    assert_eq!(summaries[0]["carrier"], "border");
    assert_eq!(summaries[0]["observed"], 2999);
    assert_eq!(summaries[0]["queue_max_us"], 104);
    assert_eq!(summaries[0]["span_max_us"], 7);
    assert_eq!(
        summaries[0]["sample"]["rect"],
        serde_json::json!([2999, 0, 100, 100]),
        "latest applied state rides the labelled sample"
    );
    assert_eq!(summaries[0]["sample"]["generation"], 3000);
    assert_eq!(summaries[0]["sample"]["safety"], 1);
    assert_eq!(summaries[0]["sample"]["hidden_proven"], false);
    assert_eq!(summaries[0]["sample"]["id_proven"], true);
    assert_eq!(summaries[0]["sample"]["outcome"], "shown");
    assert_eq!(summaries[0]["sample"]["source"], "ack");
    assert_eq!(summaries[0]["scope"], super::RENDER_FRAME_SCOPE);
    // Before/after bound: 3000 routine frames + 1 target transition + 2
    // failures collapse to 9 lines (transition proof+status, second
    // transition proof+status, 2 failure proof+status pairs, 1 summary)
    // where the old per-ack trace wrote two lines per frame (~6008).
    assert_eq!(lines.len(), 9, "{lines:?}");
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn file_sink_production_failure_gate_unchanged() {
    // Trace-off failure logging keeps its existing change gate: the first
    // failure logs, an identical repeat stays quiet (bounded failure
    // line), while a new error logs again.
    let log_path = temp_log("fail-gate");
    let mut state = bridged_state(&log_path, false);
    let start = Instant::now();
    post_ack(
        &state,
        RenderCarrier::Border,
        refused_ack(1, "scope-excluded"),
    );
    super::poll_render_acks(&mut state, start);
    post_ack(
        &state,
        RenderCarrier::Border,
        refused_ack(2, "scope-excluded"),
    );
    super::poll_render_acks(&mut state, start);
    let lines = log_values(&log_path);
    assert_eq!(lines.len(), 1, "identical repeat stays quiet: {lines:?}");
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn file_sink_preview_per_move_aggregates() {
    // Per-mouse-move preview churn with the same mover aggregates; the
    // mover transition logs immediately.
    let log_path = temp_log("preview");
    let mut state = bridged_state(&log_path, false);
    let start = Instant::now();
    for i in 0..500u64 {
        let mut ack = shown_ack(i + 1, "mover", 10 + i as i32, 20, 4);
        ack.outcome = RenderOutcome::Shown;
        post_ack(&state, RenderCarrier::Preview, ack);
        super::poll_render_acks(&mut state, start);
    }
    super::poll_render_acks(&mut state, start + RENDER_SUMMARY_WINDOW);
    let lines = log_values(&log_path);
    let status: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "drag-preview")
        .collect();
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    assert_eq!(status.len(), 1, "only the mover transition: {status:?}");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0]["carrier"], "preview");
    assert_eq!(summaries[0]["observed"], 499);
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn file_sink_coalesced_unchanged_changing_geometry_bounded() {
    // The major trace high-rate source: coalesced Shown->Unchanged acks
    // with applied payload and changing geometry aggregate instead of
    // logging render-applied per ack.
    let log_path = temp_log("unchanged");
    let mut state = bridged_state(&log_path, true);
    let start = Instant::now();
    for i in 0..2000u64 {
        post_ack(
            &state,
            RenderCarrier::Border,
            ack_ex(
                i + 1,
                "win-a",
                i as i32,
                RenderOutcome::Unchanged,
                (1, true),
                40 + (i % 9),
                6,
            ),
        );
        super::poll_render_acks(&mut state, start);
    }
    super::poll_render_acks(&mut state, start + RENDER_SUMMARY_WINDOW);
    let lines = log_values(&log_path);
    let applied: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-applied")
        .collect();
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    // First Unchanged is the visibility transition (immediate proof plus
    // the change-gated status line); the other 1999 aggregate.
    assert_eq!(applied.len(), 1, "{lines:?}");
    assert_eq!(applied[0]["outcome"], "unchanged");
    assert_eq!(summaries.len(), 1, "{lines:?}");
    assert_eq!(summaries[0]["observed"], 1999);
    assert_eq!(summaries[0]["queue_max_us"], 48);
    assert_eq!(summaries[0]["sample"]["outcome"], "unchanged");
    assert_eq!(summaries[0]["sample"]["generation"], 2000);
    assert_eq!(summaries[0]["sample"]["safety"], 1);
    assert_eq!(summaries[0]["sample"]["id_proven"], true);
    assert_eq!(summaries[0]["sample"]["source"], "ack");
    assert_eq!(
        summaries[0]["sample"]["rect"],
        serde_json::json!([1999, 0, 100, 100])
    );
    assert!(lines.len() <= 4, "bound holds: {lines:?}");
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn file_sink_alternating_routine_outcomes_aggregate() {
    // Alternating Shown/Redrew/Moved/Unchanged under one target, safety,
    // and proof state aggregates: the key carries no outcome name, so
    // cadence never breaks the window. Transitions and failures would
    // still log immediately (covered by neighbouring tests).
    let log_path = temp_log("alternating");
    let mut state = bridged_state(&log_path, true);
    let start = Instant::now();
    let outcomes = [
        RenderOutcome::Shown,
        RenderOutcome::Redrew,
        RenderOutcome::Moved,
        RenderOutcome::Unchanged,
    ];
    for i in 0..1200u64 {
        let outcome = outcomes[(i % 4) as usize].clone();
        post_ack(
            &state,
            RenderCarrier::Underlay,
            ack_ex(i + 1, "group", i as i32, outcome, (3, true), 25, 5),
        );
        super::poll_render_acks(&mut state, start);
    }
    super::poll_render_acks(&mut state, start + RENDER_SUMMARY_WINDOW);
    let lines = log_values(&log_path);
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    assert_eq!(summaries.len(), 1, "{lines:?}");
    assert_eq!(summaries[0]["carrier"], "underlay");
    assert_eq!(summaries[0]["observed"], 1199);
    // Latest sample wins, so the grouped outcome is the last one seen.
    assert_eq!(summaries[0]["sample"]["outcome"], "unchanged");
    assert_eq!(summaries[0]["sample"]["generation"], 1200);
    assert_eq!(summaries[0]["sample"]["safety"], 3);
    assert!(lines.len() <= 4, "bound holds: {lines:?}");
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn file_sink_safety_and_proof_changes_are_immediate() {
    // Safety-epoch and proof-flag changes log immediately; a steady
    // unproven state aggregates instead of failing per frame.
    let log_path = temp_log("proof-change");
    let mut state = bridged_state(&log_path, true);
    let start = Instant::now();
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(1, "win-a", 0, RenderOutcome::Shown, (1, true), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(2, "win-a", 1, RenderOutcome::Shown, (1, true), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    // Safety epoch bump: immediate even with identical geometry/target.
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(3, "win-a", 1, RenderOutcome::Shown, (2, true), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    // Proof flag flips: immediate both ways.
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(4, "win-a", 1, RenderOutcome::Shown, (2, false), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    // Steady unproven state: aggregates, not a per-frame failure.
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(5, "win-a", 2, RenderOutcome::Shown, (2, false), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    // Proof restored: immediate again.
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(6, "win-a", 2, RenderOutcome::Shown, (2, true), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    post_ack(
        &state,
        RenderCarrier::Border,
        ack_ex(7, "win-a", 3, RenderOutcome::Shown, (2, true), 10, 2),
    );
    super::poll_render_acks(&mut state, start);
    super::poll_render_acks(&mut state, start + RENDER_SUMMARY_WINDOW);
    let lines = log_values(&log_path);
    let applied: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-applied")
        .collect();
    let summaries: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["event"] == "render-frame-summary")
        .collect();
    // Transition + safety bump + two proof flips = 4 immediate proofs.
    assert_eq!(applied.len(), 4, "{lines:?}");
    assert_eq!(summaries.len(), 1, "{lines:?}");
    assert_eq!(summaries[0]["observed"], 3);
    assert_eq!(summaries[0]["sample"]["id_proven"], true);
    assert_eq!(summaries[0]["sample"]["safety"], 2);
    assert_eq!(summaries[0]["sample"]["generation"], 7);
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn same_thread_hide_then_show_preserves_lifecycle() {
    // A same-thread hide records its diagnostic key, so the next show
    // under the same target logs its lifecycle instead of aggregating
    // against the stale show key.
    let log_path = temp_log("hide-show");
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = log_path.clone();
    let now = Instant::now();
    assert!(!super::render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        true,
        super::same_thread_sample(
            "shown",
            "win-a".to_owned(),
            [0, 0, 10, 10],
            String::new(),
            String::new()
        ),
    ));
    super::hide_border(&mut state, "idle");
    // Show after hide: key changed (hidden:idle vs shown:win-a), so the
    // caller must log immediately, not aggregate.
    assert!(!super::render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        true,
        super::same_thread_sample(
            "shown",
            "win-a".to_owned(),
            [0, 0, 10, 10],
            String::new(),
            String::new()
        ),
    ));
    // And the repeat aggregates again.
    assert!(super::render_same_thread_gate(
        &mut state,
        RenderCarrier::Border,
        "win-a",
        now,
        true,
        super::same_thread_sample(
            "shown",
            "win-a".to_owned(),
            [1, 0, 10, 10],
            String::new(),
            String::new()
        ),
    ));
    let lines = log_values(&log_path);
    assert!(
        lines.iter().any(|l| l["outcome"] == "hidden"),
        "hide line kept: {lines:?}"
    );
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn struct_same_thread_samples_keep_timings_null() {
    // Same-thread windows carry no ack transport: maxima stay null (never
    // zero-filled) and the sample is labelled accordingly.
    let mut health = RenderFrameHealth::default();
    let start = Instant::now();
    health.observe_routine(
        RenderCarrier::Border,
        start,
        None,
        None,
        super::same_thread_sample(
            "shown",
            "win-a".to_owned(),
            [3, 0, 10, 10],
            String::new(),
            String::new(),
        ),
    );
    let lines = health.take_due(start + RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1);
    assert!(lines[0]["queue_max_us"].is_null(), "{lines:?}");
    assert!(lines[0]["span_max_us"].is_null(), "{lines:?}");
    assert_eq!(lines[0]["sample"]["source"], "same-thread");
    assert!(lines[0]["sample"]["generation"].is_null());
    assert!(lines[0]["sample"]["safety"].is_null());
    // Acked maxima survive alongside unmeasured samples: only actual ack
    // transport feeds the maxima.
    health.observe_routine(
        RenderCarrier::Border,
        start + RENDER_SUMMARY_WINDOW,
        Some(50),
        Some(9),
        sample_for("win-a", 0, "shown"),
    );
    health.observe_routine(
        RenderCarrier::Border,
        start + RENDER_SUMMARY_WINDOW,
        None,
        None,
        super::same_thread_sample(
            "shown",
            "win-a".to_owned(),
            [4, 0, 10, 10],
            String::new(),
            String::new(),
        ),
    );
    let lines = health.take_due(start + 2 * RENDER_SUMMARY_WINDOW);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["observed"], 2);
    assert_eq!(lines[0]["queue_max_us"], 50);
    assert_eq!(lines[0]["span_max_us"], 9);
}
