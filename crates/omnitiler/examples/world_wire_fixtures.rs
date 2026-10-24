//! Regenerate the checked-in world-wire reply fixtures from the real broker.
//!
//! The request fixtures (`discover.json`, `report-*.json`) are byte-gated
//! against the production KWin entry and stay untouched: this tool only
//! rewrites the six `reply-*.json` responses by dispatching realistic
//! complete reports through the actual [`WorldService`] (discovery,
//! capture, queue-drain, take, presentation) and writing the bound replies.
//!
//! Transport bindings are volatile by construction (fresh broker
//! incarnation, fresh generation, dispatch-clock reading), so the writer
//! normalizes exactly those three (`adapter` to `broker-1`, `gen` to
//! `gen-1`, `clock` to `"0"`) plus the per-scenario `source`/`req`
//! correlation it minted. Every other byte is the verbatim broker output:
//! workspace rows carry no `logical` key (the retired v1 field), window
//! rows carry both write tokens, and the actuator lane carries `result`
//! null. The writer asserts the exact v2 key sets and fails loudly if the
//! core shape drifts, so regeneration doubles as a producer-side shape
//! gate. Run with:
//!
//! ```sh
//! cargo run -p omnitiler --example world_wire_fixtures
//! ```
//!
//! Inputs reused verbatim where they exist: `report-observe.json` feeds the
//! golden reply and `report-outer-gap.json` feeds the outer-gap reply (both
//! are the byte-gated production payloads). The float/group/preview/migrate
//! snapshots reuse the same producer report shape (copied presentation and
//! report envelope keys from the observed report) with small synthetic
//! screens, and mirror the queued terminal commands the TS suites consume
//! through the take lane.

use omnitiler::world_service::WorldService;
use serde_json::Value;

const FIXTURE_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test-fixtures/world-wire"
);
const LIFETIME: &str = "test-adapter";
const SENDER: &str = ":1.42";

const BOUND_KEYS: [&str; 22] = [
    "adapter",
    "clock",
    "consumed",
    "effects",
    "gen",
    "gestureSettled",
    "nativeInvalid",
    "obs",
    "operationSettled",
    "outputs",
    "presentation",
    "ready",
    "req",
    "result",
    "rev",
    "scopes",
    "source",
    "transfers",
    "v",
    "warpCursor",
    "windows",
    "workspaces",
];

const WORKSPACE_KEYS: [&str; 8] = [
    "bind", "exist", "hold", "id", "mode", "order", "out", "scope",
];

fn load_report(name: &str) -> Value {
    let raw = std::fs::read_to_string(format!("{FIXTURE_DIR}/{name}"))
        .unwrap_or_else(|e| panic!("read {name}: {e}"));
    let envelope: Value = serde_json::from_str(&raw).expect("report envelope parses");
    envelope
        .get("report")
        .cloned()
        .expect("envelope carries an inner report")
}

