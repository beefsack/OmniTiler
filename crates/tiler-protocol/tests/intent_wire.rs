//! Wire ingress/publication for the opaque outer intent lanes.
//!
//! Nested `intentRecords[].data` / `intentWrites[].data` strings are
//! extracted from the immutable v4 fixtures and run through the genuine v4
//! wire path: [`decode_request`](tiler_protocol::world_protocol::decode_request)
//! preserves them verbatim (unsupported or malformed inner JSON stays an
//! accepted envelope), outer shape/byte bounds refuse the envelope, typed
//! direct construction refuses the same bounds through
//! [`validate_report_v4_reason`](tiler_core::world_report_v4_checks::validate_report_v4_reason),
//! the digest separates every carried byte, the engine accepts opaque
//! records yet publishes no writes, outbound write rows share the same
//! fixed vocabulary through a typed helper enforced at the publication
//! commit gate, and the canonical reply writer spells save/clear rows
//! with literal row-byte pins, independent core byte counts and a decoder
//! roundtrip. The staged envelope is a
//! genuine v4 envelope (immutable v4 fixture base carrying the required
//! `rendererInput` lane and the v4 wire version).
//!
//! Offline only: no adapters, no live work.

use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
};
use tiler_core::directional::WindowId;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    IntentRecord, IntentWrite, WORLD_CONTRACT_VERSION_V4, WorldReplyV4,
    validate_intent_writes_reason,
};
use tiler_core::world_report_v4_checks::{report_digest_v4, validate_report_v4_reason};
use tiler_core::world_sizing::{intent_records_estimate, intent_writes_estimate};
use tiler_core::world_sizing_v4::{
    reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, WorldOutcome, decode_reply_v4, decode_request,
    encode_reply_v4,
};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture JSON"))
        .expect("fixture JSON")
}

/// Otherwise-accepted v4 envelope carrying the intent rows under test. The
/// base is an immutable v4 fixture (valid screens, windows, presentation,
/// and the required `rendererInput` lane); only the identity stamps are
/// rebound to the connected test engine and only this lane varies.
fn envelope(intent_records: serde_json::Value) -> String {
    let mut base = fixture("report-intent-float");
    base["adapter"] = serde_json::json!("adapter-1");
    base["gen"] = serde_json::json!("gen-1");
    base["req"] = serde_json::json!("req-1");
    base["intentRecords"] = intent_records;
    base.to_string()
}

fn nested_record(name: &str) -> serde_json::Value {
    let envelope_fixture: serde_json::Value = fixture(name);
    let records = envelope_fixture["intentRecords"]
        .as_array()
        .expect("fixture intentRecords");
    assert_eq!(records.len(), 1, "{name}");
    records[0].clone()
}

fn nested_write(name: &str, index: usize) -> serde_json::Value {
    let envelope_fixture: serde_json::Value = fixture(name);
    let writes = envelope_fixture["intentWrites"]
        .as_array()
        .expect("fixture intentWrites");
    writes[index].clone()
}

