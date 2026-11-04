//! v4 staging intake census (unit 6 sub-increments 2-3): stage every accepted
//! `WorldReport` fixture and every `to: "core"` WorldReport wire step from the
//! row-hold-close-recovery, scale-identity-primary and native-domain scenarios
//! plus the five capability-context scenarios through a fresh engine census,
//! and check conservation without running the scenarios against the engine.
//!
//! Sub-increment 3 adds capability-conflict grading evidence: the four
//! immutable context fixtures stage under their declared handshake
//! capabilities (decoded with the existing handshake codec), and the five
//! named scenarios stage under each segment's declared handshake. Staging is
//! never execution: the clone-based census retains nothing, so one engine per
//! handshake only establishes capability context.

use tiler_core::connection::{FactName, HostCapabilities};
use tiler_core::directional::OutputId;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    AuthorityScope, PresentationRowV4, RowDiagnosticKind, RowDiagnosticLane, WorldReportV4,
};
use tiler_core::world_stage_v4::StagedV4Census;
use tiler_protocol::connection_protocol::decode_handshake_request;
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

/// Permissive staging engine for the conservation census: all four state
/// capabilities true so supported state facts and attempts stay live and the
/// 207-fixture count stays meaningful. Everything else keeps the
/// `new_test_bound` defaults, so only state-capability grading changes.
fn permissive_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
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

/// Every accepted report fixture stages with full conservation: lane counts
/// match the decoded report, facts cover every carried window, the
/// presentation flag and renderer input ride verbatim, no echoes stage, and
/// diagnostics cover exactly the held rows carrying a scope.
#[test]
fn v4_staging_conserves_all_accepted_reports() {
    let reports = accepted_reports();
    assert_eq!(reports.len(), 207, "accepted WorldReport fixture count");
    let engine = permissive_engine();
    for (id, report) in &reports {
        let census = engine.stage_report_v4_census(report);
        assert_conserved(id, report, &census);
    }
}

/// The same conservation holds for every WorldReport wire step staged in
/// step order from the three unit 6 scenarios. Steps are staged, never run.
#[test]
fn v4_staging_conserves_scenario_reports() {
    let engine = permissive_engine();
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

/// Decode the immutable fixture `id` plus its manifest-declared handshake
/// context, both through the existing codecs. Returns the report and the
/// handshake's capabilities for a capability-context engine.
fn fixture_report_with_handshake(id: &str) -> (WorldReportV4, HostCapabilities) {
    let root = fixture_root();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("v4 manifest reads"),
    )
    .expect("v4 manifest parses");
    let entry = manifest
        .get("fixtures")
        .and_then(serde_json::Value::as_array)
        .expect("manifest fixtures array")
        .iter()
        .find(|fixture| fixture.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("{id} listed in v4 manifest"));
    let file = entry
        .get("file")
        .and_then(serde_json::Value::as_str)
        .expect("fixture file");
    let handshake_id = entry
        .get("context")
        .and_then(|context| context.get("handshake"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("{id} declares a handshake context"));
    let handshake_entry = manifest
        .get("fixtures")
        .and_then(serde_json::Value::as_array)
        .expect("manifest fixtures array")
        .iter()
        .find(|fixture| fixture.get("id").and_then(serde_json::Value::as_str) == Some(handshake_id))
        .unwrap_or_else(|| panic!("{handshake_id} listed in v4 manifest"));
    let handshake_file = handshake_entry
        .get("file")
        .and_then(serde_json::Value::as_str)
        .expect("handshake file");
    let report_text = std::fs::read_to_string(root.join(file)).expect("fixture reads");
    let report = decode_report_v4(&report_text)
        .unwrap_or_else(|error| panic!("{id} decodes: {error:?}"))
        .report;
    let handshake_text =
        std::fs::read_to_string(root.join(handshake_file)).expect("handshake reads");
    let handshake = decode_handshake_request(&handshake_text)
        .unwrap_or_else(|error| panic!("{handshake_id} decodes: {error:?}"));
    (report, handshake.capabilities)
}

fn engine_for_handshake(caps: HostCapabilities) -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps).expect("generation")
}

fn has_conflicting(census: &StagedV4Census) -> bool {
    census
        .diagnostics
        .iter()
        .any(|item| item.kind == RowDiagnosticKind::ConflictingFact)
}

fn output_scope(out: &str) -> AuthorityScope {
    AuthorityScope::Output(OutputId(out.to_owned()))
}

