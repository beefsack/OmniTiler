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

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::*;
use tiler_protocol::connection_protocol::{decode_handshake_request, decode_settings};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, decode_reply_v4, decode_report_v4, encode_reply_v4,
};

fn scenario_path(id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active/scenarios")
        .join(format!("{id}.json"))
}

fn scenario_value(id: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(scenario_path(id)).expect("scenario JSON"))
        .expect("scenario JSON")
}

fn msg_of(scenario: &serde_json::Value, id: &str) -> serde_json::Value {
    scenario["steps"]
        .as_array()
        .expect("scenario steps")
        .iter()
        .find(|s| s["id"] == id)
        .expect("scenario step")["input"]["message"]
        .clone()
}

struct Rec {
    engine: WorldEngine,
    base: WorldReportV4,
    seq: u64,
    cseq: u64,
    out: Vec<(String, String)>,
}

impl Rec {
    fn run(
        &mut self,
        req: &str,
        obs: u64,
        cmd: Option<WorldCommand>,
        focused: &str,
        record: bool,
    ) -> WorldReplyV4 {
        self.seq += 1;
        let cseq = cmd.as_ref().map(|_| {
            self.cseq += 1;
            self.cseq
        });
        let mut report = self.base.clone();
        report.request_id = req.to_owned();
        report.request_seq = self.seq;
        report.observation_seq = obs;
        report.command_seq = cseq;
        report.command = cmd;
        report.focused_global = focused.to_owned();
        let gate = tiler_protocol::world_admission::ProtocolAdmissionGate::new();
        let reply = match self
            .engine
            .handle_report_with_gate_and_prepare(&report, &[], &gate, |_| {
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap_or_else(|e| match e {})
            .outcome
        {
            tiler_core::world_engine::WorldOutcome::Accepted(reply)
            | tiler_core::world_engine::WorldOutcome::Duplicate(reply)
            | tiler_core::world_engine::WorldOutcome::CommandConsumed(reply) => reply,
            tiler_core::world_engine::WorldOutcome::Rejected { kind, message } => {
                panic!("{req} refused {kind}/{message}")
            }
        };
        if record {
            let text = encode_reply_v4(&DecodedReplyV4 {
                roster_plan: None,
                reply: reply.clone(),
            })
            .unwrap_or_else(|error| panic!("{req} encodes: {error:?}"));
            self.out.push((req.to_owned(), text));
        }
        reply
    }
}

fn check_focus(reply: &WorldReplyV4, req: &str, target: Option<&str>, after_shown: Option<bool>) {
    match (&reply.focus_request, target) {
        (None, None) => {}
        (Some(focus), Some(expected)) => {
            assert_eq!(focus.target.0, expected, "{req} focus target");
            assert_eq!(
                focus.cause,
                FocusRequestCause::ExplicitCommand,
                "{req} cause"
            );
            assert!(focus.cmd_seq.is_some(), "{req} cmdSeq");
            if let Some(after) = after_shown {
                assert_eq!(focus.after_shown.is_some(), after, "{req} afterShown");
            }
        }
        (None, Some(expected)) => panic!("{req} expected focus {expected}, got null"),
        (Some(focus), None) => panic!("{req} expected null, got {}", focus.id),
    }
}

fn row(reply: &WorldReplyV4, win: &str) -> (bool, u64, bool, String, String) {
    let window = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == win)
        .unwrap_or_else(|| panic!("row {win} present"));
    let target = window
        .visibility
        .as_ref()
        .unwrap_or_else(|| panic!("row {win} has target"));
    (
        target.shown,
        target.revision,
        target.fallback,
        window.output.0.clone(),
        window.workspace.clone(),
    )
}

pub fn produce_order_failure() -> Vec<(String, String)> {
    let scenario = scenario_value("visibility-order-failure");
    let mut engine = WorldEngine::new("generation-a").expect("engine");
    let handshake = msg_of(&scenario, "handshake");
    engine.handshake(
        &decode_handshake_request(
            &serde_json::to_string(&handshake).expect("handshake serializes"),
        )
        .expect("handshake decodes"),
    );
    engine
        .handle_settings(
            &decode_settings(
                &serde_json::to_string(&msg_of(&scenario, "settings"))
                    .expect("settings serializes"),
            )
            .expect("settings decodes"),
        )
        .expect("settings");
    let base_wire =
        serde_json::to_string(&msg_of(&scenario, "accounting-admit")).expect("report serializes");
    let base = decode_report_v4(&base_wire).expect("report decodes").report;
    let mut rec = Rec {
        engine,
        base,
        seq: 0,
        cseq: 0,
        out: Vec::new(),
    };

    // Setup mirrors the live accounting flow through selection of ws-2.
    rec.run("dummy-setup-1", 1, None, "win-old", false);
    rec.run(
        "dummy-setup-2",
        1,
        Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            source: SendSource::HomeThenAction,
            index: 2,
            follow: false,
        }),
        "win-a",
        false,
    );
    rec.run("dummy-setup-3", 1, None, "win-old", false);
    let reply = rec.run(
        "dummy-setup-4",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-2".to_owned()),
        }),
        "win-a",
        false,
    );
    check_focus(&reply, "dummy-setup-4", Some("win-a"), Some(true));
    assert_eq!(
        row(&reply, "win-a"),
        (true, 4, false, "out-a".into(), "ws-2".into())
    );

    // apply-1 (obs 1): bare focus mint, rows pinned at rev 4.
    let reply = rec.run(
        "apply-1",
        1,
        Some(WorldCommand::Focus {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            direction: None,
        }),
        "win-old",
        true,
    );
    check_focus(&reply, "apply-1", Some("win-a"), Some(false));
    assert!(row(&reply, "win-a").0);
    assert!(!row(&reply, "win-old").0);

    // Flip-flop to freshen rows, then apply-2 (obs 2): selection-backed focus.
    rec.run(
        "dummy-flip-a1",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-2",
        2,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-2".to_owned()),
        }),
        "win-old",
        true,
    );
    check_focus(&reply, "apply-2", Some("win-a"), Some(true));

    // Flip-flop, then apply-3 (obs 3): selection-backed focus (refused host-side).
    rec.run(
        "dummy-flip-b1",
        2,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-3",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-2".to_owned()),
        }),
        "win-old",
        true,
    );
    check_focus(&reply, "apply-3", Some("win-a"), Some(true));

    // Flip-flop, retire via observed focus, then apply-4 (obs 3): no focus.
    rec.run(
        "dummy-flip-c1",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-c2",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-2".to_owned()),
        }),
        "win-old",
        false,
    );
    let reply = rec.run("apply-4", 3, None, "win-a", true);
    check_focus(&reply, "apply-4", None, None);
    assert!(!row(&reply, "win-old").0);
    rec.out
}

