//! Genuine engine provenance for the v4-active retained intent journey:
//! save R, two skipped commandless retained publications R+1/R+2, a
//! proven-refusal null clear, and the retained clear. Every recorded reply
//! below is a genuine [`WorldEngine`] product, never a hand-set fixture:
//! each step input is the scenario base report driven through the real
//! engine, then encoded with the genuine strict reply codec. The freshly
//! produced bytes must equal the committed scenario step replies exactly
//! (typed decode equality plus encode round-trip stability and exact string
//! equality), with the retained-map oracles held on every recorded step.
//!
//! Reproduction route: this file. No outside producer, no absolute paths;
//! scenario inputs resolve manifest-relative under
//! `test-fixtures/world-wire/v4-active/scenarios/`.
//!
//! Offline only: no adapters, no live work.

use tiler_core::directional::WindowId;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{IntentDesire, WorldReplyV4, validate_intent_desired_reason};
use tiler_core::world_sizing_v4::reply_exact_bytes_v4;
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, WorldOutcome, decode_reply_v4, decode_request,
    encode_reply_v4,
};

const SCENARIO_ID: &str = "intent-retained-journey";
const EXPECTED_IDS: [&str; 6] = [
    "seed",
    "save",
    "retained-1",
    "retained-2",
    "clear",
    "clear-retained",
];

fn scenario_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active/scenarios")
        .join(format!("{SCENARIO_ID}.json"))
}

fn base_report_value() -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages/report-intent-float.json");
    let mut base: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture JSON"))
            .expect("fixture JSON");
    base["adapter"] = serde_json::json!("adapter-1");
    base["gen"] = serde_json::json!("gen-1");
    base["intentRecords"] = serde_json::json!([]);
    base
}

fn step_report_wire(
    base: &serde_json::Value,
    req: &str,
    seq: &str,
    obs: &str,
    cmd_seq: serde_json::Value,
    command: serde_json::Value,
    attempts: serde_json::Value,
) -> String {
    let mut value = base.clone();
    value["req"] = serde_json::json!(req);
    value["reqSeq"] = serde_json::json!(seq);
    value["obs"] = serde_json::json!(obs);
    value["cmdSeq"] = cmd_seq;
    value["command"] = command;
    value["stateAttempts"] = attempts;
    value.to_string()
}

/// Encode one engine-published v4 reply with the canonical reply writer;
/// the bytes must match the independent core byte count exactly and
/// round-trip through the reply decoder to the identical typed reply
/// plus wire metadata.
fn reply_wire(reply: &WorldReplyV4) -> String {
    let decoded = DecodedReplyV4 {
        reply: reply.clone(),
        wire: ReplyWireOptionals {
            suspended_output_present: reply.presentation.suspended_output.is_some(),
        },
    };
    let wire = encode_reply_v4(&decoded).expect("reply encodes");
    assert_eq!(
        wire.len(),
        reply_exact_bytes_v4(reply),
        "core byte count is exact over the wire"
    );
    assert_eq!(
        decode_reply_v4(&wire).expect("reply round-trips"),
        decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    wire
}

fn drive(engine: &mut WorldEngine, report_wire: &str, step: &str) -> (WorldReplyV4, String) {
    let report =
        decode_request(report_wire).unwrap_or_else(|error| panic!("{step} decodes: {error:?}"));
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted {step}, got {other:?}"),
    };
    let wire = reply_wire(&reply);
    (reply, wire)
}

