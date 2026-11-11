//! Development-only retained strict-v4 production-broker eval seam: line-delimited world evaluation.
//!
//! Reads one JSON envelope per stdin line, evaluates each against a single
//! retained [`WorldService`], and writes one JSON reply per stdout line
//! (empty lines skipped). The envelope mirrors the raw engine seam:
//! `{"method":"DiscoverWorld","body":"<connection json>"}` carries a v4
//! connection message (`handshake` or `settings`, dispatched by its `type`),
//! `{"method":"DescribeWorld","body":"<capture envelope json>"}` runs one
//! broker capture through the true production route
//! ([`WorldService::evaluate_capture`], with broker/witness/proven-gesture
//! caching the engine-only seam lacks),
//! `{"method":"QueueWorldCommand","body":"<queue request json>"}` files one
//! external command through the true production route
//! ([`WorldService::queue_command`], parking it for the next fresh capture,
//! which drains the whole FIFO batch in order),
//! `{"method":"TakeCommandResult","body":"<take request json>"}` collects one
//! settled terminal result through the true production route
//! ([`WorldService::take_result`], per-command namespaced collection), and
//! `{"method":"ReadWorldPresentation","body":"<read request json>"}` reads
//! the cached accepted presentation via
//! [`WorldService::read_presentation`]. Same process preserves service state
//! across lines: one [`WorldService`] serves the whole session, so a
//! handshake binds the producer lifetime and later captures apply under it.
//!
//! Exactly five methods exist. Any other `method` refuses in-band with
//! `unknown-value` without touching service state (no mutation, no reset).
//! The wait route is deliberately absent: this seam records
//! stage-5 producer traffic (discovery, capture, presentation reads) plus the
//! adapter batch lanes (queue, take), never parked waiters. There is no
//! `_legacy` path, no v3 conversion, and no classification copy.
//!
//! Bounds: every stdin line (envelope plus overhead) is capped at
//! [`WORLD_SERVICE_MAX_REQUEST`] bytes (the exact 4 MiB service source
//! constant); overlong lines refuse in-band as `oversized` and the
//! remainder of the line is discarded. Every `body` is capped again by the
//! service itself at the same bound. Transport-level service failures
//! (`Oversized`, `Unavailable`), which production surfaces as D-Bus errors,
//! map to bounded in-band bodies here so stdout stays one JSON string per
//! input.
//!
//! In-process test seam only: no D-Bus, so no UID/current-owner/replay auth
//! is exercised or claimed. The queue/take lanes bind a constant test sender
//! (a single adapter namespace, never a D-Bus unique name): per-command
//! parking, batch drain order, duplicate suppression, and take collection
//! run the true production code under that fixed namespace. Not part of the
//! shipped service. Run with:
//!
//! ```sh
//! cargo run -p omnitiler --example broker_world_eval
//! ```

use std::io::{BufRead, Write};

use omnitiler::world_service::{WORLD_SERVICE_MAX_REQUEST, WorldService, WorldServiceError};

/// Stdin envelope cap: the exact service request source constant. Envelope
/// overhead counts toward the gate, so a body at the bound refuses here
/// fail-closed before the service sees it.
const LINE_CAP: usize = WORLD_SERVICE_MAX_REQUEST;

/// Constant test sender binding the queue/take namespace. Production binds
/// the authenticated D-Bus unique name per call; this seam has no D-Bus, so
/// both lanes share one fixed sender and per-command (client, clientCmd)
/// semantics run unmodified under it.
const TEST_SENDER: &str = "test-sender";

/// Map a transport-level service failure onto a bounded in-band body.
/// Production surfaces these as D-Bus errors; the seam must still emit one
/// JSON string per input. Details never echo (fixed sentences only).
fn transport_error(service: &WorldService, error: WorldServiceError) -> String {
    match error {
        WorldServiceError::Oversized => {
            service.error_body("oversized", "request exceeds size bound")
        }
        WorldServiceError::Unavailable(_) => {
            service.error_body("unavailable", "service unavailable")
        }
    }
}

fn evaluate(service: &WorldService, line: &str) -> String {
    let malformed = || service.error_body("malformed", "request is malformed");
    let envelope: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(_) => return malformed(),
    };
    let (method, body) = match (
        envelope.get("method").and_then(|value| value.as_str()),
        envelope.get("body").and_then(|value| value.as_str()),
    ) {
        (Some(method), Some(body)) => (method, body),
        _ => return malformed(),
    };
    let outcome = match method {
        "DiscoverWorld" => service.discover(body),
        "DescribeWorld" => service.evaluate_capture(body),
        "QueueWorldCommand" => service.queue_command(TEST_SENDER, body),
        "TakeCommandResult" => service.take_result(TEST_SENDER, body),
        "ReadWorldPresentation" => service.read_presentation(body),
        _ => {
            return service.error_body("unknown-value", "request contains an unknown value");
        }
    };
    match outcome {
        Ok(reply) => reply,
        Err(error) => transport_error(service, error),
    }
}

