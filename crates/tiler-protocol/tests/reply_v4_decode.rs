//! Staged v4 whole-reply decoder (5a): manifest-driven coverage for
//! [`decode_reply_v4`](tiler_protocol::world_protocol::decode_reply_v4)
//! over the `test-fixtures/world-wire/v4-active` WorldReply fixtures: 73
//! valid-wire accepts stable under re-parse, 18 context-free reject-wire
//! refusals with exact kind/message from the untouched original (then an
//! in-memory single-pointer repair as a positive counterfactual), 3
//! handshake-context rows accepted context-free but refused by
//! `decode_reply_v4_for_host` under their manifest-declared handshake
//! capabilities (with both-polarity checks per capability), and structural
//! boundary gates (duplicate keys, oversize, version, section caps,
//! identity uniqueness, row coupling rules, wire metadata distinctness).
//! Replies carry no held-row concept: any invalid row refuses the message.
//! Offline only: no adapters, no live work, no engine.

use tiler_core::connection::HostCapabilities;
use tiler_protocol::world_protocol::{
    DecodedReplyV4, decode_reply_v4, decode_reply_v4_for_host, encode_reply_v4,
};
use tiler_protocol::world_reply_write::{
    BrokerGestureAssoc, BrokerReplyBind, count_reply_v4, write_bound_reply_v4,
    write_projection_reply_v4, write_reply_v4,
};

fn fixture_root() -> std::path::PathBuf {
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
    #[serde(default)]
    context: serde_json::Value,
    #[serde(default = "default_canonical")]
    canonical: bool,
}

fn default_canonical() -> bool {
    true
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
        .filter(|fixture| fixture.family == "WorldReply")
        .collect()
}

fn fixture_text(file: &str) -> String {
    std::fs::read_to_string(fixture_root().join(file)).expect("fixture reads")
}

fn decode_ok(file: &str) -> DecodedReplyV4 {
    decode_reply_v4(&fixture_text(file)).expect("fixture decodes")
}

fn emulated_host() -> HostCapabilities {
    HostCapabilities {
        above: false,
        warp: false,
        native_sticky: false,
        ..HostCapabilities::default()
    }
}

fn native_host() -> HostCapabilities {
    HostCapabilities {
        above: true,
        warp: true,
        native_sticky: true,
        ..HostCapabilities::default()
    }
}

fn sticky_native_free_host() -> HostCapabilities {
    HostCapabilities {
        window_suppression: true,
        ..HostCapabilities::default()
    }
}

fn handshake_capabilities(file: &str) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_str(&fixture_text(file)).expect("handshake parses");
    value
        .pointer("/capabilities")
        .expect("capabilities present")
        .clone()
}

#[test]
fn v4_reply_census_matches_contract() {
    let fixtures = world_fixtures();
    assert_eq!(fixtures.len(), 94, "WorldReply fixture count");
    let valid: Vec<_> = fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "valid-wire")
        .collect();
    let reject: Vec<_> = fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "reject-wire")
        .collect();
    assert_eq!(valid.len(), 73, "valid-wire count");
    assert_eq!(reject.len(), 21, "reject-wire count");
    assert!(
        valid.iter().all(|fixture| fixture.canonical),
        "every valid-wire reply is canonical"
    );
    let context_free = reject
        .iter()
        .filter(|fixture| fixture.context.is_null())
        .count();
    let contextual = reject
        .iter()
        .filter(|fixture| !fixture.context.is_null())
        .count();
    assert_eq!(context_free, 18, "context-free reject count");
    assert_eq!(contextual, 3, "handshake-context reject count");
    for fixture in &reject {
        if fixture.context.is_null() {
            continue;
        }
        assert!(
            fixture.context.get("handshake").is_some(),
            "context reject {} declares a handshake",
            fixture.id
        );
    }
}

#[test]
fn v4_reply_valid_wire_accepts() {
    let fixtures = world_fixtures();
    let mut count = 0;
    for fixture in fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "valid-wire")
    {
        let text = fixture_text(&fixture.file);
        let first = decode_reply_v4(&text).expect("valid-wire decodes");
        let second = decode_reply_v4(&text).expect("valid-wire re-parses");
        assert_eq!(first, second, "decode stable under re-parse {}", fixture.id);
        count += 1;
    }
    assert_eq!(count, 73, "every valid-wire reply decodes");
}

#[test]
fn v4_reply_valid_shapes_spot_check() {
    let bootstrap = decode_ok("messages/reply-bootstrap.json");
    assert!(!bootstrap.reply.ready, "bootstrap not ready");
    assert!(bootstrap.reply.scopes.is_empty(), "bootstrap no scopes");
    assert!(bootstrap.reply.presentation.border.subject.is_none());
    let observe = decode_ok("messages/reply-observe.json");
    assert!(observe.reply.ready, "observe ready");
    assert_eq!(observe.reply.scopes.len(), 1);
    assert_eq!(observe.reply.workspaces.len(), 2);
    assert!(
        observe.reply.drag_preview.is_none(),
        "observe no drag preview"
    );
    assert!(observe.reply.cursor_warp.is_none(), "observe no warp");
    assert!(
        !observe.wire.suspended_output_present,
        "observe no suspension"
    );
    let focus = decode_ok("messages/reply-effect-focus.json");
    let request = focus
        .reply
        .focus_request
        .as_ref()
        .expect("focus lane present");
    assert_eq!(
        request.cause,
        tiler_core::world_report::FocusRequestCause::ExplicitCommand,
        "explicit request carries its cause"
    );
    assert_eq!(
        request.cmd_seq,
        Some(1),
        "explicit request carries its command"
    );
    assert!(
        request.after_shown.is_some(),
        "selection-backed explicit carries dependencies"
    );
    assert!(
        focus.reply.effects.is_empty(),
        "focus left the effects lane"
    );
    let drag = decode_ok("messages/reply-result-drag-left.json");
    assert!(drag.reply.drag_preview.is_some(), "drag carries preview");
    let gesture = decode_ok("messages/reply-gesture-committed.json");
    let settled = gesture.reply.gesture_settled.expect("terminal present");
    assert_eq!(settled.id, 1);
    assert!(settled.committed);
    let switch = decode_ok("messages/reply-switch-proven.json");
    assert_eq!(switch.reply.switch_settled.len(), 1);
    let diagnostics = decode_ok("messages/reply-row-diagnostics.json");
    assert_eq!(diagnostics.reply.diagnostics.len(), 5);
    let intent = decode_ok("messages/reply-intent-save-clear.json");
    assert_eq!(intent.reply.intent_desired.len(), 2);
    let suspended = decode_ok("messages/reply-presentation-hidden-suspended.json");
    assert!(
        suspended.wire.suspended_output_present,
        "suspension recorded"
    );
    assert!(
        suspended.reply.presentation.suspended_output.is_some(),
        "suspension typed"
    );
}

/// Exact (kind, message) plus the single-pointer repair source for one
/// context-free reject-wire fixture. The pointer names the note path to
/// copy from the source witness (or remove when the source lacks it).
fn expected_reject(id: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match id {
        "invalid-scope-window-arm" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-observe.json",
            "/scopes/0/scope",
        ),
        "invalid-scope-output-extra-ws" => (
            "unknown-field",
            "request contains an unknown field",
            "messages/reply-observe.json",
            "/scopes/0/scope/ws",
        ),
        "invalid-workspace-trailing-false" => (
            "snapshot-invalid",
            "workspace-trailing-false",
            "messages/reply-observe.json",
            "/workspaces/1/trailing",
        ),
        "invalid-workspace-null-bind" => (
            "malformed",
            "request is malformed",
            "messages/reply-observe.json",
            "/workspaces/0/bind",
        ),
        "invalid-output-null-view" => (
            "malformed",
            "request is malformed",
            "messages/reply-observe.json",
            "/outputs/0/view",
        ),
        "invalid-hold-null-reason" => (
            "malformed",
            "request is malformed",
            "messages/reply-hold-unreadable.json",
            "/scopes/0/reason",
        ),
        "invalid-hold-unknown-reason" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-hold-unreadable.json",
            "/scopes/0/reason",
        ),
        "invalid-nonhold-reason" => (
            "snapshot-invalid",
            "nonhold-reason",
            "messages/reply-observe.json",
            "/scopes/0/reason",
        ),
        "invalid-desire-null-frame-op" => (
            "malformed",
            "request is malformed",
            "messages/reply-observe.json",
            "/windows/0/frame_op",
        ),
        "invalid-show-with-hide-fallback" => (
            "snapshot-invalid",
            "visibility-show-fallback",
            "messages/reply-observe.json",
            "/windows/0/visibility/fallback",
        ),
        "invalid-operation-cause-null" => (
            "malformed",
            "request is malformed",
            "messages/reply-operation-continued-tiled.json",
            "/operationSettled/0/cause",
        ),
        "invalid-operation-outcome-visibility" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-operation-abandoned-closed.json",
            "/operationSettled/0/outcome",
        ),
        "invalid-border-old-absolute" => (
            "unknown-field",
            "request contains an unknown field",
            "messages/reply-observe.json",
            "/presentation/border/inner",
        ),
        "invalid-border-cloaked" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-presentation-hidden-pending.json",
            "/presentation/border/reason",
        ),
        "invalid-underlay-missing-leaf" => (
            "malformed",
            "request is malformed",
            "messages/reply-presentation-underlay-found.json",
            "/presentation/underlay/members/0/leaf",
        ),
        "invalid-state-target-count" => (
            "snapshot-invalid",
            "state-target-count",
            "messages/reply-state-authority.json",
            "/windows/0/state/4",
        ),
        "invalid-close-focus-unknown-cause" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-close-focus-pending.json",
            "/focusRequest/cause",
        ),
        "invalid-close-focus-missing-lane" => (
            "malformed",
            "request is malformed",
            "messages/reply-bootstrap.json",
            "/focusRequest",
        ),
        other => panic!("reject table misses {other}"),
    }
}

fn pointer_tokens(pointer: &str) -> Vec<String> {
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
        return;
    }
    let object = parent.as_object_mut().expect("object parent");
    match source_value {
        Some(value) => {
            object.insert(last.clone(), value);
        }
        None => {
            object.remove(last);
        }
    }
}