pub fn produce_cross_output() -> Vec<(String, String)> {
    let scenario = scenario_value("visibility-order-failure");
    let mut engine = WorldEngine::new("generation-a").expect("engine");
    let handshake = msg_of(&scenario, "handshake");
    engine.handshake(
        &decode_handshake_request(
            &serde_json::to_string(&handshake).expect("handshake serializes"),
        )
        .expect("handshake decodes"),
    );
    // Cross-output settings come from its own scenario.
    let cross = scenario_value("visibility-cross-output-supersession");
    engine
        .handle_settings(
            &decode_settings(
                &serde_json::to_string(&msg_of(&cross, "settings")).expect("settings serializes"),
            )
            .expect("settings decodes"),
        )
        .expect("settings");
    let base_wire =
        serde_json::to_string(&msg_of(&scenario, "accounting-admit")).expect("report serializes");
    let mut base = decode_report_v4(&base_wire).expect("report decodes").report;
    let mut screen_b = base.screens[0].clone();
    screen_b.output = OutputId("out-b".to_owned());
    base.screens.push(screen_b);
    let mut win_b = base.windows[0].clone();
    win_b.window = WindowId("win-b".to_owned());
    win_b.output = Some(OutputId("out-b".to_owned()));
    base.windows.push(win_b);
    let mut rec = Rec {
        engine,
        base,
        seq: 0,
        cseq: 0,
        out: Vec::new(),
    };

    rec.run("dummy-setup-1", 1, None, "win-old", false);
    rec.run(
        "dummy-setup-2",
        1,
        Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            source: SendSource::HomeThenAction,
            index: 2,
            follow: false,
        }),
        "win-a",
        false,
    );
    // Discover out-a workspace holding win-a from the send reply is skipped;
    // the ring is deterministic: probe showed ws-3. Verify via select below.
    let reply = rec.run(
        "apply-1",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-3".to_owned()),
        }),
        "win-a",
        true,
    );
    check_focus(&reply, "apply-1", Some("win-a"), Some(true));
    assert!(row(&reply, "win-a").0);
    assert!(!row(&reply, "win-old").0);

    // Flip-flop out-a, then bare focus win-b for supersession.
    rec.run(
        "dummy-flip-a1",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-a2",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-3".to_owned()),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-2",
        1,
        Some(WorldCommand::Focus {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
            direction: None,
        }),
        "win-old",
        true,
    );
    check_focus(&reply, "apply-2", Some("win-b"), Some(false));

    // Flip-flop, retire via observed win-b focus, then no-focus steps.
    rec.run(
        "dummy-flip-b1",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-b2",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-3".to_owned()),
        }),
        "win-old",
        false,
    );
    let reply = rec.run("apply-3", 3, None, "win-b", true);
    check_focus(&reply, "apply-3", None, None);
    let reply = rec.run("apply-4", 3, None, "win-b", true);
    check_focus(&reply, "apply-4", None, None);

    // Flip view to ws-1: win-old shown, win-a hidden (accepted host-side).
    let reply = rec.run(
        "apply-5",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-1".to_owned()),
        }),
        "win-a",
        true,
    );
    check_focus(&reply, "apply-5", Some("win-old"), Some(true));
    assert!(row(&reply, "win-old").0);

    // Flip back: win-a shown again, win-old hidden with a fresh revision.
    let reply = rec.run(
        "apply-6",
        4,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: tiler_core::directional::WorkspaceId("ws-3".to_owned()),
        }),
        "win-a",
        true,
    );
    check_focus(&reply, "apply-6", Some("win-a"), Some(true));
    assert!(!row(&reply, "win-old").0);
    rec.out
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