fn main() {
    let service = WorldService::new();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let mut oversize = false;
        let mut complete = false;
        let mut clean_eof = false;
        while !complete {
            // Copy out of the `fill_buf` borrow so `consume` can follow;
            // each chunk is buffer-sized, so memory stays bounded.
            enum Step {
                Eof,
                Chunk(Vec<u8>, bool),
            }
            let step = match input.fill_buf() {
                Ok([]) => Step::Eof,
                Ok(chunk) => match chunk.iter().position(|&b| b == b'\n') {
                    Some(pos) => Step::Chunk(chunk[..pos + 1].to_vec(), true),
                    None => Step::Chunk(chunk.to_vec(), false),
                },
                Err(_) => return,
            };
            match step {
                Step::Eof => {
                    if buf.is_empty() && !oversize {
                        clean_eof = true;
                    }
                    complete = true;
                }
                Step::Chunk(bytes, newline) => {
                    input.consume(bytes.len());
                    if oversize {
                        if newline {
                            complete = true;
                        }
                        continue;
                    }
                    if buf.len() + bytes.len() > LINE_CAP + 1 {
                        oversize = true;
                        buf.clear();
                        if newline {
                            complete = true;
                        }
                        continue;
                    }
                    buf.extend_from_slice(&bytes);
                    if newline {
                        complete = true;
                    }
                }
            }
        }
        if clean_eof {
            break;
        }
        if oversize {
            let reply = service.error_body("oversized", "request exceeds size bound");
            out.write_all(reply.as_bytes()).expect("stdout is writable");
            out.write_all(b"\n").expect("stdout is writable");
            out.flush().expect("stdout is writable");
            continue;
        }
        while buf.last().is_some_and(|&b| b == b'\n' || b == b'\r') {
            buf.pop();
        }
        if buf.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let reply = match std::str::from_utf8(&buf) {
            Ok(line) => evaluate(&service, line),
            Err(_) => service.error_body("malformed", "request is malformed"),
        };
        out.write_all(reply.as_bytes()).expect("stdout is writable");
        out.write_all(b"\n").expect("stdout is writable");
        out.flush().expect("stdout is writable");
    }
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(method: &str, body: &str) -> String {
        serde_json::json!({"method": method, "body": body}).to_string()
    }

    // Exact handshake shape from the service unit tests
    // (`src/world_service.rs` handshake_request).
    fn handshake_body(adapter: &str, req: &str) -> String {
        format!(
            "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
              \"applicationIdentity\":{{\"appId\":null,\"executable\":\"/usr/bin/kwin\",\
              \"processId\":\"pid-own-1\"}},\
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"sharedNativeRoster\":false,\"birthClear\":false,\
              \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
              \"maximize\":false,\"fullscreen\":false,\"nativeSticky\":false,\"above\":false,\
              \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
        )
    }

    // Exact settings shape from the service unit tests
    // (`src/world_service.rs` suspension_settings).
    fn settings_body(generation: &str) -> String {
        format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
              \"req\":\"settings-1\",\"rev\":\"1\",\"values\":{{}}}}"
        )
    }

    #[test]
    fn malformed_and_unknown_preserve_generation() {
        let service = WorldService::new();
        let before = service.generation();
        let malformed: serde_json::Value =
            serde_json::from_str(&evaluate(&service, "not json")).expect("valid JSON");
        assert_eq!(malformed["outcome"], "rejected");
        assert_eq!(malformed["kind"], "malformed");
        // The wait lane stays absent: only discovery, capture, queue, take,
        // and presentation reads exist on this seam.
        let unknown: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &envelope("WaitWorldWake", "{}")))
                .expect("valid JSON");
        assert_eq!(unknown["outcome"], "rejected");
        assert_eq!(unknown["kind"], "unknown-value");
        assert_eq!(service.generation(), before);
    }

    #[test]
    fn queue_lane_routes_to_production_broker() {
        let service = WorldService::new();
        let before = service.generation();
        // Malformed queue body refuses in-band through the broker lane (not
        // unknown-value): the seam routes QueueWorldCommand to
        // `queue_command` under the constant test sender.
        let refused: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &envelope("QueueWorldCommand", "{}")))
                .expect("valid JSON");
        assert_eq!(refused["outcome"], "rejected");
        assert_eq!(refused["kind"], "malformed");
        assert_eq!(service.generation(), before);
        // Same for the take lane.
        let take_refused: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &envelope("TakeCommandResult", "{}")))
                .expect("valid JSON");
        assert_eq!(take_refused["outcome"], "rejected");
        assert_eq!(take_refused["kind"], "malformed");
        assert_eq!(service.generation(), before);
    }

    #[test]
    fn handshake_settings_then_read_not_ready() {
        let service = WorldService::new();
        let discovery: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("DiscoverWorld", &handshake_body("adapter-1", "req-0")),
        ))
        .expect("valid JSON");
        assert_eq!(discovery["ready"], false);
        let generation = discovery["gen"].as_str().expect("gen").to_owned();
        assert!(!generation.is_empty());
        let settings: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("DiscoverWorld", &settings_body(&generation)),
        ))
        .expect("valid JSON");
        assert_eq!(settings["status"], "accepted");
        // Exact read shape (`src/world_service.rs` suspension_read); no
        // accepted capture yet, so the presentation route stays not-ready.
        let read: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("ReadWorldPresentation", "{\"v\":4,\"req\":\"read-1\"}"),
        ))
        .expect("valid JSON");
        assert_eq!(read["outcome"], "rejected");
        assert_eq!(read["kind"], "not-ready");
        assert_eq!(read["gen"].as_str(), Some(generation.as_str()));
    }

    // Minimal valid v4 inner report: one complete screen, no windows.
    // Same valid shape the service broker tests admit.
    fn empty_inner() -> String {
        "{\"v\":4,\
          \"screens\":[{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}],\
          \"windows\":[],\"closed\":[],\"native\":[],\
          \"focused\":\"\",\"foregroundKnown\":false,\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
          \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
          \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
          \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\
          \"foregroundOut\":null},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null}"
            .to_owned()
    }

    fn capture_body(generation: &str, seq: u64, inner: &str) -> String {
        format!(
            "{{\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"{generation}\",\
              \"source\":\"src-{seq}\",\"sourceSeq\":\"{seq}\",\
              \"nativeInvalid\":\"{seq}\",\"report\":{inner}}}"
        )
    }

    fn queue_body(generation: &str, client_cmd: &str, command: &str) -> String {
        format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
              \"clientCmd\":\"{client_cmd}\",\"command\":{command}}}"
        )
    }

    fn take_body(generation: &str, client_cmd: &str) -> String {
        format!(
            "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
              \"clientCmd\":\"{client_cmd}\"}}"
        )
    }

    #[test]
    fn queued_pair_drains_on_one_capture_with_distinct_takes_and_stable_replay() {
        // Positive batch smoke through the true production broker lanes:
        // two queued commands park (takes pending), ONE capture drains
        // both FIFO with one revision, each take collects ready under its
        // own key, and a byte-identical capture replays without
        // re-execution.
        let service = WorldService::new();
        let discovery: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("DiscoverWorld", &handshake_body("adapter-1", "req-0")),
        ))
        .expect("valid JSON");
        assert_eq!(discovery["ready"], false);
        let generation = discovery["gen"].as_str().expect("gen").to_owned();
        let settings: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("DiscoverWorld", &settings_body(&generation)),
        ))
        .expect("valid JSON");
        assert_eq!(settings["status"], "accepted");
        let inner = empty_inner();
        let first: serde_json::Value = serde_json::from_str(&evaluate(
            &service,
            &envelope("DescribeWorld", &capture_body(&generation, 1, &inner)),
        ))
        .expect("valid JSON");
        assert_eq!(first["ready"], true);
        for id in ["1", "2"] {
            let filed: serde_json::Value = serde_json::from_str(&evaluate(
                &service,
                &envelope(
                    "QueueWorldCommand",
                    &queue_body(
                        &generation,
                        id,
                        "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                    ),
                ),
            ))
            .expect("valid JSON");
            assert_eq!(filed["outcome"], "queued", "command {id} parks");
            let parked: serde_json::Value = serde_json::from_str(&evaluate(
                &service,
                &envelope("TakeCommandResult", &take_body(&generation, id)),
            ))
            .expect("valid JSON");
            assert_eq!(
                parked["outcome"], "pending",
                "command {id} parks until capture"
            );
        }
        let drain_envelope = envelope("DescribeWorld", &capture_body(&generation, 2, &inner));
        let drain: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &drain_envelope)).expect("valid JSON");
        assert_eq!(drain["rev"], "2", "one capture advances one revision");
        for id in ["1", "2"] {
            let settled: serde_json::Value = serde_json::from_str(&evaluate(
                &service,
                &envelope("TakeCommandResult", &take_body(&generation, id)),
            ))
            .expect("valid JSON");
            assert_eq!(
                settled["outcome"], "ready",
                "command {id} settles from the single capture"
            );
        }
        let replay: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &drain_envelope)).expect("valid JSON");
        assert_eq!(
            replay, drain,
            "duplicate capture replays byte-identically (no re-execution)"
        );
        for id in ["1", "2"] {
            let settled: serde_json::Value = serde_json::from_str(&evaluate(
                &service,
                &envelope("TakeCommandResult", &take_body(&generation, id)),
            ))
            .expect("valid JSON");
            assert_eq!(settled["outcome"], "ready", "takes stay settled");
        }
    }
}
