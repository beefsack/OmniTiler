//! Protocol-only optional wire metadata seam: [`decode_request_with_wire`]
//! records the tolerant spelling of every report-level optional group while
//! the normalized values stay in [`WorldReportV4`].
//!
//! Whole-fixture note: every `test-fixtures/world-wire/v4/messages/*.json`
//! file below is a genuine v4 envelope (`"v":4` plus `rendererInput` and the
//! v4 presentation shape) decoded through the genuine v4 ingress. Isolated
//! optional fragments (single key values, single lane rows) are extracted
//! from them and placed in the minimal v4 envelope below. Each test names
//! which inputs are fragments.
//!
//! Offline only: no adapters, no live work.

use tiler_core::connection::{
    ApplicationIdentity, HandshakeRequest, HostCapabilities, SettingsRequest,
};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    AttemptLaneV4, ClearAttempt, FrameAttempt, MemberAttempt, NativeCreationAttempt,
    NativeWorkspaceFact, ViewAttempt, WorkspaceRef, WorldReportV4,
};
use tiler_core::world_report_v4_checks::report_digest_v4 as report_digest;
use tiler_core::world_sizing_v4::reply_exact_bytes_v4;
use tiler_protocol::world_protocol::{
    CursorHitWire, DecodedReplyV4, DecodedReportV4, GestureIngressWire, ReplyWireOptionals,
    ReportWireOptionals, WorldOutcome, decode_reply_v4, decode_request, decode_request_with_wire,
    encode_reply_v4,
};

fn fixture_text(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    std::fs::read_to_string(&path).expect("fixture JSON")
}

fn fixture_value(name: &str) -> serde_json::Value {
    serde_json::from_str(&fixture_text(name)).expect("fixture JSON")
}

/// Minimal observation-only v4 envelope: the immutable
/// `report-optionals-omitted` fixture (valid screens, windows,
/// presentation, and the required `rendererInput` lane) rebound to the
/// connected test engine, with every report-level optional group stripped
/// to its absent spelling. Tests inject the spellings under test.
fn base_value() -> serde_json::Value {
    let mut base = fixture_value("report-optionals-omitted");
    base["adapter"] = serde_json::json!("adapter-1");
    base["gen"] = serde_json::json!("gen-1");
    base["req"] = serde_json::json!("req-1");
    if let Some(map) = base.as_object_mut() {
        for key in [
            "cursorHit",
            "monotonicMs",
            "primary",
            "frameAttempts",
            "memberAttempts",
            "viewAttempts",
            "clearAttempts",
            "creationAttempts",
            "gestureIngress",
        ] {
            map.remove(key);
        }
    }
    base
}

fn one_window_request() -> String {
    base_value().to_string()
}

