//! Failing-first v3 native occupancy cutover regression.
//!
//! Wire shape under test: `native[]: {out, order, current, occupancy}` with
//! `occupancy: [{native, count: integer|null}]` covering exactly each `order`
//! id. `count: null` is explicit unknown; exact zero only on readable empty.

use tiler_protocol::world_protocol::decode_request;

fn base_request(native_value: &str) -> String {
    format!(
        "{{\"v\":3,\"owner\":\"owner-a\",\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\
         \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\
         \"command\":null,\
         \"screens\":[{{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
         \"gap\":0,\"outer\":0,\"complete\":true,\"primary\":true}}],\
         \"windows\":[],\
         \"views\":[{{\"out\":\"out\",\"view\":\"\"}}],\"closed\":[],\"mode\":\"per-output-local\",\
         \"hostOwned\":true,\"equalCount\":false,\"native\":[{native_value}],\
         \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
         \"switchTarget\":\"cursor-output\",\"freshPlacement\":\"cursor-output-last-focused\",\
         \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
         \"actionOutputs\":[\"out\"],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
         \"defaultTiled\":true,\"focused\":\"\",\
         \"complete\":true,\"gesture\":false,\"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}}}}"
    )
}

fn native_row(order: &str, current: &str, occupancy: &str) -> String {
    format!(
        "{{\"out\":\"out\",\"order\":[{order}],\"current\":{current},\"occupancy\":[{occupancy}]}}"
    )
}

#[test]
fn v2_envelope_refuses_as_unsupported_version() {
    let v2 = base_request("{\"out\":\"out\",\"order\":[\"d1\"],\"current\":\"d1\"}")
        .replace("\"v\":3", "\"v\":2");
    let error = decode_request(&v2).expect_err("v2 must refuse after v3 cutover");
    assert_eq!(error.kind, "unsupported-version");
}

#[test]
fn v3_requires_occupancy_key() {
    let missing = base_request("{\"out\":\"out\",\"order\":[\"d1\"],\"current\":\"d1\"}");
    assert!(
        decode_request(&missing).is_err(),
        "missing occupancy must refuse"
    );
}

#[test]
fn v3_accepts_empty_occupied_and_unknown_counts() {
    // Readable empty.
    let empty = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0}",
    ));
    assert!(decode_request(&empty).is_ok(), "exact zero accepts");
    // Occupied.
    let occupied = base_request(&native_row(
        "\"d1\",\"d2\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":2},{\"native\":\"d2\",\"count\":0}",
    ));
    assert!(decode_request(&occupied).is_ok(), "occupied accepts");
    // Explicit unknown.
    let unknown = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":null}",
    ));
    assert!(decode_request(&unknown).is_ok(), "null count accepts");
}

#[test]
fn v3_rejects_malformed_and_coverage_violations() {
    // Uncovered order id.
    let uncovered = base_request(&native_row(
        "\"d1\",\"d2\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0}",
    ));
    assert!(decode_request(&uncovered).is_err(), "uncovered refuses");
    // Extra occupancy id not in order.
    let extra = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0},{\"native\":\"d9\",\"count\":0}",
    ));
    assert!(decode_request(&extra).is_err(), "extra refuses");
    // Duplicate occupancy row.
    let duplicate = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":0},{\"native\":\"d1\",\"count\":1}",
    ));
    assert!(decode_request(&duplicate).is_err(), "duplicate refuses");
    // Malformed count shape (string).
    let malformed = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":\"two\"}",
    ));
    assert!(
        decode_request(&malformed).is_err(),
        "malformed count refuses"
    );
    // Negative count refuses.
    let negative = base_request(&native_row(
        "\"d1\"",
        "\"d1\"",
        "{\"native\":\"d1\",\"count\":-1}",
    ));
    assert!(decode_request(&negative).is_err(), "negative refuses");
}