/// Unavailable-fact contradiction grades the window to a narrow hold: the
/// `managed` fact is non-null while `handshake-unavailable-facts` (21 names,
/// maximize/nativeSticky true) declares it unavailable. The permissive
/// control keeps the same row live, proving the hold came from the declared
/// capabilities rather than the row shape.
#[test]
fn v4_staging_grades_unavailable_fact_contradiction() {
    let (report, caps) = fixture_report_with_handshake("invalid-unavailable-fact-contradiction");
    assert!(caps.maximize && caps.native_sticky, "native routes on");
    assert!(
        caps.unavailable_facts.contains(&FactName::Managed),
        "managed unavailable"
    );
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.state_attempts.len(), 0);
    let census = engine_for_handshake(caps).stage_report_v4_census(&report);
    assert_eq!(census.facts, 0, "contradicting window held");
    assert_eq!(census.held_windows, 1);
    assert_eq!(census.diagnostics.len(), 1);
    assert_eq!(census.diagnostics[0].scope, output_scope("out-a"));
    assert_eq!(census.diagnostics[0].row, RowDiagnosticLane::Window);
    assert_eq!(census.diagnostics[0].id.as_deref(), Some("win-a"));
    assert_eq!(
        census.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );

    let control = permissive_engine().stage_report_v4_census(&report);
    assert_eq!(control.facts, 1, "permissive control keeps the row live");
    assert_eq!(control.held_windows, 0);
    assert!(control.diagnostics.is_empty());
}

/// Executable-unavailable contradiction grades the same way: the window
/// carries a non-null executable while the handshake declares it
/// unavailable. Same single Window ConflictingFact shape, same permissive
/// control.
#[test]
fn v4_staging_grades_executable_unavailable_value() {
    let (report, caps) = fixture_report_with_handshake("invalid-executable-unavailable-value");
    assert!(
        caps.unavailable_facts.contains(&FactName::Executable),
        "executable unavailable"
    );
    assert_eq!(report.windows.len(), 1);
    let census = engine_for_handshake(caps).stage_report_v4_census(&report);
    assert_eq!(census.facts, 0, "contradicting window held");
    assert_eq!(census.held_windows, 1);
    assert_eq!(census.diagnostics.len(), 1);
    assert_eq!(census.diagnostics[0].scope, output_scope("out-a"));
    assert_eq!(census.diagnostics[0].row, RowDiagnosticLane::Window);
    assert_eq!(census.diagnostics[0].id.as_deref(), Some("win-a"));
    assert_eq!(
        census.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );

    let control = permissive_engine().stage_report_v4_census(&report);
    assert_eq!(control.facts, 1, "permissive control keeps the row live");
    assert!(control.diagnostics.is_empty());
}

/// Sticky-projected fact under `handshake-sticky-native-free` (nativeSticky
/// and maximize both false): the carried `sticky:true` conflicts with the
/// native-free route, and the carried `maximized:false` conflicts with the
/// composed route (contract: maximized stays null when maximize is false).
/// Two independent causes collapse into one narrow hold with one Window
/// ConflictingFact diagnostic.
#[test]
fn v4_staging_grades_sticky_projected_fact() {
    let (report, caps) = fixture_report_with_handshake("invalid-sticky-projected-fact");
    assert!(!caps.native_sticky && !caps.maximize, "native-free route");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].sticky, Some(true));
    assert_eq!(report.windows[0].maximized, Some(false));
    let census = engine_for_handshake(caps).stage_report_v4_census(&report);
    assert_eq!(census.facts, 0, "projected fact held");
    assert_eq!(census.held_windows, 1);
    assert_eq!(census.diagnostics.len(), 1);
    assert_eq!(census.diagnostics[0].scope, output_scope("out-a"));
    assert_eq!(census.diagnostics[0].row, RowDiagnosticLane::Window);
    assert_eq!(census.diagnostics[0].id.as_deref(), Some("win-a"));
    assert_eq!(
        census.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );

    let control = permissive_engine().stage_report_v4_census(&report);
    assert_eq!(control.facts, 1, "permissive control keeps the row live");
    assert!(control.diagnostics.is_empty());
}

