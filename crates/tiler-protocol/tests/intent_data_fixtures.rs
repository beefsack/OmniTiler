//! Fixture-backed coverage for the core-owned opaque intent data codec.
//!
//! The nested `intentRecords[].data` / `intentWrites[].data` strings are
//! extracted from the immutable v4 wire fixtures and run through the core
//! decoder/canonical encoder in `tiler_core::intent_data`. Supported
//! positives must canonical-roundtrip byte-for-byte; the unsupported inner
//! version is ignored as unusable inner data while its envelope stays a
//! valid fixture. Decoder errors never bypass outer validation: the record
//! `data` still must be a string within the slot bound, an independent
//! whole-envelope responsibility. Outer opaque invalid fixtures only guide
//! the byte/string bounds; full wire codecs are out of scope here.

use tiler_core::intent_data::{
    IntentData, IntentDataError, decode_intent_data, encode_intent_data,
};

macro_rules! fixture {
    ($name:literal) => {
        serde_json::from_str::<serde_json::Value>(include_str!(concat!(
            "../../../test-fixtures/world-wire/v4/messages/",
            $name,
            ".json"
        )))
        .expect("fixture JSON")
    };
}

#[test]
fn positive_intent_records_canonical_roundtrip() {
    let cases = [
        (
            fixture!("report-intent-float"),
            "report-intent-float",
            IntentData::new(Some(true), Some(true)),
        ),
        (
            fixture!("report-intent-tile"),
            "report-intent-tile",
            IntentData::new(Some(false), Some(false)),
        ),
        (
            fixture!("report-intent-null"),
            "report-intent-null",
            IntentData::new(None, None),
        ),
    ];
    for (envelope, name, expected) in cases {
        let records = envelope["intentRecords"].as_array().expect("intentRecords");
        assert_eq!(records.len(), 1, "{name}");
        let data = records[0]["data"].as_str().expect("record data string");
        assert_eq!(decode_intent_data(data), Ok(expected), "{name}");
        assert_eq!(encode_intent_data(&expected), data, "{name}");
    }
}

#[test]
fn reply_intent_writes_roundtrip_and_null_clears() {
    let envelope: serde_json::Value = fixture!("reply-intent-save-clear");
    let writes = envelope["intentWrites"].as_array().expect("intentWrites");
    assert_eq!(writes.len(), 2);
    let data = writes[0]["data"].as_str().expect("write data string");
    let expected = IntentData::new(Some(true), Some(false));
    assert_eq!(decode_intent_data(data), Ok(expected));
    assert_eq!(encode_intent_data(&expected), data);
    // A null write clears the lifetime-bound slot; there is no inner
    // record to decode.
    assert!(writes[1]["data"].is_null());
}

#[test]
fn unsupported_inner_version_rejected_as_data_only() {
    let envelope: serde_json::Value = fixture!("report-intent-unsupported-inner-version");
    // The envelope itself stays a valid fixture: one record for win-a.
    let records = envelope["intentRecords"].as_array().expect("intentRecords");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["win"], serde_json::json!("win-a"));
    let data = records[0]["data"].as_str().expect("record data string");
    // The inner version is unusable (no intent, no write authority from
    // this record); the envelope itself is not a transport refusal. Outer
    // validation (record `data` is a bounded string) still applies
    // independently and is not bypassed by ignoring inner data.
    assert_eq!(
        decode_intent_data(data),
        Err(IntentDataError::UnsupportedVersion)
    );
}

#[test]
fn outer_invalid_fixtures_guide_bounds() {
    // Overbound outer string exceeds the 1024 UTF-8-byte slot bound. The
    // decoder reports it, and the outer record independently rejects it as
    // a whole-envelope violation: ignoring invalid inner data never admits
    // an overbound or wrong-typed outer `data` field.
    let overbound: serde_json::Value = fixture!("invalid-intent-overbound");
    let data = overbound["intentRecords"][0]["data"]
        .as_str()
        .expect("record data string");
    assert!(data.len() > tiler_core::intent_data::MAX_INTENT_DATA_BYTES);
    assert_eq!(decode_intent_data(data), Err(IntentDataError::Overbound));
    // A non-string record payload is an outer shape violation, not inner data.
    let not_string: serde_json::Value = fixture!("invalid-intent-object-not-string");
    assert!(not_string["intentRecords"][0]["data"].is_object());
    // A null record payload is an outer shape violation: records require data
    // (only writes may clear with null).
    let outer_null: serde_json::Value = fixture!("invalid-intent-outer-null");
    assert!(outer_null["intentRecords"][0]["data"].is_null());
}
