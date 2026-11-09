//! Required focus-feedback lane (`focusFeedback`) on the genuine v4 wire.
//!
//! Raw one-shot native focus evidence rides every staged report (possibly
//! empty); the core owns every verdict. The nested rows below are extracted
//! from the immutable v4 fixtures and run through the genuine v4 ingress:
//! [`decode_request`](tiler_protocol::world_protocol::decode_request)
//! preserves them as typed mechanical facts, closed shape/counter/cap gates
//! refuse with fixed vocabulary, the digest separates every raw field so
//! changed reuse still fences, and the size estimate covers the rows
//! exactly. The staged envelope carries the v4 wire version and the required
//! `rendererInput` lane.
//!
//! Offline only: no D-Bus, no KWin.

use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
};
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ArrivalLocation, VisibilityReason, VisibilitySetter, WorkspaceRef, WorldReportV4,
};
use tiler_core::world_report_v4_checks::report_digest_v4;
use tiler_core::world_sizing_v4::report_estimate_v4;
use tiler_protocol::world_protocol::decode_request;

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture bytes"))
        .expect("fixture JSON")
}

/// Otherwise-accepted v4 envelope carrying the focus rows under test. The
/// base is an immutable v4 fixture (valid screens, windows, presentation,
/// and the required `rendererInput` lane); only the identity stamps are
/// rebound to the connected test engine and only this lane varies. Raw rows
/// ride verbatim.
fn envelope(rows: &serde_json::Value) -> String {
    let mut base = fixture("report-focus-feedback-proven");
    base["adapter"] = serde_json::json!("adapter-1");
    base["gen"] = serde_json::json!("gen-1");
    base["req"] = serde_json::json!("req-1");
    base["focusFeedback"] = rows.clone();
    base.to_string()
}

fn nested_rows(name: &str) -> serde_json::Value {
    let proven = fixture(name);
    let rows = proven["focusFeedback"].clone();
    assert!(
        rows.as_array().is_some_and(|rows| !rows.is_empty()),
        "{name} carries focus rows"
    );
    rows
}

fn connected_engine(host_owned: bool) -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let handshake = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
        },
        capabilities: HostCapabilities {
            host_owned,
            native_sticky: true,
            maximize: true,
            ..HostCapabilities::default()
        },
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
fn required_lane_refuses_missing_and_null() {
    // The control envelope with a required empty lane decodes.
    let report = decode_request(&envelope(&serde_json::json!([]))).expect("empty decodes");
    assert!(report.focus_feedback.is_empty());
    // Missing refuses as malformed; explicit null is not a backfill.
    let mut value: serde_json::Value =
        serde_json::from_str(&envelope(&serde_json::json!([]))).expect("valid");
    value
        .as_object_mut()
        .expect("object")
        .remove("focusFeedback");
    let error = decode_request(&value.to_string()).expect_err("missing refuses");
    assert_eq!(error.kind, "malformed");
    let mut value: serde_json::Value =
        serde_json::from_str(&envelope(&serde_json::json!([]))).expect("valid");
    value["focusFeedback"] = serde_json::Value::Null;
    let error = decode_request(&value.to_string()).expect_err("null refuses");
    assert_eq!(error.kind, "malformed");
}

