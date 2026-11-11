//! Portable JSON codec coverage for the logical-first world boundary (v4):
//! DTO validation, u64-as-string enforcement, independent core byte count
//! plus decoder roundtrip through the v4 codec, literal
//! golden bootstrap bytes, and budget gates.
//! Every staged envelope is a genuine v4 envelope (`"v":4`, no `views`
//! lane, the v4 presentation shape, plus the required `intentRecords`,
//! `stateAttempts`, `focusFeedback`, and `rendererInput` lanes) decoded
//! through the genuine v4 ingress.
//! Offline only: no D-Bus, no KWin.

use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
};
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{WORLD_CONTRACT_VERSION_V4, WorldReplyV4, WorldReportV4};
use tiler_core::world_report_v4_checks::{report_digest_v4, validate_report_v4_reason};
use tiler_core::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4, report_estimate_v4};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES,
    WorldOutcome, decode_reply_v4, decode_request, encode_reply_v4,
};

/// Minimal observation-only v4 report envelope: screens plus facts only,
/// no native workspace concept. The v4 contract carries `rendererInput`
/// (null here), refuses the removed `views` lane, and shapes presentation
/// as the v4 ingress row.
fn request_json(
    adapter: &str,
    generation: &str,
    obs: &str,
    req_seq: &str,
    req: &str,
    facts: &str,
) -> String {
    format!(
        "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
         \"obs\":\"{obs}\",\"reqSeq\":\"{req_seq}\",\"req\":\"{req}\",\"cmdSeq\":null,\
         \"command\":null,\
         \"screens\":[{{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
          \"complete\":true,\"scale\":1.0,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
         \"windows\":[{facts}],\
          \"closed\":[],\"native\":[],\
         \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"out\"],\
          \"focused\":\"\",\"foregroundKnown\":false,\"complete\":true,\"gesture\":false,\"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\"foregroundOut\":null}},\"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\"rendererInput\":null}}"
    )
}

fn fact_json(id: &str, x: i32) -> String {
    format!(
        "{{\"win\":\"{id}\",\"out\":\"out\",\"rect\":[{x},0,200,200],\
         \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\
         \"minimized\":false,\
         \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
         \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
         \"acceptsInput\":true,\"activationInhibited\":false,\
         \"toolWindow\":false,\"owned\":false,\"transient\":false,\
         \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
         \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
         \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
         \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
         \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
         \"suppressionAvailable\":null,\
         \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
         \"lastWrite\":null}}"
    )
}

