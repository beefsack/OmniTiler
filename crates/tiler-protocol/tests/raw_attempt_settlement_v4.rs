//! Ordered internal execution of the immutable `raw-attempt-settlement` and
//! `window-state` scenarios (unit 6 sub-increments 7f-2 and 7f-3a): one fresh
//! engine per connection lifetime, genuine handshake/settings/report decoders,
//! the internal v4 admission boundary through a test-support bridge, genuine
//! canonical encoders, full engine drop/recreation on disconnect, explicit
//! OPEN-gate retention, and every original jq assertion evaluated literally in
//! step order against `{output,history}` envelopes. Public ingress, fixtures,
//! and the manifest are untouched.
//!
//! The `persistent-above` scenario additionally has a passing first-twenty-step
//! prefix test, an ignored full ordered run (parked blocker, not a pass), and
//! labelled DIAGNOSTIC tests that record executed assertion outcomes without
//! claiming any scenario pass.

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

/// Evaluate one literal fixture jq assertion: `Ok(())` on a strict JSON
/// `true` result, `Err(detail)` on a literal JSON `false`. Anything else
/// (bad exit, syntax or runtime error, non-boolean output) panics: a broken
/// jq evaluation must never be recorded as an expected semantic failure.
fn check_jq(step: &str, expression: &str, envelope: &serde_json::Value) -> Result<(), String> {
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
    if output.status.success() && stdout.trim() == "true" {
        return Ok(());
    }
    // Capped failure context: the failing step output only, never the
    // accumulated history.
    let context = serde_json::to_string(envelope.get("output").unwrap_or(&serde_json::Value::Null))
        .expect("output serializes");
    let mut capped: String = context.chars().take(8000).collect();
    if capped.len() < context.len() {
        capped.push_str("...[truncated]");
    }
    let detail = format!("stdout: {stdout}\nstderr: {stderr}\noutput: {capped}");
    // `jq -e` exits 1 with literal `false` output; that alone is a recorded
    // semantic failure. Every other non-true outcome is a jq malfunction.
    if output.status.code() == Some(1) && stdout.trim() == "false" {
        Err(detail)
    } else {
        panic!("{step}: jq evaluation error: {expression}\n{detail}");
    }
}

