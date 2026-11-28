//! Focused PW-EV-04 regressions: tagged attempt dispositions, structural frame
//! purpose, and disjoint focus no-call vocabulary.

use tiler_core::geometry::Rect;
use tiler_core::world_report::{FocusDisposition, FocusNoCallReason, FramePurpose};
use tiler_protocol::world_protocol::{decode_report_v4, encode_report_v4};

fn base_report_json() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test-fixtures/world-wire/v4-active/messages/report-activation-regular.json"
    ))
    .expect("minimal report fixture parses")
}

#[test]
fn pw_ev_04_lane_disallowed_disposition_arms_never_enter_typed_evidence() {
    for bad_disp in [
        serde_json::json!({"kind": "already-at-target"}),
        serde_json::json!({"kind": "not-called", "reason": "scope-held"}),
    ] {
        let mut msg = base_report_json();
        msg["propertyAttempts"] = serde_json::json!([{
            "kind": "frame",
            "win": "win-a",
            "target": [0, 0, 800, 600],
            "disposition": bad_disp,
            "readback": null,
            "dpiBefore": null,
            "dpiAfter": null,
            "attemptMs": "100",
            "purpose": {"kind": "layout"}
        }]);
        assert!(
            decode_report_v4(&msg.to_string()).is_err(),
            "frame must reject non-called disposition"
        );
    }

    for bad_disp in [
        serde_json::json!({"kind": "already-at-target"}),
        serde_json::json!({"kind": "not-called", "reason": "scope-held"}),
    ] {
        let mut msg = base_report_json();
        msg["propertyAttempts"] = serde_json::json!([{
            "kind": "state",
            "win": "win-a",
            "property": "maximized",
            "target": false,
            "rev": "1",
            "disposition": bad_disp,
            "pre": true,
            "post": true,
            "attemptMs": "100"
        }]);
        assert!(
            decode_report_v4(&msg.to_string()).is_err(),
            "state must reject non-called disposition"
        );
    }

    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "view",
        "op": "op-1",
        "out": "out-a",
        "ws": "ws-1",
        "disposition": {"kind": "already-at-target"},
        "pre": null,
        "post": null,
        "base": null,
        "attemptMs": "100",
        "postMember": null
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("identifiable view row holds");
    assert!(decoded.report.view_attempts.is_empty());
    assert_eq!(decoded.report.held_attempts.len(), 1);

    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "member",
        "op": "op-1",
        "win": "win-a",
        "out": "out-a",
        "ws": "ws-1",
        "disposition": {"kind": "not-called", "reason": "scope-held"},
        "pre": null,
        "post": null,
        "attemptMs": "100"
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("identifiable member row holds");
    assert!(decoded.report.member_attempts.is_empty());
    assert_eq!(decoded.report.held_attempts.len(), 1);

    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "visibility",
        "win": "win-a",
        "target": true,
        "revision": "1",
        "path": "staged",
        "disposition": {"kind": "already-at-target"},
        "pre": null,
        "post": null
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("identifiable vis row holds");
    assert!(decoded.report.visibility_attempts.is_empty());
    assert_eq!(decoded.report.held_attempts.len(), 1);

    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "visibility",
        "win": "win-a",
        "target": true,
        "revision": "1",
        "path": "staged",
        "disposition": {"kind": "not-called", "reason": "scope-held"},
        "reason": "scope-held",
        "pre": null,
        "post": null
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("vis not-called with row reason holds");
    assert!(decoded.report.visibility_attempts.is_empty());
    assert_eq!(decoded.report.held_attempts.len(), 1);
}

