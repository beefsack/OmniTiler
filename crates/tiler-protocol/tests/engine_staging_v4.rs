//! v4 staging intake census (unit 6 sub-increment 2): stage every accepted
//! `WorldReport` fixture and every `to: "core"` WorldReport wire step from
//! the `row-hold-close-recovery`, `scale-identity-primary` and
//! `native-domain` scenarios through a fresh engine census, and check
//! conservation without running the scenarios against the engine.

use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{PresentationRowV4, RowDiagnosticLane, WorldReportV4};
use tiler_core::world_stage_v4::StagedV4Census;
use tiler_protocol::world_protocol::decode_report_v4;

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

fn accepted_reports() -> Vec<(String, WorldReportV4)> {
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
        .map(|fixture| {
            let text = std::fs::read_to_string(root.join(&fixture.file)).expect("fixture reads");
            let report = decode_report_v4(&text).expect("fixture decodes").report;
            (fixture.id, report)
        })
        .collect()
}

fn lane_index(lane: RowDiagnosticLane) -> u8 {
    match lane {
        RowDiagnosticLane::Screen => 0,
        RowDiagnosticLane::Window => 1,
        RowDiagnosticLane::Native => 2,
        RowDiagnosticLane::Attempt => 3,
        RowDiagnosticLane::Presentation => 4,
    }
}

fn assert_conserved(id: &str, report: &WorldReportV4, census: &StagedV4Census) {
    assert_eq!(census.screens, report.screens.len(), "{id} screens");
    assert_eq!(
        census.held_screens,
        report.held_screens.len(),
        "{id} held_screens"
    );
    assert_eq!(
        census.held_windows,
        report.held_windows.len(),
        "{id} held_windows"
    );
    assert_eq!(
        census.held_native,
        report.held_native.len(),
        "{id} held_native"
    );
    assert_eq!(
        census.held_attempts,
        report.held_attempts.len(),
        "{id} held_attempts"
    );
    assert_eq!(census.closes, report.closed.len(), "{id} closes");
    // The test engine installs an application identity no fixture window
    // shares, so own-process exclusion never fires and every carried
    // window stages a fact.
    assert_eq!(census.facts, report.windows.len(), "{id} facts");
    assert_eq!(
        census.presentation_held,
        matches!(report.presentation, PresentationRowV4::Held(_)),
        "{id} presentation_held"
    );
    assert_eq!(
        census.renderer_input, report.renderer_input,
        "{id} renderer_input"
    );
    assert_eq!(census.echoes, 0, "{id} echoes");
    // Counting pre-resolution held diagnostics is sound for this corpus:
    // the engine is fresh (no retained home, empty prior homing), so an
    // output-less held row keeps `None`, and the decoder only emits a held
    // diagnostic for a carried output scope, never a fabricated one.
    let expected_diagnostics = report.held_screens.len()
        + report
            .held_windows
            .iter()
            .filter(|held| held.diagnostic.is_some())
            .count()
        + report.held_native.len()
        + report
            .held_attempts
            .iter()
            .filter(|held| held.diagnostic.is_some())
            .count()
        + usize::from(
            matches!(&report.presentation, PresentationRowV4::Held(held) if held.diagnostic.is_some()),
        );
    assert_eq!(
        census.diagnostics.len(),
        expected_diagnostics,
        "{id} diagnostics"
    );
    // Reordering a lane breaks the non-decreasing lane order (a dropped
    // lane alone does not; drops are caught by the count assertions above).
    let lanes: Vec<u8> = census
        .diagnostics
        .iter()
        .map(|item| lane_index(item.row))
        .collect();
    let mut ordered = lanes.clone();
    ordered.sort_unstable();
    assert_eq!(lanes, ordered, "{id} diagnostic lane order");
}

/// Every accepted report fixture stages with full conservation: lane counts
/// match the decoded report, facts cover every carried window, the
/// presentation flag and renderer input ride verbatim, no echoes stage, and
/// diagnostics cover exactly the held rows carrying a scope.
#[test]
fn v4_staging_conserves_all_accepted_reports() {
    let reports = accepted_reports();
    assert_eq!(reports.len(), 207, "accepted WorldReport fixture count");
    let engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    for (id, report) in &reports {
        let census = engine.stage_report_v4_census(report);
        assert_conserved(id, report, &census);
    }
}

/// The same conservation holds for every WorldReport wire step staged in
/// step order from the three unit 6 scenarios. Steps are staged, never run.
#[test]
fn v4_staging_conserves_scenario_reports() {
    let engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    for (scenario, expected_staged, expected_rejected) in [
        ("row-hold-close-recovery", 6, 1),
        ("scale-identity-primary", 4, 0),
        ("native-domain", 8, 0),
    ] {
        let text =
            std::fs::read_to_string(fixture_root().join(format!("scenarios/{scenario}.json")))
                .expect("scenario reads");
        let value: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
        let steps = value
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .expect("scenario steps array");
        let mut staged = 0;
        let mut rejected = 0;
        for step in steps {
            let Some(message) = step
                .get("input")
                .filter(|input| {
                    input.get("kind").and_then(serde_json::Value::as_str) == Some("wire")
                        && input.get("to").and_then(serde_json::Value::as_str) == Some("core")
                })
                .and_then(|input| input.get("message"))
                .filter(|message| {
                    message.get("reqSeq").is_some()
                        && message.get("v").and_then(serde_json::Value::as_u64) == Some(4)
                        && message.get("type").is_none()
                })
            else {
                continue;
            };
            let text = serde_json::to_string(message).expect("message serializes");
            let step_id = step
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("step");
            match decode_report_v4(&text) {
                Ok(decoded) => {
                    let census = engine.stage_report_v4_census(&decoded.report);
                    assert_conserved(&format!("{scenario}/{step_id}"), &decoded.report, &census);
                    staged += 1;
                }
                Err(_) => {
                    // Rejection-path steps (e.g. `invalid-row-identity`)
                    // never reach staging; pin that the scenario marks
                    // the step as a rejected outcome instead of silently
                    // skipping it.
                    let assertions = step
                        .get("expect")
                        .and_then(|expect| expect.get("assertions"))
                        .and_then(serde_json::Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default();
                    assert!(
                        assertions.contains("rejected"),
                        "{scenario}/{step_id} refuses without a rejected expectation"
                    );
                    rejected += 1;
                }
            }
        }
        assert_eq!(staged, expected_staged, "{scenario} staged report count");
        assert_eq!(
            rejected, expected_rejected,
            "{scenario} rejected report count"
        );
    }
}
