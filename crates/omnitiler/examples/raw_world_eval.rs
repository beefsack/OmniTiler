//! Test-only raw-capture eval seam: line-delimited world evaluation with
//! real-Rust KDE raw normalization (NOT a TS mirror).
//!
//! Reads one JSON envelope per stdin line, evaluates each against a single
//! retained [`WorldEngine`], and writes one JSON reply per stdout line
//! (empty lines skipped). The envelope mirrors the production D-Bus split:
//! `{"method":"DiscoverWorld","body":"<discovery json>"}` binds a fresh
//! generation and `{"method":"DescribeWorld","body":"<report json>"}` runs
//! the retained engine transactionally through the exact
//! decode/evaluate/encode path the shipped world service uses. D-Bus,
//! activation, and live host state are stubbed. Test-only seam; not part of
//! the shipped service.
//!
//! DescribeWorld bodies carry the KWin-shaped RAW `windows` lane (plus the
//! tolerant `floatIntent`/`tileIntent` lanes) with the broker-owned
//! sequencing namespace already stamped by the JS mini-broker. The raw lane
//! is normalized here by the production [`kde_raw::normalize_raw_windows`]
//! (direct source include, no public API widened for tests) before the
//! strict core decode. Raw refusals surface as the broker-shaped
//! in-band error (`kind`/`message`) without touching the engine.

use std::io::{BufRead, Write};

use tiler_protocol::world_protocol::{
    WorldEngine, WorldEngineConfig, WorldOutcome, decode_request, encode_error, encode_reply,
};

// Direct production-source includes: no public API widened solely for tests.
#[path = "../src/kde_gesture.rs"]
mod kde_gesture;
#[path = "../src/kde_raw.rs"]
mod kde_raw;

fn generation_for(counter: u64) -> String {
    format!("world-eval-{counter}")
}

fn discovery_reply(engine: &WorldEngine, body: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return encode_error("malformed", "request is malformed", "world-eval-0"),
    };
    let map = match parsed.as_object() {
        Some(map) => map,
        None => return encode_error("malformed", "request is malformed", "world-eval-0"),
    };
    let adapter = map
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let req = map
        .get("req")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if adapter.is_empty() || req.is_empty() {
        return encode_error("malformed", "request is malformed", "world-eval-0");
    }
    encode_reply(&engine.bootstrap_reply(adapter, req))
}

fn main() {
    let mut engine: Option<WorldEngine> = None;
    // Retained broker-owned gesture map: the same per-generation identity
    // namespace the shipped world service folds before stamping. Reset on
    // every discovery like the broker rotation resets with the engine.
    let mut gesture_map = kde_gesture::KdeGestureMap::new();
    let mut generation = generation_for(0);
    let mut discoveries: u64 = 0;
    // Test-only platform selection (never a user setting): carriers may
    // enable the fixed-send admission at construction; unset keeps the
    // default disjoint model every existing fixture assumes.
    let fixed_send = std::env::var_os("WORLD_EVAL_FIXED_SEND").is_some_and(|v| v == "1");
    let config = if fixed_send {
        WorldEngineConfig {
            fixed_send_admission: true,
            ..WorldEngineConfig::default()
        }
    } else {
        WorldEngineConfig::default()
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let reply = (|| {
            let envelope: serde_json::Value = serde_json::from_str(&line).ok()?;
            let method = envelope.get("method")?.as_str()?;
            let body = envelope.get("body")?.as_str()?;
            if method == "DiscoverWorld" {
                discoveries += 1;
                generation = generation_for(discoveries);
                let fresh = WorldEngine::new_with_config(&generation, config)?;
                engine = Some(fresh);
                gesture_map = kde_gesture::KdeGestureMap::new();
                let active = engine.as_ref()?;
                Some(discovery_reply(active, body))
            } else if method == "DescribeWorld" {
                if engine.is_none() {
                    return Some(encode_error("generation-mismatch", "stale", &generation));
                }
                // Real-Rust raw normalization on the stamped capture before
                // the strict core decode (mirrors the production broker
                // lane: normalize windows, fold the gesture lane, then
                // decode/evaluate). A raw refusal rejects in-band without
                // touching the engine or the queue.
                let normalized_body: String = match serde_json::from_str::<serde_json::Value>(body)
                {
                    Ok(serde_json::Value::Object(mut map)) => {
                        if let Err((kind, message)) = kde_raw::normalize_raw_windows(&mut map) {
                            return Some(encode_error(kind, message, &generation));
                        }
                        if let Err(err) = gesture_map.normalize_gesture_lane(&mut map) {
                            let (kind, message) = match err {
                                kde_gesture::GestureLaneError::Reject(k, m) => (k, m),
                                kde_gesture::GestureLaneError::Rotate => {
                                    return Some(encode_error(
                                        "sequence-exhausted",
                                        "sequencing exhausted",
                                        &generation,
                                    ));
                                }
                            };
                            return Some(encode_error(kind, message, &generation));
                        }
                        serde_json::Value::Object(map).to_string()
                    }
                    // Non-object bodies fall through to the strict decode,
                    // which refuses them as malformed like the core would.
                    _ => body.to_owned(),
                };
                let report = match decode_request(&normalized_body) {
                    Ok(report) => report,
                    Err(error) => {
                        return Some(encode_error(error.kind, error.message, &generation));
                    }
                };
                let engine_ref = engine.as_mut()?;
                match engine_ref.handle_report(&report) {
                    WorldOutcome::Accepted(reply)
                    | WorldOutcome::Duplicate(reply)
                    | WorldOutcome::CommandConsumed(reply) => Some(encode_reply(&reply)),
                    WorldOutcome::Rejected { kind, message } => {
                        Some(encode_error(kind, message, &generation))
                    }
                }
            } else {
                Some(encode_error(
                    "unknown-value",
                    "request contains an unknown value",
                    &generation,
                ))
            }
        })()
        .unwrap_or_else(|| encode_error("malformed", "request is malformed", &generation));
        out.write_all(reply.as_bytes()).expect("stdout is writable");
        out.write_all(b"\n").expect("stdout is writable");
    }
    out.flush().ok();
}