#[test]
fn v4_reply_reject_wire_refuses_then_repairs() {
    let fixtures = world_fixtures();
    let mut count = 0;
    for fixture in fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "reject-wire" && fixture.context.is_null())
    {
        let (kind, message, source_file, pointer) = expected_reject(&fixture.id);
        let text = fixture_text(&fixture.file);
        let error = decode_reply_v4(&text).expect_err("reject-wire refuses");
        assert_eq!(error.kind, kind, "kind {}", fixture.id);
        assert_eq!(error.message, message, "message {}", fixture.id);
        // Single-pointer causal repair: copy the source witness value at
        // exactly `pointer` into the invalid document (or remove the key
        // when the source has no such pointer). The rest of the document
        // stays original by construction; the repaired document must decode.
        let mut invalid_value: serde_json::Value =
            serde_json::from_str(&text).expect("invalid parses as JSON");
        let source_value: serde_json::Value =
            serde_json::from_str(&fixture_text(source_file)).expect("source parses");
        let before: serde_json::Value = invalid_value.clone();
        repair_single_pointer(&mut invalid_value, &source_value, pointer);
        assert_ne!(
            invalid_value, before,
            "repair changes pointer {}",
            fixture.id
        );
        assert_eq!(
            invalid_value.pointer(pointer).cloned(),
            source_value.pointer(pointer).cloned(),
            "repair matches source at pointer {}",
            fixture.id
        );
        let repaired = serde_json::to_string(&invalid_value).expect("repair serializes");
        decode_reply_v4(&repaired).expect("repaired decodes");
        count += 1;
    }
    assert_eq!(count, 18, "every context-free reject refuses then repairs");
}

#[test]
fn v4_reply_capability_context_gates() {
    let fixtures = world_fixtures();
    let by_id = |id: &str| {
        fixtures
            .iter()
            .find(|fixture| fixture.id == id)
            .expect("fixture present")
    };
    let emulated = handshake_capabilities("messages/handshake-emulated.json");
    assert_eq!(
        emulated.pointer("/above"),
        Some(&serde_json::Value::Bool(false))
    );
    assert_eq!(
        emulated.pointer("/warp"),
        Some(&serde_json::Value::Bool(false))
    );
    assert_eq!(
        emulated.pointer("/nativeSticky"),
        Some(&serde_json::Value::Bool(false))
    );
    let sticky_free = handshake_capabilities("messages/handshake-sticky-native-free.json");
    assert_eq!(
        sticky_free.pointer("/nativeSticky"),
        Some(&serde_json::Value::Bool(false))
    );

    // Above: invalid fixture is byte-identical to the valid above twin
    // (manifest bytes verified with `cmp`); the twin itself carries a
    // declared handshake-native context, so context-free acceptance and
    // declared-context refusal are the paired assertions.
    let above_valid = by_id("reply-above-true");
    assert_eq!(
        above_valid.context.pointer("/handshake"),
        Some(&serde_json::Value::String("handshake-native".to_owned()))
    );
    let above_invalid = by_id("invalid-above-unsupported");
    assert_eq!(
        above_invalid.context.pointer("/handshake"),
        Some(&serde_json::Value::String("handshake-emulated".to_owned()))
    );
    let valid_text = fixture_text(&above_valid.file);
    let invalid_text = fixture_text(&above_invalid.file);
    assert_eq!(valid_text, invalid_text, "above pair shares wire bytes");
    decode_reply_v4(&invalid_text).expect("context-free accepts above target");
    let error = decode_reply_v4_for_host(&invalid_text, &emulated_host())
        .expect_err("emulated host refuses above target");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "above-capability-false");
    decode_reply_v4_for_host(&valid_text, &native_host()).expect("native host accepts above");
    // Any above StateTarget refuses without the capability, even value false.
    let above_false = fixture_text("messages/reply-above-false.json");
    decode_reply_v4(&above_false).expect("context-free accepts above false");
    let error = decode_reply_v4_for_host(&above_false, &emulated_host())
        .expect_err("emulated host refuses above false");
    assert_eq!(error.message, "above-capability-false");

    // Warp: non-null cursor target refuses without the warp capability.
    let warp_invalid = by_id("invalid-warp-unsupported");
    assert_eq!(
        warp_invalid.context.pointer("/handshake"),
        Some(&serde_json::Value::String("handshake-emulated".to_owned()))
    );
    let warp_text = fixture_text(&warp_invalid.file);
    decode_reply_v4(&warp_text).expect("context-free accepts warp point");
    let error = decode_reply_v4_for_host(&warp_text, &emulated_host())
        .expect_err("emulated host refuses warp point");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "warp-capability-false");
    decode_reply_v4_for_host(&warp_text, &native_host()).expect("native host accepts warp");
    let plain = fixture_text("messages/reply-observe.json");
    decode_reply_v4_for_host(&plain, &emulated_host()).expect("null warp passes");

    // Sticky: any sticky StateTarget refuses without nativeSticky, even
    // value false with semantic window-suppression support.
    let sticky_invalid = by_id("invalid-sticky-native-free-target");
    assert_eq!(
        sticky_invalid.context.pointer("/handshake"),
        Some(&serde_json::Value::String(
            "handshake-sticky-native-free".to_owned()
        ))
    );
    let sticky_text = fixture_text(&sticky_invalid.file);
    decode_reply_v4(&sticky_text).expect("context-free accepts sticky target");
    let error = decode_reply_v4_for_host(&sticky_text, &sticky_native_free_host())
        .expect_err("sticky-native-free host refuses sticky target");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "sticky-capability-false");
    decode_reply_v4_for_host(&sticky_text, &native_host())
        .expect("native host accepts sticky target");
    let authority = fixture_text("messages/reply-state-authority.json");
    decode_reply_v4(&authority).expect("context-free accepts sticky false");
    // The authority row also carries an above target, and the above gate
    // runs first; grant above so this check isolates the sticky gate.
    let above_no_sticky = HostCapabilities {
        above: true,
        window_suppression: true,
        ..HostCapabilities::default()
    };
    let error = decode_reply_v4_for_host(&authority, &above_no_sticky)
        .expect_err("sticky false still refuses without nativeSticky");
    assert_eq!(error.message, "sticky-capability-false");

    // The wrapper still enforces every context-free structural rule.
    let structural = fixture_text("messages/invalid-scope-window-arm.json");
    let error = decode_reply_v4_for_host(&structural, &native_host())
        .expect_err("wrapper keeps structural refusal");
    assert_eq!(error.kind, "unknown-value");
    assert_eq!(error.message, "request contains an unknown value");
}

#[test]
fn v4_reply_decoder_rejects_strict_envelope_violations() {
    let empty = fixture_text("messages/reply-bootstrap.json");
    decode_reply_v4(&empty).expect("bootstrap decodes");

    // Duplicate keys refuse (literal and escaped-equivalent spellings).
    let literal = empty.replacen(
        "\"adapter\":\"adapter-a\"",
        "\"adapter\":\"adapter-a\",\"adapter\":\"adapter-a\"",
        1,
    );
    assert_ne!(literal, empty);
    let error = decode_reply_v4(&literal).expect_err("duplicate refuses");
    assert_eq!(error.kind, "malformed");
    assert_eq!(error.message, "request is malformed");
    let escaped = empty.replacen(
        "\"adapter\":\"adapter-a\"",
        "\"adapter\":\"adapter-a\",\"\\u0061dapter\":\"adapter-a\"",
        1,
    );
    assert_ne!(escaped, empty);
    let error = decode_reply_v4(&escaped).expect_err("escaped duplicate refuses");
    assert_eq!(error.kind, "malformed");

    // Oversize refuses before parsing.
    let oversized = "x".repeat(tiler_protocol::world_protocol::WORLD_MAX_REPLY_BYTES + 1);
    let error = decode_reply_v4(&oversized).expect_err("oversize refuses");
    assert_eq!(error.kind, "oversized");
    assert_eq!(error.message, "request exceeds size bound");

    // Wrong contract version refuses.
    let versioned = empty.replacen("\"v\":4", "\"v\":3", 1);
    assert_ne!(versioned, empty);
    let error = decode_reply_v4(&versioned).expect_err("version refuses");
    assert_eq!(error.kind, "unsupported-version");
    assert_eq!(error.message, "unsupported contract version");

    // Unknown top-level field refuses even when every section is valid.
    let mut value: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    value
        .as_object_mut()
        .expect("object")
        .insert("trailing".to_owned(), serde_json::Value::Bool(true));
    let error = decode_reply_v4(&serde_json::to_string(&value).expect("serializes"))
        .expect_err("unknown field refuses");
    assert_eq!(error.kind, "unknown-field");
    assert_eq!(error.message, "request contains an unknown field");

    // Missing required key refuses.
    let mut missing: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    missing
        .as_object_mut()
        .expect("object")
        .remove("presentation");
    let error = decode_reply_v4(&serde_json::to_string(&missing).expect("serializes"))
        .expect_err("missing key refuses");
    assert_eq!(error.kind, "malformed");

    // Envelope key order carries no meaning: shuffled keys still decode.
    // Parse fresh from `empty`: the `value` above carries an inserted
    // trailing key for the unknown-field check.
    let fresh: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    let mut shuffled = String::from("{");
    let map = fresh.as_object().expect("object");
    let mut keys: Vec<&String> = map.keys().collect();
    keys.reverse();
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            shuffled.push(',');
        }
        shuffled.push_str(&serde_json::to_string(key).expect("key"));
        shuffled.push(':');
        shuffled.push_str(&serde_json::to_string(&map[*key]).expect("value"));
    }
    shuffled.push('}');
    assert_ne!(shuffled, empty);
    let reordered = decode_reply_v4(&shuffled).expect("order-insensitive decodes");
    assert_eq!(
        reordered,
        decode_reply_v4(&empty).expect("decodes"),
        "order does not change the typed reply"
    );
}

