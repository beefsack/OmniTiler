//! Staged v4 whole-report decoder (4a): manifest-driven coverage for
//! [`decode_report_v4`](tiler_protocol::world_protocol::decode_report_v4)
//! over the `test-fixtures/world-wire/v4` WorldReport fixtures: 126
//! valid-wire accept, 77 structural held-row accept with
//! lane/identity/ordinal/raw/kind assertions, 4 handshake-context rows
//! accepted as valid typed rows (contextual classification deferred to the
//! staged engine use), 80 context-free reject-wire refusals with exact
//! kind/message from the untouched original (then a post-rejection
//! in-memory single-pointer repair as a positive counterfactual), 2
//! same-wire host-context negatives refused by
//! `decode_report_v4_for_host` under their declared wrong-host context, and
//! structural boundary gates (duplicate keys, oversize, version,
//! id/duplicate/overlap invariants).
//! Offline only: no adapters, no live work, no engine.

use std::collections::BTreeMap;

use tiler_core::connection::RawSettingValue;
use tiler_core::world_report::{
    AttemptLaneV4, PresentationRowV4, RowDiagnosticKind, RowDiagnosticLane,
};
use tiler_protocol::world_protocol::{
    CreationReceiptWire, DecodedReportV4, decode_report_v4, decode_report_v4_for_host,
    decode_request, encode_report_v4,
};

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
    #[serde(default)]
    context: serde_json::Value,
    #[serde(default = "default_canonical")]
    canonical: bool,
    invalid: Option<InvalidRef>,
}

fn default_canonical() -> bool {
    true
}

#[derive(serde::Deserialize, Clone)]
struct InvalidRef {
    #[serde(default)]
    pointer: String,
    #[serde(default)]
    rule: String,
    source: Option<String>,
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

fn fixture_value(file: &str) -> serde_json::Value {
    serde_json::from_str(&fixture_text(file)).expect("fixture parses")
}

fn decode_ok(file: &str) -> DecodedReportV4 {
    decode_report_v4(&fixture_text(file)).expect("fixture decodes")
}

/// Test-side JSON-agnostic mirror of the retained-field conversion: JSON
/// numbers become finite `f64` facts.
fn to_raw(value: &serde_json::Value) -> RawSettingValue {
    match value {
        serde_json::Value::Null => RawSettingValue::Null,
        serde_json::Value::Bool(set) => RawSettingValue::Bool(*set),
        serde_json::Value::Number(number) => {
            RawSettingValue::Number(number.as_f64().expect("finite f64"))
        }
        serde_json::Value::String(text) => RawSettingValue::String(text.clone()),
        serde_json::Value::Array(items) => {
            RawSettingValue::Array(items.iter().map(to_raw).collect())
        }
        serde_json::Value::Object(entries) => RawSettingValue::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), to_raw(item)))
                .collect(),
        ),
    }
}

fn order_object(
    entries: Vec<(String, RawSettingValue)>,
    order: &[&str],
) -> Vec<(String, RawSettingValue)> {
    let mut rest: BTreeMap<String, RawSettingValue> = entries.into_iter().collect();
    let mut ordered = Vec::with_capacity(rest.len());
    for key in order {
        if let Some(value) = rest.remove(*key) {
            ordered.push((key.to_string(), value));
        }
    }
    for (key, value) in rest {
        ordered.push((key, value));
    }
    ordered
}

/// Normalize retained fields for order-insensitive comparison: top-level
/// order is asserted separately; nested objects sort recursively so
/// schema-ordered retentions compare by value.
fn normalize(value: &RawSettingValue) -> RawSettingValue {
    match value {
        RawSettingValue::Object(entries) => {
            let mut sorted: Vec<(String, RawSettingValue)> = entries
                .iter()
                .map(|(key, item)| (key.clone(), normalize(item)))
                .collect();
            sorted.sort_by(|left, right| left.0.cmp(&right.0));
            RawSettingValue::Object(sorted)
        }
        RawSettingValue::Array(items) => {
            RawSettingValue::Array(items.iter().map(normalize).collect())
        }
        _ => value.clone(),
    }
}

fn normalize_fields(fields: &[(String, RawSettingValue)]) -> BTreeMap<String, RawSettingValue> {
    fields
        .iter()
        .map(|(key, value)| (key.clone(), normalize(value)))
        .collect()
}

fn expected_fields(
    row: &serde_json::Map<String, serde_json::Value>,
    schema: &[&str],
) -> Vec<(String, RawSettingValue)> {
    let mut fields = Vec::with_capacity(row.len());
    for key in schema {
        if let Some(value) = row.get(*key) {
            fields.push((key.to_string(), to_raw(value)));
        }
    }
    let mut extra: Vec<&String> = row
        .keys()
        .filter(|key| !schema.contains(&key.as_str()))
        .collect();
    extra.sort();
    for key in extra {
        fields.push((key.clone(), to_raw(&row[key])));
    }
    fields
}

const WINDOW_SCHEMA: &[&str] = &[
    "win",
    "out",
    "rect",
    "fullscreen",
    "maximized",
    "sticky",
    "minimized",
    "hints",
    "nativeWs",
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

#[test]
fn v4_fixture_census_matches_contract() {
    let fixtures = world_fixtures();
    assert_eq!(fixtures.len(), 289, "WorldReport fixture family size");
    let count = |expectation: &str| {
        fixtures
            .iter()
            .filter(|fixture| fixture.expectation == expectation)
            .count()
    };
    assert_eq!(count("valid-wire"), 126, "valid-wire count");
    assert_eq!(count("accepted-held-row"), 81, "accepted-held-row count");
    assert_eq!(count("reject-wire"), 82, "reject-wire count");
}

#[test]
fn v4_valid_wire_accepts() {
    let fixtures = world_fixtures();
    let valid: Vec<&ManifestFixture> = fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "valid-wire")
        .collect();
    assert_eq!(valid.len(), 126);
    for fixture in valid {
        let decoded = decode_ok(&fixture.file);
        assert_eq!(decoded.report.contract_version, 4, "{}", fixture.id);
        assert!(
            decoded.report.held_screens.is_empty()
                && decoded.report.held_windows.is_empty()
                && decoded.report.held_native.is_empty()
                && decoded.report.held_attempts.is_empty()
                && matches!(decoded.report.presentation, PresentationRowV4::Valid(_)),
            "{} holds no rows",
            fixture.id
        );
    }
}

#[test]
fn v4_valid_empty_report_shape() {
    let decoded = decode_ok("messages/report-empty.json");
    let report = &decoded.report;
    assert_eq!(report.adapter_incarnation, "adapter-a");
    assert_eq!(report.core_generation, "generation-a");
    assert_eq!(report.observation_seq, 1);
    assert_eq!(report.request_seq, 1);
    assert_eq!(report.request_id, "request-a");
    assert_eq!(report.command_seq, None);
    assert_eq!(report.command, None);
    assert!(report.screens.is_empty());
    assert!(report.windows.is_empty());
    assert!(report.closed.is_empty());
    assert!(report.native.is_empty());
    assert_eq!(report.cursor, None);
    assert_eq!(report.cursor_hit, None);
    assert_eq!(report.focused_global, "");
    assert_eq!(report.action_foreground, None);
    assert_eq!(report.action_cached, None);
    assert!(report.action_outputs.is_empty());
    assert_eq!(report.primary_output, None);
    assert!(report.inventory_complete);
    assert!(!report.gesture_barrier);
    assert_eq!(report.gesture, None);
    assert_eq!(report.monotonic_ms, 0);
    assert!(report.frames.is_empty());
    assert!(report.renderer_input.is_none());
    match &report.presentation {
        PresentationRowV4::Valid(ingress) => {
            assert_eq!(ingress.foreground, "");
            assert_eq!(ingress.foreground_minimized, None);
            assert!(!ingress.chord_held);
            assert!(ingress.pager_order.is_empty());
            assert!(ingress.pager_frames.is_empty());
            assert_eq!(ingress.witness, None);
            assert_eq!(ingress.foreground_out, None);
        }
        PresentationRowV4::Held(_) => panic!("empty presentation is valid"),
    }
}

#[test]
fn v4_valid_command_and_renderer_shapes() {
    // Direction-less by-id focus spells absence as the empty direction.
    let decoded = decode_ok("messages/report-command-focus.json");
    match decoded.report.command {
        Some(tiler_core::world_report::WorldCommand::Focus { window, direction }) => {
            assert_eq!(window.0, "win-a");
            assert_eq!(direction, "");
        }
        other => panic!("focus decodes by id, got {other:?}"),
    }
    // Promoted direct payloads ride the operation route.
    let decoded = decode_ok("messages/report-command-toggle-float.json");
    match decoded.report.command {
        Some(tiler_core::world_report::WorldCommand::Operation(
            tiler_core::boundary::CoreCommand::ToggleFloat { window, float_rect },
        )) => {
            assert_eq!(window, "win-a");
            assert!(float_rect.is_some());
        }
        other => panic!("toggle-float promotes, got {other:?}"),
    }
    let decoded = decode_ok("messages/report-command-resize.json");
    match decoded.report.command {
        Some(tiler_core::world_report::WorldCommand::Operation(
            tiler_core::boundary::CoreCommand::Resize {
                mode, press_index, ..
            },
        )) => {
            assert_eq!(mode, "outwards");
            assert_eq!(press_index, 0);
        }
        other => panic!("resize promotes, got {other:?}"),
    }
    // Renderer input requires its command.
    let decoded = decode_ok("messages/report-renderer-window.json");
    let renderer = decoded.report.renderer_input.expect("renderer rides");
    assert_eq!(renderer.rev, 1);
    assert!(decoded.report.command.is_some());
}

