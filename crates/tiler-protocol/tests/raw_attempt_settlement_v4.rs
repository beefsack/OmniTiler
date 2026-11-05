//! Ordered internal execution of the immutable `raw-attempt-settlement` and
//! `window-state` scenarios (unit 6 sub-increments 7f-2 and 7f-3a): one fresh
//! engine per connection lifetime, genuine handshake/settings/report decoders,
//! the internal v4 admission boundary through a test-support bridge, genuine
//! canonical encoders, full engine drop/recreation on disconnect, explicit
//! OPEN-gate retention, and every original jq assertion evaluated literally in
//! step order against `{output,history}` envelopes. Public ingress, fixtures,
//! and the manifest are untouched.

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
    if !(output.status.success() && stdout.trim() == "true") {
        // Capped failure context: the failing step output only, never the
        // accumulated history.
        let context =
            serde_json::to_string(envelope.get("output").unwrap_or(&serde_json::Value::Null))
                .expect("output serializes");
        let mut capped: String = context.chars().take(8000).collect();
        if capped.len() < context.len() {
            capped.push_str("...[truncated]");
        }
        panic!(
            "{step}: assertion failed: {expression}\nstdout: {stdout}\nstderr: {stderr}\noutput: {capped}"
        );
    }
}

struct OrderedRun {
    steps: usize,
    wires: usize,
    handshakes: usize,
    settings: usize,
    reports: usize,
    events: usize,
    assertions: usize,
    pending: usize,
    pending_topics: Vec<String>,
    generations: Vec<String>,
    generations_consumed: usize,
}