/// Drive the genuine engine through the full journey and return the
/// recorded (step id, report wire, reply wire) triples. The refused-proof
/// revision is read from genuine engine output, never hardcoded.
fn produce() -> Vec<(String, String, String)> {
    let base = base_report_value();
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("bound engine");
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut drive_step = |id: &str,
                          req: &str,
                          seq: &str,
                          obs: &str,
                          cmd_seq: serde_json::Value,
                          command: serde_json::Value,
                          attempts: serde_json::Value,
                          engine: &mut WorldEngine|
     -> WorldReplyV4 {
        let report_wire = step_report_wire(&base, req, seq, obs, cmd_seq, command, attempts);
        let (reply, reply_wire) = drive(engine, &report_wire, id);
        out.push((id.to_owned(), report_wire, reply_wire));
        reply
    };

    let seed = drive_step(
        "seed",
        "req-1",
        "1",
        "1",
        serde_json::Value::Null,
        serde_json::Value::Null,
        serde_json::json!([]),
        &mut engine,
    );
    assert!(seed.intent_desired.is_empty(), "seed publishes no writes");

    let save = drive_step(
        "save",
        "req-2",
        "2",
        "2",
        serde_json::json!("1"),
        serde_json::json!({"op": "set-sticky", "win": "win-a", "value": true}),
        serde_json::json!([]),
        &mut engine,
    );
    assert_eq!(
        save.intent_desired,
        vec![IntentDesire {
            win: WindowId("win-a".to_owned()),
            data: Some(tiler_core::intent_data::IntentData::new(
                Some(true),
                Some(false)
            )),
        }],
        "sticky ON records the false-prior save"
    );
    let binding_rev = save.state_revision;

    for (id, req, seq, obs) in [
        ("retained-1", "req-3", "3", "3"),
        ("retained-2", "req-4", "4", "4"),
    ] {
        let retained = drive_step(
            id,
            req,
            seq,
            obs,
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::json!([]),
            &mut engine,
        );
        assert_eq!(
            retained.intent_desired, save.intent_desired,
            "{id} retains the save with no map drain"
        );
    }

    let clear = drive_step(
        "clear",
        "req-5",
        "5",
        "5",
        serde_json::Value::Null,
        serde_json::Value::Null,
        serde_json::json!([{
            "win": "win-a",
            "property": "sticky",
            "target": true,
            "rev": binding_rev.to_string(),
            "setter": "refused",
            "pre": false,
            "post": false,
            "attemptMs": "7",
        }]),
        &mut engine,
    );
    assert_eq!(
        clear.intent_desired,
        vec![IntentDesire {
            win: WindowId("win-a".to_owned()),
            data: None,
        }],
        "proven refusal rolls back to the explicit null clear"
    );

    let held = drive_step(
        "clear-retained",
        "req-6",
        "6",
        "6",
        serde_json::Value::Null,
        serde_json::Value::Null,
        serde_json::json!([]),
        &mut engine,
    );
    assert_eq!(
        held.intent_desired, clear.intent_desired,
        "clear stays retained after rollback"
    );
    out
}

fn recorded() -> serde_json::Value {
    let path = scenario_path();
    if std::env::var("UPDATE_INTENT_RETAINED_FIXTURE").as_deref() == Ok("1") {
        let steps: Vec<serde_json::Value> = produce()
            .into_iter()
            .map(|(id, report, reply)| {
                serde_json::json!({"id": id, "report": report, "reply": reply})
            })
            .collect();
        let scenario = serde_json::json!({
            "format": "world-wire-v4-intent-journey",
            "id": SCENARIO_ID,
            "engine": "WorldEngine::new_test_bound(gen-1, adapter-1) with native_sticky+maximize; base v4 report-intent-float rebound to adapter-1/gen-1",
            "steps": steps,
        });
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&scenario).expect("scenario serializes") + "\n",
        )
        .expect("scenario records");
    }
    serde_json::from_str(&std::fs::read_to_string(&path).expect("scenario reads"))
        .expect("scenario parses")
}

