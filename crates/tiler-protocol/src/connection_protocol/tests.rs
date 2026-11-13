use super::*;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tiler_core::connection::{
    HostCapabilities, NativeSelectorScope, RawSettingValue, SettingsStatus,
};
use tiler_core::settings_state::SettingsState;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4-active")
}

#[test]
fn manifest_connection_fixtures_round_trip_or_reject() {
    let base = root();
    let manifest: Value =
        serde_json::from_slice(&fs::read(base.join("manifest.json")).unwrap()).unwrap();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    let mut count = 0;
    let mut rejects = 0;
    let mut accepted = 0;
    for f in fixtures {
        let family = f["family"].as_str().unwrap();
        if !matches!(
            family,
            "HandshakeRequest"
                | "HandshakeReply"
                | "Settings"
                | "SettingsReply"
                | "WorldProtocolError"
        ) {
            continue;
        }
        count += 1;
        let bytes = fs::read(base.join(f["file"].as_str().unwrap())).unwrap();
        let input = std::str::from_utf8(&bytes).unwrap();
        let expectation = f["expectation"].as_str().unwrap();
        if expectation == "reject-wire" {
            rejects += 1;
            let result = match family {
                "HandshakeRequest" => decode_handshake_request(input).map(|_| ()),
                "HandshakeReply" => decode_handshake_reply(input).map(|_| ()),
                "Settings" => decode_settings(input).map(|_| ()),
                "SettingsReply" => decode_settings_reply(input).map(|_| ()),
                _ => decode_error(input).map(|_| ()),
            };
            assert!(result.is_err(), "{} unexpectedly decoded", f["id"]);
        } else {
            accepted += 1;
            let result = match family {
                "HandshakeRequest" => {
                    decode_handshake_request(input).and_then(|x| encode_handshake_request(&x))
                }
                "HandshakeReply" => {
                    decode_handshake_reply(input).and_then(|x| encode_handshake_reply(&x))
                }
                "Settings" => decode_settings(input).and_then(|x| encode_settings(&x)),
                "SettingsReply" => {
                    decode_settings_reply(input).and_then(|x| encode_settings_reply(&x))
                }
                _ => decode_error(input).and_then(|x| encode_error(x.kind, &x.generation)),
            };
            assert_eq!(
                result.unwrap().as_bytes(),
                bytes.as_slice(),
                "{} bytes differ",
                f["id"]
            );
        }
    }
    assert_eq!(
        count, 92,
        "manifest fixture selection changed; update explicit family coverage deliberately"
    );
    assert_eq!(rejects, 54);
    assert_eq!(accepted, 38);
}

#[test]
fn frozen_v4_handshakes_reject_without_shared_native_roster() {
    // Historical valid v4 handshakes predate the required sharedNativeRoster
    // capability, so the current strict shape rejects them (shape, not a
    // capability-value verdict). The active family lives in v4-active.
    for id in ["handshake-native", "handshake-emulated"] {
        let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../test-fixtures/world-wire/v4/messages/{id}.json"
        )))
        .unwrap();
        let input = std::str::from_utf8(&bytes).unwrap();
        assert!(
            !input.contains("sharedNativeRoster"),
            "{id} is frozen history"
        );
        assert_eq!(
            decode_handshake_request(input),
            Err(CodecError("shape")),
            "{id} rejects without the new key"
        );
    }
}

#[test]
fn connection_messages_route_to_one_world_engine() {
    let mut engine = tiler_core::world_engine::WorldEngine::new("gen-1").unwrap();
    let handshake = fixture_text("handshake-native");
    let reply: Value =
        serde_json::from_str(&route_connection_request(&mut engine, &handshake).unwrap()).unwrap();
    assert_eq!(reply["type"], "handshake-reply");
    assert_eq!(reply["ready"], false);
    assert_eq!(reply["adapter"], "adapter-a");

    let mut settings: Value = serde_json::from_str(&fixture_text("settings-defaults")).unwrap();
    settings["gen"] = reply["gen"].clone();
    let routed = route_connection_request(&mut engine, &settings.to_string()).unwrap();
    let routed: Value = serde_json::from_str(&routed).unwrap();
    assert_eq!(routed["type"], "settings-reply");
    assert_eq!(routed["status"], "accepted");
    assert!(engine.capabilities().unwrap().host_owned);
}