#[test]
fn v4_structural_held_rows_accept_with_exact_retention() {
    let fixtures = world_fixtures();
    let mut held: Vec<&ManifestFixture> = fixtures
        .iter()
        .filter(|fixture| {
            fixture.expectation == "accepted-held-row" && fixture.context.get("handshake").is_none()
        })
        .collect();
    held.sort_by(|left, right| left.file.cmp(&right.file));
    assert_eq!(
        held.len(),
        77,
        "structural held count (81 minus 4 handshake-context)"
    );
    for fixture in held {
        let invalid = fixture.invalid.clone().expect("held pins a pointer");
        let doc = fixture_value(&fixture.file);
        let source = invalid.source.clone().expect("held pins a source");
        let source_doc = fixture_value(&format!("messages/{source}.json"));
        let source_decoded =
            decode_report_v4(&source_doc.to_string()).expect("held source decodes");
        let decoded = decode_ok(&fixture.file);
        let report = &decoded.report;
        let lane = invalid.pointer.split('/').nth(1).unwrap_or("");
        let held_total = report.held_screens.len()
            + report.held_windows.len()
            + report.held_native.len()
            + report.held_attempts.len()
            + usize::from(matches!(report.presentation, PresentationRowV4::Held(_)));
        assert_eq!(held_total, 1, "{} holds exactly one row", fixture.id);
        // Valid-plus-held conservation against the untouched source: the
        // single mutation moves one row from valid to held.
        assert_eq!(
            report.screens.len() + report.held_screens.len(),
            source_decoded.report.screens.len() + source_decoded.report.held_screens.len(),
            "{} screen conservation",
            fixture.id
        );
        assert_eq!(
            report.windows.len() + report.held_windows.len(),
            source_decoded.report.windows.len() + source_decoded.report.held_windows.len(),
            "{} window conservation",
            fixture.id
        );
        match lane {
            "screens" => assert_screen_hold(report, &doc, &invalid, fixture),
            "windows" => assert_window_hold(report, &doc, &invalid, fixture),
            "native" => assert_native_hold(report, &doc, &invalid, fixture),
            "presentation" => assert_presentation_hold(report, &doc, &invalid, fixture),
            "frameAttempts" | "memberAttempts" | "viewAttempts" | "clearAttempts"
            | "creationAttempts" => assert_attempt_hold(report, &doc, &invalid, fixture),
            other => panic!("{} unexpected held lane {other}", fixture.id),
        }
    }
}

fn assert_screen_hold(
    report: &tiler_core::world_report::WorldReportV4,
    doc: &serde_json::Value,
    invalid: &InvalidRef,
    fixture: &ManifestFixture,
) {
    let ordinal: usize = invalid
        .pointer
        .split('/')
        .nth(2)
        .expect("ordinal")
        .parse()
        .expect("usize");
    assert_eq!(report.held_screens.len(), 1, "{}", fixture.id);
    let held = &report.held_screens[0];
    assert_eq!(held.ordinal, ordinal, "{}", fixture.id);
    let row = doc["screens"][ordinal].as_object().expect("screen row");
    let out = row["out"].as_str().expect("opaque out");
    assert_eq!(held.output.0, out, "{}", fixture.id);
    assert_eq!(
        held.diagnostic.row,
        RowDiagnosticLane::Screen,
        "{}",
        fixture.id
    );
    assert_eq!(
        held.diagnostic.kind,
        RowDiagnosticKind::InvalidFact,
        "{}",
        fixture.id
    );
    assert_eq!(held.diagnostic.id.as_deref(), Some(out), "{}", fixture.id);
    // Identity sub-objects retain schema order; compare by value here and
    // pin the identity order for the overbound witness below.
    let expected = expected_fields(
        row,
        &["out", "full", "work", "complete", "scale", "identity"],
    );
    assert_eq!(
        held.fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        expected.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        "{} field order",
        fixture.id
    );
    assert_eq!(
        normalize_fields(&held.fields),
        normalize_fields(&expected),
        "{} field values",
        fixture.id
    );
}

fn assert_window_hold(
    report: &tiler_core::world_report::WorldReportV4,
    doc: &serde_json::Value,
    invalid: &InvalidRef,
    fixture: &ManifestFixture,
) {
    let ordinal: usize = invalid
        .pointer
        .split('/')
        .nth(2)
        .expect("ordinal")
        .parse()
        .expect("usize");
    assert_eq!(report.held_windows.len(), 1, "{}", fixture.id);
    let held = &report.held_windows[0];
    assert_eq!(held.ordinal, ordinal, "{}", fixture.id);
    let row = doc["windows"][ordinal].as_object().expect("window row");
    let win = row["win"].as_str().expect("opaque win");
    assert_eq!(held.window.0, win, "{}", fixture.id);
    let out = row.get("out").and_then(|value| value.as_str());
    assert_eq!(
        held.output.as_ref().map(|output| output.0.as_str()),
        out,
        "{}",
        fixture.id
    );
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact, "{}", fixture.id);
    match (&held.output, &held.diagnostic) {
        (Some(output), Some(diagnostic)) => {
            assert_eq!(
                diagnostic.scope,
                tiler_core::world_report::AuthorityScope::Output(output.clone()),
                "{}",
                fixture.id
            );
            assert_eq!(diagnostic.row, RowDiagnosticLane::Window, "{}", fixture.id);
            assert_eq!(diagnostic.id.as_deref(), Some(win), "{}", fixture.id);
            assert_eq!(
                diagnostic.kind,
                RowDiagnosticKind::InvalidFact,
                "{}",
                fixture.id
            );
        }
        (None, None) => {}
        _ => panic!("{} output-less hold carries no diagnostic", fixture.id),
    }
    let mut expected = expected_fields(row, WINDOW_SCHEMA);
    // Nested hint/visibility/lastWrite objects retain schema order (not
    // carried order); mirror the canonical retention for exact comparison.
    for (key, value) in expected.iter_mut() {
        if key == "hints" {
            if let RawSettingValue::Object(entries) = value.clone() {
                *value = RawSettingValue::Object(order_object(
                    entries,
                    &["minW", "minH", "maxW", "maxH"],
                ));
            }
        } else if key == "visibility" {
            if let RawSettingValue::Object(entries) = value.clone() {
                *value = RawSettingValue::Object(order_object(
                    entries,
                    &["visible", "suppressionOwned"],
                ));
            }
        } else if key == "lastWrite" {
            order_last_write(value);
        }
    }
    assert_eq!(
        held.fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        expected.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        "{} field order",
        fixture.id
    );
    assert_eq!(held.fields, expected, "{} exact retention", fixture.id);
}

fn order_last_write(value: &mut RawSettingValue) {
    const SAMPLE_ORDER: &[&str] = &[
        "obs",
        "lifetimeMatched",
        "visibility",
        "minimized",
        "hostHidden",
    ];
    const VISIBILITY_ORDER: &[&str] = &["visible", "suppressionOwned"];
    const LAST_WRITE_ORDER: &[&str] = &[
        "property", "target", "revision", "path", "setter", "reason", "pre", "post",
    ];
    let RawSettingValue::Object(entries) = value else {
        return;
    };
    let taken = std::mem::take(entries);
    *entries = order_object(taken, LAST_WRITE_ORDER);
    for (key, item) in entries.iter_mut() {
        if (key == "pre" || key == "post")
            && let RawSettingValue::Object(sample) = item
        {
            let taken = std::mem::take(sample);
            *sample = order_object(taken, SAMPLE_ORDER);
            for (sub_key, sub_value) in sample.iter_mut() {
                if sub_key == "visibility"
                    && let RawSettingValue::Object(visibility) = sub_value
                {
                    let taken = std::mem::take(visibility);
                    *visibility = order_object(taken, VISIBILITY_ORDER);
                }
            }
        }
    }
}

fn assert_native_hold(
    report: &tiler_core::world_report::WorldReportV4,
    doc: &serde_json::Value,
    invalid: &InvalidRef,
    fixture: &ManifestFixture,
) {
    let ordinal: usize = invalid
        .pointer
        .split('/')
        .nth(2)
        .expect("ordinal")
        .parse()
        .expect("usize");
    assert_eq!(report.held_native.len(), 1, "{}", fixture.id);
    let held = &report.held_native[0];
    assert_eq!(held.ordinal, ordinal, "{}", fixture.id);
    let row = doc["native"][ordinal].as_object().expect("native row");
    let out = row["out"].as_str().expect("opaque out");
    assert_eq!(held.output.0, out, "{}", fixture.id);
    assert_eq!(
        held.diagnostic.row,
        RowDiagnosticLane::Native,
        "{}",
        fixture.id
    );
    assert_eq!(
        held.diagnostic.kind,
        RowDiagnosticKind::InvalidFact,
        "{}",
        fixture.id
    );
    assert_eq!(held.diagnostic.id.as_deref(), Some(out), "{}", fixture.id);
    let expected = expected_fields(row, &["out", "order", "current", "occupancy"]);
    assert_eq!(
        held.fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        expected.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        "{} field order",
        fixture.id
    );
    assert_eq!(
        normalize_fields(&held.fields),
        normalize_fields(&expected),
        "{} field values",
        fixture.id
    );
}

