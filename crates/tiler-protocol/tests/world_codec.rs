//! Portable JSON codec coverage for the logical-first world boundary (v3):
//! DTO validation, u64-as-string enforcement, encoder equality with the
//! core canonical writer, golden bootstrap bytes, and budget gates.
//! Offline only: no D-Bus, no KWin.

use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
};
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{WORLD_CONTRACT_VERSION, WorldReport};
use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES, decode_request, encode_reply,
};

/// Minimal observation-only request envelope (v3): screens plus facts only,
/// no native workspace concept.
fn request_json(
    adapter: &str,
    generation: &str,
    obs: &str,
    req_seq: &str,
    req: &str,
    facts: &str,
) -> String {
    format!(
        "{{\"v\":3,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"{req}\",\"cmdSeq\":null,\
         \"command\":null,\
         \"screens\":[{{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
          \"complete\":true,\"scale\":1.0,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
         \"windows\":[{facts}],\
          \"views\":[{{\"out\":\"out\",\"view\":\"\"}}],\"closed\":[],\"native\":[],\
         \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"out\"],\
          \"focused\":\"\",\"complete\":true,\"gesture\":false,\"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}}}}"
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
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &fact_json("win-a", 0),
    )
}

fn connected_engine(generation: &str, caps: HostCapabilities) -> WorldEngine {
    let mut engine = WorldEngine::new(generation).expect("generation");
    // The caller owns availability: the synthetic test identity carries
    // exactly the facts the given capabilities support, and the
    // capabilities pass through unmodified (never silently extended).
    let unavailable = &caps.unavailable_facts;
    let handshake = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: if unavailable.contains(&tiler_core::connection::FactName::Executable) {
                None
            } else {
                Some("test-own-executable".to_owned())
            },
            process_id: if unavailable.contains(&tiler_core::connection::FactName::ProcessId) {
                None
            } else {
                Some("test-own-pid".to_owned())
            },
        },
        capabilities: caps,
    });
    let bound_generation = handshake.generation.expect("handshake accepted");
    engine
        .handle_settings(&SettingsRequest {
            adapter: "adapter-1".to_owned(),
            generation: bound_generation,
            request: "settings-1".to_owned(),
            revision: 0,
            values: Vec::new(),
        })
        .expect("settings accepted");
    engine
}

#[test]
fn decode_minimal_observation_only_request() {
    let report: WorldReport = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.contract_version, WORLD_CONTRACT_VERSION);
    assert_eq!(report.contract_version, 3);
    assert_eq!(report.observation_seq, 1);
    assert_eq!(report.request_seq, 1);
    assert_eq!(report.screens.len(), 1);
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].window.0, "win-a");
    assert_eq!(report.windows[0].output.0, "out");
    assert!(report.views.len() == 1 && report.views[0].view.is_empty());
    assert!(report.closed.is_empty());
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
    for version in [1, 2, 4] {
        let wire = one_window_request().replace("\"v\":3", &format!("\"v\":{version}"));
        let error = decode_request(&wire).expect_err("wrong version refuses");
        assert_eq!(error.kind, "unsupported-version");
    }
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["command"] = serde_json::json!({"op": "select-trailing", "out": "out"});
    let error = decode_request(&value.to_string()).expect_err("unpaired command refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    for retired in [
        "owner",
        "mode",
        "hostOwned",
        "equalCount",
        "birthClear",
        "predicate",
        "refill",
        "anchorFallback",
        "noSpaceResult",
        "bornMaximized",
        "switchTarget",
        "freshPlacement",
        "focusFollowsCursor",
        "cursorWarp",
        "defaultTiled",
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value[retired] = serde_json::Value::Null;
        let error = decode_request(&value.to_string()).expect_err("retired field refuses");
        assert_eq!(error.kind, "unknown-field", "{retired}");
    }
}

#[test]
fn native_observations_decode_without_report_owned_capability() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
    let report = decode_request(&value.to_string())
        .expect("fact shape decodes independently of capabilities");
    assert_eq!(report.native.len(), 1);
    assert_eq!(report.native[0].current_native.as_deref(), Some("ws"));
}

#[test]
fn report_owned_birth_clear_field_is_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["birthClear"] = serde_json::Value::Bool(true);
    let error = decode_request(&value.to_string()).expect_err("retired policy key refuses");
    assert_eq!(error.kind, "unknown-field");
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
            serde_json::json!({"win": "w", "source": {"explicit": "o"}, "delta": 1, "follow": false}),
        ),
        (
            "send-numbered",
            serde_json::json!({"win": "w", "source": {"explicit": "o"}, "index": 0, "follow": true}),
        ),
        (
            "set-workspace-tiling",
            serde_json::json!({"out": "o", "ws": "ws-1", "tiled": false}),
        ),
        (
            "toggle-workspace-tiling",
            serde_json::json!({"out": "o", "ws": "ws-1"}),
        ),
        (
            "migrate-workspace",
            serde_json::json!({"from": "a", "to": "b", "ws": "ws-1"}),
        ),
        (
            "focus-directional",
            serde_json::json!({"win": "w", "direction": "up"}),
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
            serde_json::json!({"direction": "right"}),
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
    let bad_send_numbered = serde_json::json!({"op": "send-numbered", "win": "w", "source": {"explicit": "o"}, "index": 10, "follow": false});
    assert!(decode_request(&with_command(&bad_send_numbered, "7")).is_err());
    let bad_migrate =
        serde_json::json!({"op": "migrate-workspace", "from": "a", "to": "a", "ws": "ws-1"});
    assert!(decode_request(&with_command(&bad_migrate, "7")).is_err());
    let bad_same_axis = serde_json::json!({"op": "move-directional", "win": "w", "direction": "down", "sameAxis": "sideways"});
    assert!(decode_request(&with_command(&bad_same_axis, "7")).is_err());
    let bad_unknown = serde_json::json!({"op": "frobnicate", "out": "o"});
    assert!(decode_request(&with_command(&bad_unknown, "7")).is_err());
    // The neutral toggle carries no preselected mode: `tiled` refuses.
    let bad_toggle_extra = serde_json::json!({"op": "toggle-workspace-tiling", "out": "o", "ws": "ws-1", "tiled": true});
    assert!(decode_request(&with_command(&bad_toggle_extra, "7")).is_err());
    // A null selection output resolves in core: it decodes to `None`,
    // never to an invented output.
    for (op, extra) in [
        (
            "select-numbered",
            serde_json::json!({"out": null, "index": 9}),
        ),
        (
            "select-relative",
            serde_json::json!({"out": null, "delta": -1}),
        ),
        ("select-previous", serde_json::json!({"out": null})),
        ("select-trailing", serde_json::json!({"out": null})),
    ] {
        let mut map = serde_json::Map::new();
        map.insert("op".to_owned(), serde_json::Value::String(op.to_owned()));
        for (key, value) in extra.as_object().expect("object") {
            map.insert(key.clone(), value.clone());
        }
        let command = serde_json::Value::Object(map);
        let report = decode_request(&with_command(&command, "7")).expect("null out decodes");
        let resolved = match report.command {
            Some(C::SelectNumbered { output, .. })
            | Some(C::SelectRelative { output, .. })
            | Some(C::SelectPrevious { output })
            | Some(C::SelectTrailing { output }) => output,
            _ => panic!("{op} decoded"),
        };
        assert_eq!(resolved, None, "{op}");
    }
    // A direction-only migration carries no source: the core resolves
    // the action source and its current view from the neutral report
    // facts, never from an adapter-preselected output.
    let migrate = serde_json::json!({"op": "migrate-directional", "direction": "right"});
    let report = decode_request(&with_command(&migrate, "7")).expect("direction decodes");
    match report.command {
        Some(C::MigrateDirectional { direction }) => {
            assert_eq!(direction, "right");
        }
        _ => panic!("migrate-directional decoded"),
    }
    // Legacy source/workspace legs are deleted and refuse as unknown.
    let legacy_migrate = serde_json::json!({"op": "migrate-directional", "from": "a", "ws": "ws-1", "direction": "right"});
    assert!(decode_request(&with_command(&legacy_migrate, "7")).is_err());
    // The neutral tiling toggle names the logical scope only; the mode
    // resolves in core at execution time.
    let toggle = serde_json::json!({"op": "toggle-workspace-tiling", "out": "o", "ws": "ws-1"});
    let report = decode_request(&with_command(&toggle, "7")).expect("toggle decodes");
    match report.command {
        Some(C::ToggleWorkspaceTiling { output, workspace }) => {
            assert_eq!(output.0, "o");
            assert_eq!(workspace, "ws-1");
        }
        _ => panic!("toggle-workspace-tiling decoded"),
    }
    // A send source carries an explicit output or a named implicit
    // fallback; legacy string/null `out` shapes refuse as malformed.
    use tiler_core::world_report::SendSource as S;
    for (op, extra, expect_implicit) in [
        (
            "send-numbered",
            serde_json::json!({"win": "w", "source": {"explicit": "o"}, "index": 9, "follow": false}),
            None,
        ),
        (
            "send-numbered",
            serde_json::json!({"win": "w", "source": {"implicit": "home-then-action"}, "index": 9, "follow": false}),
            Some("home-then-action"),
        ),
        (
            "send-relative",
            serde_json::json!({"win": "w", "source": {"implicit": "home-then-action"}, "delta": -1, "follow": true}),
            Some("home-then-action"),
        ),
    ] {
        let mut map = serde_json::Map::new();
        map.insert("op".to_owned(), serde_json::Value::String(op.to_owned()));
        for (key, value) in extra.as_object().expect("object") {
            map.insert(key.clone(), value.clone());
        }
        let command = serde_json::Value::Object(map);
        let report = decode_request(&with_command(&command, "7")).expect("source decodes");
        match (report.command, expect_implicit) {
            (
                Some(C::SendNumbered {
                    source: S::Explicit(out),
                    ..
                })
                | Some(C::SendRelative {
                    source: S::Explicit(out),
                    ..
                }),
                None,
            ) => assert_eq!(out.0, "o", "{op}"),
            (
                Some(C::SendNumbered {
                    source: S::HomeThenAction,
                    ..
                })
                | Some(C::SendRelative {
                    source: S::HomeThenAction,
                    ..
                }),
                Some("home-then-action"),
            ) => {}
            (command, _) => panic!("{op} decoded {command:?}"),
        }
    }
    for bad in [
        serde_json::json!({"op": "send-numbered", "win": "w", "source": "o", "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": null, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"explicit": ""}, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"implicit": "bogus"}, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"implicit": "home-then-cached-live"}, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"explicit": "o", "implicit": "home-then-action"}, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "index": 9, "follow": false}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err());
    }
}

#[test]
fn action_context_facts_decode_strictly() {
    // The envelope carries the required neutral facts: null legs read as
    // unknown, the ordered registry rides in adapter order.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.action_foreground, None);
    assert_eq!(report.action_cached, None);
    assert_eq!(
        report.action_outputs,
        vec![tiler_core::directional::OutputId("out".to_owned())]
    );
    // Carried outputs decode to wire ids.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["actionForeground"] = serde_json::Value::String("out".to_owned());
    value["actionCached"] = serde_json::Value::String("out".to_owned());
    value["actionOutputs"] = serde_json::json!(["out", "out-b"]);
    let report = decode_request(&value.to_string()).expect("facts decode");
    assert_eq!(
        report.action_foreground.map(|out| out.0),
        Some("out".to_owned())
    );
    assert_eq!(
        report.action_cached.map(|out| out.0),
        Some("out".to_owned())
    );
    assert_eq!(report.action_outputs.len(), 2);
    // Empty strings refuse: unknown rides `null`, never a blank id.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["actionForeground"] = serde_json::Value::String(String::new());
    assert!(decode_request(&value.to_string()).is_err());
    // Missing keys refuse: the contract requires the neutral facts.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("actionOutputs");
    assert!(decode_request(&value.to_string()).is_err());
    // Duplicate registry entries refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["actionOutputs"] = serde_json::json!(["out", "out"]);
    assert!(decode_request(&value.to_string()).is_err());
}

