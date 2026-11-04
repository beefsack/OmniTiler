//! Staged v4 report twins (unit 6): evidence for
//! [`validate_report_v4_reason`](tiler_core::world_report_v4_checks::validate_report_v4_reason),
//! [`grade_conflicting_windows_v4`](tiler_core::world_report_v4_checks::grade_conflicting_windows_v4),
//! [`report_digest_v4`](tiler_core::world_report_v4_checks::report_digest_v4),
//! and [`report_estimate_v4`](tiler_core::world_sizing_v4::report_estimate_v4)
//! over the `test-fixtures/world-wire/v4` WorldReport fixtures.
//! Offline only: no adapters, no live work, no engine.

use tiler_core::connection::FactName;
use tiler_core::directional::WindowId;
use tiler_core::world_report::{PresentationRowV4, RowDiagnosticKind, WorldReportV4};
use tiler_core::world_report_v4_checks::{
    grade_conflicting_windows_v4, report_digest_v4, validate_report_v4_reason,
};
use tiler_core::world_sizing_v4::report_estimate_v4;
use tiler_protocol::world_protocol::{decode_report_v4, encode_report_v4};

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

fn world_fixtures() -> Vec<ManifestFixture> {
    let root = fixture_root();
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("v4 manifest reads"),
    )
    .expect("v4 manifest parses");
    manifest
        .fixtures
        .into_iter()
        .filter(|fixture| fixture.family == "WorldReport")
        .collect()
}

fn fixture_text(file: &str) -> String {
    std::fs::read_to_string(fixture_root().join(file)).expect("fixture reads")
}

fn decode_ok(file: &str) -> WorldReportV4 {
    decode_report_v4(&fixture_text(file))
        .expect("fixture decodes")
        .report
}

fn accepted_reports() -> Vec<(String, WorldReportV4)> {
    world_fixtures()
        .into_iter()
        .filter(|fixture| {
            fixture.expectation == "valid-wire" || fixture.expectation == "accepted-held-row"
        })
        .map(|fixture| {
            let report = decode_ok(&fixture.file);
            (fixture.id, report)
        })
        .collect()
}

/// Every accepted wire report passes the v4 gate, digests stably, encodes
/// within the sizing bound, and grades to no change under the permissive
/// capability set.
#[test]
fn v4_twins_accept_all_wire_reports() {
    for fixture in world_fixtures() {
        if fixture.expectation != "valid-wire" && fixture.expectation != "accepted-held-row" {
            continue;
        }
        let decoded = decode_report_v4(&fixture_text(&fixture.file)).expect("fixture decodes");
        assert_eq!(
            validate_report_v4_reason(&decoded.report),
            None,
            "{}",
            fixture.id
        );
        assert_eq!(
            report_digest_v4(&decoded.report),
            report_digest_v4(&decoded.report),
            "digest stable {}",
            fixture.id
        );
        let wire = encode_report_v4(&decoded).expect("fixture encodes");
        assert!(
            report_estimate_v4(&decoded.report) >= wire.len(),
            "estimate covers wire {} ({} vs {})",
            fixture.id,
            report_estimate_v4(&decoded.report),
            wire.len()
        );
        assert!(
            grade_conflicting_windows_v4(&decoded.report, &[], true, true).is_none(),
            "permissive grade keeps {}",
            fixture.id
        );
    }
}

fn parse_caps(file: &str) -> (Vec<FactName>, bool, bool) {
    let value: serde_json::Value =
        serde_json::from_str(&fixture_text(file)).expect("handshake parses");
    let caps = &value["capabilities"];
    let unavailable = caps["unavailableFacts"]
        .as_array()
        .expect("unavailableFacts array")
        .iter()
        .map(|name| {
            FactName::parse(name.as_str().expect("fact name string")).expect("known fact name")
        })
        .collect();
    (
        unavailable,
        caps["nativeSticky"].as_bool().expect("nativeSticky bool"),
        caps["maximize"].as_bool().expect("maximize bool"),
    )
}

/// The three handshake-context conflicts grade exactly one window row into
/// `held_windows` with a scoped `ConflictingFact` diagnostic; the graded
/// report validates clean.
#[test]
fn v4_grade_moves_capability_conflicts_to_held() {
    let cases = [
        (
            "messages/invalid-unavailable-fact-contradiction.json",
            "messages/handshake-unavailable-facts.json",
        ),
        (
            "messages/invalid-executable-unavailable-value.json",
            "messages/handshake-unavailable-facts.json",
        ),
        (
            "messages/invalid-sticky-projected-fact.json",
            "messages/handshake-sticky-native-free.json",
        ),
    ];
    for (report_file, handshake_file) in cases {
        let report = decode_ok(report_file);
        assert!(report.held_windows.is_empty(), "{report_file}");
        let (unavailable, native_sticky, maximize) = parse_caps(handshake_file);
        let graded = grade_conflicting_windows_v4(&report, &unavailable, native_sticky, maximize)
            .expect("conflict grades");
        assert_eq!(graded.held_windows.len(), 1, "{report_file}");
        let held = &graded.held_windows[0];
        assert_eq!(
            held.kind,
            RowDiagnosticKind::ConflictingFact,
            "{report_file}"
        );
        let diagnostic = held.diagnostic.as_ref().expect("scoped diagnostic");
        assert_eq!(
            diagnostic.kind,
            RowDiagnosticKind::ConflictingFact,
            "{report_file}"
        );
        assert_eq!(
            diagnostic.id.as_deref(),
            Some(held.window.0.as_str()),
            "{report_file}"
        );
        assert_eq!(
            graded.windows.len() + 1,
            report.windows.len(),
            "{report_file}"
        );
        assert_eq!(validate_report_v4_reason(&graded), None, "{report_file}");
    }
}

