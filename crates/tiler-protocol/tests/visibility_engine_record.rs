//! Durable engine provenance for the ten `v4-active` visibility adapter
//! replies: the four `visibility-order-failure` steps and the six
//! `visibility-cross-output-supersession` steps are genuine
//! [`WorldEngine`] products, not adapter-directed fixture inputs. Each
//! recorded reply below is produced by driving the real engine over the
//! scenario base report inputs through
//! [`WorldEngine::handle_report_v4_for_test`] (handshake plus settings
//! ingress, then the accounting report with the scripted command series),
//! then encoded with the genuine strict reply codec. The freshly produced
//! bytes must equal the committed scenario adapter messages exactly
//! (typed decode equality plus encode round-trip stability), with the
//! scripted focus/visibility oracles held on every recorded step.
//!
//! Reproduction route: this file. No outside producer, no absolute paths;
//! scenario inputs resolve manifest-relative under
//! `test-fixtures/world-wire/v4-active/scenarios/`.
//! Opt-in recording replaces only adapter reply messages with fresh engine
//! products; it never derives producer inputs from old reply bytes:
//! `UPDATE_VISIBILITY_V4_FIXTURES=1 cargo test -q -p tiler-core -p tiler-protocol -p omnitiler -- --skip workspace_unsafe_posture`.
//! Normal mode verifies the committed messages against the same producers.
//!
//! Offline only: no adapters, no live work.

#[path = "visibility_engine_record/producer.rs"]
mod producer;

use producer::*;
use tiler_protocol::world_protocol::{decode_reply_v4, encode_reply_v4};

#[test]
fn visibility_order_failure_replies_are_genuine_engine_products() {
    let fresh = produce_order_failure();
    let expected = expected_adapter_replies("visibility-order-failure", &fresh);
    assert_eq!(fresh.len(), 4, "order-failure recorded count");
    assert_reproduces(fresh, expected);
}

#[test]
fn visibility_cross_output_supersession_replies_are_genuine_engine_products() {
    let fresh = produce_cross_output();
    let expected = expected_adapter_replies("visibility-cross-output-supersession", &fresh);
    assert_eq!(fresh.len(), 6, "cross-output recorded count");
    assert_reproduces(fresh, expected);
}

#[test]
fn visibility_update_preserves_non_reply_bytes_and_settings_key_order() {
    let fresh_all = produce_order_failure();
    let fresh_two = &fresh_all[..2];
    let raw = r#"{
  "driver": "core-and-adapter",
  "id": "synthetic-preservation",
  "initial": {
    "adapter": "adapter-a",
    "freshEngine": true,
    "native": { "obs": "1", "foreground": "win-old" }
  },
  "steps": [
    {
      "id": "settings",
      "input": {
        "kind": "wire",
        "to": "core",
        "message": {
          "v": 4,
          "type": "settings",
          "values": {
            "gap": 8,
            "outer": 8,
            "defaultTiled": true,
            "borderEnabled": true
          }
        }
      },
      "expect": { "assertions": [".output.actions == []"], "status": "settled" }
    },
    {
      "id": "prepare-1",
      "input": {
        "kind": "fixture-event",
        "to": "adapter",
        "event": { "kind": "prepare-request", "message": { "req": "apply-1" } }
      }
    },
    {
      "id": "reply-1",
      "input": {
        "kind": "wire",
        "to": "adapter",
        "message": { "req": "apply-1", "stale": true }
      },
      "expect": { "assertions": [".output.actions != []"], "status": "settled" },
      "rules": ["normative-apply-rule"]
    },
    {
      "id": "reply-2",
      "input": {
        "kind": "wire",
        "to": "adapter",
        "message": { "req": "apply-2", "stale": true }
      },
      "expect": { "assertions": [".output.actions == []"], "status": "settled" }
    }
  ]
}
"#;

    let spans = adapter_reply_message_spans(raw);
    assert_eq!(spans.len(), 2, "only adapter wire replies selected");
    assert_eq!(spans[0].0, "apply-1");
    assert_eq!(spans[1].0, "apply-2");

    let updated = replace_adapter_reply_messages(raw, fresh_two);
    let updated_spans = adapter_reply_message_spans(&updated);
    assert_eq!(updated_spans.len(), 2);

    // Every byte outside the two adapter-directed wire message spans is identical.
    assert_eq!(&updated[..updated_spans[0].1], &raw[..spans[0].1]);
    assert_eq!(
        &updated[updated_spans[0].2..updated_spans[1].1],
        &raw[spans[0].2..spans[1].1]
    );
    assert_eq!(&updated[updated_spans[1].2..], &raw[spans[1].2..]);

    // The updated document is valid JSON and each replaced span encodes identically to fresh.
    let parsed: serde_json::Value = serde_json::from_str(&updated).expect("updated JSON parses");
    assert_eq!(parsed["steps"].as_array().unwrap().len(), 4);
    for (idx, (_, start, end)) in updated_spans.iter().enumerate() {
        let decoded = decode_reply_v4(&updated[*start..*end]).expect("replaced reply decodes");
        let reencoded = encode_reply_v4(&decoded).expect("replaced reply re-encodes");
        assert_eq!(reencoded, fresh_two[idx].1);
    }
}

#[test]
fn pretty_compact_json_preserves_utf8_and_escapes() {
    let compact = concat!(
        "{\"title\":\"caf\\\"",
        "\u{00e9}",
        "{[}]\",\"emptyObj\":{},\"emptyArr\":[],\"items\":[\"a\\\\b\",\"",
        "\u{2603}",
        "\"]}"
    );
    let pretty = pretty_compact_json_preserving_order(compact, 4);
    let parsed: serde_json::Value = serde_json::from_str(&pretty).expect("pretty JSON parses");
    assert_eq!(parsed["title"], "caf\"\u{00e9}{[}]");
    assert_eq!(parsed["emptyObj"], serde_json::json!({}));
    assert_eq!(parsed["emptyArr"], serde_json::json!([]));
    assert_eq!(parsed["items"], serde_json::json!(["a\\b", "\u{2603}"]));
    assert!(pretty.contains("\"emptyObj\": {}"));
    assert!(pretty.contains("\"emptyArr\": []"));
}