fn connected_engine(generation: &str, caps: HostCapabilities) -> WorldEngine {
    let mut engine = WorldEngine::new(generation).expect("generation");
    let handshake = engine.handshake(&HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "handshake-1".to_owned(),
        application_identity: ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
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
fn reply_wire(reply: &tiler_core::world_report::WorldReplyV4) -> String {
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

/// Decode through both entry points; the wired report must equal the
/// plain report byte-for-byte in typed terms.
fn decode_both(json: &str) -> DecodedReportV4 {
    let plain = decode_request(json).expect("v4 decodes");
    let wired = decode_request_with_wire(json).expect("wired decodes");
    assert_eq!(plain, wired.report, "wrapper report matches plain decoder");
    wired
}

/// Both entry points must hold the same identifiable attempt row: the
/// genuine v4 attempt lanes keep unknown fields and identifiable value
/// failures in the narrow held lane (never a silent drop, never an
/// envelope refusal) with exact lane/ordinal/identity coverage.
fn assert_same_hold(json: &str, lane: AttemptLaneV4, identity: &[&str]) {
    let plain = decode_request(json).expect("held decodes");
    let wired = decode_request_with_wire(json).expect("held decodes");
    assert_eq!(plain, wired.report, "both decode APIs hold identically");
    assert_eq!(plain.held_attempts.len(), 1, "one row held");
    let held = &plain.held_attempts[0];
    assert_eq!(held.lane, lane);
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.identity, identity);
}

/// Both entry points must reject with the identical fixed kind/message.
fn assert_same_rejection(json: &str) -> (&'static str, &'static str) {
    let plain = decode_request(json).expect_err("plain rejects");
    let wired = decode_request_with_wire(json).expect_err("wired rejects");
    assert_eq!(
        (plain.kind, plain.message),
        (wired.kind, wired.message),
        "both decode APIs reject identically"
    );
    (plain.kind, plain.message)
}

#[test]
fn cursor_hit_preserves_all_four_spellings() {
    // Fragment sources (isolated `cursorHit` values only, never whole
    // fixtures): report-optionals-omitted (key absent),
    // report-optionals-null (`null`), report-optionals-empty (`""`),
    // report-optionals-id (`"win-a"`).
    let omitted: serde_json::Value = fixture_value("report-optionals-omitted");
    assert!(
        omitted.get("cursorHit").is_none(),
        "omitted fixture carries no cursorHit key"
    );
    // Absent backfills to no hit.
    let decoded = decode_both(&one_window_request());
    assert_eq!(decoded.wire.cursor_hit, CursorHitWire::Absent);
    assert_eq!(decoded.report.cursor_hit, None);

    // Explicit null backfills the same way with distinct metadata.
    let mut null_value = base_value();
    null_value["cursor"] = serde_json::json!([100, 200]);
    null_value["cursorHit"] = serde_json::Value::Null;
    let decoded = decode_both(&null_value.to_string());
    assert_eq!(decoded.wire.cursor_hit, CursorHitWire::Null);
    assert_eq!(decoded.report.cursor_hit, None);

    // Explicit empty backfills the same way with distinct metadata.
    let mut empty_value = base_value();
    empty_value["cursor"] = serde_json::json!([100, 200]);
    empty_value["cursorHit"] = serde_json::Value::String(String::new());
    let decoded = decode_both(&empty_value.to_string());
    assert_eq!(decoded.wire.cursor_hit, CursorHitWire::Empty);
    assert_eq!(decoded.report.cursor_hit, None);

    // A carried id decodes to the named lifetime.
    let mut id_value = base_value();
    id_value["cursor"] = serde_json::json!([100, 200]);
    id_value["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let decoded = decode_both(&id_value.to_string());
    assert_eq!(decoded.wire.cursor_hit, CursorHitWire::Id);
    assert_eq!(
        decoded.report.cursor_hit,
        Some(WindowId("win-a".to_owned()))
    );

    // Equal normalized spellings share typed reports and digests while
    // metadata differs. The cursor position is held constant so only the
    // hit spelling varies.
    let mut absent_cursor = base_value();
    absent_cursor["cursor"] = serde_json::json!([100, 200]);
    let absent_report = decode_request(&absent_cursor.to_string()).expect("decodes");
    let null_report = decode_request(&null_value.to_string()).expect("decodes");
    let empty_report = decode_request(&empty_value.to_string()).expect("decodes");
    assert_eq!(absent_report, null_report);
    assert_eq!(absent_report, empty_report);
    assert_eq!(report_digest(&absent_report), report_digest(&null_report));
    assert_eq!(report_digest(&absent_report), report_digest(&empty_report));
    let absent_wire = decode_request_with_wire(&absent_cursor.to_string())
        .expect("decodes")
        .wire;
    let null_wire = decode_request_with_wire(&null_value.to_string())
        .expect("decodes")
        .wire;
    let empty_wire = decode_request_with_wire(&empty_value.to_string())
        .expect("decodes")
        .wire;
    assert_ne!(absent_wire.cursor_hit, null_wire.cursor_hit);
    assert_ne!(absent_wire.cursor_hit, empty_wire.cursor_hit);
}

#[test]
fn monotonic_clock_absent_zero_and_max() {
    // Fragment sources (isolated `monotonicMs` values only):
    // report-optionals-omitted (`"0"`) and report-optionals-id (u64::MAX).
    let omitted: serde_json::Value = fixture_value("report-optionals-omitted");
    assert_eq!(omitted["monotonicMs"], serde_json::json!("0"));
    let id_fixture: serde_json::Value = fixture_value("report-optionals-id");
    assert_eq!(
        id_fixture["monotonicMs"],
        serde_json::json!("18446744073709551615")
    );

    let decoded = decode_both(&one_window_request());
    assert!(!decoded.wire.monotonic_ms_present);
    assert_eq!(decoded.report.monotonic_ms, 0);

    let mut zero_value = base_value();
    zero_value["monotonicMs"] = serde_json::Value::String("0".to_owned());
    let decoded = decode_both(&zero_value.to_string());
    assert!(decoded.wire.monotonic_ms_present);
    assert_eq!(decoded.report.monotonic_ms, 0);

    // Absent and explicit zero normalize identically (equal typed report
    // and digest) with differing metadata: the canonical parse makes
    // presence plus the u64 sufficient.
    let absent_report = decode_request(&one_window_request()).expect("decodes");
    let zero_report = decode_request(&zero_value.to_string()).expect("decodes");
    assert_eq!(absent_report, zero_report);
    assert_eq!(report_digest(&absent_report), report_digest(&zero_report));

    let mut max_value = base_value();
    max_value["monotonicMs"] = serde_json::Value::String(u64::MAX.to_string());
    let decoded = decode_both(&max_value.to_string());
    assert!(decoded.wire.monotonic_ms_present);
    assert_eq!(decoded.report.monotonic_ms, u64::MAX);

    // A JSON number where the clock belongs refuses on both APIs.
    let mut numeric = base_value();
    numeric["monotonicMs"] = serde_json::json!(1200);
    let (kind, _) = assert_same_rejection(&numeric.to_string());
    assert_eq!(kind, "unknown-value");
}

#[test]
fn attempt_lanes_absent_empty_and_nonempty() {
    // Fragment sources (isolated lane arrays only, never whole fixtures):
    // report-optionals-omitted (every lane `[]`) plus one representative
    // nonempty lane fixture each: report-attempt-frame,
    // report-attempt-member-logical, report-attempt-view-native,
    // report-attempt-clear, report-attempt-creation-id.
    let omitted: serde_json::Value = fixture_value("report-optionals-omitted");
    for lane in [
        "frameAttempts",
        "memberAttempts",
        "viewAttempts",
        "clearAttempts",
        "creationAttempts",
    ] {
        assert_eq!(omitted[lane], serde_json::json!([]), "{lane} empty");
    }

    let frame_fixture: serde_json::Value = fixture_value("report-attempt-frame");
    let member_fixture: serde_json::Value = fixture_value("report-attempt-member-logical");
    let view_fixture: serde_json::Value = fixture_value("report-attempt-view-native");
    let clear_fixture: serde_json::Value = fixture_value("report-attempt-clear");
    let creation_fixture: serde_json::Value = fixture_value("report-attempt-creation-id");

    // Absent lanes backfill empty with presence false.
    let decoded = decode_both(&one_window_request());
    assert!(!decoded.wire.frame_attempts_present);
    assert!(!decoded.wire.member_attempts_present);
    assert!(!decoded.wire.view_attempts_present);
    assert!(!decoded.wire.clear_attempts_present);
    assert!(!decoded.wire.creation_attempts_present);
    assert!(decoded.report.frames.is_empty());
    assert!(decoded.report.member_attempts.is_empty());
    assert!(decoded.report.view_attempts.is_empty());
    assert!(decoded.report.clear_attempts.is_empty());
    assert!(decoded.report.creation_attempts.is_empty());

    // Explicit empty arrays stay empty with presence true; the normalized
    // report matches the absent spelling exactly (equal digest).
    let mut empty_lanes = base_value();
    for lane in [
        "frameAttempts",
        "memberAttempts",
        "viewAttempts",
        "clearAttempts",
        "creationAttempts",
    ] {
        empty_lanes[lane] = serde_json::json!([]);
    }
    let decoded = decode_both(&empty_lanes.to_string());
    assert!(decoded.wire.frame_attempts_present);
    assert!(decoded.wire.member_attempts_present);
    assert!(decoded.wire.view_attempts_present);
    assert!(decoded.wire.clear_attempts_present);
    assert!(decoded.wire.creation_attempts_present);
    assert!(decoded.report.frames.is_empty());
    let absent_report = decode_request(&one_window_request()).expect("decodes");
    let empty_report = decode_request(&empty_lanes.to_string()).expect("decodes");
    assert_eq!(absent_report, empty_report);
    assert_eq!(report_digest(&absent_report), report_digest(&empty_report));

    // Nonempty lanes decode to their typed rows with presence true. The
    // rows come verbatim from the immutable v4 lane fixtures above.
    let mut framed = base_value();
    framed["monotonicMs"] = frame_fixture["monotonicMs"].clone();
    framed["frameAttempts"] = frame_fixture["frameAttempts"].clone();
    let decoded = decode_both(&framed.to_string());
    assert!(decoded.wire.frame_attempts_present);
    assert_eq!(
        decoded.report.frames.len(),
        frame_fixture["frameAttempts"]
            .as_array()
            .expect("array")
            .len()
    );
    assert_eq!(decoded.report.monotonic_ms, 50);

    let mut membered = base_value();
    membered["memberAttempts"] = member_fixture["memberAttempts"].clone();
    let decoded = decode_both(&membered.to_string());
    assert!(decoded.wire.member_attempts_present);
    assert_eq!(decoded.report.member_attempts.len(), 2);

    let mut viewed = base_value();
    viewed["viewAttempts"] = view_fixture["viewAttempts"].clone();
    let decoded = decode_both(&viewed.to_string());
    assert!(decoded.wire.view_attempts_present);
    assert_eq!(decoded.report.view_attempts.len(), 2);

    let mut cleared = base_value();
    cleared["clearAttempts"] = clear_fixture["clearAttempts"].clone();
    let decoded = decode_both(&cleared.to_string());
    assert!(decoded.wire.clear_attempts_present);
    assert_eq!(decoded.report.clear_attempts.len(), 2);

    let mut created = base_value();
    created["creationAttempts"] = creation_fixture["creationAttempts"].clone();
    let decoded = decode_both(&created.to_string());
    assert!(decoded.wire.creation_attempts_present);
    assert_eq!(decoded.report.creation_attempts.len(), 1);
    assert_eq!(
        decoded.report.creation_attempts[0]
            .created_native
            .as_deref(),
        Some("native-b")
    );
}

#[test]
fn gesture_ingress_absent_null_and_present() {
    // Fragment sources (isolated `gestureIngress` values only):
    // report-optionals-omitted (`null`) and report-gesture-started (object).
    // report-gesture-started carries neither cursorHit nor monotonicMs, so
    // both ride absent there.
    let started_text = fixture_text("report-gesture-started");
    let started: serde_json::Value = serde_json::from_str(&started_text).expect("fixture JSON");
    assert!(started.get("cursorHit").is_none());
    assert!(started.get("monotonicMs").is_none());

    let decoded = decode_both(&one_window_request());
    assert_eq!(decoded.wire.gesture_ingress, GestureIngressWire::Absent);
    assert!(decoded.report.gesture.is_none());

    let mut null_value = base_value();
    null_value["gestureIngress"] = serde_json::Value::Null;
    let decoded = decode_both(&null_value.to_string());
    assert_eq!(decoded.wire.gesture_ingress, GestureIngressWire::Null);
    assert!(decoded.report.gesture.is_none());

    // Absent and null normalize identically.
    let absent_report = decode_request(&one_window_request()).expect("decodes");
    let null_report = decode_request(&null_value.to_string()).expect("decodes");
    assert_eq!(absent_report, null_report);
    assert_eq!(report_digest(&absent_report), report_digest(&null_report));

    // A present ingress carries the normalized lifecycle facts. The object
    // is the verbatim isolated fragment from report-gesture-started.
    let mut present_value = base_value();
    present_value["gestureIngress"] = started["gestureIngress"].clone();
    let decoded = decode_both(&present_value.to_string());
    assert_eq!(decoded.wire.gesture_ingress, GestureIngressWire::Present);
    let gesture = decoded.report.gesture.as_ref().expect("gesture rides");
    assert_eq!(gesture.window.0, "win-a");
    assert_eq!(gesture.id, 1);
    assert_eq!(gesture.cursor, Some((100, 200)));

    // A second real variant (updated phase with unreadable cursor) rides
    // the same Present spelling through the real gated decoder.
    let mut updated_value = base_value();
    updated_value["gestureIngress"] = serde_json::json!({
        "win": "win-a", "kind": "resize", "phase": "updated", "id": "9",
        "cursor": null, "startCursor": [10, 10],
        "esc": true, "modal": false,
        "start": [0, 0, 40, 90],
    });
    let decoded = decode_both(&updated_value.to_string());
    assert_eq!(decoded.wire.gesture_ingress, GestureIngressWire::Present);
    let gesture = decoded.report.gesture.as_ref().expect("gesture rides");
    assert_eq!(gesture.id, 9);
    assert_eq!(gesture.cursor, None);
}

// --- Test-only canonical fragment renderers --------------------------------
// These spell isolated optional fragments from decoded metadata plus typed
// values. They are test-only sufficiency witnesses, not production writers:
// unit4 owns any production group/request writer.

fn wire_str(text: &str) -> String {
    format!("\"{text}\"")
}

fn rect_text(rect: &Rect) -> String {
    format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h)
}

fn opt_rect_text(rect: &Option<Rect>) -> String {
    rect.as_ref()
        .map(rect_text)
        .unwrap_or_else(|| "null".to_owned())
}

fn opt_bool_text(value: &Option<bool>) -> String {
    value
        .map(|set| set.to_string())
        .unwrap_or_else(|| "null".to_owned())
}

fn workspace_ref_text(reference: &WorkspaceRef) -> String {
    match reference {
        WorkspaceRef::Logical { output, workspace } => format!(
            "{{\"kind\":\"logical\",\"out\":{},\"ws\":{}}}",
            wire_str(&output.0),
            wire_str(&workspace.0)
        ),
        WorkspaceRef::Native { output, workspace } => format!(
            "{{\"kind\":\"native\",\"out\":{},\"ws\":{}}}",
            wire_str(&output.0),
            wire_str(workspace.as_str())
        ),
    }
}

fn opt_workspace_ref_text(reference: &Option<WorkspaceRef>) -> String {
    reference
        .as_ref()
        .map(workspace_ref_text)
        .unwrap_or_else(|| "null".to_owned())
}

fn render_frame_attempt(attempt: &FrameAttempt) -> String {
    let dpi_before = attempt
        .dpi_before
        .map(|dpi| dpi.to_string())
        .unwrap_or_else(|| "null".to_owned());
    let dpi_after = attempt
        .dpi_after
        .map(|dpi| dpi.to_string())
        .unwrap_or_else(|| "null".to_owned());
    format!(
        "{{\"win\":{},\"target\":{},\"setter\":\"{}\",\"readback\":{},\
         \"dpiBefore\":{},\"dpiAfter\":{},\"attemptMs\":\"{}\",\"op\":{},\"pre\":{}}}",
        wire_str(&attempt.window.0),
        rect_text(&attempt.target),
        attempt.setter.as_str(),
        opt_rect_text(&attempt.readback),
        dpi_before,
        dpi_after,
        attempt.attempt_ms,
        wire_str(attempt.operation_id.as_deref().unwrap_or("")),
        opt_rect_text(&attempt.prewrite),
    )
}

fn render_member_attempt(attempt: &MemberAttempt) -> String {
    format!(
        "{{\"op\":{},\"win\":{},\"out\":{},\"ws\":{},\"setter\":\"{}\",\
         \"pre\":{},\"post\":{},\"attemptMs\":\"{}\"}}",
        wire_str(&attempt.operation_id),
        wire_str(&attempt.window.0),
        wire_str(&attempt.target_output.0),
        wire_str(&attempt.target_workspace),
        attempt.setter.as_str(),
        opt_workspace_ref_text(&attempt.pre),
        opt_workspace_ref_text(&attempt.post),
        attempt.attempt_ms,
    )
}

fn render_view_attempt(attempt: &ViewAttempt) -> String {
    format!(
        "{{\"op\":{},\"out\":{},\"ws\":{},\"setter\":\"{}\",\
         \"pre\":{},\"post\":{},\"base\":{},\"attemptMs\":\"{}\"}}",
        wire_str(&attempt.operation_id),
        wire_str(&attempt.target_output.0),
        wire_str(&attempt.target_workspace),
        attempt.setter.as_str(),
        opt_workspace_ref_text(&attempt.pre),
        opt_workspace_ref_text(&attempt.post),
        opt_workspace_ref_text(&attempt.baseline),
        attempt.attempt_ms,
    )
}

fn render_clear_attempt(attempt: &ClearAttempt) -> String {
    format!(
        "{{\"op\":{},\"win\":{},\"setter\":\"{}\",\
         \"pre\":{},\"post\":{},\"attemptMs\":\"{}\"}}",
        wire_str(&attempt.operation_id),
        wire_str(&attempt.window.0),
        attempt.setter.as_str(),
        opt_bool_text(&attempt.pre_maximized),
        opt_bool_text(&attempt.post_maximized),
        attempt.attempt_ms,
    )
}

fn render_creation_attempt(attempt: &NativeCreationAttempt) -> String {
    let before = attempt
        .order_before
        .iter()
        .map(|id| wire_str(id))
        .collect::<Vec<_>>()
        .join(",");
    let after = attempt
        .order_after
        .iter()
        .map(|id| wire_str(id))
        .collect::<Vec<_>>()
        .join(",");
    let created = attempt
        .created_native
        .as_deref()
        .map(|id| format!(",\"created\":{}", wire_str(id)))
        .unwrap_or_default();
    format!(
        "{{\"op\":{},\"ws\":{},\"out\":{},\"setter\":\"{}\",\
         \"before\":[{}],\"after\":[{}]{},\"attemptMs\":\"{}\"}}",
        wire_str(&attempt.operation_id),
        wire_str(&attempt.logical_workspace),
        wire_str(&attempt.target_output.0),
        attempt.setter.as_str(),
        before,
        after,
        created,
        attempt.attempt_ms,
    )
}

fn render_gesture_ingress(gesture: &tiler_core::gesture::GestureIngress) -> String {
    let cursor = gesture
        .cursor
        .map(|(x, y)| format!("[{x},{y}]"))
        .unwrap_or_else(|| "null".to_owned());
    let start_cursor = gesture
        .start_cursor
        .map(|(x, y)| format!("[{x},{y}]"))
        .unwrap_or_else(|| "null".to_owned());
    format!(
        "{{\"win\":{},\"kind\":\"{}\",\"phase\":\"{}\",\"id\":\"{}\",\
         \"cursor\":{},\"startCursor\":{},\"esc\":{},\"modal\":{},\"start\":{}}}",
        wire_str(&gesture.window.0),
        gesture.kind.as_str(),
        gesture.phase.as_str(),
        gesture.id,
        cursor,
        start_cursor,
        gesture.esc,
        gesture.modal_move,
        rect_text(&gesture.start_frame),
    )
}

// --- Metadata-plus-report fragment reconstruction --------------------------
// Each helper below rebuilds the exact carried `"key":value` fragment from
// decoded wire metadata plus the normalized typed report (`None` when the
// key was omitted). Any present-flag regression fails the call-site
// assertions below.

fn render_cursor_hit(wire: &ReportWireOptionals, report: &WorldReportV4) -> Option<String> {
    match wire.cursor_hit {
        CursorHitWire::Absent => None,
        CursorHitWire::Null => Some("null".to_owned()),
        CursorHitWire::Empty => Some(wire_str("")),
        CursorHitWire::Id => Some(wire_str(
            report.cursor_hit.as_ref().expect("id hit").0.as_str(),
        )),
    }
}

fn render_monotonic_ms(wire: &ReportWireOptionals, report: &WorldReportV4) -> Option<String> {
    wire.monotonic_ms_present
        .then(|| format!("\"{}\"", report.monotonic_ms))
}

fn render_lane(present: bool, rows: Vec<String>) -> Option<String> {
    present.then(|| format!("[{}]", rows.join(",")))
}

fn render_gesture(wire: &ReportWireOptionals, report: &WorldReportV4) -> Option<String> {
    match wire.gesture_ingress {
        GestureIngressWire::Absent => None,
        GestureIngressWire::Null => Some("null".to_owned()),
        GestureIngressWire::Present => Some(render_gesture_ingress(
            report.gesture.as_ref().expect("present gesture"),
        )),
    }
}

#[test]
fn optional_fragments_reconstruct_byte_exact() {
    // Every rendered fragment below is built ONLY from decoded wire
    // metadata plus normalized typed values, then checked byte-exact
    // against the immutable fixture text it was extracted from (substring
    // match) and value-equal to the extracted parsed fragment. This proves
    // the metadata is sufficient to reproduce the carried optional groups.

    // cursorHit: every spelling rebuilds from decoded metadata plus the
    // normalized report, then matches the immutable fixture pair bytes.
    let omitted_text = fixture_text("report-optionals-omitted");
    let mut absent_cursor = base_value();
    absent_cursor["cursor"] = serde_json::json!([100, 200]);
    let decoded = decode_both(&absent_cursor.to_string());
    assert_eq!(render_cursor_hit(&decoded.wire, &decoded.report), None);
    assert!(
        !omitted_text.contains("cursorHit"),
        "omitted fixture carries no cursorHit pair"
    );
    let null_text = fixture_text("report-optionals-null");
    let mut null_cursor = base_value();
    null_cursor["cursor"] = serde_json::json!([100, 200]);
    null_cursor["cursorHit"] = serde_json::Value::Null;
    let decoded = decode_both(&null_cursor.to_string());
    assert_eq!(
        render_cursor_hit(&decoded.wire, &decoded.report),
        Some("null".to_owned())
    );
    assert!(null_text.contains("\"cursorHit\":null"));
    let empty_text = fixture_text("report-optionals-empty");
    let mut empty_cursor = base_value();
    empty_cursor["cursor"] = serde_json::json!([100, 200]);
    empty_cursor["cursorHit"] = serde_json::Value::String(String::new());
    let decoded = decode_both(&empty_cursor.to_string());
    assert_eq!(
        render_cursor_hit(&decoded.wire, &decoded.report),
        Some(wire_str(""))
    );
    assert!(empty_text.contains("\"cursorHit\":\"\""));
    let id_text = fixture_text("report-optionals-id");
    let mut id_cursor = base_value();
    id_cursor["cursor"] = serde_json::json!([100, 200]);
    id_cursor["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let decoded = decode_both(&id_cursor.to_string());
    assert_eq!(
        render_cursor_hit(&decoded.wire, &decoded.report),
        Some(wire_str("win-a"))
    );
    assert!(id_text.contains("\"cursorHit\":\"win-a\""));

    // monotonicMs: omitted renders nothing, while explicit zero and max
    // rebuild the exact canonical decimal strings (presence plus the u64
    // is sufficient).
    let decoded = decode_both(&one_window_request());
    assert_eq!(render_monotonic_ms(&decoded.wire, &decoded.report), None);
    let mut zero_clock = base_value();
    zero_clock["monotonicMs"] = serde_json::Value::String("0".to_owned());
    let decoded = decode_both(&zero_clock.to_string());
    assert_eq!(
        render_monotonic_ms(&decoded.wire, &decoded.report),
        Some("\"0\"".to_owned())
    );
    assert!(omitted_text.contains("\"monotonicMs\":\"0\""));
    let mut max_clock = base_value();
    max_clock["monotonicMs"] = serde_json::Value::String(u64::MAX.to_string());
    let decoded = decode_both(&max_clock.to_string());
    let rendered =
        render_monotonic_ms(&decoded.wire, &decoded.report).expect("present clock renders");
    assert_eq!(rendered, format!("\"{}\"", u64::MAX));
    assert!(id_text.contains(&format!("\"monotonicMs\":{rendered}")));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&format!("{{\"monotonicMs\":{rendered}}}"))
            .expect("fragment"),
        serde_json::json!({"monotonicMs": u64::MAX.to_string()}),
    );

    // Attempt lanes: an absent lane renders omitted for every lane (any
    // false-flag regression fails here), while an explicitly empty lane
    // renders `[]` byte-exact.
    let decoded = decode_both(&one_window_request());
    assert_eq!(
        render_lane(decoded.wire.frame_attempts_present, vec![]),
        None
    );
    assert_eq!(
        render_lane(decoded.wire.member_attempts_present, vec![]),
        None
    );
    assert_eq!(
        render_lane(decoded.wire.view_attempts_present, vec![]),
        None
    );
    assert_eq!(
        render_lane(decoded.wire.clear_attempts_present, vec![]),
        None
    );
    assert_eq!(
        render_lane(decoded.wire.creation_attempts_present, vec![]),
        None
    );
    let mut empty_lanes = base_value();
    for lane in [
        "frameAttempts",
        "memberAttempts",
        "viewAttempts",
        "clearAttempts",
        "creationAttempts",
    ] {
        empty_lanes[lane] = serde_json::json!([]);
    }
    let decoded = decode_both(&empty_lanes.to_string());
    for (present, lane) in [
        (decoded.wire.frame_attempts_present, "frameAttempts"),
        (decoded.wire.member_attempts_present, "memberAttempts"),
        (decoded.wire.view_attempts_present, "viewAttempts"),
        (decoded.wire.clear_attempts_present, "clearAttempts"),
        (decoded.wire.creation_attempts_present, "creationAttempts"),
    ] {
        assert_eq!(render_lane(present, vec![]), Some("[]".to_owned()));
        assert!(
            omitted_text.contains(&format!("\"{lane}\":[]")),
            "{lane} empty fragment"
        );
    }

    // Nonempty frame rows: every typed row renders back to its exact
    // fixture bytes.
    let frame_text = fixture_text("report-attempt-frame");
    let frame_fixture: serde_json::Value = serde_json::from_str(&frame_text).expect("fixture JSON");
    let mut framed = base_value();
    framed["frameAttempts"] = frame_fixture["frameAttempts"].clone();
    let decoded = decode_both(&framed.to_string());
    let rows = frame_fixture["frameAttempts"].as_array().expect("array");
    assert_eq!(decoded.report.frames.len(), rows.len());
    for (attempt, row) in decoded.report.frames.iter().zip(rows.iter()) {
        let rendered = render_frame_attempt(attempt);
        assert!(
            frame_text.contains(&rendered),
            "frame row renders byte-exact"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
            *row,
        );
    }
    // The whole carried group rebuilds from the lane flag plus typed rows.
    let rendered_rows: Vec<String> = decoded
        .report
        .frames
        .iter()
        .map(render_frame_attempt)
        .collect();
    let group =
        render_lane(decoded.wire.frame_attempts_present, rendered_rows).expect("present lane");
    assert!(
        frame_text.contains(&format!("\"frameAttempts\":{group}")),
        "frame group renders byte-exact"
    );

    // Nonempty member rows (logical plus null-homing witnesses).
    let member_text = fixture_text("report-attempt-member-logical");
    let member_fixture: serde_json::Value =
        serde_json::from_str(&member_text).expect("fixture JSON");
    let mut membered = base_value();
    membered["memberAttempts"] = member_fixture["memberAttempts"].clone();
    let decoded = decode_both(&membered.to_string());
    let rows = member_fixture["memberAttempts"].as_array().expect("array");
    for (attempt, row) in decoded.report.member_attempts.iter().zip(rows.iter()) {
        let rendered = render_member_attempt(attempt);
        assert!(
            member_text.contains(&rendered),
            "member row renders byte-exact"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
            *row,
        );
    }
    let rendered_rows: Vec<String> = decoded
        .report
        .member_attempts
        .iter()
        .map(render_member_attempt)
        .collect();
    let group =
        render_lane(decoded.wire.member_attempts_present, rendered_rows).expect("present lane");
    assert!(
        member_text.contains(&format!("\"memberAttempts\":{group}")),
        "member group renders byte-exact"
    );

    // Nonempty view rows (native proofs).
    let view_text = fixture_text("report-attempt-view-native");
    let view_fixture: serde_json::Value = serde_json::from_str(&view_text).expect("fixture JSON");
    let mut viewed = base_value();
    viewed["viewAttempts"] = view_fixture["viewAttempts"].clone();
    let decoded = decode_both(&viewed.to_string());
    let rows = view_fixture["viewAttempts"].as_array().expect("array");
    for (attempt, row) in decoded.report.view_attempts.iter().zip(rows.iter()) {
        let rendered = render_view_attempt(attempt);
        assert!(view_text.contains(&rendered), "view row renders byte-exact");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
            *row,
        );
    }
    let rendered_rows: Vec<String> = decoded
        .report
        .view_attempts
        .iter()
        .map(render_view_attempt)
        .collect();
    let group =
        render_lane(decoded.wire.view_attempts_present, rendered_rows).expect("present lane");
    assert!(
        view_text.contains(&format!("\"viewAttempts\":{group}")),
        "view group renders byte-exact"
    );

    // Nonempty clear rows.
    let clear_text = fixture_text("report-attempt-clear");
    let clear_fixture: serde_json::Value = serde_json::from_str(&clear_text).expect("fixture JSON");
    let mut cleared = base_value();
    cleared["clearAttempts"] = clear_fixture["clearAttempts"].clone();
    let decoded = decode_both(&cleared.to_string());
    let rows = clear_fixture["clearAttempts"].as_array().expect("array");
    for (attempt, row) in decoded.report.clear_attempts.iter().zip(rows.iter()) {
        let rendered = render_clear_attempt(attempt);
        assert!(
            clear_text.contains(&rendered),
            "clear row renders byte-exact"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
            *row,
        );
    }
    let rendered_rows: Vec<String> = decoded
        .report
        .clear_attempts
        .iter()
        .map(render_clear_attempt)
        .collect();
    let group =
        render_lane(decoded.wire.clear_attempts_present, rendered_rows).expect("present lane");
    assert!(
        clear_text.contains(&format!("\"clearAttempts\":{group}")),
        "clear group renders byte-exact"
    );

    // Nonempty creation row with correlated receipt.
    let creation_text = fixture_text("report-attempt-creation-id");
    let creation_fixture: serde_json::Value =
        serde_json::from_str(&creation_text).expect("fixture JSON");
    let mut created = base_value();
    created["creationAttempts"] = creation_fixture["creationAttempts"].clone();
    let decoded = decode_both(&created.to_string());
    let rows = creation_fixture["creationAttempts"]
        .as_array()
        .expect("array");
    for (attempt, row) in decoded.report.creation_attempts.iter().zip(rows.iter()) {
        let rendered = render_creation_attempt(attempt);
        assert!(
            creation_text.contains(&rendered),
            "creation row renders byte-exact"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
            *row,
        );
    }
    let rendered_rows: Vec<String> = decoded
        .report
        .creation_attempts
        .iter()
        .map(render_creation_attempt)
        .collect();
    let group =
        render_lane(decoded.wire.creation_attempts_present, rendered_rows).expect("present lane");
    assert!(
        creation_text.contains(&format!("\"creationAttempts\":{group}")),
        "creation group renders byte-exact"
    );

    // Gesture: omitted and null rebuild from metadata, while a present
    // object rebuilds from the typed ingress and matches fixture bytes.
    let decoded = decode_both(&one_window_request());
    assert_eq!(render_gesture(&decoded.wire, &decoded.report), None);
    let mut null_gesture = base_value();
    null_gesture["gestureIngress"] = serde_json::Value::Null;
    let decoded = decode_both(&null_gesture.to_string());
    assert_eq!(
        render_gesture(&decoded.wire, &decoded.report),
        Some("null".to_owned())
    );
    assert!(omitted_text.contains("\"gestureIngress\":null"));
    // Present gesture object renders back to its exact fixture bytes.
    let started_text = fixture_text("report-gesture-started");
    let started: serde_json::Value = serde_json::from_str(&started_text).expect("fixture JSON");
    let mut present_value = base_value();
    present_value["gestureIngress"] = started["gestureIngress"].clone();
    let decoded = decode_both(&present_value.to_string());
    let rendered = render_gesture(&decoded.wire, &decoded.report).expect("present gesture");
    assert!(
        started_text.contains(&rendered),
        "gesture renders byte-exact"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rendered).expect("fragment"),
        started["gestureIngress"],
    );
}

