//! Native occupancy regression (genuine v4 vocabulary).
//!
//! Wire shape under test: `native[]: {out, order, current, occupancy}` with
//! `occupancy: [{native, count: integer|null}]` covering exactly each `order`
//! id. `count: null` is explicit unknown; exact zero only on readable empty.
//!
//! The request envelope below is a genuine v4 report: the required
//! `rendererInput` lane is carried, `views` is refused, and the v4 contract
//! version gates first. The unsupported-version case below uses an
//! otherwise valid v4 envelope with only `v` replaced, isolating the
//! version gate from shape rejection.

use tiler_protocol::world_protocol::decode_request;

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture bytes"))
        .expect("fixture JSON")
}

fn base_request(native_rows: serde_json::Value) -> String {
    let mut base = fixture("report-native");
    base["native"] = native_rows;
    base.to_string()
}

fn native_row(order: &str, current: &str, occupancy: &str) -> serde_json::Value {
    serde_json::from_str(&format!(
        "{{\"out\":\"out-a\",\"order\":[{order}],\"current\":{current},\"occupancy\":[{occupancy}]}}"
    ))
    .expect("native row builds")
}

#[test]
fn v2_envelope_refuses_as_unsupported_version() {
    let v2 = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":1}"
    )]))
    .replacen("\"v\":4", "\"v\":2", 1);
    let error = decode_request(&v2).expect_err("v2 must refuse after v4 cutover");
    assert_eq!(error.kind, "unsupported-version");
}

#[test]
fn v4_requires_occupancy_key() {
    let missing: serde_json::Value =
        serde_json::from_str("{\"out\":\"out-a\",\"order\":[\"d1\"],\"current\":\"d1\"}")
            .expect("row builds");
    assert!(
        decode_request(&base_request(serde_json::json!([missing]))).is_err(),
        "missing occupancy must refuse"
    );
}

#[test]
fn v4_accepts_empty_occupied_and_unknown_counts() {
    // Readable empty.
    let empty = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0}"
    )]));
    let report = decode_request(&empty).expect("exact zero accepts");
    assert_eq!(report.native.len(), 1);
    assert_eq!(report.native[0].occupancy[0].count, Some(0));
    // Occupied.
    let occupied = base_request(serde_json::json!([native_row(
        "\"d1\",\"d2\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":2},{\"native\":\"d2\",\"count\":0}"
    )]));
    let report = decode_request(&occupied).expect("occupied accepts");
    assert_eq!(report.native[0].occupancy.len(), 2);
    // Explicit unknown.
    let unknown = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":null}"
    )]));
    let report = decode_request(&unknown).expect("null count accepts");
    assert_eq!(report.native[0].occupancy[0].count, None);
}

#[test]
fn v4_rejects_malformed_and_coverage_violations() {
    // Uncovered order id.
    let uncovered = base_request(serde_json::json!([native_row(
        "\"d1\",\"d2\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0}"
    )]));
    assert!(decode_request(&uncovered).is_err(), "uncovered refuses");
    // Extra occupancy id not in order.
    let extra = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0},{\"native\":\"d9\",\"count\":0}"
    )]));
    assert!(decode_request(&extra).is_err(), "extra refuses");
    // Duplicate occupancy row.
    let duplicate = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0},{\"native\":\"d1\",\"count\":1}"
    )]));
    assert!(decode_request(&duplicate).is_err(), "duplicate refuses");
    // Malformed count shape (string).
    let malformed = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":\"two\"}"
    )]));
    assert!(
        decode_request(&malformed).is_err(),
        "malformed count refuses"
    );
    // Negative count refuses.
    let negative = base_request(serde_json::json!([native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":-1}"
    )]));
    assert!(decode_request(&negative).is_err(), "negative refuses");
}
