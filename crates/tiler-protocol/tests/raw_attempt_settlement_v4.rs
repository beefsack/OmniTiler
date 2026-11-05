//! Ordered internal execution of the immutable `raw-attempt-settlement`
//! scenario (unit 6 sub-increment 7f-2): one fresh engine, genuine
//! handshake/settings/report decoders, the internal v4 admission boundary
//! through a test-support bridge, genuine canonical encoders, and every
//! original jq assertion evaluated literally in step order. Public ingress,
//! fixtures, and the manifest are untouched.

use std::io::Write as _;
use std::process::{Command, Stdio};

use tiler_core::world_engine::WorldEngine;
use tiler_protocol::connection_protocol::{
    decode_handshake_request, decode_settings, encode_handshake_reply, encode_settings_reply,
};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_report_v4, encode_reply_v4,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

/// Evaluate one literal fixture jq assertion against `envelope`, requiring a
/// strict JSON `true` result (not merely a successful exit with output).
fn assert_jq(step: &str, expression: &str, envelope: &serde_json::Value) {
    let input = serde_json::to_string(envelope).expect("envelope serializes");
    let mut child = Command::new("jq")
        .arg("-e")
        .arg(expression)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{step}: jq unavailable: {error}"));
    child
        .stdin
        .as_mut()
        .expect("jq stdin")
        .write_all(input.as_bytes())
        .expect("jq stdin writes");
    let output = child.wait_with_output().expect("jq completes");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.trim() == "true",
        "{step}: assertion failed: {expression}\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn raw_attempt_settlement_runs_ordered_core_scenario() {
    let text =
        std::fs::read_to_string(fixture_root().join("scenarios/raw-attempt-settlement.json"))
            .expect("scenario reads");
    let scenario: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
    assert_eq!(
        scenario.get("format").and_then(serde_json::Value::as_str),
        Some("world-wire-v4-ordered-scenario"),
        "scenario format"
    );
    assert_eq!(
        scenario.get("id").and_then(serde_json::Value::as_str),
        Some("raw-attempt-settlement"),
        "scenario id"
    );
    assert_eq!(
        scenario.get("driver").and_then(serde_json::Value::as_str),
        Some("core"),
        "scenario driver"
    );
    let initial = scenario.get("initial").expect("scenario initial");
    assert_eq!(
        initial
            .get("freshEngine")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "fresh engine"
    );
    let generations = initial
        .get("generationSequence")
        .and_then(serde_json::Value::as_array)
        .expect("generation sequence");
    assert_eq!(generations.len(), 2, "generation sequence length");
    let seed = generations[0].as_str().expect("initial generation seed");
    assert_eq!(seed, "generation-a", "initial generation seed");
    // The second seed is reserved for lifetime/reset scenarios and is not
    // exercised here; it must not be consumed or faked.
    assert_eq!(
        generations[1].as_str(),
        Some("generation-b"),
        "reserved generation seed"
    );
    assert_eq!(
        initial.get("adapter").and_then(serde_json::Value::as_str),
        Some("adapter-a"),
        "initial adapter"
    );
    assert_eq!(
        initial.get("native"),
        Some(&serde_json::json!({})),
        "empty native initial state"
    );

    let mut engine = WorldEngine::new(seed).expect("fresh engine seeds");
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    assert_eq!(steps.len(), 12, "ordered step count");
    let mut reports = 0;
    let mut assertions = 0;
    for step in steps {
        let id = step
            .get("id")
            .and_then(serde_json::Value::as_str)
            .expect("step id");
        let input = step.get("input").expect("step input");
        assert_eq!(
            input.get("kind").and_then(serde_json::Value::as_str),
            Some("wire"),
            "{id} input kind"
        );
        assert_eq!(
            input.get("to").and_then(serde_json::Value::as_str),
            Some("core"),
            "{id} input target"
        );
        let message = input.get("message").expect("step message");
        let wire = serde_json::to_string(message).expect("message serializes");
        let output_message: serde_json::Value = match message
            .get("type")
            .and_then(serde_json::Value::as_str)
        {
            Some("handshake") => {
                let request = decode_handshake_request(&wire).expect("handshake decodes");
                assert_eq!(request.adapter, "adapter-a", "{id} handshake adapter");
                let reply = engine.handshake(&request);
                let text = encode_handshake_reply(&reply).expect("handshake reply encodes");
                serde_json::from_str(&text).expect("handshake reply parses")
            }
            Some("settings") => {
                let request = decode_settings(&wire).expect("settings decodes");
                assert_eq!(request.adapter, "adapter-a", "{id} settings adapter");
                assert_eq!(
                    request.generation, "generation-a",
                    "{id} settings generation"
                );
                let reply = engine.handle_settings(&request).expect("settings admitted");
                let text = encode_settings_reply(&reply).expect("settings reply encodes");
                serde_json::from_str(&text).expect("settings reply parses")
            }
            None => {
                reports += 1;
                let decoded = decode_report_v4(&wire).expect("report decodes");
                // Precondition checks: the bridge intentionally performs no
                // envelope admission, so the runner pins the envelope
                // binding before calling it.
                assert_eq!(
                    decoded.report.adapter_incarnation, "adapter-a",
                    "{id} report adapter"
                );
                assert_eq!(
                    decoded.report.core_generation, "generation-a",
                    "{id} report generation"
                );
                match engine.handle_report_v4_for_test(&decoded.report) {
                    Ok(reply) => {
                        let wire = ReplyWireOptionals {
                            suspended_output_present: reply.presentation.suspended_output.is_some(),
                        };
                        let text = encode_reply_v4(&DecodedReplyV4 { reply, wire })
                            .expect("reply encodes");
                        serde_json::from_str(&text).expect("reply parses")
                    }
                    Err((kind, message)) => panic!("{id} refused: {kind}/{message}"),
                }
            }
            Some(other) => panic!("{id} unexpected message type {other}"),
        };
        let expect = step.get("expect").expect("step expect");
        assert_eq!(
            expect.get("status").and_then(serde_json::Value::as_str),
            Some("settled"),
            "{id} expect status"
        );
        let step_assertions = expect
            .get("assertions")
            .and_then(serde_json::Value::as_array)
            .expect("step assertions");
        // Core-only output carries no host actions; the settings step pins
        // that shape with its own assertion.
        let envelope = serde_json::json!({
            "output": {
                "message": output_message,
                "actions": [],
            }
        });
        for assertion in step_assertions {
            let expression = assertion.as_str().expect("assertion string");
            assert_jq(id, expression, &envelope);
            assertions += 1;
        }
    }
    assert_eq!(reports, 10, "report step count");
    assert_eq!(assertions, 14, "evaluated assertion count");
}