/// Sticky-native-free attempt under the same handshake: the window itself
/// carries `sticky:null`, so no sticky fact conflict fires, but the carried
/// `maximized:false` still conflicts with maximize=false (contract 682-692:
/// maximized must be null on the composed route). The engine therefore stages
/// an ancillary Window ConflictingFact hold for the maximized fact as well as
/// the new Attempt ConflictingFact diagnostic for the unsolicited native
/// sticky attempt. The extra hold is correct grader behavior, not a contract
/// problem. The raw attempt stays intact (never a held row); legacy
/// non-consumption of unsupported attempts is pinned in core
/// (`v4_unsupported_attempt_does_not_settle`).
#[test]
fn v4_staging_grades_sticky_native_free_attempt_with_maximize_hold() {
    let (report, caps) = fixture_report_with_handshake("invalid-sticky-native-free-attempt");
    assert!(!caps.native_sticky && !caps.maximize, "native-free route");
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].sticky, None, "no sticky fact conflict");
    assert_eq!(
        report.windows[0].maximized,
        Some(false),
        "maximized fact conflicts with maximize=false"
    );
    assert_eq!(report.state_attempts.len(), 1, "raw attempt carried");
    assert_eq!(report.held_attempts.len(), 0);
    let engine = engine_for_handshake(caps);
    let census = engine.stage_report_v4_census(&report);
    assert_eq!(census.facts, 0, "maximized-false window held");
    assert_eq!(census.held_windows, 1);
    assert_eq!(
        census.held_attempts, 0,
        "raw attempt never becomes a held row"
    );
    assert_eq!(census.diagnostics.len(), 2, "window hold plus attempt");
    assert_eq!(census.diagnostics[0].scope, output_scope("out-a"));
    assert_eq!(census.diagnostics[0].row, RowDiagnosticLane::Window);
    assert_eq!(census.diagnostics[0].id.as_deref(), Some("win-a"));
    assert_eq!(
        census.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );
    assert_eq!(census.diagnostics[1].scope, output_scope("out-a"));
    assert_eq!(census.diagnostics[1].row, RowDiagnosticLane::Attempt);
    assert_eq!(census.diagnostics[1].id.as_deref(), Some("win-a"));
    assert_eq!(
        census.diagnostics[1].kind,
        RowDiagnosticKind::ConflictingFact
    );

    // Permissive control: the same report stages fully live and silent,
    // proving both diagnostics came from the declared capabilities.
    let control = permissive_engine().stage_report_v4_census(&report);
    assert_eq!(control.facts, 1);
    assert_eq!(control.held_windows, 0);
    assert!(control.diagnostics.is_empty());

    // Maximize-null counterfactual, in memory only (fixture untouched): with
    // maximized nulled the window stays live and only the intended sticky
    // attempt diagnostic remains, isolating the sub-increment 3 negative.
    let mut counterfactual = report.clone();
    counterfactual.windows[0].maximized = None;
    let isolated = engine.stage_report_v4_census(&counterfactual);
    assert_eq!(isolated.facts, 1);
    assert_eq!(isolated.held_windows, 0);
    assert_eq!(isolated.diagnostics.len(), 1);
    assert_eq!(isolated.diagnostics[0].row, RowDiagnosticLane::Attempt);
    assert_eq!(
        isolated.diagnostics[0].kind,
        RowDiagnosticKind::ConflictingFact
    );
}