#[test]
fn immutable_focus_rows_decode_to_exact_typed_facts() {
    // Proven output-arm row: every carried field lands typed.
    let rows = nested_rows("report-focus-feedback-proven");
    assert_eq!(rows.as_array().expect("rows").len(), 1);
    let report = decode_request(&envelope(&rows)).expect("proven rows ride");
    assert_eq!(report.focus_feedback.len(), 1);
    let row = &report.focus_feedback[0];
    assert_eq!(row.id, "effect-a");
    assert_eq!(row.target.0, "win-a");
    assert_eq!(row.revision, 1);
    assert_eq!(row.setter, VisibilitySetter::Accepted);
    assert_eq!(row.reason, VisibilityReason::Empty);
    assert_eq!(row.pre.as_ref().map(|id| id.0.as_str()), Some("win-b"));
    assert_eq!(row.post.as_ref().map(|id| id.0.as_str()), Some("win-a"));
    assert!(row.foreground_proven);
    assert_eq!(row.attempt_ms, 20);
    assert_eq!(row.arrival.len(), 1);
    let bracket = &row.arrival[0];
    assert_eq!(bracket.window.0, "win-a");
    assert_eq!(bracket.obs, 1);
    assert_eq!(bracket.before.obs, 1);
    assert_eq!(bracket.before.lifetime_matched, Some(true));
    assert_eq!(bracket.before.visibility.visible, Some(true));
    assert_eq!(bracket.before.visibility.suppression_owned, Some(false));
    assert_eq!(bracket.before.minimized, Some(false));
    assert_eq!(bracket.before.host_hidden, Some(false));
    match &bracket.location {
        ArrivalLocation::Output { out } => {
            assert_eq!(out.as_ref().map(|id| id.0.as_str()), Some("out-a"))
        }
        ArrivalLocation::Native { .. } => panic!("proven rides the output arm"),
    }
    assert_eq!(
        bracket.frame.map(|rect| (rect.x, rect.y, rect.w, rect.h)),
        Some((8, 8, 800, 600))
    );
    assert_eq!(bracket.after.obs, 1);
    assert_eq!(bracket.after.lifetime_matched, Some(true));

    // Refused and threw rows: at-most-once dispositions with no brackets.
    let report =
        decode_request(&envelope(&nested_rows("report-focus-feedback-refused"))).expect("decodes");
    let row = &report.focus_feedback[0];
    assert_eq!(row.target.0, "win-b");
    assert_eq!(row.setter, VisibilitySetter::Refused);
    assert_eq!(row.reason, VisibilityReason::NativeRefused);
    assert!(!row.foreground_proven);
    assert!(row.arrival.is_empty());
    let report =
        decode_request(&envelope(&nested_rows("report-focus-feedback-threw"))).expect("decodes");
    let row = &report.focus_feedback[0];
    assert_eq!(row.setter, VisibilitySetter::Threw);
    assert_eq!(row.reason, VisibilityReason::Empty);
    assert_eq!(row.pre.as_ref().map(|id| id.0.as_str()), Some("win-a"));
    assert_eq!(row.post, None);
    assert!(row.arrival.is_empty());

    // Unreadable row: null endpoints, null native home, null frame, null
    // samples stay facts (never verdicts) through the typed shape.
    let report = decode_request(&envelope(&nested_rows("report-focus-feedback-unreadable")))
        .expect("decodes");
    let row = &report.focus_feedback[0];
    assert_eq!(row.setter, VisibilitySetter::NotCalled);
    assert_eq!(row.reason, VisibilityReason::ShowBlocked);
    assert_eq!(row.pre, None);
    assert_eq!(row.post, None);
    assert!(!row.foreground_proven);
    assert_eq!(row.arrival.len(), 1);
    let bracket = &row.arrival[0];
    match &bracket.location {
        ArrivalLocation::Native { home } => assert_eq!(home, &None),
        ArrivalLocation::Output { .. } => panic!("unreadable rides the native arm"),
    }
    assert_eq!(bracket.frame, None);
    assert_eq!(bracket.before.lifetime_matched, None);
    assert_eq!(bracket.before.visibility.visible, None);
    assert_eq!(bracket.before.minimized, Some(false));
    assert_eq!(bracket.after.minimized, None);
    assert_eq!(bracket.after.host_hidden, None);
}