fn with_command(command: &serde_json::Value, seq: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["command"] = command.clone();
    value["cmdSeq"] = serde_json::Value::String(seq.to_owned());
    value.to_string()
}

#[test]
fn report_owned_born_maximized_setting_is_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["bornMaximized"] = serde_json::json!("keep-maximized-overlay");
    assert_eq!(
        decode_request(&value.to_string())
            .expect_err("retired setting refuses")
            .kind,
        "unknown-field"
    );
}

#[test]
fn report_owned_capacity_settings_are_rejected() {
    for key in ["anchorFallback", "noSpaceResult"] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value[key] = serde_json::json!("no-search");
        assert_eq!(
            decode_request(&value.to_string())
                .expect_err("retired setting refuses")
                .kind,
            "unknown-field"
        );
    }
}

#[test]
fn cursor_fact_decodes_independently_of_removed_policy_fields() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cursor"] = serde_json::json!([100, 200]);
    let report = decode_request(&value.to_string()).expect("cursor decodes");
    assert_eq!(report.cursor, Some((100, 200)));
    for cursor in [
        serde_json::json!([1]),
        serde_json::json!([1, 2, 3]),
        serde_json::json!("here"),
        serde_json::json!([1.5, 2]),
        serde_json::json!([20000, 0]),
        serde_json::json!([0, -20000]),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cursor"] = cursor;
        assert!(decode_request(&value.to_string()).is_err());
    }
}

#[test]
fn cursor_hit_decodes_tolerantly_but_validates_strictly() {
    // Missing backfills null: adapters predating the hit fact keep
    // decoding, and the core reads absence as no hover.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.cursor_hit, None);
    // Present hit decodes to the named lifetime.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cursor"] = serde_json::json!([100, 200]);
    value["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let report = decode_request(&value.to_string()).expect("hit decodes");
    assert_eq!(
        report.cursor_hit,
        Some(tiler_core::directional::WindowId("win-a".to_owned()))
    );
    // Explicit null and empty string both backfill to no hit.
    for hit in [
        serde_json::Value::Null,
        serde_json::Value::String(String::new()),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cursor"] = serde_json::json!([100, 200]);
        value["cursorHit"] = hit;
        let report = decode_request(&value.to_string()).expect("null hit decodes");
        assert_eq!(report.cursor_hit, None);
    }
    // Non-id shapes refuse; a hit naming no reported window refuses at
    // the core validation gate.
    for hit in [
        serde_json::json!(7),
        serde_json::json!("not an id!"),
        serde_json::Value::String("win-ghost".to_owned()),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["cursor"] = serde_json::json!([100, 200]);
        value["cursorHit"] = hit;
        assert!(decode_request(&value.to_string()).is_err());
    }
    // A hit without the cursor it was resolved from refuses.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    assert!(decode_request(&value.to_string()).is_err());
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
    let empty = request_json("adapter-1", "gen-1", "1", "1", "req-1", "");
    let report = decode_request(&empty).expect("empty decodes");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(estimate >= empty.len());
    // Carried action-context facts stay bounded too.
    let mut full: serde_json::Value = serde_json::from_str(&compact).expect("valid");
    full["actionForeground"] = serde_json::Value::String("out".to_owned());
    full["actionCached"] = serde_json::Value::String("out".to_owned());
    let wire = full.to_string();
    let report = decode_request(&wire).expect("facts decode");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    // A populated presentation ingress stays bounded too.
    let mut full: serde_json::Value = serde_json::from_str(&compact).expect("valid");
    full["presentation"] = serde_json::json!({
        "foreground": "win-a", "minimized": false, "maximized": false,
        "fullscreen": false, "cloaked": false, "hiddenWorkspace": false, "shell": false,
        "frame": [0, 0, 200, 200], "borderEnabled": true, "underlayEnabled": true,
        "borderGap": 2, "borderWidth": 4, "underlayExtension": 3,
        "chordHeld": true, "moveWindow": "win-a", "paused": false,
        "pagerOrder": ["win-a"],
        "pagerFrames": [{"win": "win-a", "live": [0, 0, 200, 200], "float": null, "tile": [0, 0, 200, 200]}],
    });
    let wire = full.to_string();
    let report = decode_request(&wire).expect("presentation decodes");
    assert_eq!(report.presentation.foreground, "win-a");
    assert_eq!(report.presentation.pager_order.len(), 1);
    assert_eq!(report.presentation.pager_frames.len(), 1);
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
}

#[test]
fn presentation_ingress_decodes_strictly() {
    // The disabled fixture rides an empty presentation ingress.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert!(report.presentation.foreground.is_empty());
    assert!(!report.presentation.border_enabled);
    assert!(!report.presentation.paused);
    assert!(report.presentation.pager_order.is_empty());
    // Missing presentation refuses: remote scripts must send it.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("presentation");
    assert!(decode_request(&value.to_string()).is_err());
    // Unknown fields refuse; unknown id tokens refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["extra"] = serde_json::Value::Bool(true);
    let error = decode_request(&value.to_string()).expect_err("unknown presentation field refuses");
    assert_eq!(error.kind, "unknown-field");
    for (key, replacement) in [
        ("foreground", serde_json::json!("not an id!!")),
        ("moveWindow", serde_json::json!("not an id!!")),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["presentation"][key] = replacement;
        let error = decode_request(&value.to_string()).expect_err("bad presentation id refuses");
        assert_eq!(error.kind, "unknown-value");
    }
    // Out-of-range style numbers and over-cap orders refuse at the core gate.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["borderGap"] = serde_json::json!(-1);
    let error = decode_request(&value.to_string()).expect_err("negative style refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    // Snapshot rows refuse unknown ids and non-rect frames.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["pagerFrames"] = serde_json::json!([{"win": "win-a", "live": [0, 0, 200, 200], "float": null, "tile": [0, 0, 200, 200]}]);
    let report = decode_request(&value.to_string()).expect("frames decode");
    assert_eq!(report.presentation.pager_frames.len(), 1);
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["pagerFrames"] =
        serde_json::json!([{"win": "win-a", "live": [0, 0, 0, 0], "float": null, "tile": null}]);
    assert!(decode_request(&value.to_string()).is_err());
    // Empty tokens mean none and stay valid.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["foreground"] = serde_json::Value::String(String::new());
    value["presentation"]["moveWindow"] = serde_json::Value::String(String::new());
    let report = decode_request(&value.to_string()).expect("empty tokens decode");
    assert!(report.presentation.foreground.is_empty());
    assert!(report.presentation.move_window.is_empty());
}

#[test]
fn frame_attempts_decode_tolerantly_and_stay_bounded() {
    use tiler_core::world_report::FrameSetter;
    // Absent keys backfill: predates-the-field adapters stay valid with an
    // unreported clock and no attempts.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.monotonic_ms, 0);
    assert!(report.frames.is_empty());
    // Present keys decode to the typed mechanical shape.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["monotonicMs"] = serde_json::Value::String("1200".to_owned());
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "accepted",
        "readback": [0, 0, 700, 560], "dpiBefore": 96, "dpiAfter": 120,
        "attemptMs": "1000", "op": "op-7", "pre": [0, 0, 800, 560],
    }]);
    let wire = value.to_string();
    let report = decode_request(&wire).expect("attempts decode");
    assert_eq!(report.monotonic_ms, 1200);
    assert_eq!(report.frames.len(), 1);
    let attempt = &report.frames[0];
    assert_eq!(attempt.window.0, "win-a");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.dpi_before, Some(96));
    assert_eq!(attempt.dpi_after, Some(120));
    assert_eq!(attempt.attempt_ms, 1000);
    assert_eq!(attempt.operation_id.as_deref(), Some("op-7"));
    assert_eq!(
        attempt.prewrite,
        Some(tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560
        })
    );
    // Ordinary writes carry an empty operation id and no prewrite.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "refused",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1001", "op": "", "pre": null,
    }]);
    let report = decode_request(&value.to_string()).expect("ordinary decodes");
    assert_eq!(report.frames[0].operation_id, None);
    assert_eq!(report.frames[0].prewrite, None);
    // A prewrite without an operation id refuses: only placement writes
    // carry a preimage.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "refused",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1001", "op": "", "pre": [0, 0, 800, 560],
    }]);
    let error = decode_request(&value.to_string()).expect_err("pre without op refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "frame-pre-without-operation");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    // A JSON number where the clock belongs refuses like every counter.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["monotonicMs"] = serde_json::json!(1200);
    assert!(decode_request(&value.to_string()).is_err());
    // Unknown setter vocabulary refuses; unknown attempt fields refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "clamped",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1000", "op": "", "pre": null,
    }]);
    let error = decode_request(&value.to_string()).expect_err("bad setter refuses");
    assert_eq!(error.kind, "unknown-value");
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "threw",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1000", "op": "", "pre": null, "verdict": "clamp",
    }]);
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    // Duplicate windows refuse with fixed vocabulary, never payload bytes.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([
        {"win": "win-a", "target": [0, 0, 800, 560], "setter": "refused",
         "readback": null, "dpiBefore": null, "dpiAfter": null, "attemptMs": "1000",
         "op": "", "pre": null},
        {"win": "win-a", "target": [0, 0, 800, 560], "setter": "refused",
         "readback": null, "dpiBefore": null, "dpiAfter": null, "attemptMs": "1001",
         "op": "", "pre": null},
    ]);
    let error = decode_request(&value.to_string()).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-frame");
}

#[test]
fn typed_attempts_decode_v4_nested_witnesses_and_reject_old_or_wrong_arms() {
    use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
    use tiler_core::ids::NativeWorkspaceId;
    use tiler_core::world_report::{FrameSetter, MemberAttempt, ViewAttempt, WorkspaceRef};
    use tiler_protocol::world_protocol::{decode_member_attempt, decode_view_attempt};

    let logical: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-member-logical.json"
    ))
    .expect("fixture JSON");
    let row = &logical["memberAttempts"][0];
    assert_eq!(
        decode_member_attempt(row).expect("logical member witness"),
        MemberAttempt {
            operation_id: "transfer-a".into(),
            window: WindowId("win-a".into()),
            target_output: OutputId("out-a".into()),
            target_workspace: "ws-a".into(),
            setter: FrameSetter::Accepted,
            pre: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".into()),
                workspace: WorkspaceId("ws-a".into()),
            }),
            post: Some(WorkspaceRef::Logical {
                output: OutputId("out-a".into()),
                workspace: WorkspaceId("ws-a".into()),
            }),
            attempt_ms: 20,
        }
    );

    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-member-native.json"
    ))
    .expect("fixture JSON");
    assert!(matches!(
        decode_member_attempt(&native["memberAttempts"][0]).expect("native member witness").pre,
        Some(WorkspaceRef::Native { ref output, ref workspace })
            if output == &OutputId("out-a".into()) && workspace.as_str() == "native-a"
    ));

    let view: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-view-native.json"
    ))
    .expect("fixture JSON");
    let row = &view["viewAttempts"][0];
    assert_eq!(
        decode_view_attempt(row).expect("native view witness"),
        ViewAttempt {
            operation_id: "view-a".into(),
            target_output: OutputId("out-a".into()),
            target_workspace: "ws-a".into(),
            setter: FrameSetter::Accepted,
            pre: Some(WorkspaceRef::Native {
                output: OutputId("out-a".into()),
                workspace: NativeWorkspaceId::parse("native-a").unwrap(),
            }),
            post: Some(WorkspaceRef::Native {
                output: OutputId("out-a".into()),
                workspace: NativeWorkspaceId::parse("native-a").unwrap(),
            }),
            baseline: Some(WorkspaceRef::Native {
                output: OutputId("out-a".into()),
                workspace: NativeWorkspaceId::parse("native-a").unwrap(),
            }),
            attempt_ms: 20,
        }
    );

    let logical_view = serde_json::json!({
        "op":"view-a", "out":"out-a", "ws":"ws-a", "setter":"accepted",
        "pre":{"kind":"logical","out":"out-a","ws":"ws-a"},
        "post":null, "base":null, "attemptMs":"20"
    });
    assert_eq!(
        decode_view_attempt(&logical_view).unwrap_err().kind,
        "unknown-value"
    );
    let parallel = serde_json::json!({
        "op":"view-a", "out":"out-a", "ws":"ws-a", "setter":"accepted",
        "pre":null, "post":null, "base":null, "nativePost":"native-a", "attemptMs":"20"
    });
    assert_eq!(
        decode_view_attempt(&parallel).unwrap_err().kind,
        "unknown-field"
    );
    let malformed_member = serde_json::json!({
        "op":"transfer-a", "win":"win-a", "out":"out-a", "ws":"ws-a", "setter":"accepted",
        "pre":{"kind":"native","out":"out-a","ws":"native-a","extra":true},
        "post":null, "attemptMs":"20"
    });
    assert_eq!(
        decode_member_attempt(&malformed_member).unwrap_err().kind,
        "unknown-field"
    );
}