fn attempt_schema(lane: AttemptLaneV4) -> &'static [&'static str] {
    match lane {
        AttemptLaneV4::Frame => &[
            "win",
            "target",
            "setter",
            "readback",
            "dpiBefore",
            "dpiAfter",
            "attemptMs",
            "op",
            "pre",
        ],
        AttemptLaneV4::Member => &[
            "op",
            "win",
            "out",
            "ws",
            "setter",
            "pre",
            "post",
            "attemptMs",
        ],
        AttemptLaneV4::View => &[
            "op",
            "out",
            "ws",
            "setter",
            "pre",
            "post",
            "base",
            "attemptMs",
        ],
        AttemptLaneV4::Clear => &["op", "win", "setter", "pre", "post", "attemptMs"],
        AttemptLaneV4::Creation => &[
            "op",
            "ws",
            "out",
            "setter",
            "before",
            "after",
            "created",
            "attemptMs",
        ],
    }
}

fn lane_key(lane: &str) -> (&'static str, AttemptLaneV4) {
    match lane {
        "frameAttempts" => ("frameAttempts", AttemptLaneV4::Frame),
        "memberAttempts" => ("memberAttempts", AttemptLaneV4::Member),
        "viewAttempts" => ("viewAttempts", AttemptLaneV4::View),
        "clearAttempts" => ("clearAttempts", AttemptLaneV4::Clear),
        "creationAttempts" => ("creationAttempts", AttemptLaneV4::Creation),
        other => panic!("unexpected attempt lane {other}"),
    }
}

fn assert_attempt_hold(
    report: &tiler_core::world_report::WorldReportV4,
    doc: &serde_json::Value,
    invalid: &InvalidRef,
    fixture: &ManifestFixture,
) {
    let lane_name = invalid.pointer.split('/').nth(1).expect("lane");
    let (array_key, lane) = lane_key(lane_name);
    let ordinal: usize = invalid
        .pointer
        .split('/')
        .nth(2)
        .expect("ordinal")
        .parse()
        .expect("usize");
    assert_eq!(report.held_attempts.len(), 1, "{}", fixture.id);
    let held = &report.held_attempts[0];
    assert_eq!(held.lane, lane, "{}", fixture.id);
    assert_eq!(held.ordinal, ordinal, "{}", fixture.id);
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact, "{}", fixture.id);
    let row = doc[array_key][ordinal].as_object().expect("attempt row");
    for key in ["win", "op"] {
        if let Some(serde_json::Value::String(text)) = row.get(key)
            && !text.is_empty()
        {
            assert!(
                held.identity.iter().any(|identity| identity == text),
                "{} identity carries {key}={text}",
                fixture.id
            );
        }
    }
    assert!(
        !held.identity.is_empty(),
        "{} identity is nonempty",
        fixture.id
    );
    let out = row.get("out").and_then(|value| value.as_str());
    match (&held.diagnostic, out) {
        (Some(diagnostic), Some(out)) => {
            assert_eq!(
                diagnostic.scope,
                tiler_core::world_report::AuthorityScope::Output(
                    tiler_core::directional::OutputId(out.to_owned())
                ),
                "{}",
                fixture.id
            );
            assert_eq!(diagnostic.row, RowDiagnosticLane::Attempt, "{}", fixture.id);
        }
        (None, _) => {}
        (Some(_), None) => panic!("{} attempt diagnostic without output", fixture.id),
    }
    let expected = expected_fields(row, attempt_schema(lane));
    assert_eq!(
        held.fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        expected.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        "{} field order",
        fixture.id
    );
    assert_eq!(
        normalize_fields(&held.fields),
        normalize_fields(&expected),
        "{} field values",
        fixture.id
    );
}

fn assert_presentation_hold(
    report: &tiler_core::world_report::WorldReportV4,
    doc: &serde_json::Value,
    invalid: &InvalidRef,
    fixture: &ManifestFixture,
) {
    let PresentationRowV4::Held(held) = &report.presentation else {
        panic!("{} presentation holds", fixture.id);
    };
    assert_eq!(held.kind, RowDiagnosticKind::InvalidFact, "{}", fixture.id);
    let row = doc["presentation"]
        .as_object()
        .expect("presentation object");
    let expected = expected_fields(
        row,
        &[
            "foreground",
            "minimized",
            "maximized",
            "fullscreen",
            "hostHidden",
            "hiddenWorkspace",
            "frame",
            "chordHeld",
            "moveWindow",
            "pagerOrder",
            "pagerFrames",
            "witness",
            "foregroundOut",
        ],
    );
    assert_eq!(
        held.fields.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        expected.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        "{} field order",
        fixture.id
    );
    assert_eq!(
        normalize_fields(&held.fields),
        normalize_fields(&expected),
        "{} field values",
        fixture.id
    );
    let foreground_out = row.get("foregroundOut").and_then(|value| value.as_str());
    match (&held.diagnostic, foreground_out) {
        (Some(diagnostic), Some(out)) => {
            assert_eq!(
                diagnostic.scope,
                tiler_core::world_report::AuthorityScope::Output(
                    tiler_core::directional::OutputId(out.to_owned())
                ),
                "{}",
                fixture.id
            );
            assert_eq!(
                diagnostic.row,
                RowDiagnosticLane::Presentation,
                "{}",
                fixture.id
            );
        }
        (None, _) => {}
        (Some(_), None) => panic!("{} presentation diagnostic without output", fixture.id),
    }
    assert!(
        invalid.pointer.starts_with("/presentation"),
        "{}",
        fixture.id
    );
}