#[test]
fn immutable_arrival_rows_decode_as_carried_facts() {
    // Native arm with a namespaced home decodes context-free (the engine
    // host gate below owns the capability check).
    let report = decode_request(&envelope(&nested_rows("report-arrival-native"))).expect("decodes");
    let bracket = &report.focus_feedback[0].arrival[0];
    match &bracket.location {
        ArrivalLocation::Native { home } => match home.as_ref().expect("home") {
            WorkspaceRef::Native { output, workspace } => {
                assert_eq!(output.0, "out-a");
                assert_eq!(workspace.as_str(), "native-a");
            }
            WorkspaceRef::Logical { .. } => panic!("native arm carries no logical home"),
        },
        ArrivalLocation::Output { .. } => panic!("native fixture rides the native arm"),
    }
    // Null output home is valid unreadable evidence, never a default.
    let report =
        decode_request(&envelope(&nested_rows("report-arrival-output-null"))).expect("decodes");
    match &report.focus_feedback[0].arrival[0].location {
        ArrivalLocation::Output { out } => assert_eq!(out, &None),
        ArrivalLocation::Native { .. } => panic!("null fixture rides the output arm"),
    }
    // Mismatched or wrong-context facts still decode: the core decides
    // readiness at use, the codec never verdicts here.
    for (name, check) in [
        ("report-arrival-wrong-output", "out-b"),
        ("report-arrival-wrong-frame", "out-a"),
        ("report-arrival-wrong-lifetime", "out-a"),
        ("report-arrival-wrong-watermark", "out-a"),
        ("report-arrival-wrong-retained-home", "out-a"),
    ] {
        let report = decode_request(&envelope(&nested_rows(name))).expect("decodes");
        match &report.focus_feedback[0].arrival[0].location {
            ArrivalLocation::Output { out } => {
                assert_eq!(out.as_ref().map(|id| id.0.as_str()), Some(check), "{name}")
            }
            ArrivalLocation::Native { .. } => panic!("{name} rides the output arm"),
        }
    }
    let frame = decode_request(&envelope(&nested_rows("report-arrival-wrong-frame")))
        .expect("decodes")
        .focus_feedback[0]
        .arrival[0]
        .frame
        .map(|rect| (rect.x, rect.y, rect.w, rect.h));
    assert_eq!(frame, Some((9, 8, 800, 600)));
    let after = decode_request(&envelope(&nested_rows("report-arrival-wrong-lifetime")))
        .expect("decodes")
        .focus_feedback[0]
        .arrival[0]
        .after
        .lifetime_matched;
    assert_eq!(after, Some(false));
    let after_obs = decode_request(&envelope(&nested_rows("report-arrival-wrong-watermark")))
        .expect("decodes")
        .focus_feedback[0]
        .arrival[0]
        .after
        .obs;
    assert_eq!(after_obs, 2);
}

#[test]
fn invalid_arrival_shapes_refuse_context_free() {
    // Eight shape violations refuse in the decoder, before any host
    // context applies: missing keys are malformed, mixed arms are
    // unknown fields, and bad kinds/values are unknown values.
    for (name, kind) in [
        ("invalid-arrival-missing-kind", "malformed"),
        ("invalid-arrival-unknown-kind", "unknown-value"),
        ("invalid-arrival-missing-output", "malformed"),
        ("invalid-arrival-mixed-arms", "unknown-field"),
        ("invalid-arrival-output-type", "unknown-value"),
        ("invalid-arrival-empty-output", "unknown-value"),
        ("invalid-arrival-missing-home", "malformed"),
        ("invalid-arrival-logical-home", "unknown-value"),
    ] {
        let error = decode_request(&envelope(&nested_rows(name))).expect_err("{name} refuses");
        assert_eq!(error.kind, kind, "{name}");
    }
}