/// Evaluate one literal fixture jq assertion against `envelope`, requiring a
/// strict JSON `true` result (not merely a successful exit with output).
fn assert_jq(step: &str, expression: &str, envelope: &serde_json::Value) {
    if let Err(detail) = check_jq(step, expression, envelope) {
        panic!("{step}: assertion failed: {expression}\n{detail}");
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

/// Test-only lifetime-bound coupled-property stacking storage driver.
///
/// Models the adapter's mandatory persist-before-write obligation for the
/// independent native stacking properties (`keepAbove`/`keepBelow` are
/// coupled: the host clears one when the other is set). A
/// `recoverable-state` event persists the observed native preimage under
/// the exact `(win, tag)` lifetime binding before applying the requested
/// write; a failed slot (`slotWritable == false`) persists nothing and
/// writes nothing. A `restart-and-restore` event restores only on an exact
/// lifetime match; a reused lifetime with no stored preimage restores
/// nothing. Core eligibility is never consulted here: these are offline
/// storage invariants, independent of stacking policy.
#[derive(Default)]
struct StackingStorageDriver {
    slots: std::collections::BTreeMap<(String, String), StoredStackingPreimage>,
}

#[derive(Clone, Copy)]
struct StoredStackingPreimage {
    keep_above: bool,
    keep_below: bool,
}

impl StackingStorageDriver {
    fn recoverable_state(
        &mut self,
        win: &str,
        tag: &str,
        target: &serde_json::Value,
        native: &serde_json::Value,
        slot_writable: bool,
    ) -> serde_json::Value {
        // Fail-closed guards before any persistence or write: only the
        // acquire-true above write under controlled authority is modeled.
        // `reason`/`cmdSeq` are inert correlation, never consulted.
        let property = target
            .get("property")
            .and_then(serde_json::Value::as_str)
            .expect("storage target property");
        let value = target
            .get("value")
            .and_then(serde_json::Value::as_bool)
            .expect("storage target value");
        let auth = target
            .get("auth")
            .and_then(serde_json::Value::as_str)
            .expect("storage target authority");
        assert_eq!(
            property, "above",
            "unsupported storage target property \"{property}\": only above is modeled"
        );
        assert_eq!(
            auth, "controlled",
            "unsupported storage target authority \"{auth}\": only controlled is modeled"
        );
        assert!(
            value,
            "unsupported storage target release (value false): no release model; refusing"
        );
        let pre_keep_above = native
            .get("keepAbove")
            .and_then(serde_json::Value::as_bool)
            .expect("storage keepAbove");
        let pre_keep_below = native
            .get("keepBelow")
            .and_then(serde_json::Value::as_bool)
            .expect("storage keepBelow");
        if !slot_writable {
            return serde_json::json!({
                "actions": [],
                "native": {"keepAbove": pre_keep_above, "keepBelow": pre_keep_below},
            });
        }
        self.slots.insert(
            (win.to_owned(), tag.to_owned()),
            StoredStackingPreimage {
                keep_above: pre_keep_above,
                keep_below: pre_keep_below,
            },
        );
        serde_json::json!({
            "actions": [{"kind": "persist-preimage"}, {"kind": "state"}],
            "native": {"keepAbove": true, "keepBelow": false},
        })
    }

    fn restart_and_restore(&self, win: &str, tag: &str) -> serde_json::Value {
        match self.slots.get(&(win.to_owned(), tag.to_owned())) {
            Some(preimage) => serde_json::json!({
                "actions": [{"kind": "restore"}],
                "native": {"keepAbove": preimage.keep_above, "keepBelow": preimage.keep_below},
            }),
            None => serde_json::json!({"actions": []}),
        }
    }
}

fn load_scenario_value(scenario_id: &str) -> serde_json::Value {
    let text =
        std::fs::read_to_string(fixture_root().join(format!("scenarios/{scenario_id}.json")))
            .expect("scenario reads");
    serde_json::from_str(&text).expect("scenario parses")
}

/// One evaluated literal assertion, kept for labelled diagnostics.
struct StepAssertionRecord {
    sid: String,
    expression: String,
    passed: bool,
}

/// Execute one immutable ordered scenario end to end: every wire step runs
/// the genuine decoder/engine/encoder path, every `disconnect` drops the
/// full engine (the next handshake consumes the next generation seed),
/// OPEN/TBD gates are recorded without an invented outcome, and every
/// settled assertion is evaluated literally against `{output,history}`.
/// Core outputs expose the decoded message, its exact bytes and empty
/// actions. Unsupported input kinds, event kinds or expect statuses fail
/// instead of being skipped.
///
/// Test-local execution variants live in [`run_loaded_scenario`]: an ordered
/// step cap for the honest green prefix, and a diagnostic mode that records
/// every literal assertion with per-step output. This wrapper preserves the
/// strict full-scenario behavior: any false assertion panics.
fn run_ordered_scenario(scenario_id: &str) -> OrderedRun {
    run_loaded_scenario(
        &load_scenario_value(scenario_id),
        scenario_id,
        usize::MAX,
        false,
    )
    .0
}

/// Test-local execution of one ordered scenario value (fixture file content
/// or a test-local mutated copy): `max_steps` caps the ordered prefix. With
/// `diagnostic`, every settled assertion is evaluated, recorded and printed
/// instead of panicking on a literal false (jq malfunctions still panic);
/// strict mode panics on any false. Returns the run counters plus one record
/// per evaluated literal assertion.
fn run_loaded_scenario(
    scenario: &serde_json::Value,
    scenario_id: &str,
    max_steps: usize,
    diagnostic: bool,
) -> (OrderedRun, Vec<StepAssertionRecord>) {
    let mut records: Vec<StepAssertionRecord> = Vec::new();
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
    let mut storage = StackingStorageDriver::default();
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
    for step in steps.iter().take(max_steps) {
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
                    (other_to, other_kind) => match (to, event_kind) {
                        ("storage", "recoverable-state") => {
                            let win = event
                                .get("win")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage win");
                            let tag = event
                                .get("tag")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage tag");
                            let target = event.get("target").expect("storage target");
                            let native = event.get("native").expect("storage native");
                            let writable = event
                                .get("slotWritable")
                                .and_then(serde_json::Value::as_bool)
                                .expect("storage slotWritable");
                            storage.recoverable_state(win, tag, target, native, writable)
                        }
                        ("storage", "restart-and-restore") => {
                            let win = event
                                .get("win")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage win");
                            let tag = event
                                .get("tag")
                                .and_then(serde_json::Value::as_str)
                                .expect("storage tag");
                            storage.restart_and_restore(win, tag)
                        }
                        _ => panic!("{sid} unsupported fixture event {other_to}/{other_kind}"),
                    },
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
                    // Diagnostic mode records every literal result and prints
                    // it, so the run continues past semantic failures; the
                    // failed output still enters `history` below, keeping
                    // later steps honest. Strict mode panics on any false.
                    if diagnostic {
                        let passed = check_jq(sid, assertion, &envelope).is_ok();
                        println!(
                            "DIAG {sid}: {}: {assertion}",
                            if passed { "PASS" } else { "FAIL" }
                        );
                        records.push(StepAssertionRecord {
                            sid: sid.to_owned(),
                            expression: (*assertion).to_owned(),
                            passed,
                        });
                        run.assertions += 1;
                    } else {
                        assert_jq(sid, assertion, &envelope);
                        records.push(StepAssertionRecord {
                            sid: sid.to_owned(),
                            expression: (*assertion).to_owned(),
                            passed: true,
                        });
                        run.assertions += 1;
                    }
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
    (run, records)
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

fn storage_target(property: &str, value: bool, auth: &str) -> serde_json::Value {
    serde_json::json!({"property": property, "value": value, "auth": auth, "reason": "", "cmdSeq": null})
}

fn storage_native(keep_above: bool, keep_below: bool) -> serde_json::Value {
    serde_json::json!({"keepAbove": keep_above, "keepBelow": keep_below})
}

#[test]
fn stacking_storage_persists_before_write() {
    let mut driver = StackingStorageDriver::default();
    let output = driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, true),
        true,
    );
    assert_eq!(
        output
            .get("actions")
            .and_then(|actions| actions.as_array())
            .map(|actions| actions
                .iter()
                .map(|action| action.get("kind").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()),
        Some(vec![Some("persist-preimage"), Some("state")]),
        "persist precedes the coupled write"
    );
    assert_eq!(
        output.get("native"),
        Some(&serde_json::json!({"keepAbove": true, "keepBelow": false})),
        "coupled write sets above and clears below"
    );
}

#[test]
fn stacking_storage_failure_writes_nothing() {
    let mut driver = StackingStorageDriver::default();
    let output = driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, false),
        false,
    );
    assert_eq!(
        output.get("actions"),
        Some(&serde_json::json!([])),
        "failed slot performs no actions"
    );
    assert_eq!(
        output.get("native"),
        Some(&serde_json::json!({"keepAbove": false, "keepBelow": false})),
        "failed slot leaves native facts unchanged"
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({"actions": []}),
        "failed persist stores no preimage"
    );
}

#[test]
fn stacking_storage_restores_matching_lifetime_only() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "controlled"),
        &storage_native(false, true),
        true,
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-2"),
        serde_json::json!({"actions": []}),
        "reused lifetime restores nothing"
    );
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({
            "actions": [{"kind": "restore"}],
            "native": {"keepAbove": false, "keepBelow": true},
        }),
        "matching lifetime restores the owned preimage"
    );
}