fn fixture_text(id: &str) -> String {
    let base = root();
    let manifest: Value =
        serde_json::from_slice(&fs::read(base.join("manifest.json")).unwrap()).unwrap();
    let path = manifest["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id)
        .unwrap()["file"]
        .as_str()
        .unwrap();
    String::from_utf8(fs::read(base.join(path)).unwrap()).unwrap()
}

fn fixture_request(id: &str) -> SettingsRequest {
    decode_settings(&fixture_text(id)).unwrap()
}

fn fixture_caps() -> HostCapabilities {
    HostCapabilities {
        host_owned: false,
        equal_count: false,
        shared_native_roster: false,
        birth_clear: true,
        native_selector_scope: NativeSelectorScope::PerOutput,
        window_suppression: true,
        maximize: true,
        fullscreen: true,
        native_sticky: true,
        above: true,
        warp: true,
        tiled_float_transfer: true,
        unavailable_facts: Vec::new(),
    }
}

fn assert_reply_matches_fixture(reply: &SettingsReply, id: &str) {
    let expected = decode_settings_reply(&fixture_text(id)).unwrap();
    assert_eq!(reply.status, expected.status, "{id}");
    let mut actual_diagnostics = reply.diagnostics.clone();
    let mut expected_diagnostics = expected.diagnostics;
    actual_diagnostics.sort_by(|a, b| a.key.cmp(&b.key));
    expected_diagnostics.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(actual_diagnostics, expected_diagnostics, "{id}");
}

#[test]
fn settings_message_fixtures_drive_core_state_and_match_reply_fixtures() {
    let caps = fixture_caps();
    for input in [
        "settings-defaults",
        "settings-alternatives",
        "settings-third-arms",
    ] {
        let mut state = SettingsState::default();
        let reply = state.process(&fixture_request(input), &caps);
        assert!(
            reply.diagnostics.is_empty(),
            "{input}: {:?}",
            reply.diagnostics
        );
        assert_eq!(reply.status, SettingsStatus::Accepted);
        if input == "settings-defaults" {
            assert_reply_matches_fixture(&reply, "settings-reply-accepted");
        }
    }

    let mut missing = SettingsState::default();
    let reply = missing.process(&fixture_request("settings-missing"), &caps);
    assert_eq!(reply.status, SettingsStatus::Accepted);
    assert_eq!(reply.diagnostics.len(), 18);
    assert!(
        reply
            .diagnostics
            .iter()
            .all(|d| d.kind == tiler_core::connection::SettingsDiagnosticKind::Missing)
    );

    let mut invalid = SettingsState::default();
    invalid.process(&fixture_request("settings-defaults"), &caps);
    let reply = invalid.process(&fixture_request("settings-invalid-fields"), &caps);
    assert_reply_matches_fixture(&reply, "settings-reply-invalid-fields");

    let mut restarted = SettingsState::default();
    restarted.process(&fixture_request("settings-defaults"), &caps);
    let reply = restarted.process(&fixture_request("settings-restart-only"), &caps);
    assert_reply_matches_fixture(&reply, "settings-reply-restart-required");

    let mut revisions = SettingsState::default();
    let first = fixture_request("settings-defaults");
    revisions.process(&first, &caps);
    let mut older = first.clone();
    older.revision = 0;
    assert_reply_matches_fixture(&revisions.process(&older, &caps), "settings-reply-stale");
    let mut reused = first;
    reused
        .values
        .push(("gap".into(), RawSettingValue::Number(9.0)));
    assert_reply_matches_fixture(
        &revisions.process(&reused, &caps),
        "settings-reply-revision-reused",
    );
}

#[test]
fn bootstrap_settings_scenario_runs_settings_steps_through_core_state() {
    if std::env::var("UPDATE_SETTINGS_V4_FIXTURES").as_deref() == Ok("1") {
        // The typed Settings integration recorder writes this scenario after the unit-test phase.
        return;
    }
    let scenario: Value = serde_json::from_slice(
        &fs::read(root().join("scenarios/bootstrap-settings.json")).unwrap(),
    )
    .unwrap();
    let mut caps = None;
    let mut state = SettingsState::default();
    for step in scenario["steps"].as_array().unwrap() {
        let Some(message) = step.pointer("/input/message") else {
            continue;
        };
        match message["type"].as_str() {
            Some("handshake") => {
                let text = serde_json::to_string(message).unwrap();
                caps = Some(decode_handshake_request(&text).unwrap().capabilities);
            }
            Some("settings") => {
                let request = decode_settings(&serde_json::to_string(message).unwrap()).unwrap();
                let reply = state.process(&request, caps.as_ref().unwrap());
                match step["id"].as_str().unwrap() {
                    "settings" | "change" => assert_eq!(
                        (reply.status, reply.diagnostics.len()),
                        (SettingsStatus::Accepted, 0)
                    ),
                    "duplicate-settings" => {
                        assert_eq!(reply.status, SettingsStatus::Accepted);
                        assert!(reply.diagnostics.is_empty());
                    }
                    "invalid-siblings" => {
                        assert_eq!(reply.status, SettingsStatus::Accepted);
                        assert!(reply.diagnostics.iter().any(|d| d.key == "gap"
                            && d.kind
                                == tiler_core::connection::SettingsDiagnosticKind::WrongType));
                        assert!(reply.diagnostics.iter().any(|d| d.key == "unknownSetting"
                            && d.kind
                                == tiler_core::connection::SettingsDiagnosticKind::UnknownField));
                    }
                    "older-settings" => assert_eq!(reply.status, SettingsStatus::Stale),
                    "equivocating-settings" => {
                        assert_eq!(reply.status, SettingsStatus::RevisionReused)
                    }
                    "restart-setting" => assert!(reply.diagnostics.iter().any(|d| d.key == "mode"
                        && d.kind
                            == tiler_core::connection::SettingsDiagnosticKind::RestartRequired)),
                    other => panic!("unexpected settings scenario step {other}"),
                }
            }
            _ => {}
        }
    }
}

#[test]
fn canonical_numbers_cover_shortest_expansion_and_zero() {
    for (n, expected) in [
        (1.25, "1.25"),
        (1.5, "1.5"),
        (3.0, "3"),
        (1e-7, "0.0000001"),
        (-0.0, "0"),
        (1e21, "1000000000000000000000"),
    ] {
        assert_eq!(encode_json_number(n).unwrap(), expected);
    }
    assert!(encode_json_number(f64::INFINITY).is_err());
}

#[test]
fn canonical_numbers_round_trip_binary64_boundaries_and_patterns() {
    let boundaries = [
        f64::from_bits(1),
        f64::from_bits(2),
        f64::from_bits(0x000f_ffff_ffff_ffff),
        f64::MIN_POSITIVE,
        f64::from_bits(f64::MIN_POSITIVE.to_bits() - 1),
        f64::MAX,
        -f64::MAX,
        -f64::MIN_POSITIVE,
        0.0,
        -0.0,
        1.0 + f64::EPSILON,
        1.0 + f64::EPSILON / 2.0,
    ];
    for value in boundaries {
        let encoded = encode_json_number(value).unwrap();
        let decoded: f64 = encoded.parse().unwrap();
        assert_eq!(
            decoded.to_bits(),
            if value == 0.0 {
                0.0f64.to_bits()
            } else {
                value.to_bits()
            },
            "{value:?} -> {encoded}"
        );
    }

    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for _ in 0..20_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let value = f64::from_bits(state);
        if !value.is_finite() {
            continue;
        }
        let encoded = encode_json_number(value).unwrap();
        let decoded: f64 = encoded.parse().unwrap();
        assert_eq!(
            decoded.to_bits(),
            if value == 0.0 {
                0.0f64.to_bits()
            } else {
                value.to_bits()
            },
            "bits={:#018x} -> {encoded}",
            value.to_bits()
        );
    }
}