#[test]
fn clear_attempts_decode_tolerantly_and_clear_op_round_trips() {
    use tiler_core::world_report::FrameSetter;
    // Absent key backfills: predates-the-field adapters stay valid with no
    // clear attempts.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert!(report.clear_attempts.is_empty());
    // Present keys decode to the typed mechanical shape: operation plus
    // window correlation, setter disposition, and nullable immediate
    // pre/post maximized proofs. No verdict rides the lane.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["clearAttempts"] = serde_json::json!([{
        "op": "op-13", "win": "win-a", "setter": "accepted",
        "pre": true, "post": false, "attemptMs": "2000",
    }]);
    let wire = value.to_string();
    let report = decode_request(&wire).expect("clear attempts decode");
    assert_eq!(report.clear_attempts.len(), 1);
    let attempt = &report.clear_attempts[0];
    assert_eq!(attempt.operation_id, "op-13");
    assert_eq!(attempt.window.0, "win-a");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(false));
    assert_eq!(attempt.attempt_ms, 2000);
    // Null proofs stay unknown; duplicates refuse with fixed vocabulary.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["clearAttempts"] = serde_json::json!([{
        "op": "op-13", "win": "win-a", "setter": "threw",
        "pre": null, "post": null, "attemptMs": "1",
    }]);
    let null_report = decode_request(&value.to_string()).expect("null proofs decode");
    assert_eq!(null_report.clear_attempts[0].pre_maximized, None);
    assert_eq!(null_report.clear_attempts[0].post_maximized, None);
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["clearAttempts"] = serde_json::json!([
        {"op": "op-9", "win": "win-a", "setter": "accepted",
         "pre": true, "post": false, "attemptMs": "1"},
        {"op": "op-9", "win": "win-a", "setter": "accepted",
         "pre": true, "post": false, "attemptMs": "2"},
    ]);
    let error = decode_request(&value.to_string()).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-clear-attempt");
    // A maximized move through the engine publishes the one-shot clear
    // desire under the originating op, and both writers encode it.
    let maximized_fact = "{\"win\":\"win-a\",\"out\":\"out\",\"rect\":[100,0,200,200],\
          \"floating\":true,\"fitExcluded\":false,\"fullscreen\":false,\
          \"maximized\":true,\"sticky\":false,\"fixedAuto\":false,\
          \"fixedSuppress\":false,\"minimized\":false,\"observedWs\":null,
          \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}";
    let mut value: serde_json::Value = serde_json::from_str(&request_json(
        "adapter-1",
        "gen-1",
        "2",
        "2",
        "req-2",
        maximized_fact,
    ))
    .expect("valid");
    value["focused"] = serde_json::json!("win-a");
    value["command"] = serde_json::json!({"op": "move-directional", "win": "win-a", "direction": "right", "sameAxis": "group-with-neighbor"});
    value["cmdSeq"] = serde_json::Value::String("13".to_owned());
    let report = decode_request(&value.to_string()).expect("maximized command decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover keeps a row");
    assert_eq!(row.clear_operation.as_deref(), Some("op-13"));
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical, "transport mirrors the core writer");
    assert!(
        wire.contains("\"clear_op\":\"op-13\""),
        "clear desire round-trips, got {wire}"
    );
    // Estimates bound the wider wire shape.
    let request_estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        request_estimate >= value.to_string().len(),
        "estimate {request_estimate} bounds wire {}",
        value.to_string().len()
    );
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
fn empty_view_attempt_refs_are_rejected() {
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "threw",
        "pre": "", "post": "", "base": "", "attemptMs": "1",
    }]);
    let error = decode_request(&value.to_string()).expect_err("empty typed refs reject");
    assert_eq!(error.kind, "malformed");
}

#[test]
fn reply_encode_matches_core_canonical_writer() {
    // Drive the engine once, then lock the transport encoding to the core
    // canonical writer over the same reply.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
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
    assert_eq!(parsed.v, 3);
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
fn reply_gesture_settled_matches_core_sizing() {
    // A reply carrying a gesture terminal encodes identically on both
    // writers, and the structural estimate bounds the exact bytes.
    let mut reply = WorldEngine::new("gen-1")
        .expect("generation")
        .bootstrap_reply("adapter-1", "req-1");
    reply.gesture_settled = Some(tiler_core::gesture::GestureSettled {
        id: 7,
        committed: true,
    });
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical);
    assert!(wire.contains("\"gestureSettled\":{\"id\":\"7\",\"committed\":true}"));
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn reply_warp_cursor_matches_core_sizing() {
    // A reply carrying a desired cursor-warp point encodes identically on
    // both writers, and the structural estimate bounds the exact bytes.
    let mut reply = WorldEngine::new("gen-1")
        .expect("generation")
        .bootstrap_reply("adapter-1", "req-1");
    reply.cursor_warp = Some((810, 300));
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical);
    assert!(wire.contains("\"warpCursor\":[810,300]"));
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn native_current_null_strict_and_creation_attempts_decode() {
    // Strict codec: `current: null` means not an ordered desktop; empty
    // strings and missing keys refuse (no compatibility tolerance).
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": null, "occupancy": [{"native": "ws", "count": null}]}]);
    let report = decode_request(&value.to_string()).expect("null current decodes");
    assert_eq!(report.native[0].current_native, None);
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "", "occupancy": [{"native": "ws", "count": null}]}]);
    assert!(decode_request(&value.to_string()).is_err());
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "occupancy": [{"native": "ws", "count": null}]}]);
    assert!(decode_request(&value.to_string()).is_err());
    // Creation attempts decode tolerantly (missing backfills) and strictly
    // when present; the estimate bounds the wire.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert!(report.creation_attempts.is_empty());
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
    value["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": ["ws"], "after": ["ws", "ws-new"], "attemptMs": "5",
    }]);
    let wire = value.to_string();
    let report = decode_request(&wire).expect("creation decodes");
    assert_eq!(report.creation_attempts.len(), 1);
    assert_eq!(report.creation_attempts[0].operation_id, "op-native-1");
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(estimate >= wire.len());
    // Without the capability, creation attempts decode as facts but refuse
    // at Engine ingress.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": [], "after": ["ws"], "attemptMs": "5",
    }]);
    let report = decode_request(&value.to_string()).expect("creation fact decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    assert!(
        matches!(engine.handle_report(&report), WorldOutcome::Rejected { message, .. } if message == "native-without-capability")
    );
}

#[test]
fn golden_bootstrap_reply_bytes_lock_wire_shape() {
    // The discovery reply is fixed shape: v3, ready false, empty sections.
    let engine = WorldEngine::new("gen-1").expect("generation");
    let reply = engine.bootstrap_reply("adapter-1", "req-1");
    let wire = encode_reply(&reply);
    assert_eq!(
        wire,
        "{\"v\":3,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"req\":\"req-1\",\
         \"obs\":\"0\",\"rev\":\"0\",\"ready\":false,\"consumed\":false,\
         \"scopes\":[],\"workspaces\":[],\"outputs\":[],\"windows\":[],\
         \"transfers\":[],\"effects\":[],\"result\":null,\"gestureSettled\":null,\"operationSettled\":[],\"nativeCreations\":[],\"warpCursor\":null,\
         \"presentation\":{\"foreground\":\"\",\"border\":{\"visible\":false,\"reason\":\"no-target\",\"inner\":null,\"outer\":null},\
         \"underlay\":{\"status\":\"hidden\",\"reason\":\"no-group\",\"baseRev\":null},\"pager\":[]},\"diagnostics\":[]}"
    );
}

#[test]
fn reply_operation_settled_matches_core_sizing() {
    // A reply carrying terminal retained-operation events encodes
    // identically on both writers, and the structural estimate bounds the
    // exact bytes. Pending desire rides the window row, actual attempts
    // ride the world-apply diag, holds emit nothing, a clear continuation
    // means the intent resumed (never that the final frame arrived), and
    // only a float frame-proven event means observation confirmed the
    // float target.
    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{OperationEvent, OperationOutcome};
    let mut reply = WorldEngine::new("gen-1")
        .expect("generation")
        .bootstrap_reply("adapter-1", "req-1");
    reply.operation_settled = vec![
        OperationEvent {
            operation_id: "op-13".to_owned(),
            window: WindowId("win-max".to_owned()),
            outcome: OperationOutcome::ClearContinuedTiled,
            cause: None,
        },
        OperationEvent {
            operation_id: "op-14".to_owned(),
            window: WindowId("win-other".to_owned()),
            outcome: OperationOutcome::ClearContinuationRefused,
            cause: Some("constraints".to_owned()),
        },
        OperationEvent {
            operation_id: "op-15".to_owned(),
            window: WindowId("win-float".to_owned()),
            outcome: OperationOutcome::FloatFrameProven,
            cause: None,
        },
        OperationEvent {
            operation_id: "op-16".to_owned(),
            window: WindowId("win-gone".to_owned()),
            outcome: OperationOutcome::FloatAbandoned,
            cause: Some("closed".to_owned()),
        },
    ];
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical);
    assert!(wire.contains("\"operationSettled\":["));
    assert!(wire.contains("\"outcome\":\"continued-tiled\""));
    assert!(wire.contains("\"outcome\":\"continuation-refused\""));
    assert!(wire.contains("\"outcome\":\"float-frame-proven\""));
    assert!(wire.contains("\"outcome\":\"float-abandoned\""));
    assert!(wire.contains("\"cause\":\"constraints\""));
    assert!(wire.contains("\"cause\":\"closed\""));
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
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
         "complete": true, "scale": 1.0,
         "identity": {"manufacturer": null, "model": null, "serial": null, "connector": null}},
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

