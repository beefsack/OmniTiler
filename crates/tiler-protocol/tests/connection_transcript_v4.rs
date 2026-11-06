//! Public v4 connection transcript over one scenario: an in-memory ordered
//! message transcript through the existing handshake, settings, and report
//! boundaries. The caller selects the connection or the report boundary per
//! message; there is no line-delimited stdin and no unified ingress here.
//! World evaluation stdin is not exercised. The report leg drives the sole
//! public v4 ingress (`decode_request_with_wire` + `WorldEngine::handle_report`
//! and `encode_reply`) with genuine primary-codec verification
//! (`decode_reply_v4`); the internal test bridge is not used.

use tiler_core::connection::ProtocolKind;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_presentation::{GroupHideReason, PresentationUnderlay};
use tiler_core::world_report::WORLD_CONTRACT_VERSION;
use tiler_protocol::connection_protocol::{
    CodecError, decode_error, decode_handshake_reply, decode_handshake_request,
    decode_settings_reply, route_connection_request,
};
use tiler_protocol::world_protocol::{
    DecodedReplyV4, ReplyWireOptionals, decode_reply_v4, decode_request, decode_request_with_wire,
    encode_reply,
};

fn fixture_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/world-wire/v4")
}

fn scenario_message(scenario: &serde_json::Value, wanted: &str) -> String {
    let steps = scenario
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .expect("scenario steps array");
    let message = steps
        .iter()
        .find(|step| step.get("id").and_then(serde_json::Value::as_str) == Some(wanted))
        .unwrap_or_else(|| panic!("{wanted} step present"))
        .get("input")
        .expect("step input")
        .get("message")
        .expect("step message")
        .clone();
    serde_json::to_string(&message).expect("message serializes")
}

fn with_gen(line: &str, generation: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(line).expect("line parses");
    value.as_object_mut().expect("line object").insert(
        "gen".to_owned(),
        serde_json::Value::String(generation.to_owned()),
    );
    serde_json::to_string(&value).expect("line serializes")
}

/// Minimal private transcript driver holding the connection lifetime:
/// full engine drop on disconnect, next reserved seed on reconnect.
struct Driver {
    engine: Option<WorldEngine>,
    seeds: Vec<String>,
    next_seed: usize,
}

enum ReportOut {
    Accepted(String),
    Refused(String, String),
}

impl Driver {
    fn new(seeds: Vec<String>) -> Self {
        let engine = WorldEngine::new(&seeds[0]).expect("fresh engine seeds");
        Self {
            engine: Some(engine),
            seeds,
            next_seed: 1,
        }
    }

    fn disconnect(&mut self) {
        self.engine = None;
    }

    fn live(&mut self) -> &mut WorldEngine {
        if self.engine.is_none() {
            assert!(
                self.next_seed < self.seeds.len(),
                "no reserved generation for reconnect"
            );
            self.engine = Some(
                WorldEngine::new(&self.seeds[self.next_seed]).expect("reconnect engine seeds"),
            );
            self.next_seed += 1;
        }
        self.engine.as_mut().expect("live connection")
    }

    fn submit_connection(&mut self, line: &str) -> String {
        let live = self.live();
        route_connection_request(live, line).expect("connection family routes")
    }

    fn submit_report(&mut self, line: &str) -> ReportOut {
        let decoded = decode_request_with_wire(line).expect("report decodes");
        let live = self.live();
        match live.handle_report(&decoded.report) {
            WorldOutcome::Accepted(reply)
            | WorldOutcome::Duplicate(reply)
            | WorldOutcome::CommandConsumed(reply) => {
                let bytes = encode_reply(&DecodedReplyV4 {
                    wire: ReplyWireOptionals {
                        suspended_output_present: reply.presentation.suspended_output.is_some(),
                    },
                    reply,
                })
                .expect("reply encodes");
                decode_reply_v4(&bytes).expect("reply bytes verify");
                ReportOut::Accepted(bytes)
            }
            WorldOutcome::Rejected { kind, message } => {
                ReportOut::Refused(kind.to_owned(), message.to_owned())
            }
        }
    }

    fn ready(&self) -> bool {
        self.engine.as_ref().expect("live connection").is_ready()
    }
}

fn accepted(out: ReportOut) -> String {
    match out {
        ReportOut::Accepted(bytes) => bytes,
        ReportOut::Refused(kind, message) => panic!("unexpected refusal {kind}/{message}"),
    }
}

fn refused(out: ReportOut) -> (String, String) {
    match out {
        ReportOut::Accepted(_) => panic!("unexpected acceptance"),
        ReportOut::Refused(kind, message) => (kind, message),
    }
}