#[test]
fn wrong_host_arms_decode_then_engine_rejects() {
    // Both mismatched arms decode context-free; the authoritative engine
    // host gate rejects per the manifest hostOwned context, without
    // mutating, while the matching host accepts the same rows.
    let output_on_native = nested_rows("invalid-arrival-output-on-native-host");
    decode_request(&envelope(&output_on_native)).expect("output arm decodes");
    let native_on_tiler = nested_rows("invalid-arrival-native-on-tiler-host");
    decode_request(&envelope(&native_on_tiler)).expect("native arm decodes");
    for (rows, host_owned) in [
        (output_on_native.clone(), true),
        (native_on_tiler.clone(), false),
    ] {
        let mut engine = connected_engine(host_owned);
        let revision = engine.state_revision();
        match engine.handle_report(&decode_request(&envelope(&rows)).expect("decodes")) {
            WorldOutcome::Rejected { kind, message } => {
                assert_eq!(kind, "snapshot-invalid");
                assert_eq!(message, "arrival-host-mismatch");
            }
            other => panic!("expected host mismatch, got {other:?}"),
        }
        assert_eq!(
            engine.state_revision(),
            revision,
            "host mismatch mutates nothing"
        );
    }
    // Cross-acceptance on the matching host: the same rows are facts there.
    for (rows, host_owned) in [(output_on_native, false), (native_on_tiler, true)] {
        let mut engine = connected_engine(host_owned);
        match engine.handle_report(&decode_request(&envelope(&rows)).expect("decodes")) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("expected accepted, got {other:?}"),
        }
    }
}