#[test]
fn gesture_ingress_decodes_phases_and_refuses_unknown_tokens() {
    use tiler_core::gesture::{GestureKind, GesturePhase};
    // Tolerant missing backfills observation-only.
    let plain = decode_request(&one_window_request()).expect("decodes");
    assert!(plain.gesture.is_none());
    // Explicit null backfills the same way.
    let mut null_value: serde_json::Value =
        serde_json::from_str(&one_window_request()).expect("valid");
    null_value["gestureIngress"] = serde_json::Value::Null;
    assert!(
        decode_request(&null_value.to_string())
            .expect("null decodes")
            .gesture
            .is_none()
    );
    // A present ingress carries the normalized lifecycle facts, including
    // the stable nonzero id and the START down point.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["gestureIngress"] = serde_json::json!({
        "win": "win-a", "kind": "move", "phase": "updated", "id": "7",
        "cursor": [150, 50], "startCursor": [10, 10],
        "esc": false, "modal": true,
        "start": [0, 0, 40, 90],
    });
    let report = decode_request(&value.to_string()).expect("decodes");
    let gesture = report.gesture.as_ref().expect("gesture rides");
    assert_eq!(gesture.window.0, "win-a");
    assert_eq!(gesture.id, 7);
    assert_eq!(gesture.kind, GestureKind::Move);
    assert_eq!(gesture.phase, GesturePhase::Updated);
    assert_eq!(gesture.cursor, Some((150, 50)));
    assert_eq!(gesture.start_cursor, Some((10, 10)));
    assert!(!gesture.esc);
    assert!(gesture.modal_move);
    // Cursor-less updates (game pause) ride as null.
    let mut cursorless: serde_json::Value =
        serde_json::from_str(&value.to_string()).expect("valid");
    cursorless["gestureIngress"]["cursor"] = serde_json::Value::Null;
    assert!(
        decode_request(&cursorless.to_string())
            .expect("decodes")
            .gesture
            .expect("gesture rides")
            .cursor
            .is_none()
    );
    // An unreadable down point rides as null.
    let mut nocursor: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    nocursor["gestureIngress"]["startCursor"] = serde_json::Value::Null;
    assert!(
        decode_request(&nocursor.to_string())
            .expect("decodes")
            .gesture
            .expect("gesture rides")
            .start_cursor
            .is_none()
    );
    // Zero and non-decimal ids refuse: the id space starts at one.
    let mut zero_id: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    zero_id["gestureIngress"]["id"] = serde_json::json!("0");
    assert!(decode_request(&zero_id.to_string()).is_err());
    let mut bad_id: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    bad_id["gestureIngress"]["id"] = serde_json::json!(7);
    assert!(decode_request(&bad_id.to_string()).is_err());
    // Missing id or down-point keys refuse (exact shape, no backfill).
    let mut missing_id: serde_json::Value =
        serde_json::from_str(&value.to_string()).expect("valid");
    missing_id["gestureIngress"]
        .as_object_mut()
        .expect("object")
        .remove("id");
    assert!(decode_request(&missing_id.to_string()).is_err());
    let mut missing_down: serde_json::Value =
        serde_json::from_str(&value.to_string()).expect("valid");
    missing_down["gestureIngress"]
        .as_object_mut()
        .expect("object")
        .remove("startCursor");
    assert!(decode_request(&missing_down.to_string()).is_err());
    // Unknown kind/phase tokens refuse with fixed vocabulary.
    let mut bad_kind: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    bad_kind["gestureIngress"]["kind"] = serde_json::json!("drag");
    assert!(decode_request(&bad_kind.to_string()).is_err());
    let mut bad_phase: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    bad_phase["gestureIngress"]["phase"] = serde_json::json!("dropped");
    assert!(decode_request(&bad_phase.to_string()).is_err());
    // Degenerate start frames and out-of-bound cursors refuse.
    let mut bad_rect: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    bad_rect["gestureIngress"]["start"] = serde_json::json!([0, 0, 0, 90]);
    assert!(decode_request(&bad_rect.to_string()).is_err());
    let mut bad_cursor: serde_json::Value =
        serde_json::from_str(&value.to_string()).expect("valid");
    bad_cursor["gestureIngress"]["cursor"] = serde_json::json!([99999999, 50]);
    assert!(decode_request(&bad_cursor.to_string()).is_err());
    // Unknown window ids refuse.
    let mut bad_win: serde_json::Value = serde_json::from_str(&value.to_string()).expect("valid");
    bad_win["gestureIngress"]["win"] = serde_json::json!("not an id!");
    assert!(decode_request(&bad_win.to_string()).is_err());
    // The estimate still bounds the wider gesture wire shape.
    let wire = value.to_string();
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
}

#[test]
fn native_ws_field_decodes_and_gates() {
    // Missing nativeWs backfills unknown (native-free reports predate it).
    let report = decode_request(&one_window_request()).expect("missing backfills");
    assert_eq!(report.windows[0].native_workspace, None);
    let with_native = |native: serde_json::Value| {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["native"] = serde_json::json!([{"out": "out", "order": ["d1"], "current": "d1", "occupancy": [{"native": "d1", "count": null}]}]);
        value["windows"] = serde_json::json!([{
            "win": "win-a", "out": "out", "rect": [0, 0, 200, 200],
            "floating": false, "fitExcluded": false, "fullscreen": false,
            "maximized": false, "sticky": false, "fixedAuto": false,
            "fixedSuppress": false, "minimized": false, "observedWs": null,
            "nativeWs": native,
            "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
        }]);
        decode_request(&value.to_string())
    };
    // Null and empty mean unknown, never proof of absence.
    let report = with_native(serde_json::Value::Null).expect("null decodes");
    assert_eq!(report.windows[0].native_workspace, None);
    let report = with_native(serde_json::Value::String(String::new())).expect("empty decodes");
    assert_eq!(report.windows[0].native_workspace, None);
    // A valid opaque native id decodes.
    let report = with_native(serde_json::Value::String("d1".to_owned())).expect("id decodes");
    assert_eq!(report.windows[0].native_workspace.as_deref(), Some("d1"));
    // Non-id native membership refuses as unknown-value.
    let error = with_native(serde_json::Value::String("not an id!".to_owned()))
        .expect_err("bad id refuses");
    assert_eq!(error.kind, "unknown-value");
    // Non-string native membership is malformed.
    let error = with_native(serde_json::json!(7)).expect_err("number refuses");
    assert_eq!(error.kind, "malformed");
    // Capability off with native evidence refuses at the core gate.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"] = serde_json::json!([{
        "win": "win-a", "out": "out", "rect": [0, 0, 200, 200],
        "floating": false, "fitExcluded": false, "fullscreen": false,
        "maximized": false, "sticky": false, "fixedAuto": false,
        "fixedSuppress": false, "minimized": false, "observedWs": null,
        "nativeWs": "d1",
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
    }]);
    let report = decode_request(&value.to_string()).expect("native fact decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    assert!(
        matches!(engine.handle_report(&report), WorldOutcome::Rejected { message, .. } if message == "native-without-capability")
    );
}

#[test]
fn native_ws_estimate_bounds_actual_wire() {
    // The structural estimate accounts for the nativeWs field: it bounds
    // a wire message carrying a realistic Space-style id.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["9f1c-0005-space"], "current": "9f1c-0005-space", "occupancy": [{"native": "9f1c-0005-space", "count": null}]}]);
    value["windows"] = serde_json::json!([{
        "win": "win-a", "out": "out", "rect": [0, 0, 200, 200],
        "floating": false, "fitExcluded": false, "fullscreen": false,
        "maximized": false, "sticky": false, "fixedAuto": false,
        "fixedSuppress": false, "minimized": false, "observedWs": null,
        "nativeWs": "9f1c-0005-space",
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
    }]);
    let wire = value.to_string();
    assert!(wire.contains("\"nativeWs\":\"9f1c-0005-space\""));
    let report = decode_request(&wire).expect("native report decodes");
    assert_eq!(
        report.windows[0].native_workspace.as_deref(),
        Some("9f1c-0005-space")
    );
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
}

#[test]
fn creation_created_receipt_decodes_tolerantly_and_bounds() {
    // The correlated receipt is optional: missing, null, and empty all
    // backfill to uncorrelated, matching adapters predating the field.
    // A present opaque id decodes; non-id and non-string shapes refuse.
    let with_created = |created: serde_json::Value| {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
        let mut attempt = serde_json::json!({
            "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
            "before": ["ws"], "after": ["ws"], "attemptMs": "5",
        });
        attempt["created"] = created;
        value["creationAttempts"] = serde_json::json!([attempt]);
        (value.to_string(), decode_request(&value.to_string()))
    };
    let (wire, report) = with_created(serde_json::Value::String("ws-new".to_owned()));
    let report = report.expect("created decodes");
    assert_eq!(
        report.creation_attempts[0].created_native.as_deref(),
        Some("ws-new")
    );
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    for absent in [
        serde_json::Value::Null,
        serde_json::Value::String(String::new()),
    ] {
        let (_, report) = with_created(absent);
        assert_eq!(
            report.expect("absent backfills").creation_attempts[0].created_native,
            None
        );
    }
    // A missing key also backfills: predates-the-field adapters stay valid.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
    value["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": ["ws"], "after": ["ws"], "attemptMs": "5",
    }]);
    let report = decode_request(&value.to_string()).expect("missing backfills");
    assert_eq!(report.creation_attempts[0].created_native, None);
    // Non-id and non-string receipts refuse with fixed vocabulary.
    for bad in [
        serde_json::json!("not an id!"),
        serde_json::json!(7),
        serde_json::json!(["ws-new"]),
    ] {
        let (wire, _) = with_created(bad);
        assert!(decode_request(&wire).is_err(), "bad created refuses");
    }
    // Unknown receipt fields refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
    value["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": ["ws"], "after": ["ws"], "createdId": "ws-new", "attemptMs": "5",
    }]);
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
}

#[test]
fn focus_directional_empty_win_decodes_as_no_focus_marker() {
    // Explicit empty `win` is this command's no-focus marker: the key stays
    // required and string-typed, but the empty value decodes instead of
    // refusing. Every other windowed command still refuses it.
    use tiler_core::world_report::WorldCommand as C;
    let command = serde_json::json!({"op": "focus-directional", "win": "", "direction": "right"});
    let report = decode_request(&with_command(&command, "7")).expect("empty win decodes");
    assert!(matches!(
        report.command,
        Some(C::FocusDirectional { ref window, .. }) if window.0.is_empty()
    ));
    assert_eq!(report.command_seq, Some(7));
    for op in ["move-directional", "send-directional"] {
        let extra = if op.starts_with("move") {
            serde_json::json!({"win": "", "direction": "down", "sameAxis": "group-with-neighbor"})
        } else {
            serde_json::json!({"win": "", "direction": "left", "follow": true})
        };
        let mut map = serde_json::Map::new();
        map.insert("op".to_owned(), serde_json::Value::String(op.to_owned()));
        for (key, value) in extra.as_object().expect("object") {
            map.insert(key.clone(), value.clone());
        }
        assert!(
            decode_request(&with_command(&serde_json::Value::Object(map), "7")).is_err(),
            "{op} refuses empty win"
        );
    }
    // The key stays required and string-typed; blank-nonempty wins refuse.
    let missing = serde_json::json!({"op": "focus-directional", "direction": "right"});
    assert!(decode_request(&with_command(&missing, "7")).is_err());
    let null = serde_json::json!({"op": "focus-directional", "win": null, "direction": "right"});
    assert!(decode_request(&with_command(&null, "7")).is_err());
    let number = serde_json::json!({"op": "focus-directional", "win": 7, "direction": "right"});
    assert!(decode_request(&with_command(&number, "7")).is_err());
    let blank = serde_json::json!({"op": "focus-directional", "win": " ", "direction": "right"});
    assert!(decode_request(&with_command(&blank, "7")).is_err());
}