/// The five capability-context scenarios stage report steps under each
/// segment's declared handshake (decoded with the existing handshake codec).
/// One engine per handshake establishes capability context only: the
/// clone-based census retains nothing, so this pins staged/rejected counts
/// plus representative conflict/support assertions, never sequential
/// retained-state progression or full execution.
#[test]
fn v4_staging_covers_capability_context_scenarios() {
    for (scenario, expected_staged, expected_rejected) in [
        ("raw-fact-availability", 4, 0),
        ("executable-identity", 7, 0),
        ("native-domain", 8, 0),
        ("window-state", 19, 0),
        ("raw-attempt-settlement", 10, 0),
    ] {
        let text =
            std::fs::read_to_string(fixture_root().join(format!("scenarios/{scenario}.json")))
                .expect("scenario reads");
        let value: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
        let steps = value
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .expect("scenario steps array");
        let mut engine: Option<WorldEngine> = None;
        let mut staged = 0;
        let mut rejected = 0;
        for step in steps {
            let input = step.get("input").expect("step input");
            let Some(message) = input.get("message") else {
                continue;
            };
            if input.get("kind").and_then(serde_json::Value::as_str) == Some("wire")
                && input.get("to").and_then(serde_json::Value::as_str) == Some("core")
            {
                if message.get("type").and_then(serde_json::Value::as_str) == Some("handshake") {
                    let text = serde_json::to_string(message).expect("message serializes");
                    let handshake = decode_handshake_request(&text)
                        .unwrap_or_else(|error| panic!("{scenario} handshake decodes: {error:?}"));
                    engine = Some(engine_for_handshake(handshake.capabilities));
                    continue;
                }
                let is_report = message.get("reqSeq").is_some()
                    && message.get("v").and_then(serde_json::Value::as_u64) == Some(4)
                    && message.get("type").is_none();
                if !is_report {
                    continue;
                }
                let text = serde_json::to_string(message).expect("message serializes");
                let step_id = step
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("step");
                match decode_report_v4(&text) {
                    Ok(decoded) => {
                        let engine = engine.as_ref().unwrap_or_else(|| {
                            panic!("{scenario}/{step_id} reports before any handshake")
                        });
                        let census = engine.stage_report_v4_census(&decoded.report);
                        // The declared conflict row grades to a hold, so the
                        // generic conservation (facts cover every carried
                        // window) cannot hold there; that step pins its graded
                        // conservation explicitly below instead.
                        let graded = matches!(
                            (scenario, step_id),
                            ("raw-fact-availability", "unavailable-non-null-conflict")
                        );
                        if !graded {
                            assert_conserved(
                                &format!("{scenario}/{step_id}"),
                                &decoded.report,
                                &census,
                            );
                        }
                        match (scenario, step_id) {
                            ("raw-fact-availability", "unavailable-not-eternal-hold") => {
                                assert_eq!(census.facts, 1, "all-null row stages a fact");
                                assert_eq!(census.held_windows, 0);
                                assert!(
                                    !has_conflicting(&census),
                                    "null unavailable inputs never conflict"
                                );
                            }
                            ("raw-fact-availability", "unavailable-non-null-conflict") => {
                                assert_eq!(census.facts, 0, "non-null row held");
                                assert_eq!(census.held_windows, 1);
                                // Graded conservation: every carried window is
                                // accounted for between facts and holds, and
                                // all other lanes ride verbatim.
                                assert_eq!(
                                    census.facts + census.held_windows,
                                    decoded.report.windows.len()
                                        + decoded.report.held_windows.len(),
                                    "window rows conserved across the grade"
                                );
                                assert_eq!(census.screens, decoded.report.screens.len(), "screens");
                                assert_eq!(
                                    census.held_screens,
                                    decoded.report.held_screens.len(),
                                    "held_screens"
                                );
                                assert_eq!(
                                    census.held_native,
                                    decoded.report.held_native.len(),
                                    "held_native"
                                );
                                assert_eq!(
                                    census.held_attempts,
                                    decoded.report.held_attempts.len(),
                                    "held_attempts"
                                );
                                assert_eq!(census.closes, decoded.report.closed.len(), "closes");
                                assert_eq!(census.echoes, 0, "echoes");
                                assert_eq!(census.diagnostics.len(), 1);
                                assert_eq!(census.diagnostics[0].row, RowDiagnosticLane::Window);
                                assert_eq!(census.diagnostics[0].id.as_deref(), Some("win-a"));
                                assert_eq!(
                                    census.diagnostics[0].kind,
                                    RowDiagnosticKind::ConflictingFact
                                );
                            }
                            ("raw-fact-availability", _) => {
                                assert!(
                                    !has_conflicting(&census),
                                    "supported segment stages silent"
                                );
                            }
                            ("window-state", "native-sticky-observed") => {
                                assert_eq!(
                                    decoded.report.state_attempts.len(),
                                    1,
                                    "raw sticky attempt carried"
                                );
                                assert_eq!(census.held_attempts, 0, "supported attempt never held");
                                assert!(
                                    census
                                        .diagnostics
                                        .iter()
                                        .all(|item| { item.row != RowDiagnosticLane::Attempt }),
                                    "supported sticky attempt stays silent"
                                );
                            }
                            ("raw-attempt-settlement", "birth-clear-issued") => {
                                assert_eq!(
                                    census.facts, 4,
                                    "maximized-true window live under maximize=true"
                                );
                                assert!(
                                    !has_conflicting(&census),
                                    "all-native routes support every fact"
                                );
                            }
                            _ => {
                                assert!(
                                    !has_conflicting(&census),
                                    "{scenario}/{step_id} carries no capability conflict"
                                );
                            }
                        }
                        staged += 1;
                    }
                    Err(_) => {
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
        }
        assert!(engine.is_some(), "{scenario} declares a handshake");
        assert_eq!(staged, expected_staged, "{scenario} staged report count");
        assert_eq!(
            rejected, expected_rejected,
            "{scenario} rejected report count"
        );
    }
}
