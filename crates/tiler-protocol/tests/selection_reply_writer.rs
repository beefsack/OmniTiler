//! Selection reply v4 writer over an actual producer reply.
//!
//! Drives the minimal native-free suppression-capable explicit selection
//! route through the real engine (boot, send-aside, select), then asserts
//! the genuine v4 transport writer spells the producer reply with exact
//! size accounting (independent core byte count plus a decoder roundtrip
//! to the identical typed reply and wire metadata), and that the wire
//! carries the `awaitShown` grant plus the same-revision show/hide pair.
//!
//! The staged reports are genuine v4 envelopes: an immutable v4 fixture
//! (`report-focus-feedback-proven`, two valid windows plus the required
//! `rendererInput` lane) rebound to the test engine, with only identity,
//! sequence, command, focus, and settle lanes varied, decoded through the
//! genuine v4 ingress before reaching the engine.
//!
//! Offline only: no D-Bus, no KWin.

use tiler_core::connection::HostCapabilities;
use tiler_core::directional::OutputId;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ArrivalBracket, ArrivalLocation, FocusFeedback, LastWrite, LastWritePath, LastWriteProperty,
    ScopeAuthority, VisibilityFact, VisibilityReason, VisibilitySample, VisibilitySetter,
    WorldReplyV4, WorldReportV4,
};
use tiler_core::world_sizing_v4::{reply_estimate_v4, reply_exact_bytes_v4};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, decode_request, encode_reply_v4,
};

fn fixture_text(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/world-wire/v4/messages")
        .join(format!("{name}.json"));
    std::fs::read_to_string(&path).expect("fixture bytes")
}

/// Genuine v4 report: the immutable two-window fixture rebound to the
/// connected test engine, carrying no command and no settle lanes. Callers
/// clone and stage sequences, commands, focus, and settle evidence.
fn base_value() -> serde_json::Value {
    let mut base: serde_json::Value =
        serde_json::from_str(&fixture_text("report-focus-feedback-proven")).expect("fixture JSON");
    base["adapter"] = serde_json::json!("adapter-1");
    base["gen"] = serde_json::json!("gen-1");
    base["req"] = serde_json::json!("req-1");
    base["cmdSeq"] = serde_json::Value::Null;
    base["command"] = serde_json::Value::Null;
    base["focusFeedback"] = serde_json::json!([]);
    base
}

fn report(
    seq: u64,
    command_seq: Option<u64>,
    command: Option<serde_json::Value>,
    focused: &str,
) -> WorldReportV4 {
    let mut value = base_value();
    value["req"] = serde_json::json!(format!("req-{seq}"));
    value["reqSeq"] = serde_json::json!(seq.to_string());
    value["obs"] = serde_json::json!(seq.to_string());
    if let Some(cmd_seq) = command_seq {
        value["cmdSeq"] = serde_json::json!(cmd_seq.to_string());
    } else {
        value["cmdSeq"] = serde_json::Value::Null;
    }
    value["command"] = command.unwrap_or(serde_json::Value::Null);
    value["focused"] = serde_json::json!(focused);
    decode_request(&value.to_string()).expect("fixture envelope decodes")
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Encode one engine-published v4 reply with the canonical v4 writer; the
/// bytes must match the independent core byte count exactly and round-trip
/// through the reply decoder to the identical typed reply plus wire
/// metadata.
fn reply_wire(reply: &WorldReplyV4) -> String {
    let decoded = DecodedReplyV4 {
        reply: reply.clone(),
        wire: ReplyWireOptionals {
            suspended_output_present: reply.presentation.suspended_output.is_some(),
        },
    };
    let wire = encode_reply_v4(&decoded).expect("reply encodes");
    assert_eq!(
        wire.len(),
        reply_exact_bytes_v4(reply),
        "core byte count is exact over the wire"
    );
    assert_eq!(
        decode_reply_v4(&wire).expect("reply round-trips"),
        decoded,
        "wire round-trips to the identical typed reply and metadata"
    );
    wire
}

fn confirmed_write(target: bool, rev: u64, post_obs: u64) -> LastWrite {
    LastWrite {
        property: LastWriteProperty::Visibility,
        target,
        revision: rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(VisibilitySample {
            obs: post_obs,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(target),
                suppression_owned: Some(false),
            },
            minimized: Some(false),
            host_hidden: Some(false),
        }),
    }
}

fn settle_report(
    seq: u64,
    w1_last: LastWrite,
    w2_last: LastWrite,
    feedback: FocusFeedback,
) -> WorldReportV4 {
    let mut decoded = report(seq, None, None, "");
    for window in &mut decoded.windows {
        if window.window.0 == "win-a" {
            window.last_write = Some(w1_last);
        } else if window.window.0 == "win-b" {
            window.last_write = Some(w2_last);
        }
    }
    decoded.focus_feedback = vec![feedback];
    decoded
}