#[test]
fn focus_directional_no_focus_decodes_and_crosses_end_to_end() {
    // Two outputs, no focused window: the JSON no-focus marker decodes,
    // validates, and the engine crosses to the adjacent remembered
    // selection with one explicit focus effect, encoded like the core.
    fn two_output_envelope(seq: &str, req: &str, command: Option<serde_json::Value>) -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["screens"] = serde_json::json!([
            {"out": "out-a", "full": [0, 0, 800, 600], "work": [0, 0, 800, 560],
             "complete": true, "scale": 1.0,
             "identity": {"manufacturer": null, "model": null, "serial": null, "connector": null}},
            {"out": "out-b", "full": [800, 0, 800, 600], "work": [800, 0, 800, 560],
             "complete": true, "scale": 1.0,
             "identity": {"manufacturer": null, "model": null, "serial": null, "connector": null}},
        ]);
        value["windows"] = serde_json::json!([
            {"win": "win-a", "out": "out-a", "rect": [10, 0, 200, 200],
             "floating": false, "fitExcluded": false, "fullscreen": false,
             "maximized": false, "sticky": false, "fixedAuto": false,
             "fixedSuppress": false, "minimized": false, "observedWs": null,
             "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null}},
            {"win": "win-b", "out": "out-b", "rect": [810, 0, 200, 200],
             "floating": false, "fitExcluded": false, "fullscreen": false,
             "maximized": false, "sticky": false, "fixedAuto": false,
             "fixedSuppress": false, "minimized": false, "observedWs": null,
             "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null}},
        ]);
        value["actionForeground"] = serde_json::json!("out-a");
        value["actionOutputs"] = serde_json::json!(["out-a", "out-b"]);
        value["views"] = serde_json::json!([]);
        value["focused"] = serde_json::json!("");
        value["obs"] = serde_json::Value::String(seq.to_owned());
        value["reqSeq"] = serde_json::Value::String(seq.to_owned());
        value["req"] = serde_json::Value::String(req.to_owned());
        match command {
            Some(command) => {
                value["command"] = command;
                value["cmdSeq"] = serde_json::Value::String("11".to_owned());
            }
            None => {
                value["command"] = serde_json::Value::Null;
                value["cmdSeq"] = serde_json::Value::Null;
            }
        }
        value.to_string()
    }
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let seed = decode_request(&two_output_envelope("1", "req-1", None)).expect("seed decodes");
    match engine.handle_report(&seed) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted seed, got {other:?}"),
    }
    let report = decode_request(&two_output_envelope(
        "2",
        "req-2",
        Some(serde_json::json!({"op": "focus-directional", "win": "", "direction": "right"})),
    ))
    .expect("no-focus command decodes");
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let focus: Vec<&tiler_core::world_report::OneShotEffect> = reply
        .effects
        .iter()
        .filter(|effect| {
            effect.kind == tiler_core::world_report::EffectKind::Focus
                && effect.id.starts_with("fx-")
        })
        .collect();
    assert_eq!(focus.len(), 1, "one crossing effect: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "win-b");
    assert!(reply.command_result.is_none());
    let wire = encode_reply(&reply);
    assert_eq!(
        wire,
        tiler_core::world_report::encode_reply_canonical(&reply),
        "transport mirrors the core writer"
    );
}

#[test]
fn toggle_maximize_decodes_and_result_encodes() {
    // Wire toggle-maximize decodes to the typed command, and the focused
    // plain tile publishes the Maximize result through both writers with
    // exact result sizing. Malformed/missing win refuses.
    use tiler_core::world_report::CommandResult as R;
    use tiler_core::world_report::WorldCommand as C;
    let command = serde_json::json!({"op": "toggle-maximize", "win": "win-a"});
    let mut value: serde_json::Value =
        serde_json::from_str(&with_command(&command, "7")).expect("valid");
    value["focused"] = serde_json::Value::String("win-a".to_owned());
    let report = decode_request(&value.to_string()).expect("toggle-maximize decodes");
    assert!(
        matches!(report.command, Some(C::ToggleMaximize { ref window }) if window.0 == "win-a")
    );
    assert_eq!(report.command_seq, Some(7));
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    match &reply.command_result {
        Some(R::Maximize {
            window,
            maximized,
            clear_fullscreen,
            was_maximized,
        }) => {
            assert_eq!(window.0, "win-a");
            assert_eq!(
                (*maximized, *clear_fullscreen, *was_maximized),
                (true, false, false)
            );
        }
        other => panic!("expected maximize result, got {other:?}"),
    }
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical, "transport mirrors the core writer");
    assert!(
        wire.contains("\"op\":\"maximize\""),
        "maximize result round-trips, got {wire}"
    );
    let mut encoded = String::new();
    tiler_core::world_sizing::encode_command_result(&mut encoded, &reply.command_result);
    assert_eq!(
        tiler_core::world_sizing::command_result_estimate(&reply.command_result),
        encoded.len() + 10,
        "result estimate is exact: {encoded}"
    );
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
    let missing = serde_json::json!({"op": "toggle-maximize"});
    assert!(decode_request(&with_command(&missing, "7")).is_err());
    for bad in [
        serde_json::json!({"op": "toggle-maximize", "win": null}),
        serde_json::json!({"op": "toggle-maximize", "win": 7}),
        serde_json::json!({"op": "toggle-maximize", "win": ""}),
        serde_json::json!({"op": "toggle-maximize", "win": " "}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err());
    }
}

#[test]
fn reply_trailing_marker_encodes_on_both_writers() {
    // The explicit trailing marker rides only when true: both writers
    // emit `,"trailing":true` on marked plan and pager rows, omit the
    // key otherwise, and the structural estimate bounds the exact bytes.
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::{AuthorityScope, TilingMode};
    let mut reply = WorldEngine::new("gen-1")
        .expect("generation")
        .bootstrap_reply("adapter-1", "req-1");
    for (index, trailing) in [false, true].into_iter().enumerate() {
        reply
            .workspaces
            .push(tiler_core::world_report::WorkspaceDesire {
                id: format!("ws-{}", index + 1),
                output: OutputId("out".to_owned()),
                native_binding: None,
                scope: AuthorityScope::Output(OutputId("out".to_owned())),
                order: index,
                tiling_mode: TilingMode::Tiled,
                existence: true,
                hold: None,
                trailing,
            });
    }
    reply.presentation.pager = vec![tiler_core::world_presentation::PagerScreen {
        output: "out".to_owned(),
        workspaces: vec![
            tiler_core::world_presentation::PagerWorkspace {
                id: "ws-head".to_owned(),
                ordinal: 1,
                label: "Workspace 1".to_owned(),
                current: true,
                occupied: true,
                contains_focused: false,
                trailing: false,
                windows: Vec::new(),
            },
            tiler_core::world_presentation::PagerWorkspace {
                id: "ws-tail".to_owned(),
                ordinal: 2,
                label: "Workspace 2".to_owned(),
                current: false,
                occupied: false,
                contains_focused: false,
                trailing: true,
                windows: Vec::new(),
            },
        ],
    }];
    let wire = encode_reply(&reply);
    let canonical = tiler_core::world_report::encode_reply_canonical(&reply);
    assert_eq!(wire, canonical);
    assert_eq!(
        wire.matches(",\"trailing\":true").count(),
        2,
        "one plan plus one pager key"
    );
    assert!(!wire.contains("\"trailing\":false"));
    let estimate = tiler_core::world_report::reply_estimate(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn typed_scopes_match_v4_fixture_rows_and_reject_malformed_unions() {
    use tiler_core::directional::{OutputId, WorkspaceId};
    use tiler_core::world_report::{
        AuthorityScope, ScopeAuthority, ScopeStatus, TilingMode, WorkspaceDesire,
    };
    use tiler_protocol::world_protocol::{decode_authority_scope, encode_reply};

    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/reply-untouched-workspace-scope.json"
    ))
    .expect("fixture json");
    let mut reply = WorldEngine::new("generation-a")
        .expect("generation")
        .bootstrap_reply("adapter-a", "request-a");
    for (workspace, authority) in [
        ("ws-a", ScopeAuthority::Controlled),
        ("ws-spare", ScopeAuthority::Untouched),
    ] {
        let scope = AuthorityScope::Workspace {
            output: OutputId("out-a".into()),
            workspace: WorkspaceId(workspace.into()),
        };
        reply.scopes.push(ScopeStatus {
            scope: scope.clone(),
            authority,
        });
        reply.workspaces.push(WorkspaceDesire {
            id: workspace.into(),
            output: OutputId("out-a".into()),
            native_binding: None,
            scope: scope.clone(),
            order: usize::from(workspace == "ws-spare"),
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: workspace == "ws-spare",
        });
        assert_eq!(
            decode_authority_scope(&fixture["scopes"][reply.scopes.len() - 1]["scope"]),
            Ok(scope)
        );
    }
    let encoded: serde_json::Value =
        serde_json::from_str(&encode_reply(&reply)).expect("reply json");
    assert_eq!(encoded["scopes"], fixture["scopes"]);
    assert_eq!(encoded["workspaces"], fixture["workspaces"]);

    let output_fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/reply-observe.json"
    ))
    .expect("output scope fixture json");
    let output_scope = AuthorityScope::Output(OutputId("out-a".into()));
    assert_eq!(
        decode_authority_scope(&output_fixture["scopes"][0]["scope"]),
        Ok(output_scope.clone())
    );
    let mut output_reply = WorldEngine::new("generation-a")
        .expect("generation")
        .bootstrap_reply("adapter-a", "request-a");
    output_reply.scopes.push(ScopeStatus {
        scope: output_scope.clone(),
        authority: ScopeAuthority::Controlled,
    });
    for (workspace, order, trailing) in [("ws-a", 0, false), ("ws-spare", 1, true)] {
        output_reply.workspaces.push(WorkspaceDesire {
            id: workspace.into(),
            output: OutputId("out-a".into()),
            native_binding: None,
            scope: output_scope.clone(),
            order,
            tiling_mode: TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing,
        });
    }
    let output_encoded: serde_json::Value =
        serde_json::from_str(&encode_reply(&output_reply)).expect("output reply json");
    assert_eq!(output_encoded["scopes"], output_fixture["scopes"]);
    assert_eq!(output_encoded["workspaces"], output_fixture["workspaces"]);

    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let request = one_window_request()
        .replace("\"out\":\"out\"", "\"out\":\"out-a\"")
        .replace(
            "\"actionOutputs\":[\"out\"]",
            "\"actionOutputs\":[\"out-a\"]",
        );
    let report = decode_request(&request).expect("real report decode");
    let outcome = engine.handle_report(&report);
    let WorldOutcome::Accepted(published) = outcome else {
        panic!("real report was not applied: {outcome:?}");
    };
    assert_eq!(
        published.scopes,
        vec![ScopeStatus {
            scope: output_scope.clone(),
            authority: ScopeAuthority::Controlled,
        }]
    );
    assert!(
        published
            .workspaces
            .iter()
            .all(|workspace| workspace.scope == output_scope)
    );

    for invalid in [
        serde_json::json!({"kind":"other","out":"out-a"}),
        serde_json::json!({"kind":"output","out":"out-a","ws":"ws-a"}),
        serde_json::json!({"kind":"workspace","out":"out-a"}),
        serde_json::json!({"kind":"workspace","out":7,"ws":"ws-a"}),
        serde_json::json!({"kind":"output","out":"out-a","extra":true}),
        serde_json::json!("out-a/ws-a"),
    ] {
        assert!(
            decode_authority_scope(&invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn startup_ordering_end_to_end_decode_engine_encode() {
    // Full startup path through real layers: wire bytes decode, the same
    // live engine bootstraps then executes, and each reply encodes back to
    // wire bytes. Inline payload only; no fixture bytes involved.
    let wire_request = one_window_request();
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let handshake = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: None,
            process_id: None,
        },
        capabilities: HostCapabilities::without_native_identity(),
    });
    assert!(!handshake.ready);
    let generation = handshake.generation.expect("handshake accepted");

    // Pre-settings report decodes yet bootstraps: correlated, not ready,
    // no desired state, no effects.
    let pre_settings = decode_request(&wire_request).expect("decodes");
    let bootstrap = match engine.handle_report(&pre_settings) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected bootstrap, got {other:?}"),
    };
    assert!(!bootstrap.ready);
    assert!(bootstrap.scopes.is_empty());
    assert!(bootstrap.effects.is_empty());
    let bootstrap_wire = encode_reply(&bootstrap);
    assert!(bootstrap_wire.contains("\"ready\":false"));
    assert!(bootstrap_wire.contains("\"req\":\"req-1\""));

    // Settings acceptance latches nothing by itself; the same bytes then
    // execute into readiness.
    engine
        .handle_settings(&SettingsRequest {
            adapter: "adapter-1".to_owned(),
            generation,
            request: "settings-1".to_owned(),
            revision: 0,
            values: Vec::new(),
        })
        .expect("settings accepted");
    assert!(!engine.is_ready());
    let ready_report = decode_request(&wire_request).expect("decodes");
    let ready = match engine.handle_report(&ready_report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected ready reply, got {other:?}"),
    };
    assert!(ready.ready);
    assert!(engine.is_ready());
    let ready_wire = encode_reply(&ready);
    assert!(ready_wire.contains("\"ready\":true"));
    let parsed: WorldReplyWire = serde_json::from_str(&ready_wire).expect("valid json");
    assert_eq!(parsed.v, 3);
    assert!(parsed.ready);
    assert!(!parsed.workspaces.is_empty());
    assert!(!parsed.outputs.is_empty());
}