fn assert_focused_proofs(replies: &[(String, WorldReplyV4)]) {
    assert_eq!(replies.len(), EXPECTED_IDS.len(), "recorded reply count");
    for (index, expected) in EXPECTED_IDS.iter().enumerate() {
        assert_eq!(&replies[index].0, expected, "record order");
    }
    let by_id = |id: &str| {
        replies
            .iter()
            .find(|(step, _)| step == id)
            .unwrap_or_else(|| panic!("{id} reply present"))
            .1
            .clone()
    };
    let seed = by_id("seed");
    let save = by_id("save");
    let first = by_id("retained-1");
    let second = by_id("retained-2");
    let clear = by_id("clear");
    let held = by_id("clear-retained");

    assert!(seed.intent_desired.is_empty(), "seed publishes no writes");
    // Typed core rows, unique by win, no aliases: every non-seed step
    // carries exactly the single win-a row.
    for (id, reply) in replies {
        assert_eq!(
            validate_intent_desired_reason(&reply.intent_desired),
            None,
            "{id} rows are valid under the commit gate"
        );
        if id == "seed" {
            continue;
        }
        assert_eq!(reply.intent_desired.len(), 1, "{id} carries one row");
        assert_eq!(
            reply.intent_desired[0].win,
            WindowId("win-a".to_owned()),
            "{id} row targets win-a with no alias"
        );
    }
    // Save is the typed false-prior value, not a stringly copy.
    match &save.intent_desired[..] {
        [
            IntentDesire {
                win,
                data: Some(data),
            },
        ] => {
            assert_eq!(win, &WindowId("win-a".to_owned()));
            assert_eq!(
                *data,
                tiler_core::intent_data::IntentData::new(Some(true), Some(false)),
                "save is the typed false-prior value"
            );
        }
        other => panic!("save shape: {other:?}"),
    }
    // Publication never drains: the skipped commandless replies retain the
    // identical typed map.
    assert_eq!(first.intent_desired, save.intent_desired, "R+1 retains R");
    assert_eq!(second.intent_desired, save.intent_desired, "R+2 retains R");
    // The proven refusal is an explicit typed null clear, retained again
    // after rollback.
    assert_eq!(clear.intent_desired.len(), 1);
    assert_eq!(clear.intent_desired[0].win, WindowId("win-a".to_owned()));
    assert!(
        clear.intent_desired[0].data.is_none(),
        "refusal proof rolls back to the explicit null clear"
    );
    assert_eq!(
        held.intent_desired, clear.intent_desired,
        "clear retained again"
    );
    assert_ne!(
        held.intent_desired, save.intent_desired,
        "rollback replaces the save with the clear"
    );
    // The canonical writer spells the save and the clear exactly.
    let save_wire = reply_wire(&save);
    assert!(
        save_wire.contains(
            "\"intentDesired\":[{\"win\":\"win-a\",\"data\":\"{\\\"v\\\":1,\
             \\\"float\\\":true,\\\"stickyPrior\\\":false}\"}]"
        ),
        "exact save bytes, got {save_wire}"
    );
    let clear_wire = reply_wire(&clear);
    assert!(
        clear_wire.contains("\"intentDesired\":[{\"win\":\"win-a\",\"data\":null}]"),
        "exact clear bytes, got {clear_wire}"
    );
}

#[test]
fn intent_retained_journey_replies_are_genuine_engine_products() {
    let scenario = recorded();
    assert_eq!(
        scenario.get("format").and_then(serde_json::Value::as_str),
        Some("world-wire-v4-intent-journey"),
        "scenario format"
    );
    assert_eq!(
        scenario.get("id").and_then(serde_json::Value::as_str),
        Some(SCENARIO_ID),
        "scenario id"
    );
    let steps = scenario["steps"].as_array().expect("scenario steps");
    assert_eq!(steps.len(), EXPECTED_IDS.len(), "scenario step count");
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("bound engine");
    let mut replies: Vec<(String, WorldReplyV4)> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let id = step["id"].as_str().expect("step id");
        assert_eq!(id, EXPECTED_IDS[index], "record order");
        let report_wire = step["report"].as_str().expect("step report");
        let expected_wire = step["reply"].as_str().expect("step reply");
        let (reply, fresh_wire) = drive(&mut engine, report_wire, id);
        // Exact-byte replay: the fresh genuine encoder output equals the
        // committed bytes, the committed bytes decode to the identical
        // typed reply, and re-encoding is stable.
        assert_eq!(
            fresh_wire, expected_wire,
            "{id} fresh bytes equal committed bytes"
        );
        let expected_decoded =
            decode_reply_v4(expected_wire).expect("{id} committed reply decodes");
        assert_eq!(
            expected_decoded.reply, reply,
            "{id} committed reply equals the fresh engine reply"
        );
        let round_trip =
            encode_reply_v4(&expected_decoded).expect("{id} committed reply re-encodes");
        assert_eq!(
            round_trip, expected_wire,
            "{id} committed bytes are encoder-stable"
        );
        replies.push((id.to_owned(), reply));
    }
    // The refused proof binds the genuine save revision, never a
    // hardcoded constant: the stored clear input stamps the save reply's
    // own state revision.
    let save_rev = replies
        .iter()
        .find(|(id, _)| id == "save")
        .expect("save reply")
        .1
        .state_revision
        .to_string();
    assert!(
        steps[4]["report"]
            .as_str()
            .expect("clear report")
            .contains(&format!("\"rev\":\"{save_rev}\"")),
        "clear proof stamps the genuine save revision"
    );
    assert_focused_proofs(&replies);
}
