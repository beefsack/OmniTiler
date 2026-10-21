//! Portable JSON codec coverage for the logical-first world boundary (v2):
//! DTO validation, u64-as-string enforcement, encoder equality with the
//! core canonical writer, golden bootstrap bytes, and budget gates.
//! Offline only: no D-Bus, no KWin.

use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{WORLD_CONTRACT_VERSION, WorldReport};
use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, decode_request, encode_reply,
};

/// Minimal observation-only request envelope (v2): screens plus facts only,
/// no native workspace concept.
fn request_json(
    owner: &str,
    adapter: &str,
    generation: &str,
    obs: &str,
    req_seq: &str,
    req: &str,
    facts: &str,
) -> String {
    format!(
        "{{\"v\":2,\"owner\":\"{owner}\",\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"{req}\",\"cmdSeq\":null,\
         \"command\":null,\
         \"screens\":[{{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
         \"gap\":0,\"outer\":0,\"complete\":true,\"primary\":true}}],\
         \"windows\":[{facts}],\
         \"views\":[{{\"out\":\"out\",\"view\":\"\"}}],\"closed\":[],\"mode\":\"per-output-local\",
         \"hostOwned\":false,\"equalCount\":false,\"native\":[],\
         \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
         \"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"feedback\":[]}}"
    )
}

fn fact_json(id: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"out\",\"rect\":[{x},0,200,200],\
         \"floating\":false,\"fitExcluded\":false,\"fullscreen\":false,\
         \"maximized\":false,\"sticky\":false,\"fixedAuto\":false,\
         \"fixedSuppress\":false,\"minimized\":false,\"observedWs\":null,
         \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}}}"
    )
}

fn one_window_request() -> String {
    request_json(
        "owner-a",
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &fact_json("win-a", 0),
    )
}

#[test]
fn decode_minimal_observation_only_request() {
    let report: WorldReport = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.contract_version, WORLD_CONTRACT_VERSION);
    assert_eq!(report.contract_version, 2);
    assert_eq!(report.owner, "owner-a");
    assert_eq!(report.observation_seq, 1);
    assert_eq!(report.request_seq, 1);
    assert_eq!(report.screens.len(), 1);
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].window.0, "win-a");
    assert_eq!(report.windows[0].output.0, "out");
    assert!(report.views.len() == 1 && report.views[0].view.is_empty());
    assert!(report.closed.is_empty());
    assert!(!report.host_owned_workspace_domains);
    assert!(!report.equal_workspace_count);
    assert!(report.native.is_empty());
    assert!(report.command.is_none());
}

#[test]
fn counters_reject_json_numbers_and_non_canonical_strings() {
    // JSON numbers where counters belong refuse: no f64 precision loss path.
    let numeric = one_window_request().replace("\"obs\":\"1\"", "\"obs\":1");
    assert!(decode_request(&numeric).is_err());
    let float = one_window_request().replace("\"obs\":\"1\"", "\"obs\":1.0");
    assert!(decode_request(&float).is_err());
    for bad in [
        "007",
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "18446744073709551616",
        "12a",
    ] {
        let request = one_window_request().replace("\"obs\":\"1\"", &format!("\"obs\":\"{bad}\""));
        assert!(decode_request(&request).is_err(), "obs {bad:?} must refuse");
    }
    // u64::MAX is the largest accepted counter.
    let max = one_window_request()
        .replace("\"obs\":\"1\"", "\"obs\":\"18446744073709551615\"")
        .replace("\"reqSeq\":\"1\"", "\"reqSeq\":\"18446744073709551615\"");
    let report = decode_request(&max).expect("u64 max decodes");
    assert_eq!(report.observation_seq, u64::MAX);
    assert_eq!(report.request_seq, u64::MAX);
}