fn connected_engine(generation: &str, caps: HostCapabilities) -> WorldEngine {
    let mut engine = WorldEngine::new(generation).expect("generation");
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

/// Encode one engine-published v4 reply with the canonical reply
/// writer; the bytes must match the independent core byte count exactly
/// and round-trip through the reply decoder to the identical typed reply
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

#[test]
fn nested_fixture_rows_decode_verbatim() {
    // Supported inner positives ride the outer record byte-for-byte: the
    // wire never decodes or recanonicalizes the inner string.
    for name in [
        "report-intent-float",
        "report-intent-tile",
        "report-intent-null",
    ] {
        let row = nested_record(name);
        let expected = row["data"].as_str().expect("record data string").to_owned();
        let report = decode_request(&envelope(serde_json::json!([row]))).expect("decodes");
        assert_eq!(report.intent_records.len(), 1, "{name}");
        assert_eq!(report.intent_records[0].win.0, "win-a", "{name}");
        assert_eq!(report.intent_records[0].data, expected, "{name}");
    }
}

#[test]
fn unsupported_and_malformed_inner_stay_accepted_verbatim() {
    // An unsupported inner version is unusable inner data, never a
    // transport refusal: the envelope accepts and preserves it.
    let row = nested_record("report-intent-unsupported-inner-version");
    let expected = row["data"].as_str().expect("record data string").to_owned();
    assert_eq!(expected, "{\"v\":2,\"float\":true,\"stickyPrior\":null}");
    let report = decode_request(&envelope(serde_json::json!([row]))).expect("decodes");
    assert_eq!(report.intent_records[0].data, expected);
    // Malformed and whitespace-padded inner strings are equally opaque:
    // the outer record carries bytes, not verdicts.
    for data in [
        "{not json",
        "[1,2]",
        "  {\"v\":1,\"float\":true,\"stickyPrior\":null}  ",
        "",
    ] {
        let report = decode_request(&envelope(
            serde_json::json!([{"win": "win-a", "data": data}]),
        ))
        .expect("malformed inner stays accepted");
        assert_eq!(report.intent_records[0].data, data);
    }
}

#[test]
fn outer_invalid_rows_refuse_with_fixed_kinds() {
    // Missing key: required presence is malformed.
    let mut value: serde_json::Value =
        serde_json::from_str(&envelope(serde_json::json!([]))).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("intentRecords");
    let error = decode_request(&value.to_string()).expect_err("missing refuses");
    assert_eq!(error.kind, "malformed");
    // Wrong container or row shapes refuse before any bound check.
    for (rows, kind) in [
        (serde_json::json!({}), "malformed"),
        (serde_json::json!([[]]), "malformed"),
        (serde_json::json!(["x"]), "malformed"),
        (serde_json::json!([{"data": "x"}]), "malformed"),
        (
            serde_json::json!([{"win": "win-a", "data": "x", "extra": 1}]),
            "unknown-field",
        ),
        (serde_json::json!([{"win": 1, "data": "x"}]), "malformed"),
        (serde_json::json!([{"win": "win-a"}]), "malformed"),
    ] {
        let error = decode_request(&envelope(rows)).expect_err("shape refuses");
        assert_eq!(error.kind, kind);
    }
    // Bad subjects refuse as unknown values, never as typed facts.
    for win in ["", "has space", "win!a"] {
        let error = decode_request(&envelope(
            serde_json::json!([{"win": win, "data": "{\"v\":1}"}]),
        ))
        .expect_err("bad subject refuses");
        assert_eq!(error.kind, "unknown-value", "win {win:?}");
    }
    // A non-string payload is an outer shape violation, not inner data:
    // `null` (the write-lane clear spelling) never clears on the record
    // lane, and objects never coerce to strings.
    for data in [
        serde_json::Value::Null,
        serde_json::json!({"v": 1}),
        serde_json::json!(1),
        serde_json::json!(true),
    ] {
        let error = decode_request(&envelope(
            serde_json::json!([{"win": "win-a", "data": data}]),
        ))
        .expect_err("wrong-typed data refuses");
        assert_eq!(error.kind, "malformed");
    }
    // The overbound fixture string is 1025 UTF-8 bytes: one past the slot.
    let overbound = nested_record("invalid-intent-overbound");
    let data = overbound["data"].as_str().expect("record data string");
    assert_eq!(data.len(), 1025);
    let error =
        decode_request(&envelope(serde_json::json!([overbound]))).expect_err("overbound refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "intent-data-invalid");
}

#[test]
fn section_cap_refuses_at_10001_and_accepts_at_10000() {
    let rows: Vec<serde_json::Value> = (0..10_000)
        .map(|i| serde_json::json!({"win": format!("win-{i}"), "data": "{\"v\":1}"}))
        .collect();
    assert_eq!(rows.len(), 10_000);
    let report = decode_request(&envelope(serde_json::json!(rows))).expect("cap admits");
    assert_eq!(report.intent_records.len(), 10_000);
    let too_many = (0..10_001)
        .map(|i| serde_json::json!({"win": format!("win-{i}"), "data": "{\"v\":1}"}))
        .collect::<Vec<_>>();
    assert_eq!(too_many.len(), 10_001);
    // Keep the byte bound out of the cap witness: tiny payloads isolate
    // the section cap from the slot bound.
    let error =
        decode_request(&envelope(serde_json::json!(too_many))).expect_err("over cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

#[test]
fn typed_direct_construction_refuses_the_same_outer_bounds() {
    // Decode and typed-direct paths refuse the same outer bounds with the
    // same fixed vocabulary (the decoder's structural kinds stay
    // malformed/unknown-value/unknown-field; bound verdicts match here).
    let mut report = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    assert_eq!(validate_report_v4_reason(&report), None);
    report.intent_records.push(IntentRecord {
        win: WindowId("bad id!".to_owned()),
        data: "{\"v\":1}".to_owned(),
    });
    assert_eq!(
        validate_report_v4_reason(&report),
        Some("window-id-invalid")
    );
    report.intent_records[0].win = WindowId("win-a".to_owned());
    report.intent_records[0].data = "x".repeat(1025);
    assert_eq!(report.intent_records[0].data.len(), 1025);
    assert_eq!(
        validate_report_v4_reason(&report),
        Some("intent-data-invalid")
    );
    report.intent_records[0].data = "x".repeat(1024);
    assert_eq!(validate_report_v4_reason(&report), None);
    report.intent_records = (0..10_001)
        .map(|i| IntentRecord {
            win: WindowId(format!("win-{i}")),
            data: "{\"v\":1}".to_owned(),
        })
        .collect();
    assert_eq!(validate_report_v4_reason(&report), Some("section-over-cap"));
}

#[test]
fn outer_string_bound_is_utf8_bytes_not_chars() {
    // 512 "e-acute" chars are exactly 1024 UTF-8 bytes: admitted.
    let at_bound = "\u{e9}".repeat(512);
    assert_eq!(at_bound.len(), 1024);
    assert_eq!(at_bound.chars().count(), 512);
    let report = decode_request(&envelope(
        serde_json::json!([{"win": "win-a", "data": at_bound}]),
    ))
    .expect("1024 bytes admit");
    assert_eq!(report.intent_records[0].data.len(), 1024);
    // One more multibyte char is 1026 bytes: refused.
    let over = format!("{at_bound}\u{e9}");
    assert_eq!(over.len(), 1026);
    let error = decode_request(&envelope(
        serde_json::json!([{"win": "win-a", "data": over}]),
    ))
    .expect_err("multibyte overbound refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "intent-data-invalid");
}

#[test]
fn digest_separates_every_carried_record_byte() {
    let base = decode_request(&envelope(serde_json::json!([
        {"win": "win-a", "data": "{\"v\":1,\"float\":true,\"stickyPrior\":null}"},
        {"win": "win-b", "data": "{\"v\":1,\"float\":null,\"stickyPrior\":null}"},
    ])))
    .expect("decodes");
    let witness = report_digest_v4(&base);
    // Inner whitespace is an outer byte change: the digest separates it
    // even though the inner record is semantically identical.
    let mut spaced = base.clone();
    spaced.intent_records[0].data = " {\"v\":1,\"float\":true,\"stickyPrior\":null} ".to_owned();
    assert_ne!(report_digest_v4(&spaced), witness, "whitespace separates");
    assert_ne!(spaced, base);
    // Subject identity, order, and content each separate the digest.
    let mut renamed = base.clone();
    renamed.intent_records[0].win = WindowId("win-c".to_owned());
    assert_ne!(report_digest_v4(&renamed), witness, "identity separates");
    let mut reordered = base.clone();
    reordered.intent_records.swap(0, 1);
    assert_ne!(report_digest_v4(&reordered), witness, "order separates");
    let mut edited = base.clone();
    edited.intent_records[1].data = "{\"v\":1,\"float\":false,\"stickyPrior\":null}".to_owned();
    assert_ne!(report_digest_v4(&edited), witness, "content separates");
    // An identical clone digests identically.
    assert_eq!(report_digest_v4(&base.clone()), witness);
}

#[test]
fn engine_accepts_opaque_record_and_publishes_empty_writes() {
    // The engine consumes no record authority in this increment: an
    // unsupported-inner-version record still accepts, with no behavior
    // change and no published writes.
    let row = nested_record("report-intent-unsupported-inner-version");
    let report = decode_request(&envelope(serde_json::json!([row]))).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(reply.intent_writes.is_empty(), "no writes this increment");
    // Empty-record twin: the opaque record changes no behavior versus no
    // records at all (identical published reply on a fresh engine).
    let mut twin = connected_engine("gen-1", HostCapabilities::default());
    let empty = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    let twin_reply = match twin.handle_report(&empty) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(reply, twin_reply, "opaque record is behavior-free");
    // The required section publishes empty on the canonical writer,
    // after the presentation and before diagnostics.
    let wire = reply_wire(&reply);
    let presentation_end = wire
        .find("\"presentation\"")
        .expect("presentation publishes");
    let writes = wire
        .find("\"intentWrites\":[]")
        .expect("empty writes publish");
    let diagnostics = wire.find("\"diagnostics\"").expect("diagnostics publish");
    assert!(
        presentation_end < writes && writes < diagnostics,
        "key order presentation < intentWrites < diagnostics, got {wire}"
    );
}

#[test]
fn canonical_save_and_clear_spell_exact_key_order() {
    // The reply fixture's save + null-clear rows spell exactly: save data
    // as an escaped string, clear as an explicit null.
    let save = nested_write("reply-intent-save-clear", 0);
    let clear = nested_write("reply-intent-save-clear", 1);
    assert!(clear["data"].is_null(), "fixture clear is null");
    let save_data = save["data"].as_str().expect("save data string").to_owned();
    assert_eq!(save_data, "{\"v\":1,\"float\":true,\"stickyPrior\":false}");
    let report = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    reply.intent_writes = vec![
        IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some(save_data),
        },
        IntentWrite {
            win: WindowId("win-b".to_owned()),
            data: None,
        },
    ];
    let wire = reply_wire(&reply);
    assert!(
        wire.contains(
            "\"intentWrites\":[{\"win\":\"win-a\",\"data\":\"{\\\"v\\\":1,\
             \\\"float\\\":true,\\\"stickyPrior\\\":false}\"},{\"win\":\"win-b\",\"data\":null}]"
        ),
        "exact save/clear bytes, got {wire}"
    );
}

#[test]
fn estimates_count_escaped_bytes_and_both_arrays() {
    // Escapes (quote, backslash, C0) and multibyte chars count exact
    // escaped widths on both lanes.
    let data = "a\"b\\c\x01\u{e9}d".to_owned();
    let records = vec![IntentRecord {
        win: WindowId("w".to_owned()),
        data: data.clone(),
    }];
    let writes = vec![
        IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(data.clone()),
        },
        IntentWrite {
            win: WindowId("x".to_owned()),
            data: None,
        },
    ];
    // Hand-built JSON for the same rows: lengths must match the helpers.
    let escaped = "a\\\"b\\\\c\\u0001\u{e9}d";
    let record_json = format!("{{\"win\":\"w\",\"data\":\"{escaped}\"}}");
    let write_json =
        format!("{{\"win\":\"w\",\"data\":\"{escaped}\"}},{{\"win\":\"x\",\"data\":null}}");
    assert_eq!(
        intent_records_estimate(&records),
        19 + record_json.len(),
        "records: empty base plus exact row bytes"
    );
    assert_eq!(
        intent_writes_estimate(&writes),
        18 + write_json.len(),
        "writes: empty base plus exact rows with their comma"
    );
    // The reply estimator is exact over the canonical encoding, escapes
    // included.
    let mut report = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    report.intent_records = records;
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let mut reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    reply.intent_writes = writes;
    let wire = reply_wire(&reply);
    // The reply estimator is exact over the new section (the wider reply
    // base carries pre-existing one-count conservatism, so the witness
    // pins the delta plus the standing upper-bound convention).
    let mut empty_reply = reply.clone();
    empty_reply.intent_writes = Vec::new();
    let empty_wire = reply_wire(&empty_reply);
    assert_eq!(
        reply_estimate_v4(&reply) - reply_estimate_v4(&empty_reply),
        wire.len() - empty_wire.len(),
        "reply estimate delta is exact over the new rows"
    );
    assert!(
        reply_estimate_v4(&reply) >= wire.len(),
        "reply estimate bounds the wire"
    );
    // The request estimator grows by exactly the carried rows on the record
    // lane: a maxed-out `intentRecords` section alone exceeds the 4 MiB
    // candidate budget through `report_estimate_v4`. The outbound
    // `intentWrites` lane is not estimated here: the typed helper plus the
    // publication commit gate refuse over-cap/invalid rows, the
    // pre-mutation bound carries no max-write reservation, and the envelope
    // slack covers the empty 18-byte section (see
    // `outbound_write_rows_share_the_fixed_outer_vocabulary` and
    // `published_replies_originate_valid_under_the_commit_gate`).
    let wire_len = envelope(serde_json::json!([])).len();
    assert!(
        report_estimate_v4(&report) >= wire_len,
        "estimate bounds the wire"
    );
    let mut heavy = report.clone();
    heavy.intent_records = (0..10_000)
        .map(|i| IntentRecord {
            win: WindowId(format!("win-{i}")),
            data: "x".repeat(1024),
        })
        .collect();
    assert!(report_estimate_v4(&heavy) > 4 * 1024 * 1024);
    assert_eq!(WORLD_CONTRACT_VERSION_V4, 4);
}

#[test]
fn outbound_write_rows_share_the_fixed_outer_vocabulary() {
    // Empty (the engine's steady state this increment) is trivially valid.
    assert_eq!(validate_intent_writes_reason(&[]), None);
    // Save and clear spellings with valid subjects admit.
    let writes = vec![
        IntentWrite {
            win: WindowId("win-a".to_owned()),
            data: Some("{\"v\":1}".to_owned()),
        },
        IntentWrite {
            win: WindowId("win-b".to_owned()),
            data: None,
        },
    ];
    assert_eq!(validate_intent_writes_reason(&writes), None);
    // Bad subjects refuse with the same fixed reason as the record lane on
    // both the save and the clear spelling; no bytes are echoed.
    for data in [Some("{\"v\":1}".to_owned()), None] {
        let bad = vec![IntentWrite {
            win: WindowId("bad id!".to_owned()),
            data: data.clone(),
        }];
        assert_eq!(
            validate_intent_writes_reason(&bad),
            Some("window-id-invalid")
        );
    }
    // ASCII slot bound: 1024 admits, 1025 refuses.
    let save_1024 = "x".repeat(1024);
    assert_eq!(save_1024.len(), 1024);
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(save_1024),
        }]),
        None
    );
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some("x".repeat(1025)),
        }]),
        Some("intent-data-invalid")
    );
    // UTF-8 bytes, not chars: 512 e-acute are exactly 1024 bytes (admit);
    // one more multibyte char is 1026 (refuse).
    let at_bound = "\u{e9}".repeat(512);
    assert_eq!(at_bound.len(), 1024);
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(at_bound.clone()),
        }]),
        None
    );
    let over = format!("{at_bound}\u{e9}");
    assert_eq!(over.len(), 1026);
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(over),
        }]),
        Some("intent-data-invalid")
    );
    // Escapes ride raw: quote, backslash, and C0 bytes count once each
    // toward the 1024 raw-byte bound even though the wire spells them
    // wider.
    let mut escaped = String::from("a\"b\\c\x01");
    assert_eq!(escaped.len(), 6);
    escaped.push_str(&"x".repeat(1024 - escaped.len()));
    assert_eq!(escaped.len(), 1024);
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(escaped.clone()),
        }]),
        None
    );
    escaped.push('y');
    assert_eq!(escaped.len(), 1025);
    assert_eq!(
        validate_intent_writes_reason(&[IntentWrite {
            win: WindowId("w".to_owned()),
            data: Some(escaped),
        }]),
        Some("intent-data-invalid")
    );
    // Section cap: 10000 admits, 10001 refuses; the cap precedes row checks.
    let at_cap: Vec<IntentWrite> = (0..10_000)
        .map(|i| IntentWrite {
            win: WindowId(format!("win-{i}")),
            data: Some("{\"v\":1}".to_owned()),
        })
        .collect();
    assert_eq!(at_cap.len(), 10_000);
    assert_eq!(validate_intent_writes_reason(&at_cap), None);
    let mut over_cap = at_cap;
    over_cap.push(IntentWrite {
        win: WindowId("win-last".to_owned()),
        data: None,
    });
    assert_eq!(
        validate_intent_writes_reason(&over_cap),
        Some("section-over-cap")
    );
}

