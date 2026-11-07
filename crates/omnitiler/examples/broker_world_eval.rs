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
//! caching the engine-only seam lacks), and
//! `{"method":"ReadWorldPresentation","body":"<read request json>"}` reads
//! the cached accepted presentation via
//! [`WorldService::read_presentation`]. Same process preserves service state
//! across lines: one [`WorldService`] serves the whole session, so a
//! handshake binds the producer lifetime and later captures apply under it.
//!
//! Exactly three methods exist. Any other `method` refuses in-band with
//! `unknown-value` without touching service state (no mutation, no reset).
//! The queue/take/wait routes are deliberately absent: this seam records
//! stage-5 producer traffic (discovery, capture, presentation reads), never
//! external commands. There is no `_legacy` path, no v3 conversion, and no
//! classification copy.
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
//! is exercised or claimed. Not part of the shipped service. Run with:
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
              \"capabilities\":{{\"hostOwned\":false,\"equalCount\":false,\"birthClear\":false,\
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
        let unknown: serde_json::Value =
            serde_json::from_str(&evaluate(&service, &envelope("QueueWorldCommand", "{}")))
                .expect("valid JSON");
        assert_eq!(unknown["outcome"], "rejected");
        assert_eq!(unknown["kind"], "unknown-value");
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
}