#[test]
fn world_request_rejects_duplicate_json_keys_strictly() {
    // Otherwise-valid staged request decodes on its own; every duplicate
    // spelling below reuses that same valid shape with one key doubled.
    let wire = one_window_request();
    decode_request(&wire).expect("valid staged request decodes");

    let assert_malformed = |bytes: &str, case: &str| {
        // Direct decode rejects; never a decode-then-encode round trip.
        let error = decode_request(bytes).expect_err(case);
        assert_eq!(error.kind, "malformed", "{case}");
        assert_eq!(error.message, "request is malformed", "{case}");
        assert!(
            !error.message.contains("adapter-1"),
            "{case} echoes payload"
        );
    };

    // Literal duplicate root key (otherwise-valid request, `adapter` twice).
    let literal_root = wire.replacen(
        "\"adapter\":\"adapter-1\"",
        "\"adapter\":\"adapter-1\",\"adapter\":\"adapter-1\"",
        1,
    );
    assert_ne!(literal_root, wire);
    assert_malformed(&literal_root, "literal duplicate root key refuses");

    // Escaped-equivalent duplicate root key: `\u0061` is `a`, so the second
    // key decodes to the same `adapter` name.
    let escaped_root = wire.replacen(
        "\"adapter\":\"adapter-1\"",
        "\"adapter\":\"adapter-1\",\"\\u0061dapter\":\"adapter-1\"",
        1,
    );
    assert_ne!(escaped_root, wire);
    assert_malformed(&escaped_root, "escaped-equivalent duplicate root refuses");

    // Nested duplicate inside the first screen row (`out` twice). The first
    // `"out":"out"` in the wire is the screen row object.
    let nested_screen = wire.replacen("\"out\":\"out\"", "\"out\":\"out\",\"out\":\"out\"", 1);
    assert_ne!(nested_screen, wire);
    assert_malformed(&nested_screen, "duplicate screen row key refuses");

    // Nested duplicate inside window hints (`minW` twice).
    let nested_hints = wire.replacen("\"minW\":null", "\"minW\":null,\"minW\":null", 1);
    assert_ne!(nested_hints, wire);
    assert_malformed(&nested_hints, "duplicate hints key refuses");

    // Duplicate inside a command object (`op` twice). String surgery keeps
    // the duplicate on the wire; a `serde_json::Value` round trip could not
    // express it.
    let command_wire = with_command(
        &serde_json::json!({"op": "focus", "win": "win-a", "direction": "left"}),
        "7",
    );
    decode_request(&command_wire).expect("valid command decodes");
    let command_dup =
        command_wire.replacen("\"op\":\"focus\"", "\"op\":\"focus\",\"op\":\"focus\"", 1);
    assert_ne!(command_dup, command_wire);
    assert_malformed(&command_dup, "duplicate command key refuses");

    // Trailing tokens after an otherwise-valid request refuse.
    assert_malformed(&format!("{wire} {{}}"), "trailing object refuses");
    assert_malformed(&format!("{wire}x"), "trailing garbage refuses");

    // Harmless controls: a single escaped key spelling still decodes, and
    // the same key name reused across separate objects stays valid.
    let escaped_single = wire.replacen("\"adapter\"", "\"\\u0061dapter\"", 1);
    assert_ne!(escaped_single, wire);
    decode_request(&escaped_single).expect("single escaped key decodes");
    let escaped_v = wire.replacen("\"v\":3", "\"\\u0076\":3", 1);
    assert_ne!(escaped_v, wire);
    decode_request(&escaped_v).expect("single escaped v decodes");
    let two_windows = request_json(
        "adapter-1",
        "gen-1",
        "1",
        "1",
        "req-1",
        &format!("{},{}", fact_json("win-a", 0), fact_json("win-b", 300)),
    );
    let report = decode_request(&two_windows).expect("sibling key reuse decodes");
    assert_eq!(report.windows.len(), 2);

    // v4 manifest duplicate witnesses are connection-family fixtures, so
    // they are asserted at the shared strict parser directly (not counted
    // as world duplicate coverage via `decode_request`).
    let root_fixture =
        include_str!("../../../test-fixtures/world-wire/v4/messages/invalid-duplicate-root.json");
    assert!(
        tiler_protocol::connection_protocol::parse_strict_json(root_fixture).is_err(),
        "manifest duplicate-root witness rejects at strict parser"
    );
    let nested_fixture =
        include_str!("../../../test-fixtures/world-wire/v4/messages/invalid-duplicate-nested.json");
    assert!(
        tiler_protocol::connection_protocol::parse_strict_json(nested_fixture).is_err(),
        "manifest duplicate-nested witness rejects at strict parser"
    );
}

#[test]
fn v4_screen_rows_decode_valid_held_and_reject_with_canonical_bytes() {
    use tiler_core::world_report::{
        RowDiagnosticKind, RowDiagnosticLane, encode_held_screen_canonical,
    };

    fn request_with(screens: &str, windows: &str, views: &str, extra_top: &str) -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["screens"] = serde_json::from_str(screens).expect("screens json");
        value["windows"] = serde_json::from_str(windows).expect("windows json");
        value["views"] = serde_json::from_str(views).expect("views json");
        let mut wire = value.to_string();
        if !extra_top.is_empty() {
            wire.pop();
            wire.push_str(extra_top);
            wire.push('}');
        }
        wire
    }

    // Valid row: scale plus full identity, with empty manufacturer text
    // preserved (never normalized to unknown).
    let valid = request_with(
        r#"[{"out":"out","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1.5,"identity":{"manufacturer":"","model":"m","serial":null,"connector":"c"}}]"#,
        "[]",
        "[]",
        "",
    );
    let report = decode_request(&valid).expect("valid row decodes");
    assert_eq!(report.screens.len(), 1);
    assert!(report.held_screens.is_empty());
    let screen = &report.screens[0];
    assert_eq!(screen.scale, 1.5);
    assert_eq!(screen.identity.manufacturer.as_deref(), Some(""));
    assert_eq!(screen.identity.model.as_deref(), Some("m"));
    assert_eq!(screen.identity.serial, None);
    assert_eq!(screen.identity.connector.as_deref(), Some("c"));
    assert_eq!(report.primary_output, None);
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= valid.len(),
        "estimate {estimate} bounds wire {}",
        valid.len()
    );

    // Held rows: zero scale, string scale, retired per-screen primary,
    // retired gap, overbound serial, unknown identity sub-key, and
    // non-object identity. Each holds narrowly with an InvalidFact screen
    // diagnostic over the output scope naming the output.
    let held_cases = [
        (
            "zero-scale",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
            r#"{"out":"out","full":null,"work":null,"complete":false,"scale":0,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}"#,
        ),
        (
            "string-scale",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":"1.5","identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
            r#"{"out":"out","full":null,"work":null,"complete":false,"scale":"1.5","identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}"#,
        ),
        (
            "retired-primary",
            r#"[{"out":"out","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null},"primary":true}]"#,
            r#"{"out":"out","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null},"primary":true}"#,
        ),
        (
            "retired-gap",
            r#"[{"out":"out","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null},"gap":8}]"#,
            r#"{"out":"out","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null},"gap":8}"#,
        ),
        (
            "unknown-identity-subkey",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"edid":"x","manufacturer":null,"model":null,"serial":null}}]"#,
            r#"{"out":"out","full":null,"work":null,"complete":false,"scale":1,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null,"edid":"x"}}"#,
        ),
        (
            "non-object-identity",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":"unknown"}]"#,
            r#"{"out":"out","full":null,"work":null,"complete":false,"scale":1,"identity":"unknown"}"#,
        ),
    ];
    for (name, screens, expected_canonical) in held_cases {
        let wire = request_with(screens, "[]", "[]", "");
        let report = decode_request(&wire).expect("held row decodes: {name}");
        assert!(report.screens.is_empty(), "{name}: no valid row");
        assert_eq!(report.held_screens.len(), 1, "{name}: one held row");
        let held = &report.held_screens[0];
        assert_eq!(held.output.0, "out", "{name}");
        assert_eq!(
            held.diagnostic.kind,
            RowDiagnosticKind::InvalidFact,
            "{name}"
        );
        assert_eq!(held.diagnostic.row, RowDiagnosticLane::Screen, "{name}");
        assert_eq!(held.diagnostic.id.as_deref(), Some("out"), "{name}");
        assert_eq!(
            encode_held_screen_canonical(held).expect("finite canonical row"),
            expected_canonical,
            "{name}: lossless canonical bytes"
        );
        let estimate = tiler_core::world_report::report_estimate(&report);
        assert!(
            estimate >= wire.len(),
            "{name}: estimate {estimate} bounds wire {}",
            wire.len()
        );
    }
    // Overbound identity text holds (257 bytes).
    let long = "s".repeat(257);
    let wire = request_with(
        &format!(
            r#"[{{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{{"manufacturer":null,"model":null,"serial":"{long}","connector":null}}}}]"#
        ),
        "[]",
        "[]",
        "",
    );
    let report = decode_request(&wire).expect("overbound serial holds");
    assert_eq!(report.held_screens.len(), 1);
    assert_eq!(
        report.held_screens[0].diagnostic.kind,
        RowDiagnosticKind::InvalidFact
    );

    // Whole-report refuses: missing scale, missing identity, bad `out`,
    // unknown non-retired key, bad `full` rect, and null report primary.
    for (name, screens) in [
        (
            "missing-scale",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
        ),
        (
            "missing-identity",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0}]"#,
        ),
        (
            "bad-out",
            r#"[{"out":"","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
        ),
        (
            "unknown-key",
            r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null},"dpi":2}]"#,
        ),
        (
            "bad-full",
            r#"[{"out":"out","full":[0,0,-1,2],"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
        ),
    ] {
        let wire = request_with(screens, "[]", "[]", "");
        assert!(decode_request(&wire).is_err(), "{name} refuses");
    }
    let null_primary = request_with(
        r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
        "[]",
        "[]",
        r#","primary":null"#,
    );
    assert!(
        decode_request(&null_primary).is_err(),
        "null primary refuses"
    );
    let named_primary = request_with(
        r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
        "[]",
        "[]",
        r#","primary":"out""#,
    );
    let report = decode_request(&named_primary).expect("named primary decodes");
    assert_eq!(
        report.primary_output.as_ref().map(|o| o.0.as_str()),
        Some("out")
    );
}

/// Byte span of one raw top-level lane value.
fn lane_value_span(message: &str, lane: &str) -> Option<(usize, usize)> {
    fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }
    fn skip_value(bytes: &[u8], start: usize) -> usize {
        let mut i = start;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        while i < bytes.len() {
            let byte = bytes[i];
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                    if depth == 0 {
                        return i + 1;
                    }
                }
            } else if byte == b'"' {
                in_string = true;
            } else if byte == b'{' || byte == b'[' {
                depth += 1;
            } else if byte == b'}' || byte == b']' {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            } else if depth == 0 && (byte == b',' || byte == b':' || byte.is_ascii_whitespace()) {
                return i;
            }
            i += 1;
        }
        i
    }
    let bytes = message.as_bytes();
    let want = format!("\"{lane}\"");
    let mut i = skip_ws(bytes, 0);
    assert_eq!(bytes[i], b'{', "message is a JSON object");
    i += 1;
    loop {
        i = skip_ws(bytes, i);
        if bytes[i] == b'}' {
            return None;
        }
        let key_start = i;
        i = skip_value(bytes, i);
        let key = &message[key_start..i];
        i = skip_ws(bytes, i);
        assert_eq!(bytes[i], b':', "key {key} needs a value");
        i = skip_ws(bytes, i + 1);
        if key == want {
            let end = skip_value(bytes, i);
            return Some((i, end));
        }
        i = skip_value(bytes, i);
        i = skip_ws(bytes, i);
        if bytes[i] == b',' {
            i += 1;
        }
    }
}

/// Raw top-level lane value slice, without re-serializing (a
/// `serde_json::Value` round-trip would sort object keys and respell
/// numbers). Returns `None` when the lane key is absent.
fn raw_lane_value<'a>(message: &'a str, lane: &str) -> Option<&'a str> {
    lane_value_span(message, lane).map(|(start, end)| &message[start..end])
}