#[test]
fn malformed_optionals_reject_identically_on_both_apis() {
    // cursorHit: wrong types and bad grammars refuse.
    for hit in [
        serde_json::json!(7),
        serde_json::json!("not an id!"),
        serde_json::Value::String("win-ghost".to_owned()),
    ] {
        let mut value = base_value();
        value["cursor"] = serde_json::json!([100, 200]);
        value["cursorHit"] = hit;
        assert_same_rejection(&value.to_string());
    }
    // A hit without the cursor it was resolved from refuses.
    let mut cursorless = base_value();
    cursorless["cursor"] = serde_json::Value::Null;
    cursorless["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let (kind, _) = assert_same_rejection(&cursorless.to_string());
    assert_eq!(kind, "snapshot-invalid");

    // monotonicMs: non-canonical counters refuse.
    for clock in [
        serde_json::json!(1200),
        serde_json::json!("007"),
        serde_json::json!("-1"),
    ] {
        let mut value = base_value();
        value["monotonicMs"] = clock;
        let (kind, _) = assert_same_rejection(&value.to_string());
        assert_eq!(kind, "unknown-value");
    }

    // frameAttempts: a bad setter still refuses, while an unknown field
    // holds the identifiable row on both APIs (the genuine v4 attempt
    // lanes keep unknown fields narrowly instead of refusing the
    // envelope).
    let mut bad_setter = base_value();
    bad_setter["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "clamped",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1000", "op": "", "pre": null,
    }]);
    let (kind, _) = assert_same_rejection(&bad_setter.to_string());
    assert_eq!(kind, "unknown-value");
    let mut unknown_field = base_value();
    unknown_field["frameAttempts"] = serde_json::json!([{
        "win": "win-a", "target": [0, 0, 800, 560], "setter": "threw",
        "readback": null, "dpiBefore": null, "dpiAfter": null,
        "attemptMs": "1000", "op": "", "pre": null, "verdict": "clamp",
    }]);
    let unknown_wire = unknown_field.to_string();
    assert_same_hold(&unknown_wire, AttemptLaneV4::Frame, &["win-a"]);
    let held = decode_request(&unknown_wire).expect("held decodes");
    assert!(held.frames.is_empty(), "unknown field holds the frame row");
    assert!(
        decode_request_with_wire(&unknown_wire)
            .expect("held decodes")
            .wire
            .frame_attempts_present,
        "lane stays present"
    );
    assert_eq!(
        held.held_attempts[0]
            .fields
            .last()
            .map(|(name, _)| name.as_str()),
        Some("verdict"),
        "retired key stages last"
    );

    // memberAttempts: unknown fields hold the identifiable row on both
    // APIs for the same v4 reason.
    let mut bad_member = base_value();
    bad_member["memberAttempts"] = serde_json::json!([{
        "op": "transfer-a", "win": "win-a", "out": "out", "ws": "ws-a",
        "setter": "accepted", "pre": null, "post": null, "attemptMs": "20",
        "nativePost": "native-a",
    }]);
    let member_wire = bad_member.to_string();
    assert_same_hold(
        &member_wire,
        AttemptLaneV4::Member,
        &["transfer-a", "win-a"],
    );
    let held = decode_request(&member_wire).expect("held decodes");
    assert!(
        held.member_attempts.is_empty(),
        "unknown field holds the member row"
    );

    // viewAttempts: empty typed refs hold the identifiable row on both
    // APIs for the same v4 reason.
    let mut bad_view = base_value();
    bad_view["viewAttempts"] = serde_json::json!([{
        "op": "op-9", "out": "out", "ws": "ws-1", "setter": "threw",
        "pre": "", "post": "", "base": "", "attemptMs": "1",
    }]);
    let view_wire = bad_view.to_string();
    assert_same_hold(&view_wire, AttemptLaneV4::View, &["op-9"]);
    let held = decode_request(&view_wire).expect("held decodes");
    assert!(
        held.view_attempts.is_empty(),
        "empty refs hold the view row"
    );

    // clearAttempts: duplicates refuse.
    let mut dup_clear = base_value();
    dup_clear["clearAttempts"] = serde_json::json!([
        {"op": "op-9", "win": "win-a", "setter": "accepted",
         "pre": true, "post": false, "attemptMs": "1"},
        {"op": "op-9", "win": "win-a", "setter": "accepted",
         "pre": true, "post": false, "attemptMs": "2"},
    ]);
    let (kind, message) = assert_same_rejection(&dup_clear.to_string());
    assert_eq!(kind, "snapshot-invalid");
    assert_eq!(message, "duplicate-clear-attempt");

    // creationAttempts: a wrong-typed receipt holds the identifiable
    // row on both APIs for the same v4 reason (tolerated, never a
    // fabricated receipt).
    let mut bad_creation = base_value();
    bad_creation["creationAttempts"] = serde_json::json!([{
        "op": "op-native-1", "ws": "ws-1", "out": "out", "setter": "accepted",
        "before": [], "after": ["ws"], "created": 7, "attemptMs": "5",
    }]);
    let creation_wire = bad_creation.to_string();
    assert_same_hold(&creation_wire, AttemptLaneV4::Creation, &["op-native-1"]);
    let held = decode_request(&creation_wire).expect("held decodes");
    assert!(
        held.creation_attempts.is_empty(),
        "wrong-typed receipt holds the creation row"
    );

    // gestureIngress: zero id, missing keys, and unknown tokens refuse.
    let mut gesture_base = base_value();
    gesture_base["gestureIngress"] = serde_json::json!({
        "win": "win-a", "kind": "move", "phase": "updated", "id": "7",
        "cursor": [150, 50], "startCursor": [10, 10],
        "esc": false, "modal": true, "start": [0, 0, 40, 90],
    });
    let wire = gesture_base.to_string();
    let mut zero_id: serde_json::Value = serde_json::from_str(&wire).expect("valid");
    zero_id["gestureIngress"]["id"] = serde_json::json!("0");
    let (kind, _) = assert_same_rejection(&zero_id.to_string());
    assert_eq!(kind, "unknown-value");
    let mut bad_kind: serde_json::Value = serde_json::from_str(&wire).expect("valid");
    bad_kind["gestureIngress"]["kind"] = serde_json::json!("drag");
    let (kind, _) = assert_same_rejection(&bad_kind.to_string());
    assert_eq!(kind, "unknown-value");
    let mut missing_id: serde_json::Value = serde_json::from_str(&wire).expect("valid");
    missing_id["gestureIngress"]
        .as_object_mut()
        .expect("object")
        .remove("id");
    assert_same_rejection(&missing_id.to_string());

    // Report-level vocabulary is unchanged: unknown keys and explicit
    // null primary refuse on both APIs.
    let mut unknown_key = base_value();
    unknown_key["restored"] = serde_json::json!({"rings": []});
    let (kind, _) = assert_same_rejection(&unknown_key.to_string());
    assert_eq!(kind, "unknown-field");
    let mut null_primary = base_value();
    null_primary["primary"] = serde_json::Value::Null;
    let (kind, _) = assert_same_rejection(&null_primary.to_string());
    assert_eq!(kind, "malformed");
}

