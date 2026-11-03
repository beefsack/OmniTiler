//! Required native-state attempt lane (`stateAttempts`).
//!
//! Raw fullscreen/maximize/sticky/above write/proof rows ride every staged
//! report (possibly empty); the core owns every verdict. The nested rows
//! below are extracted from the immutable v4 fixtures and run through the
//! staged wire path: the in-progress envelope decoder preserves them as
//! typed mechanical facts, closed shape/counter/duplicate/cap gates refuse
//! with fixed vocabulary, the digest separates every raw field so changed
//! reuse still fences, and the size estimate covers the rows exactly. The
//! full v4 envelopes are NOT decoded here: they stage lanes beyond this
//! increment (focusFeedback, rendererInput) and a newer wire version.
//!
//! Offline only: no D-Bus, no KWin.

use tiler_core::directional::WindowId;
use tiler_core::world_report::{
    FrameSetter, StateAttempt, StateProperty, report_digest, report_estimate,
    validate_report_reason,
};
use tiler_protocol::world_protocol::{decode_request, decode_state_attempt};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture bytes"))
        .expect("fixture JSON")
}

/// Otherwise-accepted staged envelope (v3 shape) carrying the state rows
/// under test. Raw rows ride verbatim; only this lane varies.
fn envelope(rows: &serde_json::Value) -> String {
    format!(
        "{{\"v\":3,\"adapter\":\"adapter-1\",\"gen\":\"gen-1\",\
          \"obs\":\"1\",\"reqSeq\":\"1\",\"req\":\"req-1\",\"cmdSeq\":null,\
          \"command\":null,\
          \"screens\":[{{\"out\":\"out\",\"full\":[0,0,800,600],\"work\":[0,0,800,560],\
          \"complete\":true,\"scale\":1.0,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{{\"win\":\"win-a\",\"out\":\"out\",\"rect\":[0,0,200,200],\
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
          \"lastWrite\":null}}],\
          \"views\":[{{\"out\":\"out\",\"view\":\"\"}}],\"closed\":[],\"native\":[],\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"out\"],\
          \"focused\":\"\",\"complete\":true,\"gesture\":false,\"presentation\":{{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}},\
          \"intentRecords\":[],\"stateAttempts\":{rows}}}"
    )
}

#[allow(clippy::too_many_arguments)]
fn state_row(
    win: &str,
    property: &str,
    target: bool,
    rev: &str,
    setter: &str,
    pre: serde_json::Value,
    post: serde_json::Value,
    attempt_ms: &str,
) -> serde_json::Value {
    serde_json::json!({
        "win": win, "property": property, "target": target, "rev": rev,
        "setter": setter, "pre": pre, "post": post, "attemptMs": attempt_ms,
    })
}