#[test]
fn shortest_formatter_and_json_parser_match_halfway_witnesses() {
    // Exact decimal midpoints between 1.0 and its successor, then between
    // that successor and the next value. Ties select the even significand.
    let midpoints = [
        (
            "1.00000000000000011102230246251565404236316680908203125",
            1.0f64,
            "1",
        ),
        (
            "1.00000000000000033306690738754696212708950042724609375",
            f64::from_bits(1.0f64.to_bits() + 2),
            "1.0000000000000004",
        ),
    ];
    for (decimal, expected, formatted) in midpoints {
        let standard: f64 = decimal.parse().unwrap();
        let json: f64 = serde_json::from_str(decimal).unwrap();
        let strict = parse_strict_json(decimal).unwrap().as_f64().unwrap();
        assert_eq!(standard.to_bits(), expected.to_bits());
        assert_eq!(json.to_bits(), expected.to_bits());
        assert_eq!(strict.to_bits(), expected.to_bits());
        assert_eq!(expected.to_string(), formatted);
        assert_eq!(encode_json_number(expected).unwrap(), formatted);
    }

    for decimal in [
        "2.2250738585072012e-308",
        "4.9406564584124654e-324",
        "1.7976931348623157e308",
        "9007199254740993.0",
    ] {
        let standard: f64 = decimal.parse().unwrap();
        let json: f64 = serde_json::from_str(decimal).unwrap();
        let strict = parse_strict_json(decimal).unwrap().as_f64().unwrap();
        assert_eq!(json.to_bits(), standard.to_bits(), "{decimal}");
        assert_eq!(strict.to_bits(), standard.to_bits(), "{decimal}");
    }
}