#[test]
fn selection_reply_spells_through_v4_writer() {
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            window_suppression: true,
            maximize: true,
            native_sticky: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let boot = accepted(&mut engine, &report(1, None, None, ""));
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let away = ring[1].clone();
    // Stage win-b on the away workspace without following, so the explicit
    // selection below actually changes the view and resolves win-b. The
    // transfer requires the mover to be the observed focus.
    let _ = accepted(
        &mut engine,
        &report(
            2,
            Some(2),
            Some(serde_json::json!({
                "op": "send",
                "win": "win-b",
                "out": "out-a",
                "ws": away,
                "follow": false,
            })),
            "win-b",
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            Some(3),
            Some(serde_json::json!({
                "op": "select-workspace",
                "out": "out-a",
                "ws": away,
            })),
            "",
        ),
    );
    assert_eq!(reply.effects.len(), 1, "one selection focus: {reply:?}");
    assert_eq!(reply.effects[0].target.0, "win-b");
    assert!(
        reply.effects[0].await_shown,
        "selection focus awaits arrival"
    );

    // The v4 writer spells the actual producer reply, with the independent
    // core byte count exact and a decoder roundtrip to the identical typed
    // reply, and the structural estimate covers it.
    let wire = reply_wire(&reply);
    assert!(
        reply_estimate_v4(&reply) >= wire.len(),
        "estimate covers the selection reply"
    );
    assert!(
        wire.contains("\"awaitShown\":true"),
        "wire carries the grant"
    );

    // The wire pair rides one authorizing revision with no fallback.
    let show = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .and_then(|row| row.visibility)
        .expect("selected home show");
    let hide = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .and_then(|row| row.visibility)
        .expect("departing home hide");
    assert!(show.shown && !hide.shown);
    assert!(!show.fallback && !hide.fallback);
    assert_eq!(show.revision, reply.state_revision);
    assert_eq!(hide.revision, show.revision);
    assert!(wire.contains(&format!("\"rev\":\"{}\"", show.revision)));
    for row in &reply.windows {
        assert_eq!(
            row.visibility_authority,
            ScopeAuthority::Controlled,
            "selection rows stay controlled"
        );
    }
}

#[test]
fn selection_reply_carries_switch_terminal_through_v4_writer() {
    // Real-producer parity through the actual terminal: the same
    // boot/send-aside/select route, then raw LastWrite plus arrival plus
    // focus feedback settle the parent op on the real engine.
    let mut engine = WorldEngine::new_test_bound_with_capabilities(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            window_suppression: true,
            maximize: true,
            native_sticky: true,
            ..Default::default()
        },
    )
    .expect("generation");
    let boot = accepted(&mut engine, &report(1, None, None, ""));
    let ring: Vec<String> = boot
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert!(ring.len() >= 2, "owned plus trailing: {ring:?}");
    let away = ring[1].clone();
    let _ = accepted(
        &mut engine,
        &report(
            2,
            Some(2),
            Some(serde_json::json!({
                "op": "send",
                "win": "win-b",
                "out": "out-a",
                "ws": away,
                "follow": false,
            })),
            "win-b",
        ),
    );
    let granted = accepted(
        &mut engine,
        &report(
            3,
            Some(3),
            Some(serde_json::json!({
                "op": "select-workspace",
                "out": "out-a",
                "ws": away,
            })),
            "",
        ),
    );
    let effect = granted.effects[0].clone();
    assert!(effect.await_shown, "selection focus awaits arrival");
    let rev = granted.state_revision;
    let frame = granted
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .and_then(|row| row.effective_frame);
    let sample = |obs: u64| VisibilitySample {
        obs,
        lifetime_matched: Some(true),
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: Some(false),
        },
        minimized: Some(false),
        host_hidden: Some(false),
    };
    let feedback = FocusFeedback {
        id: effect.id.clone(),
        target: effect.target.clone(),
        revision: effect.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(effect.target.clone()),
        foreground_proven: true,
        arrival: vec![ArrivalBracket {
            window: effect.target.clone(),
            obs: 4,
            before: sample(4),
            location: ArrivalLocation::Output {
                out: Some(OutputId("out-a".to_owned())),
            },
            frame,
            after: sample(4),
        }],
        attempt_ms: 7,
    };
    let settled = accepted(
        &mut engine,
        &settle_report(
            4,
            confirmed_write(false, rev, 4),
            confirmed_write(true, rev, 4),
            feedback,
        ),
    );
    assert_eq!(settled.switch_settled.len(), 1, "one terminal: {settled:?}");
    let event = &settled.switch_settled[0];
    assert_eq!(event.operation_id, "op-3");
    assert_eq!(
        event.outcome,
        tiler_core::world_report::SwitchOutcome::Complete
    );
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, tiler_core::world_report::SwitchFocus::Proven);
    assert!(!event.fallback);
    // The v4 writer spells the terminal reply with the exact canonical
    // event row in frozen key order; the independent core byte count is
    // exact and the wire round-trips to the identical typed reply.
    let wire = reply_wire(&settled);
    assert!(
        wire.contains(
            "\"switchSettled\":[{\"op\":\"op-3\",\"outcome\":\"complete\",\
             \"failed\":[],\"focus\":\"proven\",\"fallback\":false}]"
        ),
        "exact canonical event row in frozen key order"
    );
    // Marginal switch lane is exact against its wire bytes (the estimate
    // carries the 17-byte section framing while the section writer keeps
    // the 2 array brackets), and the whole-reply estimate bounds the wire.
    let mut section = String::new();
    tiler_core::world_sizing::encode_switch_settled(&mut section, &settled.switch_settled);
    let estimated = tiler_core::world_sizing::switch_settled_estimate(&settled.switch_settled);
    assert_eq!(
        estimated,
        section.len() + 17,
        "section framing accounts the gap"
    );
    assert!(
        reply_estimate_v4(&settled) >= wire.len(),
        "estimate covers the terminal reply"
    );
    // The terminal never repeats: the next report publishes empty.
    let again = accepted(&mut engine, &report(5, None, None, ""));
    assert!(
        again.switch_settled.is_empty(),
        "next report publishes empty"
    );
}