#[test]
fn immutable_state_rows_decode_through_the_typed_shape() {
    // Raw fixture bytes first: the oracle is the immutable file, never a
    // decoder-shaped map.
    let state = fixture("report-attempt-state");
    let rows = state["stateAttempts"]
        .as_array()
        .expect("fixture stateAttempts");
    assert_eq!(rows.len(), 3, "report-attempt-state carries three rows");
    assert_eq!(rows[0]["win"], "win-a");
    assert_eq!(rows[0]["property"], "fullscreen");
    assert_eq!(rows[0]["target"], false);
    assert_eq!(rows[0]["rev"], "1");
    assert_eq!(rows[0]["setter"], "accepted");
    assert_eq!(rows[0]["pre"], true);
    assert_eq!(rows[0]["post"], false);
    assert_eq!(rows[1]["property"], "maximized");
    assert_eq!(rows[1]["target"], true);
    assert_eq!(rows[1]["setter"], "refused");
    assert_eq!(rows[2]["property"], "sticky");
    assert_eq!(rows[2]["setter"], "threw");
    assert_eq!(rows[2]["pre"], serde_json::Value::Null);
    assert_eq!(rows[2]["post"], serde_json::Value::Null);
    let above_true = fixture("report-attempt-above-true");
    let above_row = &above_true["stateAttempts"][0];
    assert_eq!(above_row["property"], "above");
    assert_eq!(above_row["target"], true);
    assert_eq!(above_row["pre"], false);
    assert_eq!(above_row["post"], true);
    let above_false = fixture("report-attempt-above-false");
    let below_row = &above_false["stateAttempts"][0];
    assert_eq!(below_row["property"], "above");
    assert_eq!(below_row["target"], false);
    assert_eq!(below_row["pre"], true);
    assert_eq!(below_row["post"], false);

    // Then the typed mechanical shape: exact target correlation, quoted
    // decimal stamps, nullable immediate proofs. Same window across
    // distinct properties is three facts, never a duplicate.
    assert_eq!(
        decode_state_attempt(&rows[0]).expect("fullscreen row decodes"),
        StateAttempt {
            window: WindowId("win-a".into()),
            property: StateProperty::Fullscreen,
            target: false,
            revision: 1,
            setter: FrameSetter::Accepted,
            pre: Some(true),
            post: Some(false),
            attempt_ms: 20,
        }
    );
    assert_eq!(
        decode_state_attempt(&rows[1])
            .expect("maximized row decodes")
            .property,
        StateProperty::Maximized
    );
    assert_eq!(
        decode_state_attempt(&rows[2])
            .expect("sticky row decodes")
            .pre,
        None
    );
    assert_eq!(
        decode_state_attempt(above_row).expect("above-true row decodes"),
        StateAttempt {
            window: WindowId("win-a".into()),
            property: StateProperty::Above,
            target: true,
            revision: 1,
            setter: FrameSetter::Accepted,
            pre: Some(false),
            post: Some(true),
            attempt_ms: 20,
        }
    );
    assert!(
        !decode_state_attempt(below_row)
            .expect("above-false row decodes")
            .target
    );
    // The staged envelope accepts the immutable rows verbatim.
    let report = decode_request(&envelope(&serde_json::Value::Array(rows.clone())))
        .expect("immutable rows ride the staged envelope");
    assert_eq!(report.state_attempts.len(), 3);
    assert!(
        report
            .state_attempts
            .iter()
            .any(|attempt| attempt.property == StateProperty::Sticky
                && attempt.setter == FrameSetter::Threw
                && attempt.pre.is_none())
    );
}

#[test]
fn missing_lane_refuses_without_backfill() {
    let mut value: serde_json::Value =
        serde_json::from_str(&envelope(&serde_json::json!([]))).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("stateAttempts");
    let error = decode_request(&value.to_string()).expect_err("missing refuses");
    assert_eq!(error.kind, "malformed");
}

#[test]
fn closed_shape_refuses_unknowns_and_bad_types() {
    let base = || {
        state_row(
            "win-a",
            "above",
            true,
            "20",
            "accepted",
            serde_json::json!(false),
            serde_json::json!(true),
            "20",
        )
    };
    // Unknown keys refuse as unknown-field; the eight closed keys stay
    // required.
    for (row, kind) in [
        (
            serde_json::json!({"win":"win-a","property":"above","target":true,"rev":"20","setter":"accepted","pre":false,"post":true,"attemptMs":"20","extra":1}),
            "unknown-field",
        ),
        // Unknown property/setter tokens refuse as unknown-value.
        (
            state_row(
                "win-a",
                "visibility",
                true,
                "20",
                "accepted",
                serde_json::json!(false),
                serde_json::json!(true),
                "20",
            ),
            "unknown-value",
        ),
        (
            state_row(
                "win-a",
                "above",
                true,
                "20",
                "retried",
                serde_json::json!(false),
                serde_json::json!(true),
                "20",
            ),
            "unknown-value",
        ),
        // Quoted decimal stamps only: numbers, signs, blanks refuse.
        (
            state_row(
                "win-a",
                "above",
                true,
                "20",
                "accepted",
                serde_json::json!(false),
                serde_json::json!(true),
                "020",
            ),
            "unknown-value",
        ),
        (
            state_row(
                "win-a",
                "above",
                true,
                "",
                "accepted",
                serde_json::json!(false),
                serde_json::json!(true),
                "20",
            ),
            "unknown-value",
        ),
        (
            state_row(
                "win-a",
                "above",
                true,
                "18446744073709551616",
                "accepted",
                serde_json::json!(false),
                serde_json::json!(true),
                "20",
            ),
            "unknown-value",
        ),
        // Wrong JSON types refuse: bools stay bools, ids stay strings.
        (
            state_row(
                "win-a",
                "above",
                true,
                "20",
                "accepted",
                serde_json::json!("false"),
                serde_json::json!(true),
                "20",
            ),
            "malformed",
        ),
        (
            state_row(
                "win-a",
                "above",
                true,
                "20",
                "accepted",
                serde_json::json!(false),
                serde_json::json!(0),
                "20",
            ),
            "malformed",
        ),
    ] {
        let error =
            decode_request(&envelope(&serde_json::json!([row]))).expect_err("shape refuses");
        assert_eq!(error.kind, kind);
    }
    // Untouched coverage for the remaining arms: bad window id, JSON
    // number stamps, null target, and dropped required keys.
    let mut bad_win = base();
    bad_win["win"] = serde_json::json!("not an id!!");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_win])))
            .expect_err("bad id refuses")
            .kind,
        "unknown-value"
    );
    let mut num_rev = base();
    num_rev["rev"] = serde_json::json!(20);
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([num_rev])))
            .expect_err("number stamp refuses")
            .kind,
        "malformed"
    );
    let mut null_target = base();
    null_target["target"] = serde_json::Value::Null;
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([null_target])))
            .expect_err("null target refuses")
            .kind,
        "malformed"
    );
    let mut dropped = base();
    dropped.as_object_mut().expect("object").remove("pre");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([dropped])))
            .expect_err("dropped key refuses")
            .kind,
        "malformed"
    );
}