#[test]
fn unknown_fields_and_values_refuse() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["extra"] = serde_json::Value::Bool(true);
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    // The v1 contract is gone: v1 refuses as unsupported-version.
    let old_version = one_window_request().replace("\"v\":2", "\"v\":1");
    let error = decode_request(&old_version).expect_err("v1 refuses");
    assert_eq!(error.kind, "unsupported-version");
    let bad_version = one_window_request().replace("\"v\":2", "\"v\":3");
    let error = decode_request(&bad_version).expect_err("version refuses");
    assert_eq!(error.kind, "unsupported-version");
    // Command without sequence (and sequence without command) refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["command"] = serde_json::json!({"op": "select-trailing", "out": "out"});
    let error = decode_request(&value.to_string()).expect_err("unpaired command refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    // Workspace mode is a required closed vocabulary: unknown tokens
    // refuse as unknown-value, and each approved token decodes.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["mode"] = serde_json::Value::String("bogus-mode".to_owned());
    let error = decode_request(&value.to_string()).expect_err("bad mode refuses");
    assert_eq!(error.kind, "unknown-value");
    for mode in ["per-output-local", "global-unique", "shared"] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["mode"] = serde_json::Value::String(mode.to_owned());
        let report = decode_request(&value.to_string()).expect("mode decodes");
        assert_eq!(
            tiler_core::workspace_scope::mode_token(report.workspace_mode),
            mode
        );
    }
}

#[test]
fn native_observations_require_the_capability() {
    // Native observations without hostOwned refuse at the core gate.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws"}]);
    let error = decode_request(&value.to_string()).expect_err("native refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    // With the capability they ride as pure observations.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["hostOwned"] = serde_json::Value::Bool(true);
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws"}]);
    let report = decode_request(&value.to_string()).expect("host-owned decodes");
    assert!(report.host_owned_workspace_domains);
    assert_eq!(report.native.len(), 1);
    assert_eq!(report.native[0].current_native, "ws");
    // equalCount is a plain capability flag.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["equalCount"] = serde_json::Value::Bool(true);
    let report = decode_request(&value.to_string()).expect("equalCount decodes");
    assert!(report.equal_workspace_count);
    // View echoes and closes decode; unknown view ids stay opaque here
    // (core holds them at publish, never refuses the report).
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["views"] = serde_json::json!([{"out": "out", "view": "ws-9"}]);
    value["closed"] = serde_json::json!(["win-b"]);
    let report = decode_request(&value.to_string()).expect("views/closed decode");
    assert_eq!(report.views[0].view, "ws-9");
    assert_eq!(report.closed.len(), 1);
    // A null rect is an explicitly unreadable fact, not malformed.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"] = serde_json::json!([{
        "win": "win-a", "out": "out", "rect": serde_json::Value::Null,
        "floating": false, "fitExcluded": false, "fullscreen": false,
        "maximized": false, "sticky": false, "fixedAuto": false,
        "fixedSuppress": false, "minimized": false, "observedWs": null,
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
    }]);
    let report = decode_request(&value.to_string()).expect("null rect decodes");
    assert!(report.windows[0].rect.is_none());
    // A minimized fact decodes with no frame.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"] = serde_json::json!([{
        "win": "win-a", "out": "out", "rect": serde_json::Value::Null,
        "floating": false, "fitExcluded": false, "fullscreen": false,
        "maximized": false, "sticky": true, "fixedAuto": false,
        "fixedSuppress": false, "minimized": true, "observedWs": null,
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
    }]);
    let report = decode_request(&value.to_string()).expect("minimized decodes");
    assert!(report.windows[0].minimized);
    assert!(report.windows[0].sticky);
}

