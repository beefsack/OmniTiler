//! Typed Settings recorder and byte-exact verifier for `test-fixtures/world-wire/v4-active`
//! standalone Settings messages, scenario Settings inputs, and `tests/data/world_eval_v4.jsonl`.
//!
//! Every Settings payload is constructed in Rust as an explicit typed [`SettingsRequest`] and
//! encoded via [`encode_settings`] (never read from old fixtures or key-patched).
//!
//! Normal mode asserts committed files match fresh [`encode_settings`] bytes.
//! Explicit re-recording uses `UPDATE_SETTINGS_V4_FIXTURES=1` and preserves all surrounding
//! scenario formatting and JSONL lines verbatim.

use std::fs;
use std::path::PathBuf;
use tiler_core::connection::{RawSettingValue, SettingsRequest};
use tiler_protocol::connection_protocol::encode_settings;

fn v4_active_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4-active")
}

fn world_eval_jsonl_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/world_eval_v4.jsonl")
}

fn s(v: &str) -> RawSettingValue {
    RawSettingValue::String(v.to_owned())
}

fn n(v: f64) -> RawSettingValue {
    RawSettingValue::Number(v)
}

fn b(v: bool) -> RawSettingValue {
    RawSettingValue::Bool(v)
}

fn default_values() -> Vec<(String, RawSettingValue)> {
    vec![
        ("gap".into(), n(8.0)),
        ("outer".into(), n(8.0)),
        ("defaultTiled".into(), b(true)),
        ("mode".into(), s("per-output-local")),
        ("sameAxis".into(), s("group-with-neighbor")),
        ("bornMaximized".into(), s("tile-on-launch")),
        ("predicate".into(), s("both-axes-fixed")),
        ("refill".into(), s("last-remaining-workspace")),
        ("switchTarget".into(), s("cursor-output")),
        ("freshPlacement".into(), s("cursor-output-last-focused")),
        ("focusFollowsCursor".into(), b(false)),
        ("cursorWarp".into(), b(false)),
        ("borderEnabled".into(), b(true)),
        ("underlayEnabled".into(), b(true)),
        ("borderWidth".into(), n(3.0)),
        ("borderGap".into(), n(0.0)),
        ("borderRadius".into(), n(6.0)),
        ("underlayExtension".into(), n(-1.0)),
    ]
}

fn with_overrides(overrides: &[(&str, RawSettingValue)]) -> Vec<(String, RawSettingValue)> {
    let mut values = default_values();
    for (k, v) in overrides {
        if let Some((_, existing)) = values.iter_mut().find(|(ek, _)| ek == k) {
            *existing = v.clone();
        } else {
            values.push(((*k).to_owned(), v.clone()));
        }
    }
    values
}

fn req(
    generation: &str,
    request: &str,
    revision: u64,
    values: Vec<(String, RawSettingValue)>,
) -> SettingsRequest {
    SettingsRequest {
        adapter: "adapter-a".to_owned(),
        generation: generation.to_owned(),
        request: request.to_owned(),
        revision,
        values,
    }
}

fn standalone_cases() -> Vec<(&'static str, SettingsRequest)> {
    // Deliberate invalid/missing/unknown fields for `settings-invalid-fields`:
    // - `gap`: wrong-type ("8")
    // - `outer`: out-of-range (65)
    // - `mode`: unknown-value ("unknown")
    // - `predicate`: omitted (missing)
    // - `borderWidth`: wrong-type (null)
    // - `borderRadius`: valid sibling (0.5)
    // - `unknownSetting`: explicit unknown-field (true)
    // `anchorFallback` and `noSpaceResult` are intentionally omitted here because `settings-invalid-fields`
    // pairs with frozen `../v4/messages/settings-reply-invalid-fields.json`; removed behavior is covered by
    // `settings_state.rs::removed_anchor_fallback_is_unknown_field_and_retains_valid_siblings`,
    // `settings_state.rs::removed_no_space_result_is_unknown_field_and_retains_valid_siblings`,
    // `world_connection_tests.rs::fixed_largest_selection_despite_removed_anchor_fallback_values`, and
    // `world_connection_tests.rs::fixed_float_exhaustion_despite_removed_no_space_result_values`.
    let mut invalid_values = with_overrides(&[
        ("gap", s("8")),
        ("outer", n(65.0)),
        ("mode", s("unknown")),
        ("borderWidth", RawSettingValue::Null),
        ("borderRadius", n(0.5)),
        ("unknownSetting", b(true)),
    ]);
    invalid_values.retain(|(k, _)| k != "predicate");

    vec![
        (
            "settings-defaults",
            req("generation-a", "settings-a", 1, default_values()),
        ),
        (
            "settings-alternatives",
            req(
                "generation-a",
                "settings-a",
                1,
                with_overrides(&[
                    ("gap", n(0.0)),
                    ("outer", n(64.0)),
                    ("defaultTiled", b(false)),
                    ("mode", s("global-unique")),
                    ("sameAxis", s("swap-with-neighbor")),
                    ("bornMaximized", s("keep-maximized-overlay")),
                    ("predicate", s("either-axis-fixed")),
                    ("refill", s("most-recently-used-workspace")),
                    ("switchTarget", s("focused-output")),
                    ("freshPlacement", s("cursor-output-cursor-closest")),
                    ("focusFollowsCursor", b(true)),
                    ("cursorWarp", b(true)),
                    ("borderEnabled", b(false)),
                    ("underlayEnabled", b(false)),
                    ("borderWidth", n(0.5)),
                    ("borderGap", n(1.25)),
                    ("borderRadius", n(2.5)),
                    ("underlayExtension", n(0.0)),
                ]),
            ),
        ),
        (
            "settings-third-arms",
            req(
                "generation-a",
                "settings-a",
                1,
                with_overrides(&[
                    ("mode", s("shared")),
                    ("sameAxis", s("swap-with-neighbor")),
                    ("bornMaximized", s("keep-maximized-overlay")),
                    ("predicate", s("either-axis-fixed")),
                    ("refill", s("most-recently-used-workspace")),
                    ("switchTarget", s("focused-output")),
                    ("freshPlacement", s("focused-output-last-focused")),
                ]),
            ),
        ),
        (
            "settings-invalid-fields",
            req("generation-a", "settings-a", 2, invalid_values),
        ),
        (
            "settings-restart-only",
            req(
                "generation-a",
                "settings-a",
                2,
                with_overrides(&[("mode", s("shared"))]),
            ),
        ),
    ]
}

