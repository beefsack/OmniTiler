//! Staged v4 reply twins (unit 6): evidence for the canonical writer byte
//! count (`count_reply_v4_bytes` over decoded wire optionals)
//! and [`reply_upper_bound_v4`](tiler_protocol::world_admission_sizing::reply_upper_bound_v4)
//! over the `test-fixtures/world-wire/v4-active` WorldReply fixtures. Offline only:
//! no adapters, no live work, no engine.

use tiler_core::world_report::{BorderPresentationV4, TransferKind, TransferTargetV4};
use tiler_protocol::world_admission_sizing::{
    MAX_EFFECT_RESERVE_BYTES, MAX_EFFECT_ROW_BYTES, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{decode_reply_v4, decode_report_v4, encode_reply_v4};
use tiler_protocol::world_reply_write::count_reply_v4_bytes;

fn reply_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active")
}

fn report_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active")
}

#[derive(serde::Deserialize)]
struct Manifest {
    fixtures: Vec<ManifestFixture>,
}

#[derive(serde::Deserialize)]
struct ManifestFixture {
    id: String,
    file: String,
    family: String,
    expectation: String,
}

fn reply_text(file: &str) -> String {
    let path = reply_root().join(file);
    // Active reply witnesses live locally; the manifest never points them
    // back at frozen history.
    assert!(
        !file.starts_with("../"),
        "active reply witness is local: {file}"
    );
    std::fs::read_to_string(&path).expect("fixture reads")
}

fn report_text(file: &str) -> String {
    std::fs::read_to_string(report_root().join(file)).expect("fixture reads")
}

fn reply_fixtures() -> Vec<ManifestFixture> {
    let root = reply_root();
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("v4 manifest reads"),
    )
    .expect("v4 manifest parses");
    manifest
        .fixtures
        .into_iter()
        .filter(|fixture| fixture.family == "WorldReply" && fixture.expectation == "valid-wire")
        .collect()
}

fn report_fixtures() -> Vec<ManifestFixture> {
    let root = report_root();
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("v4 manifest reads"),
    )
    .expect("v4 manifest parses");
    manifest
        .fixtures
        .into_iter()
        .filter(|fixture| {
            fixture.family == "WorldReport"
                && (fixture.expectation == "valid-wire"
                    || fixture.expectation == "accepted-held-row")
        })
        .collect()
}

/// Every valid-wire reply fixture: the canonical writer byte count matches
/// the emitted bytes exactly.
#[test]
fn v4_reply_exact_matches_canonical_encoding() {
    let fixtures = reply_fixtures();
    assert!(!fixtures.is_empty(), "reply fixtures present");
    for fixture in fixtures {
        let decoded = decode_reply_v4(&reply_text(&fixture.file)).expect("fixture decodes");
        let wire = encode_reply_v4(&decoded).expect("fixture encodes");
        assert_eq!(
            count_reply_v4_bytes(&decoded.reply, &decoded.wire),
            wire.len(),
            "canonical count matches emitted bytes {}",
            fixture.id
        );
    }
}

/// Constructed extremes: transfers with `command_seq` at `u64::MAX` and
/// `None` across every transfer kind, plus a visible anchored border with
/// widest outsets, largest non-negative width/radius, a full witness, and
/// a warp point. The canonical count still matches the writer.
#[test]
fn v4_reply_twins_cover_extreme_transfers_and_border() {
    let fixture = reply_fixtures().into_iter().next().expect("reply fixture");
    let mut decoded = decode_reply_v4(&reply_text(&fixture.file)).expect("fixture decodes");
    let wide_id = "w".repeat(128);
    decoded.reply.transfers = vec![
        TransferTargetV4 {
            operation_id: "op-extreme-a".to_owned(),
            kind: TransferKind::Window(tiler_core::directional::WindowId(wide_id.clone())),
            target_output: tiler_core::directional::OutputId("out-a".to_owned()),
            target_workspace: "ws-a".to_owned(),
            follow: true,
            held: true,
            command_seq: Some(u64::MAX),
        },
        TransferTargetV4 {
            operation_id: "op-extreme-b".to_owned(),
            kind: TransferKind::View,
            target_output: tiler_core::directional::OutputId("out-a".to_owned()),
            target_workspace: "ws-a".to_owned(),
            follow: false,
            held: false,
            command_seq: None,
        },
        TransferTargetV4 {
            operation_id: "op-extreme-c".to_owned(),
            kind: TransferKind::Migration,
            target_output: tiler_core::directional::OutputId("out-a".to_owned()),
            target_workspace: "ws-a".to_owned(),
            follow: true,
            held: false,
            command_seq: Some(1),
        },
    ];
    decoded.reply.presentation.border = BorderPresentationV4 {
        visible: true,
        reason: None,
        subject: Some(tiler_core::directional::WindowId(wide_id.clone())),
        outsets: Some([i32::MIN, i32::MIN, i32::MIN, i32::MIN]),
        width: Some(i32::MAX),
        radius: Some(i32::MAX),
        witness: Some(tiler_core::world_report::FrameWitness {
            win: tiler_core::directional::WindowId(wide_id.clone()),
            obs: u64::MAX,
            frame: tiler_core::geometry::Rect {
                x: -16384,
                y: -16384,
                w: 16384,
                h: 16384,
            },
            foreground: tiler_core::directional::WindowId(wide_id),
        }),
    };
    decoded.reply.cursor_warp = Some((16384, -16384));
    let wire = encode_reply_v4(&decoded).expect("extreme reply encodes");
    assert_eq!(
        count_reply_v4_bytes(&decoded.reply, &decoded.wire),
        wire.len(),
        "canonical count matches extreme emitted bytes"
    );
}