/// Exact bootstrap oracle: every [`tiler_core::world_report::WorldReplyV4`]
/// field for a pre-ready reply. Written from the observed contract, not
/// from the production bootstrap helper.
fn assert_bootstrap(decoded: &DecodedReplyV4, generation: &str, request: &str, obs: u64) {
    let reply = &decoded.reply;
    assert_eq!(reply.contract_version, WORLD_CONTRACT_VERSION);
    assert_eq!(reply.adapter_incarnation, "adapter-a");
    assert_eq!(reply.core_generation, generation);
    assert_eq!(reply.request_id, request);
    assert_eq!(reply.based_on_observation_seq, obs);
    assert_eq!(reply.state_revision, 0);
    assert!(!reply.ready);
    assert!(!reply.command_consumed);
    assert!(reply.scopes.is_empty());
    assert!(reply.workspaces.is_empty());
    assert!(reply.outputs.is_empty());
    assert!(reply.windows.is_empty());
    assert!(reply.transfers.is_empty());
    assert!(reply.effects.is_empty());
    assert_eq!(reply.command_result, None);
    assert_eq!(reply.gesture_settled, None);
    assert!(reply.operation_settled.is_empty());
    assert!(reply.native_creations.is_empty());
    assert_eq!(reply.cursor_warp, None);
    assert!(reply.intent_writes.is_empty());
    assert!(reply.switch_settled.is_empty());
    assert!(reply.diagnostics.is_empty());
    assert_eq!(reply.presentation.foreground, "");
    assert_eq!(reply.presentation.rev, 0);
    assert_eq!(reply.presentation.suspended_output, None);
    assert!(!reply.presentation.border.visible);
    assert_eq!(reply.presentation.border.reason, None);
    assert_eq!(reply.presentation.border.subject, None);
    assert_eq!(reply.presentation.border.outsets, None);
    assert_eq!(reply.presentation.border.width, None);
    assert_eq!(reply.presentation.border.radius, None);
    assert_eq!(reply.presentation.border.witness, None);
    assert!(matches!(
        reply.presentation.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::NoGroup,
        }
    ));
    assert!(reply.presentation.pager.is_empty());
    assert!(!decoded.wire.suspended_output_present);
}