fn scenario_cases() -> Vec<(&'static str, &'static str, SettingsRequest)> {
    vec![
        (
            "bootstrap-settings",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "bootstrap-settings",
            "change",
            req(
                "generation-a",
                "settings-2",
                2,
                with_overrides(&[("gap", n(12.0)), ("borderWidth", n(0.5))]),
            ),
        ),
        (
            "bootstrap-settings",
            "duplicate-settings",
            req(
                "generation-a",
                "settings-2",
                2,
                with_overrides(&[("gap", n(12.0)), ("borderWidth", n(0.5))]),
            ),
        ),
        (
            "bootstrap-settings",
            "invalid-siblings",
            // Deliberate invalid/unknown fields for `bootstrap-settings/invalid-siblings`:
            // `gap` wrong-type ("bad") and `unknownSetting` unknown-field alongside valid `outer: 10`.
            req(
                "generation-a",
                "settings-3",
                3,
                with_overrides(&[
                    ("gap", s("bad")),
                    ("outer", n(10.0)),
                    ("unknownSetting", b(true)),
                ]),
            ),
        ),
        (
            "bootstrap-settings",
            "older-settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "bootstrap-settings",
            "equivocating-settings",
            req(
                "generation-a",
                "settings-3",
                3,
                with_overrides(&[("gap", n(13.0))]),
            ),
        ),
        (
            "bootstrap-settings",
            "restart-setting",
            req(
                "generation-a",
                "settings-4",
                4,
                with_overrides(&[("gap", n(12.0)), ("outer", n(10.0)), ("mode", s("shared"))]),
            ),
        ),
        (
            "executable-identity",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "linked-selectors-warp",
            "requested-not-substituted",
            req(
                "generation-a",
                "requested-not-substituted",
                1,
                with_overrides(&[("cursorWarp", b(true))]),
            ),
        ),
        (
            "linked-selectors-warp",
            "raw-request-reused",
            req(
                "generation-b",
                "raw-request-reused",
                1,
                with_overrides(&[("cursorWarp", b(true))]),
            ),
        ),
        (
            "native-domain",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "persistent-above",
            "settings",
            req("generation-a", "settings", 1, default_values()),
        ),
        (
            "persistent-above",
            "unsupported-settings",
            req("generation-b", "unsupported-settings", 1, default_values()),
        ),
        (
            "raw-attempt-settlement",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "raw-fact-availability",
            "supported-settings",
            req("generation-a", "supported-settings", 1, default_values()),
        ),
        (
            "raw-fact-availability",
            "unavailable-settings",
            req("generation-b", "unavailable-settings", 1, default_values()),
        ),
        (
            "visibility-cross-output-supersession",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "visibility-order-failure",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "window-state",
            "settings",
            req("generation-a", "settings-1", 1, default_values()),
        ),
        (
            "window-state",
            "sticky-unsupported-settings",
            req("generation-b", "settings-1", 1, default_values()),
        ),
        (
            "window-state",
            "native-sticky-settings",
            req("generation-c", "settings-1", 1, default_values()),
        ),
    ]
}

fn world_eval_cases() -> Vec<(usize, SettingsRequest)> {
    vec![
        (2, req("world-eval-0", "settings-1", 1, default_values())),
        (8, req("world-eval-0", "settings-1", 1, default_values())),
        (10, req("world-eval-0-1", "settings-1", 1, default_values())),
    ]
}