#[test]
fn wrapper_preserves_primary_native_and_held_row_semantics() {
    // primary: absent stays unknown; a carried id designates.
    let decoded = decode_both(&one_window_request());
    assert_eq!(decoded.report.primary_output, None);
    let mut primary_value = base_value();
    primary_value["primary"] = serde_json::Value::String("out".to_owned());
    let decoded = decode_both(&primary_value.to_string());
    assert_eq!(
        decoded
            .report
            .primary_output
            .as_ref()
            .map(|out| out.0.as_str()),
        Some("out")
    );

    // nativeWs keeps its four tolerant spellings in the typed row; the
    // wrapper changes nothing there.
    let decoded = decode_both(&one_window_request());
    assert_eq!(
        decoded.report.windows[0].native_workspace,
        NativeWorkspaceFact::Omitted
    );
    for (native, expected) in [
        (serde_json::Value::Null, NativeWorkspaceFact::Null),
        (
            serde_json::Value::String(String::new()),
            NativeWorkspaceFact::Value(String::new()),
        ),
        (
            serde_json::Value::String("d1".to_owned()),
            NativeWorkspaceFact::Value("d1".to_owned()),
        ),
    ] {
        let mut value = base_value();
        value["native"] = serde_json::json!([{"out": "out-a", "order": ["d1"], "current": "d1", "occupancy": [{"native": "d1", "count": null}]}]);
        let windows = value["windows"].as_array_mut().expect("windows");
        // Minimal held-free v4-shaped row carrying the spelling under test.
        let mut row: serde_json::Value = serde_json::from_str(
            "{\"win\":\"win-a\",\"out\":\"out\",\"rect\":[0,0,200,200],\
              \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\"minimized\":false,\
              \"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null},\
              \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
              \"acceptsInput\":true,\"activationInhibited\":false,\
              \"toolWindow\":false,\"owned\":false,\"transient\":false,\
              \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
              \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
              \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
              \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
              \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
              \"suppressionAvailable\":null,\
              \"visibility\":{\"visible\":true,\"suppressionOwned\":null},\"lastWrite\":null}",
        )
        .expect("row");
        row["nativeWs"] = native;
        windows[0] = row;
        let decoded = decode_both(&value.to_string());
        assert_eq!(decoded.report.windows[0].native_workspace, expected);
    }

    // Held rows: a retired key holds the identifiable row with raw fields
    // staged in canonical schema order and the retired key last. The
    // wrapper report (including held staging and order) equals the plain
    // decoder.
    let mut retired_value = base_value();
    retired_value["windows"].as_array_mut().expect("windows")[0]["floating"] =
        serde_json::Value::Bool(false);
    let plain = decode_request(&retired_value.to_string()).expect("held decodes");
    let wired = decode_request_with_wire(&retired_value.to_string()).expect("held decodes");
    assert_eq!(plain, wired.report);
    assert!(plain.windows.is_empty());
    assert_eq!(plain.held_windows.len(), 1);
    let names: Vec<&str> = wired.report.held_windows[0]
        .fields
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let schema_order = [
        "win",
        "out",
        "rect",
        "fullscreen",
        "maximized",
        "sticky",
        "minimized",
        "hints",
        "hostHidden",
        "privileged",
        "managed",
        "acceptsInput",
        "activationInhibited",
        "toolWindow",
        "owned",
        "transient",
        "modal",
        "resizable",
        "above",
        "below",
        "owner",
        "parent",
        "role",
        "appId",
        "executable",
        "processId",
        "class",
        "title",
        "type",
        "activationPolicy",
        "layer",
        "fullscreenOwner",
        "fullscreenExitable",
        "suppressionAvailable",
        "visibility",
        "lastWrite",
    ];
    assert_eq!(&names[..schema_order.len()], &schema_order);
    assert_eq!(names[schema_order.len()..], ["floating"]);
}