/// Pre-mutation bound over every valid report: with zero counts the bound
/// covers the report estimate plus the count-independent reserves, and it
/// is monotone non-decreasing in each count argument.
#[test]
fn v4_reply_upper_bound_covers_report_estimate_plus_reserves() {
    let fixtures = report_fixtures();
    assert!(!fixtures.is_empty(), "report fixtures present");
    // Count-independent reserves: envelope slack, effect coexistence, the
    // float/drag result shapes, migration base, anchored border, revision
    // label, underlay base, and the gesture terminal.
    let fixed: usize = 4096 + MAX_EFFECT_RESERVE_BYTES + 2048 + 512 + 704 + 29 + 1024 + 96;
    for fixture in fixtures {
        let decoded = decode_report_v4(&report_text(&fixture.file)).expect("fixture decodes");
        let report = &decoded.report;
        let base = reply_upper_bound_v4(report, 0, 0, 0, 0, 0);
        assert!(
            base >= report_estimate_v4(report).saturating_add(fixed),
            "bound covers estimate plus fixed reserves {}",
            fixture.id
        );
        assert!(
            reply_upper_bound_v4(report, 3, 0, 0, 0, 0) >= base
                && reply_upper_bound_v4(report, 0, 3, 0, 0, 0) >= base
                && reply_upper_bound_v4(report, 0, 0, 3, 0, 0) >= base
                && reply_upper_bound_v4(report, 0, 0, 0, 3, 0) >= base
                && reply_upper_bound_v4(report, 0, 0, 0, 0, 3) >= base,
            "bound monotone in each count {}",
            fixture.id
        );
    }
}

#[test]
fn v4_reply_exact_covers_presentation_rev_marginals_and_effect_reserve_constants() {
    let fixture = reply_fixtures().into_iter().next().expect("reply fixture");
    let mut zero = decode_reply_v4(&reply_text(&fixture.file)).expect("fixture decodes");
    zero.reply.presentation.rev = 0;
    let zero_len = encode_reply_v4(&zero).expect("zero rev encodes").len();
    assert_eq!(count_reply_v4_bytes(&zero.reply, &zero.wire), zero_len);
    for (rev, expected_delta) in [(0u64, 0usize), (9, 0), (10, 1), (u64::MAX, 19)] {
        let mut rev_reply = zero.clone();
        rev_reply.reply.presentation.rev = rev;
        let wire = encode_reply_v4(&rev_reply).expect("rev reply encodes");
        assert_eq!(
            count_reply_v4_bytes(&rev_reply.reply, &rev_reply.wire),
            wire.len()
        );
        assert_eq!(wire.len() - zero_len, expected_delta, "rev {rev}");
    }

    assert_eq!(MAX_EFFECT_ROW_BYTES, 294);
    assert_eq!(MAX_EFFECT_RESERVE_BYTES, 589);
    assert_eq!(MAX_EFFECT_RESERVE_BYTES, 2 * MAX_EFFECT_ROW_BYTES + 1);
}

// Scenario files (`test-fixtures/world-wire/v4/scenarios`) are ordered
// driver scenarios (wire/fixture-event steps with assertion expressions),
// not report/reply JSON pairs, so no cheap per-step report-input to
// expected-reply byte comparison exists; the scenario leg is skipped
// rather than building a driver harness.