#[test]
fn all_commands_decode_typed_and_refuse_bad_vocab() {
    use tiler_core::world_report::WorldCommand as C;
    let command_json = serde_json::json!({"op": "focus", "win": "win-a", "direction": "left"});
    let report = decode_request(&with_command(&command_json, "7")).expect("focus decodes");
    assert!(matches!(
        report.command,
        Some(C::Focus { direction, .. }) if direction == "left"
    ));
    assert_eq!(report.command_seq, Some(7));
    let bad = serde_json::json!({"op": "focus", "win": "win-a", "direction": "sideways"});
    assert!(decode_request(&with_command(&bad, "7")).is_err());
    for (op, extra) in [
        (
            "send",
            serde_json::json!({"win": "w", "out": "o", "ws": "ws-1", "follow": true}),
        ),
        ("switch-view", serde_json::json!({"out": "o", "ws": "ws-1"})),
        (
            "select-workspace",
            serde_json::json!({"out": "o", "ws": "ws-1"}),
        ),
        (
            "select-numbered",
            serde_json::json!({"out": "o", "index": 9}),
        ),
        (
            "select-relative",
            serde_json::json!({"out": "o", "delta": -1}),
        ),
        ("select-previous", serde_json::json!({"out": "o"})),
        ("select-trailing", serde_json::json!({"out": "o"})),
        (
            "send-relative",
            serde_json::json!({"win": "w", "out": "o", "delta": 1, "follow": false}),
        ),
        (
            "send-numbered",
            serde_json::json!({"win": "w", "out": "o", "index": 0, "follow": true}),
        ),
        (
            "set-workspace-tiling",
            serde_json::json!({"out": "o", "ws": "ws-1", "tiled": false}),
        ),
        (
            "migrate-workspace",
            serde_json::json!({"from": "a", "to": "b", "ws": "ws-1"}),
        ),
        (
            "focus-directional",
            serde_json::json!({"win": "w", "direction": "up", "floatSubject": false}),
        ),
        (
            "move-directional",
            serde_json::json!({"win": "w", "direction": "down", "sameAxis": "group-with-neighbor"}),
        ),
        (
            "send-directional",
            serde_json::json!({"win": "w", "direction": "left", "follow": true}),
        ),
        (
            "migrate-directional",
            serde_json::json!({"from": "a", "ws": "ws-1", "direction": "right"}),
        ),
    ] {
        let mut map = serde_json::Map::new();
        map.insert("op".to_owned(), serde_json::Value::String(op.to_owned()));
        for (key, value) in extra.as_object().expect("object") {
            map.insert(key.clone(), value.clone());
        }
        let command = serde_json::Value::Object(map);
        let report = decode_request(&with_command(&command, "7")).expect("command decodes: {op}");
        assert!(report.command.is_some(), "{op}");
    }
    // Bad vocabularies refuse.
    let bad_numbered = serde_json::json!({"op": "select-numbered", "out": "o", "index": 0});
    assert!(decode_request(&with_command(&bad_numbered, "7")).is_err());
    let bad_relative = serde_json::json!({"op": "select-relative", "out": "o", "delta": 2});
    assert!(decode_request(&with_command(&bad_relative, "7")).is_err());
    let bad_send_numbered = serde_json::json!({"op": "send-numbered", "win": "w", "out": "o", "index": 10, "follow": false});
    assert!(decode_request(&with_command(&bad_send_numbered, "7")).is_err());
    let bad_migrate =
        serde_json::json!({"op": "migrate-workspace", "from": "a", "to": "a", "ws": "ws-1"});
    assert!(decode_request(&with_command(&bad_migrate, "7")).is_err());
    let bad_same_axis = serde_json::json!({"op": "move-directional", "win": "w", "direction": "down", "sameAxis": "sideways"});
    assert!(decode_request(&with_command(&bad_same_axis, "7")).is_err());
    let bad_unknown = serde_json::json!({"op": "frobnicate", "out": "o"});
    assert!(decode_request(&with_command(&bad_unknown, "7")).is_err());
}

fn with_command(command: &serde_json::Value, seq: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["command"] = command.clone();
    value["cmdSeq"] = serde_json::Value::String(seq.to_owned());
    value.to_string()
}

#[test]
fn tolerance_backfills_capacity_settings() {
    // Missing or invalid D2/D3 settings backfill defaults, never refuse.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(
        report.anchor_fallback.as_wire_str(),
        tiler_core::admission::AnchorFallback::default().as_wire_str()
    );
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["anchorFallback"] = serde_json::Value::String("bogus".to_owned());
    value["noSpaceResult"] = serde_json::Value::String("bogus".to_owned());
    let report = decode_request(&value.to_string()).expect("tolerant decodes");
    assert_eq!(
        report.anchor_fallback.as_wire_str(),
        tiler_core::admission::AnchorFallback::default().as_wire_str()
    );
    assert_eq!(
        report.no_space_result.as_wire_str(),
        tiler_core::admission::NoSpaceResult::default().as_wire_str()
    );
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["anchorFallback"] = serde_json::Value::String("no-search".to_owned());
    value["noSpaceResult"] = serde_json::Value::String("allow-overlap".to_owned());
    let report = decode_request(&value.to_string()).expect("valid settings decode");
    assert_eq!(report.anchor_fallback.as_wire_str(), "no-search");
    assert_eq!(report.no_space_result.as_wire_str(), "allow-overlap");
}