#[test]
fn closed_shape_refuses_unknowns_and_bad_types() {
    fn row() -> serde_json::Value {
        serde_json::json!({
            "id": "effect-a", "target": "win-a", "rev": "1",
            "setter": "accepted", "reason": "",
            "pre": "win-b", "post": "win-a", "foregroundProven": true,
            "arrival": [{
                "win": "win-a", "obs": "1",
                "before": {"obs": "1", "lifetimeMatched": true,
                    "visibility": {"visible": true, "suppressionOwned": false},
                    "minimized": false, "hostHidden": false},
                "kind": "output", "out": "out-a", "frame": [8, 8, 800, 600],
                "after": {"obs": "1", "lifetimeMatched": true,
                    "visibility": {"visible": true, "suppressionOwned": false},
                    "minimized": false, "hostHidden": false},
            }],
            "attemptMs": "20",
        })
    }
    // Unknown row/bracket keys refuse as unknown-field; unknown closed
    // tokens refuse as unknown-value.
    let mut extra = row();
    extra["extra"] = serde_json::json!(1);
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([extra])))
            .expect_err("unknown key refuses")
            .kind,
        "unknown-field"
    );
    let mut bad_setter = row();
    bad_setter["setter"] = serde_json::json!("retried");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_setter])))
            .expect_err("unknown setter refuses")
            .kind,
        "unknown-value"
    );
    let mut bad_reason = row();
    bad_reason["reason"] = serde_json::json!("bogus-reason");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_reason])))
            .expect_err("unknown reason refuses")
            .kind,
        "unknown-value"
    );
    let mut bad_kind = row();
    bad_kind["arrival"][0]["kind"] = serde_json::json!("logical");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_kind])))
            .expect_err("bad arm refuses")
            .kind,
        "unknown-value"
    );
    // Quoted decimal stamps only: numbers, signs, blanks refuse.
    for (key, value, kind) in [
        ("rev", serde_json::json!("020"), "unknown-value"),
        ("rev", serde_json::json!(""), "unknown-value"),
        (
            "rev",
            serde_json::json!("18446744073709551616"),
            "unknown-value",
        ),
        ("attemptMs", serde_json::json!("020"), "unknown-value"),
        ("rev", serde_json::json!(20), "malformed"),
        ("attemptMs", serde_json::json!(20), "malformed"),
    ] {
        let mut bad = row();
        bad[key] = value;
        let error =
            decode_request(&envelope(&serde_json::json!([bad]))).expect_err("stamp refuses");
        assert_eq!(error.kind, kind);
    }
    // Wrong JSON types refuse: foreground endpoints stay nullable window
    // ids (never empty, never bools), the proof stays a bool, and the
    // brackets stay an array of objects.
    for (mutate, kind) in [
        ("empty-pre", "unknown-value"),
        ("empty-post", "unknown-value"),
        ("bool-pre", "malformed"),
        ("string-proof", "malformed"),
        ("object-arrival", "malformed"),
        ("number-bracket", "malformed"),
        ("empty-id", "unknown-value"),
        ("bad-target", "unknown-value"),
    ] {
        let mut bad = row();
        match mutate {
            "empty-pre" => bad["pre"] = serde_json::json!(""),
            "empty-post" => bad["post"] = serde_json::json!(""),
            "bool-pre" => bad["pre"] = serde_json::json!(false),
            "string-proof" => bad["foregroundProven"] = serde_json::json!("true"),
            "object-arrival" => bad["arrival"] = serde_json::json!({}),
            "number-bracket" => bad["arrival"] = serde_json::json!([7]),
            "empty-id" => bad["id"] = serde_json::json!(""),
            "bad-target" => bad["target"] = serde_json::json!("not an id!!"),
            _ => unreachable!(),
        }
        let error = decode_request(&envelope(&serde_json::json!([bad]))).expect_err("type refuses");
        assert_eq!(error.kind, kind, "{mutate}");
    }
    // Dropped required keys refuse.
    for key in ["pre", "arrival", "foregroundProven"] {
        let mut bad = row();
        bad.as_object_mut().expect("object").remove(key);
        let error =
            decode_request(&envelope(&serde_json::json!([bad]))).expect_err("missing refuses");
        assert_eq!(error.kind, "malformed", "{key}");
    }
    // Bad nested facts refuse: bracket window ids, decimal watermarks, and
    // carried frames keep their gates; unknown sample facts stay facts
    // (proven above) rather than refusing here.
    let mut bad_win = row();
    bad_win["arrival"][0]["win"] = serde_json::json!("not an id!!");
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_win])))
            .expect_err("bad bracket window refuses")
            .kind,
        "unknown-value"
    );
    // Native home with bad opaque grammar refuses with the shared
    // workspace-ref helper kind (malformed here, not unknown-value): the
    // helper serves other lanes too, so this pins rather than changes it.
    let mut bad_home_id = row();
    bad_home_id["arrival"][0]["kind"] = serde_json::json!("native");
    bad_home_id["arrival"][0]
        .as_object_mut()
        .expect("object")
        .remove("out");
    bad_home_id["arrival"][0]["home"] =
        serde_json::json!({"kind": "native", "out": "not an id!!", "ws": "ws-1"});
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_home_id])))
            .expect_err("bad home id refuses")
            .kind,
        "malformed"
    );
    let mut bad_frame = row();
    bad_frame["arrival"][0]["frame"] = serde_json::json!([0, 0, 0, 0]);
    assert_eq!(
        decode_request(&envelope(&serde_json::json!([bad_frame])))
            .expect_err("degenerate frame refuses")
            .kind,
        "unknown-value"
    );
}

#[test]
fn wire_level_duplicates_and_trailing_refuse() {
    let rows = nested_rows("report-focus-feedback-proven");
    let wire = envelope(&rows);
    decode_request(&wire).expect("valid staged request decodes");
    // Literal duplicate row and bracket keys refuse (a Value build would
    // silently keep the last duplicate, so the witness is literal wire).
    for nested in [
        wire.replacen(
            "\"id\":\"effect-a\"",
            "\"id\":\"effect-a\",\"id\":\"effect-a\"",
            1,
        ),
        wire.replacen(
            "\"kind\":\"output\"",
            "\"kind\":\"output\",\"kind\":\"output\"",
            1,
        ),
        // Escaped-equivalent duplicate row key (`\u0069` is `i`).
        wire.replacen(
            "\"id\":\"effect-a\"",
            "\"id\":\"effect-a\",\"\\u0069d\":\"effect-a\"",
            1,
        ),
    ] {
        assert_ne!(nested, wire);
        let error = decode_request(&nested).expect_err("duplicate row key refuses");
        assert_eq!(error.kind, "malformed");
    }
    // Trailing tokens after an otherwise-valid request refuse.
    for trailing in [format!("{wire} {{}}"), format!("{wire}x")] {
        let error = decode_request(&trailing).expect_err("trailing refuses");
        assert_eq!(error.kind, "malformed");
    }
    // Harmless control: a single escaped key spelling still decodes.
    let single = wire.replacen("\"foregroundProven\"", "\"\\u0066oregroundProven\"", 1);
    assert_ne!(single, wire);
    decode_request(&single).expect("single escaped key decodes");
}

