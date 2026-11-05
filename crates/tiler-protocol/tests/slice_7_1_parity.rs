use tiler_core::connection::HostCapabilities;
use tiler_core::directional::OutputId;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::RowDiagnosticKind;
use tiler_core::world_report_v4_checks::{grade_invalid_screens_v4, grade_invalid_windows_v4};
use tiler_protocol::world_protocol::decode_report_v4;

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-fixtures/world-wire/v4/messages")
            .join(name),
    )
    .expect("fixture reads")
}

fn bound_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "generation-a",
        "adapter-a",
        HostCapabilities {
            maximize: true,
            fullscreen: true,
            native_sticky: true,
            above: true,
            ..HostCapabilities::default()
        },
    )
    .expect("generation")
}

#[test]
fn slice_7_1_screen_typed_matches_decoded() {
    let valid_text = fixture_text("report-observe.json");
    let valid = decode_report_v4(&valid_text).expect("valid decodes");
    let mut typed_invalid = valid.report.clone();
    typed_invalid.screens[0].scale = 0.0;
    let graded = grade_invalid_screens_v4(&typed_invalid).expect("typed grades");
    assert_eq!(graded.held_screens.len(), 1);

    let mut value: serde_json::Value = serde_json::from_str(&valid_text).expect("fixture parses");
    value["screens"][0]["scale"] = serde_json::json!(0);
    let decoded = decode_report_v4(&value.to_string()).expect("decoded holds");
    assert_eq!(decoded.report.held_screens.len(), 1);
    assert_eq!(
        graded.held_screens[0].fields, decoded.report.held_screens[0].fields,
        "typed and decoded held screen fields match"
    );
    assert_eq!(
        graded.held_screens[0].diagnostic,
        decoded.report.held_screens[0].diagnostic
    );
    assert_eq!(
        graded.held_screens[0].diagnostic.kind,
        RowDiagnosticKind::InvalidFact
    );
}

#[test]
fn slice_7_1_window_typed_matches_decoded() {
    let valid_text = fixture_text("report-observe.json");
    let valid = decode_report_v4(&valid_text).expect("valid decodes");
    let overbound = "x".repeat(5000);
    let mut typed_invalid = valid.report.clone();
    typed_invalid.windows[0].title = Some(overbound.clone());
    let graded = grade_invalid_windows_v4(&typed_invalid).expect("typed grades");
    assert_eq!(graded.held_windows.len(), 1);

    let mut value: serde_json::Value = serde_json::from_str(&valid_text).expect("fixture parses");
    value["windows"][0]["title"] = serde_json::json!(overbound);
    let decoded = decode_report_v4(&value.to_string()).expect("decoded holds");
    assert_eq!(decoded.report.held_windows.len(), 1);
    assert_eq!(
        graded.held_windows[0].fields, decoded.report.held_windows[0].fields,
        "typed and decoded held window fields match"
    );
    assert_eq!(
        graded.held_windows[0].diagnostic,
        decoded.report.held_windows[0].diagnostic
    );
    assert_eq!(graded.held_windows[0].kind, RowDiagnosticKind::InvalidFact);
}

#[test]
fn slice_7_1_bootstrap_typed_matches_decoded() {
    // Invalid-only first observation: the typed invalid report and the
    // wire-decoded invalid report bootstrap identically (no readiness,
    // no authority), then a later valid observation readies both
    // identically. Separate identically bound engines; same request
    // identities, so the full replies must match exactly.
    let valid_text = fixture_text("report-observe.json");
    let valid = decode_report_v4(&valid_text).expect("valid decodes");
    let mut typed_invalid = valid.report.clone();
    typed_invalid.screens[0].scale = 0.0;
    let mut invalid_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    invalid_value["screens"][0]["scale"] = serde_json::json!(0);
    let decoded_invalid =
        decode_report_v4(&invalid_value.to_string()).expect("decoded holds invalid");
    assert_eq!(decoded_invalid.report.held_screens.len(), 1);
    let mut typed_engine = bound_engine();
    let mut decoded_engine = bound_engine();
    let typed_reply = typed_engine
        .handle_report_v4_for_test(&typed_invalid)
        .expect("typed bootstraps");
    let decoded_reply = decoded_engine
        .handle_report_v4_for_test(&decoded_invalid.report)
        .expect("decoded bootstraps");
    assert!(
        !typed_reply.ready && !decoded_reply.ready,
        "invalid-only first observation stays bootstrap"
    );
    assert_eq!(typed_reply, decoded_reply, "bootstrap replies match");
    // Valid observation readies both engines identically.
    let mut typed_valid = valid.report.clone();
    typed_valid.observation_seq += 1;
    typed_valid.request_seq += 1;
    typed_valid.request_id = "request-b".to_owned();
    let mut valid_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    valid_value["obs"] = serde_json::json!("2");
    valid_value["reqSeq"] = serde_json::json!("2");
    valid_value["req"] = serde_json::json!("request-b");
    let decoded_valid = decode_report_v4(&valid_value.to_string()).expect("bumped valid decodes");
    let typed_ready = typed_engine
        .handle_report_v4_for_test(&typed_valid)
        .expect("typed readies");
    let decoded_ready = decoded_engine
        .handle_report_v4_for_test(&decoded_valid.report)
        .expect("decoded readies");
    assert!(typed_ready.ready && decoded_ready.ready);
    assert_eq!(typed_ready, decoded_ready, "ready replies match");
}