/// The four handshake-context rows are invisible to the context-free shape
/// codec: no structural hold applies, so they decode as valid typed rows
/// with their carried values preserved. Contextual classification (sticky
/// projection, unavailable facts) belongs to the staged engine use, which
/// owns the handshake capabilities. This test pins the deferral: exact
/// carried values, zero held rows.
#[test]
fn v4_handshake_context_rows_decode_as_valid_typed() {
    // Declared-unavailable managed fact carried non-null: conflicting-fact
    // evidence for the engine, a plain valid bool to the shape codec.
    let decoded = decode_ok("messages/invalid-unavailable-fact-contradiction.json");
    assert!(decoded.report.held_windows.is_empty());
    assert_eq!(decoded.report.windows.len(), 1);
    assert_eq!(decoded.report.windows[0].managed, Some(true));
    // Declared-unavailable executable carried with a value: same deferral.
    let decoded = decode_ok("messages/invalid-executable-unavailable-value.json");
    assert!(decoded.report.held_windows.is_empty());
    assert_eq!(decoded.report.windows.len(), 1);
    assert!(
        decoded.report.windows[0]
            .executable
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    // Sticky projection without the native-sticky capability: same deferral.
    let decoded = decode_ok("messages/invalid-sticky-projected-fact.json");
    assert!(decoded.report.held_windows.is_empty());
    assert_eq!(decoded.report.windows.len(), 1);
    assert_eq!(decoded.report.windows[0].sticky, Some(true));
    // Native sticky attempt without nativeSticky: a valid typed state row.
    let decoded = decode_ok("messages/invalid-sticky-native-free-attempt.json");
    assert!(decoded.report.held_attempts.is_empty());
    assert_eq!(decoded.report.state_attempts.len(), 1);
    assert_eq!(
        decoded.report.state_attempts[0].property,
        tiler_core::world_report::StateProperty::Sticky
    );
}

/// Normative bounded-text coverage (contract role/appId/class/title are
/// bounded text; general wire rules allow C0 including NUL JSON-escaped):
/// each of the four accepts arbitrary UTF-8 including empty, NUL, and
/// controls within the bound as valid rows, while `executable` keeps its
/// distinct nonempty NUL-free identity grammar (empty/NUL holds).
#[test]
fn v4_bounded_text_accepts_empty_nul_controls() {
    for field in ["role", "appId", "class", "title"] {
        for value in [
            "".to_owned(),
            "\0".to_owned(),
            "\u{1}\u{8}\u{1f}".to_owned(),
        ] {
            let mut doc = fixture_value("messages/report-observe.json");
            doc["windows"][0][field] = serde_json::Value::String(value.clone());
            let decoded = decode_report_v4(&doc.to_string()).unwrap_or_else(|error| {
                panic!(
                    "{field} {value:?} accepts, got {}:{}",
                    error.kind, error.message
                )
            });
            assert!(
                decoded.report.held_windows.is_empty(),
                "{field} {value:?} holds no row"
            );
            assert_eq!(
                decoded.report.windows.len(),
                1,
                "{field} {value:?} stays valid"
            );
            let fact = &decoded.report.windows[0];
            let carried = match field {
                "role" => fact.role.as_deref(),
                "appId" => fact.app_id.as_deref(),
                "class" => fact.class.as_deref(),
                "title" => fact.title.as_deref(),
                _ => unreachable!(),
            };
            assert_eq!(carried, Some(value.as_str()), "{field} retains value");
        }
    }
    // Executable keeps the identity grammar: empty/NUL holds narrowly.
    for value in ["".to_owned(), "\0".to_owned()] {
        let mut doc = fixture_value("messages/report-observe.json");
        doc["windows"][0]["executable"] = serde_json::Value::String(value.clone());
        let decoded = decode_report_v4(&doc.to_string()).unwrap_or_else(|error| {
            panic!(
                "executable {value:?} holds, got {}:{}",
                error.kind, error.message
            )
        });
        assert_eq!(
            decoded.report.windows.len(),
            0,
            "executable {value:?} leaves valid"
        );
        assert_eq!(
            decoded.report.held_windows.len(),
            1,
            "executable {value:?} holds one row"
        );
        assert_eq!(
            decoded.report.held_windows[0].kind,
            RowDiagnosticKind::InvalidFact
        );
    }
}

fn expected_reject(rule: &str) -> (&'static str, &'static str) {
    if rule.starts_with("Removed report key") {
        return ("unknown-field", "request contains an unknown field");
    }
    if rule.starts_with("Removed wrapper/alias") {
        return ("unknown-value", "request contains an unknown value");
    }
    if rule.starts_with("Unidentifiable") {
        return ("malformed", "request is malformed");
    }
    match rule {
        "counter-numeric" => ("malformed", "request is malformed"),
        "counter-leading-zero" | "counter-negative" | "counter-overflow" => {
            ("unknown-value", "request contains an unknown value")
        }
        "generation-uppercase" => ("unknown-value", "request contains an unknown value"),
        "id-empty" | "id-alphabet" | "id-length" => {
            ("unknown-value", "request contains an unknown value")
        }
        "report-type-tag" => ("unknown-field", "request contains an unknown field"),
        "command-seq-pair" | "command-pair" => ("snapshot-invalid", "command-pair-invalid"),
        "missing-required-focused" | "focused-null" | "primary-null" => {
            ("malformed", "request is malformed")
        }
        "monotonic-number" => ("unknown-value", "request contains an unknown value"),
        "command-unknown-op" => ("unknown-value", "request contains an unknown value"),
        "focus-ignored-direction" => ("unknown-field", "request contains an unknown field"),
        "focus-directional-missing" | "focus-directional-null" => {
            ("malformed", "request is malformed")
        }
        "move-directional-empty" => ("unknown-value", "request contains an unknown value"),
        "direction-token" | "same-axis-token" => {
            ("unknown-value", "request contains an unknown value")
        }
        "select-numbered-zero" | "select-numbered-ten" | "relative-delta-zero" => {
            ("unknown-value", "request contains an unknown value")
        }
        "send-source-string" => ("malformed", "request is malformed"),
        "send-source-removed" => ("unknown-value", "request contains an unknown value"),
        "send-source-both" => ("unknown-field", "request contains an unknown field"),
        "resize-negative-press" | "resize-overflow-press" => ("malformed", "request is malformed"),
        "resize-mode" => ("unknown-value", "request contains an unknown value"),
        "toggle-float-missing-rect" => ("malformed", "request is malformed"),
        "state-command-property-name" => ("unknown-field", "request contains an unknown field"),
        "gesture-zero" | "gesture-kind" | "gesture-phase" => {
            ("unknown-value", "request contains an unknown value")
        }
        "intent-object-not-string" | "intent-outer-null" => ("malformed", "request is malformed"),
        "intent-overbound" => ("snapshot-invalid", "intent-data-invalid"),
        "renderer-without-command" => ("snapshot-invalid", "command-pair-invalid"),
        "renderer-target-missing-win" => ("malformed", "request is malformed"),
        "renderer-extra-win" => ("unknown-field", "request contains an unknown field"),
        "renderer-unknown-kind" => ("unknown-value", "request contains an unknown value"),
        "arrival-missing-kind" | "arrival-missing-output" | "arrival-missing-home" => {
            ("malformed", "request is malformed")
        }
        "arrival-unknown-kind"
        | "arrival-output-type"
        | "arrival-empty-output"
        | "arrival-logical-home" => ("unknown-value", "request contains an unknown value"),
        "arrival-mixed-arms" => ("unknown-field", "request contains an unknown field"),
        "arrival-output-on-native-host" | "arrival-native-on-tiler-host" => {
            ("snapshot-invalid", "arrival-host-mismatch")
        }
        "executable-missing" => ("malformed", "request is malformed"),
        other => panic!("reject table misses rule {other}"),
    }
}

fn pointer_tokens(pointer: &str) -> Vec<String> {
    if pointer.is_empty() {
        return Vec::new();
    }
    pointer
        .split('/')
        .skip(1)
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn repair_single_pointer(
    invalid: &mut serde_json::Value,
    source: &serde_json::Value,
    pointer: &str,
) {
    let tokens = pointer_tokens(pointer);
    assert!(!tokens.is_empty(), "pointer is not root {pointer}");
    let mut parent = invalid;
    for token in &tokens[..tokens.len() - 1] {
        if parent.is_array() {
            let index: usize = token.parse().expect("array index");
            parent = &mut parent[index];
        } else {
            parent = parent.get_mut(token).expect("repair parent exists");
        }
    }
    let last = tokens.last().expect("token");
    let source_value = source.pointer(pointer).cloned();
    if parent.is_array() {
        let index: usize = last.parse().expect("array index");
        match source_value {
            Some(value) => parent[index] = value,
            None => {
                parent.as_array_mut().expect("array").remove(index);
            }
        }
    } else {
        let map = parent.as_object_mut().expect("object parent");
        match source_value {
            Some(value) => {
                map.insert(last.clone(), value);
            }
            None => {
                map.remove(last);
            }
        }
    }
}

fn without_pointer(value: &serde_json::Value, pointer: &str) -> serde_json::Value {
    let mut masked = value.clone();
    let tokens = pointer_tokens(pointer);
    assert!(!tokens.is_empty(), "pointer is not root {pointer}");
    let mut parent = &mut masked;
    for token in &tokens[..tokens.len() - 1] {
        if parent.is_array() {
            let index: usize = token.parse().expect("array index");
            parent = &mut parent[index];
        } else {
            parent = parent.get_mut(token).expect("mask parent exists");
        }
    }
    let last = tokens.last().expect("token");
    if parent.is_array() {
        let index: usize = last.parse().expect("array index");
        if let Some(slot) = parent.get_mut(index) {
            *slot = serde_json::Value::String("__MASKED__".to_owned());
        }
    } else if let Some(map) = parent.as_object_mut() {
        map.insert(
            last.clone(),
            serde_json::Value::String("__MASKED__".to_owned()),
        );
    }
    masked
}

#[test]
fn v4_reject_wire_refuses_then_repairs() {
    let fixtures = world_fixtures();
    let mut rejects: Vec<&ManifestFixture> = fixtures
        .iter()
        .filter(|fixture| {
            fixture.expectation == "reject-wire"
                && fixture.file != "messages/invalid-arrival-output-on-native-host.json"
                && fixture.file != "messages/invalid-arrival-native-on-tiler-host.json"
        })
        .collect();
    rejects.sort_by(|left, right| left.file.cmp(&right.file));
    assert_eq!(rejects.len(), 80, "non-host reject count");
    for fixture in rejects {
        let invalid = fixture.invalid.clone().expect("reject pins a pointer");
        let (kind, message) = expected_reject(&invalid.rule);
        let invalid_text = fixture_text(&fixture.file);
        let error = decode_report_v4(&invalid_text).expect_err("reject refuses");
        assert_eq!(error.kind, kind, "{} kind", fixture.id);
        assert_eq!(error.message, message, "{} message", fixture.id);
        assert!(
            !error.message.contains("win-a"),
            "{} echoes payload",
            fixture.id
        );
        // Single-pointer causal repair: copy the source witness value at
        // exactly `invalid.pointer` into the invalid document (or remove
        // the key when the source has no such pointer). The rest of the
        // document stays original by construction; the repaired document
        // must then decode.
        let source_name = invalid.source.clone().expect("reject pins a source");
        let source_text = fixture_text(&format!("messages/{source_name}.json"));
        let source_value: serde_json::Value =
            serde_json::from_str(&source_text).expect("source parses");
        let mut invalid_value: serde_json::Value =
            serde_json::from_str(&invalid_text).expect("invalid parses");
        let before = invalid_value.clone();
        repair_single_pointer(&mut invalid_value, &source_value, &invalid.pointer);
        assert_ne!(
            invalid_value, before,
            "{} repair changes pointer",
            fixture.id
        );
        assert_eq!(
            without_pointer(&before, &invalid.pointer),
            without_pointer(&invalid_value, &invalid.pointer),
            "{} rest unchanged",
            fixture.id
        );
        let repaired_text = invalid_value.to_string();
        decode_report_v4(&repaired_text).unwrap_or_else(|error| {
            panic!(
                "{} repair accepts, got {}:{}",
                fixture.id, error.kind, error.message
            )
        });
    }
}

/// The two same-wire/opposite-host arrival pairs are not fixture errors:
/// the context-free shape codec accepts both arms, and the staged
/// connection-context gate (`decode_report_v4_for_host`) applies the same
/// arrival-arm rule the runtime owns. The caller passes the bound handshake
/// capability; the context is never inferred from the report. This test
/// proves the exact same wire is accepted under the correct host and
/// rejected under the wrong host with the distinct fixed message.
#[test]
fn v4_host_arm_context_gate() {
    let fixtures = world_fixtures();
    let by_file = |file: &str| {
        fixtures
            .iter()
            .find(|fixture| fixture.file == file)
            .expect("fixture present")
    };
    // Output arm: valid on tiler hosts, wrong on native hosts.
    let valid_output = by_file("messages/report-focus-feedback-proven.json");
    let invalid_output = by_file("messages/invalid-arrival-output-on-native-host.json");
    assert_eq!(
        valid_output.context.get("hostOwned"),
        Some(&serde_json::Value::Bool(false))
    );
    assert_eq!(
        invalid_output.context.get("hostOwned"),
        Some(&serde_json::Value::Bool(true))
    );
    let valid_text = fixture_text(&valid_output.file);
    let invalid_text = fixture_text(&invalid_output.file);
    assert_eq!(valid_text, invalid_text, "output arm same wire");
    decode_report_v4(&valid_text).expect("shape-only accepts output arm");
    decode_report_v4_for_host(&valid_text, false).expect("correct host accepts output arm");
    let error =
        decode_report_v4_for_host(&invalid_text, true).expect_err("wrong host rejects output arm");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "arrival-host-mismatch");
    // Native arm: valid on native hosts, wrong on tiler hosts.
    let valid_native = by_file("messages/report-arrival-native.json");
    let invalid_native = by_file("messages/invalid-arrival-native-on-tiler-host.json");
    assert_eq!(
        valid_native.context.get("hostOwned"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        invalid_native.context.get("hostOwned"),
        Some(&serde_json::Value::Bool(false))
    );
    let valid_text = fixture_text(&valid_native.file);
    let invalid_text = fixture_text(&invalid_native.file);
    assert_eq!(valid_text, invalid_text, "native arm same wire");
    decode_report_v4(&valid_text).expect("shape-only accepts native arm");
    decode_report_v4_for_host(&valid_text, true).expect("correct host accepts native arm");
    let error =
        decode_report_v4_for_host(&invalid_text, false).expect_err("wrong host rejects native arm");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "arrival-host-mismatch");
}

#[test]
fn v4_decoder_rejects_strict_envelope_violations() {
    let empty = fixture_text("messages/report-empty.json");
    decode_report_v4(&empty).expect("empty decodes");

    // Duplicate keys refuse (literal and escaped-equivalent spellings).
    let literal = empty.replacen(
        "\"adapter\":\"adapter-a\"",
        "\"adapter\":\"adapter-a\",\"adapter\":\"adapter-a\"",
        1,
    );
    assert_ne!(literal, empty);
    let error = decode_report_v4(&literal).expect_err("duplicate refuses");
    assert_eq!(error.kind, "malformed");
    assert_eq!(error.message, "request is malformed");
    let escaped = empty.replacen(
        "\"adapter\":\"adapter-a\"",
        "\"adapter\":\"adapter-a\",\"\\u0061dapter\":\"adapter-a\"",
        1,
    );
    assert_ne!(escaped, empty);
    let error = decode_report_v4(&escaped).expect_err("escaped duplicate refuses");
    assert_eq!(error.kind, "malformed");

    // Oversize refuses before parsing.
    let oversized = "x".repeat(tiler_protocol::world_protocol::WORLD_MAX_REQUEST_BYTES + 1);
    let error = decode_report_v4(&oversized).expect_err("oversize refuses");
    assert_eq!(error.kind, "oversized");
    assert_eq!(error.message, "request exceeds size bound");

    // Wrong contract version refuses; the v3 entrypoint keeps serving v3.
    let versioned = empty.replacen("\"v\":4", "\"v\":3", 1);
    assert_ne!(versioned, empty);
    let error = decode_report_v4(&versioned).expect_err("version refuses");
    assert_eq!(error.kind, "unsupported-version");
    assert_eq!(error.message, "unsupported contract version");
    let error = decode_request(&empty).expect_err("v3 rejects the v4 envelope");
    assert_eq!(error.kind, "unknown-field");
}

#[test]
fn v4_decoder_enforces_cross_row_structural_gates() {
    // Duplicate window lifetimes refuse across valid and held rows.
    let mut doc = fixture_value("messages/report-observe.json");
    let twin = doc["windows"][0].clone();
    doc["windows"].as_array_mut().expect("array").push(twin);
    let error = decode_report_v4(&doc.to_string()).expect_err("duplicate window refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-window");

    // Closed ids overlapping live windows refuse.
    let mut doc = fixture_value("messages/report-observe.json");
    let win = doc["windows"][0]["win"].as_str().expect("win").to_owned();
    doc["closed"] = serde_json::json!([win]);
    let error = decode_report_v4(&doc.to_string()).expect_err("closed overlap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "closed-overlaps-live");

    // Duplicate screen outputs refuse.
    let mut doc = fixture_value("messages/report-observe.json");
    let screen = doc["screens"][0].clone();
    doc["screens"].as_array_mut().expect("array").push(screen);
    let error = decode_report_v4(&doc.to_string()).expect_err("duplicate screen refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-screen");

    // Duplicate frame identities refuse.
    let mut doc = fixture_value("messages/report-attempt-frame-ordinary.json");
    let row = doc["frameAttempts"][0].clone();
    doc["frameAttempts"]
        .as_array_mut()
        .expect("array")
        .push(row);
    let error = decode_report_v4(&doc.to_string()).expect_err("duplicate frame refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-frame");
}

// ---- Unit 4a regression: attempt section caps count held rows ----

fn frame_attempt_row(win: usize, held: bool) -> serde_json::Value {
    let mut row = serde_json::json!({
        "win": format!("win-{win:05}"),
        "target": [8, 8, 800, 600],
        "setter": "accepted",
        "readback": [8, 8, 800, 600],
        "dpiBefore": null,
        "dpiAfter": null,
        "attemptMs": "20",
        "op": "",
        "pre": null,
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

fn creation_attempt_row(op: usize, held: bool) -> serde_json::Value {
    let mut row = serde_json::json!({
        "op": format!("create-{op:05}"),
        "ws": "ws-b",
        "out": "out-a",
        "setter": "accepted",
        "before": ["native-a"],
        "after": ["native-a", "native-b"],
        "attemptMs": "20",
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

fn member_attempt_row(i: usize, held: bool) -> serde_json::Value {
    let mut row = serde_json::json!({
        "op": format!("transfer-{i:05}"),
        "win": format!("win-{i:05}"),
        "out": "out-a",
        "ws": "ws-a",
        "setter": "accepted",
        "pre": {"kind": "native", "out": "out-a", "ws": "native-a"},
        "post": {"kind": "native", "out": "out-a", "ws": "native-a"},
        "attemptMs": "20",
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

fn view_attempt_row(i: usize, held: bool) -> serde_json::Value {
    let mut row = serde_json::json!({
        "op": format!("view-{i:05}"),
        "out": "out-a",
        "ws": "ws-a",
        "setter": "accepted",
        "pre": {"kind": "native", "out": "out-a", "ws": "native-a"},
        "post": {"kind": "native", "out": "out-a", "ws": "native-a"},
        "base": {"kind": "native", "out": "out-a", "ws": "native-a"},
        "attemptMs": "20",
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

fn clear_attempt_row(i: usize, held: bool) -> serde_json::Value {
    let mut row = serde_json::json!({
        "op": format!("clear-{i:05}"),
        "win": format!("win-{i:05}"),
        "setter": "refused",
        "pre": true,
        "post": true,
        "attemptMs": "20",
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

fn set_lane(doc: &mut serde_json::Value, lane: &str, rows: Vec<serde_json::Value>) {
    doc.as_object_mut().expect("object")[lane] = serde_json::Value::Array(rows);
}

#[test]
fn v4_attempt_section_caps_count_held_rows() {
    // Frame lane (cap 1024): at-cap mixed valid+held decodes.
    let mut doc = fixture_value("messages/report-attempt-frame-ordinary.json");
    set_lane(
        &mut doc,
        "frameAttempts",
        (0..1024)
            .map(|i| frame_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    let decoded = decode_report_v4(&doc.to_string()).expect("frame at-cap decodes");
    assert_eq!(decoded.report.frames.len(), 512);
    assert_eq!(
        decoded
            .report
            .held_attempts
            .iter()
            .filter(|held| held.lane == AttemptLaneV4::Frame)
            .count(),
        512
    );
    // Frame lane cap+1 mixed valid+held refuses.
    let mut doc = fixture_value("messages/report-attempt-frame-ordinary.json");
    set_lane(
        &mut doc,
        "frameAttempts",
        (0..1025)
            .map(|i| frame_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("frame cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
    // Frame lane cap+1 all-held refuses.
    let mut doc = fixture_value("messages/report-attempt-frame-ordinary.json");
    set_lane(
        &mut doc,
        "frameAttempts",
        (0..1025).map(|i| frame_attempt_row(i, true)).collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("frame all-held cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");

    // Creation lane (cap 256): at-cap mixed decodes.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    set_lane(
        &mut doc,
        "creationAttempts",
        (0..256)
            .map(|i| creation_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    // The base fixture carries a valid native row the creations settle
    // against; keep it and drop the base creation row count to the new lane.
    let decoded = decode_report_v4(&doc.to_string()).expect("creation at-cap decodes");
    assert_eq!(decoded.report.creation_attempts.len(), 128);
    // Creation lane cap+1 mixed refuses.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    set_lane(
        &mut doc,
        "creationAttempts",
        (0..257)
            .map(|i| creation_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("creation cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
    // Creation lane cap+1 all-held refuses.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    set_lane(
        &mut doc,
        "creationAttempts",
        (0..257).map(|i| creation_attempt_row(i, true)).collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("creation all-held cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");

    // Member/view/clear lanes (cap 1024 each): cap+1 mixed refuses.
    let mut doc = fixture_value("messages/report-attempt-member-native.json");
    set_lane(
        &mut doc,
        "memberAttempts",
        (0..1025)
            .map(|i| member_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("member cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
    let mut doc = fixture_value("messages/report-attempt-view-native.json");
    set_lane(
        &mut doc,
        "viewAttempts",
        (0..1025).map(|i| view_attempt_row(i, i % 2 == 0)).collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("view cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
    let mut doc = fixture_value("messages/report-attempt-clear.json");
    set_lane(
        &mut doc,
        "clearAttempts",
        (0..1025)
            .map(|i| clear_attempt_row(i, i % 2 == 0))
            .collect(),
    );
    let error = decode_report_v4(&doc.to_string()).expect_err("clear cap+1 refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

// ---- Unit 4a regression: native structural gates run before the hold ----

fn native_row(out: &str, order: Vec<String>, held: bool) -> serde_json::Value {
    let occupancy: Vec<serde_json::Value> = order
        .iter()
        .map(|id| serde_json::json!({"native": id, "count": 1}))
        .collect();
    let mut row = serde_json::json!({
        "out": out,
        "order": order,
        "current": order.first().cloned().unwrap_or_default(),
        "occupancy": occupancy,
    });
    if held {
        row.as_object_mut()
            .expect("object")
            .insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

#[test]
fn v4_native_structural_gates_run_before_hold() {
    // Hold trigger plus per-output order over-cap refuses.
    let big: Vec<String> = (0..257).map(|i| format!("native-{i:03}")).collect();
    let mut doc = fixture_value("messages/report-native.json");
    doc["native"] = serde_json::Value::Array(vec![native_row("out-a", big, true)]);
    let error = decode_report_v4(&doc.to_string()).expect_err("held over-cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "native-invalid");
    // Hold trigger plus duplicate order ids refuses.
    let mut doc = fixture_value("messages/report-native.json");
    doc["native"] = serde_json::Value::Array(vec![native_row(
        "out-a",
        vec!["native-a".to_owned(), "native-a".to_owned()],
        true,
    )]);
    let error = decode_report_v4(&doc.to_string()).expect_err("held duplicate refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-native");
    // Hold trigger plus occupancy coverage gap refuses.
    let mut doc = fixture_value("messages/report-native.json");
    let mut row = native_row(
        "out-a",
        vec!["native-a".to_owned(), "native-b".to_owned()],
        true,
    );
    row.as_object_mut().expect("object")["occupancy"] =
        serde_json::json!([{"native": "native-a", "count": 1}]);
    doc["native"] = serde_json::Value::Array(vec![row]);
    let error = decode_report_v4(&doc.to_string()).expect_err("held coverage gap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "native-occupancy-incomplete");
    // Hold trigger on an unknown output refuses at the binding gate.
    let mut doc = fixture_value("messages/report-native.json");
    doc["native"] = serde_json::Value::Array(vec![native_row(
        "out-zzz",
        vec!["native-a".to_owned()],
        true,
    )]);
    let error = decode_report_v4(&doc.to_string()).expect_err("held unknown output refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "native-output-unknown");
    // A valid native on a held screen is accepted: held screens are known.
    let mut doc = fixture_value("messages/report-native.json");
    doc["screens"][0]
        .as_object_mut()
        .expect("object")
        .insert("gap".to_owned(), serde_json::json!(8));
    let decoded = decode_report_v4(&doc.to_string()).expect("native on held screen decodes");
    assert_eq!(decoded.report.screens.len(), 0);
    assert_eq!(decoded.report.held_screens.len(), 1);
    assert_eq!(decoded.report.native.len(), 1);
    assert!(decoded.report.held_native.is_empty());
    // Global native-id budget boundary: 16 outputs times 256 ids is exact.
    let mut doc = fixture_value("messages/report-native.json");
    let mut screens = Vec::new();
    let mut natives = Vec::new();
    for output in 0..16 {
        let out = format!("out-{output:02}");
        let mut screen = doc["screens"][0].clone();
        screen.as_object_mut().expect("object")["out"] = serde_json::Value::String(out.clone());
        screens.push(screen);
        let order: Vec<String> = (0..256)
            .map(|i| format!("native-{output:02}-{i:03}"))
            .collect();
        natives.push(native_row(&out, order, false));
    }
    doc["screens"] = serde_json::Value::Array(screens);
    doc["native"] = serde_json::Value::Array(natives);
    let decoded = decode_report_v4(&doc.to_string()).expect("4096 native ids decode");
    assert_eq!(
        decoded
            .report
            .native
            .iter()
            .map(|row| row.native_order.len())
            .sum::<usize>(),
        4096
    );
}

// ---- Unit 4a regression: cursor-hit and action-output cross-checks ----

#[test]
fn v4_common_cross_checks_before_accept() {
    // Cursor hit without a cursor refuses.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["cursor"] = serde_json::Value::Null;
    doc["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("hit without cursor refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "cursor-hit-without-cursor");
    // Cursor hit naming no valid window refuses.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["cursorHit"] = serde_json::Value::String("win-zzz".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("unknown hit refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "cursor-hit-unknown");
    // Cursor hit naming only a held window refuses: held rows are not reported.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["windows"][0]
        .as_object_mut()
        .expect("object")
        .insert("floating".to_owned(), serde_json::Value::Bool(false));
    doc["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("held-only hit refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "cursor-hit-unknown");
    // Duplicate action outputs refuse.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["actionOutputs"] = serde_json::json!(["out-a", "out-a"]);
    let error = decode_report_v4(&doc.to_string()).expect_err("duplicate outputs refuse");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-action-output");
    // Healthy cursor hit plus unique outputs still decode.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["cursorHit"] = serde_json::Value::String("win-a".to_owned());
    let decoded = decode_report_v4(&doc.to_string()).expect("healthy hit decodes");
    assert_eq!(
        decoded.report.cursor_hit.as_ref().map(|hit| hit.0.as_str()),
        Some("win-a")
    );
}

// ---- Unit 4a regression: creation invariants span held rows ----

fn creation_row_full(
    op: &str,
    ws: Option<&str>,
    before: Vec<serde_json::Value>,
    after: Vec<serde_json::Value>,
    created: serde_json::Value,
    held_extra: bool,
) -> serde_json::Value {
    let mut row = serde_json::json!({
        "op": op,
        "out": "out-a",
        "setter": "accepted",
        "before": before,
        "after": after,
        "created": created,
        "attemptMs": "20",
    });
    let map = row.as_object_mut().expect("object");
    if let Some(ws) = ws {
        map.insert("ws".to_owned(), serde_json::Value::String(ws.to_owned()));
    }
    if held_extra {
        map.insert("zzzExtra".to_owned(), serde_json::Value::Bool(true));
    }
    row
}

#[test]
fn v4_creation_invariants_span_held_rows() {
    // Held row with duplicate ids inside `before` refuses.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            vec![serde_json::json!("native-a"), serde_json::json!("native-a")],
            vec![],
            serde_json::Value::Null,
            true,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("held before-dupe refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-creation-attempt");
    // A created receipt reused by a distinct held op refuses.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            vec![serde_json::json!("native-a")],
            vec![serde_json::json!("native-a")],
            serde_json::Value::String("native-b".to_owned()),
            true,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("reused receipt refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-receipt-reused");
    // Held row repeating the op without ws/out still joins op uniqueness.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-a",
            None,
            vec![serde_json::json!("native-a")],
            vec![serde_json::json!("native-a")],
            serde_json::Value::Null,
            false,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("same op without triple refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-creation-attempt");
    // Full triple values that differ keep the mismatch refusal.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-a",
            Some("ws-c"),
            vec![serde_json::json!("native-a")],
            vec![serde_json::json!("native-a")],
            serde_json::Value::Null,
            false,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("triple mismatch refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-triple-mismatch");
    // Bad before ids are not hidden by a wrong-typed receipt plus extras.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            vec![serde_json::json!("!!!")],
            vec![],
            serde_json::Value::Bool(false),
            true,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("bad id behind receipt refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-id-invalid");
    // Per-row order over-cap is not hidden by a wrong-typed receipt.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    let wide: Vec<serde_json::Value> = (0..257)
        .map(|i| serde_json::Value::String(format!("native-{i:03}")))
        .collect();
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            wide,
            vec![],
            serde_json::Value::Bool(false),
            false,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("held order over-cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-order-over-cap");
    // Bad-grammar receipt string behind extras refuses as a bad id.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            vec![serde_json::json!("native-a")],
            vec![serde_json::json!("native-a")],
            serde_json::Value::String("!!!".to_owned()),
            true,
        ));
    let error = decode_report_v4(&doc.to_string()).expect_err("bad receipt refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-id-invalid");
    // Aggregate before+after+created budget: 16 full rows (8192 ids) decode.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    let before: Vec<serde_json::Value> = (0..256)
        .map(|i| serde_json::Value::String(format!("native-b-{i:03}")))
        .collect();
    let after: Vec<serde_json::Value> = (0..256)
        .map(|i| serde_json::Value::String(format!("native-c-{i:03}")))
        .collect();
    let rows: Vec<serde_json::Value> = (0..16)
        .map(|i| {
            creation_row_full(
                &format!("create-{i:02}"),
                Some("ws-b"),
                before.clone(),
                after.clone(),
                serde_json::Value::Null,
                false,
            )
        })
        .collect();
    doc["creationAttempts"] = serde_json::Value::Array(rows);
    decode_report_v4(&doc.to_string()).expect("8192 creation ids decode");
    // One more full row (8704 ids) refuses over the aggregate budget.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    let rows: Vec<serde_json::Value> = (0..17)
        .map(|i| {
            creation_row_full(
                &format!("create-{i:02}"),
                Some("ws-b"),
                before.clone(),
                after.clone(),
                serde_json::Value::Null,
                i % 2 == 0,
            )
        })
        .collect();
    doc["creationAttempts"] = serde_json::Value::Array(rows);
    let error = decode_report_v4(&doc.to_string()).expect_err("creation over-cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-over-cap");
}

// ---- Unit 4a regression: presentation preflight runs before the hold ----

fn observe_with_presentation_hold() -> serde_json::Value {
    let mut doc = fixture_value("messages/report-observe.json");
    doc["presentation"]
        .as_object_mut()
        .expect("object")
        .insert("borderEnabled".to_owned(), serde_json::Value::Bool(true));
    doc
}

#[test]
fn v4_presentation_structural_preflight_before_hold() {
    // Hold trigger plus bad foreground identity refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"].as_object_mut().expect("object")["foreground"] =
        serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad foreground refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad moveWindow identity refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"].as_object_mut().expect("object")["moveWindow"] =
        serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad moveWindow refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad pager order identity refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"].as_object_mut().expect("object")["pagerOrder"] = serde_json::json!(["!!!"]);
    let error = decode_report_v4(&doc.to_string()).expect_err("bad pager order refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad pager frame window refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"]["pagerFrames"][0]
        .as_object_mut()
        .expect("object")["win"] = serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad pager win refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad foregroundOut identity refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"].as_object_mut().expect("object")["foregroundOut"] =
        serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad foregroundOut refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad witness-object identity refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"]["witness"]
        .as_object_mut()
        .expect("object")["win"] = serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad witness win refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad witness frame geometry refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"]["witness"]
        .as_object_mut()
        .expect("object")["frame"] = serde_json::json!([8, 8, 0, 0]);
    let error = decode_report_v4(&doc.to_string()).expect_err("bad witness frame refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad witness counter refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"]["witness"]
        .as_object_mut()
        .expect("object")["obs"] = serde_json::Value::String("01".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("bad witness counter refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus bad foreground frame geometry refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"].as_object_mut().expect("object")["frame"] = serde_json::json!([8, 8, 0, 0]);
    let error = decode_report_v4(&doc.to_string()).expect_err("bad frame refuses");
    assert_eq!(error.kind, "unknown-value");
    // Hold trigger plus a missing required key refuses.
    let mut doc = observe_with_presentation_hold();
    doc["presentation"]
        .as_object_mut()
        .expect("object")
        .remove("foreground");
    let error = decode_report_v4(&doc.to_string()).expect_err("missing key refuses");
    assert_eq!(error.kind, "malformed");
    // A constant STRING witness cannot be inspected, but the surrounding
    // known fields still preflight: bad foreground still refuses.
    let mut doc = observe_with_presentation_hold();
    let presentation = doc["presentation"].as_object_mut().expect("object");
    presentation["witness"] = serde_json::Value::String("constant".to_owned());
    presentation["foreground"] = serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("string witness hides nothing");
    assert_eq!(error.kind, "unknown-value");
    // Pager float echo holds, but the surrounding known fields still
    // preflight: bad foreground still refuses.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["presentation"]["pagerFrames"][0]
        .as_object_mut()
        .expect("object")
        .insert("float".to_owned(), serde_json::json!([8, 8, 800, 600]));
    doc["presentation"].as_object_mut().expect("object")["foreground"] =
        serde_json::Value::String("!!!".to_owned());
    let error = decode_report_v4(&doc.to_string()).expect_err("float echo hides nothing");
    assert_eq!(error.kind, "unknown-value");
    // A constant STRING witness alone stays held: its fields are unavailable.
    let decoded = decode_ok("messages/invalid-witness-constant-string.json");
    assert!(
        matches!(decoded.report.presentation, PresentationRowV4::Held(_)),
        "string witness holds"
    );
    // A well-formed but dangling witness stays valid shape-only.
    let mut doc = fixture_value("messages/report-observe.json");
    doc["presentation"].as_object_mut().expect("object")["witness"] = serde_json::json!({
        "win": "win-zzz",
        "obs": "7",
        "frame": [8, 8, 800, 600],
        "foreground": "win-zzz",
    });
    let decoded = decode_report_v4(&doc.to_string()).expect("dangling witness decodes");
    assert!(
        matches!(decoded.report.presentation, PresentationRowV4::Valid(_)),
        "dangling witness is valid"
    );
}

// ---- Unit 4b: staged canonical v4 report writer ----

/// Canonical round trip over every accepted fixture (126 valid-wire plus
/// 81 held accepted): decode, re-encode, re-decode, and the value must
/// equal the first decode and stay accepted. The 205 canonical fixtures
/// are byte-exact; the two non-canonical decoder-input fixtures (short
/// escape spellings, and an object-valued window rect whose nested key order
/// no contract rule fixes) assert decoded-value equality only.
#[test]
fn v4_writer_round_trips_all_accepted_fixtures() {
    let fixtures = world_fixtures();
    let accepted: Vec<&ManifestFixture> = fixtures
        .iter()
        .filter(|fixture| {
            fixture.expectation == "valid-wire" || fixture.expectation == "accepted-held-row"
        })
        .collect();
    assert_eq!(accepted.len(), 207, "accepted fixture count");
    for fixture in accepted {
        let text = fixture_text(&fixture.file);
        let decoded = decode_report_v4(&text).expect("accepted fixture decodes");
        let encoded = encode_report_v4(&decoded).unwrap_or_else(|error| {
            panic!(
                "{} encodes, got {}:{}",
                fixture.id, error.kind, error.message
            )
        });
        let back = decode_report_v4(&encoded).expect("encoding stays accepted");
        assert_eq!(back, decoded, "{} decode stable", fixture.id);
        if fixture.canonical {
            assert_eq!(encoded, text, "{} byte-exact", fixture.id);
        }
    }
}

/// Exactly two accepted fixtures are decoder input rather than canonical
/// bytes: the text-escaping relationships row, whose short escapes decode
/// to the exact C0 controls the canonical writer spells as lowercase
/// `\u00XX`, and the object-valued window rect row, whose nested key order
/// the contract does not fix.
#[test]
fn v4_writer_non_canonical_fixture_is_the_escaping_input() {
    let fixtures = world_fixtures();
    let accepted: Vec<&ManifestFixture> = fixtures
        .iter()
        .filter(|fixture| {
            fixture.expectation == "valid-wire" || fixture.expectation == "accepted-held-row"
        })
        .collect();
    assert_eq!(accepted.len(), 207, "accepted fixture count");
    let non_canonical: Vec<&ManifestFixture> = accepted
        .iter()
        .filter(|fixture| !fixture.canonical)
        .copied()
        .collect();
    let mut ids: Vec<&str> = non_canonical
        .iter()
        .map(|fixture| fixture.id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "invalid-window-rect-object",
            "report-text-escaping-relationships"
        ],
        "non-canonical accepted fixtures"
    );
    let escaping = non_canonical
        .iter()
        .find(|fixture| fixture.id == "report-text-escaping-relationships")
        .expect("escaping fixture");
    let decoded = decode_ok(&escaping.file);
    assert_eq!(
        decoded.report.windows[0].title.as_deref(),
        Some("Quote \" backslash \\ tab\t newline\n return\r backspace\u{8} formfeed\u{c} nul\0"),
        "title decodes to exact controls",
    );
}

/// The writer refuses inconsistent metadata instead of emitting non-wire
/// bytes: every refusal is `snapshot-invalid` with a fixed message and
/// never echoes payload bytes.
#[test]
fn v4_writer_refuses_inconsistent_metadata() {
    // Creation receipts align to the valid creation rows in order.
    let mut decoded = decode_ok("messages/report-attempt-creation-id.json");
    decoded.creation_receipts.pop();
    let error = encode_report_v4(&decoded).expect_err("receipt count refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-receipt-mismatch");
    // An `Id` receipt with no correlated id refuses.
    let mut decoded = decode_ok("messages/report-attempt-creation-id.json");
    decoded.report.creation_attempts[0].created_native = None;
    let error = encode_report_v4(&decoded).expect_err("id without value refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-receipt-inconsistent");
    // An absent receipt with a correlated id refuses.
    let mut decoded = decode_ok("messages/report-attempt-creation-omitted.json");
    decoded.report.creation_attempts[0].created_native = Some("native-b".to_owned());
    let error = encode_report_v4(&decoded).expect_err("value without id refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "creation-receipt-inconsistent");
    // Held ordinals must be unique and inside the merged lane.
    let mut decoded = decode_ok("messages/invalid-frame-missing-op.json");
    decoded.report.held_attempts[0].ordinal = 999;
    let error = encode_report_v4(&decoded).expect_err("held ordinal refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "held-ordinal-invalid");
    // A cursor hit needs the `Id` spelling; a missing hit needs none.
    let mut decoded = decode_ok("messages/report-optionals-id.json");
    decoded.wire.cursor_hit = tiler_protocol::world_protocol::CursorHitWire::Null;
    let error = encode_report_v4(&decoded).expect_err("hit without id refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "cursor-hit-inconsistent");
    let mut decoded = decode_ok("messages/report-observe.json");
    decoded.wire.cursor_hit = tiler_protocol::world_protocol::CursorHitWire::Id;
    let error = encode_report_v4(&decoded).expect_err("id without hit refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "cursor-hit-inconsistent");
    // Gesture ingress presence must match the carried value.
    let mut decoded = decode_ok("messages/report-gesture-started.json");
    decoded.wire.gesture_ingress = tiler_protocol::world_protocol::GestureIngressWire::Absent;
    let error = encode_report_v4(&decoded).expect_err("ingress without presence refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "gesture-ingress-inconsistent");
    let mut decoded = decode_ok("messages/report-observe.json");
    decoded.wire.gesture_ingress = tiler_protocol::world_protocol::GestureIngressWire::Present;
    let error = encode_report_v4(&decoded).expect_err("presence without ingress refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "gesture-ingress-inconsistent");
    // Carried attempt rows need their lane; a nonzero clock needs its key.
    let mut decoded = decode_ok("messages/report-attempt-frame-ordinary.json");
    decoded.wire.frame_attempts_present = false;
    let error = encode_report_v4(&decoded).expect_err("lane without key refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "attempt-lane-inconsistent");
    let mut decoded = decode_ok("messages/report-empty.json");
    decoded.report.monotonic_ms = 42;
    let error = encode_report_v4(&decoded).expect_err("clock without key refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "monotonic-inconsistent");
    // The v4 focus shape spells absence as the empty direction; a nonempty
    // direction, the removed switch-view, and unpromoted core operations
    // refuse instead of emitting unknown wire shapes.
    let mut decoded = decode_ok("messages/report-command-focus.json");
    decoded.report.command = Some(tiler_core::world_report::WorldCommand::Focus {
        window: tiler_core::directional::WindowId("win-a".to_owned()),
        direction: "left".to_owned(),
    });
    let error = encode_report_v4(&decoded).expect_err("focus direction refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "command-invalid");
    let mut decoded = decode_ok("messages/report-empty.json");
    decoded.report.command_seq = Some(1);
    decoded.report.command = Some(tiler_core::world_report::WorldCommand::SwitchView {
        output: tiler_core::directional::OutputId("out-a".to_owned()),
        workspace: "ws-a".to_owned(),
    });
    let error = encode_report_v4(&decoded).expect_err("switch-view refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "command-invalid");
    let mut decoded = decode_ok("messages/report-empty.json");
    decoded.report.command_seq = Some(1);
    decoded.report.command = Some(tiler_core::world_report::WorldCommand::Operation(
        tiler_core::boundary::CoreCommand::Reconcile,
    ));
    let error = encode_report_v4(&decoded).expect_err("unpromoted operation refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "command-invalid");
    // Command/sequence pairing and renderer/command pairing refuse.
    let mut decoded = decode_ok("messages/report-command-focus.json");
    decoded.report.command_seq = None;
    let error = encode_report_v4(&decoded).expect_err("unpaired command refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "command-pair-invalid");
    let mut decoded = decode_ok("messages/report-renderer-window.json");
    decoded.report.command = None;
    decoded.report.command_seq = None;
    let error = encode_report_v4(&decoded).expect_err("renderer without command refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "command-pair-invalid");
    // A non-finite scale never normalizes to null on the wire.
    let mut decoded = decode_ok("messages/report-observe.json");
    decoded.report.screens[0].scale = f64::INFINITY;
    let error = encode_report_v4(&decoded).expect_err("nonfinite refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "nonfinite");
    // An ungrammatical id fails the self-check instead of emitting.
    let mut decoded = decode_ok("messages/report-command-focus.json");
    decoded.report.command = Some(tiler_core::world_report::WorldCommand::Focus {
        window: tiler_core::directional::WindowId("!!!".to_owned()),
        direction: String::new(),
    });
    let error = encode_report_v4(&decoded).expect_err("bad id refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "encode-mismatch");
}

/// The writer spells the decoded value, never the original bytes: edited
/// documents that still decode must re-encode to the edited bytes.
#[test]
fn v4_writer_spells_the_value_not_the_bytes() {
    let empty = fixture_text("messages/report-empty.json");
    let edited = empty.replacen("\"req\":\"request-a\"", "\"req\":\"request-b\"", 1);
    assert_ne!(edited, empty);
    let decoded = decode_report_v4(&edited).expect("edited decodes");
    assert_eq!(encode_report_v4(&decoded).expect("edits encode"), edited);
    let optionals = fixture_text("messages/report-optionals-id.json");
    let edited = optionals.replacen(
        "\"monotonicMs\":\"18446744073709551615\"",
        "\"monotonicMs\":\"42\"",
        1,
    );
    assert_ne!(edited, optionals);
    let decoded = decode_report_v4(&edited).expect("edited clock decodes");
    assert_eq!(
        encode_report_v4(&decoded).expect("edited clock encodes"),
        edited
    );
}

/// Order-preserving scan of the compact writer output: top-level `"key":`
/// names in emission order (`serde_json::Map` sorts, so it cannot observe
/// order).
fn top_level_keys(json: &str) -> Vec<String> {
    let bytes = json.as_bytes();
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'"' {
            let mut key = String::new();
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                if bytes[j] == b'\\' {
                    key.push(bytes[j + 1] as char);
                    j += 2;
                } else {
                    key.push(bytes[j] as char);
                    j += 1;
                }
            }
            let mut k = j + 1;
            while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            if depth == 1 && k < bytes.len() && bytes[k] == b':' {
                keys.push(key);
            }
            i = j + 1;
            continue;
        }
        match byte {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    keys
}

/// Contract key order wins over emission convenience: `primary` sits after
/// the attempt lanes and `gestureIngress`, before `intentRecords`
/// (docs/porting/world-contract.md message field order is table order).
#[test]
fn v4_writer_primary_key_order_follows_contract() {
    let mut decoded = decode_ok("messages/report-optionals-id.json");
    assert!(
        decoded.report.primary_output.is_none(),
        "base carries no primary"
    );
    decoded.report.primary_output = Some(tiler_core::directional::OutputId("out-a".to_owned()));
    let encoded = encode_report_v4(&decoded).expect("primary with lanes encodes");
    let keys = top_level_keys(&encoded);
    let expected: Vec<String> = [
        "v",
        "adapter",
        "gen",
        "obs",
        "reqSeq",
        "req",
        "cmdSeq",
        "command",
        "screens",
        "windows",
        "closed",
        "native",
        "focused",
        "complete",
        "gesture",
        "cursor",
        "actionForeground",
        "actionCached",
        "actionOutputs",
        "presentation",
        "cursorHit",
        "monotonicMs",
        "frameAttempts",
        "memberAttempts",
        "viewAttempts",
        "clearAttempts",
        "creationAttempts",
        "gestureIngress",
        "primary",
        "intentRecords",
        "stateAttempts",
        "focusFeedback",
        "rendererInput",
    ]
    .iter()
    .map(|key| key.to_string())
    .collect();
    assert_eq!(keys, expected, "contract key order");
    let back = decode_report_v4(&encoded).expect("encoding decodes");
    assert_eq!(back, decoded, "decode stable");
}

/// Byte-level C0 witness: the escaping title re-encodes with lowercase
/// `\u00XX` escapes and no short `\t \n \r \b \f` spellings. The title
/// also carries a literal backslash (escaped `\\` plus a space), so the
/// assertion pins the exact encoded title substring instead of a loose
/// search.
#[test]
fn v4_writer_spells_c0_controls_as_lowercase_unicode_escapes() {
    let text = fixture_text("messages/report-text-escaping-relationships.json");
    let decoded = decode_report_v4(&text).expect("escaping fixture decodes");
    let encoded = encode_report_v4(&decoded).expect("escaping value encodes");
    let expected_title = "\"title\":\"Quote \\\" backslash \\\\ tab\\u0009 newline\\u000a return\\u000d backspace\\u0008 formfeed\\u000c nul\\u0000\"";
    let start = encoded.find("\"title\":\"Quote").expect("title present");
    let region = &encoded[start..start + expected_title.len()];
    assert_eq!(region, expected_title, "canonical C0 escapes");
    for short in ["\\t", "\\n", "\\r", "\\b", "\\f"] {
        assert!(
            !region.contains(short),
            "no short escape {short:?} in canonical title"
        );
    }
}

// ---- Unit 4a: creation-receipt wire spellings stay lossless ----

#[test]
fn v4_creation_receipt_wire_spellings() {
    // The four immutable valid fixtures pin the spelling plus the
    // normalized receipt: the first three normalize to none, the id
    // carries through.
    for (file, spelling, receipt) in [
        (
            "messages/report-attempt-creation-omitted.json",
            CreationReceiptWire::Absent,
            None,
        ),
        (
            "messages/report-attempt-creation-null.json",
            CreationReceiptWire::Null,
            None,
        ),
        (
            "messages/report-attempt-creation-empty.json",
            CreationReceiptWire::Empty,
            None,
        ),
        (
            "messages/report-attempt-creation-id.json",
            CreationReceiptWire::Id,
            Some("native-b"),
        ),
    ] {
        let decoded = decode_ok(file);
        assert_eq!(decoded.report.creation_attempts.len(), 1, "{file}");
        assert_eq!(decoded.creation_receipts, vec![spelling], "{file}");
        assert_eq!(
            decoded.report.creation_attempts[0]
                .created_native
                .as_deref(),
            receipt,
            "{file}"
        );
    }
    // Mixed valid plus held rows align the metadata to the valid rows in
    // order: held rows keep raw plus ordinal instead.
    let mut doc = fixture_value("messages/report-attempt-creation-id.json");
    doc["creationAttempts"]
        .as_array_mut()
        .expect("array")
        .push(creation_row_full(
            "create-b",
            Some("ws-b"),
            vec![serde_json::json!("native-a")],
            vec![serde_json::json!("native-a")],
            serde_json::Value::Bool(false),
            false,
        ));
    let decoded = decode_report_v4(&doc.to_string()).expect("mixed creations decode");
    assert_eq!(decoded.report.creation_attempts.len(), 1);
    assert_eq!(decoded.report.held_attempts.len(), 1);
    assert_eq!(decoded.report.held_attempts[0].ordinal, 1);
    assert_eq!(decoded.creation_receipts, vec![CreationReceiptWire::Id]);
}
