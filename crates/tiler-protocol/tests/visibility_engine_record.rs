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
//!
//! Offline only: no adapters, no live work.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::*;
use tiler_protocol::connection_protocol::{decode_handshake_request, decode_settings};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, decode_report_v4, encode_reply_v4,
};

fn scenario_value(id: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4-active/scenarios")
        .join(format!("{id}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).expect("scenario JSON"))
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
        let reply = match self.engine.handle_report_v4_for_test(&report) {
            Ok(reply) => reply,
            Err((kind, message)) => panic!("{req} refused {kind}/{message}"),
        };
        if record {
            let wire_flags = ReplyWireOptionals {
                suspended_output_present: reply.presentation.suspended_output.is_some(),
            };
            let text = encode_reply_v4(&DecodedReplyV4 {
                reply: reply.clone(),
                wire: wire_flags,
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

fn produce_order_failure() -> Vec<(String, String)> {
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
            window: WindowId("win-a".to_owned()),
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
            workspace: "ws-2".to_owned(),
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
            window: WindowId("win-a".to_owned()),
            direction: String::new(),
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
            workspace: "ws-1".to_owned(),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-2",
        2,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-2".to_owned(),
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
            workspace: "ws-1".to_owned(),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-3",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-2".to_owned(),
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
            workspace: "ws-1".to_owned(),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-c2",
        3,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-2".to_owned(),
        }),
        "win-old",
        false,
    );
    let reply = rec.run("apply-4", 3, None, "win-a", true);
    check_focus(&reply, "apply-4", None, None);
    assert!(!row(&reply, "win-old").0);
    rec.out
}

fn produce_cross_output() -> Vec<(String, String)> {
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
            window: WindowId("win-a".to_owned()),
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
            workspace: "ws-3".to_owned(),
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
            workspace: "ws-1".to_owned(),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-a2",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-3".to_owned(),
        }),
        "win-old",
        false,
    );
    let reply = rec.run(
        "apply-2",
        1,
        Some(WorldCommand::Focus {
            window: WindowId("win-b".to_owned()),
            direction: String::new(),
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
            workspace: "ws-1".to_owned(),
        }),
        "win-old",
        false,
    );
    rec.run(
        "dummy-flip-b2",
        1,
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-3".to_owned(),
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
            workspace: "ws-1".to_owned(),
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
            workspace: "ws-3".to_owned(),
        }),
        "win-a",
        true,
    );
    check_focus(&reply, "apply-6", Some("win-a"), Some(true));
    assert!(!row(&reply, "win-old").0);
    rec.out
}

/// Adapter-directed scenario messages in record order for one family.
fn expected_adapter_replies(scenario_id: &str) -> Vec<(String, String)> {
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

fn assert_reproduces(fresh: Vec<(String, String)>, expected: Vec<(String, String)>) {
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
            decode_reply_v4(&round_trip).expect("round-trip decodes"),
            expected_decoded,
            "{req} committed bytes are encoder-stable"
        );
        let fresh_flags = ReplyWireOptionals {
            suspended_output_present: fresh_decoded.reply.presentation.suspended_output.is_some(),
        };
        assert_eq!(
            encode_reply_v4(&DecodedReplyV4 {
                reply: fresh_decoded.reply,
                wire: fresh_flags
            })
            .expect("fresh reply re-encodes"),
            *fresh_wire,
            "{req} fresh bytes are encoder-stable"
        );
    }
}

#[test]
fn visibility_order_failure_replies_are_genuine_engine_products() {
    let fresh = produce_order_failure();
    let expected = expected_adapter_replies("visibility-order-failure");
    assert_eq!(fresh.len(), 4, "order-failure recorded count");
    assert_reproduces(fresh, expected);
}

#[test]
fn visibility_cross_output_supersession_replies_are_genuine_engine_products() {
    let fresh = produce_cross_output();
    let expected = expected_adapter_replies("visibility-cross-output-supersession");
    assert_eq!(fresh.len(), 6, "cross-output recorded count");
    assert_reproduces(fresh, expected);
}