/// Mutating a decoded report trips each v4 validation family with its fixed
/// token.
#[test]
fn v4_validate_rejects_mutated_reports() {
    let mut version = decode_ok("messages/report-observe.json");
    version.contract_version = 3;
    assert_eq!(
        validate_report_v4_reason(&version),
        Some("version-mismatch")
    );

    let mut over_cap = decode_ok("messages/report-observe.json");
    over_cap
        .closed
        .extend((0..10_001).map(|i| WindowId(format!("win-over-cap-{i}"))));
    assert_eq!(
        validate_report_v4_reason(&over_cap),
        Some("section-over-cap")
    );

    let mut unpaired = decode_ok("messages/report-renderer-window.json");
    unpaired.command = None;
    unpaired.command_seq = None;
    assert_eq!(
        validate_report_v4_reason(&unpaired),
        Some("command-pair-invalid")
    );

    let mut bad_ordinal = decode_ok("messages/invalid-frame-missing-op.json");
    assert_eq!(bad_ordinal.held_attempts.len(), 1);
    bad_ordinal.held_attempts[0].ordinal = usize::MAX;
    assert_eq!(
        validate_report_v4_reason(&bad_ordinal),
        Some("held-ordinal-invalid")
    );

    let (_, mut pager) = accepted_reports()
        .into_iter()
        .find(|(_, report)| matches!(report.presentation, PresentationRowV4::Valid(_)))
        .expect("valid presentation fixture");
    let PresentationRowV4::Valid(ref mut ingress) = pager.presentation else {
        unreachable!();
    };
    ingress
        .pager_order
        .extend((0..1025).map(|i| WindowId(format!("win-pager-{i}"))));
    assert_eq!(
        validate_report_v4_reason(&pager),
        Some("presentation-order-over-cap")
    );

    let mut dup_native = decode_ok("messages/invalid-native-order-known.json");
    assert_eq!(dup_native.held_native.len(), 1);
    dup_native
        .held_native
        .push(dup_native.held_native[0].clone());
    assert_eq!(
        validate_report_v4_reason(&dup_native),
        Some("duplicate-native")
    );
}

/// Flipping one field in each lane family changes the digest.
#[test]
fn v4_digest_sees_each_lane_family() {
    let reports = accepted_reports();
    fn find_with(
        reports: &[(String, WorldReportV4)],
        pick: impl Fn(&WorldReportV4) -> bool,
    ) -> WorldReportV4 {
        reports
            .iter()
            .find(|entry| pick(&entry.1))
            .expect("lane fixture")
            .1
            .clone()
    }
    let find = |pick: &dyn Fn(&WorldReportV4) -> bool| find_with(&reports, pick);

    let mut screen = find(&|report| !report.screens.is_empty());
    let before = report_digest_v4(&screen);
    screen.screens[0].scale += 1.0;
    assert_ne!(report_digest_v4(&screen), before, "screen");

    let mut window = find(&|report| !report.windows.is_empty());
    let before = report_digest_v4(&window);
    let minimized = window.windows[0].minimized.unwrap_or(false);
    window.windows[0].minimized = Some(!minimized);
    assert_ne!(report_digest_v4(&window), before, "window");

    let mut native = find(&|report| !report.native.is_empty());
    let before = report_digest_v4(&native);
    if native.native[0].current_native.is_some() {
        native.native[0].current_native = None;
    } else {
        native.native[0].current_native = Some("native-digest-probe".to_owned());
    }
    assert_ne!(report_digest_v4(&native), before, "native");

    let mut held_native = find(&|report| !report.held_native.is_empty());
    let before = report_digest_v4(&held_native);
    held_native.held_native[0].ordinal = held_native.held_native[0].ordinal.wrapping_add(1);
    assert_ne!(report_digest_v4(&held_native), before, "held_native");

    let mut attempt = find(&|report| !report.frames.is_empty());
    let before = report_digest_v4(&attempt);
    attempt.frames[0].attempt_ms += 1;
    assert_ne!(report_digest_v4(&attempt), before, "attempt");

    let mut held_attempt = find(&|report| !report.held_attempts.is_empty());
    let before = report_digest_v4(&held_attempt);
    held_attempt.held_attempts[0].ordinal = held_attempt.held_attempts[0].ordinal.wrapping_add(1);
    assert_ne!(report_digest_v4(&held_attempt), before, "held_attempts");

    let mut presentation =
        find(&|report| matches!(report.presentation, PresentationRowV4::Valid(_)));
    let before = report_digest_v4(&presentation);
    let PresentationRowV4::Valid(ref mut ingress) = presentation.presentation else {
        unreachable!();
    };
    ingress.chord_held = !ingress.chord_held;
    assert_ne!(report_digest_v4(&presentation), before, "presentation");

    let mut renderer = find(&|report| report.renderer_input.is_some());
    let before = report_digest_v4(&renderer);
    renderer.renderer_input.as_mut().expect("renderer").rev += 1;
    assert_ne!(report_digest_v4(&renderer), before, "renderer_input");
}