#[test]
fn slice_7_1_ready_hold_typed_matches_decoded() {
    // Valid fixture baseline establishes readiness and binding
    // legitimately; the invalid second observation then holds narrowly
    // on both paths with identical full replies.
    let valid_text = fixture_text("report-observe.json");
    let valid = decode_report_v4(&valid_text).expect("valid decodes");
    let mut typed_engine = bound_engine();
    let mut decoded_engine = bound_engine();
    typed_engine
        .handle_report_v4_for_test(&valid.report)
        .expect("typed baseline readies");
    decoded_engine
        .handle_report_v4_for_test(&valid.report)
        .expect("decoded baseline readies");
    let mut typed_invalid = valid.report.clone();
    typed_invalid.screens[0].scale = 0.0;
    typed_invalid.observation_seq += 1;
    typed_invalid.request_seq += 1;
    typed_invalid.request_id = "request-b".to_owned();
    let mut invalid_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    invalid_value["screens"][0]["scale"] = serde_json::json!(0);
    invalid_value["obs"] = serde_json::json!("2");
    invalid_value["reqSeq"] = serde_json::json!("2");
    invalid_value["req"] = serde_json::json!("request-b");
    let decoded_invalid =
        decode_report_v4(&invalid_value.to_string()).expect("decoded holds invalid");
    let typed_reply = typed_engine
        .handle_report_v4_for_test(&typed_invalid)
        .expect("typed holds");
    let decoded_reply = decoded_engine
        .handle_report_v4_for_test(&decoded_invalid.report)
        .expect("decoded holds");
    assert!(typed_reply.ready && decoded_reply.ready);
    assert_eq!(typed_reply, decoded_reply, "ready hold replies match");

    let mut typed_window = valid.report.clone();
    typed_window.windows[0].title = Some("x".repeat(5000));
    typed_window.observation_seq += 2;
    typed_window.request_seq += 2;
    typed_window.request_id = "request-c".to_owned();
    let mut window_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    window_value["windows"][0]["title"] = serde_json::json!("x".repeat(5000));
    window_value["obs"] = serde_json::json!("3");
    window_value["reqSeq"] = serde_json::json!("3");
    window_value["req"] = serde_json::json!("request-c");
    let decoded_window = decode_report_v4(&window_value.to_string()).expect("decoded holds window");
    let typed_reply = typed_engine
        .handle_report_v4_for_test(&typed_window)
        .expect("typed window holds");
    let decoded_reply = decoded_engine
        .handle_report_v4_for_test(&decoded_window.report)
        .expect("decoded window holds");
    assert_eq!(
        typed_reply, decoded_reply,
        "ready window hold replies match"
    );
    assert_eq!(
        typed_engine.handle_report_v4_for_test(&typed_window),
        Ok(typed_reply),
        "raw typed invalid report replays its own cached reply"
    );
    assert_eq!(
        decoded_engine.handle_report_v4_for_test(&decoded_window.report),
        Ok(decoded_reply),
        "decoded invalid report replays its own cached reply"
    );
}