fn js_source(seq: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut value = seq;
    let mut counter = Vec::new();
    if value == 0 {
        counter.push(b'0');
    }
    while value > 0 {
        counter.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    counter.reverse();
    format!("src-{}-t", String::from_utf8(counter).unwrap())
}

fn capture_envelope(
    lifetime: &str,
    generation: &str,
    seq: u64,
    watermark: u64,
    inner: &Value,
) -> String {
    let source = js_source(seq);
    format!(
        "{{\"v\":2,\"adapter\":\"{lifetime}\",\"gen\":\"{generation}\",\
         \"source\":\"{source}\",\"sourceSeq\":\"{seq}\",\
         \"nativeInvalid\":\"{watermark}\",\"report\":{inner}}}"
    )
}

fn check_keys(value: &Value, keys: &[&str], what: &str) {
    let map = value
        .as_object()
        .unwrap_or_else(|| panic!("{what} is an object"));
    let mut got: Vec<&str> = map.keys().map(String::as_str).collect();
    got.sort_unstable();
    let mut want: Vec<&str> = keys.to_vec();
    want.sort_unstable();
    assert_eq!(got, want, "exact v2 keys for {what}");
}

/// Discover once; returns the serving generation and broker incarnation.
/// Verifies the bootstrap binds the broker identity (never the producer
/// lifetime) with `ready=false` and no capture transport bindings.
fn discover(service: &WorldService, request_id: &str) -> (String, String) {
    let bootstrap: Value = serde_json::from_str(&service.discovery_body(LIFETIME, request_id))
        .expect("bootstrap parses");
    assert_eq!(bootstrap["ready"], false);
    assert_eq!(bootstrap["req"], request_id);
    let generation = bootstrap["gen"].as_str().expect("gen").to_owned();
    let incarnation = bootstrap["adapter"].as_str().expect("broker").to_owned();
    assert!(!generation.is_empty() && !incarnation.is_empty());
    assert_ne!(
        incarnation, LIFETIME,
        "reply names the broker, not the producer"
    );
    assert!(
        bootstrap.get("source").is_none(),
        "bootstrap binds no source"
    );
    assert!(bootstrap.get("clock").is_none(), "bootstrap binds no clock");
    let mut core_keys = BOUND_KEYS
        .iter()
        .filter(|k| **k != "source" && **k != "nativeInvalid" && **k != "clock")
        .copied()
        .collect::<Vec<_>>();
    core_keys.sort_unstable();
    let mut got: Vec<&str> = bootstrap
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    got.sort_unstable();
    assert_eq!(got, core_keys, "exact bootstrap keys");
    (generation, incarnation)
}

/// Dispatch one capture; asserts the exact bound v2 shape (no retired
/// `logical` workspace key, both window write tokens, null actuator
/// result) and returns the parsed reply.
fn dispatch(service: &WorldService, envelope: &str) -> Value {
    let text = service
        .evaluate_capture(envelope)
        .expect("capture accepted");
    let reply: Value = serde_json::from_str(&text).expect("reply parses");
    assert_eq!(reply["v"], 2);
    assert_eq!(reply["ready"], true);
    assert!(
        reply["result"].is_null(),
        "actuator lane carries no terminal result"
    );
    // The dispatch clock is a canonical decimal u64 in a foreign
    // (broker-local) epoch; the writer normalizes it to "0" for stable
    // bytes, but the live value must already be well-formed here.
    assert!(
        reply["clock"].as_str().is_some_and(|c| c
            .parse::<u64>()
            .is_ok_and(|_| !c.is_empty() && c.len() <= 20 && (c == "0" || !c.starts_with('0')))),
        "canonical dispatch clock"
    );
    check_keys(&reply, &BOUND_KEYS, "bound reply");
    for ws in reply["workspaces"].as_array().expect("workspaces") {
        check_keys(ws, &WORKSPACE_KEYS, "workspace row");
    }
    reply
}

/// Normalize exactly the volatile transport bindings (broker incarnation,
/// generation, dispatch clock) plus the minted per-scenario correlation to
/// stable tokens. Snapshot rows stay byte-identical broker output.
fn normalize(mut reply: Value, source: &str) -> Value {
    let map = reply.as_object_mut().expect("object");
    map.insert("adapter".to_owned(), Value::String("broker-1".to_owned()));
    map.insert("gen".to_owned(), Value::String("gen-1".to_owned()));
    map.insert("req".to_owned(), Value::String(source.to_owned()));
    map.insert("source".to_owned(), Value::String(source.to_owned()));
    map.insert("clock".to_owned(), Value::String("0".to_owned()));
    reply
}

fn write_fixture(name: &str, reply: &Value) {
    let mut text = serde_json::to_string(reply).expect("serializes");
    text.push('\n');
    // Round-trip determinism: sorted keys serialize byte-identically.
    let again = {
        let v: Value = serde_json::from_str(&text).expect("reparses");
        let mut t = serde_json::to_string(&v).expect("reserializes");
        t.push('\n');
        t
    };
    assert_eq!(text, again, "stable serialization for {name}");
    std::fs::write(format!("{FIXTURE_DIR}/{name}"), &text)
        .unwrap_or_else(|e| panic!("write {name}: {e}"));
    println!("wrote {name} ({} bytes)", text.len());
}

fn base_inner() -> Value {
    load_report("report-observe.json")
}

fn producer_window(win: &str, out: &str, native_ws: &str, rect: [i64; 4]) -> Value {
    serde_json::json!({
        "win": win,
        "out": out,
        "rect": rect,
        "floating": false,
        "fitExcluded": false,
        "fullscreen": false,
        "maximized": false,
        "sticky": false,
        "fixedAuto": false,
        "fixedSuppress": false,
        "minimized": false,
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
        "observedWs": null,
        "nativeWs": native_ws,
    })
}

fn main() {
    // Golden: the byte-gated production observation through the real broker.
    {
        let service = WorldService::new();
        let (generation, incarnation) = discover(&service, "disc-2");
        println!("golden broker={incarnation} gen={generation}");
        let inner = load_report("report-observe.json");
        let envelope = capture_envelope(LIFETIME, &generation, 2, 2, &inner);
        let reply = dispatch(&service, &envelope);
        assert_eq!(reply["rev"], "1");
        assert_eq!(reply["adapter"], incarnation.as_str());
        write_fixture("reply-golden.json", &normalize(reply, "req-2"));
    }
    // Outer gap: the byte-gated outer-gap observation through the broker.
    {
        let service = WorldService::new();
        let (generation, incarnation) = discover(&service, "disc-1");
        println!("outer-gap broker={incarnation} gen={generation}");
        let inner = load_report("report-outer-gap.json");
        let envelope = capture_envelope(LIFETIME, &generation, 1, 1, &inner);
        let reply = dispatch(&service, &envelope);
        assert_eq!(reply["adapter"], incarnation.as_str());
        let frames: Vec<Value> = reply["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .map(|w| w["frame"].clone())
            .collect();
        assert!(frames.iter().all(|f| f.is_array()), "both leaves tiled");
        write_fixture("reply-outer-gap.json", &normalize(reply, "req-1"));
    }
    // Two-leaf snapshot shared by the group and preview take-lane suites:
    // one 800x600 output, inner gap 8, outer gap 4.
    let two_leaf = |focused: &str| -> Value {
        let mut inner = base_inner();
        let map = inner.as_object_mut().expect("object");
        map.insert(
            "screens".to_owned(),
            serde_json::json!([{"out": "out", "full": [0, 0, 800, 600],
                "work": [0, 0, 800, 600], "gap": 8, "outer": 4,
                "complete": true, "primary": true}]),
        );
        map.insert(
            "windows".to_owned(),
            Value::Array(vec![
                producer_window("win-a", "out", "ws", [0, 0, 100, 100]),
                producer_window("win-b", "out", "ws", [400, 0, 100, 100]),
            ]),
        );
        map.insert(
            "views".to_owned(),
            serde_json::json!([{"out": "out", "view": ""}]),
        );
        map.insert(
            "native".to_owned(),
            serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws"}]),
        );
        map.insert("hostOwned".to_owned(), Value::Bool(true));
        map.insert("focused".to_owned(), Value::String(focused.to_owned()));
        map.insert("actionOutputs".to_owned(), serde_json::json!(["out"]));
        map.insert(
            "actionForeground".to_owned(),
            Value::String("out".to_owned()),
        );
        map.insert("closed".to_owned(), Value::Array(vec![]));
        inner
    };
    for (name, source) in [
        ("reply-group.json", "req-2"),
        ("reply-preview.json", "req-2"),
    ] {
        let service = WorldService::new();
        let (generation, _) = discover(&service, "disc-2");
        let inner = two_leaf("win-a");
        let envelope = capture_envelope(LIFETIME, &generation, 2, 1, &inner);
        let reply = dispatch(&service, &envelope);
        // The paired take lane (not the file) carries the correlated
        // terminal result; the snapshot stays result-free.
        let presentation: Value = serde_json::from_str(
            &service
                .read_presentation("{\"v\":2,\"req\":\"pager-1\"}")
                .expect("presentation reads"),
        )
        .expect("presentation parses");
        assert_eq!(presentation["outcome"], "presentation");
        for window in presentation["windows"].as_array().expect("windows") {
            assert!(window.get("frame_op").is_none());
            assert!(window.get("clear_op").is_none());
        }
        write_fixture(name, &normalize(reply, source));
    }
    // Retained float placement: toggle-float without an explicit rect
    // centers the float, and the draining capture keeps the write token.
    {
        let service = WorldService::new();
        let (generation, _) = discover(&service, "disc-2");
        let inner = two_leaf("win-a");
        let queued: Value = serde_json::from_str(
            &service
                .queue_command(
                    SENDER,
                    &format!(
                        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
                         \"command\":{{\"op\":\"operation\",\"command\":\
                         {{\"op\":\"toggle-float\",\"win\":\"win-a\",\"rect\":null}}}}}}"
                    ),
                )
                .expect("float queues"),
        )
        .expect("queue parses");
        assert_eq!(queued["outcome"], "queued");
        let envelope = capture_envelope(LIFETIME, &generation, 2, 1, &inner);
        let reply = dispatch(&service, &envelope);
        let float = reply["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .find(|w| w["win"] == "win-a")
            .expect("win-a present")
            .clone();
        assert_eq!(float["floating"], true);
        assert!(
            float["frame_op"].as_str().is_some_and(|op| !op.is_empty()),
            "drain keeps the float write token"
        );
        let ready: Value = serde_json::from_str(
            &service
                .take_result(
                    SENDER,
                    &format!(
                        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
                         \"clientCmd\":\"1\"}}"
                    ),
                )
                .expect("take collects"),
        )
        .expect("take parses");
        assert_eq!(ready["outcome"], "ready");
        assert_eq!(ready["result"]["op"], "toggle-float");
        write_fixture("reply-float.json", &normalize(reply, "req-2"));
    }
    // Whole-workspace migration: baseline, file the move, then drain with
    // the members still natively behind, so the transfer retains the hold.
    {
        let service = WorldService::new();
        let (generation, _) = discover(&service, "disc-2");
        let mut inner = base_inner();
        let map = inner.as_object_mut().expect("object");
        map.insert(
            "screens".to_owned(),
            serde_json::json!([
                {"out": "out-a", "full": [0, 0, 800, 600], "work": [0, 0, 800, 600],
                 "gap": 8, "outer": 4, "complete": true, "primary": true},
                {"out": "out-b", "full": [800, 0, 800, 600], "work": [800, 0, 800, 600],
                 "gap": 8, "outer": 4, "complete": true, "primary": false},
            ]),
        );
        map.insert(
            "windows".to_owned(),
            Value::Array(vec![
                producer_window("win-a", "out-a", "wa1", [0, 0, 100, 100]),
                producer_window("win-b", "out-b", "wb1", [800, 0, 100, 100]),
            ]),
        );
        map.insert(
            "views".to_owned(),
            serde_json::json!([
                {"out": "out-a", "view": "wa1"},
                {"out": "out-b", "view": "wb1"},
            ]),
        );
        map.insert(
            "native".to_owned(),
            serde_json::json!([
                {"out": "out-a", "order": ["wa1"], "current": "wa1"},
                {"out": "out-b", "order": ["wb1"], "current": "wb1"},
            ]),
        );
        map.insert("hostOwned".to_owned(), Value::Bool(true));
        map.insert("focused".to_owned(), Value::String("win-a".to_owned()));
        map.insert(
            "actionOutputs".to_owned(),
            serde_json::json!(["out-a", "out-b"]),
        );
        map.insert(
            "actionForeground".to_owned(),
            Value::String("out-a".to_owned()),
        );
        map.insert("closed".to_owned(), Value::Array(vec![]));
        let baseline = capture_envelope(LIFETIME, &generation, 1, 1, &inner);
        let first = dispatch(&service, &baseline);
        assert_eq!(first["rev"], "1");
        // Current mapping semantics: native desktops bind to
        // planner-owned logical ids (wa1 -> ws-1 here), never double as
        // them. The migration names the stable logical id from the
        // baseline, exactly like the production entry resolves it.
        let logical = first["workspaces"]
            .as_array()
            .expect("workspaces")
            .iter()
            .find(|w| w["out"] == "out-a" && w["bind"] == "wa1")
            .expect("wa1 bound on out-a")["id"]
            .as_str()
            .expect("logical id")
            .to_owned();
        let queued: Value = serde_json::from_str(
            &service
                .queue_command(
                    SENDER,
                    &format!(
                        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\"clientCmd\":\"1\",\
                         \"command\":{{\"op\":\"migrate-workspace\",\"from\":\"out-a\",\
                         \"to\":\"out-b\",\"ws\":\"{logical}\"}}}}",
                    ),
                )
                .expect("migration queues"),
        )
        .expect("queue parses");
        assert_eq!(queued["outcome"], "queued");
        // Members still natively behind on the drain: the core moves the
        // homing and retains the transfer with the paired source refill.
        let drain = capture_envelope(LIFETIME, &generation, 2, 1, &inner);
        let reply = dispatch(&service, &drain);
        let migrations: Vec<&Value> = reply["transfers"]
            .as_array()
            .expect("transfers")
            .iter()
            .filter(|t| t["kind"] == "migration")
            .collect();
        assert_eq!(migrations.len(), 1, "drain retains the migration transfer");
        let ready: Value = serde_json::from_str(
            &service
                .take_result(
                    SENDER,
                    &format!(
                        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"panel-1\",\
                         \"clientCmd\":\"1\"}}"
                    ),
                )
                .expect("take collects"),
        )
        .expect("take parses");
        assert_eq!(ready["outcome"], "ready");
        assert_eq!(ready["result"]["op"], "migrate-workspace");
        write_fixture("reply-migrate.json", &normalize(reply, "req-2"));
    }
    println!("all reply fixtures regenerated from the live broker");
}