/// Canonical v4 raw window row as JSON: every REQUIRED key present with
/// hold-healthy values (class/type/policy/layer known so the core admits
/// the row; identity/role nulls stay unknown). Callers override keys
/// (e.g. `nativeWs`, `out`) for the case under test.
fn v4_window(win: &str, out: &str, rect: [i32; 4]) -> serde_json::Value {
    serde_json::json!({
        "win": win, "out": out, "rect": rect,
        "fullscreen": false, "maximized": false, "sticky": false,
        "minimized": false,
        "hints": {"minW": null, "minH": null, "maxW": null, "maxH": null},
        "hostHidden": false, "privileged": false, "managed": true,
        "acceptsInput": true, "activationInhibited": false,
        "toolWindow": false, "owned": false, "transient": false,
        "modal": false, "resizable": true, "above": false, "below": false,
        "owner": null, "parent": null, "role": null, "appId": null,
        "executable": null, "processId": "pid-foreign-1", "class": "Synthetic",
        "title": null, "type": "normal", "activationPolicy": "regular",
        "layer": 0, "fullscreenOwner": null, "fullscreenExitable": null,
        "suppressionAvailable": null,
        "visibility": {"visible": true, "suppressionOwned": null},
        "lastWrite": null,
    })
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

/// Encode one engine-published v4 reply with the canonical v4 writer; the
/// bytes must match the independent core byte count exactly and round-trip
/// through the reply decoder to the identical typed reply plus wire
/// metadata. This is the sole writer evidence: no legacy writer exists on
/// this path.
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

#[test]
fn decode_minimal_observation_only_request() {
    let report: WorldReportV4 = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.contract_version, WORLD_CONTRACT_VERSION_V4);
    assert_eq!(report.contract_version, 4);
    assert_eq!(report.observation_seq, 1);
    assert_eq!(report.request_seq, 1);
    assert_eq!(report.screens.len(), 1);
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].window.0, "win-a");
    assert_eq!(report.windows[0].output.as_ref().expect("out").0, "out");
    // The removed `views` lane carries nothing: the v4 envelope refuses a
    // `views` key outright as an unknown field.
    assert!(report.closed.is_empty());
    assert!(report.native.is_empty());
    assert!(report.command.is_none());
    assert_eq!(report.focused_global, "");
    assert!(report.inventory_complete);
    assert!(report.renderer_input.is_none());
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
    for version in [1, 2, 3] {
        let wire = one_window_request().replace("\"v\":4", &format!("\"v\":{version}"));
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
    // By-id focus (v4): the ignored direction is removed from the wire;
    // the staged record spells the absent direction as empty.
    let command_json = serde_json::json!({"op": "focus", "win": "win-a"});
    let report = decode_request(&with_command(&command_json, "7")).expect("focus decodes");
    assert!(matches!(
        report.command,
        Some(C::Focus { direction, .. }) if direction.is_empty()
    ));
    assert_eq!(report.command_seq, Some(7));
    let bad = serde_json::json!({"op": "focus", "win": "win-a", "direction": "sideways"});
    assert!(decode_request(&with_command(&bad, "7")).is_err());
    for (op, extra) in [
        (
            "send",
            serde_json::json!({"win": "w", "out": "o", "ws": "ws-1", "follow": true}),
        ),
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
    // Retired v4 ops refuse as unknown values (contract 665-668): neither
    // `switch-view` nor `operation` decodes on the v4 ingress.
    for retired in [
        serde_json::json!({"op": "switch-view", "out": "o", "ws": "ws-1"}),
        serde_json::json!({"op": "operation", "out": "o"}),
    ] {
        let error = decode_request(&with_command(&retired, "7")).expect_err("retired op refuses");
        assert_eq!(error.kind, "unknown-value");
    }
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
fn fullscreen_commands_decode_strictly() {
    use tiler_core::world_report::WorldCommand as C;
    // Valid set/toggle spellings decode typed.
    let set = serde_json::json!({"op": "set-fullscreen", "win": "win-a", "value": true});
    match decode_request(&with_command(&set, "7"))
        .expect("set decodes")
        .command
    {
        Some(C::SetFullscreen { window, value }) => {
            assert_eq!(window.0, "win-a");
            assert!(value);
        }
        other => panic!("set-fullscreen decoded {other:?}"),
    }
    let set_false = serde_json::json!({"op": "set-fullscreen", "win": "win-a", "value": false});
    match decode_request(&with_command(&set_false, "8"))
        .expect("set false decodes")
        .command
    {
        Some(C::SetFullscreen { window, value }) => {
            assert_eq!(window.0, "win-a");
            assert!(!value);
        }
        other => panic!("set-fullscreen decoded {other:?}"),
    }
    let toggle = serde_json::json!({"op": "toggle-fullscreen", "win": "win-a"});
    match decode_request(&with_command(&toggle, "9"))
        .expect("toggle decodes")
        .command
    {
        Some(C::ToggleFullscreen { window }) => assert_eq!(window.0, "win-a"),
        other => panic!("toggle-fullscreen decoded {other:?}"),
    }
    // Closed keys: extra keys refuse.
    for bad in [
        serde_json::json!({"op": "set-fullscreen", "win": "w", "value": true, "extra": 1}),
        serde_json::json!({"op": "toggle-fullscreen", "win": "w", "extra": 1}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err(), "{bad}");
    }
    // Missing keys refuse.
    for bad in [
        serde_json::json!({"op": "set-fullscreen", "win": "w"}),
        serde_json::json!({"op": "set-fullscreen", "value": true}),
        serde_json::json!({"op": "toggle-fullscreen"}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err(), "{bad}");
    }
    // Wrong types refuse (value must be bool, win must be an id string).
    for bad in [
        serde_json::json!({"op": "set-fullscreen", "win": "w", "value": "true"}),
        serde_json::json!({"op": "set-fullscreen", "win": "w", "value": 1}),
        serde_json::json!({"op": "set-fullscreen", "win": "w", "value": null}),
        serde_json::json!({"op": "set-fullscreen", "win": "", "value": true}),
        serde_json::json!({"op": "set-fullscreen", "win": 1, "value": true}),
        serde_json::json!({"op": "toggle-fullscreen", "win": ""}),
        serde_json::json!({"op": "toggle-fullscreen", "win": 1}),
        serde_json::json!({"op": "toggle-fullscreen", "win": "w", "value": true}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err(), "{bad}");
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
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= compact.len(),
        "estimate {estimate} bounds wire {}",
        compact.len()
    );
    let empty = request_json("adapter-1", "gen-1", "1", "1", "req-1", "");
    let report = decode_request(&empty).expect("empty decodes");
    let estimate = report_estimate_v4(&report);
    assert!(estimate >= empty.len());
    // Carried action-context facts stay bounded too.
    let mut full: serde_json::Value = serde_json::from_str(&compact).expect("valid");
    full["actionForeground"] = serde_json::Value::String("out".to_owned());
    full["actionCached"] = serde_json::Value::String("out".to_owned());
    let wire = full.to_string();
    let report = decode_request(&wire).expect("facts decode");
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    // A populated presentation ingress stays bounded too.
    let mut full: serde_json::Value = serde_json::from_str(&compact).expect("valid");
    full["presentation"] = serde_json::json!({
        "foreground": "win-a", "minimized": false, "maximized": false,
        "fullscreen": false, "hostHidden": false, "hiddenWorkspace": false,
        "frame": [0, 0, 200, 200],
        "chordHeld": true, "moveWindow": "win-a",
        "pagerOrder": ["win-a"],
        "pagerFrames": [{"win": "win-a", "live": [0, 0, 200, 200]}],
        "witness": null, "foregroundOut": null,
    });
    let wire = full.to_string();
    let report = decode_request(&wire).expect("presentation decodes");
    let ingress = match &report.presentation {
        tiler_core::world_report::PresentationRowV4::Valid(ingress) => ingress,
        tiler_core::world_report::PresentationRowV4::Held(_) => panic!("staged ingress is valid"),
    };
    assert_eq!(ingress.foreground, "win-a");
    assert_eq!(ingress.pager_order.len(), 1);
    assert_eq!(ingress.pager_frames.len(), 1);
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
}

#[test]
fn presentation_ingress_decodes_strictly() {
    use tiler_core::world_report::PresentationRowV4;
    fn ingress_of(report: &WorldReportV4) -> &tiler_core::world_report::PresentationIngressV4 {
        match &report.presentation {
            PresentationRowV4::Valid(ingress) => ingress,
            PresentationRowV4::Held(_) => panic!("staged ingress is valid"),
        }
    }
    // The disabled fixture rides an empty presentation ingress.
    let report = decode_request(&one_window_request()).expect("decodes");
    let ingress = ingress_of(&report);
    assert!(ingress.foreground.is_empty());
    assert!(ingress.pager_order.is_empty());
    // Missing presentation refuses: remote scripts must send it.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("presentation");
    assert!(decode_request(&value.to_string()).is_err());
    // Unknown top-level keys hold narrowly in v4 (contract 307-314
    // identifiable-row scope, 758-769 row presentation): the carried key
    // rides the held row instead of refusing the envelope. Bad identity
    // tokens and zero-rect frames still refuse the whole report (below).
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["extra"] = serde_json::Value::Bool(true);
    let report = decode_request(&value.to_string()).expect("unknown presentation field holds");
    match &report.presentation {
        PresentationRowV4::Held(held) => {
            assert_eq!(
                held.kind,
                tiler_core::world_report::RowDiagnosticKind::InvalidFact
            );
            assert!(
                held.fields.iter().any(|(name, field)| name == "extra"
                    && *field == tiler_core::connection::RawSettingValue::Bool(true)),
                "extra key retained"
            );
        }
        PresentationRowV4::Valid(_) => panic!("extra key must hold, not validate"),
    }
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
    // Removed style keys hold narrowly in v4: border presentation is an
    // anchored settings-derived descriptor, never ingress style numbers
    // (contract presentation keys removed), and the identifiable ingress
    // row carries the retired key instead of refusing the envelope.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["borderEnabled"] = serde_json::json!(true);
    let report = decode_request(&value.to_string()).expect("removed style holds");
    match &report.presentation {
        PresentationRowV4::Held(held) => {
            assert_eq!(
                held.kind,
                tiler_core::world_report::RowDiagnosticKind::InvalidFact
            );
            assert!(
                held.fields
                    .iter()
                    .any(|(name, field)| name == "borderEnabled"
                        && *field == tiler_core::connection::RawSettingValue::Bool(true)),
                "retired key retained"
            );
        }
        PresentationRowV4::Valid(_) => panic!("removed style must hold, not validate"),
    }
    // Snapshot rows refuse unknown ids and non-rect frames.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["pagerFrames"] =
        serde_json::json!([{"win": "win-a", "live": [0, 0, 200, 200]}]);
    let report = decode_request(&value.to_string()).expect("frames decode");
    assert_eq!(ingress_of(&report).pager_frames.len(), 1);
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["pagerFrames"] =
        serde_json::json!([{"win": "win-a", "live": [0, 0, 0, 0]}]);
    assert!(decode_request(&value.to_string()).is_err());
    // Empty tokens mean none and stay valid.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["presentation"]["foreground"] = serde_json::Value::String(String::new());
    value["presentation"]["moveWindow"] = serde_json::Value::String(String::new());
    let report = decode_request(&value.to_string()).expect("empty tokens decode");
    let ingress = ingress_of(&report);
    assert!(ingress.foreground.is_empty());
    assert!(ingress.move_window.is_empty());
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
    // A prewrite without an operation id holds narrowly in v4: only
    // placement writes carry a preimage, and the identifiable row is
    // retained instead of refusing the envelope.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "refused",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1001", "op": "", "pre": [0, 0, 800, 560],
    }]);
    let held_report = decode_request(&value.to_string()).expect("pre without op holds");
    assert!(held_report.frames.is_empty(), "no valid frame row");
    assert_eq!(held_report.held_attempts.len(), 1, "one held attempt row");
    assert_eq!(
        held_report.held_attempts[0].lane,
        tiler_core::world_report::AttemptLaneV4::Frame
    );
    let estimate = report_estimate_v4(&report);
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
    // Unknown attempt keys hold the identifiable row in v4 instead of
    // refusing the envelope.
    let held_report = decode_request(&value.to_string()).expect("unknown field holds");
    assert!(held_report.frames.is_empty(), "no valid frame row");
    assert_eq!(held_report.held_attempts.len(), 1, "one held attempt row");
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

    // The same rows ride genuine v4 envelopes: each immutable fixture
    // carries TWO valid rows (contract fixture authority 1323-1331), and
    // the full fixture messages decode through the v4 ingress to the
    // identical typed witnesses row-for-row.
    let logical_report = decode_request(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-member-logical.json"
    ))
    .expect("logical member envelope decodes");
    assert_eq!(logical_report.member_attempts.len(), 2);
    for (attempt, row) in logical_report
        .member_attempts
        .iter()
        .zip(logical["memberAttempts"].as_array().expect("array"))
    {
        assert_eq!(
            *attempt,
            decode_member_attempt(row).expect("logical member witness")
        );
    }
    let native_report = decode_request(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-member-native.json"
    ))
    .expect("native member envelope decodes");
    assert_eq!(native_report.member_attempts.len(), 2);
    for (attempt, row) in native_report
        .member_attempts
        .iter()
        .zip(native["memberAttempts"].as_array().expect("array"))
    {
        assert_eq!(
            *attempt,
            decode_member_attempt(row).expect("native member witness")
        );
    }
    let view_report = decode_request(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-attempt-view-native.json"
    ))
    .expect("native view envelope decodes");
    assert_eq!(view_report.view_attempts.len(), 2);
    for (attempt, row) in view_report
        .view_attempts
        .iter()
        .zip(view["viewAttempts"].as_array().expect("array"))
    {
        assert_eq!(
            *attempt,
            decode_view_attempt(row).expect("native view witness")
        );
    }

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
    // desire under the originating op, and the canonical v4 writer encodes it.
    let maximized_fact = "{\"win\":\"win-a\",\"out\":\"out\",\"rect\":[100,0,200,200],\
          \"fullscreen\":false,\"maximized\":true,\"sticky\":false,\
          \"minimized\":false,\
          \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
          \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
          \"acceptsInput\":true,\"activationInhibited\":false,\
          \"toolWindow\":false,\"owned\":false,\"transient\":true,\
          \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
          \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
          \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
          \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
          \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
          \"suppressionAvailable\":null,\
          \"visibility\":{\"visible\":true,\"suppressionOwned\":null},\
          \"lastWrite\":null}";
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
    value["foregroundKnown"] = serde_json::json!(true);
    value["command"] = serde_json::json!({"op": "move-directional", "win": "win-a", "direction": "right", "sameAxis": "group-with-neighbor"});
    value["cmdSeq"] = serde_json::Value::String("13".to_owned());
    let report = decode_request(&value.to_string()).expect("maximized command decodes");
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover keeps a row");
    assert_eq!(row.clear_operation.as_deref(), Some("op-13"));
    let wire = reply_wire(&reply);
    assert!(
        wire.contains("\"clear_op\":\"op-13\""),
        "clear desire round-trips, got {wire}"
    );
    // Estimates bound the wider wire shape.
    let request_estimate = report_estimate_v4(&report);
    assert!(
        request_estimate >= value.to_string().len(),
        "estimate {request_estimate} bounds wire {}",
        value.to_string().len()
    );
}

#[test]
fn retired_window_keys_hold_identifiable_row_with_key_retained() {
    // Every retired adapter verdict holds the identifiable row (fail
    // closed): the report decodes, the row leaves `windows`, and the
    // carried key is retained in `held_windows`.
    for key in [
        "floating",
        "fitExcluded",
        "fixedAuto",
        "fixedSuppress",
        "observedWs",
        "shell",
        "tool",
        "ownedDialog",
        "dialog",
        "noActivate",
        "cannotControl",
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        let windows = value["windows"].as_array_mut().expect("windows");
        windows[0][key] = serde_json::Value::Bool(false);
        let report = decode_request(&value.to_string()).expect("retired holds");
        assert!(report.windows.is_empty(), "{key}");
        assert_eq!(report.held_windows.len(), 1, "{key}");
        assert!(
            report.held_windows[0]
                .fields
                .iter()
                .any(|(name, _)| name == key),
            "{key} retained"
        );
    }
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
fn empty_view_attempt_refs_hold_identifiable_row() {
    // Identifiable empty refs hold narrowly in v4 (contract 307-314):
    // the row leaves `view_attempts` with its carried refs retained.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "threw",
        "pre": "", "post": "", "base": "", "attemptMs": "1",
    }]);
    let report = decode_request(&value.to_string()).expect("empty refs hold");
    assert!(report.view_attempts.is_empty(), "no valid view row");
    assert_eq!(report.held_attempts.len(), 1, "one held attempt row");
    let held = &report.held_attempts[0];
    assert_eq!(held.lane, tiler_core::world_report::AttemptLaneV4::View);
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.identity, vec!["op-9".to_owned()]);
    for key in ["pre", "post", "base"] {
        assert!(
            held.fields.iter().any(|(name, value)| name == key
                && *value == tiler_core::connection::RawSettingValue::String(String::new())),
            "{key} empty ref retained"
        );
    }
}