/// Pretty-print compact JSON from `encode_settings` with 2-space indents at `base_indent`,
/// preserving the exact key order emitted by the encoder.
fn pretty_compact_json_preserving_order(compact: &str, base_indent: usize) -> String {
    let bytes = compact.as_bytes();
    let mut out = String::with_capacity(compact.len() * 2);
    let mut i = 0;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    let push_indent = |buf: &mut String, d: usize| {
        for _ in 0..(base_indent + d * 2) {
            buf.push(' ');
        }
    };

    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push('"');
            }
            '{' => {
                out.push('{');
                if bytes.get(i + 1) == Some(&b'}') {
                    out.push('}');
                    i += 2;
                    continue;
                }
                depth += 1;
                out.push('\n');
                push_indent(&mut out, depth);
            }
            '}' => {
                depth -= 1;
                out.push('\n');
                push_indent(&mut out, depth);
                out.push('}');
            }
            ',' => {
                out.push(',');
                out.push('\n');
                push_indent(&mut out, depth);
            }
            ':' => {
                out.push_str(": ");
            }
            _ => out.push(ch),
        }
        i += 1;
    }
    out
}

fn find_step_message_span(text: &str, step_id: &str) -> (usize, usize) {
    let id_needle = format!("\"id\": \"{step_id}\"");
    let step_start = text
        .find(&id_needle)
        .unwrap_or_else(|| panic!("missing step id {step_id}"));
    let msg_needle = "\"message\": {";
    let msg_rel = text[step_start..]
        .find(msg_needle)
        .unwrap_or_else(|| panic!("missing message object in step {step_id}"));
    let obj_start = step_start + msg_rel + "\"message\": ".len();
    let bytes = text.as_bytes();
    let mut i = obj_start;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return (obj_start, i + 1);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    panic!("unclosed message object in step {step_id}");
}

#[test]
fn settings_v4_active_witnesses_and_inputs_match_typed_recorder() {
    let update = std::env::var("UPDATE_SETTINGS_V4_FIXTURES").as_deref() == Ok("1");
    let active_messages = v4_active_root().join("messages");

    if update {
        fs::create_dir_all(&active_messages).unwrap();
    }

    for (id, request) in standalone_cases() {
        let encoded = encode_settings(&request).unwrap_or_else(|e| panic!("{id} encode: {e:?}"));
        let path = active_messages.join(format!("{id}.json"));
        if update {
            fs::write(&path, &encoded).unwrap();
        }
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{id} read: {e}"));
        assert_eq!(committed, encoded, "{id} bytes must match typed recorder");
    }

    let scenarios_dir = v4_active_root().join("scenarios");
    let mut by_scenario: Vec<(&'static str, Vec<(&'static str, String)>)> = Vec::new();
    for (scenario_id, step_id, request) in scenario_cases() {
        let encoded = encode_settings(&request)
            .unwrap_or_else(|e| panic!("{scenario_id}/{step_id} encode: {e:?}"));
        let pretty = pretty_compact_json_preserving_order(&encoded, 8);
        if let Some((_, list)) = by_scenario.iter_mut().find(|(s, _)| *s == scenario_id) {
            list.push((step_id, pretty));
        } else {
            by_scenario.push((scenario_id, vec![(step_id, pretty)]));
        }
    }

    for (scenario_id, steps) in by_scenario {
        let path = scenarios_dir.join(format!("{scenario_id}.json"));
        let mut text = fs::read_to_string(&path).unwrap();
        if update {
            for (step_id, pretty) in &steps {
                let (start, end) = find_step_message_span(&text, step_id);
                text.replace_range(start..end, pretty);
            }
            fs::write(&path, &text).unwrap();
        }
        let committed = fs::read_to_string(&path).unwrap();
        for (step_id, pretty) in &steps {
            let (start, end) = find_step_message_span(&committed, step_id);
            assert_eq!(
                &committed[start..end],
                pretty,
                "{scenario_id}/{step_id} settings payload must match typed recorder"
            );
        }
    }

    let eval_path = world_eval_jsonl_path();
    let eval_text = fs::read_to_string(&eval_path).unwrap();
    let mut lines: Vec<String> = eval_text.lines().map(str::to_owned).collect();
    let eval_expected: Vec<(usize, String)> = world_eval_cases()
        .into_iter()
        .map(|(line_idx, request)| {
            let encoded = encode_settings(&request).unwrap();
            let line = format!(
                "{{\"method\":\"DiscoverWorld\",\"body\":{}}}",
                serde_json::to_string(&encoded).unwrap()
            );
            (line_idx, line)
        })
        .collect();

    if update {
        for (line_idx, expected_line) in &eval_expected {
            lines[*line_idx] = expected_line.clone();
        }
        fs::write(&eval_path, lines.join("\n") + "\n").unwrap();
    }

    let committed_eval = fs::read_to_string(&eval_path).unwrap();
    let committed_lines: Vec<&str> = committed_eval.lines().collect();
    for (line_idx, expected_line) in &eval_expected {
        assert_eq!(
            committed_lines[*line_idx],
            expected_line,
            "world_eval_v4.jsonl line {} must match typed recorder",
            line_idx + 1
        );
    }
}
