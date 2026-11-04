//! Staged v4 reply twins (unit 6): evidence for
//! [`reply_estimate_v4`](tiler_core::world_sizing_v4::reply_estimate_v4),
//! [`reply_exact_bytes_v4`](tiler_core::world_sizing_v4::reply_exact_bytes_v4),
//! and [`reply_upper_bound_v4`](tiler_core::world_sizing_v4::reply_upper_bound_v4)
//! over the `test-fixtures/world-wire/v4` WorldReply fixtures. Offline only:
//! no adapters, no live work, no engine.

use tiler_core::world_report::{BorderPresentationV4, TransferKind, TransferTargetV4};
use tiler_core::world_sizing::MAX_EFFECT_RESERVE_BYTES;
use tiler_core::world_sizing_v4::{
    reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
};
use tiler_protocol::world_protocol::{decode_reply_v4, decode_report_v4, encode_reply_v4};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
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

fn fixture_text(file: &str) -> String {
    std::fs::read_to_string(fixture_root().join(file)).expect("fixture reads")
}

fn reply_fixtures() -> Vec<ManifestFixture> {
    let root = fixture_root();
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
    let root = fixture_root();
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

/// Every valid-wire reply fixture: the exact twin matches the canonical
/// writer byte-for-byte and the estimate covers it.
#[test]
fn v4_reply_exact_matches_canonical_encoding() {
    let fixtures = reply_fixtures();
    assert!(!fixtures.is_empty(), "reply fixtures present");
    for fixture in fixtures {
        let decoded = decode_reply_v4(&fixture_text(&fixture.file)).expect("fixture decodes");
        let wire = encode_reply_v4(&decoded).expect("fixture encodes");
        assert_eq!(
            reply_exact_bytes_v4(&decoded.reply),
            wire.len(),
            "exact matches canonical bytes {}",
            fixture.id
        );
        assert!(
            reply_estimate_v4(&decoded.reply) >= wire.len(),
            "estimate covers wire {} ({} vs {})",
            fixture.id,
            reply_estimate_v4(&decoded.reply),
            wire.len()
        );
    }
}

/// Constructed extremes: transfers with `command_seq` at `u64::MAX` and
/// `None` across every transfer kind, plus a visible anchored border with
/// widest outsets, largest non-negative width/radius, a full witness, and
/// a warp point. Exact still matches the writer; the estimate still covers.
#[test]
fn v4_reply_twins_cover_extreme_transfers_and_border() {
    let fixture = reply_fixtures().into_iter().next().expect("reply fixture");
    let mut decoded = decode_reply_v4(&fixture_text(&fixture.file)).expect("fixture decodes");
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
        reply_exact_bytes_v4(&decoded.reply),
        wire.len(),
        "exact matches extreme canonical bytes"
    );
    assert!(
        reply_estimate_v4(&decoded.reply) >= wire.len(),
        "estimate covers extreme wire"
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
        let decoded = decode_report_v4(&fixture_text(&fixture.file)).expect("fixture decodes");
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

// Scenario files (`test-fixtures/world-wire/v4/scenarios`) are ordered
// driver scenarios (wire/fixture-event steps with assertion expressions),
// not report/reply JSON pairs, so no cheap per-step report-input to
// expected-reply byte comparison exists; the scenario leg is skipped
// rather than building a driver harness.