/// Execute one immutable ordered scenario end to end: every wire step runs
/// the genuine decoder/engine/encoder path, every `disconnect` drops the
/// full engine (the next handshake consumes the next generation seed),
/// OPEN/TBD gates are recorded without an invented outcome, and every
/// settled assertion is evaluated literally against `{output,history}`.
/// Core outputs expose the decoded message, its exact bytes and empty
/// actions. Unsupported input kinds, event kinds or expect statuses fail
/// instead of being skipped.
fn run_ordered_scenario(scenario_id: &str) -> OrderedRun {
    let text =
        std::fs::read_to_string(fixture_root().join(format!("scenarios/{scenario_id}.json")))
            .expect("scenario reads");
    let scenario: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
    assert_eq!(
        scenario.get("format").and_then(serde_json::Value::as_str),
        Some("world-wire-v4-ordered-scenario"),
        "scenario format"
    );
    assert_eq!(
        scenario.get("id").and_then(serde_json::Value::as_str),
        Some(scenario_id),
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
    let generations: Vec<String> = initial
        .get("generationSequence")
        .and_then(serde_json::Value::as_array)
        .expect("generation sequence")
        .iter()
        .map(|seed| seed.as_str().expect("generation seed string").to_owned())
        .collect();
    assert!(!generations.is_empty(), "generation sequence");
    let adapter = initial
        .get("adapter")
        .and_then(serde_json::Value::as_str)
        .expect("initial adapter")
        .to_owned();
    assert_eq!(adapter, "adapter-a", "initial adapter");
    assert_eq!(
        initial.get("native"),
        Some(&serde_json::json!({})),
        "empty native initial state"
    );

    let mut engine = Some(WorldEngine::new(&generations[0]).expect("fresh engine seeds"));
    let mut gen_idx = 0usize;
    let mut history = serde_json::Map::new();
    let mut run = OrderedRun {
        steps: 0,
        wires: 0,
        handshakes: 0,
        settings: 0,
        reports: 0,
        events: 0,
        assertions: 0,
        pending: 0,
        pending_topics: Vec::new(),
        generations: generations.clone(),
        generations_consumed: 1,
    };
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    for step in steps {
        let sid = step
            .get("id")
            .and_then(serde_json::Value::as_str)
            .expect("step id");
        let input = step.get("input").expect("step input");
        let expect = step.get("expect").expect("step expect");
        let status = expect
            .get("status")
            .and_then(serde_json::Value::as_str)
            .expect("step status");
        let assertions: Vec<&str> = expect
            .get("assertions")
            .and_then(serde_json::Value::as_array)
            .expect("step assertions")
            .iter()
            .map(|item| item.as_str().expect("assertion string"))
            .collect();
        let kind = input
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .expect("input kind");
        assert!(!history.contains_key(sid), "{sid} duplicate step id");
        let output = match kind {
            "wire" => {
                run.wires += 1;
                assert_eq!(
                    input.get("to").and_then(serde_json::Value::as_str),
                    Some("core"),
                    "{sid} input target"
                );
                let message = input.get("message").expect("step message");
                let wire = serde_json::to_string(message).expect("message serializes");
                let (message_value, bytes) = match message
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                {
                    Some("handshake") => {
                        let request = decode_handshake_request(&wire).expect("handshake decodes");
                        assert_eq!(request.adapter, adapter, "{sid} handshake adapter");
                        if engine.is_none() {
                            gen_idx += 1;
                            assert!(
                                gen_idx < generations.len(),
                                "{sid} no reserved generation for reconnect"
                            );
                            engine = Some(
                                WorldEngine::new(&generations[gen_idx])
                                    .expect("reconnect engine seeds"),
                            );
                            run.generations_consumed += 1;
                        }
                        let live = engine.as_mut().expect("{sid} no live connection");
                        let reply = live.handshake(&request);
                        assert_eq!(
                            reply.generation.as_deref(),
                            Some(generations[gen_idx].as_str()),
                            "{sid} handshake generation binding"
                        );
                        run.handshakes += 1;
                        let text = encode_handshake_reply(&reply).unwrap_or_else(|error| {
                            panic!("{sid} handshake reply encodes: {error:?}")
                        });
                        let value: serde_json::Value =
                            serde_json::from_str(&text).expect("handshake reply parses");
                        (value, text)
                    }
                    Some("settings") => {
                        let request = decode_settings(&wire).expect("settings decodes");
                        assert_eq!(request.adapter, adapter, "{sid} settings adapter");
                        assert_eq!(
                            request.generation, generations[gen_idx],
                            "{sid} settings generation"
                        );
                        let live = engine.as_mut().expect("{sid} no live connection");
                        let reply = live.handle_settings(&request).expect("settings admitted");
                        run.settings += 1;
                        let text = encode_settings_reply(&reply).unwrap_or_else(|error| {
                            panic!("{sid} settings reply encodes: {error:?}")
                        });
                        let value: serde_json::Value =
                            serde_json::from_str(&text).expect("settings reply parses");
                        (value, text)
                    }
                    None => {
                        let decoded = decode_report_v4(&wire).expect("report decodes");
                        // Precondition checks: the bridge intentionally performs no
                        // envelope admission, so the runner pins the envelope
                        // binding before calling it.
                        assert_eq!(
                            decoded.report.adapter_incarnation, adapter,
                            "{sid} report adapter"
                        );
                        assert_eq!(
                            decoded.report.core_generation, generations[gen_idx],
                            "{sid} report generation"
                        );
                        let live = engine.as_mut().expect("{sid} no live connection");
                        match live.handle_report_v4_for_test(&decoded.report) {
                            Ok(reply) => {
                                run.reports += 1;
                                let wire_flags = ReplyWireOptionals {
                                    suspended_output_present: reply
                                        .presentation
                                        .suspended_output
                                        .is_some(),
                                };
                                let text = encode_reply_v4(&DecodedReplyV4 {
                                    reply,
                                    wire: wire_flags,
                                })
                                .unwrap_or_else(|error| panic!("{sid} reply encodes: {error:?}"));
                                let value: serde_json::Value =
                                    serde_json::from_str(&text).expect("reply parses");
                                (value, text)
                            }
                            Err((kind, message)) => {
                                panic!("{sid} refused: {kind}/{message}")
                            }
                        }
                    }
                    Some(other) => panic!("{sid} unexpected message type {other}"),
                };
                serde_json::json!({
                    "message": message_value,
                    "bytes": bytes,
                    "actions": [],
                })
            }
            "fixture-event" => {
                run.events += 1;
                let to = input
                    .get("to")
                    .and_then(serde_json::Value::as_str)
                    .expect("event target");
                let event = input.get("event").expect("step event");
                let event_kind = event
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .expect("event kind");
                match (to, event_kind) {
                    ("connection", "disconnect") => {
                        // Full engine drop: destroys connection correlations,
                        // core lifetime state and model grants without replay.
                        engine = None;
                        serde_json::json!({"actions": []})
                    }
                    ("platform-evidence", "evidence-gate") => {
                        // Explicit pending gate: recorded, never executed.
                        // A settled evidence gate is rejected below; TBD
                        // records TBD only when the fixture says so.
                        assert!(
                            status == "OPEN" || status == "TBD",
                            "{sid} evidence gate must stay pending"
                        );
                        let topic = event
                            .get("topic")
                            .and_then(serde_json::Value::as_str)
                            .expect("evidence gate topic");
                        assert_eq!(
                            topic, "windows-app-owned-fullscreen-exit",
                            "{sid} unsupported evidence topic"
                        );
                        serde_json::json!({
                            "actions": [],
                            "pending": topic,
                            "status": status,
                        })
                    }
                    (other_to, other_kind) => {
                        panic!("{sid} unsupported fixture event {other_to}/{other_kind}")
                    }
                }
            }
            other => panic!("{sid} unsupported input kind {other}"),
        };
        let envelope = serde_json::json!({"output": output, "history": history.clone()});
        match status {
            "settled" => {
                assert!(
                    !assertions.is_empty(),
                    "{sid} settled step must carry assertions"
                );
                for assertion in &assertions {
                    assert_jq(sid, assertion, &envelope);
                    run.assertions += 1;
                }
            }
            "OPEN" | "TBD" => {
                assert!(
                    assertions.is_empty(),
                    "{sid} pending gate must not carry assertions"
                );
                run.pending += 1;
                run.pending_topics.push(
                    output
                        .get("pending")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(sid)
                        .to_owned(),
                );
            }
            other => panic!("{sid} unsupported expect status {other}"),
        }
        history.insert(sid.to_owned(), output);
        run.steps += 1;
    }
    // Reserved seeds validate but are never consumed or faked.
    for seed in generations.iter().skip(run.generations_consumed) {
        assert!(
            WorldEngine::new(seed).is_some(),
            "reserved generation seed validates"
        );
    }
    run
}

#[test]
fn raw_attempt_settlement_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("raw-attempt-settlement");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned()],
        "generation sequence"
    );
    // The second seed is reserved for lifetime/reset scenarios and is not
    // exercised here; it must validate without being consumed or faked.
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 12, "ordered step count");
    assert_eq!(run.wires, 12, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 10, "report step count");
    assert_eq!(run.events, 0, "event step count");
    assert_eq!(run.assertions, 14, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn window_state_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("window-state");
    assert_eq!(
        run.generations,
        vec![
            "generation-a".to_owned(),
            "generation-b".to_owned(),
            "generation-c".to_owned(),
        ],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 3,
        "every generation seed consumed across reconnects"
    );
    assert_eq!(run.steps, 28, "ordered step count");
    assert_eq!(run.wires, 25, "wire step count");
    assert_eq!(run.handshakes, 3, "handshake step count");
    assert_eq!(run.settings, 3, "settings step count");
    assert_eq!(run.reports, 19, "report step count");
    assert_eq!(run.events, 3, "event step count");
    assert_eq!(run.assertions, 56, "evaluated assertion count");
    // The native app-owned fullscreen baseline stays an explicit pending
    // gate; this scenario is not full acceptance.
    assert_eq!(run.pending, 1, "pending gate count");
    assert_eq!(
        run.pending_topics,
        vec!["windows-app-owned-fullscreen-exit".to_owned()],
        "pending gate topic"
    );
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
