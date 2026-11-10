//! Focused wire conformance for the retained close-focus lane (P3/P4):
//! `foregroundKnown` provenance on reports and the always-present nullable
//! `closeFocus` lane on replies. Offline only.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_report::{CloseFocusCause, CloseFocusRequest};
use tiler_core::world_report_v4_checks::{report_digest_v4, validate_report_v4_reason};
use tiler_core::world_sizing_v4::{
    reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{
    decode_reply_v4, decode_report_v4, encode_reply_v4, encode_report_v4,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

fn fixture_text(file: &str) -> String {
    std::fs::read_to_string(fixture_root().join(file)).expect("fixture reads")
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
fn reply_close_focus_null_round_trips() {
    let text = fixture_text("messages/reply-bootstrap.json");
    assert!(
        text.contains("\"closeFocus\":null"),
        "bootstrap carries null lane"
    );
    let decoded = decode_reply_v4(&text).expect("null lane decodes");
    assert!(decoded.reply.close_focus.is_none());
    let encoded = encode_reply_v4(&decoded).expect("null lane encodes");
    assert_eq!(encoded, text, "null lane is canonical");
    assert_eq!(decode_reply_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn reply_close_focus_pending_object_round_trips() {
    let text = fixture_text("messages/reply-close-focus-pending.json");
    let decoded = decode_reply_v4(&text).expect("pending object decodes");
    let request = decoded.reply.close_focus.as_ref().expect("lane present");
    assert_eq!(
        request,
        &CloseFocusRequest {
            id: "close-focus-42".to_owned(),
            target: WindowId("C".to_owned()),
            issued_revision: 42,
            cause: CloseFocusCause::PositiveClose,
            output: OutputId("L".to_owned()),
            workspace: "W1".to_owned(),
        }
    );
    let encoded = encode_reply_v4(&decoded).expect("pending encodes");
    assert_eq!(encoded, text, "pending object is canonical");
    assert_eq!(decode_reply_v4(&encoded).expect("re-decode"), decoded);
}

#[test]
fn reply_close_focus_unknown_cause_and_missing_key_refuse() {
    let text = fixture_text("messages/invalid-close-focus-unknown-cause.json");
    let error = decode_reply_v4(&text).expect_err("unknown cause refuses");
    assert_eq!(error.kind, "unknown-value");
    assert_eq!(error.message, "request contains an unknown value");
    let text = fixture_text("messages/invalid-close-focus-missing-lane.json");
    let error = decode_reply_v4(&text).expect_err("missing lane refuses");
    assert_eq!(error.kind, "malformed");
    assert_eq!(error.message, "request is malformed");
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
        decode_reply_v4(&fixture_text("messages/reply-bootstrap.json")).expect("reply decodes");
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
    wide.reply.close_focus = Some(CloseFocusRequest {
        id: long.clone(),
        target: WindowId(long.clone()),
        issued_revision: u64::MAX,
        cause: CloseFocusCause::PositiveClose,
        output: OutputId(long.clone()),
        workspace: long.clone(),
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

    // The pre-mutation upper bound over one staged report covers both staged
    // replies, so the new lane needs no bound repair.
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
}