#[test]
fn pw_ev_04_focus_no_call_rejects_visibility_vocab_and_accepts_focus_vocab() {
    for vis_token in ["", "scope-held", "show-blocked", "focus-unproven"] {
        let mut msg = base_report_json();
        msg["focusFeedback"] = serde_json::json!([{
            "id": "fx-1",
            "target": "win-a",
            "rev": "1",
            "disposition": {"kind": "not-called", "reason": vis_token},
            "pre": null,
            "post": null,
            "foregroundProven": false,
            "arrival": [],
            "attemptMs": "100"
        }]);
        assert!(
            decode_report_v4(&msg.to_string()).is_err(),
            "focus not-called must reject visibility token {vis_token:?}"
        );
    }

    let mut msg = base_report_json();
    msg["focusFeedback"] = serde_json::json!([{
        "id": "fx-1",
        "target": "win-a",
        "rev": "1",
        "disposition": {"kind": "not-called", "reason": "focus-foreground-unproven"},
        "pre": null,
        "post": null,
        "foregroundProven": false,
        "arrival": [],
        "attemptMs": "100"
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("focus no-call decodes");
    assert_eq!(
        decoded.report.focus_feedback[0].disposition,
        FocusDisposition::NotCalled {
            reason: FocusNoCallReason::ForegroundUnproven,
        }
    );
    let encoded = encode_report_v4(&decoded).expect("focus no-call encodes");
    assert!(encoded.contains(
        "\"disposition\":{\"kind\":\"not-called\",\"reason\":\"focus-foreground-unproven\"}"
    ));
    assert!(!encoded.contains("},\"reason\":"));
}

#[test]
fn pw_ev_04_frame_purposes_round_trip_and_legacy_op_pre_held() {
    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "frame",
        "win": "win-a",
        "target": [0, 0, 800, 600],
        "disposition": {"kind": "called", "setter": "accepted"},
        "readback": [0, 0, 800, 600],
        "dpiBefore": 96,
        "dpiAfter": 96,
        "attemptId": "1",
        "attemptMs": "100",
        "purpose": {"kind": "layout"}
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("layout frame decodes");
    assert_eq!(decoded.report.frames.len(), 1);
    assert_eq!(decoded.report.frames[0].purpose, FramePurpose::Layout);
    let encoded = encode_report_v4(&decoded).expect("layout frame encodes");
    assert!(encoded.contains("\"purpose\":{\"kind\":\"layout\"}"));
    let redecoded = decode_report_v4(&encoded).expect("encoded layout frame re-decodes");
    assert_eq!(redecoded.report.frames.len(), 1);
    assert_eq!(redecoded.report.frames[0].purpose, FramePurpose::Layout);

    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "frame",
        "win": "win-a",
        "target": [0, 0, 800, 600],
        "disposition": {"kind": "called", "setter": "refused"},
        "readback": [0, 0, 700, 500],
        "dpiBefore": null,
        "dpiAfter": null,
        "attemptId": "2",
        "attemptMs": "101",
        "purpose": {"kind": "float-placement", "op": "op-9", "pre": [0, 0, 700, 500]}
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("float frame decodes");
    assert_eq!(decoded.report.frames.len(), 1);
    assert_eq!(
        decoded.report.frames[0].purpose,
        FramePurpose::FloatPlacement {
            operation_id: "op-9".to_owned(),
            prewrite: Some(Rect {
                x: 0,
                y: 0,
                w: 700,
                h: 500,
            }),
        }
    );
    let encoded = encode_report_v4(&decoded).expect("float frame encodes");
    assert!(encoded.contains(
        "\"purpose\":{\"kind\":\"float-placement\",\"op\":\"op-9\",\"pre\":[0,0,700,500]}"
    ));
    let redecoded = decode_report_v4(&encoded).expect("encoded float frame re-decodes");
    assert_eq!(redecoded.report.frames.len(), 1);
    assert_eq!(
        redecoded.report.frames[0].purpose,
        decoded.report.frames[0].purpose
    );

    // Hybrid shape (tagged disposition + flat op/pre) is held, never enters typed frames.
    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "frame",
        "win": "win-a",
        "target": [0, 0, 800, 600],
        "disposition": {"kind": "called", "setter": "accepted"},
        "readback": [0, 0, 800, 600],
        "dpiBefore": null,
        "dpiAfter": null,
        "attemptId": "3",
        "attemptMs": "102",
        "op": "",
        "pre": null
    }]);
    let decoded = decode_report_v4(&msg.to_string()).expect("hybrid legacy frame holds");
    assert!(decoded.report.frames.is_empty());
    assert_eq!(decoded.report.held_attempts.len(), 1);

    // Fully retired flat shape (flat setter + flat op/pre, no disposition/purpose) refuses.
    let mut msg = base_report_json();
    msg["propertyAttempts"] = serde_json::json!([{
        "kind": "frame",
        "win": "win-a",
        "target": [0, 0, 800, 600],
        "setter": "accepted",
        "readback": [0, 0, 800, 600],
        "dpiBefore": null,
        "dpiAfter": null,
        "attemptMs": "103",
        "op": "",
        "pre": null
    }]);
    assert!(
        decode_report_v4(&msg.to_string()).is_err(),
        "fully retired flat frame shape must refuse"
    );
}
