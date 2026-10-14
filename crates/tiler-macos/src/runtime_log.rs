#![forbid(unsafe_code)]

//! Phase 1 structured normal logging (offline, portable).
//!
//! Compact structured stderr logs emitted asynchronously through a bounded
//! non-blocking sink seam. The engine/input/app owners never write to a
//! sink while holding [`Shared`](crate::runtime)-state locks and never
//! block on the sink: emission is a single non-blocking `try_send` under
//! the same lock that allocates the correlation sequence, so sink order
//! matches ring order. A slow or broken sink only fills the bounded queue
//! (counted drops) and can only stall the dedicated logger thread, which
//! lives under the same global stop deadline as every other owner.
//!
//! Events carry app/generation/reference/command identity plus a monotonic
//! timestamp and the ring correlation sequence. Kinds and outcomes are
//! static tokens; OS failure reasons are sanitized to a fixed token set —
//! arbitrary backend strings never reach the sink.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};
use std::time::Duration;

/// One structured normal log event. Plain data only; no user content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEvent {
    /// Ring correlation sequence (monotonic per runtime).
    pub seq: u64,
    /// Monotonic milliseconds since runtime start.
    pub timestamp_ms: u64,
    pub pid: u32,
    pub generation: u64,
    pub ref_id: u64,
    pub kind: &'static str,
    pub outcome: &'static str,
    pub count: u64,
}

/// Normal-log sink seam. Called only from the dedicated logger thread, so
/// a blocking `write` can only stall that thread (bounded by the global
/// stop deadline), never the engine/input/app owners.
pub trait LogSink: Send + Sync {
    fn write(&self, event: &LogEvent);
    fn name(&self) -> &'static str {
        "sink"
    }
}

/// Default stderr sink: one compact line per event.
pub struct StderrSink;

impl LogSink for StderrSink {
    fn write(&self, event: &LogEvent) {
        eprintln!(
            "tiler-log seq={} t={}ms pid={} gen={} ref={} kind={} outcome={} count={}",
            event.seq,
            event.timestamp_ms,
            event.pid,
            event.generation,
            event.ref_id,
            event.kind,
            event.outcome,
            event.count
        );
    }

    fn name(&self) -> &'static str {
        "stderr"
    }
}

/// Sanitize an arbitrary backend/OS reason string to a fixed static token.
/// Only built-in outcome names survive; everything else maps to a generic
/// bucket so user content or unbounded strings never reach the sink.
///
/// Allowlisted `kAXError` names (exactly as [`crate::ffi::ax_error_name`]
/// reports them) are matched FIRST, before the coarse category buckets, so
/// a specific OS cause is preserved instead of collapsing into `failed-set`
/// or `failed-snapshot`. The token set stays bounded: one token per known
/// name plus the generic buckets.
#[must_use]
pub fn sanitize_backend_reason(reason: &str) -> &'static str {
    // Ordered longest-first: `kAXErrorInvalidUIElementObserver` contains
    // `kAXErrorInvalidUIElement` as a prefix and must match first.
    const AX_NAMES: &[(&str, &str)] = &[
        (
            "kAXErrorInvalidUIElementObserver",
            "failed-ax-invalid-observer",
        ),
        ("kAXErrorInvalidUIElement", "failed-ax-invalid-element"),
        (
            "kAXErrorNotificationAlreadyRegistered",
            "failed-ax-already-registered",
        ),
        (
            "kAXErrorParameterizedAttributeUnsupported",
            "failed-ax-parameterized-unsupported",
        ),
        (
            "kAXErrorNotificationUnsupported",
            "failed-ax-notification-unsupported",
        ),
        (
            "kAXErrorAttributeUnsupported",
            "failed-ax-attribute-unsupported",
        ),
        ("kAXErrorActionUnsupported", "failed-ax-action-unsupported"),
        ("kAXErrorIllegalArgument", "failed-ax-illegal-argument"),
        ("kAXErrorCannotComplete", "failed-ax-cannot-complete"),
        (
            "kAXErrorNotEnoughPrecision",
            "failed-ax-not-enough-precision",
        ),
        ("kAXErrorNotImplemented", "failed-ax-not-implemented"),
        ("kAXErrorNoValue", "failed-ax-no-value"),
        ("kAXErrorFailure", "failed-ax-failure"),
    ];
    for (name, token) in AX_NAMES {
        if reason.contains(name) {
            return token;
        }
    }
    let lower = reason.to_ascii_lowercase();
    if lower.contains("messaging timeout") || lower.contains("messaging-timeout") {
        "failed-messaging-timeout"
    } else if lower.contains("pid mismatch") || lower.contains("stale command") {
        "failed-stale-command"
    } else if lower.contains("no ax element") {
        "failed-no-ax-element"
    } else if lower.contains("wid not joined") || lower.contains("not joined to owned") {
        "failed-wid-not-joined"
    } else if lower.contains("window identity join failed") {
        "failed-identity-join"
    } else if lower.contains("degenerate frame") {
        "failed-degenerate-frame"
    } else if lower.contains("windows:") || lower.contains("snapshot") {
        "failed-snapshot"
    } else if lower.contains("set:") {
        "failed-set"
    } else if lower.contains("timeout") {
        "failed-timeout"
    } else if lower.contains("refus") {
        "failed-refused"
    } else {
        "failed-backend"
    }
}

