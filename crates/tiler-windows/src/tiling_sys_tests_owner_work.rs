use super::{OwnerWorkGuard, hide_border, hide_underlay};
use crate::input_health::Health;
use std::time::{Duration, Instant};

fn temp_log(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tiler-owner-work-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn log_lines(path: &std::path::Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines().map(|line| line.to_owned()).collect()
}

#[test]
fn hidden_hide_dedupes_identical_repeats_but_keeps_first_and_reason_change() {
    let log_path = temp_log("hide-dedup");
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = log_path.clone();
    state.trace = true;
    // First hide logs (proof scripts need the first hide, not repeats).
    hide_border(&mut state, "idle");
    hide_underlay(&mut state, "idle");
    // Identical already-hidden repeats stay silent, even when tracing.
    hide_border(&mut state, "idle");
    hide_underlay(&mut state, "idle");
    hide_border(&mut state, "idle");
    hide_underlay(&mut state, "idle");
    // Reason changes still log.
    hide_border(&mut state, "suspended");
    hide_underlay(&mut state, "suspended");
    let lines = log_lines(&log_path);
    let _ = std::fs::remove_file(&log_path);
    let borders: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("active-border"))
        .collect();
    let underlays: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("group-underlay"))
        .collect();
    assert_eq!(borders.len(), 2, "first + reason change: {lines:?}");
    assert_eq!(underlays.len(), 2, "first + reason change: {lines:?}");
    assert!(borders[0].contains("\"reason\":\"idle\""), "{lines:?}");
    assert!(borders[1].contains("\"reason\":\"suspended\""), "{lines:?}");
    assert!(underlays[0].contains("\"reason\":\"idle\""), "{lines:?}");
    assert!(
        underlays[1].contains("\"reason\":\"suspended\""),
        "{lines:?}"
    );
}

#[test]
fn owner_work_guard_stays_silent_when_trace_disabled_or_quiet() {
    // Trace-disabled: arm is a no-op and no file appears.
    let log_path = temp_log("disabled");
    {
        let mut guard = OwnerWorkGuard::begin(false, &log_path, 0, 7, None);
        guard.arm(3, 1, 1, 1, true);
        guard.add_border_refresh(Duration::from_micros(5));
    }
    assert!(
        !log_path.exists(),
        "trace-disabled guard must not touch the log"
    );
    // Quiet trace (never armed): drops silently with no lines.
    let log_path = temp_log("quiet");
    {
        let mut health = Health::new();
        let _guard = OwnerWorkGuard::begin(true, &log_path, 0, 7, Some(&mut health));
        assert_eq!(log_lines(&log_path).len(), 0);
    }
    assert_eq!(log_lines(&log_path).len(), 0);
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn owner_work_guard_aggregates_instead_of_per_wake_writes() {
    // Armed samples aggregate into the window with no per-wake file
    // line; the early-return path still commits exactly once via RAII.
    let log_path = temp_log("aggregate");
    let mut health = Health::new();
    {
        let mut guard = OwnerWorkGuard::begin(true, &log_path, 4, 9, Some(&mut health));
        assert!(
            !guard.timing_active(),
            "guard starts disarmed so quiet paths stay silent"
        );
        guard.arm(3, 1, 1, 1, true);
        assert!(guard.timing_active());
        guard.add_border_refresh(Duration::from_micros(12));
        guard.add_border_refresh(Duration::from_micros(8));
        guard.finish_turn(false);
    }
    {
        // Early exit without `finish_turn`: `Drop` still commits.
        let mut guard = OwnerWorkGuard::begin(true, &log_path, 5, 10, Some(&mut health));
        guard.arm(1, 0, 1, 0, false);
    }
    assert_eq!(
        log_lines(&log_path).len(),
        0,
        "non-outlier samples write nothing per wake"
    );
    let _ = std::fs::remove_file(&log_path);
    let lines = health.take_end_of_run(Instant::now());
    assert_eq!(lines.len(), 1, "{lines:?}");
    let summary = &lines[0];
    assert_eq!(summary["event"], "owner-work-summary");
    assert_eq!(summary["samples"], 2, "explicit plus Drop-path commit");
    assert_eq!(summary["border_refresh_max_us"], 20);
    assert_eq!(summary["batch_events_total"], 4);
    assert_eq!(summary["outliers"], 0);
    assert_eq!(summary["scope"], crate::input_health::OWNER_SCOPE);
    for banned in ["hwnd", "pid", "exe", "title", "token"] {
        assert!(
            !summary.to_string().to_lowercase().contains(banned),
            "no identity in {summary}"
        );
    }
}

#[test]
fn owner_work_guard_outlier_is_immediate_and_counted_in_summary() {
    // A slow turn logs one immediate outlier; the summary still
    // carries the outlier total (immediate records keep window data).
    let log_path = temp_log("outlier");
    let mut health = Health::new();
    {
        let mut guard = OwnerWorkGuard::begin(true, &log_path, 6, 11, Some(&mut health));
        guard.arm(2, 1, 0, 0, false);
        std::thread::sleep(Duration::from_millis(60));
        guard.finish_turn(false);
    }
    let lines = log_lines(&log_path);
    let _ = std::fs::remove_file(&log_path);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let outlier: serde_json::Value = serde_json::from_str(&lines[0]).expect("outlier parses");
    assert_eq!(outlier["event"], "owner-work-outlier");
    assert_eq!(outlier["seq"], 6);
    assert_eq!(outlier["tick_at_start"], 11);
    assert!(
        outlier["busy_us"].as_u64().unwrap_or(0) > crate::input_health::OWNER_BUSY_OUTLIER_US,
        "{outlier}"
    );
    assert_eq!(outlier["stream_outliers_total"], 1);
    let trailing = health.take_end_of_run(Instant::now());
    assert_eq!(trailing.len(), 1);
    assert_eq!(trailing[0]["event"], "owner-work-summary");
    assert_eq!(trailing[0]["samples"], 1);
    assert_eq!(trailing[0]["outliers"], 1, "summary keeps the total");
}