#[test]
fn v4_reply_envelope_ids_match_report_validators() {
    use tiler_protocol::world_protocol::decode_report_v4;
    let reply = fixture_text("messages/reply-bootstrap.json");
    let report = fixture_text("messages/report-empty.json");
    let long_adapter = "a".repeat(129);
    let long_gen = "g".repeat(65);
    let long_req = "r".repeat(129);
    let cases: [(&str, &str); 7] = [
        ("adapter", ""),
        ("adapter", long_adapter.as_str()),
        ("gen", "GEN-A"),
        ("gen", "generation_a"),
        ("gen", long_gen.as_str()),
        ("req", ""),
        ("req", long_req.as_str()),
    ];
    for (key, bad) in cases {
        let mut reply_value: serde_json::Value = serde_json::from_str(&reply).expect("parses");
        reply_value
            .as_object_mut()
            .expect("object")
            .insert(key.to_owned(), serde_json::Value::String(bad.to_owned()));
        let reply_error =
            decode_reply_v4(&serde_json::to_string(&reply_value).expect("serializes"))
                .expect_err("reply refuses bad envelope id");
        let mut report_value: serde_json::Value = serde_json::from_str(&report).expect("parses");
        report_value
            .as_object_mut()
            .expect("object")
            .insert(key.to_owned(), serde_json::Value::String(bad.to_owned()));
        let report_error =
            decode_report_v4(&serde_json::to_string(&report_value).expect("serializes"))
                .expect_err("report refuses bad envelope id");
        assert_eq!(
            reply_error.kind, report_error.kind,
            "reply matches report kind for {key}={bad}"
        );
        assert_eq!(
            reply_error.kind, "unknown-value",
            "bad envelope id {key}={bad} is unknown-value"
        );
    }
    decode_reply_v4(&reply).expect("untouched reply still decodes");
    decode_report_v4(&report).expect("untouched report still decodes");
}

#[test]
fn v4_reply_hidden_underlay_base_rev_number_is_malformed() {
    let empty = fixture_text("messages/reply-bootstrap.json");
    let mut numbered: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    *numbered
        .pointer_mut("/presentation/underlay/baseRev")
        .expect("baseRev present") = serde_json::json!(7);
    let error = decode_reply_v4(&serde_json::to_string(&numbered).expect("serializes"))
        .expect_err("numeric baseRev refuses");
    assert_eq!(error.kind, "malformed");
    let mut bad_text: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    *bad_text
        .pointer_mut("/presentation/underlay/baseRev")
        .expect("baseRev present") = serde_json::json!("00");
    let error = decode_reply_v4(&serde_json::to_string(&bad_text).expect("serializes"))
        .expect_err("bad decimal baseRev refuses");
    assert_eq!(error.kind, "unknown-value");
    let mut quoted: serde_json::Value = serde_json::from_str(&empty).expect("parses");
    *quoted
        .pointer_mut("/presentation/underlay/baseRev")
        .expect("baseRev present") = serde_json::json!("7");
    decode_reply_v4(&serde_json::to_string(&quoted).expect("serializes"))
        .expect("quoted baseRev decodes");
}

#[test]
fn v4_reply_wire_metadata_distinctness() {
    let plain = decode_ok("messages/reply-observe.json");
    let suspended = decode_ok("messages/reply-presentation-hidden-suspended.json");
    assert_ne!(
        plain.wire, suspended.wire,
        "suspension presence changes wire metadata"
    );
    assert!(suspended.reply.presentation.suspended_output.is_some());
    assert!(plain.reply.presentation.suspended_output.is_none());
    let text = fixture_text("messages/reply-observe.json");
    assert_eq!(
        decode_reply_v4(&text).expect("decodes").wire,
        decode_reply_v4(&text).expect("decodes").wire,
        "same bytes carry same metadata"
    );
    // Metadata never leaks into the typed reply comparison target: two
    // decodes of the same bytes are fully equal, wire included.
    assert_eq!(
        decode_reply_v4(&text).expect("decodes"),
        decode_reply_v4(&text).expect("decodes")
    );
}

fn valid_reply_value() -> serde_json::Value {
    serde_json::from_str(&fixture_text("messages/reply-observe.json")).expect("parses")
}

fn decode_value(
    value: &serde_json::Value,
) -> Result<DecodedReplyV4, tiler_protocol::world_protocol::WorldProtocolError> {
    decode_reply_v4(&serde_json::to_string(value).expect("serializes"))
}