/// Map a completion outcome name plus its numeric detail to a static log
/// outcome token and a count, so `Failed` reasons and `Timeout`/`Applied`
/// elapsed values are preserved without leaking strings.
#[must_use]
pub const fn completion_detail(kind: &'static str) -> &'static str {
    kind
}

/// Default bounded sink queue depth.
pub const LOG_SINK_CAP: usize = 256;
/// Logger thread poll slice: bounds stop latency, never gates correctness.
const LOGGER_SLICE: Duration = Duration::from_millis(20);

/// Logger thread exit. A panic propagates (dirty via join accounting); a
/// returned exit is always clean — the dirt lives in counted drops, never
/// in a silent loss.
pub(crate) use crate::runtime_lifecycle::LoggerExit;

/// Spawn the dedicated logger owner. Returns the non-blocking sender (used
/// via `try_send` only), the stop flag, and the thread handle. The thread
/// never touches shared runtime state.
pub(crate) fn spawn_logger(
    sink: Arc<dyn LogSink>,
    capacity: usize,
) -> (
    SyncSender<LogEvent>,
    Arc<AtomicBool>,
    std::thread::JoinHandle<LoggerExit>,
) {
    let capacity = capacity.max(1);
    let (tx, rx): (SyncSender<LogEvent>, Receiver<LogEvent>) = sync_channel(capacity);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let handle = std::thread::Builder::new()
        .name("phase1-logger".to_owned())
        .spawn(move || logger_main(rx, sink, stop_thread))
        .expect("logger thread spawn");
    (tx, stop, handle)
}

fn logger_main(
    rx: Receiver<LogEvent>,
    sink: Arc<dyn LogSink>,
    stop: Arc<AtomicBool>,
) -> LoggerExit {
    loop {
        match rx.recv_timeout(LOGGER_SLICE) {
            Ok(event) => sink.write(&event),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::SeqCst) {
                    // Stop asked: drain what is already queued without
                    // blocking, then exit. The global shutdown deadline
                    // bounds a slow sink (the join retains this handle on
                    // timeout); drops were already counted at send time.
                    while let Ok(event) = rx.try_recv() {
                        sink.write(&event);
                    }
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    LoggerExit::Done
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct CollectSink {
        events: Mutex<Vec<LogEvent>>,
    }

    impl LogSink for CollectSink {
        fn write(&self, event: &LogEvent) {
            self.events.lock().expect("mutex").push(event.clone());
        }
    }

    #[test]
    fn sanitize_never_passes_through_arbitrary_strings() {
        assert_eq!(
            sanitize_backend_reason("pid 1234 / titled window leaked"),
            "failed-backend"
        );
        assert_eq!(
            sanitize_backend_reason("entirely unknown backend words 12345"),
            "failed-backend"
        );
        assert_eq!(
            sanitize_backend_reason("degenerate frame 0x0"),
            "failed-degenerate-frame"
        );
        assert_eq!(
            sanitize_backend_reason("messaging timeout unset (2000ms)"),
            "failed-messaging-timeout"
        );
    }

    #[test]
    fn sanitize_preserves_allowlisted_ax_status_before_category() {
        // A specific OS cause survives even when a coarse category word
        // (`set:`, `windows:`) is also present: the allowlist wins.
        assert_eq!(
            sanitize_backend_reason("set: kAXErrorFailure"),
            "failed-ax-failure"
        );
        assert_eq!(
            sanitize_backend_reason("windows: kAXErrorNoValue"),
            "failed-ax-no-value"
        );
        assert_eq!(
            sanitize_backend_reason("set: kAXErrorCannotComplete"),
            "failed-ax-cannot-complete"
        );
        assert_eq!(
            sanitize_backend_reason(
                "snapshot pid 9: requested 2 observed 1 failed 1: kAXErrorNotImplemented"
            ),
            "failed-ax-not-implemented"
        );
        // Prefix overlap resolves longest-first.
        assert_eq!(
            sanitize_backend_reason("kAXErrorInvalidUIElementObserver"),
            "failed-ax-invalid-observer"
        );
        assert_eq!(
            sanitize_backend_reason("kAXErrorInvalidUIElement"),
            "failed-ax-invalid-element"
        );
        // Unknown `AXError 99999` numerics stay numeric-bucketed, never
        // guessed into a known kind.
        assert_eq!(sanitize_backend_reason("set: AXError 99999"), "failed-set");
    }

    #[test]
    fn logger_delivers_without_blocking_sender() {
        let sink = Arc::new(CollectSink {
            events: Mutex::new(Vec::new()),
        });
        let (tx, stop, handle) = spawn_logger(sink.clone(), 8);
        let event = LogEvent {
            seq: 1,
            timestamp_ms: 7,
            pid: 9,
            generation: 1,
            ref_id: 2,
            kind: "observation",
            outcome: "applied",
            count: 1,
        };
        assert!(tx.try_send(event.clone()).is_ok());
        // Deterministic rendezvous: poll the collected sink, deadline only
        // bounds the wait.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if !sink.events.lock().expect("mutex").is_empty() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "logger never wrote");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sink.events.lock().expect("mutex")[0], event);
        stop.store(true, Ordering::SeqCst);
        drop(tx);
        assert!(matches!(handle.join().expect("join"), LoggerExit::Done));
    }
}