#[test]
fn request_estimate_bounds_actual_wire_bytes() {
    let compact = one_window_request();
    let report = decode_request(&compact).expect("decodes");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= compact.len(),
        "estimate {estimate} bounds wire {}",
        compact.len()
    );
    let empty = request_json("owner-a", "adapter-1", "gen-1", "1", "1", "req-1", "");
    let report = decode_request(&empty).expect("empty decodes");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(estimate >= empty.len());
}

#[test]
fn overbound_request_refuses_before_core_mutation() {
    let big = "w".repeat(WORLD_MAX_REQUEST_BYTES + 1);
    let request = one_window_request().replace("win-a", &big);
    assert!(request.len() > WORLD_MAX_REQUEST_BYTES);
    let error = decode_request(&request).expect_err("oversized refuses");
    assert_eq!(error.kind, "oversized");
}

#[test]
fn reply_encode_matches_core_canonical_writer() {
    // Drive the engine once, then lock the transport encoding to the core
    // canonical writer over the same reply.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical);
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
    // The reply carries logical workspaces (stable core ids), screens as
    // outputs, and echo views.
    let parsed: WorldReplyWire = serde_json::from_str(&wire).expect("valid json");
    assert_eq!(parsed.v, 2);
    assert!(parsed.ready);
    assert!(!parsed.workspaces.is_empty());
    assert!(!parsed.outputs.is_empty());
}

#[derive(serde::Deserialize)]
struct WorldReplyWire {
    v: u32,
    ready: bool,
    workspaces: Vec<serde_json::Value>,
    outputs: Vec<serde_json::Value>,
}

#[test]
fn golden_bootstrap_reply_bytes_lock_wire_shape() {
    // The discovery reply is fixed shape: v2, ready false, empty sections.
    let engine = WorldEngine::new("gen-1").expect("generation");
    let reply = engine.bootstrap_reply("adapter-1", "req-1");
    let wire = encode_reply(&reply);
    assert_eq!(
        wire,
        "{\"v\":2,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"req\":\"req-1\",\
         \"obs\":\"0\",\"rev\":\"0\",\"ready\":false,\"consumed\":false,\
         \"scopes\":[],\"workspaces\":[],\"outputs\":[],\"windows\":[],\
         \"transfers\":[],\"effects\":[],\"result\":null}"
    );
}

#[test]
fn snapshot_rejection_names_fixed_check_reason() {
    // A fact on an unknown screen refuses with fixed vocabulary, never
    // payload bytes.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"] = serde_json::json!([{
        "win": "win-a", "out": "nope", "rect": [0, 0, 10, 10],
        "floating": false, "fitExcluded": false, "fullscreen": false,
        "maximized": false, "sticky": false, "fixedAuto": false,
        "fixedSuppress": false, "minimized": false, "observedWs": null,
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
    }]);
    // Unknown output screens must exist for the fact: add the screen and
    // the fact validates structurally; core then holds (not tested here).
    value["screens"] = serde_json::json!([
        {"out": "out", "full": [0, 0, 800, 600], "work": [0, 0, 800, 560],
         "gap": 0, "outer": 0, "complete": true, "primary": true},
    ]);
    let error = decode_request(&value.to_string()).expect_err("unknown output refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "window-output-unknown");
}

#[test]
fn unknown_restoration_key_refuses() {
    // No restoration ingress exists on the wire: a present `restored`
    // key is unknown and refuses before any semantic check. Reports
    // without the key decode.
    let report = decode_request(&one_window_request()).expect("decodes");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= one_window_request().len(),
        "estimate {estimate} bounds wire {}",
        one_window_request().len()
    );
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["restored"] = serde_json::json!({
        "rings": [{"out": "out", "ws": ["ws-1"]}],
        "views": [{"out": "out", "view": "ws-1"}],
        "tiling": [{"out": "out", "ws": "ws-1", "tiled": true}],
        "members": [{"win": "win-a", "out": "out", "ws": "ws-1"}],
    });
    assert!(decode_request(&value.to_string()).is_err());
    let mut null_value: serde_json::Value =
        serde_json::from_str(&one_window_request()).expect("valid");
    null_value["restored"] = serde_json::Value::Null;
    assert!(decode_request(&null_value.to_string()).is_err());
}