#[test]
fn connection_transcript_v4() {
    let text =
        std::fs::read_to_string(fixture_root().join("scenarios/raw-attempt-settlement.json"))
            .expect("scenario reads");
    let scenario: serde_json::Value = serde_json::from_str(&text).expect("scenario parses");
    let generations: Vec<String> = scenario
        .get("initial")
        .expect("scenario initial")
        .get("generationSequence")
        .and_then(serde_json::Value::as_array)
        .expect("generation sequence")
        .iter()
        .take(2)
        .map(|seed| seed.as_str().expect("seed string").to_owned())
        .collect();
    assert_eq!(generations, vec!["generation-a", "generation-b"]);

    let handshake_line = scenario_message(&scenario, "handshake");
    let settings_line = scenario_message(&scenario, "settings");
    let observe_line = scenario_message(&scenario, "observe");
    let place_a_line = scenario_message(&scenario, "place-a");
    let place_b_line = scenario_message(&scenario, "place-b");

    let mut driver = Driver::new(generations);

    // Invalid protocol/version samples refuse through the existing router.
    let mut bad_version: serde_json::Value =
        serde_json::from_str(&handshake_line).expect("handshake parses");
    bad_version
        .as_object_mut()
        .expect("handshake object")
        .insert("v".to_owned(), serde_json::json!(3));
    let bad_version_line = serde_json::to_string(&bad_version).expect("handshake serializes");
    assert_eq!(
        route_connection_request(driver.live(), &bad_version_line),
        Err(CodecError("version"))
    );
    assert_eq!(
        route_connection_request(driver.live(), "{\"type\":\"nope\",\"v\":4}"),
        Err(CodecError("message-type"))
    );

    // Initial handshake pins exact correlation, capabilities, ready=false.
    let request = decode_handshake_request(&handshake_line).expect("handshake decodes");
    assert_eq!(request.adapter, "adapter-a");
    assert_eq!(request.request, "handshake-a");
    assert!(!request.capabilities.host_owned);
    assert!(!request.capabilities.equal_count);
    assert!(request.capabilities.birth_clear);
    assert!(request.capabilities.window_suppression);
    assert!(request.capabilities.maximize);
    assert!(request.capabilities.fullscreen);
    assert!(request.capabilities.native_sticky);
    assert!(request.capabilities.above);
    assert!(request.capabilities.warp);
    assert!(request.capabilities.tiled_float_transfer);
    assert!(request.capabilities.unavailable_facts.is_empty());
    assert_eq!(request.application_identity.app_id, None);
    assert_eq!(
        request.application_identity.executable.as_deref(),
        Some("c:\\synthetic\\omnitiler.exe")
    );
    assert_eq!(
        request.application_identity.process_id.as_deref(),
        Some("pid-own-1")
    );
    let reply_bytes = driver.submit_connection(&handshake_line);
    let reply = decode_handshake_reply(&reply_bytes).expect("handshake reply decodes");
    assert_eq!(reply.adapter, "adapter-a");
    assert_eq!(reply.request, "handshake-a");
    assert_eq!(reply.generation.as_deref(), Some("generation-a"));
    assert!(!reply.ready);
    assert_eq!(reply.kind, None);

    // Report before settings latches nothing: exact empty bootstrap.
    let bytes = accepted(driver.submit_report(&observe_line));
    let decoded = decode_reply_v4(&bytes).expect("bootstrap reply decodes");
    assert_bootstrap(&decoded, "generation-a", "request-1", 1);
    assert!(!driver.ready());

    // Initial settings admit with full reply correlation.
    let settings_bytes = driver.submit_connection(&settings_line);
    let settings_reply = decode_settings_reply(&settings_bytes).expect("settings reply decodes");
    assert_eq!(settings_reply.adapter, "adapter-a");
    assert_eq!(settings_reply.generation, "generation-a");
    assert_eq!(settings_reply.request, "settings-1");
    assert_eq!(settings_reply.revision, 1);
    assert!(matches!(
        settings_reply.status,
        tiler_core::connection::SettingsStatus::Accepted
    ));
    assert!(!driver.ready());

    // Complete usable report latches ready.
    let ready_bytes = accepted(driver.submit_report(&observe_line));
    let ready = decode_reply_v4(&ready_bytes).expect("ready reply decodes");
    assert!(ready.reply.ready);
    assert_eq!(ready.reply.request_id, "request-1");
    assert_eq!(ready.reply.based_on_observation_seq, 1);
    assert!(driver.ready());

    // Exact report replay returns the identical encoded reply.
    let replay_bytes = accepted(driver.submit_report(&observe_line));
    assert_eq!(replay_bytes, ready_bytes);

    // Changed body under a reused sequence refuses without corruption.
    let mut changed: serde_json::Value =
        serde_json::from_str(&observe_line).expect("report parses");
    changed
        .as_object_mut()
        .expect("report object")
        .insert("cursor".to_owned(), serde_json::json!([21, 30]));
    let changed_line = serde_json::to_string(&changed).expect("report serializes");
    assert!(decode_request(&changed_line).is_ok());
    let (kind, _) = refused(driver.submit_report(&changed_line));
    assert_eq!(kind, "request-reused");

    // Older sequence than the accepted watermark refuses; the next
    // ordered report (command-carrying place-a) still admits.
    let stale_bytes = accepted(driver.submit_report(&place_a_line));
    let stale_reply = decode_reply_v4(&stale_bytes).expect("place-a reply decodes");
    assert_eq!(stale_reply.reply.based_on_observation_seq, 2);
    let (kind, _) = refused(driver.submit_report(&observe_line));
    assert_eq!(kind, "request-superseded");
    let next_bytes = accepted(driver.submit_report(&place_b_line));
    let next = decode_reply_v4(&next_bytes).expect("place-b reply decodes");
    assert!(next.reply.ready);

    // Older observation than the accepted watermark refuses; the ordered
    // report still admits afterwards.
    let mut stale_obs: serde_json::Value =
        serde_json::from_str(&place_b_line).expect("report parses");
    let obs_object = stale_obs.as_object_mut().expect("report object");
    obs_object.insert("reqSeq".to_owned(), serde_json::json!("9"));
    obs_object.insert("req".to_owned(), serde_json::json!("request-9"));
    obs_object.insert("obs".to_owned(), serde_json::json!("1"));
    let stale_obs_line = serde_json::to_string(&stale_obs).expect("report serializes");
    assert!(decode_request(&stale_obs_line).is_ok());
    let (kind, _) = refused(driver.submit_report(&stale_obs_line));
    assert_eq!(kind, "observation-stale");
    let _ = accepted(driver.submit_report(&place_b_line));

    // Reconnect rebinds the generation; old envelopes refuse.
    driver.disconnect();
    let reply_bytes = driver.submit_connection(&handshake_line);
    let reply = decode_handshake_reply(&reply_bytes).expect("handshake reply decodes");
    assert_eq!(reply.generation.as_deref(), Some("generation-b"));
    assert!(!reply.ready);

    let old_settings_bytes = driver.submit_connection(&settings_line);
    let old_settings_error = decode_error(&old_settings_bytes).expect("error wire decodes");
    assert_eq!(old_settings_error.kind, ProtocolKind::GenerationMismatch);
    assert_eq!(old_settings_error.generation, "generation-b");

    let (kind, _) = refused(driver.submit_report(&observe_line));
    assert_eq!(kind, "generation-mismatch");

    // New lifetime bootstraps empty until settings plus a usable report.
    let new_observe = with_gen(&observe_line, "generation-b");
    let new_settings = with_gen(&settings_line, "generation-b");
    let bytes = accepted(driver.submit_report(&new_observe));
    let decoded = decode_reply_v4(&bytes).expect("bootstrap reply decodes");
    assert_bootstrap(&decoded, "generation-b", "request-1", 1);
    assert!(!driver.ready());

    let settings_bytes = driver.submit_connection(&new_settings);
    let settings_reply = decode_settings_reply(&settings_bytes).expect("settings reply decodes");
    assert_eq!(settings_reply.generation, "generation-b");
    assert!(matches!(
        settings_reply.status,
        tiler_core::connection::SettingsStatus::Accepted
    ));
    assert!(!driver.ready());

    let bytes = accepted(driver.submit_report(&new_observe));
    let decoded = decode_reply_v4(&bytes).expect("ready reply decodes");
    assert!(decoded.reply.ready);
    assert_eq!(decoded.reply.core_generation, "generation-b");
    assert!(driver.ready());
}