#[test]
fn cap_1024_accepts_and_1025_refuses() {
    fn rows(count: usize) -> serde_json::Value {
        serde_json::Value::Array(
            (0..count)
                .map(|n| {
                    serde_json::json!({
                        "id": format!("fb-{n:04}"), "target": "win-a", "rev": "7",
                        "setter": "accepted", "reason": "",
                        "pre": null, "post": null, "foregroundProven": true,
                        "arrival": [], "attemptMs": "9",
                    })
                })
                .collect(),
        )
    }
    let report = decode_request(&envelope(&rows(1024))).expect("cap accepts");
    assert_eq!(report.focus_feedback.len(), 1024);
    let error = decode_request(&envelope(&rows(1025))).expect_err("cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

#[test]
fn arrival_total_cap_refuses_over_10000() {
    fn bracket(n: usize) -> serde_json::Value {
        serde_json::json!({
            "win": format!("w{n:05}"), "obs": "1",
            "before": {"obs": "1", "lifetimeMatched": null,
                "visibility": {"visible": null, "suppressionOwned": null},
                "minimized": null, "hostHidden": null},
            "kind": "output", "out": null, "frame": null,
            "after": {"obs": "1", "lifetimeMatched": null,
                "visibility": {"visible": null, "suppressionOwned": null},
                "minimized": null, "hostHidden": null},
        })
    }
    fn envelope_with_brackets(count: usize) -> String {
        let brackets: Vec<serde_json::Value> = (0..count).map(bracket).collect();
        envelope(&serde_json::json!([{
            "id": "effect-a", "target": "win-a", "rev": "7",
            "setter": "accepted", "reason": "",
            "pre": null, "post": null, "foregroundProven": true,
            "arrival": brackets, "attemptMs": "9",
        }]))
    }
    let report = decode_request(&envelope_with_brackets(10_000)).expect("total accepts");
    assert_eq!(report.focus_feedback[0].arrival.len(), 10_000);
    let error = decode_request(&envelope_with_brackets(10_001)).expect_err("total refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
    // The total spans rows: two rows of 5000 brackets accept, while
    // 5001 + 5000 refuses.
    fn envelope_with_split(first: usize, second: usize) -> String {
        let rows: Vec<serde_json::Value> = [first, second]
            .into_iter()
            .enumerate()
            .map(|(r, count)| {
                let brackets: Vec<serde_json::Value> = (0..count).map(bracket).collect();
                serde_json::json!({
                    "id": format!("effect-{r}"), "target": "win-a", "rev": "7",
                    "setter": "accepted", "reason": "",
                    "pre": null, "post": null, "foregroundProven": true,
                    "arrival": brackets, "attemptMs": "9",
                })
            })
            .collect();
        envelope(&serde_json::Value::Array(rows))
    }
    let split = decode_request(&envelope_with_split(5_000, 5_000)).expect("split accepts");
    assert_eq!(split.focus_feedback.len(), 2);
    assert_eq!(split.focus_feedback[0].arrival.len(), 5_000);
    assert_eq!(split.focus_feedback[1].arrival.len(), 5_000);
    let error = decode_request(&envelope_with_split(5_001, 5_000)).expect_err("split refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

#[test]
fn digest_separates_every_raw_field() {
    let report =
        decode_request(&envelope(&nested_rows("report-focus-feedback-proven"))).expect("decodes");
    assert_eq!(report.focus_feedback.len(), 1);
    let baseline = report_digest_v4(&report);
    // Identical bytes redeliver the identical digest.
    assert_eq!(report_digest_v4(&report.clone()), baseline);
    // Every raw field, presence bit, arm, and bracket order separates the
    // digest: a changed row never aliases the accepted one.
    let mut cases: Vec<(&str, WorldReportV4)> = Vec::new();
    let mut id = report.clone();
    id.focus_feedback[0].id = "effect-b".to_owned();
    cases.push(("id", id));
    let mut target = report.clone();
    target.focus_feedback[0].target = tiler_core::directional::WindowId("win-b".to_owned());
    cases.push(("target", target));
    let mut rev = report.clone();
    rev.focus_feedback[0].revision = 2;
    cases.push(("rev", rev));
    let mut setter = report.clone();
    setter.focus_feedback[0].setter = VisibilitySetter::Refused;
    cases.push(("setter", setter));
    let mut reason = report.clone();
    reason.focus_feedback[0].reason = VisibilityReason::NativeRefused;
    cases.push(("reason", reason));
    let mut pre = report.clone();
    pre.focus_feedback[0].pre = None;
    cases.push(("pre-presence", pre));
    let mut post = report.clone();
    post.focus_feedback[0].post = None;
    cases.push(("post-presence", post));
    let mut proven = report.clone();
    proven.focus_feedback[0].foreground_proven = false;
    cases.push(("foregroundProven", proven));
    let mut stamp = report.clone();
    stamp.focus_feedback[0].attempt_ms = 21;
    cases.push(("attemptMs", stamp));
    let mut before = report.clone();
    before.focus_feedback[0].arrival[0].before.obs = 2;
    cases.push(("before-obs", before));
    let mut matched = report.clone();
    matched.focus_feedback[0].arrival[0].before.lifetime_matched = Some(false);
    cases.push(("before-lifetime", matched));
    let mut visible = report.clone();
    visible.focus_feedback[0].arrival[0]
        .after
        .visibility
        .visible = Some(false);
    cases.push(("after-visible", visible));
    let mut arm = report.clone();
    arm.focus_feedback[0].arrival[0].location = ArrivalLocation::Native { home: None };
    cases.push(("arm", arm));
    let mut home = report.clone();
    home.focus_feedback[0].arrival[0].location = ArrivalLocation::Native {
        home: Some(WorkspaceRef::Native {
            output: tiler_core::directional::OutputId("out-a".to_owned()),
            workspace: tiler_core::ids::NativeWorkspaceId::parse("native-a")
                .expect("native workspace"),
        }),
    };
    cases.push(("home", home));
    let mut out = report.clone();
    out.focus_feedback[0].arrival[0].location = ArrivalLocation::Output { out: None };
    cases.push(("out-presence", out));
    let mut frame = report.clone();
    frame.focus_feedback[0].arrival[0].frame = None;
    cases.push(("frame-presence", frame));
    let mut win = report.clone();
    win.focus_feedback[0].arrival[0].window = tiler_core::directional::WindowId("win-b".to_owned());
    cases.push(("arrival-win", win));
    let mut ordered = report.clone();
    let mut second = ordered.focus_feedback[0].arrival[0].clone();
    second.window = tiler_core::directional::WindowId("win-b".to_owned());
    ordered.focus_feedback[0].arrival.push(second);
    let mut swapped = ordered.clone();
    swapped.focus_feedback[0].arrival.swap(0, 1);
    // Bracket order itself separates the digest: two distinct brackets
    // swapped digest differently (compared directly, not to baseline).
    assert_ne!(
        report_digest_v4(&ordered),
        report_digest_v4(&swapped),
        "arrival order separates"
    );
    cases.push(("arrival-second-bracket", swapped));
    // Row order itself separates the digest: two distinct rows swapped
    // digest differently.
    let mut two_rows = report.clone();
    let mut second_row = two_rows.focus_feedback[0].clone();
    second_row.id = "effect-b".to_owned();
    two_rows.focus_feedback.push(second_row);
    let mut swapped_rows = two_rows.clone();
    swapped_rows.focus_feedback.swap(0, 1);
    assert_ne!(
        report_digest_v4(&two_rows),
        report_digest_v4(&swapped_rows),
        "row order separates"
    );
    assert_eq!(cases.len(), 18);
    for (case, mutated) in cases {
        assert_ne!(report_digest_v4(&mutated), baseline, "{case} separates");
    }
}

#[test]
fn estimate_covers_focus_rows_exactly_at_the_margin() {
    for name in [
        "report-focus-feedback-proven",
        "report-focus-feedback-unreadable",
    ] {
        let rows = nested_rows(name);
        let empty = envelope(&serde_json::json!([]));
        let full = envelope(&rows);
        let empty_report = decode_request(&empty).expect("empty decodes");
        let full_report = decode_request(&full).expect("rows decode");
        assert_eq!(full_report.focus_feedback.len(), 1, "{name}");
        let empty_estimate = report_estimate_v4(&empty_report);
        let full_estimate = report_estimate_v4(&full_report);
        assert!(
            empty_estimate >= empty.len(),
            "estimate {empty_estimate} bounds empty wire {}",
            empty.len()
        );
        assert!(
            full_estimate >= full.len(),
            "estimate {full_estimate} bounds row wire {} for {name}",
            full.len()
        );
        // A single row's marginal estimate equals its marginal wire bytes:
        // the lane carries no per-row slack beyond the shared brackets.
        assert_eq!(
            full_estimate - empty_estimate,
            full.len() - empty.len(),
            "{name} row costs its exact wire bytes"
        );
    }
}

#[test]
fn compact_automatic_ids_decode_and_overlong_spellings_refuse() {
    // A compact producer identity at the widest correlation-valid shape
    // (token and revision at u64::MAX, 128-byte subject) decodes, while
    // the retired window-spelling identities overrun the opaque bound and
    // refuse as unknown values.
    fn row(id: &str, target: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "target": target, "rev": "18446744073709551615",
            "setter": "accepted", "reason": "",
            "pre": "win-a", "post": target, "foregroundProven": true,
            "arrival": [], "attemptMs": "20",
        })
    }
    let target = "w".repeat(128);
    let compact = decode_request(&envelope(&serde_json::json!([row(
        &format!("af-{}", u64::MAX),
        &target
    ),])))
    .expect("compact admission feedback decodes");
    assert_eq!(compact.focus_feedback.len(), 1);
    assert_eq!(compact.focus_feedback[0].id, format!("af-{}", u64::MAX));
    assert_eq!(compact.focus_feedback[0].target.0, target);
    assert_eq!(compact.focus_feedback[0].revision, u64::MAX);
    let hover = decode_request(&envelope(&serde_json::json!([row(
        &format!("ffc-{}", u64::MAX),
        &target
    ),])))
    .expect("compact hover feedback decodes");
    assert_eq!(hover.focus_feedback[0].id, format!("ffc-{}", u64::MAX));
    for old in [format!("af-{target}"), format!("ffc-{target}")] {
        assert!(
            old.len() > 128,
            "retired spelling overruns the bound: {}",
            old.len()
        );
        assert_eq!(
            decode_request(&envelope(&serde_json::json!([row(&old, &target)])))
                .expect_err("overlong identity refuses")
                .kind,
            "unknown-value",
            "retired spelling refuses: {old}"
        );
    }
}
