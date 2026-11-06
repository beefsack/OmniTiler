//! v4 line-delimited world evaluation transcript: the durable
//! `tests/data/world_eval_v4.jsonl` envelope transcript (no v3 envelopes)
//! through the same per-line dispatch the `world_eval` example serves on
//! stdin/stdout. This test drives that dispatch in-process over the
//! fixture lines and asserts each leg; it does not spawn the example
//! subprocess. Run the example over the same file directly with
//! `cargo run -q -p tiler-protocol --example world_eval` to observe the
//! true stdin/stdout path.

use tiler_core::connection::ProtocolKind;
use tiler_protocol::connection_protocol::{
    decode_error, decode_handshake_reply, decode_settings_reply, route_connection_request,
};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, WorldEngine, WorldOutcome, decode_reply_v4, decode_request,
    encode_error, encode_reply,
};

fn seed() -> String {
    "world-eval-0".to_owned()
}

fn submit_report(engine: &mut WorldEngine, body: &str) -> String {
    let generation = engine.core_generation().to_owned();
    let report = match decode_request(body) {
        Ok(report) => report,
        Err(error) => return encode_error(error.kind, error.message, &generation),
    };
    match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply)
        | WorldOutcome::Duplicate(reply)
        | WorldOutcome::CommandConsumed(reply) => {
            let suspended_output_present = reply.presentation.suspended_output.is_some();
            encode_reply(&DecodedReplyV4 {
                wire: ReplyWireOptionals {
                    suspended_output_present,
                },
                reply,
            })
            .expect("reply encodes")
        }
        WorldOutcome::Rejected { kind, message } => encode_error(kind, message, &generation),
    }
}

fn submit_line(engine: &mut Option<WorldEngine>, line: &str) -> String {
    if engine.is_none() {
        *engine = Some(WorldEngine::new(&seed()).expect("evaluator seed parses"));
    }
    let envelope: serde_json::Value = serde_json::from_str(line).expect("envelope parses");
    let method = envelope
        .get("method")
        .and_then(serde_json::Value::as_str)
        .expect("method present");
    let body = envelope
        .get("body")
        .and_then(serde_json::Value::as_str)
        .expect("body present");
    if method == "DiscoverWorld" {
        let live = engine.as_mut().expect("live engine");
        match route_connection_request(live, body) {
            Ok(bytes) => bytes,
            Err(error) => encode_error(error.0, error.0, live.core_generation()),
        }
    } else if method == "DescribeWorld" {
        submit_report(engine.as_mut().expect("live engine"), body)
    } else {
        panic!("unexpected method {method}");
    }
}

fn rejected_kind(line: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(line).expect("error parses");
    assert_eq!(
        value.get("outcome").and_then(serde_json::Value::as_str),
        Some("rejected")
    );
    value
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .expect("kind present")
        .to_owned()
}

#[test]
fn world_eval_v4_transcript() {
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/world_eval_v4.jsonl"),
    )
    .expect("transcript reads");
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 12);

    let mut engine: Option<WorldEngine> = None;

    // Handshake binds the deterministic evaluator seed.
    let reply = submit_line(&mut engine, lines[0]);
    let handshake = decode_handshake_reply(&reply).expect("handshake reply decodes");
    assert_eq!(handshake.adapter, "adapter-a");
    assert_eq!(handshake.request, "handshake-a");
    assert_eq!(handshake.generation.as_deref(), Some("world-eval-0"));
    assert!(!handshake.ready);

    // Report before settings latches nothing: accepted bootstrap, not ready.
    let bootstrap = submit_line(&mut engine, lines[1]);
    let decoded = decode_reply_v4(&bootstrap).expect("bootstrap reply decodes");
    assert_eq!(decoded.reply.core_generation, "world-eval-0");
    assert_eq!(decoded.reply.request_id, "request-1");
    assert_eq!(decoded.reply.based_on_observation_seq, 1);
    assert!(!decoded.reply.ready);

    // Settings admit for the bound generation.
    let reply = submit_line(&mut engine, lines[2]);
    let settings = decode_settings_reply(&reply).expect("settings reply decodes");
    assert_eq!(settings.generation, "world-eval-0");
    assert_eq!(settings.request, "settings-1");
    assert!(matches!(
        settings.status,
        tiler_core::connection::SettingsStatus::Accepted
    ));

    // The same report now latches readiness.
    let ready = submit_line(&mut engine, lines[3]);
    let decoded = decode_reply_v4(&ready).expect("ready reply decodes");
    assert!(decoded.reply.ready);
    assert_eq!(decoded.reply.core_generation, "world-eval-0");

    // Identical replay returns the identical encoded reply.
    let replay = submit_line(&mut engine, lines[4]);
    assert_eq!(replay, ready);

    // Changed body under a reused sequence refuses without corruption.
    let refused = submit_line(&mut engine, lines[5]);
    assert_eq!(rejected_kind(&refused), "request-reused");

    // Reconnect handshake mints the next generation from the seed.
    let reply = submit_line(&mut engine, lines[6]);
    let handshake = decode_handshake_reply(&reply).expect("handshake reply decodes");
    assert_eq!(handshake.generation.as_deref(), Some("world-eval-0-1"));
    assert!(!handshake.ready);

    // Prior-generation report refuses as stale.
    let refused = submit_line(&mut engine, lines[7]);
    assert_eq!(rejected_kind(&refused), "generation-mismatch");

    // Prior-generation settings refuse through the connection router.
    let reply = submit_line(&mut engine, lines[8]);
    let error = decode_error(&reply).expect("error wire decodes");
    assert_eq!(error.kind, ProtocolKind::GenerationMismatch);
    assert_eq!(error.generation, "world-eval-0-1");

    // Fresh lifetime bootstraps empty until settings plus a usable report.
    let bootstrap = submit_line(&mut engine, lines[9]);
    let decoded = decode_reply_v4(&bootstrap).expect("bootstrap reply decodes");
    assert_eq!(decoded.reply.core_generation, "world-eval-0-1");
    assert!(!decoded.reply.ready);

    let reply = submit_line(&mut engine, lines[10]);
    let settings = decode_settings_reply(&reply).expect("settings reply decodes");
    assert_eq!(settings.generation, "world-eval-0-1");
    assert!(matches!(
        settings.status,
        tiler_core::connection::SettingsStatus::Accepted
    ));

    let ready = submit_line(&mut engine, lines[11]);
    let decoded = decode_reply_v4(&ready).expect("ready reply decodes");
    assert!(decoded.reply.ready);
    assert_eq!(decoded.reply.core_generation, "world-eval-0-1");
}
