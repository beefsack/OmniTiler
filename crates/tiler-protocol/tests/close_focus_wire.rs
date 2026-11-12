//! Focused wire conformance for the retained focus-request lane:
//! `foregroundKnown` provenance on reports and the always-present nullable
//! `focusRequest` lane on replies. Offline only.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_report::{
    AfterShown, AfterShownMember, AfterShownScope, EffectCause, EffectKind, FocusRequest,
    FocusRequestCause, OneShotEffect,
};
use tiler_core::world_report_v4_checks::{report_digest_v4, validate_report_v4_reason};
use tiler_core::world_sizing_v4::{
    reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{
    WORLD_MAX_REPLY_BYTES, decode_reply_v4, decode_report_v4, encode_reply_v4, encode_report_v4,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

fn reply_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active")
}

fn fixture_text(file: &str) -> String {
    std::fs::read_to_string(fixture_root().join(file)).expect("fixture reads")
}

fn reply_text(file: &str) -> String {
    let path = reply_root().join(file);
    assert!(
        !file.starts_with("../"),
        "active reply witness is local: {file}"
    );
    std::fs::read_to_string(&path).expect("fixture reads")
}

#[test]
fn report_foreground_known_none_round_trips() {
    let text = fixture_text("messages/report-foreground-known-none.json");
    let decoded = decode_report_v4(&text).expect("known none decodes");
    assert!(decoded.report.foreground_known);
    assert!(decoded.report.focused_global.is_empty());
    let encoded = encode_report_v4(&decoded).expect("known none encodes");
    assert_eq!(encoded, text, "known none is canonical");
    assert_eq!(decode_report_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn report_foreground_known_bound_round_trips() {
    let text = fixture_text("messages/report-foreground-known-bound.json");
    let decoded = decode_report_v4(&text).expect("bound known decodes");
    assert!(decoded.report.foreground_known);
    assert!(!decoded.report.focused_global.is_empty());
    let encoded = encode_report_v4(&decoded).expect("bound known encodes");
    assert_eq!(encoded, text, "bound known is canonical");
    assert_eq!(decode_report_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn report_foreground_unknown_with_focus_refuses_both_directions() {
    let text = fixture_text("messages/invalid-foreground-unknown-mismatch.json");
    let error = decode_report_v4(&text).expect_err("false+nonempty refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focused-unknown-mismatch");
    let mut decoded =
        decode_report_v4(&fixture_text("messages/report-foreground-known-bound.json"))
            .expect("valid decodes");
    decoded.report.foreground_known = false;
    let error = encode_report_v4(&decoded).expect_err("encode refuses mismatch");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focused-unknown-mismatch");
    assert_eq!(
        validate_report_v4_reason(&decoded.report),
        Some("focused-unknown-mismatch")
    );
}

#[test]
fn reply_focus_request_null_round_trips() {
    let text = reply_text("messages/reply-bootstrap.json");
    assert!(
        text.contains("\"focusRequest\":null"),
        "bootstrap carries null lane"
    );
    assert!(!text.contains("closeFocus"), "old lane is gone");
    let decoded = decode_reply_v4(&text).expect("null lane decodes");
    assert!(decoded.reply.focus_request.is_none());
    let encoded = encode_reply_v4(&decoded).expect("null lane encodes");
    assert_eq!(encoded, text, "null lane is canonical");
    assert_eq!(decode_reply_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn reply_focus_request_pending_object_round_trips() {
    let text = reply_text("messages/reply-close-focus-pending.json");
    let decoded = decode_reply_v4(&text).expect("pending object decodes");
    let request = decoded.reply.focus_request.as_ref().expect("lane present");
    assert_eq!(
        request,
        &FocusRequest {
            id: "close-focus-42".to_owned(),
            target: WindowId("C".to_owned()),
            issued_revision: 42,
            cause: FocusRequestCause::PositiveClose,
            output: OutputId("L".to_owned()),
            workspace: "W1".to_owned(),
            cmd_seq: None,
            after_shown: None,
        }
    );
    let encoded = encode_reply_v4(&decoded).expect("pending encodes");
    assert_eq!(encoded, text, "pending object is canonical");
    assert_eq!(decode_reply_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn reply_focus_request_all_causes_round_trip_with_correlation() {
    // Every closed cause spells its command correlation: only
    // explicit-command carries a sequence, and only it may carry frozen
    // dependencies.
    let base =
        decode_reply_v4(&reply_text("messages/reply-bootstrap.json")).expect("bootstrap decodes");
    let cases: Vec<(FocusRequestCause, Option<u64>, Option<AfterShown>)> = vec![
        (FocusRequestCause::ExplicitCommand, Some(7), None),
        (FocusRequestCause::AutomaticAdmission, None, None),
        (FocusRequestCause::FocusFollowsCursor, None, None),
        (FocusRequestCause::PositiveClose, None, None),
    ];
    for (cause, cmd_seq, after_shown) in cases {
        let mut reply = base.clone();
        reply.reply.focus_request = Some(FocusRequest {
            id: "focus-7".to_owned(),
            target: WindowId("B".to_owned()),
            issued_revision: 9,
            cause,
            output: OutputId("L".to_owned()),
            workspace: "W1".to_owned(),
            cmd_seq,
            after_shown,
        });
        let wire = encode_reply_v4(&reply).expect("cause encodes");
        let decoded = decode_reply_v4(&wire).expect("cause decodes");
        assert_eq!(decoded, reply, "cause {cause:?} round-trips");
        assert_eq!(
            reply_exact_bytes_v4(&reply.reply),
            wire.len(),
            "exact matches {cause:?} bytes"
        );
        assert!(
            reply_estimate_v4(&reply.reply) >= wire.len(),
            "estimate covers {cause:?} bytes"
        );
    }
    // The selection-backed explicit witness carries a non-null projection.
    let explicit = decode_reply_v4(&reply_text("messages/reply-effect-focus.json"))
        .expect("explicit witness decodes");
    let request = explicit.reply.focus_request.as_ref().expect("lane present");
    assert_eq!(request.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(request.cmd_seq, Some(1));
    let shown = request.after_shown.as_ref().expect("explicit dependencies");
    assert_eq!(shown.op, "op-1");
    assert_eq!(shown.obs, 1);
    assert_eq!(shown.members.len(), 1);
    assert_eq!(shown.views.len(), 1);
    assert_eq!(shown.selected.len(), 1);
    assert!(
        explicit.reply.effects.is_empty(),
        "focus left the effects lane"
    );
    let wire = encode_reply_v4(&explicit).expect("explicit witness encodes");
    assert_eq!(
        wire,
        reply_text("messages/reply-effect-focus.json"),
        "explicit witness is canonical"
    );
}

#[test]
fn reply_focus_request_unknown_cause_and_missing_key_refuse() {
    let text = reply_text("messages/invalid-close-focus-unknown-cause.json");
    let error = decode_reply_v4(&text).expect_err("unknown cause refuses");
    assert_eq!(error.kind, "unknown-value");
    assert_eq!(error.message, "request contains an unknown value");
    let text = reply_text("messages/invalid-close-focus-missing-lane.json");
    let error = decode_reply_v4(&text).expect_err("missing lane refuses");
    assert_eq!(error.kind, "malformed");
    assert_eq!(error.message, "request is malformed");
}

#[test]
fn reply_mixed_old_lane_and_focus_effect_refuse() {
    // The old `closeFocus` key is unknown on the new wire: mixed old/new
    // focus authority refuses without mutating.
    let mixed = reply_text("messages/reply-bootstrap.json").replacen(
        "\"focusRequest\":null",
        "\"focusRequest\":null,\"closeFocus\":null",
        1,
    );
    assert_ne!(
        mixed,
        reply_text("messages/reply-bootstrap.json"),
        "stages the old lane"
    );
    let error = decode_reply_v4(&mixed).expect_err("old lane refuses");
    assert_eq!(error.kind, "unknown-field");
    // A renamed old pending object also refuses as an unknown field, never
    // as a cross-field mismatch.
    let pending = reply_text("messages/reply-close-focus-pending.json").replacen(
        "\"focusRequest\":",
        "\"closeFocus\":",
        1,
    );
    let error = decode_reply_v4(&pending).expect_err("renamed lane refuses");
    assert_eq!(error.kind, "unknown-field");
    // Focus left the effects lane: a focus row refuses on decode even when
    // the retained lane is null.
    let mut effect_text: serde_json::Value =
        serde_json::from_str(&reply_text("messages/reply-bootstrap.json")).expect("valid");
    effect_text["effects"] = serde_json::json!([{
        "id": "effect-a", "kind": "focus", "target": "win-a", "rev": "1",
        "cause": "explicit-command", "cmdSeq": "1", "awaitShown": false,
    }]);
    let error = decode_reply_v4(&effect_text.to_string()).expect_err("focus effect refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "effect-focus-removed");
    // The writer refuses a focus carrier the same way instead of emitting
    // it: the core strips focus before publish.
    let mut decoded =
        decode_reply_v4(&reply_text("messages/reply-bootstrap.json")).expect("decodes");
    decoded.reply.effects = vec![OneShotEffect {
        id: "effect-a".to_owned(),
        kind: EffectKind::Focus,
        target: WindowId("win-a".to_owned()),
        issued_revision: 1,
        cause: EffectCause::ExplicitCommand,
        command_seq: Some(1),
        await_shown: false,
    }];
    let error = encode_reply_v4(&decoded).expect_err("focus effect never encodes");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "effect-focus-removed");
}

#[test]
fn focus_request_cmdseq_and_aftershown_correlation_refuse_both_directions() {
    fn request(
        cause: FocusRequestCause,
        cmd_seq: Option<u64>,
        after_shown: Option<AfterShown>,
    ) -> FocusRequest {
        FocusRequest {
            id: "focus-1".to_owned(),
            target: WindowId("B".to_owned()),
            issued_revision: 3,
            cause,
            output: OutputId("L".to_owned()),
            workspace: "W1".to_owned(),
            cmd_seq,
            after_shown,
        }
    }
    fn shown() -> AfterShown {
        AfterShown {
            op: "op-1".to_owned(),
            obs: 1,
            members: Vec::new(),
            views: Vec::new(),
            selected: Vec::new(),
            departing: Vec::new(),
        }
    }
    let base =
        decode_reply_v4(&reply_text("messages/reply-bootstrap.json")).expect("bootstrap decodes");
    // Explicit without a sequence, or any other cause with one, refuses.
    for (cause, cmd_seq, message) in [
        (
            FocusRequestCause::ExplicitCommand,
            None,
            "explicit requires cmdSeq",
        ),
        (
            FocusRequestCause::AutomaticAdmission,
            Some(4),
            "admission carries no cmdSeq",
        ),
        (
            FocusRequestCause::FocusFollowsCursor,
            Some(4),
            "hover carries no cmdSeq",
        ),
        (
            FocusRequestCause::PositiveClose,
            Some(4),
            "close carries no cmdSeq",
        ),
    ] {
        let mut reply = base.clone();
        reply.reply.focus_request = Some(request(cause, cmd_seq, None));
        let error = encode_reply_v4(&reply).expect_err(message);
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "focus-request-cmdseq-mismatch");
        // Same rule on decode: patch the wire of a valid sibling.
        let mut value: serde_json::Value =
            serde_json::from_str(&reply_text("messages/reply-bootstrap.json")).expect("valid");
        let (cause_token, seq) = match cause {
            FocusRequestCause::ExplicitCommand => ("explicit-command", serde_json::Value::Null),
            FocusRequestCause::AutomaticAdmission => {
                ("automatic-admission", serde_json::json!("4"))
            }
            FocusRequestCause::FocusFollowsCursor => {
                ("focus-follows-cursor", serde_json::json!("4"))
            }
            FocusRequestCause::PositiveClose => ("positive-close", serde_json::json!("4")),
        };
        value["focusRequest"] = serde_json::json!({
            "id": "focus-1", "target": "B", "rev": "3", "cause": cause_token,
            "out": "L", "ws": "W1", "cmdSeq": seq, "afterShown": null,
        });
        let error = decode_reply_v4(&value.to_string()).expect_err(message);
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "focus-request-cmdseq-mismatch");
    }
    // Frozen dependencies require explicit-command on both sides.
    for cause in [
        FocusRequestCause::AutomaticAdmission,
        FocusRequestCause::FocusFollowsCursor,
        FocusRequestCause::PositiveClose,
    ] {
        let mut reply = base.clone();
        reply.reply.focus_request = Some(request(cause, None, Some(shown())));
        let error = encode_reply_v4(&reply).expect_err("afterShown needs explicit");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "focus-request-aftershown-non-explicit");
        let mut value: serde_json::Value =
            serde_json::from_str(&reply_text("messages/reply-bootstrap.json")).expect("valid");
        value["focusRequest"] = serde_json::json!({
            "id": "focus-1", "target": "B", "rev": "3", "cause": cause.as_str(),
            "out": "L", "ws": "W1", "cmdSeq": null,
            "afterShown": {"op": "op-1", "obs": "1", "members": [], "views": [], "selected": [], "departing": []},
        });
        let error = decode_reply_v4(&value.to_string()).expect_err("afterShown needs explicit");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, "focus-request-aftershown-non-explicit");
    }
    // Explicit with a null projection stays valid (ordinary explicit focus).
    let mut reply = base.clone();
    reply.reply.focus_request = Some(request(FocusRequestCause::ExplicitCommand, Some(7), None));
    let wire = encode_reply_v4(&reply).expect("ordinary explicit encodes");
    assert_eq!(
        decode_reply_v4(&wire).expect("ordinary explicit decodes"),
        reply
    );
}

#[test]
fn focus_request_aftershown_shape_uniqueness_and_bounds() {
    fn valid_value() -> serde_json::Value {
        serde_json::from_str(&reply_text("messages/reply-effect-focus.json")).expect("valid")
    }
    // Required keys: every object key participates; only `afterShown`
    // itself and member `frame` may be null.
    for key in ["op", "obs", "members", "views", "selected", "departing"] {
        let mut value = valid_value();
        value["focusRequest"]["afterShown"]
            .as_object_mut()
            .expect("object")
            .remove(key);
        let error = decode_reply_v4(&value.to_string()).expect_err("required key");
        assert_eq!(error.kind, "malformed", "{key} is required");
    }
    for key in [
        "id",
        "target",
        "rev",
        "cause",
        "out",
        "ws",
        "cmdSeq",
        "afterShown",
    ] {
        let mut value = valid_value();
        value["focusRequest"]
            .as_object_mut()
            .expect("object")
            .remove(key);
        let error = decode_reply_v4(&value.to_string()).expect_err("required key");
        assert_eq!(error.kind, "malformed", "{key} is required");
    }
    // Unknown keys refuse on every object level.
    let mut value = valid_value();
    value["focusRequest"]["extra"] = serde_json::Value::Bool(true);
    assert_eq!(
        decode_reply_v4(&value.to_string())
            .expect_err("unknown request key")
            .kind,
        "unknown-field"
    );
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["extra"] = serde_json::Value::Bool(true);
    assert_eq!(
        decode_reply_v4(&value.to_string())
            .expect_err("unknown shown key")
            .kind,
        "unknown-field"
    );
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["members"][0]["extra"] = serde_json::Value::Bool(true);
    assert_eq!(
        decode_reply_v4(&value.to_string())
            .expect_err("unknown member key")
            .kind,
        "unknown-field"
    );
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["views"][0]["extra"] = serde_json::Value::Bool(true);
    assert_eq!(
        decode_reply_v4(&value.to_string())
            .expect_err("unknown scope key")
            .kind,
        "unknown-field"
    );
    // Duplicate member lifetimes refuse; duplicate scope pairs refuse on
    // every scope lane; duplicate view outputs refuse even for distinct
    // workspaces.
    let mut value = valid_value();
    let member = value["focusRequest"]["afterShown"]["members"][0].clone();
    value["focusRequest"]["afterShown"]["members"]
        .as_array_mut()
        .expect("array")
        .push(member);
    let error = decode_reply_v4(&value.to_string()).expect_err("duplicate member refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-duplicate-member");
    let mut value = valid_value();
    let scope = value["focusRequest"]["afterShown"]["selected"][0].clone();
    value["focusRequest"]["afterShown"]["selected"]
        .as_array_mut()
        .expect("array")
        .push(scope);
    let error = decode_reply_v4(&value.to_string()).expect_err("duplicate scope refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-duplicate-scope");
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["views"] = serde_json::json!([
        {"out": "out-a", "ws": "ws-a"},
        {"out": "out-a", "ws": "ws-other"},
    ]);
    value["focusRequest"]["afterShown"]["selected"] = serde_json::json!([
        {"out": "out-a", "ws": "ws-a"},
        {"out": "out-a", "ws": "ws-other"},
    ]);
    let error = decode_reply_v4(&value.to_string()).expect_err("duplicate view out refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-duplicate-view-output");
    // Every view target must occur in `selected`.
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["selected"] = serde_json::json!([]);
    let error = decode_reply_v4(&value.to_string()).expect_err("view needs selected");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-view-not-selected");
    // Null member frames waive equality and stay valid; empty arrays stay
    // valid for ordinary explicit focus.
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["members"][0]["frame"] = serde_json::Value::Null;
    let decoded = decode_reply_v4(&value.to_string()).expect("null frame decodes");
    assert!(
        decoded
            .reply
            .focus_request
            .expect("lane")
            .after_shown
            .expect("shown")
            .members[0]
            .frame
            .is_none()
    );
    let mut value = valid_value();
    value["focusRequest"]["afterShown"]["members"] = serde_json::json!([]);
    value["focusRequest"]["afterShown"]["views"] = serde_json::json!([]);
    value["focusRequest"]["afterShown"]["selected"] = serde_json::json!([]);
    value["focusRequest"]["afterShown"]["departing"] = serde_json::json!([]);
    let decoded = decode_reply_v4(&value.to_string()).expect("empty projection decodes");
    let wire = encode_reply_v4(&decoded).expect("empty projection encodes");
    assert_eq!(decode_reply_v4(&wire).expect("re-decode"), decoded);
    // Duplicate JSON keys refuse as malformed even with otherwise valid
    // content.
    let text = reply_text("messages/reply-effect-focus.json").replacen(
        "\"cmdSeq\":\"1\"",
        "\"cmdSeq\":\"1\",\"cmdSeq\":\"1\"",
        1,
    );
    let error = decode_reply_v4(&text).expect_err("duplicate key refuses");
    assert_eq!(error.kind, "malformed");
    // Section caps refuse before mutation on both sides: 17 scope rows
    // overflow the 16-output cap; the writer mirrors the decode.
    for (lane, token) in [
        ("views", "focus-views-over-cap"),
        ("selected", "focus-selected-over-cap"),
        ("departing", "focus-departing-over-cap"),
    ] {
        let mut value = valid_value();
        let rows: Vec<serde_json::Value> = (0..17)
            .map(|i| serde_json::json!({"out": format!("out-{i}"), "ws": format!("ws-{i}")}))
            .collect();
        value["focusRequest"]["afterShown"][lane] = serde_json::Value::Array(rows);
        if lane == "views" {
            value["focusRequest"]["afterShown"]["selected"] =
                value["focusRequest"]["afterShown"][lane].clone();
        }
        let error = decode_reply_v4(&value.to_string()).expect_err("over cap refuses");
        assert_eq!(error.kind, "snapshot-invalid");
        assert_eq!(error.message, token);
    }
    let mut decoded =
        decode_reply_v4(&reply_text("messages/reply-effect-focus.json")).expect("decodes");
    let shown = decoded
        .reply
        .focus_request
        .as_mut()
        .expect("lane")
        .after_shown
        .as_mut()
        .expect("shown");
    shown.views = (0..17)
        .map(|i| AfterShownScope {
            out: OutputId(format!("out-{i}")),
            ws: format!("ws-{i}"),
        })
        .collect();
    shown.selected = shown.views.clone();
    let error = encode_reply_v4(&decoded).expect_err("writer caps views");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-views-over-cap");
}

#[test]
fn core_digest_and_sizing_cover_new_lanes() {
    // The digest isolates the provenance bit: the same empty focused string
    // digests differently with known true versus false.
    let known = decode_report_v4(&fixture_text("messages/report-foreground-known-none.json"))
        .expect("known none decodes");
    assert!(known.report.focused_global.is_empty() && known.report.foreground_known);
    let mut unknown = known.report.clone();
    unknown.foreground_known = false;
    assert_eq!(unknown.focused_global, known.report.focused_global);
    assert_ne!(
        report_digest_v4(&known.report),
        report_digest_v4(&unknown),
        "provenance feeds the digest"
    );
    assert!(
        report_estimate_v4(&known.report) > 0,
        "report estimate covers the bit"
    );

    // Exact byte length pins the encoded bytes for the null lane and for a
    // pending lane with longest valid opaque fields at max revision; the
    // structural estimate covers both.
    let null_reply =
        decode_reply_v4(&reply_text("messages/reply-bootstrap.json")).expect("reply decodes");
    let null_wire = encode_reply_v4(&null_reply).expect("null encodes");
    assert_eq!(
        reply_exact_bytes_v4(&null_reply.reply),
        null_wire.len(),
        "exact matches null-lane bytes"
    );
    assert!(
        reply_estimate_v4(&null_reply.reply) >= null_wire.len(),
        "estimate covers null-lane bytes"
    );
    let mut wide = null_reply.clone();
    let long = "w".repeat(128);
    wide.reply.focus_request = Some(FocusRequest {
        id: long.clone(),
        target: WindowId(long.clone()),
        issued_revision: u64::MAX,
        cause: FocusRequestCause::PositiveClose,
        output: OutputId(long.clone()),
        workspace: long.clone(),
        cmd_seq: None,
        after_shown: None,
    });
    let wide_wire = encode_reply_v4(&wide).expect("max-width pending encodes");
    assert_eq!(
        reply_exact_bytes_v4(&wide.reply),
        wide_wire.len(),
        "exact matches max-width pending bytes"
    );
    assert!(
        reply_estimate_v4(&wide.reply) >= wide_wire.len(),
        "estimate covers max-width pending bytes"
    );
    // An explicit request with frozen dependencies is exact too: one member
    // with a null frame plus one view/selected scope and no departing
    // scopes.
    let mut projected = null_reply.clone();
    projected.reply.focus_request = Some(FocusRequest {
        id: "focus-1".to_owned(),
        target: WindowId("B".to_owned()),
        issued_revision: 3,
        cause: FocusRequestCause::ExplicitCommand,
        output: OutputId("L".to_owned()),
        workspace: "W1".to_owned(),
        cmd_seq: Some(7),
        after_shown: Some(AfterShown {
            op: "op-1".to_owned(),
            obs: 1,
            members: vec![AfterShownMember {
                win: WindowId("win-a".to_owned()),
                out: OutputId("out-a".to_owned()),
                ws: "ws-a".to_owned(),
                frame: None,
            }],
            views: vec![AfterShownScope {
                out: OutputId("out-a".to_owned()),
                ws: "ws-a".to_owned(),
            }],
            selected: vec![AfterShownScope {
                out: OutputId("out-a".to_owned()),
                ws: "ws-a".to_owned(),
            }],
            departing: Vec::new(),
        }),
    });
    let projected_wire = encode_reply_v4(&projected).expect("projected encodes");
    assert_eq!(
        reply_exact_bytes_v4(&projected.reply),
        projected_wire.len(),
        "exact matches projected bytes"
    );
    assert!(
        reply_estimate_v4(&projected.reply) >= projected_wire.len(),
        "estimate covers projected bytes"
    );

    // The pre-mutation upper bound over one staged report covers every
    // staged reply shape, so the new lane needs no bound repair.
    let report = decode_report_v4(&fixture_text("messages/report-foreground-known-bound.json"))
        .expect("report decodes")
        .report;
    let bound = reply_upper_bound_v4(&report, 0, 0, 0, 0, 0);
    assert!(
        bound >= reply_exact_bytes_v4(&null_reply.reply),
        "upper bound covers null-lane reply"
    );
    assert!(
        bound >= reply_exact_bytes_v4(&wide.reply),
        "upper bound covers max-width pending reply"
    );
    assert!(
        bound >= reply_exact_bytes_v4(&projected.reply),
        "upper bound covers projected reply"
    );
}

#[test]
fn focus_request_aftershown_member_cap_refuses_both_directions() {
    // Members mirror the scope lanes: 10_000 admits, 10_001 refuses on
    // decode and on encode with the same fixed token.
    fn valid_value() -> serde_json::Value {
        serde_json::from_str(&reply_text("messages/reply-effect-focus.json")).expect("valid")
    }
    let mut value = valid_value();
    let rows: Vec<serde_json::Value> = (0..10_001)
        .map(|i| serde_json::json!({"win": format!("w-{i}"), "out": "o", "ws": "s", "frame": null}))
        .collect();
    value["focusRequest"]["afterShown"]["members"] = serde_json::Value::Array(rows);
    let error = decode_reply_v4(&value.to_string()).expect_err("members over cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-members-over-cap");
    let mut decoded =
        decode_reply_v4(&reply_text("messages/reply-effect-focus.json")).expect("decodes");
    let shown = decoded
        .reply
        .focus_request
        .as_mut()
        .expect("lane")
        .after_shown
        .as_mut()
        .expect("shown");
    shown.members = (0..10_001)
        .map(|i| AfterShownMember {
            win: WindowId(format!("w-{i}")),
            out: OutputId("o".to_owned()),
            ws: "s".to_owned(),
            frame: None,
        })
        .collect();
    let error = encode_reply_v4(&decoded).expect_err("writer caps members");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "focus-members-over-cap");
}

#[test]
fn focus_request_aftershown_at_cap_exact_and_oversize_contrast() {
    // At-cap scope lanes stay valid with the exact/estimate/upper contrast;
    // a within-cap max-width member projection exceeds the 4 MiB transport
    // budget and refuses on encode even though every section cap passes.
    let base = decode_reply_v4(&reply_text("messages/reply-bootstrap.json")).expect("decodes");
    let report = decode_report_v4(&fixture_text("messages/report-foreground-known-bound.json"))
        .expect("report decodes")
        .report;
    let scopes: Vec<AfterShownScope> = (0..16)
        .map(|i| AfterShownScope {
            out: OutputId(format!("out-{i}")),
            ws: format!("ws-{i}"),
        })
        .collect();
    let mut at_cap = base.clone();
    at_cap.reply.focus_request = Some(FocusRequest {
        id: "focus-1".to_owned(),
        target: WindowId("B".to_owned()),
        issued_revision: 3,
        cause: FocusRequestCause::ExplicitCommand,
        output: OutputId("L".to_owned()),
        workspace: "W1".to_owned(),
        cmd_seq: Some(7),
        after_shown: Some(AfterShown {
            op: "op-1".to_owned(),
            obs: 1,
            members: vec![AfterShownMember {
                win: WindowId("win-a".to_owned()),
                out: OutputId("out-a".to_owned()),
                ws: "ws-a".to_owned(),
                frame: None,
            }],
            views: scopes.clone(),
            selected: scopes.clone(),
            departing: scopes.clone(),
        }),
    });
    let wire = encode_reply_v4(&at_cap).expect("at-cap encodes");
    let decoded = decode_reply_v4(&wire).expect("at-cap decodes");
    assert_eq!(decoded, at_cap, "at-cap round-trips");
    assert_eq!(
        reply_exact_bytes_v4(&at_cap.reply),
        wire.len(),
        "exact matches at-cap bytes"
    );
    assert!(
        reply_estimate_v4(&at_cap.reply) >= wire.len(),
        "estimate covers at-cap bytes"
    );
    assert!(
        reply_upper_bound_v4(&report, 0, 0, 0, 0, 0) >= wire.len(),
        "upper bound covers at-cap bytes"
    );
    // Within-cap but over-bytes: 10_000 max-width members exceed 4 MiB.
    let long_out = "o".repeat(128);
    let long_ws = "s".repeat(128);
    let mut heavy = base.clone();
    heavy.reply.focus_request = Some(FocusRequest {
        id: "focus-1".to_owned(),
        target: WindowId("B".to_owned()),
        issued_revision: 3,
        cause: FocusRequestCause::ExplicitCommand,
        output: OutputId("L".to_owned()),
        workspace: "W1".to_owned(),
        cmd_seq: Some(7),
        after_shown: Some(AfterShown {
            op: "op-1".to_owned(),
            obs: 1,
            members: (0..10_000)
                .map(|i| AfterShownMember {
                    win: WindowId(format!("w{i:0127}")),
                    out: OutputId(long_out.clone()),
                    ws: long_ws.clone(),
                    frame: None,
                })
                .collect(),
            views: Vec::new(),
            selected: Vec::new(),
            departing: Vec::new(),
        }),
    });
    let exact = reply_exact_bytes_v4(&heavy.reply);
    let estimate = reply_estimate_v4(&heavy.reply);
    assert!(
        exact > WORLD_MAX_REPLY_BYTES,
        "max-width capped projection exceeds the byte budget (exact {exact})"
    );
    assert!(
        estimate >= exact,
        "estimate covers the heavy projection (estimate {estimate}, exact {exact})"
    );
    let error = encode_reply_v4(&heavy).expect_err("over-byte projection refuses");
    assert_eq!(error.kind, "snapshot-invalid");
}