#[test]
fn v4_reply_structural_regression() {
    // Cap violation in a later section still refuses after earlier
    // unusual-but-valid rows: a held scope plus an over-cap operation list.
    let mut held = valid_reply_value();
    held["scopes"] = serde_json::from_str(
        r#"[{"scope":{"kind":"output","out":"out-a"},"auth":"hold","reason":"unreadable"}]"#,
    )
    .expect("parses");
    held["workspaces"] = serde_json::from_str(
        r#"[{"id":"ws-a","out":"out-a","bind":"","scope":{"kind":"output","out":"out-a"},"order":0,"mode":"tiled","exist":true,"hold":"unreadable"}]"#,
    )
    .expect("parses");
    let mut operations = Vec::new();
    for index in 0..1025 {
        operations.push(serde_json::json!({
            "op": format!("operation-{index}"),
            "win": "win-a",
            "outcome": "continued-tiled",
            "cause": ""
        }));
    }
    held["operationSettled"] = serde_json::Value::Array(operations);
    let error = decode_value(&held).expect_err("later cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");

    // Duplicate identity refuses per section even with a trailing unknown
    // top-level key present: the envelope check runs first.
    let mut duplicate = valid_reply_value();
    let window = duplicate["windows"][0].clone();
    duplicate["windows"]
        .as_array_mut()
        .expect("array")
        .push(window);
    duplicate["trailing"] = serde_json::Value::Bool(true);
    let error = decode_value(&duplicate).expect_err("envelope check runs first");
    assert_eq!(error.kind, "unknown-field");
    duplicate
        .as_object_mut()
        .expect("object")
        .remove("trailing");
    let error = decode_value(&duplicate).expect_err("duplicate window refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-window");

    // Counters: JSON numbers refuse as malformed, bad spellings as
    // unknown values, overflow as unknown values.
    let mut numeric = valid_reply_value();
    numeric["obs"] = serde_json::json!(1);
    let error = decode_value(&numeric).expect_err("numeric counter refuses");
    assert_eq!(error.kind, "malformed");
    let mut leading = valid_reply_value();
    leading["obs"] = serde_json::Value::String("01".to_owned());
    let error = decode_value(&leading).expect_err("leading zero refuses");
    assert_eq!(error.kind, "unknown-value");
    let mut overflow = valid_reply_value();
    overflow["rev"] = serde_json::Value::String("18446744073709551616".to_owned());
    let error = decode_value(&overflow).expect_err("overflow refuses");
    assert_eq!(error.kind, "unknown-value");

    // Row coupling rules combine without hiding one another: a document
    // violating both the trailing rule and the fallback rule still refuses,
    // and each violation alone names its own message.
    let mut trailing = valid_reply_value();
    trailing["workspaces"][1]["trailing"] = serde_json::Value::Bool(false);
    trailing["windows"][0]["visibility"]["fallback"] = serde_json::Value::Bool(true);
    let error = decode_value(&trailing).expect_err("combined violations refuse");
    assert_eq!(error.kind, "snapshot-invalid");
    trailing["windows"][0]["visibility"]["fallback"] = serde_json::Value::Bool(false);
    let error = decode_value(&trailing).expect_err("trailing alone refuses");
    assert_eq!(error.message, "workspace-trailing-false");
    let mut fallback = valid_reply_value();
    fallback["windows"][0]["visibility"]["fallback"] = serde_json::Value::Bool(true);
    let error = decode_value(&fallback).expect_err("fallback alone refuses");
    assert_eq!(error.message, "visibility-show-fallback");

    // Effect correlation and state-count rules.
    let mut effect = valid_reply_value();
    effect["effects"] = serde_json::json!([{
        "id": "effect-a", "kind": "focus", "target": "win-a", "rev": "1",
        "cause": "automatic-admission", "cmdSeq": "1", "awaitShown": false
    }]);
    let error = decode_value(&effect).expect_err("non-explicit seq refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "effect-non-explicit-seq");
    let mut explicit_null = valid_reply_value();
    explicit_null["effects"] = serde_json::json!([{
        "id": "effect-a", "kind": "focus", "target": "win-a", "rev": "1",
        "cause": "explicit-command", "cmdSeq": null, "awaitShown": false
    }]);
    let error = decode_value(&explicit_null).expect_err("explicit null seq refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "effect-explicit-null-seq");
    let mut automatic_shown = valid_reply_value();
    automatic_shown["effects"] = serde_json::json!([{
        "id": "effect-a", "kind": "focus", "target": "win-a", "rev": "1",
        "cause": "automatic-admission", "cmdSeq": null, "awaitShown": true
    }]);
    let error = decode_value(&automatic_shown).expect_err("automatic await-shown refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "effect-automatic-await-shown");
    let mut state = valid_reply_value();
    state["windows"][0]["state"] = serde_json::json!([
        {"property": "fullscreen", "value": false, "auth": "controlled", "reason": "", "cmdSeq": null},
        {"property": "fullscreen", "value": true, "auth": "controlled", "reason": "", "cmdSeq": null}
    ]);
    let error = decode_value(&state).expect_err("duplicate property refuses");
    assert_eq!(error.message, "state-target-count");

    // Overbound intent data refuses with the typed helper token. This is a
    // strict raw-wire negative only: typed `IntentData` always encodes to a
    // short canonical form, so an overbound typed value is unspellable and
    // has no writer-side counterpart (see `v4_reply_writer_refuses_unspellable`).
    let mut intent = valid_reply_value();
    intent["intentDesired"] = serde_json::json!([{"win": "win-a", "data": "x".repeat(1025)}]);
    let error = decode_value(&intent).expect_err("overbound intent refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "intent-data-invalid");

    // Duplicate identities refuse across every identity-keyed section.
    for (section, row, message) in [
        (
            "scopes",
            serde_json::json!({"scope":{"kind":"output","out":"out-a"},"auth":"controlled","reason":""}),
            "duplicate-scope",
        ),
        (
            "workspaces",
            serde_json::json!({"id":"ws-a","out":"out-a","bind":"","scope":{"kind":"output","out":"out-a"},"order":0,"mode":"tiled","exist":true,"hold":""}),
            "duplicate-workspace",
        ),
        (
            "outputs",
            serde_json::json!({"out":"out-a","view":"ws-a","mode":"tiled","hold":""}),
            "duplicate-output",
        ),
        (
            "transfers",
            serde_json::json!({"op":"transfer-a","kind":"window","win":"win-a","out":"out-a","ws":"ws-a","follow":true,"held":false,"cmdSeq":"1","parent":null}),
            "duplicate-transfer",
        ),
        (
            "operationSettled",
            serde_json::json!({"win":"win-a","kind":"clear","op":"operation-a","outcome":"continued-tiled","cause":""}),
            "duplicate-operation",
        ),
        (
            "nativeCreations",
            serde_json::json!({"op":"create-a","ws":"ws-new","out":"out-a"}),
            "duplicate-native-creation",
        ),
        (
            "intentDesired",
            serde_json::json!({"win":"win-a","data":null}),
            "duplicate-intent-desire",
        ),
        (
            "switchSettled",
            serde_json::json!({"out":"out-a","op":"view-a","outcome":"complete","failed":[],"focus":"proven","fallback":false}),
            "duplicate-switch",
        ),
    ] {
        let mut document = valid_reply_value();
        document[section].as_array_mut().expect("array").push(row);
        // Seed each section with one valid row first where it starts empty.
        if document[section].as_array().expect("array").len() == 1 {
            let seed = document[section][0].clone();
            document[section].as_array_mut().expect("array").push(seed);
        }
        let error = decode_value(&document).expect_err("duplicate refuses");
        assert_eq!(error.kind, "snapshot-invalid", "section {section}");
        assert_eq!(error.message, message, "section {section}");
    }
}

#[test]
fn v4_reply_intent_desired_lane_strictness() {
    // Required field: missing `intentDesired` refuses as malformed, even
    // when every other lane is valid.
    let mut missing = valid_reply_value();
    missing
        .as_object_mut()
        .expect("object")
        .remove("intentDesired");
    let error = decode_value(&missing).expect_err("missing intentDesired refuses");
    assert_eq!(error.kind, "malformed");

    // Old field: the retired `intentWrites` spelling refuses as an unknown
    // field, never as a silent alias. Both the extra-key and the
    // swapped-key shapes refuse.
    let mut extra = valid_reply_value();
    extra["intentWrites"] = serde_json::json!([]);
    let error = decode_value(&extra).expect_err("extra intentWrites refuses");
    assert_eq!(error.kind, "unknown-field");
    assert_eq!(error.message, "request contains an unknown field");
    let mut swapped = valid_reply_value();
    let kept = swapped
        .as_object_mut()
        .expect("object")
        .remove("intentDesired");
    assert!(kept.is_some(), "valid reply carries intentDesired");
    swapped["intentWrites"] = kept.expect("kept");
    let error = decode_value(&swapped).expect_err("swapped intentWrites refuses");
    assert_eq!(error.kind, "unknown-field");

    // Null save/clear spellings: an explicit null clears, a canonical
    // string saves; both ride the required lane with unique windows.
    let mut save_clear = valid_reply_value();
    save_clear["intentDesired"] = serde_json::json!([
        {"win": "win-a", "data": "{\"v\":1,\"float\":true,\"stickyPrior\":false}"},
        {"win": "win-b", "data": null},
    ]);
    let decoded = decode_value(&save_clear).expect("save/clear decodes");
    assert_eq!(decoded.reply.intent_desired.len(), 2);
    assert!(decoded.reply.intent_desired[1].data.is_none());

    // Unique windows: a repeated win refuses on both decode and encode.
    let mut dup = valid_reply_value();
    dup["intentDesired"] = serde_json::json!([
        {"win": "win-a", "data": null},
        {"win": "win-a", "data": null},
    ]);
    let error = decode_value(&dup).expect_err("duplicate win refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-intent-desire");

    // Row cap: 10000 tiny rows admit, 10001 refuse with the section token.
    let at_cap: Vec<serde_json::Value> = (0..10_000)
        .map(|i| serde_json::json!({"win": format!("win-{i}"), "data": null}))
        .collect();
    let mut capped = valid_reply_value();
    capped["intentDesired"] = serde_json::Value::Array(at_cap);
    decode_value(&capped).expect("cap admits");
    let mut over = valid_reply_value();
    let rows = (0..10_001)
        .map(|i| serde_json::json!({"win": format!("win-{i}"), "data": null}))
        .collect();
    over["intentDesired"] = serde_json::Value::Array(rows);
    let error = decode_value(&over).expect_err("over cap refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "section-over-cap");
}

#[test]
fn v4_reply_writer_round_trips_all_valid_wire() {
    let fixtures = world_fixtures();
    let mut count = 0;
    for fixture in fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "valid-wire")
    {
        let text = fixture_text(&fixture.file);
        let decoded = decode_reply_v4(&text).expect("valid-wire decodes");
        let encoded = encode_reply_v4(&decoded).expect("valid-wire encodes");
        assert_eq!(
            count_reply_v4(&decoded.reply, &decoded.wire).expect("valid-wire counts"),
            encoded.len(),
            "exact count matches encoded len {}",
            fixture.id
        );
        assert_eq!(
            write_reply_v4(&decoded.reply, &decoded.wire).expect("valid-wire writes"),
            encoded,
            "borrowed writer matches encode_reply_v4 {}",
            fixture.id
        );
        if encoded != text {
            let common = encoded
                .bytes()
                .zip(text.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let lo = common.saturating_sub(60);
            panic!(
                "byte mismatch {} at {common}: wire[{lo}..]={:?} encoded[{lo}..]={:?}",
                fixture.id,
                &text[lo..(common + 60).min(text.len())],
                &encoded[lo..(common + 60).min(encoded.len())],
            );
        }
        let back = decode_reply_v4(&encoded).expect("encoding re-decodes");
        assert_eq!(back, decoded, "re-decode equals {}", fixture.id);
        count += 1;
    }
    assert_eq!(count, 73, "every valid-wire reply round-trips");
}

#[test]
fn v4_reply_writer_warp_constructed() {
    let text = fixture_text("messages/reply-observe.json");
    let base = decode_reply_v4(&text).expect("observe decodes");
    assert!(base.reply.cursor_warp.is_none());
    for point in [(20, 30), (-16384, 16384)] {
        let mut constructed = base.clone();
        constructed.reply.cursor_warp = Some(point);
        let expected = text.replacen(
            "\"warpCursor\":null",
            &format!("\"warpCursor\":[{},{}]", point.0, point.1),
            1,
        );
        assert_ne!(expected, text);
        let encoded = encode_reply_v4(&constructed).expect("warp encodes");
        assert_eq!(encoded, expected, "warp point {point:?} spells exactly");
        let back = decode_reply_v4(&encoded).expect("warp re-decodes");
        assert_eq!(back, constructed, "warp re-decode equals");
    }
}

#[test]
fn v4_reply_writer_suspended_output_present_and_inconsistent() {
    let suspended_text = fixture_text("messages/reply-presentation-hidden-suspended.json");
    let suspended = decode_reply_v4(&suspended_text).expect("suspended decodes");
    assert!(suspended.wire.suspended_output_present);
    assert!(suspended.reply.presentation.suspended_output.is_some());
    let encoded = encode_reply_v4(&suspended).expect("suspended encodes");
    assert_eq!(encoded, suspended_text, "suspended presence round-trips");
    // Flag true with no typed output refuses.
    let mut missing = suspended.clone();
    missing.reply.presentation.suspended_output = None;
    let error = encode_reply_v4(&missing).expect_err("flag without value refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "suspended-output-inconsistent");
    // Flag false with a typed output refuses.
    let plain_text = fixture_text("messages/reply-observe.json");
    let mut plain = decode_reply_v4(&plain_text).expect("observe decodes");
    assert!(!plain.wire.suspended_output_present);
    plain.reply.presentation.suspended_output =
        Some(tiler_core::directional::OutputId("out-a".to_owned()));
    let error = encode_reply_v4(&plain).expect_err("value without flag refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "suspended-output-inconsistent");
}

#[test]
fn v4_reply_writer_construction_independence() {
    let text = fixture_text("messages/reply-observe.json");
    let edited = text
        .replacen("\"adapter\":\"adapter-a\"", "\"adapter\":\"adapter-b\"", 1)
        .replacen("\"req\":\"request-a\"", "\"req\":\"request-b\"", 1)
        .replacen("\"obs\":\"1\"", "\"obs\":\"42\"", 1)
        .replacen("\"rev\":\"1\"", "\"rev\":\"7\"", 1);
    assert_ne!(edited, text);
    let decoded = decode_reply_v4(&edited).expect("edited decodes");
    assert_eq!(decoded.reply.adapter_incarnation, "adapter-b");
    assert_eq!(decoded.reply.request_id, "request-b");
    assert_eq!(decoded.reply.based_on_observation_seq, 42);
    assert_eq!(decoded.reply.state_revision, 7);
    let encoded = encode_reply_v4(&decoded).expect("edited encodes");
    assert_eq!(encoded, edited, "text edits survive the round trip");
}

fn writer_base() -> DecodedReplyV4 {
    decode_reply_v4(&fixture_text("messages/reply-observe.json")).expect("observe decodes")
}

#[test]
fn roster_plan_round_trips_and_rejects_invalid_authority() {
    let base = writer_base();
    let mut value: serde_json::Value =
        serde_json::from_str(&encode_reply_v4(&base).unwrap()).unwrap();
    assert!(value.get("rosterPlan").is_none());
    value["rosterPlan"] = serde_json::json!({
        "op": "roster-1", "gen": value["gen"], "rev": value["rev"], "obs": value["obs"],
        "baseline": {
            "epoch": "1", "order": ["d1", "d2", "d3"],
            "projections": [{"out":"out-a","rows":["ws-a","ws-b"]}]
        },
        "currents": [{"out":"out-a","native":"d1"}],
        "occupants": [{"native":"d1","windows":[{"win":"win-a","out":"out-a"}]}],
        "projections": [{"out":"out-a","rows":["ws-a","ws-c"]}],
        "slots": [{"kind":"existing","native":"d1"},{"kind":"existing","native":"d2"},
            {"kind":"append","step":"append-1"}],
        "members": [{"step":"member-1","win":"win-a","before":{"out":"out-a","native":"d1"},
            "out":"out-a","target":{"kind":"append","step":"append-1"},"deps":["append-1"]}],
        "views": [{"step":"view-1","out":"out-a","before":"d1",
            "target":{"kind":"existing","native":"d2"},"deps":["member-1"]}],
        "removals": [{"step":"remove-1","native":"d3","deps":["view-1"]}],
        "focus": {"id":"focus-a","target":"win-a","rev":"1","cause":"explicit-command",
            "out":"out-a","ws":"ws-c","cmdSeq":"1","afterShown":null}
    });
    let decoded = decode_reply_v4(&value.to_string()).expect("slot reply decodes");
    let encoded = encode_reply_v4(&decoded).expect("slot reply encodes");
    assert_eq!(decode_reply_v4(&encoded).unwrap(), decoded);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
        value
    );
    assert_eq!(decoded.reply, base.reply);

    for (path, bad) in [
        ("/rosterPlan", serde_json::Value::Null),
        ("/rosterPlan/rev", serde_json::json!("01")),
        ("/rosterPlan/obs", serde_json::json!("18446744073709551615")),
        ("/rosterPlan/gen", serde_json::json!("foreign-gen")),
        (
            "/rosterPlan/slots/0",
            serde_json::json!({"kind":"unknown","native":"d1"}),
        ),
        ("/rosterPlan/slots/1/native", serde_json::json!("d1")),
        ("/rosterPlan/members/0/deps", serde_json::json!(["view-1"])),
        (
            "/rosterPlan/views/0",
            serde_json::json!({"step":"view-1","extra":true}),
        ),
        ("/rosterPlan/focus/cmdSeq", serde_json::Value::Null),
        (
            "/rosterPlan/baseline/order",
            serde_json::json!((1..=26).map(|i| format!("d{i}")).collect::<Vec<_>>()),
        ),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(path).unwrap() = bad;
        assert!(
            decode_reply_v4(&invalid.to_string()).is_err(),
            "accepted {path}"
        );
    }
}

fn writer_focus_effect() -> DecodedReplyV4 {
    // Focus is the only effect kind and it never encodes: the writer
    // refusal below stays an effect-lane check. The duplicate-effect case
    // pins the pre-pass uniqueness gate; explicit/null-seq and
    // automatic/await coupling(shape) violations are decode-side checks
    // (see `decode_effect`) and have no encodable witness.
    let mut base = writer_base();
    base.reply.effects = vec![tiler_core::world_report::OneShotEffect {
        id: "effect-a".to_owned(),
        kind: tiler_core::world_report::EffectKind::Focus,
        target: tiler_core::directional::WindowId("win-a".to_owned()),
        issued_revision: 1,
        cause: tiler_core::world_report::EffectCause::ExplicitCommand,
        command_seq: Some(1),
        await_shown: false,
    }];
    base
}

#[test]
fn v4_reply_writer_refuses_unspellable() {
    // (case, kind, message, mutation)
    let mut over_cap = writer_base();
    over_cap.reply.outputs = vec![over_cap.reply.outputs[0].clone(); 17];
    let mut duplicate_window = writer_base();
    duplicate_window
        .reply
        .windows
        .push(duplicate_window.reply.windows[0].clone());
    let mut duplicate_effect = writer_focus_effect();
    duplicate_effect
        .reply
        .effects
        .push(duplicate_effect.reply.effects[0].clone());
    let mut duplicate_scope = writer_base();
    duplicate_scope
        .reply
        .scopes
        .push(duplicate_scope.reply.scopes[0].clone());
    let mut show_fallback = writer_base();
    show_fallback.reply.windows[0]
        .visibility
        .as_mut()
        .expect("visible")
        .fallback = true;
    let mut duplicate_intent = writer_base();
    duplicate_intent
        .reply
        .intent_desired
        .push(tiler_core::world_report::IntentDesire {
            win: tiler_core::directional::WindowId("win-a".to_owned()),
            data: Some(tiler_core::intent_data::IntentData::new(Some(true), None)),
        });
    duplicate_intent
        .reply
        .intent_desired
        .push(tiler_core::world_report::IntentDesire {
            win: tiler_core::directional::WindowId("win-a".to_owned()),
            data: None,
        });
    let mut warp_overbound = writer_base();
    warp_overbound.reply.cursor_warp = Some((32769, 0));
    let mut bad_rect = writer_base();
    bad_rect.reply.windows[0].allocation = Some(tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 5,
    });
    let mut bad_cause = writer_base();
    bad_cause
        .reply
        .operation_settled
        .push(tiler_core::world_report::OperationEvent {
            window: tiler_core::directional::WindowId("win-a".to_owned()),
            kind: tiler_core::world_report::OperationKind::Clear,
            operation_id: "operation-a".to_owned(),
            outcome: tiler_core::world_report::OperationOutcome::ClearContinuedTiled,
            cause: Some("bogus-cause".to_owned()),
        });
    let mut bad_id = writer_base();
    bad_id.reply.windows[0].lifetime = tiler_core::directional::WindowId(String::new());
    let mut bad_adapter = writer_base();
    bad_adapter.reply.adapter_incarnation = String::new();
    let mut bad_generation = writer_base();
    bad_generation.reply.core_generation = "GEN-A".to_owned();
    let mut bad_request = writer_base();
    bad_request.reply.request_id = String::new();
    let mut bad_version = writer_base();
    bad_version.reply.contract_version = 3;
    let mut state_dup =
        decode_reply_v4(&fixture_text("messages/reply-state-authority.json")).expect("decodes");
    let dup_target = state_dup.reply.windows[0].state[0];
    state_dup.reply.windows[0].state.push(dup_target);
    let mut state_over_cap =
        decode_reply_v4(&fixture_text("messages/reply-state-authority.json")).expect("decodes");
    state_over_cap.reply.windows[0].state = vec![
        tiler_core::world_report::StateTarget {
            property: tiler_core::world_report::StateProperty::Fullscreen,
            value: false,
            authority: tiler_core::world_report::ScopeAuthority::Controlled,
            command_seq: None,
        },
        tiler_core::world_report::StateTarget {
            property: tiler_core::world_report::StateProperty::Maximized,
            value: false,
            authority: tiler_core::world_report::ScopeAuthority::Controlled,
            command_seq: None,
        },
        tiler_core::world_report::StateTarget {
            property: tiler_core::world_report::StateProperty::Sticky,
            value: false,
            authority: tiler_core::world_report::ScopeAuthority::Controlled,
            command_seq: None,
        },
        tiler_core::world_report::StateTarget {
            property: tiler_core::world_report::StateProperty::Above,
            value: false,
            authority: tiler_core::world_report::ScopeAuthority::Controlled,
            command_seq: None,
        },
        tiler_core::world_report::StateTarget {
            property: tiler_core::world_report::StateProperty::Fullscreen,
            value: true,
            authority: tiler_core::world_report::ScopeAuthority::Controlled,
            command_seq: None,
        },
    ];
    let cases: [(&str, &str, &str, DecodedReplyV4); 16] = [
        ("over-cap", "snapshot-invalid", "section-over-cap", over_cap),
        (
            "duplicate-window",
            "snapshot-invalid",
            "duplicate-window",
            duplicate_window,
        ),
        (
            "duplicate-effect",
            "snapshot-invalid",
            "duplicate-effect",
            duplicate_effect,
        ),
        (
            "duplicate-scope",
            "snapshot-invalid",
            "duplicate-scope",
            duplicate_scope,
        ),
        (
            "show-fallback",
            "snapshot-invalid",
            "visibility-show-fallback",
            show_fallback,
        ),
        (
            "duplicate-intent-desire",
            "snapshot-invalid",
            "duplicate-intent-desire",
            duplicate_intent,
        ),
        (
            "warp-overbound",
            "unknown-value",
            "request contains an unknown value",
            warp_overbound,
        ),
        (
            "bad-rect",
            "unknown-value",
            "request contains an unknown value",
            bad_rect,
        ),
        (
            "bad-cause",
            "unknown-value",
            "request contains an unknown value",
            bad_cause,
        ),
        (
            "bad-id",
            "unknown-value",
            "request contains an unknown value",
            bad_id,
        ),
        (
            "bad-adapter",
            "unknown-value",
            "request contains an unknown value",
            bad_adapter,
        ),
        (
            "bad-generation",
            "unknown-value",
            "request contains an unknown value",
            bad_generation,
        ),
        (
            "bad-request",
            "unknown-value",
            "request contains an unknown value",
            bad_request,
        ),
        (
            "bad-version",
            "unsupported-version",
            "unsupported contract version",
            bad_version,
        ),
        (
            "state-dup",
            "snapshot-invalid",
            "state-target-count",
            state_dup,
        ),
        (
            "state-over-cap",
            "snapshot-invalid",
            "state-target-count",
            state_over_cap,
        ),
    ];
    for (case, kind, message, decoded) in cases {
        let error = encode_reply_v4(&decoded).expect_err("writer refuses {case}");
        assert_eq!(error.kind, kind, "kind {case}");
        assert_eq!(error.message, message, "message {case}");
        let count_error = count_reply_v4(&decoded.reply, &decoded.wire)
            .expect_err("counter refuses unspellable case");
        assert_eq!(count_error.kind, kind, "count kind {case}");
        assert_eq!(count_error.message, message, "count message {case}");
    }
}

#[test]
fn v4_reply_workspace_row_identity_cross_output_accepts() {
    // Row identity is the (output, id) pair: the same logical id on
    // different outputs is two rows, not a duplicate.
    let mut document = valid_reply_value();
    document["workspaces"] = serde_json::json!([
        {"id":"ws-a","out":"out-a","bind":"","scope":{"kind":"output","out":"out-a"},"order":0,"mode":"tiled","exist":true,"hold":""},
        {"id":"ws-a","out":"out-b","bind":"","scope":{"kind":"output","out":"out-b"},"order":0,"mode":"tiled","exist":true,"hold":""}
    ]);
    let decoded = decode_value(&document).expect("shared id across outputs decodes");
    let pairs: Vec<(&str, &str)> = decoded
        .reply
        .workspaces
        .iter()
        .map(|row| (row.output.0.as_str(), row.id.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![("out-a", "ws-a"), ("out-b", "ws-a")],
        "both rows survive decode"
    );
    let encoded = encode_reply_v4(&decoded).expect("shared id across outputs encodes");
    let back = decode_reply_v4(&encoded).expect("encoding re-decodes");
    assert_eq!(back, decoded, "cross-output rows round-trip");
}

#[test]
fn v4_reply_workspace_row_identity_exact_pair_refuses() {
    // Repeating the full (output, id) pair still refuses on both paths.
    let mut document = valid_reply_value();
    let row = document["workspaces"][0].clone();
    document["workspaces"]
        .as_array_mut()
        .expect("array")
        .push(row);
    let error = decode_value(&document).expect_err("repeated pair refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-workspace");

    let mut constructed = writer_base();
    constructed
        .reply
        .workspaces
        .push(constructed.reply.workspaces[0].clone());
    let error = encode_reply_v4(&constructed).expect_err("repeated pair refuses");
    assert_eq!(error.kind, "snapshot-invalid");
    assert_eq!(error.message, "duplicate-workspace");
}

#[test]
fn v4_reply_historical_close_focus_rejects() {
    // Frozen `test-fixtures/world-wire/v4` predates the retained
    // `focusRequest` lane: every historical valid-wire WorldReply carries
    // the retired `closeFocus` key and no `focusRequest`. The current
    // strict shape refuses each one as `unknown-field` (the retired key),
    // never as a silently accepted history.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4");
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("manifest.json")).expect("historical manifest reads"),
    )
    .expect("historical manifest parses");
    let valid: Vec<_> = manifest
        .fixtures
        .into_iter()
        .filter(|fixture| fixture.family == "WorldReply" && fixture.expectation == "valid-wire")
        .collect();
    assert_eq!(valid.len(), 115, "historical valid-wire WorldReply count");
    for fixture in &valid {
        let text =
            std::fs::read_to_string(root.join(&fixture.file)).expect("historical fixture reads");
        let value: serde_json::Value = serde_json::from_str(&text).expect("historical parses");
        let object = value.as_object().expect("historical object");
        assert!(
            object.contains_key("closeFocus"),
            "{} carries the retired lane",
            fixture.id
        );
        assert!(
            !object.contains_key("focusRequest"),
            "{} predates the retained lane",
            fixture.id
        );
        let error = decode_reply_v4(&text).expect_err("historical reply refuses");
        assert_eq!(error.kind, "unknown-field", "kind {}", fixture.id);
        assert_eq!(
            error.message, "request contains an unknown field",
            "message {}",
            fixture.id
        );
    }
}

#[test]
fn v4_reply_operation_and_switch_settled_strict_keys_and_round_trip() {
    // Same op under distinct (win, kind) phases is accepted; duplicate (win, kind)
    // or kind/outcome mismatch or duplicate switch out is rejected.
    let mut doc = valid_reply_value();
    doc["operationSettled"] = serde_json::json!([
        {"win":"win-a","kind":"clear","op":"op-1","outcome":"continued-float","cause":""},
        {"win":"win-a","kind":"float","op":"op-1","outcome":"float-frame-proven","cause":""}
    ]);
    doc["switchSettled"] = serde_json::json!([
        {"out":"out-a","op":"op-2","outcome":"complete","failed":[],"focus":"proven","fallback":false},
        {"out":"out-b","op":"op-3","outcome":"completed-with-failures","failed":["win-c"],"focus":"unproven","fallback":false}
    ]);
    let decoded = decode_value(&doc).expect("distinct phase/output keys decode");
    let wire = encode_reply_v4(&decoded).expect("encodes");
    assert_eq!(
        count_reply_v4(&decoded.reply, &decoded.wire).expect("writer counts"),
        wire.len()
    );
    let redecoded = decode_reply_v4(&wire).expect("re-decodes");
    assert_eq!(redecoded, decoded);

    let mut dup_op = doc.clone();
    dup_op["operationSettled"] = serde_json::json!([
        {"win":"win-a","kind":"clear","op":"op-1","outcome":"continued-tiled","cause":""},
        {"win":"win-a","kind":"clear","op":"op-2","outcome":"superseded","cause":""}
    ]);
    let err = decode_value(&dup_op).expect_err("duplicate (win, kind) rejects");
    assert_eq!(err.message, "duplicate-operation");

    let mut mismatch = doc.clone();
    mismatch["operationSettled"] = serde_json::json!([
        {"win":"win-a","kind":"clear","op":"op-1","outcome":"float-frame-proven","cause":""}
    ]);
    let err = decode_value(&mismatch).expect_err("kind/outcome mismatch rejects");
    assert_eq!(err.kind, "unknown-value");

    let mut dup_switch = doc;
    dup_switch["switchSettled"] = serde_json::json!([
        {"out":"out-a","op":"op-2","outcome":"complete","failed":[],"focus":"proven","fallback":false},
        {"out":"out-a","op":"op-3","outcome":"complete","failed":[],"focus":"unproven","fallback":false}
    ]);
    let err = decode_value(&dup_switch).expect_err("duplicate out rejects");
    assert_eq!(err.message, "duplicate-switch");
}

#[test]
fn v4_reply_cap_valid_near_4mib_windows_encodes_and_roundtrips() {
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::world_report::{
        MAX_INTENT_DESIRED, MAX_REPLY_DIAGNOSTICS, MAX_REPLY_NATIVE_CREATIONS,
        MAX_REPLY_OPERATIONS, MAX_REPLY_OUTPUTS, MAX_REPLY_SCOPES, MAX_REPLY_SWITCH_EVENTS,
        MAX_REPLY_TRANSFERS, MAX_REPLY_WINDOWS, MAX_REPLY_WORKSPACES, OperationEvent,
        OperationKind, OperationOutcome, ScopeAuthority, StateProperty, StateTarget, WindowDesire,
    };
    use tiler_protocol::world_protocol::WORLD_MAX_REPLY_BYTES;

    // Paired with `staged_pending_operation_future_budget_rejects_over_4mib_transactionally`
    // in `crates/tiler-core/src/world_publish_v4_tests.rs`: same deterministic
    // cap-valid `WindowDesire` padding recipe, proved through production `encode_reply_v4`.
    let mut decoded = decode_value(&valid_reply_value()).expect("base reply decodes");
    decoded.reply.windows.clear();
    decoded.reply.operation_settled = vec![OperationEvent {
        window: WindowId("w-0".to_owned()),
        kind: OperationKind::Clear,
        operation_id: "op-0".to_owned(),
        outcome: OperationOutcome::ClearContinuedTiled,
        cause: None,
    }];

    let pad_win = |i: usize| WindowDesire {
        lifetime: WindowId(format!("w-{i:05}-{}", "x".repeat(120))),
        output: OutputId(format!("o-{}", "y".repeat(126))),
        workspace: format!("ws-{}", "z".repeat(125)),
        allocation: Some(Rect {
            x: 100,
            y: 100,
            w: 1000,
            h: 1000,
        }),
        effective_frame: Some(Rect {
            x: 100,
            y: 100,
            w: 1000,
            h: 1000,
        }),
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        floating: false,
        float_origin: None,
        frame_operation: Some(format!("op-f-{i:05}-{}", "f".repeat(115))),
        clear_operation: Some(format!("op-c-{i:05}-{}", "c".repeat(115))),
        command_seq: None,
        visibility: None,
        visibility_authority: ScopeAuthority::Untouched,
        state: vec![
            StateTarget {
                property: StateProperty::Fullscreen,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Maximized,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Sticky,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
            StateTarget {
                property: StateProperty::Above,
                value: false,
                authority: ScopeAuthority::Untouched,
                command_seq: None,
            },
        ],
    };

    decoded.reply.windows.push(pad_win(0));
    let after_one = count_reply_v4(&decoded.reply, &decoded.wire).expect("one row counts");
    decoded.reply.windows.push(pad_win(1));
    let after_two = count_reply_v4(&decoded.reply, &decoded.wire).expect("two rows count");
    let row_step = after_two - after_one;
    let target_bytes = WORLD_MAX_REPLY_BYTES - 500;
    let extra = target_bytes.saturating_sub(after_two) / row_step;
    decoded.reply.windows.extend((2..2 + extra).map(pad_win));

    assert!(decoded.reply.scopes.len() <= MAX_REPLY_SCOPES);
    assert!(decoded.reply.workspaces.len() <= MAX_REPLY_WORKSPACES);
    assert!(decoded.reply.outputs.len() <= MAX_REPLY_OUTPUTS);
    assert!(decoded.reply.windows.len() <= MAX_REPLY_WINDOWS);
    assert!(decoded.reply.transfers.len() <= MAX_REPLY_TRANSFERS);
    assert!(decoded.reply.effects.is_empty());
    assert!(decoded.reply.operation_settled.len() <= MAX_REPLY_OPERATIONS);
    assert!(decoded.reply.native_creations.len() <= MAX_REPLY_NATIVE_CREATIONS);
    assert!(decoded.reply.intent_desired.len() <= MAX_INTENT_DESIRED);
    assert!(decoded.reply.switch_settled.len() <= MAX_REPLY_SWITCH_EVENTS);
    assert!(decoded.reply.diagnostics.len() <= MAX_REPLY_DIAGNOSTICS);
    assert!(
        count_reply_v4(&decoded.reply, &decoded.wire).expect("writer counts")
            <= WORLD_MAX_REPLY_BYTES
    );

    let wire = encode_reply_v4(&decoded).expect("cap-valid ~4 MiB reply encodes");
    assert!(wire.len() <= WORLD_MAX_REPLY_BYTES);
    assert_eq!(
        wire.len(),
        count_reply_v4(&decoded.reply, &decoded.wire).expect("writer counts")
    );
    let redecoded = decode_reply_v4(&wire).expect("encoded ~4 MiB reply re-decodes");
    assert_eq!(redecoded, decoded);
}

#[test]
fn v4_reply_drag_preview_strict_validation_and_bounds() {
    use tiler_core::contract::DragSide;
    use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
    use tiler_core::geometry::Rect;
    use tiler_core::policy::PriorGroupEdge;
    use tiler_core::session::{DomainKey, DragHoverPrior};
    use tiler_core::world_presentation::{GroupHideReason, PresentationUnderlay};
    use tiler_core::world_report::{
        BorderHideReasonV4, BorderPresentationV4, DragPreviewV4, WORLD_CONTRACT_VERSION_V4,
        WorldPresentationV4, WorldReplyV4,
    };
    use tiler_protocol::world_protocol::ReplyWireOptionals;

    let domain = DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: WorkspaceId("ws-a".to_owned()),
    };
    let preview = DragPreviewV4 {
        id: 7,
        rect: Rect {
            x: 8,
            y: 8,
            w: 400,
            h: 600,
        },
        hover: Some(DragHoverPrior {
            domain: domain.clone(),
            source_leaf: NodeId("leaf-a".to_owned()),
            source_window: WindowId("win-a".to_owned()),
            revision: 3,
            prior: Some(PriorGroupEdge {
                group: NodeId("group-a".to_owned()),
                edge: DragSide::Left,
            }),
            prior_domain: domain.clone(),
        }),
        domain: domain.clone(),
        source: domain,
        target: WindowId("win-a".to_owned()),
        revision: 2,
    };
    let base_decoded = DecodedReplyV4 {
        roster_plan: None,
        reply: WorldReplyV4 {
            contract_version: WORLD_CONTRACT_VERSION_V4,
            adapter_incarnation: "adapter-a".to_owned(),
            core_generation: "generation-a".to_owned(),
            request_id: "request-a".to_owned(),
            based_on_observation_seq: 1,
            state_revision: 2,
            ready: true,
            command_consumed: false,
            scopes: vec![],
            workspaces: vec![],
            outputs: vec![],
            windows: vec![],
            transfers: vec![],
            effects: vec![],
            focus_request: None,
            drag_preview: Some(preview),
            gesture_settled: None,
            operation_settled: vec![],
            native_creations: vec![],
            cursor_warp: None,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 2,
                suspended_output: None,
                border: BorderPresentationV4 {
                    visible: false,
                    reason: Some(BorderHideReasonV4::NoTarget),
                    subject: None,
                    outsets: None,
                    width: None,
                    radius: None,
                    witness: None,
                },
                underlay: PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: GroupHideReason::NoGroup,
                },
                pager: vec![],
            },
            intent_desired: vec![],
            switch_settled: vec![],
            diagnostics: vec![],
        },
        wire: ReplyWireOptionals {
            suspended_output_present: false,
        },
    };
    let base_wire = encode_reply_v4(&base_decoded).expect("encodes");
    assert_eq!(
        count_reply_v4(&base_decoded.reply, &base_decoded.wire).expect("writer counts"),
        base_wire.len()
    );
    assert_eq!(decode_reply_v4(&base_wire).expect("decodes"), base_decoded);

    let base_doc: serde_json::Value = serde_json::from_str(&base_wire).expect("parses");

    // Missing required dragPreview key refuses as malformed.
    let mut missing = base_doc.clone();
    missing.as_object_mut().unwrap().remove("dragPreview");
    assert_eq!(
        decode_value(&missing)
            .expect_err("missing dragPreview")
            .kind,
        "malformed"
    );

    // Retired result key refuses as unknown-field (no alias or dual shape).
    let mut old_result = base_doc.clone();
    old_result
        .as_object_mut()
        .unwrap()
        .insert("result".to_owned(), serde_json::Value::Null);
    assert_eq!(
        decode_value(&old_result).expect_err("old result key").kind,
        "unknown-field"
    );

    // Old op key inside dragPreview refuses as unknown-field.
    let mut with_op = base_doc.clone();
    with_op["dragPreview"]["op"] = serde_json::json!("drag-preview");
    assert_eq!(
        decode_value(&with_op)
            .expect_err("op inside dragPreview")
            .kind,
        "unknown-field"
    );

    // Non-object dragPreview refuses as malformed.
    let mut non_obj = base_doc.clone();
    non_obj["dragPreview"] = serde_json::json!([]);
    assert_eq!(
        decode_value(&non_obj)
            .expect_err("non-object dragPreview")
            .kind,
        "malformed"
    );

    // Missing id or win inside dragPreview refuses as malformed.
    for key in ["id", "win", "hover"] {
        let mut dropped = base_doc.clone();
        dropped["dragPreview"].as_object_mut().unwrap().remove(key);
        assert_eq!(
            decode_value(&dropped).expect_err("missing key").kind,
            "malformed",
            "missing {key}"
        );
    }

    // id must be a bounded canonical quoted u64.
    let mut num_id = base_doc.clone();
    num_id["dragPreview"]["id"] = serde_json::json!(7);
    assert_eq!(
        decode_value(&num_id).expect_err("numeric id").kind,
        "malformed"
    );
    for bad_id in ["", "01", "18446744073709551616"] {
        let mut doc = base_doc.clone();
        doc["dragPreview"]["id"] = serde_json::json!(bad_id);
        assert_eq!(
            decode_value(&doc).expect_err("bad id").kind,
            "unknown-value",
            "id={bad_id}"
        );
    }

    // Malformed rect inside dragPreview refuses.
    let mut bad_rect = base_doc;
    bad_rect["dragPreview"]["rect"] = serde_json::json!([8, 8, 0, 600]);
    assert_eq!(
        decode_value(&bad_rect).expect_err("zero-width rect").kind,
        "unknown-value"
    );
}

#[test]
fn v4_reply_writer_after_shown_duplicate_and_later_error_precedence() {
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::world_report::{AfterShown, AfterShownMember, FocusRequest, FocusRequestCause};

    let mut dup_only = decode_ok("messages/reply-observe.json");
    dup_only.reply.focus_request = Some(FocusRequest {
        id: "focus-1".to_owned(),
        target: WindowId("win-a".to_owned()),
        issued_revision: 1,
        cause: FocusRequestCause::ExplicitCommand,
        output: OutputId("out-a".to_owned()),
        workspace: "ws-a".to_owned(),
        cmd_seq: Some(1),
        after_shown: Some(AfterShown {
            op: "op-1".to_owned(),
            obs: 1,
            members: vec![
                AfterShownMember {
                    win: WindowId("win-a".to_owned()),
                    out: OutputId("out-a".to_owned()),
                    ws: "ws-a".to_owned(),
                    frame: None,
                },
                AfterShownMember {
                    win: WindowId("win-a".to_owned()),
                    out: OutputId("out-a".to_owned()),
                    ws: "ws-a".to_owned(),
                    frame: None,
                },
            ],
            views: Vec::new(),
            selected: Vec::new(),
            departing: Vec::new(),
        }),
    });

    let err = encode_reply_v4(&dup_only).expect_err("duplicate afterShown member refuses");
    assert_eq!(err.kind, "snapshot-invalid");
    assert_eq!(err.message, "encode-mismatch");
    let count_err =
        count_reply_v4(&dup_only.reply, &dup_only.wire).expect_err("count matches encode error");
    assert_eq!(count_err, err);

    let mut with_later_error = dup_only;
    with_later_error.reply.presentation.border.width = Some(-1);
    let later_err =
        encode_reply_v4(&with_later_error).expect_err("later writer error takes precedence");
    assert_eq!(later_err.kind, "unknown-value");
    assert_eq!(later_err.message, "request contains an unknown value");
    let later_count_err = count_reply_v4(&with_later_error.reply, &with_later_error.wire)
        .expect_err("count matches later writer error");
    assert_eq!(later_count_err, later_err);
}

fn legacy_bound_oracle(
    decoded: &DecodedReplyV4,
    roster_plan: Option<&serde_json::Value>,
    bind: &BrokerReplyBind<'_>,
) -> String {
    let encoded = write_reply_v4(&decoded.reply, &decoded.wire).expect("oracle inner encodes");
    let mut val: serde_json::Value = serde_json::from_str(&encoded).expect("oracle json");
    let map = val.as_object_mut().expect("object");
    if let Some(plan) = roster_plan {
        map.insert("rosterPlan".to_owned(), plan.clone());
    }
    let assoc_val = |a: Option<BrokerGestureAssoc<'_>>| match a {
        Some(a) => serde_json::json!({
            "instance": a.instance,
            "serial": a.serial,
            "coreId": a.core_id.to_string(),
        }),
        None => serde_json::Value::Null,
    };
    map.insert(
        "source".to_owned(),
        serde_json::Value::String(bind.source.to_owned()),
    );
    map.insert(
        "nativeInvalid".to_owned(),
        serde_json::Value::String(bind.native_invalid.to_string()),
    );
    map.insert(
        "clock".to_owned(),
        serde_json::Value::String(bind.clock.to_string()),
    );
    map.insert("gestureAssoc".to_owned(), assoc_val(bind.gesture_assoc));
    map.insert(
        "gestureSettledAssoc".to_owned(),
        assoc_val(bind.gesture_settled_assoc),
    );
    val.to_string()
}

fn legacy_projection_oracle(decoded: &DecodedReplyV4, producer: Option<(&str, &str)>) -> String {
    let encoded = encode_reply_v4(decoded).expect("oracle inner encodes");
    let val: serde_json::Value = serde_json::from_str(&encoded).expect("oracle json");
    let get = |k: &str| val.get(k).cloned().unwrap_or(serde_json::Value::Null);
    let mut windows = get("windows");
    if let Some(rows) = windows.as_array_mut() {
        for row in rows {
            if let Some(map) = row.as_object_mut() {
                map.remove("frame_op");
                map.remove("clear_op");
            }
        }
    }
    let suspended = val
        .get("dragPreview")
        .and_then(|p| p.get("domainOut"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|d| {
            val.get("presentation")
                .and_then(|p| p.get("suspendedOutput"))
                .and_then(serde_json::Value::as_str)
                == Some(d)
        });
    let preview = if suspended {
        serde_json::Value::Null
    } else if let Some(p) = val.get("dragPreview").and_then(|v| v.as_object()) {
        let mut obj = serde_json::Map::new();
        obj.insert("rect".to_owned(), p["rect"].clone());
        obj.insert("sourceOut".to_owned(), p["sourceOut"].clone());
        obj.insert("sourceWs".to_owned(), p["sourceWs"].clone());
        obj.insert("win".to_owned(), p["win"].clone());
        match producer {
            Some((inst, ser)) => {
                obj.insert("producerInstance".to_owned(), serde_json::json!(inst));
                obj.insert("producerSerial".to_owned(), serde_json::json!(ser));
            }
            None => {
                obj.insert("producerInstance".to_owned(), serde_json::Value::Null);
                obj.insert("producerSerial".to_owned(), serde_json::Value::Null);
            }
        }
        serde_json::Value::Object(obj)
    } else {
        serde_json::Value::Null
    };
    serde_json::json!({
        "v": 4,
        "outcome": "presentation",
        "rev": get("rev"),
        "obs": get("obs"),
        "scopes": get("scopes"),
        "workspaces": get("workspaces"),
        "outputs": get("outputs"),
        "windows": windows,
        "presentation": get("presentation"),
        "preview": preview,
    })
    .to_string()
}

#[test]
fn v4_bound_and_projection_match_stage5_fixtures_and_serde_oracle_byte_for_byte() {
    let stage5_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/stage5-v4-active/full");
    let strip_and_decode = |raw: &str| -> (DecodedReplyV4, serde_json::Value) {
        let mut v: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
        let saved = v.clone();
        let map = v.as_object_mut().expect("object");
        for k in [
            "source",
            "nativeInvalid",
            "clock",
            "gestureAssoc",
            "gestureSettledAssoc",
        ] {
            map.remove(k);
        }
        (
            decode_reply_v4(&v.to_string()).expect("inner decodes"),
            saved,
        )
    };

    // 1. Exact byte-for-byte match against stage5-v4-active/full/reply-genesis-0.json
    let genesis_raw = std::fs::read_to_string(stage5_dir.join("reply-genesis-0.json"))
        .expect("reply-genesis-0.json");
    let genesis_trimmed = genesis_raw.trim_end_matches('\n');
    let (genesis_decoded, genesis_saved) = strip_and_decode(genesis_trimmed);
    let genesis_bind = BrokerReplyBind {
        source: genesis_saved["source"].as_str().unwrap(),
        native_invalid: genesis_saved["nativeInvalid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        clock: genesis_saved["clock"].as_str().unwrap().parse().unwrap(),
        gesture_assoc: None,
        gesture_settled_assoc: None,
    };
    assert!(
        genesis_decoded.roster_plan.is_some(),
        "active genesis carries slot authority"
    );
    let actual_genesis = write_bound_reply_v4(
        &genesis_decoded.reply,
        &genesis_decoded.wire,
        genesis_decoded.roster_plan.as_ref(),
        &genesis_bind,
    )
    .expect("write_bound_reply_v4");
    assert_eq!(actual_genesis, genesis_trimmed);
    assert_eq!(
        actual_genesis,
        legacy_bound_oracle(
            &genesis_decoded,
            genesis_saved.get("rosterPlan"),
            &genesis_bind
        )
    );

    // 2. Exact byte-for-byte match against stage5-v4-active/full/reply-preview-0.json
    // and projection-preview.json (with read-time gen/req/covered fields added).
    let preview_raw = std::fs::read_to_string(stage5_dir.join("reply-preview-0.json"))
        .expect("reply-preview-0.json");
    let preview_trimmed = preview_raw.trim_end_matches('\n');
    let (preview_decoded, preview_saved) = strip_and_decode(preview_trimmed);
    let preview_bind = BrokerReplyBind {
        source: preview_saved["source"].as_str().unwrap(),
        native_invalid: preview_saved["nativeInvalid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        clock: preview_saved["clock"].as_str().unwrap().parse().unwrap(),
        gesture_assoc: Some(BrokerGestureAssoc {
            instance: preview_saved["gestureAssoc"]["instance"].as_str().unwrap(),
            serial: preview_saved["gestureAssoc"]["serial"].as_str().unwrap(),
            core_id: preview_saved["gestureAssoc"]["coreId"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        }),
        gesture_settled_assoc: None,
    };
    let actual_preview_bound = write_bound_reply_v4(
        &preview_decoded.reply,
        &preview_decoded.wire,
        preview_decoded.roster_plan.as_ref(),
        &preview_bind,
    )
    .expect("write_bound_reply_v4");
    assert_eq!(actual_preview_bound, preview_trimmed);
    assert_eq!(
        actual_preview_bound,
        legacy_bound_oracle(
            &preview_decoded,
            preview_saved.get("rosterPlan"),
            &preview_bind
        )
    );

    let proj_fixture_raw = std::fs::read_to_string(stage5_dir.join("projection-preview.json"))
        .expect("projection-preview.json");
    let proj_fixture_trimmed = proj_fixture_raw.trim_end_matches('\n');
    let actual_proj = write_projection_reply_v4(
        &preview_decoded.reply,
        &preview_decoded.wire,
        Some(("gesture-1", "1")),
    )
    .expect("write_projection_reply_v4");
    let mut proj_val: serde_json::Value = serde_json::from_str(&actual_proj).expect("proj json");
    let proj_fixture_val: serde_json::Value =
        serde_json::from_str(proj_fixture_trimmed).expect("fixture proj json");
    if let Some(map) = proj_val.as_object_mut() {
        for k in ["gen", "req", "coveredInstance", "coveredInvalid"] {
            map.insert(k.to_owned(), proj_fixture_val[k].clone());
        }
    }
    assert_eq!(proj_val.to_string(), proj_fixture_trimmed);

    // 3. Constructed rich reply with IntentDesire nested data + C0/serde escapes,
    // trailing=true, suspended_output_present=true/false, and full oracle comparison.
    let mut rich = decode_ok("messages/reply-observe.json");
    rich.reply
        .workspaces
        .push(tiler_core::world_report::WorkspaceDesire {
            id: "ws-trail".to_owned(),
            output: tiler_core::directional::OutputId("out-a".to_owned()),
            native_binding: None,
            scope: tiler_core::world_report::AuthorityScope::Output(
                tiler_core::directional::OutputId("out-a".to_owned()),
            ),
            order: 9,
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing: true,
        });
    rich.reply
        .intent_desired
        .push(tiler_core::world_report::IntentDesire {
            win: tiler_core::directional::WindowId("win-a".to_owned()),
            data: Some(tiler_core::intent_data::IntentData {
                float: Some(true),
                sticky_prior: Some(false),
            }),
        });
    rich.reply
        .presentation
        .pager
        .push(tiler_core::world_presentation::PagerScreen {
            output: "out-a".to_owned(),
            workspaces: vec![tiler_core::world_presentation::PagerWorkspace {
                id: "ws-trail".to_owned(),
                ordinal: 1,
                label: "Line1\nTab\tQuote\"Slash\\Ctrl\x08\x0c\x01-\u{2603}".to_owned(),
                current: true,
                occupied: false,
                contains_focused: false,
                trailing: true,
                windows: Vec::new(),
            }],
        });
    rich.reply.presentation.suspended_output =
        Some(tiler_core::directional::OutputId("out-a".to_owned()));
    rich.wire.suspended_output_present = true;

    let rich_bind = BrokerReplyBind {
        source: "src-rich-1",
        native_invalid: 42,
        clock: 999,
        gesture_assoc: Some(BrokerGestureAssoc {
            instance: "g-inst",
            serial: "7",
            core_id: 3,
        }),
        gesture_settled_assoc: Some(BrokerGestureAssoc {
            instance: "g-inst-0",
            serial: "6",
            core_id: 2,
        }),
    };
    let actual_rich_bound =
        write_bound_reply_v4(&rich.reply, &rich.wire, None, &rich_bind).expect("rich bound");
    let oracle_rich_bound = legacy_bound_oracle(&rich, None, &rich_bind);
    assert_eq!(actual_rich_bound, oracle_rich_bound);

    let actual_rich_proj =
        write_projection_reply_v4(&rich.reply, &rich.wire, None).expect("rich proj");
    let oracle_rich_proj = legacy_projection_oracle(&rich, None);
    assert_eq!(actual_rich_proj, oracle_rich_proj);
}

#[test]
fn underlay_unrepresentable_roundtrips_and_unknown_reason_rejects() {
    use tiler_core::world_presentation::{GroupHideReason, PresentationUnderlay};

    let mut decoded = decode_ok("messages/reply-observe.json");
    decoded.reply.presentation.underlay = PresentationUnderlay::Hidden {
        base_revision: Some(5),
        reason: GroupHideReason::Unrepresentable,
    };
    let wire = encode_reply_v4(&decoded).expect("unrepresentable underlay encodes");
    assert!(
        wire.contains("\"reason\":\"unrepresentable\""),
        "wire carries unrepresentable reason: {wire}"
    );
    let reparsed = decode_reply_v4(&wire).expect("unrepresentable underlay decodes");
    assert_eq!(
        reparsed.reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: Some(5),
            reason: GroupHideReason::Unrepresentable,
        }
    );

    let mut value: serde_json::Value = serde_json::from_str(&wire).expect("parses as json");
    value["presentation"]["underlay"]["reason"] = serde_json::Value::String("bogus-reason".into());
    let err = decode_reply_v4(&value.to_string()).expect_err("unknown reason refuses");
    assert_eq!(err.kind, "unknown-value");
}

#[test]
fn transfer_parent_required_nullable_bounded_opaque_id_codec_semantics() {
    let mut decoded = decode_ok("messages/reply-transfer-view.json");
    assert_eq!(decoded.reply.transfers[0].parent, None);

    decoded.reply.transfers[0].parent = Some("opaque-parent-7".to_owned());
    let wire = encode_reply_v4(&decoded).expect("explicit opaque parent encodes");
    assert!(wire.contains("\"parent\":\"opaque-parent-7\""));
    let reparsed = decode_reply_v4(&wire).expect("explicit opaque parent decodes");
    assert_eq!(
        reparsed.reply.transfers[0].parent.as_deref(),
        Some("opaque-parent-7")
    );

    let max_parent = "p".repeat(128);
    decoded.reply.transfers[0].parent = Some(max_parent.clone());
    let max_wire = encode_reply_v4(&decoded).expect("128-byte opaque parent encodes");
    let max_reparsed = decode_reply_v4(&max_wire).expect("128-byte opaque parent decodes");
    assert_eq!(
        max_reparsed.reply.transfers[0].parent.as_deref(),
        Some(max_parent.as_str())
    );

    let mut missing: serde_json::Value = serde_json::from_str(&wire).expect("json");
    missing["transfers"][0]
        .as_object_mut()
        .expect("transfer object")
        .remove("parent");
    let missing_err =
        decode_reply_v4(&missing.to_string()).expect_err("missing parent key must refuse");
    assert_eq!(missing_err.kind, "malformed");

    for bad in [
        serde_json::Value::String(String::new()),
        serde_json::Value::String("p".repeat(129)),
        serde_json::json!(42),
    ] {
        let mut invalid: serde_json::Value = serde_json::from_str(&wire).expect("json");
        invalid["transfers"][0]["parent"] = bad;
        let err = decode_reply_v4(&invalid.to_string()).expect_err("invalid parent refuses");
        assert_eq!(err.kind, "unknown-value");
    }
}