/// Replace one top-level lane value with exact replacement bytes.
fn splice_lane(message: &str, lane: &str, replacement: &str) -> String {
    let (start, end) = lane_value_span(message, lane).expect("lane present");
    let mut out = String::with_capacity(message.len() + replacement.len());
    out.push_str(&message[..start]);
    out.push_str(replacement);
    out.push_str(&message[end..]);
    out
}

/// Raw top-level array-lane elements in wire order, sliced from the
/// original message bytes.
fn raw_lane_elements<'a>(message: &'a str, lane: &str) -> Vec<&'a str> {
    let value = raw_lane_value(message, lane).expect("lane {lane} present");
    let bytes = value.as_bytes();
    assert_eq!(bytes[0], b'[', "lane {lane} is an array");
    let mut rows = Vec::new();
    let mut i = 1usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes[i] == b']' {
            return rows;
        }
        // Reuse the value splitter by scanning the lane slice: element
        // boundaries are depth/string aware.
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let start = i;
        while i < bytes.len() {
            let byte = bytes[i];
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else if byte == b'"' {
                in_string = true;
            } else if byte == b'{' || byte == b'[' {
                depth += 1;
            } else if byte == b'}' || byte == b']' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if depth == 0 && byte == b',' {
                break;
            }
            i += 1;
        }
        rows.push(&value[start..i]);
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            if bytes[i] == b',' {
                i += 1;
                break;
            }
            i += 1;
        }
    }
    rows
}

/// Stage exact nested row bytes into the current parent report shape: the
/// staged parent is one the current codec decodes, while every screen row
/// (and the report-level primary value) is spliced textually from the
/// immutable fixture so no `Value` round-trip can sort keys or respell
/// numbers.
fn stage_fixture_rows(rows: &[&str], primary: Option<&str>) -> String {
    // one_window_request decodes on the current contract; empty the lanes
    // whose rows ride along so only the staged screen rows (and the
    // staged primary value) carry facts.
    let mut wire = splice_lane(&one_window_request(), "windows", "[]");
    wire = splice_lane(&wire, "views", "[]");
    wire = splice_lane(&wire, "closed", "[]");
    wire = splice_lane(&wire, "screens", &format!("[{}]", rows.join(",")));
    wire = splice_lane(&wire, "actionOutputs", "[]");
    if let Some(primary_value) = primary {
        wire.pop();
        wire.push_str(",\"primary\":");
        wire.push_str(primary_value);
        wire.push('}');
    }
    wire
}

/// Immutable v4 message bytes pin the canonical screen-row spelling: every
/// identifiable-but-bad row must retain its exact carried bytes (schema
/// order, canonical scalars), and the valid scale/identity/primary rows
/// must parse with facts preserved. Expected bytes come from the fixture
/// file itself, never from a `Value` re-serialization or a hand-authored
/// row.
#[test]
fn v4_screen_rows_retain_canonical_bytes_from_immutable_fixtures() {
    use tiler_core::world_report::{
        RowDiagnosticKind, RowDiagnosticLane, encode_held_screen_canonical, encode_screen_canonical,
    };

    let held: &[(&str, &str)] = &[
        (
            "zero-scale",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-screen-zero-scale.json"
            ),
        ),
        (
            "negative-scale",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-screen-negative-scale.json"
            ),
        ),
        (
            "string-scale",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-screen-string-scale.json"
            ),
        ),
        (
            "overbound-identity",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-identity-overbound.json"
            ),
        ),
        (
            "removed-gap",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-removed-screen-gap.json"
            ),
        ),
        (
            "removed-outer",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-removed-screen-outer.json"
            ),
        ),
        (
            "removed-primary",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-removed-screen-primary.json"
            ),
        ),
    ];
    for (name, message) in held {
        let rows = raw_lane_elements(message, "screens");
        assert_eq!(rows.len(), 1, "{name}: one raw row");
        // The nested row bytes ride the production route staged in the
        // current parent shape; the row itself is never re-serialized.
        let staged = stage_fixture_rows(&rows, None);
        let report = decode_request(&staged).expect("{name}: held row decodes");
        assert!(report.screens.is_empty(), "{name}: no valid row");
        assert_eq!(report.held_screens.len(), 1, "{name}: one held row");
        let held_row = &report.held_screens[0];
        assert_eq!(held_row.output.0, "out-a", "{name}");
        assert_eq!(held_row.ordinal, 0, "{name}: original screens index");
        assert_eq!(
            held_row.diagnostic.kind,
            RowDiagnosticKind::InvalidFact,
            "{name}"
        );
        assert_eq!(held_row.diagnostic.row, RowDiagnosticLane::Screen, "{name}");
        assert_eq!(held_row.diagnostic.id.as_deref(), Some("out-a"), "{name}");
        assert_eq!(
            encode_held_screen_canonical(held_row).expect("finite canonical row"),
            rows[0],
            "{name}: lossless canonical bytes"
        );
    }

    // The valid scale/identity/primary message parses with facts intact.
    let valid = include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-scale-identity-primary.json"
    );
    let valid_rows = raw_lane_elements(valid, "screens");
    assert_eq!(valid_rows.len(), 2);
    let valid_primary = raw_lane_value(valid, "primary");
    assert_eq!(valid_primary, Some("\"out-b\""));
    let staged = stage_fixture_rows(&valid_rows, valid_primary);
    let report = decode_request(&staged).expect("valid rows decode");
    assert_eq!(report.screens.len(), 2);
    assert!(report.held_screens.is_empty());
    assert_eq!(report.screens[0].scale, 1.25);
    assert_eq!(report.screens[1].scale, 1.5);
    assert_eq!(
        report.screens[0].identity.connector.as_deref(),
        Some("connector-a")
    );
    assert_eq!(
        report.screens[1].identity.connector.as_deref(),
        Some("connector-b")
    );
    assert_eq!(
        report.primary_output.as_ref().map(|o| o.0.as_str()),
        Some("out-b")
    );
    // A decoded valid row re-encodes to its exact carried bytes.
    for (screen, raw) in report.screens.iter().zip(valid_rows.iter()) {
        assert_eq!(encode_screen_canonical(screen).expect("finite row"), *raw);
    }

    // An explicit report-level null primary refuses (unknown is absence,
    // never an explicit null).
    let null_primary =
        include_str!("../../../test-fixtures/world-wire/v4/messages/invalid-primary-null.json");
    assert_eq!(raw_lane_value(null_primary, "primary"), Some("null"));
    let staged = stage_fixture_rows(&raw_lane_elements(null_primary, "screens"), Some("null"));
    assert!(
        decode_request(&staged).is_err(),
        "explicit null primary refuses"
    );
}

/// Held rows retain multibyte identity text byte-exactly: the 256-byte
/// bound counts UTF-8 bytes (a 257-byte serial straddling a character
/// boundary holds while the 256-byte prefix stays valid), and C0, quote,
/// and control escapes spell canonically.
#[test]
fn v4_held_rows_retain_multibyte_identity_bytes() {
    use tiler_core::world_report::encode_held_screen_canonical;

    // 254 ASCII bytes plus one 2-byte character: exactly the bound.
    let at_bound = format!("{}é", "s".repeat(254));
    assert_eq!(at_bound.len(), 256);
    let valid_row = format!(
        "{{\"out\":\"out-a\",\"full\":null,\"work\":null,\"complete\":false,\
         \"scale\":1,\"identity\":{{\"manufacturer\":\"Müller-µ-日本語-🎛\",\
         \"model\":\"m\",\"serial\":\"{at_bound}\",\"connector\":null}}}}"
    );
    let staged = stage_fixture_rows(&[&valid_row], None);
    let report = decode_request(&staged).expect("bound-exact identity decodes");
    assert_eq!(report.screens.len(), 1);
    assert_eq!(
        report.screens[0].identity.manufacturer.as_deref(),
        Some("Müller-µ-日本語-🎛")
    );
    assert_eq!(
        report.screens[0].identity.serial.as_deref(),
        Some(at_bound.as_str())
    );
    assert_eq!(
        tiler_core::world_report::encode_screen_canonical(&report.screens[0]).expect("finite row"),
        valid_row
    );

    // One more byte tips the serial over the bound; the row holds with its
    // exact carried bytes, including the C0/quote escapes.
    let over_bound = format!("{at_bound}s");
    assert_eq!(over_bound.len(), 257);
    let held_row = format!(
        "{{\"out\":\"out-b\",\"full\":null,\"work\":null,\"complete\":false,\
         \"scale\":1,\"identity\":{{\"manufacturer\":\"a\\\"b\\\\c\\u0001\\u000a終\",\"model\":\"m\",\
         \"serial\":\"{over_bound}\",\"connector\":null}}}}"
    );
    let staged = stage_fixture_rows(&[&held_row], None);
    let report = decode_request(&staged).expect("overbound identity holds");
    assert!(report.screens.is_empty());
    assert_eq!(report.held_screens.len(), 1);
    let held = &report.held_screens[0];
    assert_eq!(held.ordinal, 0);
    assert_eq!(
        encode_held_screen_canonical(held).expect("finite row"),
        held_row
    );
}

/// A mixed valid/held/valid lane keeps every row's original index: the
/// valid rows stay in relative order and the held row carries its carried
/// position, so the factual order reconstructs losslessly.
#[test]
fn v4_mixed_valid_held_rows_keep_original_index() {
    let rows = [
        r#"{"out":"out-a","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}"#,
        r#"{"out":"out-b","full":null,"work":null,"complete":false,"scale":0,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}"#,
        r#"{"out":"out-c","full":[800,0,800,600],"work":[800,0,800,560],"complete":true,"scale":2,"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}"#,
    ];
    let staged = stage_fixture_rows(&rows, None);
    let report = decode_request(&staged).expect("mixed rows decode");
    assert_eq!(report.screens.len(), 2);
    assert_eq!(report.screens[0].output.0, "out-a");
    assert_eq!(report.screens[1].output.0, "out-c");
    assert_eq!(report.held_screens.len(), 1);
    assert_eq!(report.held_screens[0].output.0, "out-b");
    assert_eq!(report.held_screens[0].ordinal, 1);
    assert_eq!(
        tiler_core::world_report::encode_held_screen_canonical(&report.held_screens[0])
            .expect("finite row"),
        rows[1]
    );
    assert!(tiler_core::world_report::validate_report(&report));
}