#[test]
fn published_replies_originate_valid_under_the_commit_gate() {
    // The engine emits no writes this increment; the commit gate proves the
    // outbound helper over the real publication (not a fixture copy), and
    // the canonical writer spells the publication while the independent
    // core byte count plus a decoder roundtrip pin it under that
    // precondition.
    let report = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert_eq!(validate_intent_writes_reason(&reply.intent_writes), None);
    let wire = reply_wire(&reply);
    // The pre-mutation bound covers the published bytes, including the
    // empty 18-byte `intentWrites` section inside the envelope slack.
    assert!(
        reply_upper_bound_v4(&report, 0, 0, report.screens.len(), 0, 0) >= wire.len(),
        "pre-mutation bound covers the published reply"
    );
}

#[test]
fn duplicate_keys_inside_intent_row_refuse_on_the_literal_wire() {
    // A `serde_json::Value` construction silently keeps the last duplicate,
    // so the witness must be literal wire: the strict transport parser
    // refuses decoded duplicates at every nesting depth.
    let base = envelope(serde_json::json!([]));
    for rows in [
        "[{\"win\":\"win-a\",\"data\":\"x\",\"data\":\"y\"}]",
        "[{\"win\":\"win-a\",\"win\":\"win-b\",\"data\":\"x\"}]",
        // Escaped spelling decodes to the same key, so it duplicates too.
        "[{\"win\":\"win-a\",\"data\":\"x\",\"\\u0064ata\":\"y\"}]",
    ] {
        let wire = base.replace("\"intentRecords\":[]", &format!("\"intentRecords\":{rows}"));
        let error = decode_request(&wire).expect_err("duplicate refuses");
        assert_eq!(error.kind, "malformed", "rows {rows}");
    }
}