#[test]
fn reply_encode_matches_core_canonical_writer() {
    // Drive the engine once, then lock the transport encoding to the
    // canonical v4 writer over the same reply: exact core byte count plus
    // a decoder roundtrip to the identical typed reply.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let wire = reply_wire(&reply);
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
    // The reply carries logical workspaces (stable core ids), screens as
    // outputs, and echo views.
    let parsed: serde_json::Value = serde_json::from_str(&wire).expect("valid json");
    assert_eq!(parsed["v"], serde_json::json!(4));
    assert!(parsed["ready"].as_bool().unwrap_or(false));
    assert!(
        !parsed["workspaces"]
            .as_array()
            .expect("workspaces")
            .is_empty()
    );
    assert!(!parsed["outputs"].as_array().expect("outputs").is_empty());
}

#[test]
fn compact_automatic_effects_encode_through_v4_writer() {
    // Correlation-valid compact producer shapes at maximum widths (token
    // and revision at u64::MAX, distinct 128-byte subjects, longest
    // explicit sequence): synthetic encoder witnesses only, never a real
    // coexisting pair (same-target admission dedups in favor of the
    // explicit winner). The canonical v4 writer must spell them with
    // exact size accounting (independent core byte count plus a decoder
    // roundtrip), and the row widths must match the core reserve
    // (explicit 294, compact admission 279, pair plus comma 574).
    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{EffectCause, EffectKind, OneShotEffect};
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let admission_target = WindowId("a".repeat(128));
    let explicit_target = WindowId("b".repeat(128));
    assert_ne!(admission_target, explicit_target);
    reply.effects = vec![
        OneShotEffect {
            id: format!("af-{}", u64::MAX),
            kind: EffectKind::Focus,
            target: admission_target,
            issued_revision: u64::MAX,
            cause: EffectCause::AutomaticAdmission,
            command_seq: None,
            await_shown: false,
        },
        OneShotEffect {
            id: format!("fx-{}", u64::MAX),
            kind: EffectKind::Focus,
            target: explicit_target,
            issued_revision: u64::MAX,
            cause: EffectCause::ExplicitCommand,
            command_seq: Some(u64::MAX),
            await_shown: false,
        },
    ];
    let wire = reply_wire(&reply);
    let empty_wire = reply_wire(&{
        let mut empty = reply.clone();
        empty.effects = Vec::new();
        empty
    });
    assert_eq!(
        wire.len() - empty_wire.len(),
        279 + 294 + 1,
        "compact coexistence spells the reserved widths"
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn reply_presentation_rev_encodes_quoted_exact_order() {
    // Required revision label at maximum width: the canonical v4 writer
    // spells the quoted `u64::MAX`, the label rides immediately after the
    // foreground and before the optional suspended output, and the nested
    // label stays independent of the outer reply revision.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(
        reply.presentation.rev, reply.state_revision,
        "real publication labels its own revision"
    );
    reply.presentation.foreground = "win-a".to_owned();
    reply.presentation.rev = u64::MAX;
    reply.state_revision = 7;
    reply.presentation.suspended_output =
        Some(tiler_core::directional::OutputId("out-b".to_owned()));
    let wire = reply_wire(&reply);
    assert!(
        wire.contains(
            "\"foreground\":\"win-a\",\
             \"rev\":\"18446744073709551615\",\
             \"suspendedOutput\":\"out-b\""
        ),
        "quoted max label rides between foreground and suspended output"
    );
    assert!(
        wire.contains("\"rev\":\"7\""),
        "outer reply revision stays independent"
    );
    let estimate = reply_estimate_v4(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn reply_gesture_settled_matches_core_sizing() {
    // A reply carrying a gesture terminal encodes through the canonical
    // v4 writer with exact size accounting, and the structural estimate
    // bounds the exact bytes.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    reply.gesture_settled = Some(tiler_core::gesture::GestureSettled {
        id: 7,
        committed: true,
    });
    let wire = reply_wire(&reply);
    assert!(wire.contains("\"gestureSettled\":{\"id\":\"7\",\"committed\":true}"));
    let estimate = reply_estimate_v4(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn reply_warp_cursor_matches_core_sizing() {
    // A reply carrying a desired cursor-warp point encodes through the
    // canonical v4 writer with exact size accounting, and the structural
    // estimate bounds the exact bytes.
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    reply.cursor_warp = Some((810, 300));
    let wire = reply_wire(&reply);
    assert!(wire.contains("\"warpCursor\":[810,300]"));
    let estimate = reply_estimate_v4(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn native_current_null_strict_and_creation_attempts_decode() {
    // `current: null` means not an ordered desktop and stays valid, while
    // empty and missing currents hold the native row narrowly in v4
    // (contract 572-579 empty/missing not null state, 307-314 narrow row
    // hold, 767 native row diagnostic): the valid lane stays empty with
    // the carried distinction retained in the held fields.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": null, "occupancy": [{"native": "ws", "count": null}]}]);
    let report = decode_request(&value.to_string()).expect("null current decodes");
    assert_eq!(report.native[0].current_native, None);
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "", "occupancy": [{"native": "ws", "count": null}]}]);
    let report = decode_request(&value.to_string()).expect("empty current holds");
    assert!(report.native.is_empty(), "no valid native row");
    assert_eq!(report.held_native.len(), 1, "one held native row");
    assert!(
        report.held_native[0]
            .fields
            .iter()
            .any(|(name, field)| name == "current"
                && *field == tiler_core::connection::RawSettingValue::String(String::new())),
        "empty spelling retained"
    );
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "occupancy": [{"native": "ws", "count": null}]}]);
    let report = decode_request(&value.to_string()).expect("missing current holds");
    assert!(report.native.is_empty(), "no valid native row");
    assert_eq!(report.held_native.len(), 1, "one held native row");
    assert!(
        !report.held_native[0]
            .fields
            .iter()
            .any(|(name, _)| name == "current"),
        "missing stays missing"
    );
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
    let estimate = report_estimate_v4(&report);
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
        matches!(engine.handle_report(&report, &[]).0, WorldOutcome::Rejected { message, .. } if message == "native-without-capability")
    );
}

#[test]
fn golden_bootstrap_reply_bytes_lock_wire_shape() {
    // The discovery reply is fixed shape: v4, ready false, empty sections.
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
    // Pre-settings report decodes yet bootstraps: correlated, not ready,
    // no desired state, no effects.
    let pre_settings = decode_request(&one_window_request()).expect("decodes");
    let reply = match engine.handle_report(&pre_settings, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected bootstrap, got {other:?}"),
    };
    assert!(!reply.ready);
    assert!(reply.scopes.is_empty());
    assert!(reply.effects.is_empty());
    let wire = reply_wire(&reply);
    assert_eq!(
        wire,
        "{\"v\":4,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\"req\":\"req-1\",\
        \"obs\":\"1\",\"rev\":\"0\",\"ready\":false,\"consumed\":false,\"scopes\":[],\
        \"workspaces\":[],\"outputs\":[],\"windows\":[],\"transfers\":[],\"effects\":[],\
        \"closeFocus\":null,\"result\":null,\"gestureSettled\":null,\"operationSettled\":[],\"nativeCreations\":[],\
        \"warpCursor\":null,\"presentation\":{\"foreground\":\"\",\"rev\":\"0\",\
        \"border\":{\"visible\":false,\"reason\":\"\",\"subject\":null,\"outsets\":null,\
        \"width\":null,\"radius\":null,\"witness\":null},\
        \"underlay\":{\"status\":\"hidden\",\"reason\":\"no-group\",\"baseRev\":null},\"pager\":[]},\
        \"intentWrites\":[],\"switchSettled\":[],\"diagnostics\":[]}"
    );
}

#[test]
fn reply_operation_settled_matches_core_sizing() {
    // A reply carrying terminal retained-operation events encodes
    // through the canonical v4 writer, and the structural estimate bounds the
    // exact bytes. Pending desire rides the window row, actual attempts
    // ride the world-apply diag, holds emit nothing, a clear continuation
    // means the intent resumed (never that the final frame arrived), and
    // only a float frame-proven event means observation confirmed the
    // float target.
    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{OperationEvent, OperationOutcome};
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
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
    let wire = reply_wire(&reply);
    assert!(wire.contains("\"operationSettled\":["));
    assert!(wire.contains("\"outcome\":\"continued-tiled\""));
    assert!(wire.contains("\"outcome\":\"continuation-refused\""));
    assert!(wire.contains("\"outcome\":\"float-frame-proven\""));
    assert!(wire.contains("\"outcome\":\"float-abandoned\""));
    assert!(wire.contains("\"cause\":\"constraints\""));
    assert!(wire.contains("\"cause\":\"closed\""));
    let estimate = reply_estimate_v4(&reply);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
}

#[test]
fn unknown_valid_output_stays_valid_for_core_hold() {
    // v4 premise: an unknown-but-valid output is not structurally
    // invalid (the retired `window-output-unknown` refusal is gone).
    // The fact decodes with its carried output; the core retains the
    // prior home and holds the scope.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    let window = v4_window("win-a", "nope", [0, 0, 10, 10]);
    value["windows"] = serde_json::json!([window]);
    // Unknown output screens must exist for the fact: add the screen and
    // the fact validates structurally; core then holds (not tested here).
    value["screens"] = serde_json::json!([
        {"out": "out", "full": [0, 0, 800, 600], "work": [0, 0, 800, 560],
         "complete": true, "scale": 1.0,
         "identity": {"manufacturer": null, "model": null, "serial": null, "connector": null}},
    ]);
    let report = decode_request(&value.to_string()).expect("unknown valid out decodes");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(
        report.windows[0].output.as_ref().map(|out| out.0.as_str()),
        Some("nope")
    );
}

#[test]
fn unknown_restoration_key_refuses() {
    // No restoration ingress exists on the wire: a present `restored`
    // key is unknown and refuses before any semantic check. Reports
    // without the key decode.
    let report = decode_request(&one_window_request()).expect("decodes");
    let estimate = report_estimate_v4(&report);
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
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
    );
}