/// Causal equivalence (finding B): directly constructed identifiable
/// invalid rows (bad scale, overbound identity) reach the same engine
/// reply as their protocol-decoded narrow-hold twins. Post-ready the
/// healthy output stays controlled while the bad output holds with its
/// InvalidFact diagnostic, and the graded direct report equals the decoded
/// report field-for-field with ordinals preserved.
#[test]
fn direct_typed_invalid_rows_match_decoded_hold_replies() {
    use tiler_core::world_engine::WorldOutcome;
    use tiler_core::world_report::{
        AuthorityScope, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority, grade_invalid_screens,
    };

    fn healthy_row(out: &str) -> String {
        format!(
            r#"{{"out":"{out}","full":[0,0,800,600],"work":[0,0,800,560],"complete":true,"scale":1,"identity":{{"manufacturer":null,"model":null,"serial":null,"connector":null}}}}"#
        )
    }
    fn two_healthy_wire() -> String {
        let wire = splice_lane(
            &one_window_request(),
            "screens",
            &format!("[{},{}]", healthy_row("out-a"), healthy_row("out-b")),
        );
        let wire = splice_lane(&wire, "windows", "[]");
        let wire = splice_lane(&wire, "views", "[]");
        splice_lane(&wire, "actionOutputs", "[]")
    }
    fn check_scenario(
        name: &str,
        bad_row: &str,
        mutate: impl FnOnce(&mut WorldReport),
        expected_held: usize,
    ) {
        let decoded_wire = splice_lane(
            &two_healthy_wire(),
            "screens",
            &format!("[{},{}]", healthy_row("out-a"), bad_row),
        );
        let mut decoded: WorldReport = decode_request(&decoded_wire).expect("held row decodes");
        assert_eq!(decoded.screens.len(), 1, "{name}: one valid row");
        assert_eq!(decoded.held_screens.len(), 1, "{name}: one held row");
        assert_eq!(decoded.held_screens[0].output.0, "out-b", "{name}");
        assert_eq!(
            decoded.held_screens[0].ordinal, 1,
            "{name}: ordinal preserved"
        );

        let mut direct: WorldReport =
            decode_request(&two_healthy_wire()).expect("healthy rows decode");
        assert_eq!(direct.screens.len(), 2, "{name}");
        mutate(&mut direct);

        for report in [&mut decoded, &mut direct] {
            report.observation_seq = 2;
            report.request_seq = 2;
            report.request_id = "req-2".to_owned();
        }
        // Structural field equivalence, not merely reply equality: the
        // graded direct report must equal the decoded report.
        assert_eq!(
            grade_invalid_screens(&direct),
            Some(decoded.clone()),
            "{name}: graded direct report equals decoded report"
        );

        let mut engine_held = connected_engine("gen-1", HostCapabilities::default());
        let mut engine_direct = connected_engine("gen-1", HostCapabilities::default());
        let seed: WorldReport = decode_request(&two_healthy_wire()).expect("seed decodes");
        for engine in [&mut engine_held, &mut engine_direct] {
            match engine.handle_report(&seed) {
                WorldOutcome::Accepted(reply) => assert!(reply.ready, "{name}: seed latches"),
                other => panic!("{name}: seed must latch readiness, got {other:?}"),
            }
        }
        let held_reply = match engine_held.handle_report(&decoded) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("{name}: decoded hold must be accepted, got {other:?}"),
        };
        let direct_reply = match engine_direct.handle_report(&direct) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("{name}: direct invalid row must grade to hold, got {other:?}"),
        };
        assert!(held_reply.ready, "{name}");
        assert_eq!(held_reply.diagnostics.len(), expected_held, "{name}");
        assert_eq!(
            held_reply.diagnostics[0].kind,
            RowDiagnosticKind::InvalidFact,
            "{name}"
        );
        assert_eq!(
            held_reply.diagnostics[0].row,
            RowDiagnosticLane::Screen,
            "{name}"
        );
        assert_eq!(
            held_reply.diagnostics[0].id.as_deref(),
            Some("out-b"),
            "{name}"
        );
        let healthy = direct_reply
            .scopes
            .iter()
            .find(|status| matches!(&status.scope, AuthorityScope::Output(o) if o.0 == "out-a"))
            .expect("healthy output scope");
        assert_eq!(healthy.authority, ScopeAuthority::Controlled, "{name}");
        let held = direct_reply
            .scopes
            .iter()
            .find(|status| matches!(&status.scope, AuthorityScope::Output(o) if o.0 == "out-b"))
            .expect("held output scope");
        assert!(
            matches!(held.authority, ScopeAuthority::Hold(_)),
            "{name}: bad output holds"
        );
        assert_eq!(direct_reply, held_reply, "{name}: reply equivalence");
    }

    check_scenario(
        "zero-scale",
        &healthy_row("out-b").replace("\"scale\":1", "\"scale\":0"),
        |direct| direct.screens[1].scale = 0.0,
        1,
    );
    let long_serial = "s".repeat(257);
    check_scenario(
        "overbound-identity",
        &healthy_row("out-b").replace("\"serial\":null", &format!("\"serial\":\"{long_serial}\"")),
        |direct| {
            direct.screens[1].identity.serial = Some(long_serial.clone());
        },
        1,
    );
}

/// Decoder structural parity: rect-pair/complete violations refuse before
/// any narrow hold for bad scale, overbound identity, or retired keys. The
/// valid-scale baselines already refuse through the post-split core gate;
/// the hold-eligible twins must refuse with the same reason instead of
/// decoding Held. Each direct typed twin refuses through
/// `validate_report_reason` with the identical reason.
#[test]
fn decode_screen_structural_violations_refuse_before_narrow_holds() {
    use tiler_core::world_report::{WorldReport, validate_report_reason};

    const FULL: &str = "[0,0,800,600]";
    const NULL_IDENTITY: &str =
        r#"{"manufacturer":null,"model":null,"serial":null,"connector":null}"#;
    fn row(
        out: &str,
        full: &str,
        work: &str,
        complete: bool,
        scale: &str,
        identity: &str,
        extra: &str,
    ) -> String {
        format!(
            r#"{{"out":"{out}","full":{full},"work":{work},"complete":{complete},"scale":{scale},"identity":{identity}{extra}}}"#
        )
    }
    fn healthy(out: &str) -> String {
        row(out, FULL, "[0,0,800,560]", true, "1", NULL_IDENTITY, "")
    }
    fn check_wire(name: &str, rows: &[String], reason: &str) {
        let wire = stage_fixture_rows(&rows.iter().map(String::as_str).collect::<Vec<_>>(), None);
        let error = decode_request(&wire).expect_err(&format!("{name}: refuses"));
        assert_eq!(error.kind, "snapshot-invalid", "{name}: kind");
        assert_eq!(error.message, reason, "{name}: reason");
    }
    fn check_direct(
        name: &str,
        base_rows: &[String],
        mutate: impl FnOnce(&mut WorldReport),
        reason: &str,
    ) {
        let wire = stage_fixture_rows(
            &base_rows.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
        );
        let mut direct: WorldReport = decode_request(&wire).expect("{name}: base decodes");
        mutate(&mut direct);
        assert_eq!(
            validate_report_reason(&direct),
            Some(reason),
            "{name}: direct whole-refusal"
        );
    }

    let long_serial = "s".repeat(257);
    let long_identity = format!(
        r#"{{"manufacturer":null,"model":null,"serial":"{long_serial}","connector":null}}"#
    );
    // (name, bad row, valid-scale baseline row, reason)
    let cases: Vec<(&str, String, String, &str)> = vec![
        (
            "complete-without-rects-bad-scale",
            row("out-a", "null", "null", true, "0", NULL_IDENTITY, ""),
            row("out-a", "null", "null", true, "1", NULL_IDENTITY, ""),
            "screen-complete-invalid",
        ),
        (
            "partial-rects-bad-scale",
            row("out-a", FULL, "null", false, "0", NULL_IDENTITY, ""),
            row("out-a", FULL, "null", false, "1", NULL_IDENTITY, ""),
            "screen-rect-partial",
        ),
        (
            "partial-rects-overbound-identity",
            row("out-a", FULL, "null", false, "1", &long_identity, ""),
            row("out-a", FULL, "null", false, "1", &long_identity, ""),
            "screen-rect-partial",
        ),
        (
            "complete-without-rects-overbound-identity",
            row("out-a", "null", "null", true, "1", &long_identity, ""),
            row("out-a", "null", "null", true, "1", &long_identity, ""),
            "screen-complete-invalid",
        ),
        (
            "partial-rects-retired-gap",
            row(
                "out-a",
                FULL,
                "null",
                false,
                "1",
                NULL_IDENTITY,
                r#","gap":8"#,
            ),
            row(
                "out-a",
                FULL,
                "null",
                false,
                "1",
                NULL_IDENTITY,
                r#","gap":8"#,
            ),
            "screen-rect-partial",
        ),
        (
            "complete-without-rects-retired-gap",
            row(
                "out-a",
                "null",
                "null",
                true,
                "1",
                NULL_IDENTITY,
                r#","gap":8"#,
            ),
            row(
                "out-a",
                "null",
                "null",
                true,
                "1",
                NULL_IDENTITY,
                r#","gap":8"#,
            ),
            "screen-complete-invalid",
        ),
    ];
    for (name, bad, baseline, reason) in &cases {
        // Valid-scale baseline already refuses through the post-split gate.
        check_wire(
            &format!("{name}-baseline"),
            std::slice::from_ref(baseline),
            reason,
        );
        // Hold-eligible twin must refuse with the same reason, not hold.
        check_wire(name, std::slice::from_ref(bad), reason);
    }

    // Direct typed parity for every shape.
    check_direct(
        "complete-without-rects-bad-scale",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].full_rect = None;
            direct.screens[0].work_area = None;
            direct.screens[0].complete = true;
            direct.screens[0].scale = 0.0;
        },
        "screen-complete-invalid",
    );
    check_direct(
        "partial-rects-bad-scale",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].work_area = None;
            direct.screens[0].complete = false;
            direct.screens[0].scale = 0.0;
        },
        "screen-rect-partial",
    );
    check_direct(
        "partial-rects-overbound-identity",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].work_area = None;
            direct.screens[0].complete = false;
            direct.screens[0].identity.serial = Some(long_serial.clone());
        },
        "screen-rect-partial",
    );
    check_direct(
        "complete-without-rects-overbound-identity",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].full_rect = None;
            direct.screens[0].work_area = None;
            direct.screens[0].complete = true;
            direct.screens[0].identity.serial = Some(long_serial.clone());
        },
        "screen-complete-invalid",
    );
    check_direct(
        "partial-rects-retired-gap",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].work_area = None;
            direct.screens[0].complete = false;
        },
        "screen-rect-partial",
    );
    check_direct(
        "complete-without-rects-retired-gap",
        &[healthy("out-a")],
        |direct| {
            direct.screens[0].full_rect = None;
            direct.screens[0].work_area = None;
            direct.screens[0].complete = true;
        },
        "screen-complete-invalid",
    );
    // Both row orders refuse for the two concrete witnesses.
    let witness_scale_complete = row("out-b", "null", "null", true, "0", NULL_IDENTITY, "");
    let witness_scale_partial = row("out-b", FULL, "null", false, "0", NULL_IDENTITY, "");
    for (name, bad, reason) in [
        (
            "complete-without-rects-bad-scale",
            witness_scale_complete,
            "screen-complete-invalid",
        ),
        (
            "partial-rects-bad-scale",
            witness_scale_partial,
            "screen-rect-partial",
        ),
    ] {
        check_wire(
            &format!("{name}-bad-second"),
            &[healthy("out-a"), bad.clone()],
            reason,
        );
        check_wire(
            &format!("{name}-bad-first"),
            &[bad.clone(), healthy("out-a")],
            reason,
        );
        // Direct twins refuse in both orders too.
        check_direct(
            &format!("{name}-bad-second"),
            &[healthy("out-a"), healthy("out-b")],
            |direct| {
                if reason == "screen-complete-invalid" {
                    direct.screens[1].full_rect = None;
                    direct.screens[1].work_area = None;
                    direct.screens[1].complete = true;
                } else {
                    direct.screens[1].work_area = None;
                    direct.screens[1].complete = false;
                }
                direct.screens[1].scale = 0.0;
            },
            reason,
        );
        check_direct(
            &format!("{name}-bad-first"),
            &[healthy("out-a"), healthy("out-b")],
            |direct| {
                direct.screens[0].work_area = None;
                if reason == "screen-complete-invalid" {
                    direct.screens[0].full_rect = None;
                    direct.screens[0].complete = true;
                } else {
                    direct.screens[0].complete = false;
                }
                direct.screens[0].scale = 0.0;
            },
            reason,
        );
    }
}

#[test]
fn executable_null_rows_keep_an_independent_known_process_id() {
    // Process identity is never inferred from executable: a supported-null
    // executable stays unknown while the row carries its own known foreign
    // process. Full-unavailable contexts keep null/null for both.
    let null_exe: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-executable-null.json"
    ))
    .expect("fixture json");
    let row = &null_exe["windows"][0];
    assert!(row["executable"].is_null(), "executable stays unknown");
    assert_eq!(
        row["processId"],
        serde_json::Value::String("pid-foreign-1".to_owned()),
        "processId is an independent known foreign pid, not inferred null"
    );
    for (file, text) in [
        (
            "report-unavailable-facts",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/report-unavailable-facts.json"
            ),
        ),
        (
            "invalid-unavailable-fact-contradiction",
            include_str!(
                "../../../test-fixtures/world-wire/v4/messages/invalid-unavailable-fact-contradiction.json"
            ),
        ),
    ] {
        let unavailable: serde_json::Value = serde_json::from_str(text).expect("fixture json");
        let row = &unavailable["windows"][0];
        assert!(
            row["executable"].is_null() && row["processId"].is_null(),
            "{file}: full-unavailable context keeps null/null"
        );
    }
}