#[test]
fn reused_seq_with_only_intent_bytes_changed_reuses_nothing() {
    // Same reqSeq with edited raw intent bytes is not a duplicate: the
    // digest separates every carried byte, so the engine refuses reuse and
    // keeps the prior cached reply and state for the true duplicate.
    let first = decode_request(&envelope(serde_json::json!([
        {"win": "win-a", "data": "{\"v\":1,\"float\":true,\"stickyPrior\":null}"},
    ])))
    .expect("decodes");
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    let reply = match engine.handle_report(&first) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let mut edited = first.clone();
    edited.intent_records[0].data = "{\"v\":1,\"float\":false,\"stickyPrior\":null}".to_owned();
    assert_ne!(edited, first);
    match engine.handle_report(&edited) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected request-reused, got {other:?}"),
    }
    // The refused edit committed nothing: the identical original still hits
    // the cached reply byte-for-byte.
    match engine.handle_report(&first) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached, reply),
        other => panic!("expected duplicate, got {other:?}"),
    }
}

#[test]
fn heavy_intent_report_refuses_oversized_without_mutation() {
    // Typed-direct heavy section (no wire involved): the pre-mutation
    // estimate gate refuses before any convergence or slot work, so a
    // follow-up report still accepts on the unmutated engine.
    let mut base = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    base.intent_records = (0..10_000)
        .map(|i| IntentRecord {
            win: WindowId(format!("win-{i}")),
            data: "x".repeat(1024),
        })
        .collect();
    assert!(report_estimate_v4(&base) > 4 * 1024 * 1024);
    let mut engine = connected_engine("gen-1", HostCapabilities::default());
    match engine.handle_report(&base) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "oversized"),
        other => panic!("expected oversized, got {other:?}"),
    }
    // Untouched engine: the next sequence accepts normally.
    let mut next = decode_request(&envelope(serde_json::json!([]))).expect("decodes");
    next.request_seq = base.request_seq + 1;
    next.observation_seq = base.observation_seq + 1;
    next.request_id = "req-2".to_owned();
    match engine.handle_report(&next) {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted, got {other:?}"),
    }
}