#[test]
#[should_panic(expected = "unsupported storage target property")]
fn stacking_storage_rejects_non_above_property() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("below", true, "controlled"),
        &storage_native(false, false),
        true,
    );
}

#[test]
#[should_panic(expected = "unsupported storage target authority")]
fn stacking_storage_rejects_uncontrolled_auth() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", true, "native"),
        &storage_native(false, false),
        true,
    );
}

#[test]
#[should_panic(expected = "unsupported storage target release")]
fn stacking_storage_rejects_false_target_release() {
    let mut driver = StackingStorageDriver::default();
    driver.recoverable_state(
        "win-a",
        "tag-1",
        &storage_target("above", false, "controlled"),
        &storage_native(true, false),
        true,
    );
}

#[test]
fn stacking_storage_rejected_target_persists_nothing() {
    use std::panic::AssertUnwindSafe;
    let mut driver = StackingStorageDriver::default();
    let rejected = std::panic::catch_unwind(AssertUnwindSafe(|| {
        driver.recoverable_state(
            "win-a",
            "tag-1",
            &storage_target("below", true, "controlled"),
            &storage_native(false, false),
            true,
        )
    }));
    assert!(rejected.is_err(), "unsupported target must be rejected");
    assert_eq!(
        driver.restart_and_restore("win-a", "tag-1"),
        serde_json::json!({"actions": []}),
        "rejected target stores no preimage"
    );
}

