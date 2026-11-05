//! Staged v4 whole-reply decoder (5a): manifest-driven coverage for
//! [`decode_reply_v4`](tiler_protocol::world_protocol::decode_reply_v4)
//! over the `test-fixtures/world-wire/v4` WorldReply fixtures: 114
//! valid-wire accepts stable under re-parse, 23 context-free reject-wire
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
    assert_eq!(fixtures.len(), 140, "WorldReply fixture count");
    let valid: Vec<_> = fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "valid-wire")
        .collect();
    let reject: Vec<_> = fixtures
        .iter()
        .filter(|fixture| fixture.expectation == "reject-wire")
        .collect();
    assert_eq!(valid.len(), 114, "valid-wire count");
    assert_eq!(reject.len(), 26, "reject-wire count");
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
    assert_eq!(context_free, 23, "context-free reject count");
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
    assert_eq!(count, 114, "every valid-wire reply decodes");
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
    assert!(observe.reply.command_result.is_none(), "observe no result");
    assert!(observe.reply.cursor_warp.is_none(), "observe no warp");
    assert!(
        !observe.wire.suspended_output_present,
        "observe no suspension"
    );
    let focus = decode_ok("messages/reply-effect-focus.json");
    assert_eq!(focus.reply.effects.len(), 1);
    assert_eq!(
        focus.reply.effects[0].command_seq,
        Some(1),
        "explicit effect carries its command"
    );
    let float = decode_ok("messages/reply-result-float-placed.json");
    assert!(float.reply.command_result.is_some(), "float carries result");
    let gesture = decode_ok("messages/reply-gesture-committed.json");
    let settled = gesture.reply.gesture_settled.expect("terminal present");
    assert_eq!(settled.id, 1);
    assert!(settled.committed);
    let switch = decode_ok("messages/reply-switch-proven.json");
    assert_eq!(switch.reply.switch_settled.len(), 1);
    let diagnostics = decode_ok("messages/reply-row-diagnostics.json");
    assert_eq!(diagnostics.reply.diagnostics.len(), 5);
    let intent = decode_ok("messages/reply-intent-save-clear.json");
    assert_eq!(intent.reply.intent_writes.len(), 2);
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
        "invalid-effect-explicit-null-seq" => (
            "snapshot-invalid",
            "effect-explicit-null-seq",
            "messages/reply-effect-focus.json",
            "/effects/0/cmdSeq",
        ),
        "invalid-effect-automatic-await-shown" => (
            "snapshot-invalid",
            "effect-automatic-await-shown",
            "messages/reply-effect-automatic-admission.json",
            "/effects/0/awaitShown",
        ),
        "invalid-effect-unknown-cause" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-effect-focus.json",
            "/effects/0/cause",
        ),
        "invalid-result-applied-wrapper" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-result-maximize.json",
            "/result",
        ),
        "invalid-result-noop-wrapper" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-observe.json",
            "/result",
        ),
        "invalid-result-float-false-marker" => (
            "snapshot-invalid",
            "result-float-false-marker",
            "messages/reply-result-float-placed.json",
            "/result/noSpace",
        ),
        "invalid-directional-cause-string-empty" => (
            "unknown-value",
            "request contains an unknown value",
            "messages/reply-directional-cause-focus-present.json",
            "/result/cause",
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
    assert_eq!(count, 23, "every context-free reject refuses then repairs");
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
    // One-shot raise/stack effects are unaffected by the above gate.
    let stack = fixture_text("messages/reply-effect-stack.json");
    decode_reply_v4_for_host(&stack, &emulated_host())
        .expect("raise/stack effects pass without above");

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

    // Float marker, effect correlation, and state-count rules.
    let mut marker = valid_reply_value();
    marker["result"] = serde_json::json!({
        "op": "toggle-float", "win": "win-a", "rect": [10, 20, 640, 480], "noSpace": false
    });
    let error = decode_value(&marker).expect_err("false marker refuses");
    assert_eq!(error.message, "result-float-false-marker");
    let mut effect = valid_reply_value();
    effect["effects"] = serde_json::json!([{
        "id": "effect-a", "kind": "focus", "target": "win-a", "rev": "1",
        "cause": "automatic-admission", "cmdSeq": "1", "awaitShown": false
    }]);
    let error = decode_value(&effect).expect_err("non-explicit seq refuses");
    assert_eq!(error.message, "effect-non-explicit-seq");
    let mut state = valid_reply_value();
    state["windows"][0]["state"] = serde_json::json!([
        {"property": "fullscreen", "value": false, "auth": "controlled", "reason": "", "cmdSeq": null},
        {"property": "fullscreen", "value": true, "auth": "controlled", "reason": "", "cmdSeq": null}
    ]);
    let error = decode_value(&state).expect_err("duplicate property refuses");
    assert_eq!(error.message, "state-target-count");

    // Overbound intent data refuses with the typed helper token.
    let mut intent = valid_reply_value();
    intent["intentWrites"] = serde_json::json!([{"win": "win-a", "data": "x".repeat(1025)}]);
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
            serde_json::json!({"op":"transfer-a","kind":"window","win":"win-a","out":"out-a","ws":"ws-a","follow":true,"held":false,"cmdSeq":"1"}),
            "duplicate-transfer",
        ),
        (
            "operationSettled",
            serde_json::json!({"op":"operation-a","win":"win-a","outcome":"continued-tiled","cause":""}),
            "duplicate-operation",
        ),
        (
            "nativeCreations",
            serde_json::json!({"op":"create-a","ws":"ws-new","out":"out-a"}),
            "duplicate-native-creation",
        ),
        (
            "intentWrites",
            serde_json::json!({"win":"win-a","data":null}),
            "duplicate-intent-write",
        ),
        (
            "switchSettled",
            serde_json::json!({"op":"view-a","outcome":"complete","failed":[],"focus":"proven","fallback":false}),
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
    assert_eq!(count, 114, "every valid-wire reply round-trips");
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

fn writer_effect_focus() -> DecodedReplyV4 {
    decode_reply_v4(&fixture_text("messages/reply-effect-focus.json")).expect("decodes")
}

fn writer_effect_automatic() -> DecodedReplyV4 {
    decode_reply_v4(&fixture_text(
        "messages/reply-effect-automatic-admission.json",
    ))
    .expect("decodes")
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
    let mut duplicate_effect = writer_effect_focus();
    duplicate_effect
        .reply
        .effects
        .push(duplicate_effect.reply.effects[0].clone());
    let mut duplicate_scope = writer_base();
    duplicate_scope
        .reply
        .scopes
        .push(duplicate_scope.reply.scopes[0].clone());
    let mut explicit_null_seq = writer_effect_focus();
    explicit_null_seq.reply.effects[0].command_seq = None;
    let mut non_explicit_seq = writer_effect_automatic();
    non_explicit_seq.reply.effects[0].command_seq = Some(1);
    let mut automatic_await = writer_effect_automatic();
    automatic_await.reply.effects[0].await_shown = true;
    let mut show_fallback = writer_base();
    show_fallback.reply.windows[0]
        .visibility
        .as_mut()
        .expect("visible")
        .fallback = true;
    let mut intent_overbound = writer_base();
    intent_overbound
        .reply
        .intent_writes
        .push(tiler_core::world_report::IntentWrite {
            win: tiler_core::directional::WindowId("win-a".to_owned()),
            data: Some("x".repeat(1025)),
        });
    let mut warp_overbound = writer_base();
    warp_overbound.reply.cursor_warp = Some((16385, 0));
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
            operation_id: "operation-a".to_owned(),
            window: tiler_core::directional::WindowId("win-a".to_owned()),
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
    let cases: [(&str, &str, &str, DecodedReplyV4); 19] = [
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
            "explicit-null-seq",
            "snapshot-invalid",
            "effect-explicit-null-seq",
            explicit_null_seq,
        ),
        (
            "non-explicit-seq",
            "snapshot-invalid",
            "effect-non-explicit-seq",
            non_explicit_seq,
        ),
        (
            "automatic-await",
            "snapshot-invalid",
            "effect-automatic-await-shown",
            automatic_await,
        ),
        (
            "show-fallback",
            "snapshot-invalid",
            "visibility-show-fallback",
            show_fallback,
        ),
        (
            "intent-overbound",
            "snapshot-invalid",
            "intent-data-invalid",
            intent_overbound,
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