#[test]
fn canonical_number_ties_choose_even_decimal_last_digit() {
    for (value, lower, upper, expected) in [
        (
            1_000_000_000_000_000f64 + 0.25,
            "1000000000000000.2",
            "1000000000000000.3",
            "1000000000000000.2",
        ),
        (
            1_000_000_000_000_000f64 + 0.75,
            "1000000000000000.7",
            "1000000000000000.8",
            "1000000000000000.8",
        ),
    ] {
        let bits = value.to_bits();
        assert_eq!(lower.parse::<f64>().unwrap().to_bits(), bits);
        assert_eq!(upper.parse::<f64>().unwrap().to_bits(), bits);
        for shorter in ["1000000000000000", "1000000000000001"] {
            assert_ne!(shorter.parse::<f64>().unwrap().to_bits(), bits);
        }
        let actual = encode_json_number(value).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn string_encoding_escapes_only_json_and_c0_controls() {
    assert_eq!(
        encode_json_string("quote\" slash\\ line\n café 日本"),
        "\"quote\\\" slash\\\\ line\\u000a café 日本\""
    );
    assert_eq!(encode_json_string("\u{1f}"), "\"\\u001f\"");
}

#[test]
fn strict_parser_rejects_decoded_duplicate_keys_recursively() {
    assert!(parse_strict_json(r#"{"x":1,"x":2}"#).is_err());
    assert!(parse_strict_json(r#"{"x":{"a":1,"\u0061":2}}"#).is_err());
    assert!(parse_strict_json(r#"{"x":[{"a":1,"a":2}]}"#).is_err());
    assert!(parse_strict_json(r#"{"x":1}"#).is_ok());
}

#[test]
fn settings_numbers_normalize_exponents_and_signed_zero() {
    let input = r#"{"type":"settings","v":4,"adapter":"a","gen":"g-1","req":"r","rev":"1","values":{"z":-0.0,"a":1e-7}}"#;
    let parsed = decode_settings(input).unwrap();
    assert_eq!(
        encode_settings(&parsed).unwrap(),
        r#"{"type":"settings","v":4,"adapter":"a","gen":"g-1","req":"r","rev":"1","values":{"a":0.0000001,"z":0}}"#
    );
}

#[test]
fn error_and_diagnostic_text_enforce_utf8_metadata_limit() {
    for (text, accepted) in [
        ("a".repeat(4096), true),
        ("a".repeat(4097), false),
        ("é".repeat(2048), true),
        ("é".repeat(2049), false),
    ] {
        let wire = format!(
            "{{\"v\":4,\"outcome\":\"rejected\",\"kind\":\"malformed\",\"message\":{},\"gen\":\"\"}}",
            encode_json_string(&text)
        );
        assert_eq!(
            decode_error(&wire).is_ok(),
            accepted,
            "error bytes={}",
            text.len()
        );
    }

    for (key, accepted) in [
        ("a".repeat(4096), true),
        ("a".repeat(4097), false),
        ("é".repeat(2048), true),
        ("é".repeat(2049), false),
    ] {
        let reply = SettingsReply {
            adapter: "a".into(),
            generation: "g-1".into(),
            request: "r".into(),
            revision: 1,
            status: SettingsStatus::Accepted,
            diagnostics: vec![SettingsDiagnostic {
                key: key.clone(),
                kind: SettingsDiagnosticKind::Missing,
            }],
        };
        assert_eq!(
            encode_settings_reply(&reply).is_ok(),
            accepted,
            "diagnostic bytes={}",
            key.len()
        );
        let wire = format!(
            "{{\"type\":\"settings-reply\",\"v\":4,\"adapter\":\"a\",\"gen\":\"g-1\",\"req\":\"r\",\"rev\":\"1\",\"status\":\"accepted\",\"diagnostics\":[{{\"key\":{},\"kind\":\"missing\"}}]}}",
            encode_json_string(&key)
        );
        assert_eq!(
            decode_settings_reply(&wire).is_ok(),
            accepted,
            "diagnostic bytes={}",
            key.len()
        );
    }
}

#[test]
fn setting_name_utf8_bounds_and_unknown_name_diagnostics_round_trip() {
    let valid_names = ["a".repeat(4096), "é".repeat(2048)];
    for name in valid_names {
        let wire = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"a\",\"gen\":\"g-1\",\"req\":\"r\",\"rev\":\"1\",\"values\":{{{}:true}}}}",
            encode_json_string(&name)
        );
        let request = decode_settings(&wire).expect("4096-byte setting name");
        assert_eq!(request.values[0].0, name);
        assert!(encode_settings(&request).is_ok());
    }

    for name in ["a".repeat(4097), format!("{}a", "é".repeat(2048))] {
        let wire = format!(
            "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"a\",\"gen\":\"g-1\",\"req\":\"r\",\"rev\":\"1\",\"values\":{{{}:true}}}}",
            encode_json_string(&name)
        );
        assert_eq!(name.len(), 4097);
        assert!(decode_settings(&wire).is_err());
        let request = SettingsRequest {
            adapter: "a".into(),
            generation: "g-1".into(),
            request: "r".into(),
            revision: 1,
            values: vec![(name, RawSettingValue::Bool(true))],
        };
        assert!(encode_settings(&request).is_err());
    }

    // Only direct values-object keys are setting names. Nested raw-object keys
    // remain opaque data, and accepted unknown names are returned verbatim.
    let nested_name = "n".repeat(4097);
    let wire = format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"a\",\"gen\":\"g-1\",\"req\":\"r\",\"rev\":\"1\",\"values\":{{\"unknown\":{{{}:null}}}}}}",
        encode_json_string(&nested_name)
    );
    assert!(decode_settings(&wire).is_ok());

    let request = SettingsRequest {
        adapter: "a".into(),
        generation: "g-1".into(),
        request: "r".into(),
        revision: 1,
        values: vec![
            (String::new(), RawSettingValue::Bool(true)),
            ("\0setting".into(), RawSettingValue::Null),
            ("未知".into(), RawSettingValue::Bool(false)),
        ],
    };
    let capabilities = HostCapabilities {
        host_owned: true,
        equal_count: false,
        shared_native_roster: false,
        birth_clear: false,
        native_selector_scope: NativeSelectorScope::PerOutput,
        window_suppression: false,
        maximize: false,
        fullscreen: false,
        native_sticky: false,
        above: false,
        warp: false,
        tiled_float_transfer: false,
        unavailable_facts: Vec::new(),
    };
    let reply = SettingsState::default().process(&request, &capabilities);
    assert_eq!(reply.diagnostics.len(), 21);
    assert!(
        reply
            .diagnostics
            .iter()
            .any(|d| d.key.is_empty() && d.kind == SettingsDiagnosticKind::UnknownField)
    );
    assert!(
        reply
            .diagnostics
            .iter()
            .any(|d| d.key == "\0setting" && d.kind == SettingsDiagnosticKind::UnknownField)
    );
    assert!(
        reply
            .diagnostics
            .iter()
            .any(|d| d.key == "未知" && d.kind == SettingsDiagnosticKind::UnknownField)
    );
    let decoded = decode_settings_reply(&encode_settings_reply(&reply).unwrap()).unwrap();
    assert_eq!(decoded.diagnostics, reply.diagnostics);
}

#[test]
fn error_encoder_uses_fixed_text_for_typed_kind() {
    let encoded = encode_error(ProtocolKind::Malformed, "").unwrap();
    assert_eq!(
        encoded,
        r#"{"v":4,"outcome":"rejected","kind":"malformed","message":"malformed","gen":""}"#
    );
}

fn handshake_native_text() -> String {
    fixture_text("handshake-native")
}

#[test]
fn process_id_is_a_required_third_identity_key_in_canonical_order() {
    let valid = handshake_native_text();
    assert!(valid.contains("\"appId\":null,\"executable\":\"c:\\\\synthetic\\\\omnitiler.exe\",\"processId\":\"pid-own-1\""));
    let request = decode_handshake_request(&valid).unwrap();
    assert_eq!(
        request.application_identity.process_id.as_deref(),
        Some("pid-own-1")
    );
    assert_eq!(encode_handshake_request(&request).unwrap(), valid);
    // Missing key refuses on shape; a fourth key refuses as unknown-field.
    let missing = valid.replace(",\"processId\":\"pid-own-1\"", "");
    assert_eq!(decode_handshake_request(&missing), Err(CodecError("shape")));
    let extra = valid.replace(
        "\"processId\":\"pid-own-1\"",
        "\"processId\":\"pid-own-1\",\"processId2\":null",
    );
    assert!(decode_handshake_request(&extra).is_err());
}

#[test]
fn process_id_rejects_wrong_type_and_non_opaque_values() {
    let valid = handshake_native_text();
    let typed = valid.replace("\"processId\":\"pid-own-1\"", "\"processId\":7");
    assert!(decode_handshake_request(&typed).is_err());
    for bad in ["", "bad id", "has space", &"x".repeat(129)] {
        let wire = valid.replace(
            "\"processId\":\"pid-own-1\"",
            &format!("\"processId\":{}", encode_json_string(bad)),
        );
        assert!(
            decode_handshake_request(&wire).is_err(),
            "processId {bad:?} must refuse"
        );
    }
    // Opaque namespace: a decimal native pid spelling is a valid Id.
    let decimal = valid.replace("\"processId\":\"pid-own-1\"", "\"processId\":\"4242\"");
    let request = decode_handshake_request(&decimal).unwrap();
    assert_eq!(
        request.application_identity.process_id.as_deref(),
        Some("4242")
    );
    assert_eq!(encode_handshake_request(&request).unwrap(), decimal);
}

#[test]
fn process_id_support_mirrors_executable_without_bypass() {
    let valid = handshake_native_text();
    // Listed but carried: refuses like the executable contradiction.
    let listed = valid.replace(
        "\"unavailableFacts\":[]",
        "\"unavailableFacts\":[\"processId\"]",
    );
    assert_eq!(
        decode_handshake_request(&listed),
        Err(CodecError("identity-support"))
    );
    // Supported but null: refuses like the executable omission.
    let nulled = valid.replace("\"processId\":\"pid-own-1\"", "\"processId\":null");
    assert_eq!(
        decode_handshake_request(&nulled),
        Err(CodecError("identity-support"))
    );
    // Consistently unavailable: null plus listed in catalog order.
    let unavailable = valid
        .replace("\"processId\":\"pid-own-1\"", "\"processId\":null")
        .replace(
            "\"executable\":\"c:\\\\synthetic\\\\omnitiler.exe\"",
            "\"executable\":null",
        )
        .replace(
            "\"unavailableFacts\":[]",
            "\"unavailableFacts\":[\"executable\",\"processId\"]",
        );
    let request = decode_handshake_request(&unavailable).unwrap();
    assert_eq!(request.application_identity.process_id, None);
    assert_eq!(encode_handshake_request(&request).unwrap(), unavailable);
}

#[test]
fn engine_handshake_refuses_typed_identity_support_mismatch() {
    use tiler_core::connection::{ApplicationIdentity, HandshakeRequest, HostCapabilities};
    let mut engine = tiler_core::world_engine::WorldEngine::new("gen-1").unwrap();
    // Supported executable but null processId bypasses nothing: refused.
    let reply = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("example".to_owned()),
            process_id: None,
        },
        capabilities: HostCapabilities::default(),
    });
    assert_eq!(reply.kind, Some(ProtocolKind::Malformed));
    assert!(reply.generation.is_none());
    // Consistently unavailable binds cleanly with no state reset.
    let clean = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-2".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: None,
            process_id: None,
        },
        capabilities: HostCapabilities::without_native_identity(),
    });
    assert_eq!(clean.kind, None);
    assert!(clean.generation.is_some());
}

#[test]
fn split_identity_availability_round_trips_canonically() {
    let valid = handshake_native_text();
    // Supported executable with unavailable process: carried exe plus a
    // null listed pid encodes in canonical order.
    let exe_only = valid
        .replace("\"processId\":\"pid-own-1\"", "\"processId\":null")
        .replace(
            "\"unavailableFacts\":[]",
            "\"unavailableFacts\":[\"processId\"]",
        );
    let request = decode_handshake_request(&exe_only).unwrap();
    assert_eq!(
        request.application_identity.executable.as_deref(),
        Some("c:\\synthetic\\omnitiler.exe")
    );
    assert_eq!(request.application_identity.process_id, None);
    assert_eq!(encode_handshake_request(&request).unwrap(), exe_only);
    // Supported process with unavailable executable: the mirror image.
    let pid_only = valid
        .replace(
            "\"executable\":\"c:\\\\synthetic\\\\omnitiler.exe\"",
            "\"executable\":null",
        )
        .replace(
            "\"unavailableFacts\":[]",
            "\"unavailableFacts\":[\"executable\"]",
        );
    let request = decode_handshake_request(&pid_only).unwrap();
    assert_eq!(request.application_identity.executable, None);
    assert_eq!(
        request.application_identity.process_id.as_deref(),
        Some("pid-own-1")
    );
    assert_eq!(encode_handshake_request(&request).unwrap(), pid_only);
}