#[test]
fn persistent_above_prefix_first_twenty_steps_pass() {
    // Honest green prefix only: the first 20 ordered steps (one handshake,
    // one settings, eighteen reports, 24 literal assertions) run the genuine
    // decoder/engine/encoder path and pass. This is not a scenario pass.
    let scenario = load_scenario_value("persistent-above");
    let (run, _) = run_loaded_scenario(&scenario, "persistent-above", 20, false);
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    // The second seed is reserved for the later reconnect and is not
    // exercised here; it must validate without being consumed or faked.
    assert_eq!(
        run.generations_consumed, 1,
        "reserved generation seed unconsumed"
    );
    assert_eq!(run.steps, 20, "ordered step count");
    assert_eq!(run.wires, 20, "wire step count");
    assert_eq!(run.handshakes, 1, "handshake step count");
    assert_eq!(run.settings, 1, "settings step count");
    assert_eq!(run.reports, 18, "report step count");
    assert_eq!(run.events, 0, "event step count");
    assert_eq!(run.assertions, 24, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
#[ignore = "7f-3b blocker: capacity oracle contradicts the recorded D1 admission decision; see docs/changes/v4-phase1-core-protocol.md phase note (parked for user)"]
fn persistent_above_runs_ordered_core_scenario() {
    let run = run_ordered_scenario("persistent-above");
    assert_eq!(
        run.generations,
        vec!["generation-a".to_owned(), "generation-b".to_owned(),],
        "generation sequence"
    );
    assert_eq!(
        run.generations_consumed, 2,
        "every generation seed consumed across reconnects"
    );
    assert_eq!(run.steps, 30, "ordered step count");
    assert_eq!(run.wires, 25, "wire step count");
    assert_eq!(run.handshakes, 2, "handshake step count");
    assert_eq!(run.settings, 2, "settings step count");
    assert_eq!(run.reports, 21, "report step count");
    assert_eq!(run.events, 5, "event step count");
    assert_eq!(run.assertions, 41, "evaluated assertion count");
    assert_eq!(run.pending, 0, "pending gate count");
}

#[test]
fn persistent_above_capacity_diagnostic_maps_post_failure_steps() {
    // DIAGNOSTIC ONLY, not scenario evidence: runs all 30 ordered steps in
    // record-everything mode and pins the exact failing literal assertions.
    // Observed failures: step 21 `capacity-float-acquire` (sole
    // oversized-minimum newcomer tiles under recorded D1) and step 22
    // `workspace-mode-float-acquire`'s win-mode-float assertion (win-mode
    // arrives with no allocation and floating:false). Whether step 22 is
    // caused by step 21's tiling is UNRESOLVED: no controlled rerun with a
    // floated step 21 outcome exists, and the same-report close/command/admit
    // ordering is not traced here. Steps 23-30 pass, all eight of them.
    let scenario = load_scenario_value("persistent-above");
    let assertion_at = |index: usize, position: usize| {
        scenario["steps"][index]["expect"]["assertions"][position]
            .as_str()
            .expect("fixture assertion string")
            .to_owned()
    };
    let expected = vec![
        ("capacity-float-acquire".to_owned(), assertion_at(20, 0)),
        (
            "workspace-mode-float-acquire".to_owned(),
            assertion_at(21, 1),
        ),
    ];
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", usize::MAX, true);
    assert_eq!(run.steps, 30, "diagnostic runs every ordered step");
    assert_eq!(run.reports, 21, "diagnostic runs every report");
    assert_eq!(records.len(), 41, "every literal assertion evaluated");
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures, expected,
        "exactly the two observed assertions fail, identified by step and expression"
    );
    println!(
        "DIAG summary: steps={} reports={} assertions={} failures=2 \
         (capacity-float-acquire, workspace-mode-float-acquire win-mode-float); \
         steps 23-30 (8 steps) pass with the actual tiled output in history",
        run.steps,
        run.reports,
        records.len(),
    );
}

#[test]
fn persistent_above_capacity_copy_two_newcomer_exhaustion() {
    // PAIRED DIAGNOSTIC ONLY, not scenario evidence or acceptance: a
    // test-local copy of the capacity step carrying two newcomers (a fresh
    // ordinary anchor plus win-capacity) on tiny-but-valid geometry. The
    // original capacity assertion and every remaining immutable step run
    // unchanged; step 22 closes win-capacity on the standard screen, so the
    // tiny geometry stays local to the copy. Serial admission tiles the
    // empty domain with the listed-first anchor, leaving win-capacity as the
    // geometrically exhausted second leg with pure 1x1 hints.
    //
    // Observed: the original capacity assertion PASSES here (genuine D1
    // exhaustion floats win-capacity with capacity origin and controlled
    // above), while step 22's win-mode-float assertion still FAILS with
    // steps 23-30 passing. That weighs against step 22 being caused by step
    // 21's tiling, but it is not a clean control: the anchor necessarily
    // stays tiled at step 22 entry, unlike the intended empty domain, so
    // full independence is UNRESOLVED. Decision evidence only.
    let mut scenario = load_scenario_value("persistent-above");
    {
        let steps = scenario
            .get_mut("steps")
            .and_then(|value| value.as_array_mut())
            .expect("scenario steps array");
        assert_eq!(
            steps[20].get("id").and_then(serde_json::Value::as_str),
            Some("capacity-float-acquire"),
            "copied step"
        );
        let message = steps[20]
            .pointer_mut("/input/message")
            .expect("step message");
        // Work 25x25 inset by outer 8 on every side gives 9x9 domain bounds;
        // a two-leaf split needs extent at least gap(8) + 2 = 10 on the split
        // axis. Every extent stays positive: valid geometry, not held output
        // or an invalid-bounds refusal.
        for screen in message
            .get_mut("screens")
            .and_then(|value| value.as_array_mut())
            .expect("step screens")
        {
            screen["full"] = serde_json::json!([0, 0, 32, 32]);
            screen["work"] = serde_json::json!([0, 0, 25, 25]);
        }
        let windows = message
            .get_mut("windows")
            .and_then(|value| value.as_array_mut())
            .expect("step windows");
        assert_eq!(windows.len(), 1, "copied step starts with one newcomer");
        let mut anchor = windows[0].clone();
        anchor["win"] = serde_json::json!("win-anchor");
        anchor["rect"] = serde_json::json!([8, 8, 8, 8]);
        anchor["hints"] = serde_json::json!({"minW": 1, "minH": 1, "maxW": null, "maxH": null});
        let capacity = &mut windows[0];
        capacity["rect"] = serde_json::json!([9, 9, 8, 8]);
        capacity["hints"] = serde_json::json!({"minW": 1, "minH": 1, "maxW": null, "maxH": null});
        // Anchor first: serial admission tiles the empty domain with it,
        // leaving win-capacity as the geometrically exhausted second leg.
        windows.insert(0, anchor);
        message["cursor"] = serde_json::json!([10, 10]);
        message["presentation"]["frame"] = serde_json::json!([8, 8, 8, 8]);
        message["presentation"]["pagerOrder"] = serde_json::json!(["win-anchor", "win-capacity"]);
        message["presentation"]["pagerFrames"] = serde_json::json!([
            {"win": "win-anchor", "live": [8, 8, 8, 8]},
            {"win": "win-capacity", "live": [9, 9, 8, 8]},
        ]);
        message["presentation"]["witness"]["frame"] = serde_json::json!([9, 9, 8, 8]);
    }
    let expected = vec![(
        "workspace-mode-float-acquire".to_owned(),
        scenario["steps"][21]["expect"]["assertions"][1]
            .as_str()
            .expect("fixture assertion string")
            .to_owned(),
    )];
    let (run, records) = run_loaded_scenario(&scenario, "persistent-above", usize::MAX, true);
    let failures: Vec<(String, String)> = records
        .iter()
        .filter(|record| !record.passed)
        .map(|record| (record.sid.clone(), record.expression.clone()))
        .collect();
    assert_eq!(
        failures, expected,
        "only step 22's win-mode-float assertion fails in the copy"
    );
    let capacity = records
        .iter()
        .find(|record| record.sid == "capacity-float-acquire")
        .expect("capacity record present");
    assert!(
        capacity.passed,
        "genuine exhaustion satisfies the original capacity oracle"
    );
    assert_eq!(run.steps, 30, "copy runs every ordered step");
    assert_eq!(run.assertions, 41, "copy evaluates every assertion");
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
