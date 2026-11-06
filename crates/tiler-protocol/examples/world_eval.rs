//! Minimal offline test seam: line-delimited v4 world evaluation.
//!
//! Reads one JSON envelope per stdin line, evaluates each against a single
//! retained [`WorldEngine`], and writes one JSON reply per stdout line
//! (empty lines skipped). The envelope mirrors the production D-Bus split:
//! `{"method":"DiscoverWorld","body":"<connection json>"}` carries a v4
//! connection message (`handshake` or `settings`, dispatched by its `type`)
//! routed authentically through [`route_connection_request`], and
//! `{"method":"DescribeWorld","body":"<report json>"}` runs the retained
//! engine transactionally through the genuine v4 report path
//! (`decode_request` + `WorldEngine::handle_report` + `encode_reply`).
//! Only the handshake resets the lifetime (the core handshake mints the
//! next generation from the deterministic evaluator seed); settings never
//! reset. D-Bus, activation, and live host state are stubbed. Test-only
//! seam; not part of the shipped service.

use std::io::{BufRead, Write};

use tiler_protocol::connection_protocol::route_connection_request;
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, WorldEngine, WorldEngineConfig, WorldOutcome,
    decode_request, encode_error, encode_reply,
};

fn generation_seed() -> String {
    "world-eval-0".to_owned()
}

fn ensure_engine(engine: &mut Option<WorldEngine>, config: &WorldEngineConfig, seed: &str) {
    if engine.is_none() {
        *engine = Some(WorldEngine::new_with_config(seed, *config).expect("evaluator seed parses"));
    }
}

fn serving_generation(engine: &Option<WorldEngine>, seed: &str) -> String {
    engine
        .as_ref()
        .map(|live| live.core_generation().to_owned())
        .unwrap_or_else(|| seed.to_owned())
}

fn submit_report(engine: &mut WorldEngine, body: &str, generation: &str) -> String {
    let report = match decode_request(body) {
        Ok(report) => report,
        Err(error) => return encode_error(error.kind, error.message, generation),
    };
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply)
        | WorldOutcome::Duplicate(reply)
        | WorldOutcome::CommandConsumed(reply) => {
            let suspended_output_present = reply.presentation.suspended_output.is_some();
            match encode_reply(&DecodedReplyV4 {
                wire: ReplyWireOptionals {
                    suspended_output_present,
                },
                reply,
            }) {
                Ok(bytes) => bytes,
                Err(error) => encode_error(error.kind, error.message, generation),
            }
        }
        WorldOutcome::Rejected { kind, message } => encode_error(kind, message, generation),
    }
}

fn main() {
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
    let seed = generation_seed();
    let mut engine: Option<WorldEngine> = None;
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
        ensure_engine(&mut engine, &config, &seed);
        let generation = serving_generation(&engine, &seed);
        let reply = (|| {
            let envelope: serde_json::Value = serde_json::from_str(&line).ok()?;
            let method = envelope.get("method")?.as_str()?;
            let body = envelope.get("body")?.as_str()?;
            if method == "DiscoverWorld" {
                let live = engine.as_mut()?;
                match route_connection_request(live, body) {
                    Ok(bytes) => Some(bytes),
                    Err(error) => Some(encode_error(error.0, error.0, live.core_generation())),
                }
            } else if method == "DescribeWorld" {
                let live = engine.as_mut()?;
                let generation = live.core_generation().to_owned();
                Some(submit_report(live, body, &generation))
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