/// The test bridge refuses an unbound engine, which remains usable for
/// the initial handshake afterwards.
#[test]
fn test_bridge_refuses_unbound_engine() {
    let text =
        std::fs::read_to_string(fixture_root().join("scenarios/raw-attempt-settlement.json"))
            .expect("scenario reads");
    let scenario: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    let message_of = |wanted: &str| {
        steps
            .iter()
            .find(|step| step.get("id").and_then(serde_json::Value::as_str) == Some(wanted))
            .unwrap_or_else(|| panic!("{wanted} step present"))
            .get("input")
            .expect("step input")
            .get("message")
            .expect("step message")
            .clone()
    };
    let report = decode_report_v4(
        &serde_json::to_string(&message_of("observe")).expect("message serializes"),
    )
    .expect("report decodes")
    .report;
    let mut engine = WorldEngine::new("generation-a").expect("fresh engine seeds");
    assert!(
        engine.handle_report_v4_for_test(&report).is_err(),
        "unbound engine refuses"
    );
    let handshake = message_of("handshake");
    let request =
        decode_handshake_request(&serde_json::to_string(&handshake).expect("message serializes"))
            .expect("handshake decodes");
    let reply = engine.handshake(&request);
    assert_eq!(reply.generation.as_deref(), Some("generation-a"));
    assert!(!reply.ready);
}
