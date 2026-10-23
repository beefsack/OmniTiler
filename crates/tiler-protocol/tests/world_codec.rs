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
         \"switchTarget\":\"cursor-output\",\"freshPlacement\":\"cursor-output-last-focused\",\
         \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
         \"actionOutputs\":[\"out\"],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
         \"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false}}"
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
            serde_json::json!({"win": "w", "source": {"implicit": "home-then-cached-live"}, "delta": -1, "follow": true}),
            Some("home-then-cached-live"),
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
            (
                Some(C::SendNumbered {
                    source: S::HomeThenCachedLive,
                    ..
                })
                | Some(C::SendRelative {
                    source: S::HomeThenCachedLive,
                    ..
                }),
                Some("home-then-cached-live"),
            ) => {}
            (command, _) => panic!("{op} decoded {command:?}"),
        }
    }
    for bad in [
        serde_json::json!({"op": "send-numbered", "win": "w", "source": "o", "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": null, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"explicit": ""}, "index": 9, "follow": false}),
        serde_json::json!({"op": "send-numbered", "win": "w", "source": {"implicit": "bogus"}, "index": 9, "follow": false}),
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
fn cursor_targeting_settings_decode_strictly() {
    use tiler_core::cursor_targeting::{FreshPlacement, SwitchTarget};
    // Defaults ride the envelope: cursor output, last-focused anchors,
    // no cursor, no coupling.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert_eq!(report.switch_target, SwitchTarget::Cursor);
    assert_eq!(report.fresh_placement, FreshPlacement::CursorOutputMru);
    assert_eq!(report.cursor, None);
    assert!(!report.focus_follows_cursor);
    assert!(!report.cursor_warp);
    // Every approved token decodes, including a carried cursor point.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["switchTarget"] = serde_json::Value::String("focused-output".to_owned());
    value["freshPlacement"] = serde_json::Value::String("cursor-output-cursor-closest".to_owned());
    value["cursor"] = serde_json::json!([100, 200]);
    value["focusFollowsCursor"] = serde_json::Value::Bool(true);
    value["cursorWarp"] = serde_json::Value::Bool(true);
    let report = decode_request(&value.to_string()).expect("targeting decodes");
    assert_eq!(report.switch_target, SwitchTarget::Focused);
    assert_eq!(
        report.fresh_placement,
        FreshPlacement::CursorOutputCursorClosest
    );
    assert_eq!(report.cursor, Some((100, 200)));
    assert!(report.focus_follows_cursor);
    assert!(report.cursor_warp);
    // Unknown tokens refuse loudly: no silent wrong-policy fallback.
    for (key, token) in [
        ("switchTarget", "bogus"),
        ("freshPlacement", "bogus"),
        ("freshPlacement", "cursor-output"),
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value[key] = serde_json::Value::String(token.to_owned());
        let error = decode_request(&value.to_string()).expect_err("bad token refuses");
        assert_eq!(error.kind, "unknown-value", "{key}={token}");
    }
    // Missing keys refuse as malformed (strict, no compat backfill).
    for key in [
        "switchTarget",
        "freshPlacement",
        "cursor",
        "focusFollowsCursor",
        "cursorWarp",
    ] {
        let mut value: serde_json::Value =
            serde_json::from_str(&one_window_request()).expect("valid");
        value.as_object_mut().expect("object").remove(key);
        assert!(
            decode_request(&value.to_string()).is_err(),
            "missing {key} refuses"
        );
    }
    // Wrong cursor shapes refuse; out-of-bound coordinates refuse.
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
    let empty = request_json("owner-a", "adapter-1", "gen-1", "1", "1", "req-1", "");
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
fn member_and_view_attempts_decode_tolerantly_and_stay_bounded() {
    use tiler_core::world_report::FrameSetter;
    // Absent keys backfill: predates-the-field adapters stay valid with no
    // transfer/view attempts.
    let report = decode_request(&one_window_request()).expect("decodes");
    assert!(report.member_attempts.is_empty());
    assert!(report.view_attempts.is_empty());
    // Present keys decode to the typed mechanical shape: operation plus
    // window plus target correlation, setter disposition, and nullable
    // immediate pre/post proofs. No verdict rides either lane.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["memberAttempts"] = serde_json::json!([{
        "op": "op-11", "win": "win-a", "out": "out-b", "ws": "ws-2",
        "setter": "refused",
        "pre": {"out": "out-a", "ws": "ws-1"},
        "post": {"out": "out-a", "ws": "ws-1"},
        "attemptMs": "2000",
    }]);
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "threw",
        "pre": "ws-1", "post": "ws-x", "base": "ws-1", "attemptMs": "2001",
    }]);
    let wire = value.to_string();
    let report = decode_request(&wire).expect("attempts decode");
    assert_eq!(report.member_attempts.len(), 1);
    let member = &report.member_attempts[0];
    assert_eq!(member.operation_id, "op-11");
    assert_eq!(member.window.0, "win-a");
    assert_eq!(member.target_output.0, "out-b");
    assert_eq!(member.target_workspace, "ws-2");
    assert_eq!(member.setter, FrameSetter::Refused);
    assert_eq!(
        member.pre,
        Some((
            tiler_core::directional::OutputId("out-a".to_owned()),
            "ws-1".to_owned()
        ))
    );
    assert_eq!(member.post, member.pre);
    assert_eq!(member.attempt_ms, 2000);
    assert_eq!(report.view_attempts.len(), 1);
    let view = &report.view_attempts[0];
    assert_eq!(view.operation_id, "op-9");
    assert_eq!(view.setter, FrameSetter::Threw);
    assert_eq!(view.pre.as_deref(), Some("ws-1"));
    assert_eq!(view.post.as_deref(), Some("ws-x"));
    assert_eq!(view.baseline.as_deref(), Some("ws-1"));
    assert_eq!(view.attempt_ms, 2001);
    // Unknown views ride null (never an empty id, never inferred proof).
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "accepted",
        "pre": null, "post": null, "base": null, "attemptMs": "1",
    }]);
    let null_report = decode_request(&value.to_string()).expect("null views decode");
    assert_eq!(null_report.view_attempts[0].pre, None);
    assert_eq!(null_report.view_attempts[0].post, None);
    assert_eq!(null_report.view_attempts[0].baseline, None);
    // A missing attemptMs refuses like every other counter.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "accepted",
        "pre": null, "post": null, "base": null,
    }]);
    let error = decode_request(&value.to_string()).expect_err("missing stamp refuses");
    assert_eq!(error.kind, "malformed");
    // Duplicate operation windows refuse with fixed vocabulary.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["memberAttempts"] = serde_json::json!([
        {"op": "op-9", "win": "win-a", "out": "out-a", "ws": "ws-2",
         "setter": "accepted", "pre": null, "post": null, "attemptMs": "1"},
        {"op": "op-9", "win": "win-a", "out": "out-a", "ws": "ws-2",
         "setter": "accepted", "pre": null, "post": null, "attemptMs": "2"},
    ]);
    let error = decode_request(&value.to_string()).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-member");
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([
        {"op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "accepted",
         "pre": null, "post": null, "base": null, "attemptMs": "1"},
        {"op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "accepted",
         "pre": null, "post": null, "base": null, "attemptMs": "1"},
    ]);
    let error = decode_request(&value.to_string()).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-view-attempt");
    // Unknown setter vocabulary refuses; unknown attempt fields refuse.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["memberAttempts"] = serde_json::json!([{
        "op": "op-9", "win": "win-a", "out": "out-a", "ws": "ws-2",
        "setter": "clamped", "pre": null, "post": null, "attemptMs": "1",
    }]);
    let error = decode_request(&value.to_string()).expect_err("bad setter refuses");
    assert_eq!(error.kind, "unknown-value");
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "accepted",
        "pre": null, "post": null, "base": null, "attemptMs": "1", "verdict": "applied",
    }]);
    let error = decode_request(&value.to_string()).expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    // Retired external verdict ingress refuses as an unknown field: the core
    // derives every verdict from raw attempts, so no feedback key exists.
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["feedback"] = serde_json::json!([{
        "op": "op-9", "kind": "transfer", "outcome": "applied", "hold": "",
    }]);
    let error = decode_request(&value.to_string()).expect_err("feedback key refuses");
    assert_eq!(error.kind, "unknown-field");
    // Estimates bound the wider wire shape.
    let estimate = tiler_core::world_report::report_estimate(&report);
    assert!(
        estimate >= wire.len(),
        "estimate {estimate} bounds wire {}",
        wire.len()
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
        "owner-a",
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
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
fn empty_view_strings_normalize_to_unknown_instead_of_rejecting() {
    // Empty pre/post/base normalize to None (unknown), consistently with
    // views[]/current empty-means-unknown: validation rejects empty proof
    // rows outright, so decoding must not produce Some("").
    let mut value: serde_json::Value = serde_json::from_str(&one_window_request()).expect("valid");
    value["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out-a", "ws": "ws-2", "setter": "threw",
        "pre": "", "post": "", "base": "", "attemptMs": "1",
    }]);
    let report = decode_request(&value.to_string()).expect("empty views decode");
    assert_eq!(report.view_attempts.len(), 1);
    assert_eq!(report.view_attempts[0].pre, None);
    assert_eq!(report.view_attempts[0].post, None);
    assert_eq!(report.view_attempts[0].baseline, None);
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
         \"transfers\":[],\"effects\":[],\"result\":null,\"gestureSettled\":null,\"operationSettled\":[],\"warpCursor\":null}"
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
