#![forbid(unsafe_code)]
use super::*;

use super::owner_work::OwnerWorkGuard;

/// Process-relative monotonic start for the `mono_ms` stamp below.
static LOG_START: OnceLock<Instant> = OnceLock::new();

/// Milliseconds since the first tiling log row of this process. Common
/// monotonic time across normal log records; ticks order work, this measures
/// delays. Never overwrites an existing field.
pub(crate) fn log_mono_ms() -> u64 {
    LOG_START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

pub(super) fn log_json_at(path: &Path, mut value: serde_json::Value) {
    if let Some(row) = value.as_object_mut()
        && row.get("mono_ms").is_none()
    {
        row.insert("mono_ms".to_owned(), serde_json::json!(log_mono_ms()));
    }
    let _ = crate::log_sink::append_json(path, &value);
}

/// Fresh-border refresh (foreground query plus overlay presentation together)
/// with trace-only timing into the owner-work guard. `refresh_active_border`
/// itself is unchanged: no paint-API signature churn.
pub(super) fn refresh_border_timed(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    guard: &mut OwnerWorkGuard<'_>,
) {
    if guard.timing_active() {
        let started = Instant::now();
        refresh_active_border(state, me, fulls);
        guard.add_border_refresh(started.elapsed());
    } else {
        refresh_active_border(state, me, fulls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_sink::{AppendOutcome, LOG_LINE_CAP_BYTES, RunLogSink};
    use std::path::{Path, PathBuf};

    fn temp_log(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omnitiler-trace-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("trace log dir");
        dir.join("run.log")
    }

    #[test]
    fn trace_log_sink_caps_growth_and_keeps_one_closure_row() {
        let path = temp_log("cap");
        let mut sink = RunLogSink::default();
        let cap = 1024u64;
        let row = "{\"event\":\"routine\"}";
        let mut outcome = AppendOutcome::Written;
        for _ in 0..200 {
            outcome = sink.append(&path, row, cap).expect("append");
            if outcome == AppendOutcome::Capped {
                break;
            }
        }
        assert_eq!(outcome, AppendOutcome::Capped);
        let bounded = std::fs::metadata(&path).expect("log metadata").len();
        assert!(bounded <= cap, "run log stays bounded: {bounded} <= {cap}");
        let text = std::fs::read_to_string(&path).expect("read log");
        let last = text.lines().last().expect("closure row");
        let marker: serde_json::Value = serde_json::from_str(last).expect("marker parses");
        assert_eq!(marker["event"], serde_json::json!("log-capped"));
        assert_eq!(marker["cap_bytes"], serde_json::json!(cap));
        // A capped sink is idempotent: no second marker and no further bytes.
        assert_eq!(
            sink.append(&path, row, cap).expect("capped append"),
            AppendOutcome::Capped
        );
        assert_eq!(
            std::fs::metadata(&path).expect("log metadata").len(),
            bounded
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn trace_log_sink_omits_oversized_row_with_parseable_marker() {
        let path = temp_log("oversized");
        let mut sink = RunLogSink::default();
        let oversized = "x".repeat(LOG_LINE_CAP_BYTES + 1);
        assert_eq!(
            sink.append(&path, &oversized, 4096).expect("append"),
            AppendOutcome::LineOmitted
        );
        let text = std::fs::read_to_string(&path).expect("read log");
        let marker: serde_json::Value =
            serde_json::from_str(text.trim_end()).expect("marker parses");
        assert_eq!(marker["event"], serde_json::json!("log-line-omitted"));
        assert_eq!(
            marker["bytes"],
            serde_json::json!(LOG_LINE_CAP_BYTES + 1),
            "the marker names the rejected size, never the content"
        );
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn trace_log_json_injects_mono_ms_and_writes_one_row() {
        let path = temp_log("mono");
        log_json_at(&path, serde_json::json!({"event": "probe"}));
        let text = std::fs::read_to_string(&path).expect("read log");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let value: serde_json::Value = serde_json::from_str(lines[0]).expect("row parses");
        assert_eq!(value["event"], serde_json::json!("probe"));
        assert!(value["mono_ms"].is_u64(), "common clock stamp is present");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn trace_panic_lane_records_at_cap_but_not_past_headroom() {
        let path = temp_log("emergency");
        let cap = 256u64;
        let cap_bytes = usize::try_from(cap).expect("test cap fits usize");
        let row = "{\"event\":\"panic\"}";
        std::fs::write(&path, vec![b'x'; cap_bytes]).expect("full log");
        assert_eq!(
            crate::log_sink::append_emergency_line_with_cap(&path, row, cap)
                .expect("capped emergency append"),
            AppendOutcome::Capped
        );
        assert_eq!(
            std::fs::metadata(&path).expect("log metadata").len(),
            cap,
            "no bytes beyond the emergency cap"
        );
        std::fs::write(&path, vec![b'x'; cap_bytes - row.len() - 1]).expect("room below cap");
        assert_eq!(
            crate::log_sink::append_emergency_line_with_cap(&path, row, cap)
                .expect("emergency append"),
            AppendOutcome::Written
        );
        let text = std::fs::read_to_string(&path).expect("read log");
        assert!(text.ends_with("{\"event\":\"panic\"}\n"));
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }

    /// Diagnostic audit: every run-log append site shares the bounded sink.
    /// A new direct append-mode open fails here unless it is the sink itself
    /// or a test-only fixture, which keeps the F26 bound from regressing.
    #[test]
    fn trace_run_log_append_sites_share_the_bounded_sink() {
        let needle = concat!(".append", "(true)");
        fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect(&path, out);
                } else if path
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|ext| ext == "rs")
                {
                    out.push(path);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect(&root, &mut files);
        let mut direct = Vec::new();
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !text.contains(needle) {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let test_fixture = relative
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("tests_"));
            if relative != "log_sink.rs" && !test_fixture {
                direct.push(relative);
            }
        }
        assert!(
            direct.is_empty(),
            "unbounded run-log append sites bypass log_sink: {direct:?}"
        );
    }

    /// Diagnostic audit: these rows identify windows by opaque token only.
    /// App identity and pid are allowed by decision, but these rows need
    /// neither; titles, content, paths, SIDs/sessions and raw native handles
    /// never appear in them. Trace-only native correlation rows are the
    /// separate opt-in lane documented in `log_sink.rs`.
    #[test]
    fn trace_normal_diagnostic_payloads_stay_redacted() {
        let payloads = [
            crate::tiling_sys::visibility_diag::reveal_log_line(
                7,
                "op-1",
                Some("w1"),
                "revealed",
                Some("hide-claim"),
                None,
                12,
            ),
            crate::tiling_sys::visibility_diag::hide_log_line(
                7,
                "op-1",
                Some("w1"),
                "hidden",
                None,
                false,
                12,
            ),
            crate::tiling_sys::adoption_baseline::trace_line(
                "w1",
                crate::tiling_sys::adoption_baseline::CaptureOutcome::Captured,
            ),
            crate::panic_diag::panic_event("owner", "trace.rs", Some(1), Some(2), "str"),
        ];
        for value in payloads {
            let line = serde_json::to_string(&value).expect("payload serializes");
            let lower = line.to_ascii_lowercase();
            for forbidden in [
                "hwnd",
                "handle",
                "title",
                "content",
                "exe_path",
                "full_path",
                "user_sid",
                "session_id",
                "creation",
                "pid",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "diagnostic payload leaks {forbidden}: {line}"
                );
            }
        }
    }
}