#[test]
fn native_ws_field_decodes_and_gates() {
    use tiler_core::world_report::NativeWorkspaceFact;
    // Missing nativeWs stays omitted (native-free reports predate it).
    let report = decode_request(&one_window_request()).expect("missing stays omitted");
    assert_eq!(
        report.windows[0].native_workspace,
        NativeWorkspaceFact::Omitted
    );
    let with_native = |native: serde_json::Value| {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["native"] = serde_json::json!([{"out": "out", "order": ["d1"], "current": "d1", "occupancy": [{"native": "d1", "count": null}]}]);
        let mut window = v4_window("win-a", "out", [0, 0, 200, 200]);
        window["nativeWs"] = native;
        value["windows"] = serde_json::json!([window]);
        decode_request(&value.to_string())
    };
    // Null, empty, and missing are distinct spellings of the same unknown.
    let report = with_native(serde_json::Value::Null).expect("null decodes");
    assert_eq!(
        report.windows[0].native_workspace,
        NativeWorkspaceFact::Null
    );
    assert_eq!(report.windows[0].native_workspace.as_deref(), None);
    let report = with_native(serde_json::Value::String(String::new())).expect("empty decodes");
    assert_eq!(
        report.windows[0].native_workspace,
        NativeWorkspaceFact::Value(String::new())
    );
    assert_eq!(report.windows[0].native_workspace.as_deref(), None);
    // A valid opaque native id decodes.
    let report = with_native(serde_json::Value::String("d1".to_owned())).expect("id decodes");
    assert_eq!(report.windows[0].native_workspace.as_deref(), Some("d1"));
    // Non-id and non-string native membership holds the identifiable row
    // narrowly in v4 (contract 307-314, 393): the bad spelling rides the
    // held row with its raw bytes retained, never a silent drop or
    // envelope refusal.
    for (label, native) in [
        ("bad id", serde_json::Value::String("not an id!".to_owned())),
        ("number", serde_json::json!(7)),
    ] {
        let report = with_native(native.clone()).expect("bad spelling holds");
        assert!(report.windows.is_empty(), "{label}: no valid row");
        assert_eq!(report.held_windows.len(), 1, "{label}: one held row");
        let held = &report.held_windows[0];
        assert_eq!(
            held.kind,
            tiler_core::world_report::RowDiagnosticKind::InvalidFact,
            "{label}"
        );
        assert!(
            held.fields.iter().any(|(name, _)| name == "nativeWs"),
            "{label}: bad spelling retained"
        );
    }
    // Capability off with native evidence refuses at the core gate.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    let mut window = v4_window("win-a", "out", [0, 0, 200, 200]);
    window["nativeWs"] = serde_json::Value::String("d1".to_owned());
    value["windows"] = serde_json::json!([window]);
    let report = decode_request(&value.to_string()).expect("native fact decodes");
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixture carries native sticky and maximize facts; only
            // host-owned domains stay off so the native gate still refuses.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    assert!(
        matches!(engine.handle_report(&report, &[]).0, WorldOutcome::Rejected { message, .. } if message == "native-without-capability")
    );
}

#[test]
fn native_ws_estimate_bounds_actual_wire() {
    // The structural estimate accounts for the nativeWs field: it bounds
    // a wire message carrying a realistic Space-style id.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["9f1c-0005-space"], "current": "9f1c-0005-space", "occupancy": [{"native": "9f1c-0005-space", "count": null}]}]);
    let mut window = v4_window("win-a", "out", [0, 0, 200, 200]);
    window["nativeWs"] = serde_json::Value::String("9f1c-0005-space".to_owned());
    value["windows"] = serde_json::json!([window]);
    let wire = value.to_string();
    assert!(wire.contains("\"nativeWs\":\"9f1c-0005-space\""));
    let report = decode_request(&wire).expect("native report decodes");
    assert_eq!(
        report.windows[0].native_workspace.as_deref(),
        Some("9f1c-0005-space")
    );
    let estimate = report_estimate_v4(&report);
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
    // A present opaque id decodes; a bad-grammar string refuses in the
    // structural preflight while ambiguous (non-string) shapes hold.
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
    let estimate = report_estimate_v4(&report);
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
    // Non-id string receipts refuse in the structural preflight, while
    // non-string receipts (numeric, array) hold the identifiable row
    // narrowly in v4 (contract 1006-1016 unknown/ambiguous identity hold,
    // 307-314 narrow hold): the valid lane stays empty with the carried
    // value retained.
    let (bad_wire, _) = with_created(serde_json::json!("not an id!"));
    assert!(
        decode_request(&bad_wire).is_err(),
        "bad grammar string refuses"
    );
    for bad in [serde_json::json!(7), serde_json::json!(["ws-new"])] {
        let (wire, _) = with_created(bad.clone());
        let report = decode_request(&wire).expect("ambiguous receipt holds");
        assert!(report.creation_attempts.is_empty(), "no valid creation row");
        assert_eq!(report.held_attempts.len(), 1, "one held attempt row");
        let held = &report.held_attempts[0];
        assert_eq!(held.lane, tiler_core::world_report::AttemptLaneV4::Creation);
        assert_eq!(held.ordinal, 0);
        assert_eq!(held.identity, vec!["op-native-1".to_owned()]);
    }
    // Unknown receipt fields hold the identifiable row with the carried
    // key retained instead of refusing the envelope.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["native"] = serde_json::json!([{"out": "out", "order": ["ws"], "current": "ws", "occupancy": [{"native": "ws", "count": null}]}]);
    value["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": ["ws"], "after": ["ws"], "createdId": "ws-new", "attemptMs": "5",
    }]);
    let report = decode_request(&value.to_string()).expect("unknown receipt field holds");
    assert!(report.creation_attempts.is_empty(), "no valid creation row");
    assert_eq!(report.held_attempts.len(), 1, "one held attempt row");
    let held = &report.held_attempts[0];
    assert_eq!(held.lane, tiler_core::world_report::AttemptLaneV4::Creation);
    assert_eq!(held.identity, vec!["op-native-1".to_owned()]);
    assert!(
        held.fields.iter().any(|(name, _)| name == "createdId"),
        "carried key retained"
    );
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
            v4_window("win-a", "out-a", [10, 0, 200, 200]),
            v4_window("win-b", "out-b", [810, 0, 200, 200]),
        ]);
        value["actionForeground"] = serde_json::json!("out-a");
        value["actionOutputs"] = serde_json::json!(["out-a", "out-b"]);
        // No `views` lane: v4 refuses it as an unknown field.
        value["focused"] = serde_json::json!("");
        value["foregroundKnown"] = serde_json::json!(false);
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
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let seed = decode_request(&two_output_envelope("1", "req-1", None)).expect("seed decodes");
    match engine.handle_report(&seed, &[]).0 {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted seed, got {other:?}"),
    }
    let report = decode_request(&two_output_envelope(
        "2",
        "req-2",
        Some(serde_json::json!({"op": "focus-directional", "win": "", "direction": "right"})),
    ))
    .expect("no-focus command decodes");
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let focus: Vec<&tiler_core::world_report::OneShotEffect> = reply
        .effects
        .iter()
        .filter(|effect| effect.is_explicit_focus_for(11))
        .collect();
    assert_eq!(focus.len(), 1, "one crossing effect: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "win-b");
    assert!(reply.command_result.is_none());
    reply_wire(&reply);
}

#[test]
fn explicit_focus_effect_wire_carries_cause_metadata() {
    // Typed effect vocabulary slice (baseline a2aa3197): the contract
    // requires cause/cmdSeq/awaitShown on every OneShotEffect row. The
    // explicit crossing focus must ride the wire as cause
    // explicit-command with the quoted command seq and awaitShown false.
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
            v4_window("win-a", "out-a", [10, 0, 200, 200]),
            v4_window("win-b", "out-b", [810, 0, 200, 200]),
        ]);
        value["actionForeground"] = serde_json::json!("out-a");
        value["actionOutputs"] = serde_json::json!(["out-a", "out-b"]);
        // No `views` lane: v4 refuses it as an unknown field.
        value["focused"] = serde_json::json!("");
        value["foregroundKnown"] = serde_json::json!(false);
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
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let seed = decode_request(&two_output_envelope("1", "req-1", None)).expect("seed decodes");
    match engine.handle_report(&seed, &[]).0 {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted seed, got {other:?}"),
    }
    let report = decode_request(&two_output_envelope(
        "2",
        "req-2",
        Some(serde_json::json!({"op": "focus-directional", "win": "", "direction": "right"})),
    ))
    .expect("no-focus command decodes");
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let focus: Vec<&tiler_core::world_report::OneShotEffect> = reply
        .effects
        .iter()
        .filter(|effect| effect.is_explicit_focus_for(11))
        .collect();
    assert_eq!(focus.len(), 1, "one crossing effect: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "win-b");
    assert_eq!(
        focus[0].cause,
        tiler_core::world_report::EffectCause::ExplicitCommand,
        "typed cause: {:?}",
        focus[0]
    );
    assert_eq!(focus[0].command_seq, Some(11));
    assert!(!focus[0].await_shown);
    let wire = reply_wire(&reply);
    let wire_value: serde_json::Value = serde_json::from_str(&wire).expect("wire is JSON");
    let row = &wire_value["effects"][0];
    assert_eq!(
        row["cause"],
        serde_json::json!("explicit-command"),
        "wire cause: {row}"
    );
    assert_eq!(row["cmdSeq"], serde_json::json!("11"), "wire cmdSeq: {row}");
    assert_eq!(
        row["awaitShown"],
        serde_json::json!(false),
        "wire awaitShown: {row}"
    );
}