fn scan_string_end(bytes: &[u8], start: usize) -> usize {
    assert_eq!(bytes[start], b'"', "expected JSON string");
    let mut i = start + 1;
    let mut escaped = false;
    while i < bytes.len() {
        if escaped {
            escaped = false;
        } else if bytes[i] == b'\\' {
            escaped = true;
        } else if bytes[i] == b'"' {
            return i + 1;
        }
        i += 1;
    }
    panic!("unterminated JSON string");
}

fn scan_value_end(bytes: &[u8], start: usize) -> usize {
    match bytes[start] {
        b'"' => scan_string_end(bytes, start),
        b'{' | b'[' => {
            let mut i = start;
            let mut depth = 0usize;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = scan_string_end(bytes, i);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            panic!("unterminated JSON container");
        }
        _ => {
            let mut i = start;
            while i < bytes.len()
                && !matches!(bytes[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            {
                i += 1;
            }
            i
        }
    }
}

fn find_object_field_value_span(
    bytes: &[u8],
    obj_start: usize,
    obj_end: usize,
    key: &str,
) -> Option<(usize, usize)> {
    assert_eq!(bytes[obj_start], b'{', "expected JSON object");
    let mut i = skip_ws(bytes, obj_start + 1);
    while i < obj_end {
        if bytes[i] == b'}' {
            return None;
        }
        if bytes[i] == b',' {
            i = skip_ws(bytes, i + 1);
            continue;
        }
        let k_end = scan_string_end(bytes, i);
        let k = &bytes[i + 1..k_end - 1];
        i = skip_ws(bytes, k_end);
        assert_eq!(bytes[i], b':', "expected ':' after object key");
        let val_start = skip_ws(bytes, i + 1);
        let val_end = scan_value_end(bytes, val_start);
        if k == key.as_bytes() {
            return Some((val_start, val_end));
        }
        i = skip_ws(bytes, val_end);
    }
    None
}

fn array_element_spans(bytes: &[u8], arr_start: usize, arr_end: usize) -> Vec<(usize, usize)> {
    assert_eq!(bytes[arr_start], b'[', "expected JSON array");
    let mut out = Vec::new();
    let mut i = skip_ws(bytes, arr_start + 1);
    while i < arr_end {
        if bytes[i] == b']' {
            break;
        }
        if bytes[i] == b',' {
            i = skip_ws(bytes, i + 1);
            continue;
        }
        let elem_end = scan_value_end(bytes, i);
        out.push((i, elem_end));
        i = skip_ws(bytes, elem_end);
    }
    out
}

/// Pretty-print compact JSON from `encode_reply_v4` with 2-space indents at
/// `base_indent`, preserving the exact key order emitted by the encoder.
pub fn pretty_compact_json_preserving_order(compact: &str, base_indent: usize) -> String {
    let mut chars = compact.chars().peekable();
    let mut out = String::with_capacity(compact.len() * 2);
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    let push_indent = |buf: &mut String, d: usize| {
        for _ in 0..(base_indent + d * 2) {
            buf.push(' ');
        }
    };

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push('"');
            }
            '{' => {
                out.push('{');
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
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
            '[' => {
                out.push('[');
                if chars.peek() == Some(&']') {
                    chars.next();
                    out.push(']');
                    continue;
                }
                depth += 1;
                out.push('\n');
                push_indent(&mut out, depth);
            }
            ']' => {
                depth -= 1;
                out.push('\n');
                push_indent(&mut out, depth);
                out.push(']');
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
    }
    out
}

pub fn adapter_reply_message_spans(raw: &str) -> Vec<(String, usize, usize)> {
    let bytes = raw.as_bytes();
    let root_start = skip_ws(bytes, 0);
    let root_end = scan_value_end(bytes, root_start);
    let (steps_start, steps_end) =
        find_object_field_value_span(bytes, root_start, root_end, "steps").expect("scenario steps");
    let mut spans = Vec::new();
    for (step_start, step_end) in array_element_spans(bytes, steps_start, steps_end) {
        let step: serde_json::Value =
            serde_json::from_slice(&bytes[step_start..step_end]).expect("step JSON");
        if step["input"]["to"] != "adapter" || step["input"]["kind"] != "wire" {
            continue;
        }
        let req = step["input"]["message"]["req"]
            .as_str()
            .expect("reply req")
            .to_owned();
        let (input_start, input_end) =
            find_object_field_value_span(bytes, step_start, step_end, "input").expect("step input");
        let (msg_start, msg_end) =
            find_object_field_value_span(bytes, input_start, input_end, "message")
                .expect("step input message");
        spans.push((req, msg_start, msg_end));
    }
    spans
}

fn line_indent_at(raw: &str, offset: usize) -> usize {
    raw[..offset].rfind('\n').map_or(0, |nl| {
        raw[nl + 1..offset]
            .bytes()
            .take_while(|b| *b == b' ')
            .count()
    })
}

pub fn replace_adapter_reply_messages(raw: &str, fresh: &[(String, String)]) -> String {
    let spans = adapter_reply_message_spans(raw);
    assert_eq!(spans.len(), fresh.len(), "recorded reply count");
    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    for ((req, msg_start, msg_end), (fresh_req, wire)) in spans.iter().zip(fresh.iter()) {
        assert_eq!(req, fresh_req, "record order");
        out.push_str(&raw[cursor..*msg_start]);
        let indent = line_indent_at(raw, *msg_start);
        out.push_str(&pretty_compact_json_preserving_order(wire, indent));
        cursor = *msg_end;
    }
    out.push_str(&raw[cursor..]);
    out
}

/// Adapter-directed scenario messages in record order for one family.
pub fn expected_adapter_replies(
    scenario_id: &str,
    fresh: &[(String, String)],
) -> Vec<(String, String)> {
    let path = scenario_path(scenario_id);
    if std::env::var("UPDATE_VISIBILITY_V4_FIXTURES").as_deref() == Ok("1") {
        let raw = std::fs::read_to_string(&path).expect("scenario JSON");
        let updated = replace_adapter_reply_messages(&raw, fresh);
        std::fs::write(&path, updated).expect("scenario records");
    }
    let scenario = scenario_value(scenario_id);
    scenario["steps"]
        .as_array()
        .expect("scenario steps")
        .iter()
        .filter(|step| step["input"]["to"] == "adapter" && step["input"]["kind"] == "wire")
        .map(|step| {
            let message = step["input"]["message"].clone();
            let wire = serde_json::to_string(&message).expect("message serializes");
            (message["req"].as_str().expect("reply req").to_owned(), wire)
        })
        .collect()
}

pub fn assert_reproduces(fresh: Vec<(String, String)>, expected: Vec<(String, String)>) {
    assert_eq!(fresh.len(), expected.len(), "recorded reply count");
    for ((req, fresh_wire), (expected_req, expected_wire)) in fresh.iter().zip(expected.iter()) {
        assert_eq!(req, expected_req, "record order");
        let fresh_decoded = decode_reply_v4(fresh_wire).expect("fresh reply decodes");
        let expected_decoded = decode_reply_v4(expected_wire).expect("expected reply decodes");
        assert_eq!(
            fresh_decoded.reply, expected_decoded.reply,
            "{req} engine reply equals the committed scenario reply"
        );
        // The committed bytes are the genuine encoder output: re-encoding
        // the decoded expected value is byte-stable, and the fresh bytes
        // carry the same wire shape.
        let round_trip = encode_reply_v4(&expected_decoded).expect("expected reply re-encodes");
        assert_eq!(
            round_trip, *fresh_wire,
            "{req} committed message has the fresh encoder bytes"
        );
        assert_eq!(
            decode_reply_v4(&round_trip).expect("round-trip decodes"),
            expected_decoded,
            "{req} committed bytes are encoder-stable"
        );
        assert_eq!(
            encode_reply_v4(&DecodedReplyV4 {
                roster_plan: None,
                reply: fresh_decoded.reply,
            })
            .expect("fresh reply re-encodes"),
            *fresh_wire,
            "{req} fresh bytes are encoder-stable"
        );
    }
}

// Used by raw_attempt_settlement_v4; unused in the standalone visibility_engine_record test binary.
#[allow(dead_code)]
pub fn record_all_if_enabled() {
    if std::env::var("UPDATE_VISIBILITY_V4_FIXTURES").as_deref() == Ok("1") {
        let _ = expected_adapter_replies("visibility-order-failure", &produce_order_failure());
        let _ = expected_adapter_replies(
            "visibility-cross-output-supersession",
            &produce_cross_output(),
        );
    }
}