#[test]
fn duplicates_refuse_per_window_property_across_revisions() {
    // Same window plus property at a different revision is still one
    // latest-wins entry for that property, never two settled facts.
    let rows = serde_json::json!([
        state_row(
            "win-a",
            "above",
            true,
            "1",
            "accepted",
            serde_json::json!(false),
            serde_json::json!(true),
            "20"
        ),
        state_row(
            "win-a",
            "above",
            false,
            "2",
            "accepted",
            serde_json::json!(true),
            serde_json::json!(false),
            "21"
        ),
    ]);
    let error = decode_request(&envelope(&rows)).expect_err("duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-state-attempt");
    // Bad window ids refuse at the wire alphabet gate.
    let rows = serde_json::json!([state_row(
        "not an id!!",
        "above",
        true,
        "1",
        "accepted",
        serde_json::json!(false),
        serde_json::json!(true),
        "20"
    ),]);
    let error = decode_request(&envelope(&rows)).expect_err("bad id refuses");
    assert_eq!(error.kind, "unknown-value");
    // The core gate names the same lane for directly constructed rows
    // (duplicate delivery of a decoded report never reaches it).
    let mut report = decode_request(&envelope(&serde_json::json!([]))).expect("empty decodes");
    report.state_attempts.push(StateAttempt {
        window: WindowId("not an id!!".into()),
        property: StateProperty::Above,
        target: true,
        revision: 1,
        setter: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(true),
        attempt_ms: 20,
    });
    assert_eq!(
        validate_report_reason(&report),
        Some("state-window-invalid")
    );
}