#[test]
fn toggle_maximize_decodes_and_result_encodes() {
    // Wire toggle-maximize decodes to the typed command, and the focused
    // plain tile publishes the Maximize result through the canonical v4 writer with
    // exact result sizing. Malformed/missing win refuses.
    use tiler_core::world_report::CommandResult as R;
    use tiler_core::world_report::WorldCommand as C;
    let command = serde_json::json!({"op": "toggle-maximize", "win": "win-a"});
    let mut value: serde_json::Value =
        serde_json::from_str(&with_command(&command, "7")).expect("valid");
    value["focused"] = serde_json::Value::String("win-a".to_owned());
    value["foregroundKnown"] = serde_json::json!(true);
    let report = decode_request(&value.to_string()).expect("toggle-maximize decodes");
    assert!(
        matches!(report.command, Some(C::ToggleMaximize { ref window }) if window.0 == "win-a")
    );
    assert_eq!(report.command_seq, Some(7));
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let reply = match engine.handle_report(&report, &[]).0 {
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
    let wire = reply_wire(&reply);
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
    let estimate = reply_estimate_v4(&reply);
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
fn sticky_commands_decode_typed_and_rejected_result_encodes_exact() {
    // Neutral sticky wire ops decode to the typed world commands through
    // the staged envelope: exact required keys, unknown keys refuse, wrong
    // types refuse, non-bool values refuse. The typed validation and digest
    // distinguish the rows, and the rejected result keeps the frozen shape
    // through the canonical v4 writer with exact sizing.
    use tiler_core::world_report::CommandRejectKind as K;
    use tiler_core::world_report::CommandResult as R;
    use tiler_core::world_report::WorldCommand as C;
    let set = serde_json::json!({"op": "set-sticky", "win": "win-a", "value": true});
    let report = decode_request(&with_command(&set, "7")).expect("set-sticky decodes");
    assert!(matches!(
        report.command,
        Some(C::SetSticky { ref window, value: true }) if window.0 == "win-a"
    ));
    assert_eq!(report.command_seq, Some(7));
    assert!(tiler_core::world_report::valid_command(
        report.command.as_ref().expect("command")
    ));
    let toggle = serde_json::json!({"op": "toggle-sticky", "win": "win-a"});
    let toggled = decode_request(&with_command(&toggle, "7")).expect("toggle-sticky decodes");
    assert!(matches!(
        toggled.command,
        Some(C::ToggleSticky { ref window }) if window.0 == "win-a"
    ));
    // Required keys stay required; unknown keys refuse.
    for bad in [
        serde_json::json!({"op": "set-sticky", "win": "win-a"}),
        serde_json::json!({"op": "set-sticky", "value": true}),
        serde_json::json!({"op": "toggle-sticky"}),
        serde_json::json!({"op": "set-sticky", "win": "win-a", "value": true, "extra": 1}),
        serde_json::json!({"op": "toggle-sticky", "win": "win-a", "value": true}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err());
    }
    // Wrong types refuse: null/number/empty wins, non-bool values.
    for bad in [
        serde_json::json!({"op": "set-sticky", "win": null, "value": true}),
        serde_json::json!({"op": "set-sticky", "win": 7, "value": true}),
        serde_json::json!({"op": "set-sticky", "win": "", "value": true}),
        serde_json::json!({"op": "set-sticky", "win": " ", "value": true}),
        serde_json::json!({"op": "set-sticky", "win": "win-a", "value": null}),
        serde_json::json!({"op": "set-sticky", "win": "win-a", "value": 1}),
        serde_json::json!({"op": "set-sticky", "win": "win-a", "value": "true"}),
        serde_json::json!({"op": "toggle-sticky", "win": null}),
        serde_json::json!({"op": "toggle-sticky", "win": 7}),
        serde_json::json!({"op": "toggle-sticky", "win": ""}),
    ] {
        assert!(decode_request(&with_command(&bad, "7")).is_err());
    }
    // Typed rows digest distinctly: value and op both separate.
    let set_false = serde_json::json!({"op": "set-sticky", "win": "win-a", "value": false});
    let off = decode_request(&with_command(&set_false, "7")).expect("set-sticky false decodes");
    let digest_on = report_digest_v4(&report);
    let digest_off = report_digest_v4(&off);
    let digest_toggle = report_digest_v4(&toggled);
    assert_ne!(digest_on, digest_off);
    assert_ne!(digest_on, digest_toggle);
    assert_ne!(digest_off, digest_toggle);
    // Frozen rejection shape through the canonical v4 writer, exact sizing.
    for kind in [
        K::OperationRefused,
        K::StateUnsupported,
        K::StaleRenderTarget,
    ] {
        let result = Some(R::Rejected { kind });
        let mut encoded = String::new();
        tiler_core::world_sizing::encode_command_result(&mut encoded, &result);
        assert_eq!(
            encoded,
            format!("{{\"op\":\"rejected\",\"kind\":\"{}\"}}", kind.as_str())
        );
        assert_eq!(
            tiler_core::world_sizing::command_result_estimate(&result),
            encoded.len() + 10,
            "result estimate is exact: {encoded}"
        );
        let staged = decode_request(&one_window_request()).expect("decodes");
        let mut engine = connected_engine("gen-1", HostCapabilities::default());
        let mut reply = match engine.handle_report(&staged, &[]).0 {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        reply.command_result = result;
        let wire = reply_wire(&reply);
        assert!(
            wire.contains(&format!(
                "\"op\":\"rejected\",\"kind\":\"{}\"",
                kind.as_str()
            )),
            "rejected result round-trips, got {wire}"
        );
        let estimate = reply_estimate_v4(&reply);
        assert!(
            estimate >= wire.len(),
            "estimate {estimate} bounds wire {}",
            wire.len()
        );
        assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
    }
}

#[test]
fn reply_trailing_marker_encodes_on_v4_writer() {
    // The explicit trailing marker rides only when true: the canonical v4
    // writer emits `,"trailing":true` on marked plan and pager rows, omits
    // the key otherwise, and the structural estimate bounds the exact bytes.
    use tiler_core::directional::OutputId;
    use tiler_core::world_report::{AuthorityScope, TilingMode};
    let report = decode_request(&one_window_request()).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    // The engine reply already publishes plan rows, and the v4 writer
    // refuses duplicate workspace ids (contract 863-867 dedup rule), so
    // the crafted rows replace the published plan: exactly the two rows
    // below ride the wire.
    reply.workspaces.clear();
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
    let wire = reply_wire(&reply);
    assert_eq!(
        wire.matches(",\"trailing\":true").count(),
        2,
        "one plan plus one pager key"
    );
    assert!(!wire.contains("\"trailing\":false"));
    let estimate = reply_estimate_v4(&reply);
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
    use tiler_protocol::world_protocol::decode_authority_scope;

    // Workspace scopes: the immutable fixture reply decodes through the
    // genuine v4 reply ingress to the expected typed scopes/workspaces,
    // and the canonical v4 writer spells them back with exact size
    // accounting plus a decoder roundtrip.
    let fixture_text = include_str!(
        "../../../test-fixtures/world-wire/v4/messages/reply-untouched-workspace-scope.json"
    );
    let fixture: serde_json::Value = serde_json::from_str(fixture_text).expect("fixture json");
    let decoded = decode_reply_v4(fixture_text).expect("fixture reply decodes");
    let mut expected_scopes = Vec::new();
    let mut expected_workspaces = Vec::new();
    for (workspace, authority) in [
        ("ws-a", ScopeAuthority::Controlled),
        ("ws-spare", ScopeAuthority::Untouched),
    ] {
        let scope = AuthorityScope::Workspace {
            output: OutputId("out-a".into()),
            workspace: WorkspaceId(workspace.into()),
        };
        expected_scopes.push(ScopeStatus {
            scope: scope.clone(),
            authority,
        });
        expected_workspaces.push(WorkspaceDesire {
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
            decode_authority_scope(&fixture["scopes"][expected_scopes.len() - 1]["scope"]),
            Ok(scope)
        );
    }
    assert_eq!(decoded.reply.scopes, expected_scopes);
    assert_eq!(decoded.reply.workspaces, expected_workspaces);
    let wire = encode_reply_v4(&decoded).expect("reply encodes");
    assert_eq!(
        wire.len(),
        reply_exact_bytes_v4(&decoded.reply),
        "core byte count is exact over the wire"
    );
    assert_eq!(
        decode_reply_v4(&wire).expect("reply round-trips"),
        decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    let encoded: serde_json::Value = serde_json::from_str(&wire).expect("reply json");
    assert_eq!(encoded["scopes"], fixture["scopes"]);
    assert_eq!(encoded["workspaces"], fixture["workspaces"]);

    // Output scope: same fixture-first conformance over the observe reply.
    let output_text =
        include_str!("../../../test-fixtures/world-wire/v4/messages/reply-observe.json");
    let output_fixture: serde_json::Value =
        serde_json::from_str(output_text).expect("output scope fixture json");
    let output_decoded = decode_reply_v4(output_text).expect("observe reply decodes");
    let output_scope = AuthorityScope::Output(OutputId("out-a".into()));
    assert_eq!(
        decode_authority_scope(&output_fixture["scopes"][0]["scope"]),
        Ok(output_scope.clone())
    );
    assert_eq!(
        output_decoded.reply.scopes,
        vec![ScopeStatus {
            scope: output_scope.clone(),
            authority: ScopeAuthority::Controlled,
        }]
    );
    let mut expected_output_workspaces = Vec::new();
    for (workspace, order, trailing) in [("ws-a", 0, false), ("ws-spare", 1, true)] {
        expected_output_workspaces.push(WorkspaceDesire {
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
    assert_eq!(output_decoded.reply.workspaces, expected_output_workspaces);
    let output_wire = encode_reply_v4(&output_decoded).expect("output reply encodes");
    assert_eq!(
        output_wire.len(),
        reply_exact_bytes_v4(&output_decoded.reply),
        "core byte count is exact over the wire"
    );
    assert_eq!(
        decode_reply_v4(&output_wire).expect("output reply round-trips"),
        output_decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    let output_encoded: serde_json::Value =
        serde_json::from_str(&output_wire).expect("output reply json");
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
    let outcome = engine.handle_report(&report, &[]).0;
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
    let bootstrap = match engine.handle_report(&pre_settings, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected bootstrap, got {other:?}"),
    };
    assert!(!bootstrap.ready);
    assert!(bootstrap.scopes.is_empty());
    assert!(bootstrap.effects.is_empty());
    assert_eq!(
        bootstrap.presentation.rev, bootstrap.state_revision,
        "empty bootstrap mirrors the current revision"
    );
    let bootstrap_wire = reply_wire(&bootstrap);
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
    let ready = match engine.handle_report(&ready_report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected ready reply, got {other:?}"),
    };
    assert!(ready.ready);
    assert!(engine.is_ready());
    assert_eq!(
        ready.presentation.rev, ready.state_revision,
        "ready publication labels its own revision"
    );
    let ready_wire = reply_wire(&ready);
    assert!(ready_wire.contains("\"ready\":true"));
    let parsed: serde_json::Value = serde_json::from_str(&ready_wire).expect("valid json");
    assert_eq!(parsed["v"], serde_json::json!(4));
    assert!(parsed["ready"].as_bool().unwrap_or(false));
    assert!(
        !parsed["workspaces"]
            .as_array()
            .expect("workspaces")
            .is_empty()
    );
    assert!(!parsed["outputs"].as_array().expect("outputs").is_empty());
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
    let command_wire = with_command(&serde_json::json!({"op": "focus", "win": "win-a"}), "7");
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
    let escaped_v = wire.replacen("\"v\":4", "\"\\u0076\":4", 1);
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

    fn request_with(screens: &str, windows: &str, extra_top: &str) -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["screens"] = serde_json::from_str(screens).expect("screens json");
        value["windows"] = serde_json::from_str(windows).expect("windows json");
        // No `views` lane: v4 refuses it as an unknown field.
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
    let estimate = report_estimate_v4(&report);
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
        let wire = request_with(screens, "[]", "");
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
        let estimate = report_estimate_v4(&report);
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
        let wire = request_with(screens, "[]", "");
        assert!(decode_request(&wire).is_err(), "{name} refuses");
    }
    let null_primary = request_with(
        r#"[{"out":"out","full":null,"work":null,"complete":false,"scale":1.0,"identity":{"connector":null,"manufacturer":null,"model":null,"serial":null}}]"#,
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
    assert!(validate_report_v4_reason(&report).is_none());
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
        AuthorityScope, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority,
    };
    use tiler_core::world_report_v4_checks::grade_invalid_screens_v4;

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
        splice_lane(&wire, "actionOutputs", "[]")
    }
    fn check_scenario(
        name: &str,
        bad_row: &str,
        mutate: impl FnOnce(&mut WorldReportV4),
        expected_held: usize,
    ) {
        let decoded_wire = splice_lane(
            &two_healthy_wire(),
            "screens",
            &format!("[{},{}]", healthy_row("out-a"), bad_row),
        );
        let mut decoded: WorldReportV4 = decode_request(&decoded_wire).expect("held row decodes");
        assert_eq!(decoded.screens.len(), 1, "{name}: one valid row");
        assert_eq!(decoded.held_screens.len(), 1, "{name}: one held row");
        assert_eq!(decoded.held_screens[0].output.0, "out-b", "{name}");
        assert_eq!(
            decoded.held_screens[0].ordinal, 1,
            "{name}: ordinal preserved"
        );

        let mut direct: WorldReportV4 =
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
            grade_invalid_screens_v4(&direct),
            Some(decoded.clone()),
            "{name}: graded direct report equals decoded report"
        );

        let mut engine_held = connected_engine("gen-1", HostCapabilities::default());
        let mut engine_direct = connected_engine("gen-1", HostCapabilities::default());
        let seed: WorldReportV4 = decode_request(&two_healthy_wire()).expect("seed decodes");
        for engine in [&mut engine_held, &mut engine_direct] {
            match engine.handle_report(&seed, &[]).0 {
                WorldOutcome::Accepted(reply) => assert!(reply.ready, "{name}: seed latches"),
                other => panic!("{name}: seed must latch readiness, got {other:?}"),
            }
        }
        let held_reply = match engine_held.handle_report(&decoded, &[]).0 {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("{name}: decoded hold must be accepted, got {other:?}"),
        };
        let direct_reply = match engine_direct.handle_report(&direct, &[]).0 {
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
    use tiler_core::world_report::WorldReportV4;

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
        mutate: impl FnOnce(&mut WorldReportV4),
        reason: &str,
    ) {
        let wire = stage_fixture_rows(
            &base_rows.iter().map(String::as_str).collect::<Vec<_>>(),
            None,
        );
        let mut direct: WorldReportV4 = decode_request(&wire).expect("{name}: base decodes");
        mutate(&mut direct);
        assert_eq!(
            validate_report_v4_reason(&direct),
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

#[test]
fn bad_process_grammar_holds_with_invalid_kind_parity() {
    use tiler_core::world_report::RowDiagnosticKind;
    // Bad process grammar holds (typed parity): the row leaves `windows`
    // with its carried pid retained and an InvalidFact kind.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"][0]["processId"] = serde_json::json!("has space");
    let report = decode_request(&value.to_string()).expect("bad process holds");
    assert!(report.windows.is_empty());
    assert_eq!(report.held_windows.len(), 1);
    let held = &report.held_windows[0];
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact);
    let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
    assert_eq!(diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(
        held.fields.iter().find(|(k, _)| k == "processId"),
        Some(&(
            "processId".to_owned(),
            tiler_core::connection::RawSettingValue::String("has space".to_owned())
        ))
    );
}

#[test]
fn masked_owner_with_overbound_refuses_decoder() {
    // A hold cause (retired key) must not mask a bad owner reference.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"][0]["floating"] = serde_json::Value::Bool(false);
    value["windows"][0]["owner"] = serde_json::json!("has space");
    let error = decode_request(&value.to_string()).expect_err("masked owner refuses");
    assert_eq!(error.kind, "unknown-value");
}

#[test]
fn masked_owner_with_bad_process_refuses_decoder() {
    // Bad process holds, but a bad owner in the same row still refuses.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"][0]["processId"] = serde_json::json!("has space");
    value["windows"][0]["owner"] = serde_json::json!("also bad!");
    assert!(decode_request(&value.to_string()).is_err());
}

#[test]
fn output_null_decodes_without_fabricated_scope() {
    // `out: null` decodes with no output authority: the held/valid row
    // retains evidence but names no scope.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["windows"][0]["out"] = serde_json::Value::Null;
    let report = decode_request(&value.to_string()).expect("null out decodes");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].output, None);
}

/// Stage exact nested window-row bytes into the current parent report shape:
/// outputs retargeted to `out-a` so staged rows stay homed, while every
/// window row is spliced textually from the immutable fixture so no `Value`
/// round-trip can sort keys or respell numbers.
fn stage_fixture_window_rows(rows: &[&str]) -> String {
    let wire = one_window_request()
        .replace("\"out\":\"out\"", "\"out\":\"out-a\"")
        .replace("[\"out\"]", "[\"out-a\"]");
    splice_lane(&wire, "windows", &format!("[{}]", rows.join(",")))
}

/// Immutable v4 message bytes pin the canonical window-row spelling: a
/// healthy row and a nullable row must re-encode to their exact carried
/// bytes (LastWrite/VisibilitySample nested key order, counters as decimal
/// strings, preserved valid bytes), while a retired-key row holds narrowly
/// with its exact carried bytes. Expected bytes come from the fixture files
/// themselves, never from a `Value` re-serialization or a hand-authored row.
#[test]
fn v4_window_rows_retain_canonical_bytes_from_immutable_fixtures() {
    use tiler_core::world_report::{
        RowDiagnosticKind, RowDiagnosticLane, encode_held_window_canonical, encode_window_canonical,
    };

    fn raw_window(message: &str) -> String {
        let rows = raw_lane_elements(message, "windows");
        assert_eq!(rows.len(), 1, "one raw row");
        rows[0].to_owned()
    }

    // 1. Healthy row: full `lastWrite` with pre/post samples.
    let healthy_raw = raw_window(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-last-write-accepted.json"
    ));
    assert!(
        !healthy_raw.contains("\"nativeWs\""),
        "ordinary row omits nativeWs"
    );
    let staged = stage_fixture_window_rows(&[&healthy_raw]);
    let report = decode_request(&staged).expect("healthy row decodes");
    assert_eq!(report.windows.len(), 1);
    assert!(report.held_windows.is_empty());
    let fact = &report.windows[0];
    assert_eq!(
        fact.native_workspace,
        tiler_core::world_report::NativeWorkspaceFact::Omitted
    );
    let last = fact.last_write.as_ref().expect("lastWrite rides");
    assert_eq!(
        last.property,
        tiler_core::world_report::LastWriteProperty::Visibility
    );
    assert_eq!(last.revision, 1);
    assert_eq!(last.path, tiler_core::world_report::LastWritePath::Staged);
    assert_eq!(
        last.setter,
        tiler_core::world_report::VisibilitySetter::Accepted
    );
    assert_eq!(last.pre.as_ref().expect("pre").obs, 1);
    assert_eq!(last.post.as_ref().expect("post").obs, 1);
    assert_eq!(
        encode_window_canonical(fact).expect("finite row"),
        healthy_raw,
        "lossless healthy canonical bytes"
    );
    assert!(validate_report_v4_reason(&report).is_none());
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= staged.len(),
        "estimate {estimate} bounds wire {}",
        staged.len()
    );
    assert_eq!(
        report_digest_v4(&report),
        report_digest_v4(&decode_request(&staged).expect("redigest")),
        "digest stable over identical bytes"
    );

    // 2. Nullable row: `pre`/`post` null samples stay unknown, never defaults.
    let nullable_raw = raw_window(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/report-last-write-preimage-unavailable.json"
    ));
    let staged = stage_fixture_window_rows(&[&nullable_raw]);
    let report = decode_request(&staged).expect("nullable row decodes");
    assert_eq!(report.windows.len(), 1);
    assert!(report.held_windows.is_empty());
    let fact = &report.windows[0];
    let last = fact.last_write.as_ref().expect("lastWrite rides");
    assert!(last.pre.is_none() && last.post.is_none());
    assert_eq!(
        encode_window_canonical(fact).expect("finite row"),
        nullable_raw,
        "lossless nullable canonical bytes"
    );
    assert!(validate_report_v4_reason(&report).is_none());
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= staged.len(),
        "estimate {estimate} bounds wire {}",
        staged.len()
    );
    assert_ne!(
        report_digest_v4(
            &decode_request(&stage_fixture_window_rows(&[&healthy_raw])).expect("healthy")
        ),
        report_digest_v4(&report),
        "digest separates healthy from nullable"
    );

    // 3. Held row: retired `floating` verdict holds the identifiable row with
    // its carried key retained (fail closed, never a silent drop).
    let held_raw = raw_window(include_str!(
        "../../../test-fixtures/world-wire/v4/messages/invalid-removed-window-floating.json"
    ));
    let staged = stage_fixture_window_rows(&[&held_raw]);
    let report = decode_request(&staged).expect("retired holds");
    assert!(report.windows.is_empty(), "no valid row");
    assert_eq!(report.held_windows.len(), 1, "one held row");
    let held = &report.held_windows[0];
    assert_eq!(held.window.0, "win-a");
    assert_eq!(held.ordinal, 0, "original windows index");
    assert_eq!(
        held.output.as_ref().map(|out| out.0.as_str()),
        Some("out-a")
    );
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact);
    let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
    assert_eq!(diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(diagnostic.row, RowDiagnosticLane::Window);
    assert_eq!(diagnostic.id.as_deref(), Some("win-a"));
    assert!(
        held.fields.iter().any(|(name, value)| name == "floating"
            && *value == tiler_core::connection::RawSettingValue::Bool(false)),
        "retired key retained"
    );
    assert_eq!(
        encode_held_window_canonical(held).expect("finite canonical row"),
        held_raw,
        "lossless held canonical bytes"
    );
    assert!(validate_report_v4_reason(&report).is_none());
    let estimate = report_estimate_v4(&report);
    assert!(
        estimate >= staged.len(),
        "estimate {estimate} bounds wire {}",
        staged.len()
    );
}

/// The valid writer spells lexical escapes canonically, emits `hints: null`
/// for unknown hints, and preserves `nativeWs` spelling exactly: absent stays
/// absent, `null` stays `null`, `""` stays `""`, a known id stays itself.
/// A prior revision asserted null/empty collapse to omission; that expectation
/// was incorrect and is replaced here with exact preservation. No fixture is
/// weakened: the immutable healthy row still omits, and each spelling
/// round-trips to its own exact bytes with a distinct digest.
#[test]
fn v4_window_canonical_spells_escapes_and_preserves_native_ws() {
    use tiler_core::world_report::{NativeWorkspaceFact, encode_window_canonical};

    let healthy_raw = {
        let message = include_str!(
            "../../../test-fixtures/world-wire/v4/messages/report-last-write-accepted.json"
        );
        let rows = raw_lane_elements(message, "windows");
        assert_eq!(rows.len(), 1);
        rows[0].to_owned()
    };
    let staged = stage_fixture_window_rows(&[&healthy_raw]);
    let report = decode_request(&staged).expect("healthy row decodes");
    let mut fact = report.windows[0].clone();
    fact.title = Some("a\"b\\c\u{1}é".to_owned());
    fact.hints = None;
    let wire = encode_window_canonical(&fact).expect("finite row");
    assert!(
        wire.contains("\"title\":\"a\\\"b\\\\c\\u0001é\""),
        "lexical escapes spell canonically, got {wire}"
    );
    assert!(wire.contains("\"hints\":null"), "unknown hints spell null");
    assert!(
        !wire.contains("\"nativeWs\""),
        "omitted nativeWs stays omitted"
    );
    // A re-decode of the escaped bytes preserves the text exactly.
    let restaged = stage_fixture_window_rows(&[&wire]);
    let roundtrip = decode_request(&restaged).expect("escaped row decodes");
    assert_eq!(roundtrip.windows[0].title.as_deref(), Some("a\"b\\c\u{1}é"));
    assert_eq!(roundtrip.windows[0].hints, None);

    // Each `nativeWs` spelling decodes to its typed twin and re-encodes to
    // its exact carried bytes; digests stay distinct for exact request
    // identity even though all three unknown spellings mean the same
    // semantically.
    let cases: &[(&str, NativeWorkspaceFact, &str)] = &[
        (
            "omitted",
            NativeWorkspaceFact::Omitted,
            healthy_raw.as_str(),
        ),
        ("null", NativeWorkspaceFact::Null, "null"),
        ("empty", NativeWorkspaceFact::Value(String::new()), "\"\""),
        ("id", NativeWorkspaceFact::Value("d1".to_owned()), "\"d1\""),
    ];
    let mut digests = Vec::new();
    for (label, expected, _spelling) in cases.iter() {
        let with_native = match *label {
            "omitted" => healthy_raw.clone(),
            _ => healthy_raw.replacen(
                "\"hostHidden\"",
                &format!(
                    "\"nativeWs\":{},\"hostHidden\"",
                    match *label {
                        "null" => "null".to_owned(),
                        "empty" => "\"\"".to_owned(),
                        _ => "\"d1\"".to_owned(),
                    }
                ),
                1,
            ),
        };
        let staged = stage_fixture_window_rows(&[&with_native]);
        let report = decode_request(&staged).expect("native spelling decodes");
        assert_eq!(
            &report.windows[0].native_workspace, expected,
            "{label} typed twin identical"
        );
        // Semantic readers treat all three unknowns the same.
        match *label {
            "id" => assert_eq!(report.windows[0].native_workspace.as_deref(), Some("d1")),
            _ => assert_eq!(report.windows[0].native_workspace.as_deref(), None),
        }
        assert_eq!(
            encode_window_canonical(&report.windows[0]).expect("finite row"),
            with_native,
            "{label} re-encodes to exact carried bytes"
        );
        let estimate = report_estimate_v4(&report);
        assert!(
            estimate >= staged.len(),
            "estimate {estimate} bounds wire {}",
            staged.len()
        );
        digests.push(report_digest_v4(&report));
    }
    for i in 0..digests.len() {
        for j in (i + 1)..digests.len() {
            assert_ne!(
                digests[i], digests[j],
                "wire spellings {i} and {j} digest distinctly"
            );
        }
    }
}

/// An unknown `type` token holds its identifiable row narrowly in v4
/// (contract 307-314, 63-65, 442) with lossless canonical bytes, while
/// every retired verdict key holds its identifiable row with the key
/// retained.
#[test]
fn v4_window_vocab_rejects_unknown_enums_and_holds_retired() {
    use tiler_core::world_report::{
        RowDiagnosticKind, RowDiagnosticLane, encode_held_window_canonical, encode_window_canonical,
    };

    // Unknown `type` token holds the identifiable row with exact bytes.
    let unknown = {
        let message = include_str!(
            "../../../test-fixtures/world-wire/v4/messages/invalid-window-type-unknown.json"
        );
        let rows = raw_lane_elements(message, "windows");
        assert_eq!(rows.len(), 1);
        rows[0].to_owned()
    };
    let report =
        decode_request(&stage_fixture_window_rows(&[&unknown])).expect("unknown type holds");
    assert!(report.windows.is_empty(), "no valid row");
    assert_eq!(report.held_windows.len(), 1, "one held row");
    let held = &report.held_windows[0];
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact);
    let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
    assert_eq!(diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(diagnostic.row, RowDiagnosticLane::Window);
    assert_eq!(
        encode_held_window_canonical(held).expect("finite row"),
        unknown,
        "lossless held canonical bytes"
    );

    // Wrong-typed `type` holds the identifiable row with exact bytes.
    let mistyped = {
        let message = include_str!(
            "../../../test-fixtures/world-wire/v4/messages/invalid-window-type-bool.json"
        );
        let rows = raw_lane_elements(message, "windows");
        assert_eq!(rows.len(), 1);
        rows[0].to_owned()
    };
    let report = decode_request(&stage_fixture_window_rows(&[&mistyped])).expect("mistyped holds");
    assert!(report.windows.is_empty());
    assert_eq!(report.held_windows.len(), 1);
    assert_eq!(report.held_windows[0].kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(
        encode_held_window_canonical(&report.held_windows[0]).expect("finite row"),
        mistyped
    );

    // All eleven retired verdict keys hold with the key retained and exact
    // bytes (fixture-derived healthy row plus one carried retired key).
    let healthy = {
        let message = include_str!(
            "../../../test-fixtures/world-wire/v4/messages/report-last-write-accepted.json"
        );
        let rows = raw_lane_elements(message, "windows");
        assert_eq!(rows.len(), 1);
        rows[0].to_owned()
    };
    for key in [
        "floating",
        "fitExcluded",
        "fixedAuto",
        "fixedSuppress",
        "observedWs",
        "shell",
        "tool",
        "ownedDialog",
        "dialog",
        "noActivate",
        "cannotControl",
    ] {
        let mut row = healthy.clone();
        row.pop();
        row.push_str(&format!(",\"{key}\":false}}"));
        let report = decode_request(&stage_fixture_window_rows(&[&row])).expect("{key} holds");
        assert!(report.windows.is_empty(), "{key}");
        assert_eq!(report.held_windows.len(), 1, "{key}");
        assert_eq!(
            report.held_windows[0].kind,
            RowDiagnosticKind::InvalidFact,
            "{key}"
        );
        assert!(
            report.held_windows[0]
                .fields
                .iter()
                .any(|(name, _)| name == key),
            "{key} retained"
        );
        assert_eq!(
            encode_held_window_canonical(&report.held_windows[0]).expect("finite row"),
            row,
            "{key}: lossless held bytes"
        );
    }

    // A non-finite retained number never spells `null`: the held writer errs.
    let nonfinite = tiler_core::world_report::HeldWindowRow {
        window: tiler_core::directional::WindowId("win-a".to_owned()),
        ordinal: 0,
        output: Some(tiler_core::directional::OutputId("out-a".to_owned())),
        fields: vec![(
            "layer".to_owned(),
            tiler_core::connection::RawSettingValue::Number(f64::NAN),
        )],
        diagnostic: None,
        kind: RowDiagnosticKind::InvalidFact,
    };
    assert!(encode_held_window_canonical(&nonfinite).is_err());
    // Valid integer geometry is always finite: the valid writer never fails.
    let staged = stage_fixture_window_rows(&[&healthy]);
    let report = decode_request(&staged).expect("healthy decodes");
    assert!(encode_window_canonical(&report.windows[0]).is_ok());
}

fn fixture_text(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    std::fs::read_to_string(&path).expect("fixture bytes")
}

fn fixture_effects_slice(wire: &str) -> &str {
    // Effect rows carry no arrays, so the first `]` after the array open
    // closes the effects array.
    let key = "\"effects\":[";
    let open = wire.find(key).expect("effects array") + key.len() - 1;
    let close = wire[open..].find(']').expect("effects close") + open;
    &wire[open..=close]
}

fn test_only_typed_effect(
    row: &serde_json::Value,
    name: &str,
) -> tiler_core::world_report::OneShotEffect {
    use tiler_core::directional::WindowId;
    use tiler_core::world_report::{EffectCause, EffectKind};
    let kind = match row.get("kind").and_then(serde_json::Value::as_str) {
        Some("focus") => EffectKind::Focus,
        Some("raise") => EffectKind::Raise,
        Some("stack") => EffectKind::Stack,
        other => panic!("{name} closed kind: {other:?}"),
    };
    let cause = match row.get("cause").and_then(serde_json::Value::as_str) {
        Some("explicit-command") => EffectCause::ExplicitCommand,
        Some("automatic-admission") => EffectCause::AutomaticAdmission,
        Some("focus-follows-cursor") => EffectCause::FocusFollowsCursor,
        other => panic!("{name} closed cause: {other:?}"),
    };
    let command_seq = match row.get("cmdSeq") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => Some(
            text.parse::<u64>()
                .unwrap_or_else(|_| panic!("{name} cmdSeq parses")),
        ),
        other => panic!("{name} cmdSeq shape: {other:?}"),
    };
    // Closed-shape cross-checks (contract 818-821): explicit commands
    // always correlate, other causes never do, and only an explicit
    // selection may grant arrival-ready focus.
    match cause {
        EffectCause::ExplicitCommand => assert!(
            command_seq.is_some(),
            "{name} explicit cause carries its sequence"
        ),
        _ => assert!(
            command_seq.is_none(),
            "{name} non-explicit cause carries no sequence"
        ),
    }
    tiler_core::world_report::OneShotEffect {
        id: row
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{name} id"))
            .to_owned(),
        kind,
        target: WindowId(
            row.get("target")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("{name} target"))
                .to_owned(),
        ),
        issued_revision: row
            .get("rev")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{name} rev"))
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{name} rev parses")),
        cause,
        command_seq,
        await_shown: row
            .get("awaitShown")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| panic!("{name} awaitShown")),
    }
}

#[test]
fn immutable_effect_rows_match_fixture_bytes() {
    // Independent conformance: raw immutable fixture bytes on one side,
    // actual code encoding on the other. For each manifest effect row,
    // map ONLY the effect fields into a typed row (test-only token
    // mapping, never a production decoder), publish it through a
    // bootstrap reply, and compare the encoded effects array against the
    // raw bytes sliced from the fixture. The focus `awaitShown: true`
    // witness is a manually constructed typed row only: every engine
    // producer publishes false until explicit-selection readiness is
    // implemented.
    for (name, expected) in [
        (
            "reply-effect-focus",
            "{\"id\":\"effect-a\",\"kind\":\"focus\",\"target\":\"win-a\",\
             \"rev\":\"1\",\"cause\":\"explicit-command\",\"cmdSeq\":\"1\",\
             \"awaitShown\":true}",
        ),
        (
            "reply-effect-raise",
            "{\"id\":\"effect-a\",\"kind\":\"raise\",\"target\":\"win-a\",\
             \"rev\":\"1\",\"cause\":\"explicit-command\",\"cmdSeq\":\"1\",\
             \"awaitShown\":false}",
        ),
        (
            "reply-effect-stack",
            "{\"id\":\"effect-a\",\"kind\":\"stack\",\"target\":\"win-a\",\
             \"rev\":\"1\",\"cause\":\"explicit-command\",\"cmdSeq\":\"1\",\
             \"awaitShown\":false}",
        ),
        (
            "reply-effect-automatic-admission",
            "{\"id\":\"effect-a\",\"kind\":\"focus\",\"target\":\"win-a\",\
             \"rev\":\"1\",\"cause\":\"automatic-admission\",\
             \"cmdSeq\":null,\"awaitShown\":false}",
        ),
        (
            "reply-effect-focus-follows-cursor",
            "{\"id\":\"effect-a\",\"kind\":\"focus\",\"target\":\"win-a\",\
             \"rev\":\"1\",\"cause\":\"focus-follows-cursor\",\
             \"cmdSeq\":null,\"awaitShown\":false}",
        ),
    ] {
        let wire = fixture_text(name);
        let fixture_slice = fixture_effects_slice(&wire).to_owned();
        assert_eq!(
            fixture_slice,
            format!("[{expected}]"),
            "{name} fixture bytes stay immutable"
        );
        let value: serde_json::Value = serde_json::from_str(&wire).expect("fixture parses");
        let row = value
            .get("effects")
            .and_then(serde_json::Value::as_array)
            .and_then(|effects| effects.first())
            .unwrap_or_else(|| panic!("{name} carries one effect row"));
        let report = decode_request(&one_window_request()).expect("decodes");
        let mut engine = connected_engine("gen-1", HostCapabilities::default());
        let mut reply = match engine.handle_report(&report, &[]).0 {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        reply.effects = vec![test_only_typed_effect(row, name)];
        let wire = reply_wire(&reply);
        let encoded_slice = fixture_effects_slice(&wire).to_owned();
        assert_eq!(
            encoded_slice, fixture_slice,
            "code encoding matches immutable {name} bytes"
        );
        assert!(
            reply_estimate_v4(&reply) >= wire.len(),
            "estimate covers {name}"
        );
    }
}

#[test]
fn admission_focus_effect_carries_causal_metadata() {
    // A newcomer with no command mints the automatic-admission intent:
    // typed cause, no sequence, no arrival grant, on the wire through the
    // canonical v4 writer.
    fn envelope_with_windows(windows: serde_json::Value, seq: &str, req: &str) -> String {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["windows"] = windows;
        // No `views` lane: v4 refuses it as an unknown field.
        value["obs"] = serde_json::Value::String(seq.to_owned());
        value["reqSeq"] = serde_json::Value::String(seq.to_owned());
        value["req"] = serde_json::Value::String(req.to_owned());
        value["command"] = serde_json::Value::Null;
        value["cmdSeq"] = serde_json::Value::Null;
        value.to_string()
    }
    fn envelope_with_newcomer(seq: &str, req: &str) -> String {
        envelope_with_windows(
            serde_json::json!([
                v4_window("win-a", "out", [0, 0, 200, 200]),
                v4_window("win-b", "out", [210, 0, 200, 200]),
            ]),
            seq,
            req,
        )
    }
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; windows
            // home only under the matching capabilities.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let seed = decode_request(&envelope_with_windows(
        serde_json::json!([v4_window("win-a", "out", [0, 0, 200, 200])]),
        "1",
        "req-1",
    ))
    .expect("seed decodes");
    match engine.handle_report(&seed, &[]).0 {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted seed, got {other:?}"),
    }
    let report = decode_request(&envelope_with_newcomer("2", "req-2")).expect("decodes");
    let reply = match engine.handle_report(&report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(reply.effects.len(), 1, "one admission: {:?}", reply.effects);
    let effect = &reply.effects[0];
    assert_eq!(effect.kind, tiler_core::world_report::EffectKind::Focus);
    assert_eq!(
        effect.cause,
        tiler_core::world_report::EffectCause::AutomaticAdmission
    );
    assert_eq!(effect.command_seq, None);
    assert!(!effect.await_shown);
    assert!(!effect.is_explicit_focus_for(2));
    let wire = reply_wire(&reply);
    let wire_value: serde_json::Value = serde_json::from_str(&wire).expect("wire is JSON");
    let row = &wire_value["effects"][0];
    assert_eq!(
        row["cause"],
        serde_json::json!("automatic-admission"),
        "wire cause: {row}"
    );
    assert_eq!(row["cmdSeq"], serde_json::Value::Null, "wire cmdSeq: {row}");
    assert_eq!(
        row["awaitShown"],
        serde_json::json!(false),
        "wire grant: {row}"
    );
}

#[test]
fn consumed_command_replays_no_explicit_effect_with_metadata() {
    // Duplicate/consumed replies keep their behavior under the new
    // metadata: a repeated command sequence consumes without
    // re-executing, so no explicit effect re-emits and the observation
    // effects keep their causal shape.
    let mut engine = connected_engine(
        "gen-1",
        HostCapabilities {
            // Fixtures carry native sticky and maximize facts; windows
            // home only under the matching capabilities.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
    );
    let seed = decode_request(&one_window_request()).expect("seed decodes");
    match engine.handle_report(&seed, &[]).0 {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted seed, got {other:?}"),
    }
    let command = serde_json::json!({"op": "focus", "win": "win-a"});
    let first_value: serde_json::Value = {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value["command"] = command.clone();
        value["cmdSeq"] = serde_json::Value::String("5".to_owned());
        value["obs"] = serde_json::Value::String("2".to_owned());
        value["reqSeq"] = serde_json::Value::String("2".to_owned());
        value["req"] = serde_json::Value::String("req-2".to_owned());
        value
    };
    let first = decode_request(&first_value.to_string()).expect("decodes");
    let before = match engine.handle_report(&first, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        before
            .effects
            .iter()
            .any(|effect| effect.is_explicit_focus_for(5)),
        "first execution emits the explicit focus: {:?}",
        before.effects
    );
    let retry_value: serde_json::Value = {
        let mut value = first_value;
        value["obs"] = serde_json::Value::String("3".to_owned());
        value["reqSeq"] = serde_json::Value::String("3".to_owned());
        value["req"] = serde_json::Value::String("req-3".to_owned());
        value
    };
    let retry = decode_request(&retry_value.to_string()).expect("decodes");
    let reply = match engine.handle_report(&retry, &[]).0 {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(reply.command_consumed, "repeat consumes: {reply:?}");
    assert!(
        !reply
            .effects
            .iter()
            .any(|effect| effect.is_explicit_focus_for(5)),
        "no explicit re-emission on the consumed path: {:?}",
        reply.effects
    );
    assert!(
        reply.command_result.is_none(),
        "consumed command carries no result: {:?}",
        reply.command_result
    );
    reply_wire(&reply);
}