#[test]
fn engine_duplicate_and_reused_on_optional_spellings() {
    // Real handshake plus settings, then same-sequence reports that differ
    // only in optional spelling. Normalized-equal spellings stay Duplicate
    // with unchanged bytes; a changed optional value reuses the sequence
    // and is rejected.
    let mut engine = connected_engine("gen-1", HostCapabilities::default());

    let first_report = decode_request(&one_window_request()).expect("decodes");
    let accepted = match engine.handle_report(&first_report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    let accepted_bytes = reply_wire(&accepted);

    // Same reqSeq/req/obs with an equivalent spelling (explicit null hit)
    // decodes to the identical typed report: Duplicate, same bytes.
    let mut null_hit = base_value();
    null_hit["cursorHit"] = serde_json::Value::Null;
    let null_json = null_hit.to_string();
    let plain = decode_request(&null_json).expect("decodes");
    let wired = decode_request_with_wire(&null_json).expect("decodes");
    assert_eq!(plain, wired.report);
    assert_eq!(
        plain,
        decode_request(&one_window_request()).expect("decodes"),
        "null hit normalizes like absence"
    );
    assert_eq!(
        report_digest(&plain),
        report_digest(&first_report),
        "digests match across equivalent spellings"
    );
    assert_ne!(
        decode_request_with_wire(&one_window_request())
            .expect("decodes")
            .wire,
        wired.wire,
        "metadata still differs"
    );
    match engine.handle_report(&plain) {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(
                reply_wire(&cached),
                accepted_bytes,
                "duplicate keeps unchanged bytes"
            );
        }
        other => panic!("expected duplicate, got {other:?}"),
    }

    // Same reqSeq with a changed optional value (clock 0 -> 1200) carries a
    // different digest: Reused rejection, never execution.
    let mut clocked = base_value();
    clocked["monotonicMs"] = serde_json::Value::String("1200".to_owned());
    let clocked_report = decode_request(&clocked.to_string()).expect("decodes");
    assert_ne!(
        report_digest(&clocked_report),
        report_digest(&decode_request(&one_window_request()).expect("decodes")),
    );
    match engine.handle_report(&clocked_report) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "request-reused");
            assert_eq!(message, "request sequence reused with different input");
        }
        other => panic!("expected reused rejection, got {other:?}"),
    }
}