#[test]
fn cap_1024_accepts_and_1025_refuses() {
    fn rows(count: usize) -> serde_json::Value {
        let properties = ["fullscreen", "maximized", "sticky", "above"];
        serde_json::Value::Array(
            (0..count)
                .map(|n| {
                    state_row(
                        &format!("w{n:04}"),
                        properties[n % properties.len()],
                        n % 2 == 0,
                        "7",
                        "accepted",
                        serde_json::json!(false),
                        serde_json::json!(true),
                        "9",
                    )
                })
                .collect(),
        )
    }
    let report = decode_request(&envelope(&rows(1024))).expect("cap accepts");
    assert_eq!(report.state_attempts.len(), 1024);
    let error = decode_request(&envelope(&rows(1025))).expect_err("cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

#[test]
fn digest_separates_every_raw_field_so_changed_reuse_fences() {
    let row = || {
        state_row(
            "win-a",
            "above",
            true,
            "20",
            "accepted",
            serde_json::json!(false),
            serde_json::json!(true),
            "20",
        )
    };
    let digest = |row: serde_json::Value| {
        let report = decode_request(&envelope(&serde_json::json!([row]))).expect("decodes");
        report_digest(&report)
    };
    let baseline = digest(row());
    // Identical bytes redeliver the identical digest: reuse fences hold.
    assert_eq!(digest(row()), baseline);
    // Every raw field, presence bit, and the original stamp separates the
    // digest: a changed row never aliases the accepted one.
    let mut cases: Vec<(&str, serde_json::Value)> = Vec::new();
    let mut win = row();
    win["win"] = serde_json::json!("win-b");
    cases.push(("win", win));
    let mut property = row();
    property["property"] = serde_json::json!("sticky");
    cases.push(("property", property));
    let mut target = row();
    target["target"] = serde_json::json!(false);
    cases.push(("target", target));
    let mut rev = row();
    rev["rev"] = serde_json::Value::String("21".to_owned());
    cases.push(("rev", rev));
    let mut setter = row();
    setter["setter"] = serde_json::json!("refused");
    cases.push(("setter", setter));
    let mut pre = row();
    pre["pre"] = serde_json::Value::Null;
    cases.push(("pre-presence", pre));
    let mut post = row();
    post["post"] = serde_json::json!(false);
    cases.push(("post", post));
    let mut stamp = row();
    stamp["attemptMs"] = serde_json::Value::String("21".to_owned());
    cases.push(("attemptMs", stamp));
    assert_eq!(cases.len(), 8);
    for (case, row) in cases {
        assert_ne!(digest(row), baseline, "{case} separates the digest");
    }
}

#[test]
fn estimate_covers_state_rows_exactly_at_the_margin() {
    let rows = fixture("report-attempt-state")["stateAttempts"].clone();
    let empty = envelope(&serde_json::json!([]));
    let full = envelope(&rows);
    let empty_report = decode_request(&empty).expect("empty decodes");
    let full_report = decode_request(&full).expect("rows decode");
    assert_eq!(full_report.state_attempts.len(), 3);
    let empty_estimate = report_estimate(&empty_report);
    let full_estimate = report_estimate(&full_report);
    assert!(
        empty_estimate >= empty.len(),
        "estimate {empty_estimate} bounds empty wire {}",
        empty.len()
    );
    assert!(
        full_estimate >= full.len(),
        "estimate {full_estimate} bounds row wire {}",
        full.len()
    );
    // The lane accounts every additional key, null, and string exactly:
    // a single row's marginal estimate equals its marginal wire bytes
    // (multi-row arrays share the existing inter-row comma slack with
    // every other attempt lane).
    let one = envelope(&serde_json::json!([state_row(
        "win-a",
        "sticky",
        true,
        "1",
        "threw",
        serde_json::Value::Null,
        serde_json::Value::Null,
        "20",
    )]));
    let one_report = decode_request(&one).expect("single row decodes");
    assert_eq!(one_report.state_attempts.len(), 1);
    assert_eq!(
        report_estimate(&one_report) - empty_estimate,
        one.len() - empty.len(),
        "one state row costs its exact wire bytes"
    );
}

#[test]
fn wire_level_duplicates_and_trailing_refuse() {
    let wire = envelope(&serde_json::json!([state_row(
        "win-a",
        "above",
        true,
        "20",
        "accepted",
        serde_json::json!(false),
        serde_json::json!(true),
        "20"
    ),]));
    decode_request(&wire).expect("valid staged request decodes");
    // Literal duplicate row key refuses (scoped to the state row: the
    // envelope spells rows with sorted keys, and the windows array
    // carries its own `win` fact earlier in the wire).
    let nested = wire.replacen(
        "\"target\":true,\"win\":\"win-a\"",
        "\"target\":true,\"win\":\"win-a\",\"win\":\"win-a\"",
        1,
    );
    assert_ne!(nested, wire);
    let error = decode_request(&nested).expect_err("duplicate row key refuses");
    assert_eq!(error.kind, "malformed");
    // Escaped-equivalent duplicate row key (`\u0077` is `w`) refuses.
    let escaped = wire.replacen(
        "\"target\":true,\"win\":\"win-a\"",
        "\"target\":true,\"win\":\"win-a\",\"\\u0077in\":\"win-a\"",
        1,
    );
    assert_ne!(escaped, wire);
    let error = decode_request(&escaped).expect_err("escaped duplicate refuses");
    assert_eq!(error.kind, "malformed");
    // Trailing tokens after an otherwise-valid request refuse.
    for trailing in [format!("{wire} {{}}"), format!("{wire}x")] {
        let error = decode_request(&trailing).expect_err("trailing refuses");
        assert_eq!(error.kind, "malformed");
    }
    // Harmless control: a single escaped key spelling still decodes.
    let single = wire.replacen("\"property\"", "\"\\u0070roperty\"", 1);
    assert_ne!(single, wire);
    decode_request(&single).expect("single escaped key decodes");
}