#[test]
fn slice_7_1_invalid_lanes_typed_match_decoded() {
    let valid_text = fixture_text("report-observe.json");
    let valid = decode_report_v4(&valid_text).expect("valid decodes");
    // Screen identity overbound: typed vs decoded held rows match.
    let overbound = "m".repeat(300);
    let mut typed_identity = valid.report.clone();
    typed_identity.screens[0].identity.manufacturer = Some(overbound.clone());
    let mut identity_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    identity_value["screens"][0]["identity"]["manufacturer"] = serde_json::json!(overbound);
    let decoded_identity =
        decode_report_v4(&identity_value.to_string()).expect("decoded holds identity");
    let typed_graded = grade_invalid_screens_v4(&typed_identity).expect("typed grades");
    assert_eq!(typed_graded.held_screens.len(), 1);
    assert_eq!(decoded_identity.report.held_screens.len(), 1);
    assert_eq!(
        typed_graded.held_screens[0].fields, decoded_identity.report.held_screens[0].fields,
        "identity held fields match"
    );
    assert_eq!(
        typed_graded.held_screens[0].diagnostic,
        decoded_identity.report.held_screens[0].diagnostic
    );
    // Window process-id invalid: typed vs decoded held rows match.
    let mut typed_window = valid.report.clone();
    typed_window.windows[0].process_id = Some("bad pid".to_owned());
    let mut window_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    window_value["windows"][0]["processId"] = serde_json::json!("bad pid");
    let decoded_window = decode_report_v4(&window_value.to_string()).expect("decoded holds window");
    let typed_window_graded = grade_invalid_windows_v4(&typed_window).expect("typed grades");
    assert_eq!(typed_window_graded.held_windows.len(), 1);
    assert_eq!(decoded_window.report.held_windows.len(), 1);
    assert_eq!(
        typed_window_graded.held_windows[0].fields, decoded_window.report.held_windows[0].fields,
        "process-id held fields match"
    );
    assert_eq!(
        typed_window_graded.held_windows[0].diagnostic,
        decoded_window.report.held_windows[0].diagnostic
    );
    // A cursor hit naming the held window refuses on both paths with
    // the identical verdict: the gradable row is not a readable
    // reported window, so the typed validator fails closed exactly
    // like the wire decoder, before any grading.
    let mut typed_hit = typed_window.clone();
    typed_hit.cursor = Some((10, 20));
    typed_hit.cursor_hit = Some(typed_hit.windows[0].window.clone());
    assert!(
        grade_invalid_windows_v4(&typed_hit).is_none(),
        "typed hit on gradable row grades nothing"
    );
    let mut hit_engine = bound_engine();
    let typed_hit_err = hit_engine
        .handle_report_v4_for_test(&typed_hit)
        .expect_err("typed refuses held hit");
    let mut hit_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    hit_value["windows"][0]["processId"] = serde_json::json!("bad pid");
    hit_value["cursor"] = serde_json::json!([10, 20]);
    hit_value["cursorHit"] = hit_value["windows"][0]["win"].clone();
    let hit_err = decode_report_v4(&hit_value.to_string()).expect_err("wire refuses held hit");
    assert_eq!(typed_hit_err.0, hit_err.kind);
    assert_eq!(typed_hit_err.1, hit_err.message);
    assert_eq!(hit_err.kind, "snapshot-invalid");
    assert_eq!(hit_err.message, "cursor-hit-unknown");
    // Ordinal preservation: a second bad-scale screen holds at ordinal 1.
    let mut typed_two = valid.report.clone();
    let mut second = typed_two.screens[0].clone();
    second.output = OutputId("out-b".to_owned());
    second.scale = 0.0;
    typed_two.screens.push(second);
    let mut two_value: serde_json::Value =
        serde_json::from_str(&valid_text).expect("fixture parses");
    let mut second_value = two_value["screens"][0].clone();
    second_value["out"] = serde_json::json!("out-b");
    second_value["scale"] = serde_json::json!(0);
    two_value
        .get_mut("screens")
        .and_then(serde_json::Value::as_array_mut)
        .expect("screens array")
        .push(second_value);
    let decoded_two = decode_report_v4(&two_value.to_string()).expect("decoded holds second");
    let typed_two_graded = grade_invalid_screens_v4(&typed_two).expect("typed grades second");
    assert_eq!(typed_two_graded.held_screens.len(), 1);
    assert_eq!(typed_two_graded.held_screens[0].ordinal, 1);
    assert_eq!(decoded_two.report.held_screens.len(), 1);
    assert_eq!(
        typed_two_graded.held_screens, decoded_two.report.held_screens,
        "ordinal-held rows match"
    );
    let mut typed_preheld = decoded_two.report.clone();
    typed_preheld.screens[0].scale = 0.0;
    let typed_both = grade_invalid_screens_v4(&typed_preheld).expect("grades beside existing hold");
    two_value["screens"][0]["scale"] = serde_json::json!(0);
    let decoded_both =
        decode_report_v4(&two_value.to_string()).expect("decoded holds both screens");
    assert_eq!(
        typed_both.held_screens, decoded_both.report.held_screens,
        "existing ordinal one stays fixed while the newly invalid row takes ordinal zero"
    );
}
