//! Broker-level world flow: sole-broker capture/discovery, generation and
//! capture-lifetime binding, watermark monotonicity, source-nonce replay and
//! reuse, sender-bound command namespaces, terminal success/failure
//! settlement, presentation projection without one-shot lanes or write
//! tokens, foreign-epoch clock gating, and overbound fail-closed behavior.
//! Offline only: no D-Bus, no KWin.
//!
//! Boundary values are realistic: UUID native desktop ids, opaque window
//! and output ids, D-Bus unique-name senders, canonical decimal counters.

use omnitiler::world_broker::{MAX_REMEMBERED_SOURCES, WorldBroker, parse_wait_request};
use omnitiler::world_service::{WORLD_SERVICE_MAX_REPLY, WorldService, WorldServiceError};

const UUID_A: &str = "550e8400-e29b-41d4-a716-446655440001";
const UUID_B: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const SENDER_A: &str = ":1.42";
const SENDER_B: &str = ":1.77";
const ADAPTER: &str = "kwin-actuator-1";

fn disabled_presentation() -> String {
    "{\"foreground\":\"\",\"minimized\":false,\"maximized\":false,\
     \"fullscreen\":false,\"hostHidden\":false,\"hiddenWorkspace\":false,\
     \"frame\":null,\"chordHeld\":false,\"moveWindow\":\"\",\
     \"pagerOrder\":[],\"pagerFrames\":[],\"witness\":null,\
     \"foregroundOut\":null}"
        .to_owned()
}

fn window_json(id: &str, rect: &str, native_ws: Option<&str>) -> String {
    let native_lane = native_ws.map_or(String::new(), |native| {
        format!(",\"nativeWs\":\"{native}\"")
    });
    let frame = rect
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    assert_eq!(frame.len(), 4, "raw test rects are [x,y,w,h]");
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\
          \"rect\":[{x},{y},{w},{h}],\
          \"fullscreen\":false,\"maximized\":false,\"sticky\":false,\
          \"minimized\":false,\
          \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}}{native_lane},\
          \"hostHidden\":false,\"privileged\":false,\"managed\":true,\
          \"acceptsInput\":true,\"activationInhibited\":false,\
          \"toolWindow\":false,\"owned\":false,\"transient\":false,\
          \"modal\":false,\"resizable\":true,\"above\":false,\"below\":false,\
          \"owner\":null,\"parent\":null,\"role\":null,\"appId\":null,\
          \"executable\":null,\"processId\":\"pid-foreign-1\",\"class\":\"Synthetic\",\
          \"title\":null,\"type\":\"normal\",\"activationPolicy\":\"regular\",\
          \"layer\":0,\"fullscreenOwner\":null,\"fullscreenExitable\":null,\
          \"suppressionAvailable\":null,\
          \"visibility\":{{\"visible\":true,\"suppressionOwned\":null}},\
          \"lastWrite\":null}}",
        x = frame[0],
        y = frame[1],
        w = frame[2],
        h = frame[3],
    )
}

/// Minimal v4 inner report: one complete screen, caller-chosen windows.
/// Native desktop evidence rides the `native` section plus per-window
/// `nativeWs`; the host-owned capability lives in the handshake (always
/// declared by these helpers), never in this report. Birth-clear is a
/// handshake capability, never an inner key. Broker-owned sequencing keys
/// are never carried. Strict v4 ingress:
/// no `mode`, `views`, `hostOwned`, `predicate`, `refill`, `bornMaximized`
/// or float-intent lanes; the broker stamps the sequencing namespace.
fn inner_report(windows: &[String], complete: bool, host_owned: bool, native: &str) -> String {
    inner_report_with_attempts(windows, complete, host_owned, native, "", false)
}

/// Producer inner report with the birth-clear capability opted in. In v4
/// `birthClear` is a handshake capability, never an inner key, so this
/// helper is signature-compatible only; the flag is ignored here.
fn inner_report_birth(
    windows: &[String],
    complete: bool,
    host_owned: bool,
    native: &str,
) -> String {
    inner_report_with_attempts(windows, complete, host_owned, native, "", true)
}

fn inner_report_with_attempts(
    windows: &[String],
    complete: bool,
    host_owned: bool,
    native: &str,
    attempts: &str,
    birth_clear: bool,
) -> String {
    let _ = host_owned;
    let _ = birth_clear;
    let attempts_json = if attempts.is_empty() {
        String::new()
    } else {
        format!(",\"frameAttempts\":[{attempts}]")
    };
    let native_json = if native.is_empty() {
        String::new()
    } else {
        native.to_owned()
    };
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{}],\"closed\":[],\"native\":[{native_json}],\
          \"focused\":\"\",\
          \"complete\":{complete},\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\
          \"presentation\":{}{attempts_json},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null}}",
        windows.join(","),
        disabled_presentation()
    )
}

fn capture(adapter: &str, generation: &str, seq: u64, watermark: u64, inner: &str) -> String {
    capture_source(adapter, generation, &js_source(seq), seq, watermark, inner)
}

/// Raw capture envelope with explicit correlation and sequence, for
/// adversarial shapes (reused sources, rewound sequences) the producer
/// never mints.
fn capture_source(
    adapter: &str,
    generation: &str,
    source: &str,
    seq: u64,
    watermark: u64,
    inner: &str,
) -> String {
    format!(
        "{{\"v\":4,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
          \"source\":\"{source}\",\"sourceSeq\":\"{seq}\",\
          \"nativeInvalid\":\"{watermark}\",\"report\":{inner}}}"
    )
}

/// JS producer correlation for capture `seq`: `src-<counter36>-<rand36>`
/// from `kwin/src/world-entry.ts` `mintSource` (the counter half in base 36,
/// the random half opaque transport randomness, fixed to `t` here for
/// determinism). The decimal `seq` rides alongside as `sourceSeq`.
fn js_source(seq: u64) -> String {
    format!("src-{}-t", to_base36(seq))
}

fn to_base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 is ascii")
}

fn parse(reply: &str) -> serde_json::Value {
    serde_json::from_str(reply).expect("reply is JSON")
}

/// v4 connection handshake for the discovery route (dispatched by type
/// through the connection router). Producer lifetime is the opaque
/// actuator incarnation; every handshake resets onto a fresh generation.
/// The fixture identity is generic on purpose: no machine-local app
/// executable paths. Host capabilities declare the facts these tests
/// carry: host-owned native desktops plus native fullscreen/maximized/
/// sticky facts (otherwise those facts grade to conflicting holds or
/// refuse as native-without-capability).
fn handshake_req(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"fixture-actuator\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":true,\"equalCount\":false,\"birthClear\":false,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

/// v4 settings for the discovery route. Reports wait for initial settings:
/// the handshake binds the lifetime and settings unlock report readiness.
/// Settings never reset the lifetime. Values use the short wire names;
/// the baseline carries the ordinary tiling seed.
fn settings_req(adapter: &str, generation: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\
          \"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"1\",\
          \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\
          \"bornMaximized\":\"tile-on-launch\"}}}}"
    )
}

fn handshake_discover(service: &WorldService, adapter: &str, req: &str) -> String {
    service
        .discover(&handshake_req(adapter, req))
        .expect("handshake routes")
}

fn discover_settings(service: &WorldService, adapter: &str, generation: &str, req: &str) -> String {
    service
        .discover(&settings_req(adapter, generation, req))
        .expect("settings routes")
}

fn handshake_discover_broker(broker: &mut WorldBroker, adapter: &str, req: &str) -> String {
    broker
        .discover_connection(&handshake_req(adapter, req))
        .expect("handshake routes")
}

/// Queue-lane request bound to the serving generation: strict v4
/// {v:4,gen,client,clientCmd,command}.
fn queue_req(generation: &str, client: &str, client_cmd: &str, command: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\",\"command\":{command}}}"
    )
}

/// Take-lane request bound to the serving generation: strict v4
/// {v:4,gen,client,clientCmd}.
fn take_req(generation: &str, client: &str, client_cmd: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\"}}"
    )
}

/// Fresh service with a discovered actuator binding. Returns the service
/// plus its serving generation (read from the handshake: the first
/// discovery binds the lifetime and mints its generation, then settings
/// unlock report readiness).
fn discovered_service() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(&handshake_discover(&service, ADAPTER, "req-0"));
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
    let settings = parse(&discover_settings(
        &service,
        ADAPTER,
        &generation,
        "settings-0",
    ));
    assert_eq!(settings["status"], "accepted");
    (service, generation)
}

fn is_canonical_u64(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 20
        && (text == "0" || (text.as_bytes()[0] != b'0' && text.bytes().all(|b| b.is_ascii_digit())))
        && (text.len() < 20 || text <= "18446744073709551615")
}

#[test]
fn discovery_binds_generation_and_broker_incarnation() {
    let (service, generation) = discovered_service();
    let rediscovery = parse(&handshake_discover(&service, ADAPTER, "req-1"));
    // The reply echoes the producer lifetime verbatim, never a broker
    // incarnation.
    assert_eq!(rediscovery["adapter"], ADAPTER);
    let rotated = rediscovery["gen"].as_str().expect("gen").to_owned();
    assert_ne!(
        rotated, generation,
        "same-lifetime rebind rotates generation"
    );
    let settings = parse(&discover_settings(
        &service,
        ADAPTER,
        &rotated,
        "settings-1",
    ));
    assert_eq!(settings["status"], "accepted");
    // No accepted capture yet on the rotated generation: the presentation
    // route stays not-ready.
    let reply = service
        .read_presentation("{\"v\":4,\"req\":\"read-0\"}")
        .expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "not-ready");
    assert_eq!(parsed["gen"].as_str(), Some(rotated.as_str()));
    // v1 and v3 envelopes refuse on the strict v4 boundary.
    let v1 = "{\"v\":1,\"adapter\":\"a\",\"gen\":\"g\",\"source\":\"src-1\",\
        \"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{\"v\":4}}";
    let reply = service.evaluate_capture(v1).expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "unsupported-version");
    let v3 = "{\"v\":3,\"adapter\":\"a\",\"gen\":\"g\",\"source\":\"src-1\",\
        \"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{\"v\":4}}";
    let reply = service.evaluate_capture(v3).expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "unsupported-version");
    // Captures from undiscovered adapters refuse without applying.
    let inner = inner_report(&[], true, false, "");
    let reply = service
        .evaluate_capture(&capture_source(
            "never-discovered",
            &rotated,
            &js_source(1),
            1,
            1,
            &inner,
        ))
        .expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "unknown-adapter");
    // The bound adapter is the producer lifetime, stable across captures.
    let reply = service
        .evaluate_capture(&capture(ADAPTER, &rotated, 1, 1, &inner))
        .expect("accepted");
    let parsed = parse(&reply);
    assert_eq!(parsed["adapter"].as_str(), Some(ADAPTER));
}

#[test]
fn stale_generation_refuses_without_applying() {
    let (first, gen_first) = discovered_service();
    let (second, gen_second) = discovered_service();
    assert_ne!(gen_first, gen_second);
    let inner = inner_report(&[], true, false, "");
    // A capture bound to another generation refuses on this broker.
    let foreign = capture(ADAPTER, &gen_first, 1, 1, &inner);
    let reply = second.evaluate_capture(&foreign).expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "generation-mismatch");
    assert_eq!(parsed["gen"].as_str(), Some(gen_second.as_str()));
    // The refusing broker is untouched: a valid capture starts at rev 1.
    let reply = second
        .evaluate_capture(&capture(ADAPTER, &gen_second, 1, 1, &inner))
        .expect("accepted");
    assert_eq!(parse(&reply)["rev"], "1");
    // Malformed generation tokens refuse as values, never as matches.
    let bad_gen = capture(ADAPTER, "GEN STALE!", 1, 1, &inner);
    let reply = first.evaluate_capture(&bad_gen).expect("in-band");
    assert_eq!(parse(&reply)["kind"], "unknown-value");
}

#[test]
fn capture_accepts_and_binds_source_watermark_clock() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let reply = service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("accepted");
    let parsed = parse(&reply);
    assert_eq!(parsed["ready"], true);
    assert_eq!(parsed["rev"], "1");
    assert_eq!(parsed["obs"], "1");
    assert_eq!(parsed["req"], js_source(1));
    assert_eq!(parsed["source"], js_source(1));
    assert_eq!(parsed["nativeInvalid"], "1");
    assert!(is_canonical_u64(
        parsed["clock"].as_str().expect("dispatch clock")
    ));
    assert_eq!(parsed["gen"].as_str(), Some(generation.as_str()));
}

#[test]
fn invalidation_watermark_is_monotonic_per_adapter() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let first = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(first["obs"], "1");
    // Invalidation during flight is fenced client-side: the actuator bumps
    // the watermark and the broker binds the new one with fresher
    // observation sequencing.
    let second = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 3, &inner))
            .expect("accepted"),
    );
    assert_eq!(second["obs"], "2");
    assert_eq!(second["nativeInvalid"], "3");
    // A regressed watermark refuses: delayed captures cannot roll back
    // native invalidation.
    let stale = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 2, &inner))
            .expect("in-band"),
    );
    assert_eq!(stale["outcome"], "rejected");
    assert_eq!(stale["kind"], "stale-watermark");
    // An equal watermark is a fresh observation sample, not a regression.
    let same = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 4, 3, &inner))
            .expect("accepted"),
    );
    assert_eq!(same["obs"], "3");
}

#[test]
fn source_correlation_replays_exactly_and_rejects_mutation() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let envelope = capture(ADAPTER, &generation, 1, 1, &inner);
    let first = service.evaluate_capture(&envelope).expect("accepted");
    let replay = service.evaluate_capture(&envelope).expect("replay");
    assert_eq!(replay, first, "redelivery replays byte-identically");
    assert_eq!(parse(&replay)["rev"], "1");
    // Same source with a different body refuses as reused.
    let mutated = capture_source(ADAPTER, &generation, &js_source(1), 1, 2, &inner);
    assert_ne!(mutated, envelope);
    let refused = parse(&service.evaluate_capture(&mutated).expect("in-band"));
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "source-reused");
}

#[test]
fn replay_never_drains_from_an_old_capture() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    // Fresh capture drains the queued command.
    let first_envelope = capture(ADAPTER, &generation, 1, 1, &inner);
    let dispatched = parse(&service.evaluate_capture(&first_envelope).expect("accepted"));
    assert_eq!(dispatched["rev"], "1");
    let ready = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready["outcome"], "ready");
    // A new command queues; redelivering the OLD capture replays exactly
    // instead of draining.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "2",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let replay = service.evaluate_capture(&first_envelope).expect("replay");
    assert_eq!(parse(&replay)["rev"], "1");
    let pending = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "2"))
            .expect("in-band"),
    );
    assert_eq!(pending["outcome"], "pending");
    // Only a fresh source drains.
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], "2");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "2"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn queued_command_drains_exactly_once_with_terminal_result() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    // File a core-resolved numbered selection from an independent client.
    let queued = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    assert_eq!(queued["globalCmd"], "1");
    // Collection before dispatch reports pending, never a result.
    let pending = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(pending["outcome"], "pending");
    // The next fresh capture drains the single queued command.
    let dispatched = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(dispatched["consumed"], false);
    assert_eq!(dispatched["rev"], "1");
    // Exactly-once collection for the issuer; a lost reply re-take
    // re-serves the same settlement verbatim.
    let ready = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready["outcome"], "ready");
    assert_eq!(ready["globalCmd"], "1");
    assert!(ready.get("result").is_some());
    let retake = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(retake, ready, "lost take replies re-serve verbatim");
    // Re-queueing the same per-sender per-client id never re-executes: the
    // next capture dispatches nothing new.
    let duplicate = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(duplicate["outcome"], "duplicate");
    assert_eq!(duplicate["globalCmd"], "1");
    let after = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(after["rev"], "2");
    // Unknown client commands reject instead of inventing state.
    let unknown = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "9"))
            .expect("in-band"),
    );
    assert_eq!(unknown["outcome"], "rejected");
    assert_eq!(unknown["kind"], "unknown-command");
}

#[test]
fn cross_sender_commands_never_steal_or_move_highwater() {
    let (service, generation) = discovered_service();
    // Single-spare fixture: minimized occupant on the second native desktop
    // mints the second numbered slot (occupied final supplies the trailing
    // spare) while taking no switch target, preserving both independent
    // valid selections (indices 1/2).
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    );
    let occupant = window_json("win-stay", "[0,0,200,200]", Some(UUID_B))
        .replace("\"minimized\":false", "\"minimized\":true");
    let inner = inner_report(&[occupant], true, true, &native);
    // Sender A files; sender B can neither collect it nor observe it.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let theft = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(theft["outcome"], "rejected");
    assert_eq!(theft["kind"], "unknown-command");
    // The same client id under another sender is an independent namespace
    // with its own global sequence.
    let other = parse(
        &service
            .queue_command(
                SENDER_B,
                &queue_req(
                    &generation,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":2}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(other["outcome"], "queued");
    assert_eq!(other["globalCmd"], "2");
    let dispatched = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(dispatched["rev"], "1");
    // Each issuer collects only its own settlement. One capture drains the
    // whole batch, so both settlements are ready after the first capture.
    let ready_a = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready_a["outcome"], "ready");
    assert_eq!(ready_a["globalCmd"], "1");
    let ready_b = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready_b["outcome"], "ready");
    assert_eq!(ready_b["globalCmd"], "2");
    // The second capture drains nothing; re-takes re-serve identical
    // settlements without re-execution.
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], "2");
    let again_a = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(again_a, ready_a);
    let again_b = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(again_b, ready_b);
}

#[test]
fn semantic_rejection_settles_terminally_and_stream_continues() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    // Index 9 names no workspace of this world: shape-valid, so it queues,
    // but the world rejects it semantically at dispatch.
    let queued = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":9}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(queued["outcome"], "queued");
    // The dispatch runs the same fresh facts observation-only after
    // settling the command: the actuator lane stays usable and the stream
    // never wedges.
    let observed = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("in-band"),
    );
    assert_eq!(observed["ready"], true);
    assert_eq!(observed["rev"], "1");
    // The issuer collects a truthful terminal failure, never a null
    // result masquerading as success.
    let failed = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(failed["outcome"], "failed");
    assert_eq!(failed["globalCmd"], "1");
    assert_eq!(failed["kind"], "selection-unknown");
    assert!(failed.get("result").is_none());
    // The stream continues observation-only on the next fresh capture.
    let accepted = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(accepted["ready"], true);
    // Repeat takes re-serve the failure; other senders see nothing.
    let again = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(again, failed);
    let theft = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(theft["kind"], "unknown-command");
}

#[test]
fn invalid_capture_never_retires_a_good_queued_command() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    // A syntactically invalid capture (bad window id grammar)
    // rejects without touching the engine or the queue.
    let bad_window = window_json("has space", "[0,0,960,1040]", None);
    let bad_inner = inner_report(&[bad_window], true, false, "");
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &bad_inner))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    let pending = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(pending["outcome"], "pending");
    // The next valid capture drains the still-queued command.
    let accepted = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(accepted["rev"], "1");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn invalid_queued_commands_fail_closed_before_dispatch() {
    let (service, generation) = discovered_service();
    // Unknown ops refuse at queue time through the strict codec.
    let refused = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(&generation, "panel-1", "1", "{\"op\":\"no-such-op\"}"),
            )
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
}

#[test]
fn actuator_reply_carries_no_terminal_result_but_presentation_hides_tokens() {
    let (service, generation) = discovered_service();
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    );
    let inner = inner_report(
        &[window_json("win-001", "[0,0,960,1040]", Some(UUID_A))],
        true,
        true,
        &native,
    );
    // Dispatch an intentional-float placement (terminal result expected).
    let queued_body = service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"toggle-float\",\
                 \"win\":\"win-001\",\"rect\":[100,100,400,300]}",
            ),
        )
        .expect("queued");
    assert_eq!(
        parse(&queued_body)["outcome"],
        "queued",
        "queue body: {queued_body}"
    );
    let dispatched_text = service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("accepted");
    let dispatched = parse(&dispatched_text);
    assert_eq!(dispatched["consumed"], false);
    // The actuator lane keeps its one-shot native work (effects) but never
    // the requester-correlated terminal result, even though no separate
    // issuer exists here.
    assert!(dispatched.get("effects").is_some());
    assert!(
        dispatched["result"].is_null(),
        "actuator reply carries no terminal result"
    );
    // The issuer collects the real verdict via take.
    let ready = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready["outcome"], "ready", "take body: {ready}");
    assert!(!ready["result"].is_null());
    assert_eq!(ready["result"]["op"], "toggle-float");
    // Subscribers read the cached snapshot plus presentation only: no
    // one-shot lanes and no native write tokens in window rows.
    let projection = parse(
        &service
            .read_presentation("{\"v\":4,\"req\":\"pager-1\"}")
            .expect("in-band"),
    );
    assert_eq!(projection["outcome"], "presentation");
    for key in ["scopes", "workspaces", "outputs", "windows", "presentation"] {
        assert!(projection.get(key).is_some(), "projection carries {key}");
    }
    for key in [
        "effects",
        "transfers",
        "result",
        "operationSettled",
        "gestureSettled",
        "cursorWarp",
    ] {
        assert!(
            projection.get(key).is_none(),
            "projection never replays {key}"
        );
    }
    for window in projection["windows"].as_array().expect("windows") {
        assert!(
            window.get("frame_op").is_none(),
            "no float write token leaks to subscribers"
        );
        assert!(
            window.get("clear_op").is_none(),
            "no clear write token leaks to subscribers"
        );
    }
}

#[test]
fn foreign_epoch_attempt_stamps_refuse_above_the_dispatch_clock() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    // A baseline capture publishes the broker clock.
    let baseline = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    let clock = baseline["clock"].as_str().expect("dispatch clock");
    assert!(is_canonical_u64(clock));
    // A raw attempt stamped past the dispatch clock (foreign epoch) refuses;
    // the stream and queue stay untouched.
    let future_attempt = "{\"win\":\"win-001\",\"target\":[0,0,100,100],\"setter\":\"accepted\",\
          \"readback\":[0,0,100,100],\"dpiBefore\":null,\"dpiAfter\":null,\
          \"attemptMs\":\"18446744073709551615\",\"op\":\"\",\"pre\":null}"
        .to_owned();
    let future_inner = inner_report_with_attempts(&[], true, false, "", &future_attempt, false);
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &future_inner))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
    assert!(refused["clock"].as_str().is_some_and(is_canonical_u64));
    // The next valid capture still starts from the untouched revision.
    let accepted = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
            .expect("accepted"),
    );
    assert_eq!(accepted["rev"], "2");
}

#[test]
fn bounded_result_memory_forgets_without_reexecution() {
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    // Settle more commands than the bounded result memory holds.
    for index in 1..=1030_u64 {
        service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "bulk",
                    &index.to_string(),
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("queued");
        service
            .evaluate_capture(&capture(ADAPTER, &generation, index, index, &inner))
            .expect("accepted");
    }
    // The evicted head is forgotten: duplicate, never re-executed.
    let forgotten = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "bulk", "1"))
            .expect("in-band"),
    );
    assert_eq!(forgotten["outcome"], "duplicate");
    // The retained tail still collects.
    let retained = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "bulk", "1030"))
            .expect("in-band"),
    );
    assert_eq!(retained["outcome"], "ready");
}

#[test]
fn host_owned_native_capture_adopts_uuid_desktops_truthfully() {
    let (service, generation) = discovered_service();
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    );
    let inner = inner_report(
        &[
            window_json("win-001", "[0,0,960,1040]", Some(UUID_A)),
            window_json("win-002", "[960,0,960,1040]", Some(UUID_B)),
        ],
        true,
        true,
        &native,
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(reply["ready"], true);
    let workspaces = reply["workspaces"].as_array().expect("workspaces");
    assert!(
        workspaces.len() >= 2,
        "native desktops adopt in native order: {workspaces:?}"
    );
    let windows = reply["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 2, "complete inventory, no silent absence");
}

#[test]
fn unreadable_frames_hold_without_fabricated_allocations() {
    let (service, generation) = discovered_service();
    // An unreadable frame never rides the raw lane: the producer skips the
    // window and marks the inventory incomplete, so the scope holds with
    // no fabricated geometry.
    let inner = inner_report(&[], false, false, "");
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    let windows = reply["windows"].as_array().expect("windows");
    assert_eq!(
        windows.len(),
        0,
        "no fabricated allocation for unreadable facts"
    );
}

#[test]
fn constructions_mint_fresh_namespaces() {
    let (first, _) = discovered_service();
    let (second, _) = discovered_service();
    assert_ne!(first.generation(), second.generation());
    // Command state never crosses generations or senders.
    first
        .queue_command(
            SENDER_A,
            &queue_req(
                &first.generation(),
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":2}",
            ),
        )
        .expect("queued");
    let unknown = parse(
        &second
            .take_result(SENDER_A, &take_req(&second.generation(), "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(unknown["outcome"], "rejected");
    assert_eq!(unknown["kind"], "unknown-command");
}

#[test]
fn overbound_capture_fails_closed_without_touching_backend() {
    let (service, generation) = discovered_service();
    let big = "x".repeat(4 * 1024 * 1024 + 1);
    match service.evaluate_capture(&big) {
        Err(WorldServiceError::Oversized) => {}
        other => panic!("expected oversized, got {other:?}"),
    }
    assert_eq!(service.generation(), generation);
    // Backend untouched: a valid capture still starts at revision 1.
    let inner = inner_report(&[], true, false, "");
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(reply["rev"], "1");
}

#[test]
fn bootstrap_discovery_and_full_capture_match_the_js_producer_shape() {
    // Producer contract end to end through the real discovery route: the
    // handshake request names the producer lifetime, the handshake reply
    // echoes that producer identity (no broker incarnation) with a fresh
    // generation, and the capture envelope binds the lifetime, the serving
    // generation, the JS correlation (`src-<counter36>-<rand36>`) with its
    // decimal sequence, the local watermark, and the commandless inner
    // report. Stable schemas: handshake
    // {type,v,adapter,req,applicationIdentity,capabilities}; capture
    // {v,adapter,gen,source,sourceSeq,nativeInvalid,report}; replies bind
    // source/nativeInvalid/clock only (no sourceSeq echo).
    let service = WorldService::new();
    let lifetime = "kwin-world-adapter-1";
    let discovery = parse(&handshake_discover(&service, lifetime, "disc-1"));
    assert_eq!(discovery["type"], "handshake-reply");
    assert_eq!(discovery["v"], 4);
    assert_eq!(discovery["ready"], false);
    assert_eq!(discovery["kind"], "");
    assert_eq!(discovery["req"], "disc-1");
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
    assert_eq!(service.generation(), generation);
    // The reply echoes the producer lifetime verbatim, never a broker
    // incarnation.
    assert_eq!(discovery["adapter"], lifetime);
    let settings = parse(&discover_settings(
        &service,
        lifetime,
        &generation,
        "settings-1",
    ));
    assert_eq!(settings["status"], "accepted");

    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    );
    let inner = inner_report(
        &[window_json("win-001", "[0,0,960,1040]", Some(UUID_A))],
        true,
        true,
        &native,
    );
    let envelope = capture(lifetime, &generation, 1, 1, &inner);
    let body: serde_json::Value = serde_json::from_str(&envelope).expect("envelope is JSON");
    let mut keys: Vec<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "adapter",
            "gen",
            "nativeInvalid",
            "report",
            "source",
            "sourceSeq",
            "v"
        ],
        "exact stable capture schema"
    );
    assert_eq!(body["source"], js_source(1));
    assert_eq!(body["sourceSeq"], "1");
    let reply = parse(&service.evaluate_capture(&envelope).expect("accepted"));
    assert_eq!(reply["ready"], true);
    assert_eq!(reply["rev"], "1");
    assert_eq!(reply["obs"], "1");
    assert_eq!(reply["req"], js_source(1));
    assert_eq!(reply["source"], js_source(1));
    assert_eq!(reply["nativeInvalid"], "1");
    assert!(
        reply.get("sourceSeq").is_none(),
        "no sequence echo in replies"
    );
    assert!(reply["clock"].as_str().is_some_and(is_canonical_u64));
    assert_eq!(reply["adapter"].as_str(), Some(lifetime));
    assert_eq!(reply["gen"].as_str(), Some(generation.as_str()));
    assert_eq!(
        reply["windows"].as_array().map(Vec::len),
        Some(1),
        "homed window admits, no silent absence"
    );
}

#[test]
fn redelivered_older_capture_never_drains_a_newer_queue() {
    // A,B,A with a drain: A dispatches the first queued command, B the
    // second, then a third command queues and A is redelivered byte
    // identically. The redelivery replays A's own bound outcome (rev 1),
    // never dispatches, and never drains the newer queue; only a fresh
    // sequence drains it. The cached replay fits the expanded reply cap.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let queue = |client_cmd: &str| {
        service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "panel-1",
                    client_cmd,
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("queued")
    };
    let take = |client_cmd: &str| {
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", client_cmd))
                .expect("in-band"),
        )
    };
    queue("1");
    let envelope_a = capture(ADAPTER, &generation, 1, 1, &inner);
    let accepted_a = service.evaluate_capture(&envelope_a).expect("accepted");
    assert_eq!(parse(&accepted_a)["rev"], "1");
    assert_eq!(take("1")["outcome"], "ready");
    queue("2");
    let envelope_b = capture(ADAPTER, &generation, 2, 2, &inner);
    let accepted_b = service.evaluate_capture(&envelope_b).expect("accepted");
    assert_eq!(parse(&accepted_b)["rev"], "2");
    assert_eq!(take("2")["outcome"], "ready");
    // A third command queues; redelivering A replays A's bytes exactly.
    queue("3");
    let replay = service.evaluate_capture(&envelope_a).expect("replay");
    assert_eq!(
        replay, accepted_a,
        "older redelivery replays, never dispatches"
    );
    assert_eq!(parse(&replay)["rev"], "1");
    assert!(
        replay.len() <= WORLD_SERVICE_MAX_REPLY,
        "cached capture replay fits the expanded reply cap"
    );
    assert_eq!(
        take("3")["outcome"],
        "pending",
        "older redelivery drains nothing"
    );
    // Only the fresh sequence drains the newer queue.
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &inner))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], "3");
    assert_eq!(take("3")["outcome"], "ready");
}

#[test]
fn rewound_source_sequence_refuses_without_draining() {
    // A fresh capture advances the per-lifetime floor; an unremembered
    // source at or below the floor is an older capture (or a rewound
    // producer) and refuses as stale instead of re-dispatching.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let fifth = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 5, 5, &inner))
            .expect("accepted"),
    );
    assert_eq!(fifth["rev"], "1");
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let stale = parse(
        &service
            .evaluate_capture(&capture_source(
                ADAPTER,
                &generation,
                &js_source(99),
                3,
                6,
                &inner,
            ))
            .expect("in-band"),
    );
    assert_eq!(stale["outcome"], "rejected");
    assert_eq!(stale["kind"], "stale-source");
    assert_eq!(stale["source"], js_source(99));
    let pending = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(
        pending["outcome"], "pending",
        "stale capture drains nothing"
    );
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 6, 7, &inner))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], "2");
}

#[test]
fn evicted_source_redelivery_refuses_without_draining() {
    // Fill the bounded per-source response memory past eviction, then prove
    // a forgotten redelivery cannot resurrect: it refuses as stale and never
    // drains a newer queue, while a remembered tail still replays exactly.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    let bound = u64::try_from(MAX_REMEMBERED_SOURCES).expect("bound fits");
    let mut first_envelope = String::new();
    let mut last_envelope = String::new();
    let mut last_reply = String::new();
    for seq in 1..=bound + 8 {
        let envelope = capture(ADAPTER, &generation, seq, seq, &inner);
        let reply = service.evaluate_capture(&envelope).expect("accepted");
        if seq == 1 {
            first_envelope.clone_from(&envelope);
        }
        last_envelope = envelope;
        last_reply = reply;
    }
    assert_eq!(parse(&last_reply)["rev"], (bound + 8).to_string());
    // The evicted head refuses without re-dispatching: its watermark
    // regressed (stale-watermark), and even a current watermark under the
    // forgotten sequence refuses through the floor (stale-source).
    let refused = parse(&service.evaluate_capture(&first_envelope).expect("in-band"));
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "stale-watermark");
    let mutated = capture_source(ADAPTER, &generation, &js_source(1), 1, bound + 8, &inner);
    let refused = parse(&service.evaluate_capture(&mutated).expect("in-band"));
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "stale-source");
    // The forgotten redelivery drains nothing queued after it.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let refused = parse(&service.evaluate_capture(&first_envelope).expect("in-band"));
    assert_eq!(refused["kind"], "stale-watermark");
    let pending = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(pending["outcome"], "pending");
    // The remembered tail still replays byte-identically (also cap-bound).
    let replay = service.evaluate_capture(&last_envelope).expect("replay");
    assert_eq!(replay, last_reply);
    assert!(replay.len() <= WORLD_SERVICE_MAX_REPLY);
    // Only a fresh sequence past the floor drains.
    let drained = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                bound + 9,
                bound + 10,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], (bound + 9).to_string());
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn failed_drain_falls_back_to_fresh_facts_without_poisoning_cache() {
    // A queued command that is shape-valid but semantically rejected at
    // dispatch settles terminally as failed, then the same fresh facts
    // dispatch observation-only. The fallback binds a fresh full reply for
    // its source (never a stale cached full reply), the actuator lane keeps
    // a null result, the presentation cache holds the fresh projection, and
    // redelivery replays the fallback outcome without re-dispatching.
    let (service, generation) = discovered_service();
    let inner = inner_report(&[], true, false, "");
    // Index 9 names no workspace of this world: queues, then fails at
    // dispatch.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":9}",
            ),
        )
        .expect("queued");
    let envelope = capture(ADAPTER, &generation, 1, 1, &inner);
    let observed_text = service
        .evaluate_capture(&envelope)
        .expect("fallback accepted");
    let observed = parse(&observed_text);
    assert_eq!(observed["ready"], true);
    assert_eq!(observed["rev"], "1");
    assert!(
        observed["result"].is_null(),
        "actuator lane keeps no result"
    );
    let failed = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(failed["outcome"], "failed");
    assert_eq!(failed["globalCmd"], "1");
    assert_eq!(failed["kind"], "selection-unknown");
    assert!(failed.get("result").is_none());
    // The cached presentation is the fresh fallback projection, not a stale
    // full reply and not a poisoned null projection.
    let projection = parse(
        &service
            .read_presentation("{\"v\":4,\"req\":\"pager-1\"}")
            .expect("in-band"),
    );
    assert_eq!(projection["outcome"], "presentation");
    assert_eq!(projection["rev"], "1");
    assert!(projection.get("result").is_none());
    // Redelivery replays the fallback outcome byte-identically.
    let replay = service.evaluate_capture(&envelope).expect("replay");
    assert_eq!(replay, observed_text);
    // The stream continues on the next fresh capture.
    let next = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(next["rev"], "2");
}

#[test]
fn discovery_binds_a_single_lifetime_and_rotates_on_change() {
    // Discovery binds exactly one active capture lifetime, and every
    // handshake resets onto a fresh generation: same-lifetime rediscovery
    // rotates the generation and drops broker state (captures, watermarks,
    // queues, and presentations from the old generation are cancelled, so
    // no old capture can ever apply again), while a new lifetime rotates
    // the producer binding too.
    let service = WorldService::new();
    let inner = inner_report(&[], true, false, "");
    let boot_a = parse(&handshake_discover(&service, "lifetime-a", "req-0"));
    let gen_a = boot_a["gen"].as_str().expect("gen").to_owned();
    // The reply echoes the producer lifetime verbatim, never a broker
    // incarnation.
    assert_eq!(boot_a["adapter"], "lifetime-a");
    assert_eq!(
        parse(&discover_settings(
            &service,
            "lifetime-a",
            &gen_a,
            "settings-0"
        ))["status"],
        "accepted"
    );
    // Same-lifetime rediscovery resets: fresh generation, same producer
    // binding.
    let again = parse(&handshake_discover(&service, "lifetime-a", "req-1"));
    let gen_a2 = again["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_a2, gen_a, "same-lifetime rebind rotates generation");
    assert_eq!(again["adapter"], "lifetime-a");
    assert_eq!(
        parse(&discover_settings(
            &service,
            "lifetime-a",
            &gen_a2,
            "settings-1"
        ))["status"],
        "accepted"
    );
    // The rotated generation cancels the old one: its captures, queues,
    // and takes refuse as mismatched, never applying.
    let stale_capture = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_a,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("in-band"),
    );
    assert_eq!(stale_capture["outcome"], "rejected");
    assert_eq!(stale_capture["kind"], "generation-mismatch");
    // Accept on the serving generation, rediscover, and prove the reset:
    // the presentation cache clears and the revision restarts.
    let first = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_a2,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(first["rev"], "1");
    let same = parse(&handshake_discover(&service, "lifetime-a", "req-2"));
    let gen_a3 = same["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_a3, gen_a2);
    assert_eq!(
        parse(&discover_settings(
            &service,
            "lifetime-a",
            &gen_a3,
            "settings-2"
        ))["status"],
        "accepted"
    );
    let cleared = parse(
        &service
            .read_presentation("{\"v\":4,\"req\":\"r-0\"}")
            .expect("in-band"),
    );
    assert_eq!(cleared["outcome"], "rejected");
    assert_eq!(cleared["kind"], "not-ready");
    let second = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_a3,
                &js_source(2),
                2,
                2,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(second["rev"], "1", "reset restarts the revision");
    // A new lifetime rotates: fresh generation and producer binding.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &gen_a3,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let boot_b = parse(&handshake_discover(&service, "lifetime-b", "req-3"));
    let gen_b = boot_b["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_b, gen_a3);
    assert_eq!(service.generation(), gen_b);
    assert_eq!(boot_b["adapter"], "lifetime-b");
    assert_eq!(
        parse(&discover_settings(
            &service,
            "lifetime-b",
            &gen_b,
            "settings-3"
        ))["status"],
        "accepted"
    );
    // No old capture applies: the old lifetime under the new generation
    // refuses, and the old generation refuses outright.
    let stale_lifetime = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_b,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("in-band"),
    );
    assert_eq!(stale_lifetime["outcome"], "rejected");
    assert_eq!(stale_lifetime["kind"], "unknown-adapter");
    let stale_gen = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-b",
                &gen_a3,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("in-band"),
    );
    assert_eq!(stale_gen["outcome"], "rejected");
    assert_eq!(stale_gen["kind"], "generation-mismatch");
    // Queue/take requests bound to the dead generation refuse as
    // generation-mismatch (never unknown-command, never a silent drop):
    // issuers re-establish with fresh per-client command ids, and
    // presentations from the old lifetime are cancelled.
    let stale_queue = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &gen_a3,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(stale_queue["outcome"], "rejected");
    assert_eq!(stale_queue["kind"], "generation-mismatch");
    assert_eq!(stale_queue["gen"], gen_b.as_str());
    let dropped = parse(
        &service
            .take_result(SENDER_A, &take_req(&gen_a3, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(dropped["outcome"], "rejected");
    assert_eq!(dropped["kind"], "generation-mismatch");
    assert_eq!(dropped["gen"], gen_b.as_str());
    // The dead generation filed nothing: the same per-client id files
    // fresh under the new generation (global sequence restarts, never a
    // remint of the old intent).
    let refiled = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(
                    &gen_b,
                    "panel-1",
                    "1",
                    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
                ),
            )
            .expect("in-band"),
    );
    assert_eq!(refiled["outcome"], "queued");
    assert_eq!(refiled["globalCmd"], "1");
    assert_eq!(refiled["gen"], gen_b.as_str());
    let not_ready = parse(
        &service
            .read_presentation("{\"v\":4,\"req\":\"r-1\"}")
            .expect("in-band"),
    );
    assert_eq!(not_ready["outcome"], "rejected");
    assert_eq!(not_ready["kind"], "not-ready");
    // The watermark floor restarts: the new lifetime observes from zero and
    // binds the producer lifetime.
    let fresh = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-b",
                &gen_b,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(fresh["rev"], "1");
    assert_eq!(fresh["adapter"].as_str(), Some("lifetime-b"));
}

/// Strict wait-request shape: exact keys, v4, generation binding, producer
/// lifetime, waiter correlation, canonical cursor.
fn wait_req(generation: &str, adapter: &str, nonce: &str, cursor: &str) -> String {
    format!(
        "{{\"v\":4,\"gen\":\"{generation}\",\"adapter\":\"{adapter}\",\
          \"nonce\":\"{nonce}\",\"cursor\":\"{cursor}\"}}"
    )
}

fn noop_park(broker: &mut WorldBroker, nonce: &str, cursor: u64) {
    broker.wait_park(nonce, cursor, None);
}

/// Fresh broker with a discovered actuator binding, plus its generation.
fn discovered_broker() -> (WorldBroker, String) {
    let mut broker = WorldBroker::new();
    let discovery: serde_json::Value =
        serde_json::from_str(&handshake_discover_broker(&mut broker, ADAPTER, "req-0"))
            .expect("bootstrap");
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    let settings = broker
        .discover_connection(&settings_req(ADAPTER, &generation, "settings-0"))
        .expect("settings routes");
    assert_eq!(parse(&settings)["status"], "accepted");
    (broker, generation)
}

fn select_numbered_command() -> &'static str {
    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}"
}

#[test]
fn wait_request_shape_is_strict_v4() {
    let (_, generation) = discovered_broker();
    let (wait_gen, adapter, nonce, cursor) =
        parse_wait_request(&wait_req(&generation, ADAPTER, "wait-1", "0"))
            .expect("producer wait parses");
    assert_eq!(wait_gen, generation);
    assert_eq!(adapter, ADAPTER);
    assert_eq!(nonce, "wait-1");
    assert_eq!(cursor, 0);
    // v1/v2/v3 refuse; unknown fields refuse; non-canonical cursors refuse.
    assert!(parse_wait_request("{\"v\":1}").is_err());
    assert!(parse_wait_request("{\"v\":2}").is_err());
    assert!(parse_wait_request("{\"v\":3}").is_err());
    assert!(
        parse_wait_request(&format!(
            "{},\"x\":1}}",
            wait_req(&generation, ADAPTER, "wait-1", "0").trim_end_matches('}')
        ))
        .is_err()
    );
    assert!(parse_wait_request(&wait_req(&generation, ADAPTER, "wait-1", "01")).is_err());
    assert!(parse_wait_request(&wait_req(&generation, ADAPTER, "", "0")).is_err());
}

#[test]
fn wait_rejects_stale_generation_and_unknown_lifetime() {
    let (mut broker, generation) = discovered_broker();
    let stale = broker
        .wait_immediate(ADAPTER, "w-stale", 0)
        .expect_err("stale generation rejects");
    let stale_value: serde_json::Value = serde_json::from_str(&stale).expect("in-band JSON");
    assert_eq!(stale_value["outcome"], "rejected");
    assert_eq!(stale_value["kind"], "generation-mismatch");
    let unknown = broker
        .wait_immediate("other-lifetime", &generation, 0)
        .expect_err("unknown lifetime rejects");
    let unknown_value: serde_json::Value = serde_json::from_str(&unknown).expect("in-band JSON");
    assert_eq!(unknown_value["outcome"], "rejected");
    assert_eq!(unknown_value["kind"], "unknown-adapter");
    assert_eq!(broker.waiter_nonce(), None, "rejections park nothing");
}

#[test]
fn wait_parks_while_idle_and_wakes_on_queued_command() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-1", 0);
    assert_eq!(broker.waiter_nonce(), Some("wait-1"));
    assert_eq!(broker.take_wait_ready("wait-1"), None);
    // A newly queued command advances the epoch and completes the waiter.
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let wake = broker.take_wait_ready("wait-1").expect("wake delivered");
    let wake_value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(wake_value["outcome"], "wake");
    assert_eq!(
        wake_value["gen"],
        serde_json::Value::String(generation.clone())
    );
    assert_eq!(wake_value["cursor"], "1");
    assert_eq!(broker.waiter_nonce(), None, "delivery clears the slot");
    // A foreign nonce collects nothing.
    assert_eq!(broker.take_wait_ready("wait-2"), None);
}

#[test]
fn wait_is_immediate_once_the_epoch_advanced_with_pending_work() {
    let (mut broker, generation) = discovered_broker();
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let immediate = broker
        .wait_immediate(ADAPTER, &generation, 0)
        .expect("binds")
        .expect("immediate wake");
    let value: serde_json::Value = serde_json::from_str(&immediate).expect("wake JSON");
    assert_eq!(value["outcome"], "wake");
    assert_eq!(value["cursor"], "1");
    assert_eq!(broker.waiter_nonce(), None, "immediate wake parks nothing");
}

#[test]
fn wait_stays_parked_after_the_queue_drains() {
    let (mut broker, generation) = discovered_broker();
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let inner = inner_report(&[], true, false, "");
    broker
        .dispatch_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("drains");
    // The drain advanced the epoch, but nothing is pending: a waiter at the
    // older cursor still parks instead of hot-looping on a drained queue.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 1)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-drained", 1);
    assert_eq!(broker.take_wait_ready("wait-drained"), None);
    // Lease expiry keeps the cursor: re-arming with it can never skip work.
    assert_eq!(broker.expire_wait("wait-drained"), Some(1));
    assert_eq!(broker.waiter_nonce(), None);
    assert_eq!(broker.expire_wait("wait-drained"), None);
    let lease = broker.wait_lease_body(1);
    let lease_value: serde_json::Value = serde_json::from_str(&lease).expect("lease JSON");
    assert_eq!(lease_value["outcome"], "lease");
    assert_eq!(lease_value["cursor"], "1");
}

#[test]
fn rejected_capture_never_advances_the_epoch_or_wakes() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-1", 0);
    // A malformed capture rejects without draining: the epoch is
    // untouched, so the waiter stays parked (no hot-loop on re-arm).
    let rejected = broker
        .dispatch_capture("{\"v\":3,\"bogus\":true}")
        .expect("in-band");
    let rejected_value: serde_json::Value =
        serde_json::from_str(&rejected).expect("rejection JSON");
    assert_eq!(rejected_value["outcome"], "rejected");
    assert_eq!(broker.waiter_nonce(), Some("wait-1"));
    assert_eq!(broker.take_wait_ready("wait-1"), None);
    // The next queue still wakes from the same cursor: nothing was lost.
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let wake = broker.take_wait_ready("wait-1").expect("wake delivered");
    let wake_value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(wake_value["cursor"], "1");
}

#[test]
fn exact_capture_replay_never_wakes() {
    let (mut broker, generation) = discovered_broker();
    let inner = inner_report(&[], true, false, "");
    let envelope = capture(ADAPTER, &generation, 1, 1, &inner);
    let first = broker.dispatch_capture(&envelope).expect("accepted");
    // Park at the current epoch (no pending work, no epoch advance).
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-replay", 0);
    let replayed = broker.dispatch_capture(&envelope).expect("replays");
    assert_eq!(replayed, first, "exact redelivery replays byte-identically");
    assert_eq!(broker.waiter_nonce(), Some("wait-replay"));
    assert_eq!(broker.take_wait_ready("wait-replay"), None);
}

#[test]
fn newer_waiter_supersedes_the_parked_one() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-old", 0);
    noop_park(&mut broker, "wait-new", 0);
    assert_eq!(broker.waiter_nonce(), Some("wait-new"));
    assert_eq!(broker.take_wait_ready("wait-old"), None);
    let superseded = broker.wait_superseded_body();
    let value: serde_json::Value = serde_json::from_str(&superseded).expect("superseded JSON");
    assert_eq!(value["outcome"], "superseded");
    assert_eq!(value["gen"], serde_json::Value::String(generation));
}

#[test]
fn burst_commands_wake_capture_by_capture() {
    let (mut broker, generation) = discovered_broker();
    for client_cmd in ["1", "2", "3"] {
        broker
            .queue_command(
                SENDER_A,
                &queue_req(
                    &generation,
                    "panel-1",
                    client_cmd,
                    select_numbered_command(),
                ),
            )
            .expect("queued");
    }
    // One immediate wake covers the whole burst at the current epoch.
    let immediate = broker
        .wait_immediate(ADAPTER, &generation, 0)
        .expect("binds")
        .expect("immediate wake");
    let value: serde_json::Value = serde_json::from_str(&immediate).expect("wake JSON");
    assert_eq!(value["cursor"], "3");
    // Park at the delivered cursor; the first draining capture settles the
    // whole batch at once, leaving nothing pending so no tail wake fires.
    let inner = inner_report(&[], true, false, "");
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 3)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-burst", 3);
    broker
        .dispatch_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("drains all");
    assert_eq!(broker.take_wait_ready("wait-burst"), None);
    // One capture settles all three commands.
    for client_cmd in ["1", "2", "3"] {
        let settled = parse(
            &broker
                .take_result(SENDER_A, &take_req(&generation, "panel-1", client_cmd))
                .expect("in-band"),
        );
        assert_eq!(settled["outcome"], "ready");
    }
    // Nothing pending, so a waiter at the delivered cursor parks; the next
    // capture drains nothing and delivers no spurious wake.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 3)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-tail", 3);
    broker
        .dispatch_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
        .expect("drains none");
    assert_eq!(broker.take_wait_ready("wait-tail"), None);
    // A newly queued command wakes the parked waiter.
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "4", select_numbered_command()),
        )
        .expect("queued");
    assert!(
        broker
            .wait_immediate(ADAPTER, &generation, 3)
            .expect("binds")
            .is_some(),
        "new command wakes"
    );
}

#[test]
fn lifetime_rotation_drops_the_waiter_and_resets_the_epoch() {
    let (mut broker, generation) = discovered_broker();
    // Park while idle, then queue: the waiter wakes at the first epoch.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-1", 0);
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let wake = broker.take_wait_ready("wait-1").expect("wake delivered");
    let wake_value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(wake_value["cursor"], "1");
    // Re-park at the delivered cursor; rotation drops the waiter and
    // resets the epoch under a fresh generation.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 1)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-2", 1);
    let rotated: serde_json::Value = serde_json::from_str(&handshake_discover_broker(
        &mut broker,
        "lifetime-b",
        "req-1",
    ))
    .expect("bootstrap");
    let fresh_generation = rotated["gen"].as_str().expect("gen").to_owned();
    assert_ne!(fresh_generation, generation);
    assert_eq!(broker.waiter_nonce(), None, "rotation drops the waiter");
    // The old cursor is past the reset epoch: no phantom wake, just park.
    assert_eq!(
        broker
            .wait_immediate("lifetime-b", &fresh_generation, 1)
            .expect("binds"),
        None
    );
    assert!(parse_wait_request(&wait_req(&generation, ADAPTER, "wait-1", "0")).is_ok());
}

#[test]
fn rotation_wakes_the_displaced_waiter_with_mismatch() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-1", 0);
    // A new producer lifetime rotates the broker: the displaced waiter
    // wakes immediately with generation-mismatch against the fresh
    // generation instead of sleeping out its lease (and never superseded:
    // a stale generation answers mismatch).
    let rotated: serde_json::Value = serde_json::from_str(&handshake_discover_broker(
        &mut broker,
        "lifetime-b",
        "req-1",
    ))
    .expect("bootstrap");
    let fresh = rotated["gen"].as_str().expect("gen").to_owned();
    assert_ne!(fresh, generation);
    assert_eq!(broker.waiter_nonce(), None);
    let displaced = broker
        .take_wait_ready("wait-1")
        .expect("displaced waiter wakes");
    let value: serde_json::Value = serde_json::from_str(&displaced).expect("mismatch JSON");
    assert_eq!(value["outcome"], "rejected");
    assert_eq!(value["kind"], "generation-mismatch");
    assert_eq!(value["gen"], serde_json::Value::String(fresh.clone()));
    // The old generation stays mismatched (never superseded) after rotation.
    let stale = broker
        .wait_immediate(ADAPTER, &generation, 0)
        .expect_err("rotated generation mismatches");
    let stale_value: serde_json::Value = serde_json::from_str(&stale).expect("mismatch JSON");
    assert_eq!(stale_value["kind"], "generation-mismatch");
    assert_eq!(stale_value["gen"], serde_json::Value::String(fresh));
}

#[test]
fn park_clears_a_stale_ready_for_another_nonce() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-orphan", 0);
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    assert!(broker.take_wait_ready("wait-orphan").is_some());
    // Re-parking under a new nonce drops the orphaned ready: the single
    // bounded slot never wedges on an uncollected reply.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 1)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-live", 1);
    assert_eq!(broker.take_wait_ready("wait-orphan"), None);
    // The live waiter still wakes on the next queued work: no lost work
    // for the live slot after the orphaned ready was dropped.
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "2", select_numbered_command()),
        )
        .expect("queued");
    let wake = broker
        .take_wait_ready("wait-live")
        .expect("live waiter wakes");
    let value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(value["outcome"], "wake");
    assert_eq!(value["cursor"], "2");
}

#[test]
fn expiry_and_collection_touch_only_the_owned_nonce() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-owned", 0);
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    // A foreign expiry or collection touches nothing: the owned ready and
    // waiter survive intact.
    assert_eq!(broker.expire_wait("wait-foreign"), None);
    assert_eq!(broker.take_wait_ready("wait-foreign"), None);
    assert_eq!(broker.waiter_nonce(), None, "delivery cleared the slot");
    let wake = broker
        .take_wait_ready("wait-owned")
        .expect("owned ready survives");
    let value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(value["cursor"], "1");
}

#[test]
fn dropped_waiter_cannot_displace_a_newer_one() {
    let (mut broker, generation) = discovered_broker();
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 0)
            .expect("parks"),
        None
    );
    noop_park(&mut broker, "wait-old", 0);
    noop_park(&mut broker, "wait-new", 0);
    // The dropped old waiter cleans up only its own nonce: the newer slot
    // survives, and the next drain still wakes it (no lost work).
    assert_eq!(broker.expire_wait("wait-old"), None);
    assert_eq!(broker.take_wait_ready("wait-old"), None);
    assert_eq!(broker.waiter_nonce(), Some("wait-new"));
    broker
        .queue_command(
            SENDER_A,
            &queue_req(&generation, "panel-1", "1", select_numbered_command()),
        )
        .expect("queued");
    let wake = broker
        .take_wait_ready("wait-new")
        .expect("newer waiter wakes");
    let value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(value["outcome"], "wake");
    assert_eq!(value["cursor"], "1");
}

#[test]
fn raw_capture_rejects_before_drain_without_consuming_counters() {
    // Strict v4 lane: missing/mismatched/extra/duplicate window facts
    // reject before any command drain and before the observation sequence
    // advances, so a queued command stays pending and the next valid
    // capture still mints the unskipped revision. (Retired verdict keys
    // and wrong-typed values hold the identifiable row instead of
    // refusing; only the refusal shapes below belong here.)
    let (service, generation) = discovered_service();
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let good = window_json("win-001", "[0,0,960,1040]", None);
    let mutate = |from: &str, to: &str| inner_report(&[good.replace(from, to)], true, false, "");
    let cases: Vec<(&str, String, &str)> = vec![
        ("missing", mutate("\"out\":\"DP-1\",", ""), "malformed"),
        (
            "bad-id",
            mutate("\"win\":\"win-001\"", "\"win\":\"has space\""),
            "unknown-value",
        ),
        (
            "bad-out-id",
            mutate("\"out\":\"DP-1\"", "\"out\":\"has space\""),
            "unknown-value",
        ),
        (
            "unknown-key",
            mutate("\"minimized\":false", "\"minimized\":false,\"bogus\":1"),
            "unknown-field",
        ),
    ];
    for (seq, (name, inner, kind)) in cases.into_iter().enumerate() {
        let seq = (seq as u64) + 1;
        let refused = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, &generation, seq, seq, &inner))
                .expect("in-band"),
        );
        assert_eq!(refused["outcome"], "rejected", "{name}");
        assert_eq!(refused["kind"], kind, "{name}");
        assert_eq!(
            parse(
                &service
                    .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                    .expect("in-band")
            )["outcome"],
            "pending",
            "{name} drains nothing"
        );
    }
    // Duplicate native identities refuse with the core duplicate vocabulary.
    let dup_inner = inner_report(
        &[
            window_json("win-001", "[0,0,960,1040]", None),
            window_json("win-001", "[0,0,960,1040]", None),
        ],
        true,
        false,
        "",
    );
    let dup = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 6, 6, &dup_inner))
            .expect("in-band"),
    );
    assert_eq!(dup["outcome"], "rejected");
    assert_eq!(dup["kind"], "snapshot-invalid");
    assert_eq!(dup["message"], "duplicate-window");
    // The next valid capture still mints the unskipped revision and drains.
    let accepted = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                7,
                7,
                &inner_report(&[good], true, false, ""),
            ))
            .expect("accepted"),
    );
    assert_eq!(accepted["rev"], "1", "no counter consumed by raw rejects");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn raw_rejection_replays_exactly_while_changed_source_refuses_reuse() {
    // A remembered v4 rejection replays byte-identically (never draining);
    // the same source with different bytes refuses as reused.
    let (service, generation) = discovered_service();
    let bad_inner = inner_report(
        &[window_json("has space", "[0,0,960,1040]", None)],
        true,
        false,
        "",
    );
    let envelope = capture(ADAPTER, &generation, 1, 1, &bad_inner);
    let first = service.evaluate_capture(&envelope).expect("in-band");
    assert_eq!(parse(&first)["kind"], "unknown-value");
    let replay = service.evaluate_capture(&envelope).expect("replay");
    assert_eq!(replay, first, "exact redelivery replays");
    let changed_inner = inner_report(
        &[window_json("win-002", "[0,0,960,1040]", None)],
        true,
        false,
        "",
    );
    let changed = capture_source(ADAPTER, &generation, &js_source(1), 1, 1, &changed_inner);
    let refused = parse(&service.evaluate_capture(&changed).expect("in-band"));
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "source-reused");
}

#[test]
fn v4_facts_project_fixed_minimized_overlay_and_safety() {
    // Producer-shaped v4 facts: native UUID desktop ids, sanitized frame
    // hints (equal nonzero pin, zero/sentinel bounds, settled minimum with
    // unbounded maximum), minimized retention, a fullscreen overlay, and a
    // homed non-normal non-dialog native kind. The core derives
    // float/fixed verdicts from the carried frame hints and owns explicit
    // overrides; the homed menu kind holds with no frame and no actuation
    // (only actual dialogs open the non-normal retained lane).
    let (service, generation) = discovered_service();
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    );
    let fixed = window_json("win-fixed", "[0,0,408,308]", Some(UUID_A)).replace(
        "\"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}",
        "\"hints\":{\"minW\":408,\"minH\":308,\"maxW\":408,\"maxH\":308}",
    );
    let sentinel = window_json("win-sentinel", "[0,0,200,200]", Some(UUID_A)).replace(
        "\"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}",
        "\"hints\":{\"minW\":0,\"minH\":0,\"maxW\":16777215,\"maxH\":16777215}",
    );
    let unbounded = window_json("win-unbounded", "[0,0,200,200]", Some(UUID_A)).replace(
        "\"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}",
        "\"hints\":{\"minW\":176,\"minH\":150,\"maxW\":2147483647,\"maxH\":2147483647}",
    );
    let minimized = window_json("win-min", "[0,0,200,200]", Some(UUID_B))
        .replace("\"minimized\":false", "\"minimized\":true");
    let overlay = window_json("win-full", "[0,0,1920,1080]", Some(UUID_B))
        .replace("\"fullscreen\":false", "\"fullscreen\":true");
    let menu = window_json("win-menu", "[0,0,200,200]", Some(UUID_B))
        .replace("\"type\":\"normal\"", "\"type\":\"menu\"");
    // The fullscreen member rides a non-current desktop: the overlay is
    // retained there while the current scope stays controlled.
    let inner = inner_report(
        &[fixed, sentinel, unbounded, minimized, overlay, menu],
        true,
        true,
        &native,
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(reply["ready"], true);
    let windows = reply["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 6, "homed menu holds as a row, rest project");
    let row = |id: &str| {
        windows
            .iter()
            .find(|w| w["win"] == id)
            .unwrap_or_else(|| panic!("projected row {id}"))
            .clone()
    };
    // A homed non-dialog native kind holds its row with no frame and no
    // actuation: geometry and membership stay held, no frame or clear
    // write rides, no state write is desired, yet the row homes on its
    // native desktop (never confused with an unknown home). The probe
    // rides the non-current desktop so the current scope still lays out
    // the float and hint rows below: a held kind holds its own scope.
    let menu_row = row("win-menu");
    assert_eq!(menu_row["ws"], "ws-2", "held kind keeps its home");
    assert!(menu_row["frame"].is_null(), "held kind takes no frame");
    assert!(menu_row["alloc"].is_null(), "held kind takes no allocation");
    assert_eq!(menu_row["frame_auth"], "hold", "held kind geometry holds");
    assert_eq!(
        menu_row["member_auth"], "hold",
        "held kind membership holds"
    );
    assert_eq!(menu_row["frame_op"], "", "held kind writes no frame");
    assert_eq!(menu_row["clear_op"], "", "held kind binds no clear");
    assert!(
        menu_row["state"].as_array().expect("state").is_empty(),
        "held kind desires no state write"
    );
    // Equal nonzero frame pin floats automatically.
    let fixed_row = row("win-fixed");
    assert_eq!(fixed_row["floating"], true);
    // Zero/sentinel hints ride for core-side sanitize without floating.
    assert_eq!(row("win-sentinel")["floating"], false);
    // Ordinary resizable client with a settled minimum and an unbounded
    // maximum stays an ordinary tiled client: no automatic float, and the
    // engine still allocates it a frame.
    let unbounded_row = row("win-unbounded");
    assert_eq!(unbounded_row["floating"], false);
    assert!(
        !unbounded_row["alloc"].is_null(),
        "ordinary client keeps its allocation"
    );
    // Minimized facts are retained on their native home, never excluded
    // by the safety predicate.
    let min_row = row("win-min");
    assert_eq!(min_row["ws"], "ws-2");
    // Overlays ride truthfully on their native home; the retired pause
    // lane carries nothing (no any-fullscreen-game heuristic): the v4
    // ingress has no `paused` key even with a fullscreen member present.
    let full_row = row("win-full");
    assert_eq!(full_row["ws"], "ws-2");
    let inner_value: serde_json::Value = serde_json::from_str(&inner).expect("inner parses");
    assert!(
        inner_value["presentation"].get("paused").is_none(),
        "no retired pause lane in v4 ingress"
    );
    assert_eq!(inner_value["presentation"]["fullscreen"], false);
    assert!(
        reply["presentation"].get("paused").is_none(),
        "no inferred pause in the projection"
    );
}

#[test]
fn v4_intent_records_restore_explicit_float_and_tile_override() {
    // Core-owned opaque intent returns in the first admissible report
    // (preadmission import, never adapter verdict lanes): an explicit
    // float floats a plain window and an explicit tile override
    // suppresses a fixed pin's automatic float. The opaque data strings
    // apply as retained explicit classifications; no verdict lane leaks
    // into the reply.
    let (service, generation) = discovered_service();
    let native = format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\"],\"current\":\"{UUID_A}\",\
          \"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}}]}}"
    );
    let fixed = window_json("win-fixed", "[0,0,400,300]", Some(UUID_A)).replace(
        "\"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}",
        "\"hints\":{\"minW\":400,\"minH\":300,\"maxW\":400,\"maxH\":300}",
    );
    let plain = window_json("win-plain", "[0,0,400,300]", Some(UUID_A));
    let inner = inner_report(&[fixed, plain], true, true, &native).replace(
        "\"intentRecords\":[]",
        "\"intentRecords\":[\
          {\"win\":\"win-plain\",\"data\":\"{\\\"v\\\":1,\\\"float\\\":true,\\\"stickyPrior\\\":null}\"},\
          {\"win\":\"win-fixed\",\"data\":\"{\\\"v\\\":1,\\\"float\\\":false,\\\"stickyPrior\\\":null}\"}]",
    );
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    let windows = reply["windows"].as_array().expect("windows");
    let row = |id: &str| {
        windows
            .iter()
            .find(|w| w["win"] == id)
            .unwrap_or_else(|| panic!("projected row {id}"))
            .clone()
    };
    assert_eq!(row("win-plain")["floating"], true, "intent floats");
    let fixed_row = row("win-fixed");
    assert_eq!(fixed_row["floating"], false, "tile override suppresses");
}

/// N6 born-clear journey over the genuine v4 wire: producer-shaped v4 window
/// facts (normalized frame/hints, nativeWs, boolean states) -> broker
/// stamping/sequencing -> core `WorldEngine` -> `clear_op` desires ->
/// simulated native actuation as v4 `clearAttempts` -> core settlement.
/// Lifecycle rides the explicit v4 lanes: closes ride top-level `closed`
/// ids, unmanaged rides `managed:false` facts. The retired raw `deleted`
/// mapping stays adapter-side: no `deleted` key ever rides this wire.
/// Birth-clear opts in through the handshake `birthClear` capability and
/// the genuine `bornMaximized` settings short key, never through inner
/// keys. UUID native desktop ids and opaque window ids throughout.
const BORN_WIN_A: &str = "550e8400-e29b-41d4-a716-4466554400a1";
const BORN_WIN_B: &str = "550e8400-e29b-41d4-a716-4466554400a2";
const BORN_WIN_C: &str = "550e8400-e29b-41d4-a716-4466554400a3";

/// v4 connection handshake carrying the birth-clear capability. Mirrors
/// `handshake_req` exactly except `birthClear:true`: the born lane mints
/// only when the producer declares it.
fn handshake_req_birth(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"fixture-actuator\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":true,\"equalCount\":false,\"birthClear\":true,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

/// v4 settings carrying the genuine `bornMaximized` short key with a
/// caller-chosen mode and settings revision. Mirrors `settings_req`
/// exactly except the mode.
fn settings_req_born(adapter: &str, generation: &str, req: &str, mode: &str, rev: &str) -> String {
    format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\
          \"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"{rev}\",\
          \"values\":{{\"mode\":\"per-output-local\",\"gap\":8,\"outer\":8,\
          \"bornMaximized\":\"{mode}\"}}}}"
    )
}

/// Fresh service with a birth-clear-capable actuator binding under the
/// given `bornMaximized` mode. Returns the service plus its serving
/// generation.
fn discovered_born_service_with_mode(mode: &str) -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(
        &service
            .discover(&handshake_req_birth(ADAPTER, "req-born-0"))
            .expect("handshake routes"),
    );
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
    let settings = parse(
        &service
            .discover(&settings_req_born(
                ADAPTER,
                &generation,
                "settings-born-0",
                mode,
                "1",
            ))
            .expect("settings routes"),
    );
    assert_eq!(settings["status"], "accepted");
    (service, generation)
}

/// Fresh birth-clear service under the default `tile-on-launch` mode.
fn discovered_born_service() -> (WorldService, String) {
    discovered_born_service_with_mode("tile-on-launch")
}

/// Genuine v4 maximized birth fact: normalized frame, null hints for an
/// ordinary resizable client, native desktop home, maximized boolean set.
/// Mirrors `window_json` exactly (never raw geometry enums).
fn born_maximized_fact(win: &str, native_ws: &str) -> String {
    window_json(win, "[0,0,1920,1040]", Some(native_ws))
        .replace("\"maximized\":false", "\"maximized\":true")
}

/// Genuine v4 restored (unmaximized) fact for the same lifetime.
fn born_restored_fact(win: &str, native_ws: &str) -> String {
    window_json(win, "[100,100,960,500]", Some(native_ws))
}

/// Genuine v4 fullscreen birth fact.
fn born_fullscreen_fact(win: &str, native_ws: &str) -> String {
    window_json(win, "[0,0,1920,1080]", Some(native_ws))
        .replace("\"fullscreen\":false", "\"fullscreen\":true")
}

/// Genuine v4 fixed-size maximized fact: the equal nonzero hint pin floats
/// it automatically with no clear.
fn born_fixed_maximized_fact(win: &str, native_ws: &str) -> String {
    window_json(win, "[0,0,408,308]", Some(native_ws))
        .replace("\"maximized\":false", "\"maximized\":true")
        .replace(
            "\"hints\":{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}",
            "\"hints\":{\"minW\":408,\"minH\":308,\"maxW\":408,\"maxH\":308}",
        )
}

/// Genuine v4 unmanaged fact: application-handled, absent from projection
/// without any close verdict.
fn born_unmanaged_fact(win: &str, native_ws: &str) -> String {
    window_json(win, "[0,0,1920,1040]", Some(native_ws))
        .replace("\"managed\":true", "\"managed\":false")
}

fn born_native_order() -> String {
    format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    )
}

/// Inject a genuine v4 `clearAttempts` lane (the mechanical maximize-clear
/// write/proof facts: `{op,win,setter,pre,post,attemptMs}` with a
/// decimal-string clock stamp) into a built inner report ahead of the
/// required `intentRecords` lane.
fn with_clear_attempts(inner: &str, attempts: &str) -> String {
    inner.replace(
        "\"intentRecords\":[]",
        &format!("\"clearAttempts\":[{attempts}],\"intentRecords\":[]"),
    )
}

fn clear_attempt(op: &str, win: &str, setter: &str, pre: bool, post: Option<bool>) -> String {
    let post_json = post.map_or("null".to_owned(), |v| v.to_string());
    format!(
        "{{\"op\":\"{op}\",\"win\":\"{win}\",\"setter\":\"{setter}\",\
          \"pre\":{pre},\"post\":{post_json},\"attemptMs\":\"0\"}}"
    )
}

/// Inject explicit `closed` ids into a built inner report (the event-gated
/// explicit-close lane the producer carries alongside raw rows).
fn with_closed(inner: &str, ids: &[&str]) -> String {
    let list = ids
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(",");
    inner.replace("\"closed\":[]", &format!("\"closed\":[{list}]"))
}

/// Assert no born-clear token rides any projected row: the lifecycle
/// boundary authorizes no `op-born-*` desire.
fn assert_no_born_clear(reply: &serde_json::Value, context: &str) {
    for row in reply["windows"].as_array().expect("windows") {
        let op = row["clear_op"].as_str().unwrap_or("");
        assert!(
            !op.starts_with("op-born-"),
            "{context}: no born clear authorized, got {op}"
        );
    }
}

fn reply_row(reply: &serde_json::Value, win: &str) -> serde_json::Value {
    reply["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .find(|w| w["win"] == win)
        .unwrap_or_else(|| panic!("reply row {win}: {}", reply["windows"]))
        .clone()
}

fn clear_op_of(reply: &serde_json::Value, win: &str) -> String {
    reply_row(reply, win)["clear_op"]
        .as_str()
        .expect("clear_op string")
        .to_owned()
}

/// Maximized, non-fixed birth facts: full-work-area frame, ordinary
/// resizable null hints, native desktop home, maximized boolean set.
fn born_maximized_row(win: &str, desktop: &str) -> String {
    born_maximized_fact(win, desktop)
}

fn born_restored_row(win: &str, desktop: &str) -> String {
    born_restored_fact(win, desktop)
}

#[test]
fn born_maximized_cohort_clears_and_settles_through_broker() {
    // Same-desktop maximized cohort: first complete captured-shaped report
    // binds both births through the broker/core lane, the
    // reply rides both one-shot `clear_op` tokens with no geometry while
    // held, and correlated accepted attempts plus restored facts settle both
    // with no re-mint.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let first_inner = inner_report_birth(
        &[
            born_maximized_row(BORN_WIN_A, UUID_A),
            born_maximized_row(BORN_WIN_B, UUID_A),
        ],
        true,
        true,
        &native,
    );
    let first = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &first_inner))
            .expect("accepted"),
    );
    assert_eq!(first["ready"], true);
    let opa = clear_op_of(&first, BORN_WIN_A);
    let opb = clear_op_of(&first, BORN_WIN_B);
    assert!(
        opa.starts_with("op-born-"),
        "commandless born id, got {opa}"
    );
    assert!(
        opb.starts_with("op-born-"),
        "commandless born id, got {opb}"
    );
    assert_ne!(opa, opb, "distinct one-shot tokens");
    for win in [BORN_WIN_A, BORN_WIN_B] {
        let row = reply_row(&first, win);
        assert!(row["alloc"].is_null(), "{win}: no geometry while held");
        assert_eq!(row["frame_auth"], "hold", "{win}: frame stays held");
        assert_eq!(row["member_auth"], "hold", "{win}: membership stays held");
        assert_eq!(row["frame_op"], "", "{win}: no frame write rides");
        assert!(!row["clear_op"].as_str().expect("token").is_empty());
    }
    let ws = reply_row(&first, BORN_WIN_A)["ws"]
        .as_str()
        .expect("ws")
        .to_owned();
    let scope = first["scopes"]
        .as_array()
        .expect("scopes")
        .iter()
        .find(|s| s["scope"]["out"] == "DP-1" && s["scope"]["ws"] == ws.as_str())
        .unwrap_or_else(|| panic!("cohort scope: {}", first["scopes"]))
        .clone();
    assert_eq!(scope["auth"], "hold", "cohort geometry hold stands");
    // Simulated native actuation: both clears dispatch, then restored facts
    // plus the correlated v4 attempts ride the next capture.
    let second_inner = with_clear_attempts(
        &inner_report_birth(
            &[
                born_restored_row(BORN_WIN_A, UUID_A),
                born_restored_row(BORN_WIN_B, UUID_A),
            ],
            true,
            true,
            &native,
        ),
        &format!(
            "{},{}",
            clear_attempt(&opa, BORN_WIN_A, "accepted", true, Some(false)),
            clear_attempt(&opb, BORN_WIN_B, "accepted", true, Some(false)),
        ),
    );
    let second = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &second_inner))
            .expect("accepted"),
    );
    assert_eq!(clear_op_of(&second, BORN_WIN_A), "", "proof retires a");
    assert_eq!(clear_op_of(&second, BORN_WIN_B), "", "proof retires b");
    let scope = second["scopes"]
        .as_array()
        .expect("scopes")
        .iter()
        .find(|s| s["scope"]["kind"] == "output" && s["scope"]["out"] == "DP-1")
        .unwrap_or_else(|| panic!("converged scope: {}", second["scopes"]))
        .clone();
    assert_eq!(scope["auth"], "controlled", "proven frames converge");
    for win in [BORN_WIN_A, BORN_WIN_B] {
        assert!(
            !reply_row(&second, win)["alloc"].is_null(),
            "{win}: allocation resumes after the clear lands"
        );
    }
    // A further unchanged report never re-mints: at-most-once holds.
    let third = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &second_inner))
            .expect("accepted"),
    );
    assert_eq!(clear_op_of(&third, BORN_WIN_A), "");
    assert_eq!(clear_op_of(&third, BORN_WIN_B), "");
}

#[test]
fn born_fullscreen_first_exit_clears_under_both_modes() {
    // Fullscreen birth holds slotless with no clear; its first exit to
    // maximized binds the held-exit lane under tile-on-launch and under
    // keep-maximized-overlay, then an accepted attempt plus restored facts
    // retires it.
    for mode in ["tile-on-launch", "keep-maximized-overlay"] {
        let (service, generation) = discovered_born_service_with_mode(mode);
        let native = born_native_order();
        let birth = born_fullscreen_fact(BORN_WIN_C, UUID_A);
        let birth_inner = inner_report_birth(&[birth], true, true, &native);
        let birth_reply = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &birth_inner))
                .expect("accepted"),
        );
        assert_eq!(
            clear_op_of(&birth_reply, BORN_WIN_C),
            "",
            "{mode}: fullscreen birth never clears"
        );
        let exit_inner = inner_report_birth(
            &[born_maximized_row(BORN_WIN_C, UUID_A)],
            true,
            true,
            &native,
        );
        let exit_reply = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &exit_inner))
                .expect("accepted"),
        );
        let op = clear_op_of(&exit_reply, BORN_WIN_C);
        assert!(
            op.starts_with("op-born-"),
            "{mode}: held exit binds, got {op}"
        );
        let settled_inner = with_clear_attempts(
            &inner_report_birth(
                &[born_restored_row(BORN_WIN_C, UUID_A)],
                true,
                true,
                &native,
            ),
            &clear_attempt(&op, BORN_WIN_C, "accepted", true, Some(false)),
        );
        let settled = parse(
            &service
                .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &settled_inner))
                .expect("accepted"),
        );
        assert_eq!(
            clear_op_of(&settled, BORN_WIN_C),
            "",
            "{mode}: exit retires"
        );
    }
}

#[test]
fn born_fixed_birth_and_fixed_exit_float_without_clear() {
    // Fixed-size maximized birth floats beneath its overlay with no clear;
    // a held fullscreen birth whose exit still carries fixed hints floats
    // automatic with no clear, consuming the hold.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let fixed_birth = born_fixed_maximized_fact(BORN_WIN_A, UUID_A);
    let reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[fixed_birth], true, true, &native),
            ))
            .expect("accepted"),
    );
    let row = reply_row(&reply, BORN_WIN_A);
    assert_eq!(row["floating"], true, "fixed birth floats");
    assert_eq!(
        clear_op_of(&reply, BORN_WIN_A),
        "",
        "fixed birth never clears"
    );
    // Fullscreen birth, then a fixed exit: no clear, hold consumed.
    let (service, generation) = discovered_born_service();
    let birth = born_fullscreen_fact(BORN_WIN_B, UUID_A);
    service
        .evaluate_capture(&capture(
            ADAPTER,
            &generation,
            1,
            1,
            &inner_report_birth(&[birth], true, true, &native),
        ))
        .expect("accepted");
    let fixed_exit = born_fixed_maximized_fact(BORN_WIN_B, UUID_A);
    let exit_reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                2,
                2,
                &inner_report_birth(&[fixed_exit], true, true, &native),
            ))
            .expect("accepted"),
    );
    let row = reply_row(&exit_reply, BORN_WIN_B);
    assert_eq!(row["floating"], true, "fixed exit floats automatic");
    assert_eq!(
        clear_op_of(&exit_reply, BORN_WIN_B),
        "",
        "fixed exit never clears"
    );
}

#[test]
fn born_keep_overlay_skips_birth_but_latches_per_lifetime() {
    // Under keep-maximized-overlay a tiled-first born maximized window keeps
    // its overlay with no clear, and a later flip to tile-on-launch never
    // reclears the latched lifetime. The mode rides the genuine settings
    // short key; the flip re-discovers settings on the same lifetime.
    let (service, generation) = discovered_born_service_with_mode("keep-maximized-overlay");
    let native = born_native_order();
    let kept_inner = inner_report_birth(
        &[born_maximized_row(BORN_WIN_A, UUID_A)],
        true,
        true,
        &native,
    );
    let kept = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &kept_inner))
            .expect("accepted"),
    );
    assert!(
        reply_row(&kept, BORN_WIN_A).get("win").is_some(),
        "kept birth keeps its row"
    );
    assert_eq!(
        clear_op_of(&kept, BORN_WIN_A),
        "",
        "keep mode never clears births"
    );
    // Flip the genuine setting back to tile-on-launch on the same
    // lifetime: the latched lifetime never reclears.
    let flipped_settings = parse(
        &service
            .discover(&settings_req_born(
                ADAPTER,
                &generation,
                "settings-born-1",
                "tile-on-launch",
                "2",
            ))
            .expect("settings routes"),
    );
    assert_eq!(flipped_settings["status"], "accepted");
    let flipped = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                2,
                2,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(
        clear_op_of(&flipped, BORN_WIN_A),
        "",
        "latched lifetime never reclears after the mode flips"
    );
}

#[test]
fn born_async_accepted_then_later_read_settles_without_reactuation() {
    // Setter acceptance is never synchronous completion: the binding holds
    // while the window still reads maximized, then retires when later
    // readable facts prove the async clear landed. The pending attempt never
    // re-actuates: the follow-up carries no desire.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let first = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    let op = clear_op_of(&first, BORN_WIN_A);
    assert!(op.starts_with("op-born-"));
    // Accepted attempt, but the frame still reads maximized: hold, and the
    // spent operation withholds its desire on the follow-up.
    let second_inner = with_clear_attempts(
        &inner_report_birth(
            &[born_maximized_row(BORN_WIN_A, UUID_A)],
            true,
            true,
            &native,
        ),
        &clear_attempt(&op, BORN_WIN_A, "accepted", true, Some(true)),
    );
    let second = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &second_inner))
            .expect("accepted"),
    );
    assert_eq!(
        clear_op_of(&second, BORN_WIN_A),
        "",
        "spent operation never re-actuates while maximized"
    );
    // Later readable restored facts retire the binding even though this
    // capture carries no new attempt: the staged pending attempt survives.
    let third = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                3,
                3,
                &inner_report_birth(
                    &[born_restored_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(clear_op_of(&third, BORN_WIN_A), "");
    assert!(
        !reply_row(&third, BORN_WIN_A)["alloc"].is_null(),
        "proven frames converge after the async clear lands"
    );
    // A re-maximized lifetime never re-mints: at-most-once holds.
    let fourth = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                4,
                4,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(
        clear_op_of(&fourth, BORN_WIN_A),
        "",
        "resolved lifetimes never re-clear"
    );
}

#[test]
fn born_refused_attempt_retires_without_retry() {
    // An explicit refusal with an unchanged readable maximized preimage
    // proves refusal: the binding retires and later maximized reports never
    // re-mint.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let first = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    let op = clear_op_of(&first, BORN_WIN_A);
    assert!(op.starts_with("op-born-"));
    let second_inner = with_clear_attempts(
        &inner_report_birth(
            &[born_maximized_row(BORN_WIN_A, UUID_A)],
            true,
            true,
            &native,
        ),
        &clear_attempt(&op, BORN_WIN_A, "refused", true, Some(true)),
    );
    let second = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &second_inner))
            .expect("accepted"),
    );
    assert_eq!(clear_op_of(&second, BORN_WIN_A), "");
    let third = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                3,
                3,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(clear_op_of(&third, BORN_WIN_A), "", "refused never retries");
}

#[test]
fn born_safety_boundary_holds_non_normal_kind_and_holds_minimized() {
    // The roster contract over genuine v4 facts: a homed non-dialog
    // native kind holds its row with no frame and no actuation (geometry
    // and membership held, no frame or clear write, no state write),
    // minimized facts retain with no clear. The held kind rides its
    // native home, never a missing home: classification safety, not
    // absent-home substitution. Lifecycle rides the explicit lanes:
    // closes ride `closed`, unmanaged rides `managed:false`; the
    // dedicated lifecycle tests below pin that boundary.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let menu = window_json(BORN_WIN_A, "[100,100,400,300]", Some(UUID_A))
        .replace("\"type\":\"normal\"", "\"type\":\"menu\"");
    let minimized = window_json(BORN_WIN_B, "[100,100,400,300]", Some(UUID_B))
        .replace("\"maximized\":false", "\"maximized\":true")
        .replace("\"minimized\":false", "\"minimized\":true");
    let reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[menu, minimized], true, true, &native),
            ))
            .expect("accepted"),
    );
    let windows = reply["windows"].as_array().expect("windows");
    let menu_row = windows
        .iter()
        .find(|w| w["win"] == BORN_WIN_A)
        .unwrap_or_else(|| panic!("held kind keeps its row: {}", reply["windows"]))
        .clone();
    assert_eq!(menu_row["ws"], "ws-1", "held kind keeps its home");
    assert!(menu_row["frame"].is_null(), "held kind takes no frame");
    assert!(menu_row["alloc"].is_null(), "held kind takes no allocation");
    assert_eq!(menu_row["frame_auth"], "hold", "held kind geometry holds");
    assert_eq!(
        menu_row["member_auth"], "hold",
        "held kind membership holds"
    );
    assert_eq!(menu_row["frame_op"], "", "held kind writes no frame");
    assert_eq!(menu_row["clear_op"], "", "held kind binds no clear");
    assert!(
        menu_row["state"].as_array().expect("state").is_empty(),
        "held kind desires no state write"
    );
    assert_eq!(
        clear_op_of(&reply, BORN_WIN_B),
        "",
        "minimized proves no birth"
    );
}

#[test]
fn born_closed_closes_without_candidate_or_clear() {
    // Explicit close semantics over genuine v4 facts: an id on the
    // top-level `closed` lane (already gone, never a live fact) means
    // CLOSE. It never projects a row and mints no born clear; the close
    // retires through the existing roster machinery.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let inner = with_closed(&inner_report(&[], true, true, &native), &[BORN_WIN_C]);
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert_eq!(reply["ready"], true);
    let windows = reply["windows"].as_array().expect("windows");
    assert!(
        windows.iter().all(|w| w["win"] != BORN_WIN_C),
        "closed never a candidate: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "closed");
}

#[test]
fn born_unmanaged_absent_without_close_or_clear() {
    // Explicit lifecycle fact over genuine v4 facts: `managed:false`
    // (application-handled override-redirect: menus, tooltips, DnD icons)
    // means absent. The row never projects, mints no born clear, and
    // reports no close: unmanaged absence is NOT a membership-policy
    // verdict.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let row = born_unmanaged_fact(BORN_WIN_C, UUID_A);
    let reply = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(&[row], true, true, &native),
            ))
            .expect("accepted"),
    );
    assert_eq!(reply["ready"], true);
    let windows = reply["windows"].as_array().expect("windows");
    assert!(
        windows.iter().all(|w| w["win"] != BORN_WIN_C),
        "unmanaged never projects: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "unmanaged");
}

#[test]
fn born_closed_overlaps_live_refuses_without_drain() {
    // A capture that reports the same id as both a live fact and an
    // explicit close refuses fail-closed (`closed-overlaps-live`): the
    // boundary never guesses absent or closed. The queued command stays
    // pending, and the next valid capture still mints the unskipped
    // revision.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let inner = with_closed(
        &inner_report_birth(
            &[born_restored_row(BORN_WIN_C, UUID_A)],
            true,
            true,
            &native,
        ),
        &[BORN_WIN_C],
    );
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
    assert_eq!(refused["message"], "closed-overlaps-live");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "pending",
        "overlapping close drains nothing"
    );
    let accepted = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                2,
                2,
                &inner_report_birth(
                    &[born_restored_row(BORN_WIN_C, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(
        accepted["rev"], "1",
        "no counter consumed by the overlap refusal"
    );
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn born_closed_retires_roster_slot_for_re_admission() {
    // An admitted roster member that reports an explicit close leaves the
    // roster (no row, no clear), and the close prunes the born lifetime so
    // a later re-admission evaluates as a fresh lifetime that may clear
    // again.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let first = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                1,
                1,
                &inner_report_birth(
                    &[born_restored_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(
        clear_op_of(&first, BORN_WIN_A),
        "",
        "tiled admission clears nothing"
    );
    let tombstone = with_closed(&inner_report(&[], true, true, &native), &[BORN_WIN_A]);
    let second = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &tombstone))
            .expect("accepted"),
    );
    assert!(
        second["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| w["win"] != BORN_WIN_A),
        "closed roster member leaves: {}",
        second["windows"]
    );
    assert_no_born_clear(&second, "roster-close");
    // Re-admission after the close starts a fresh lifetime: a maximized
    // arrival binds a born clear again.
    let third = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                3,
                3,
                &inner_report_birth(
                    &[born_maximized_row(BORN_WIN_A, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert!(
        clear_op_of(&third, BORN_WIN_A).starts_with("op-born-"),
        "retired slot re-admits as a fresh lifetime"
    );
}

#[test]
fn born_closed_authoritative_on_partial_inventory() {
    // An explicit close applies even when the inventory is partial: the
    // lifecycle fact never holds the scope waiting for geometry.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    let inner = with_closed(&inner_report(&[], false, true, &native), &[BORN_WIN_C]);
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    assert!(
        reply["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| w["win"] != BORN_WIN_C),
        "partial-inventory close still applies: {}",
        reply["windows"]
    );
    assert_no_born_clear(&reply, "partial-close");
}

#[test]
fn born_duplicate_closed_refuses_without_drain() {
    // A doubled explicit close never double-applies: duplicate ids on the
    // `closed` lane refuse fail-closed (`duplicate-closed`), the queued
    // command stays pending, and the next valid capture still mints the
    // unskipped revision. (The retired raw lane merged the event-gated
    // explicit close with the native deleted row broker-side and accepted
    // once; the v4 producer carries each close exactly once and the
    // boundary refuses doubles instead of merging them.)
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let inner = with_closed(
        &inner_report(&[], true, true, &native),
        &[BORN_WIN_C, BORN_WIN_C],
    );
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
    assert_eq!(refused["message"], "duplicate-closed");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "pending",
        "doubled close drains nothing"
    );
    let close = with_closed(&inner_report(&[], true, true, &native), &[BORN_WIN_C]);
    let accepted = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &close))
            .expect("accepted"),
    );
    assert_eq!(
        accepted["rev"], "1",
        "no counter consumed by the doubled close"
    );
    assert!(
        accepted["windows"]
            .as_array()
            .expect("windows")
            .iter()
            .all(|w| w["win"] != BORN_WIN_C),
        "single close still applies: {}",
        accepted["windows"]
    );
    assert_no_born_clear(&accepted, "dedup-close");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

#[test]
fn born_lifecycle_close_keeps_queue_and_replay() {
    // A lifecycle close drains like any fresh capture: the queued command
    // settles, and redelivery replays byte-identically without re-draining.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let inner = with_closed(&inner_report(&[], true, true, &native), &[BORN_WIN_C]);
    let envelope = capture(ADAPTER, &generation, 1, 1, &inner);
    let first_text = service.evaluate_capture(&envelope).expect("accepted");
    let first = parse(&first_text);
    assert_eq!(first["rev"], "1");
    assert_no_born_clear(&first, "drain-close");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready",
        "lifecycle close drains the queue"
    );
    let replay = service.evaluate_capture(&envelope).expect("replay");
    assert_eq!(replay, first_text, "redelivery replays byte-identically");
}

#[test]
fn born_unknown_closed_ids_refuse() {
    // Unknown lifecycle facts never fabricate: a non-id close token
    // refuses as a value, a missing `closed` lane refuses as malformed,
    // the queued command stays pending, and the next valid capture still
    // mints the unskipped revision.
    let (service, generation) = discovered_born_service();
    let native = born_native_order();
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let good = inner_report(&[], true, true, &native);
    let bad_id = with_closed(&good, &["has space"]);
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &bad_id))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "unknown-value");
    let bad_missing = good.replace(",\"closed\":[]", "");
    let refused = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &bad_missing))
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "malformed");
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "pending",
        "invalid lifecycle captures drain nothing"
    );
    let accepted = parse(
        &service
            .evaluate_capture(&capture(
                ADAPTER,
                &generation,
                3,
                3,
                &inner_report_birth(
                    &[born_restored_row(BORN_WIN_C, UUID_A)],
                    true,
                    true,
                    &native,
                ),
            ))
            .expect("accepted"),
    );
    assert_eq!(
        accepted["rev"], "1",
        "no counter consumed by lifecycle rejects"
    );
    assert_eq!(
        parse(
            &service
                .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
                .expect("in-band")
        )["outcome"],
        "ready"
    );
}

fn preview_native() -> String {
    format!(
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}}]}}"
    )
}

fn preview_windows() -> Vec<String> {
    vec![
        window_json("win-001", "[0,0,200,200]", Some(UUID_A)),
        window_json("win-002", "[300,0,200,200]", Some(UUID_A)),
    ]
}

fn preview_inner(windows: &[String]) -> String {
    inner_report(windows, true, false, &preview_native())
}

fn preview_read(service: &WorldService, req: &str) -> serde_json::Value {
    parse(
        &service
            .read_presentation(&format!("{{\"v\":4,\"req\":\"{req}\"}}"))
            .expect("in-band"),
    )
}

/// Raw native gesture lane (the broker-retained v4 lane): the exact shape
/// from the migrated gesture-transport tests, homed on the preview mover.
fn preview_gesture_raw(
    instance: &str,
    serial: &str,
    rev: &str,
    phase: &str,
    cursor: &str,
) -> String {
    format!(
        "{{\"v\":1,\"instance\":\"{instance}\",\"serial\":\"{serial}\",\"revision\":\"{rev}\",\
          \"phase\":\"{phase}\",\"win\":\"win-001\",\"kind\":\"move\",\
          \"startRect\":[0,0,200,200],\"startCursor\":[10,10],\"cursor\":{cursor},\
          \"modalMove\":1,\"changed\":1,\"superseded\":0,\"available\":1}}"
    )
}

/// Fold a raw native gesture lane plus the same-capture script cursor into a
/// built inner report: inject the retained broker lanes, flip the gesture
/// flag, and carry the live script pointer.
fn with_preview_gesture(inner: &str, native: &str, script_cursor: &str) -> String {
    let stepped = inner.replacen(
        "\"rendererInput\":null",
        &format!(
            "\"rendererInput\":null,\"gestureIngress\":null,\
             \"nativeGesture\":{native},\"cursorWindow\":null,\
             \"nativeInvalidWitness\":null"
        ),
        1,
    );
    let stepped = stepped.replacen("\"gesture\":false", "\"gesture\":true", 1);
    stepped.replacen("\"cursor\":null", &format!("\"cursor\":{script_cursor}"), 1)
}

/// v4 settings carrying a caller-chosen workspace mode on the genuine short
/// wire key. Mirrors `settings_req` exactly except the mode.
fn settings_req_mode(adapter: &str, generation: &str, req: &str, mode: &str) -> String {
    format!(
        "{{\"type\":\"settings\",\"v\":4,\"adapter\":\"{adapter}\",\
          \"gen\":\"{generation}\",\"req\":\"{req}\",\"rev\":\"1\",\
          \"values\":{{\"mode\":\"{mode}\",\"gap\":8,\"outer\":8,\
          \"bornMaximized\":\"tile-on-launch\"}}}}"
    )
}

/// v4 connection handshake carrying the equal-count capability. Mirrors
/// `handshake_req` exactly except `equalCount:true`: the all-mode probe
/// carries equal host counts, so rings pad to equal length without linking.
fn handshake_req_equal_count(adapter: &str, req: &str) -> String {
    format!(
        "{{\"type\":\"handshake\",\"v\":4,\"adapter\":\"{adapter}\",\"req\":\"{req}\",\
          \"applicationIdentity\":{{\"appId\":null,\"executable\":\"fixture-actuator\",\
          \"processId\":\"pid-own-1\"}},\
          \"capabilities\":{{\"hostOwned\":true,\"equalCount\":true,\"birthClear\":false,\
          \"nativeSelectorScope\":\"per-output\",\"windowSuppression\":false,\
          \"maximize\":true,\"fullscreen\":true,\"nativeSticky\":true,\"above\":false,\
          \"warp\":false,\"tiledFloatTransfer\":false,\"unavailableFacts\":[]}}}}"
    )
}

fn discovered_equal_count_service_with_mode(mode: &str) -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(
        &service
            .discover(&handshake_req_equal_count(ADAPTER, "req-0"))
            .expect("handshake routes"),
    );
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    let settings = parse(
        &service
            .discover(&settings_req_mode(ADAPTER, &generation, "settings-0", mode))
            .expect("settings routes"),
    );
    assert_eq!(settings["status"], "accepted");
    (service, generation)
}

fn preview_keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys = value
        .as_object()
        .expect("preview is an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

/// Renderer-only preview shape: paint geometry plus subject proof, plus
/// the producing-gesture identity. Live native gestures are the only preview
/// source: every served preview names its producing gesture, and a
/// preview-less accept publishes null (last accepted capture wins).
/// Issuer correlation (`hover`, result tag) and core provenance (session
/// revision, resolution domain) never ride alongside the snapshot.
fn assert_trimmed_preview(preview: &serde_json::Value) {
    assert_eq!(
        preview_keys(preview),
        [
            "producerInstance",
            "producerSerial",
            "rect",
            "sourceOut",
            "sourceWs",
            "win"
        ]
    );
    assert!(preview["rect"].is_array());
    for excluded in ["op", "hover", "rev", "domainOut", "domainWs"] {
        assert!(
            preview.get(excluded).is_none(),
            "issuer/provenance field {excluded} never rides the preview"
        );
    }
}

/// A gesture-produced preview names its producing native gesture identity:
/// the broker-mapped instance/serial of the live gesture lane.
fn assert_proven_producer(preview: &serde_json::Value, instance: &str, serial: &str) {
    assert_eq!(
        preview["producerInstance"].as_str(),
        Some(instance),
        "proven preview names its instance"
    );
    assert_eq!(
        preview["producerSerial"].as_str(),
        Some(serial),
        "proven preview names its serial"
    );
}

/// Retired direct drag-preview vocabulary (bare or operation-wrapped): kept
/// only as a queue-refusal probe. The v4 queue holds no drag-preview
/// vocabulary, so queueing this refuses strict and settles nothing; live
/// native gestures stay the only preview source.
fn drag_preview_command() -> &'static str {
    "{\"op\":\"operation\",\"command\":{\"op\":\"drag-preview\",\
      \"win\":\"win-001\",\"x\":155,\"y\":50,\"hover\":null,\"source\":null}}"
}

#[test]
fn drop_preview_projection_is_shared_read_only_and_cleared_by_fresh_facts() {
    // Stage 5 consumer contract over the genuine v4 wire: the preview
    // lifecycle rides the retained native-gesture lane (started/updated
    // publishes a proven preview; a preview-less accept clears it), while
    // the shared read-only presentation carries only the trimmed renderer
    // geometry beside the snapshot (never the result lane, never hover,
    // never write tokens). The retired direct drag-preview command refuses
    // strict at queue time and settles nothing: live gestures stay the only
    // preview source. The next preview-less accept clears it: the last
    // accepted capture wins.
    let (service, generation) = discovered_service();
    let inner = preview_inner(&preview_windows());
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("admit");
    // The retired direct vocabulary fails closed at queue time (strict v4
    // refusal, never a drain, never a take settlement).
    let refused = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(&generation, "drag-1", "1", drag_preview_command()),
            )
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
    let missing = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "drag-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(missing["outcome"], "rejected");
    assert_eq!(missing["kind"], "unknown-command");
    // Gesture open: started binds the association, updated publishes the
    // proven preview.
    let started_inner = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "1", "started", "[150,50]"),
        "[150,50]",
    );
    let opened = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &started_inner))
            .expect("accepted"),
    );
    assert_eq!(opened["gestureAssoc"]["instance"], "gesture-1");
    assert_eq!(opened["gestureAssoc"]["serial"], "1");
    let updated_inner = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "2", "updated", "[155,50]"),
        "[155,50]",
    );
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &updated_inner))
            .expect("preview drained"),
    );
    assert_eq!(drained["rev"], "3");
    assert!(
        drained["result"].is_null(),
        "actuator lane keeps no terminal result"
    );
    // Any subscriber reads the shared trimmed preview beside the snapshot:
    // paint geometry plus subject proof with the producing-gesture identity,
    // no result lane, no hover chain, no write tokens.
    let projection = preview_read(&service, "sub-1");
    assert_eq!(projection["outcome"], "presentation");
    assert_eq!(
        projection["gen"].as_str(),
        Some(generation.as_str()),
        "read binds the serving generation"
    );
    assert!(projection.get("result").is_none());
    assert_trimmed_preview(&projection["preview"]);
    assert_proven_producer(&projection["preview"], "gesture-1", "1");
    // Subject proof: the neighbor identity names a live row of the same
    // cached snapshot, homed on the mover's own workspace.
    let live: Vec<&str> = projection["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .filter_map(|row| row.get("win").and_then(serde_json::Value::as_str))
        .collect();
    let subject = projection["preview"]["win"]
        .as_str()
        .expect("preview subject");
    assert!(
        live.contains(&subject),
        "preview subject names a live snapshot row"
    );
    let home = projection["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .find(|row| row["win"] == subject)
        .expect("subject row")["ws"]
        .as_str()
        .expect("ws")
        .to_owned();
    assert_eq!(projection["preview"]["sourceWs"], home);
    assert_eq!(projection["preview"]["sourceOut"], "DP-1");
    for window in projection["windows"].as_array().expect("windows") {
        assert!(window.get("frame_op").is_none());
        assert!(window.get("clear_op").is_none());
    }
    // A preview-less follow-up accept clears the shared preview (drag
    // termination): last accepted capture wins, snapshot stays fresh.
    let next = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 4, 4, &inner))
            .expect("accepted"),
    );
    assert_eq!(next["rev"], "4");
    let cleared = preview_read(&service, "sub-2");
    assert_eq!(cleared["outcome"], "presentation");
    assert!(cleared["preview"].is_null());
    assert_eq!(cleared["rev"], "4");
    // A non-preview terminal never rides the preview lane.
    let queued_toggle = service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("in-band");
    assert_eq!(parse(&queued_toggle)["outcome"], "queued");
    let toggled = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 5, 5, &inner))
            .expect("drained"),
    );
    assert!(toggled["result"].is_null());
    let settled = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(settled["outcome"], "ready");
    let projection = preview_read(&service, "sub-3");
    assert!(
        projection["preview"].is_null(),
        "only gesture previews ride the preview lane"
    );
}

#[test]
fn preview_survives_rejected_captures_but_fallback_clears_it() {
    // Freshness: in-band rejections never touch the cached preview, while a
    // failed-drain fallback accept publishes its own preview-less
    // projection (last accepted capture wins, never a stale preview). The
    // preview rides the live native-gesture lane throughout.
    let (service, generation) = discovered_service();
    let inner = preview_inner(&preview_windows());
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("admit");
    let started_inner = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "1", "started", "[150,50]"),
        "[150,50]",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &started_inner))
        .expect("gesture binds");
    let updated_inner = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "2", "updated", "[155,50]"),
        "[155,50]",
    );
    service
        .evaluate_capture(&capture(ADAPTER, &generation, 3, 3, &updated_inner))
        .expect("preview drained");
    let seeded = preview_read(&service, "seed");
    assert_trimmed_preview(&seeded["preview"]);
    assert_proven_producer(&seeded["preview"], "gesture-1", "1");
    // A malformed envelope rejects without touching the cache.
    let malformed = parse(&service.evaluate_capture("{\"v\":3}").expect("in-band"));
    assert_eq!(malformed["outcome"], "rejected");
    // A remembered source with different bytes refuses as reused.
    let stale_inner = inner_report(&preview_windows(), false, false, &preview_native());
    let reused = parse(
        &service
            .evaluate_capture(&capture_source(
                ADAPTER,
                &generation,
                &js_source(3),
                3,
                3,
                &stale_inner,
            ))
            .expect("in-band"),
    );
    assert_eq!(reused["outcome"], "rejected");
    assert_eq!(reused["kind"], "source-reused");
    let kept = preview_read(&service, "kept");
    assert_eq!(
        kept["preview"], seeded["preview"],
        "rejections never touch the cached preview"
    );
    // A failed drain settles terminally, then the same fresh facts dispatch
    // observation-only: the fallback accept clears the preview.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":9}",
            ),
        )
        .expect("queued");
    let fallback = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 4, 4, &inner))
            .expect("fallback accepted"),
    );
    assert_eq!(fallback["rev"], "4");
    assert!(fallback["result"].is_null());
    let failed = parse(
        &service
            .take_result(SENDER_A, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(failed["outcome"], "failed");
    let cleared = preview_read(&service, "after-fallback");
    assert_eq!(cleared["outcome"], "presentation");
    assert_eq!(cleared["rev"], "4");
    assert!(
        cleared["preview"].is_null(),
        "fallback accept publishes no stale preview"
    );
}

const UUID_C: &str = "6ba7b811-9dad-11d1-80b4-00c04fd430c8";
const UUID_D: &str = "6ba7b812-9dad-11d1-80b4-00c04fd430c8";

/// Two-output genuine v4 inner report for the all-mode probe: DP-1 plus
/// DP-2 screens, per-output host-owned native rows with the same global
/// 4-native order and a shared current, and the caller-retained window riding
/// DP-2's fourth native (B/ws4 occupied, B1-3 empty). Strict v4 ingress: no
/// `mode`, `views`, `hostOwned`, `predicate`, `refill`, `bornMaximized` or
/// float-intent lanes; the workspace mode rides genuine settings on the
/// DiscoverWorld short wire key instead.
fn two_output_all_mode_inner(window: &str) -> String {
    format!(
        "{{\"v\":4,\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}},\
          {{\"out\":\"DP-2\",\"full\":[1920,0,1920,1080],\
          \"work\":[1920,0,1920,1040],\"complete\":true,\"scale\":1,\
          \"identity\":{{\"manufacturer\":null,\"model\":null,\
          \"serial\":null,\"connector\":null}}}}],\
          \"windows\":[{window}],\"closed\":[],\
          \"native\":[{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\",\"{UUID_C}\",\"{UUID_D}\"],\
          \"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}},{{\"native\":\"{UUID_C}\",\"count\":null}},{{\"native\":\"{UUID_D}\",\"count\":null}}]}},\
          {{\"out\":\"DP-2\",\"order\":[\"{UUID_A}\",\"{UUID_B}\",\"{UUID_C}\",\"{UUID_D}\"],\
          \"current\":\"{UUID_A}\",\"occupancy\":[{{\"native\":\"{UUID_A}\",\"count\":null}},{{\"native\":\"{UUID_B}\",\"count\":null}},{{\"native\":\"{UUID_C}\",\"count\":null}},{{\"native\":\"{UUID_D}\",\"count\":null}}]}}],\
          \"focused\":\"\",\
          \"complete\":true,\"gesture\":false,\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\",\"DP-2\"],\
          \"presentation\":{},\
          \"intentRecords\":[],\"stateAttempts\":[],\"focusFeedback\":[],\
          \"rendererInput\":null}}",
        disabled_presentation()
    )
}

#[test]
fn all_modes_two_output_native_rings_through_service() {
    // Stage 7 all-mode REAL broker/native outcomes through WorldService: the
    // same genuine v4 report (2 outputs, one global 4-native list observed
    // per output, retained win-b on DP-2's fourth native) under each
    // workspace mode, where the mode rides genuine settings on the
    // DiscoverWorld short wire key. Separate modes keep disjoint per-output
    // logical rings with distinct current receipts; shared links one ring
    // and one view. Local vs global-unique share one public shape here
    // (the swap discriminator lives at engine-command level, not in this
    // reply); shared is the observed distinction.
    let window = window_json("win-b", "[1920,0,960,1040]", Some(UUID_D))
        .replace("\"out\":\"DP-1\"", "\"out\":\"DP-2\"");
    for mode in ["per-output-local", "global-unique", "shared"] {
        let (service, generation) = discovered_equal_count_service_with_mode(mode);
        let reply = parse(
            &service
                .evaluate_capture(&capture(
                    ADAPTER,
                    &generation,
                    1,
                    1,
                    &two_output_all_mode_inner(&window),
                ))
                .expect("accepted"),
        );
        assert_eq!(reply["ready"], true, "{mode}");
        assert_eq!(reply["rev"], "1", "{mode}");
        let workspaces = reply["workspaces"].as_array().expect("workspaces");
        let ring = |out: &str| {
            workspaces
                .iter()
                .filter(|w| w["out"] == out)
                .map(|w| w["id"].as_str().expect("id").to_owned())
                .collect::<Vec<_>>()
        };
        let bind_of = |out: &str, native: &str| {
            workspaces
                .iter()
                .find(|w| w["out"] == out && w["bind"] == native)
                .unwrap_or_else(|| panic!("{mode}: {out} binds {native}: {workspaces:?}"))["id"]
                .as_str()
                .expect("id")
                .to_owned()
        };
        let view_of = |out: &str| {
            reply["outputs"]
                .as_array()
                .expect("outputs")
                .iter()
                .find(|o| o["out"] == out)
                .unwrap_or_else(|| panic!("{mode}: output {out}"))["view"]
                .as_str()
                .expect("view")
                .to_owned()
        };
        let ring_a = ring("DP-1");
        let ring_b = ring("DP-2");
        let view_a = view_of("DP-1");
        let view_b = view_of("DP-2");
        // Both outputs observed all four natives, so both bind four.
        assert_eq!(
            ring_a.len(),
            ring_b.len(),
            "{mode}: parity {ring_a:?} {ring_b:?}"
        );
        // Current receipts map per output onto the first-native logical.
        assert_eq!(view_a, bind_of("DP-1", UUID_A), "{mode}: DP-1 receipt");
        assert_eq!(view_b, bind_of("DP-2", UUID_A), "{mode}: DP-2 receipt");
        // The retained window truths DP-2's fourth native; B1-3 stay empty
        // and DP-1 stays fully empty (padded, never fabricated).
        let home = reply_row(&reply, "win-b")["ws"]
            .as_str()
            .expect("ws")
            .to_owned();
        assert_eq!(home, bind_of("DP-2", UUID_D), "{mode}: B/ws4 occupied");
        assert!(
            !bind_of("DP-2", UUID_D).is_empty(),
            "{mode}: truthful backing"
        );
        for native in [UUID_A, UUID_B, UUID_C] {
            assert_ne!(home, bind_of("DP-2", native), "{mode}: B1-3 empty");
        }
        assert!(
            reply["windows"]
                .as_array()
                .expect("windows")
                .iter()
                .all(|w| w["out"] == "DP-2"),
            "{mode}: DP-1 holds no window row"
        );
        // Truthful per-output backing: every observed native binds exactly
        // once per output, in observed order (both outputs observed the
        // same global list, so both bind all four, including shared).
        let binds = |out: &str| {
            workspaces
                .iter()
                .filter(|w| w["out"] == out && !w["bind"].as_str().unwrap_or("").is_empty())
                .map(|w| w["bind"].as_str().expect("bind").to_owned())
                .collect::<Vec<_>>()
        };
        let global = vec![
            UUID_A.to_owned(),
            UUID_B.to_owned(),
            UUID_C.to_owned(),
            UUID_D.to_owned(),
        ];
        assert_eq!(binds("DP-1"), global, "{mode}: DP-1 backing");
        assert_eq!(binds("DP-2"), global, "{mode}: DP-2 backing");
        if mode == "shared" {
            assert_eq!(ring_a, ring_b, "shared: one linked set");
            assert_eq!(view_a, view_b, "shared: one linked view");
        } else {
            assert!(
                !ring_a.contains(&home),
                "{mode}: separate DP-1 ring holds no member id"
            );
            assert!(
                ring_a.iter().all(|id| !ring_b.contains(id)),
                "{mode}: separate rings stay disjoint despite equal counts"
            );
            assert_ne!(view_a, view_b, "{mode}: distinct current receipts");
        }
    }
}
#[test]
fn preview_drops_on_rotation_and_reseeds_for_the_new_lifetime() {
    // Generation fencing and reconnect seeding: rotation drops the cached
    // preview (not-ready under the fresh generation), a stale take refuses,
    // and the new lifetime re-seeds its own gesture preview after
    // re-discovery. The old preview never leaks across. Each lifetime binds
    // through the genuine handshake plus settings flow.
    let service = WorldService::new();
    let inner = preview_inner(&preview_windows());
    let boot_a = parse(&handshake_discover(&service, "lifetime-a", "req-0"));
    let gen_a = boot_a["gen"].as_str().expect("gen").to_owned();
    assert_eq!(
        parse(
            &service
                .discover(&settings_req("lifetime-a", &gen_a, "settings-a"))
                .expect("settings routes")
        )["status"],
        "accepted"
    );
    service
        .evaluate_capture(&capture_source(
            "lifetime-a",
            &gen_a,
            &js_source(1),
            1,
            1,
            &inner,
        ))
        .expect("admit");
    // The retired direct vocabulary refuses on the old lifetime too: no
    // command preview can seed the cache.
    let refused = parse(
        &service
            .queue_command(
                SENDER_A,
                &queue_req(&gen_a, "drag-1", "1", drag_preview_command()),
            )
            .expect("in-band"),
    );
    assert_eq!(refused["outcome"], "rejected");
    assert_eq!(refused["kind"], "snapshot-invalid");
    let started_a = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "1", "started", "[150,50]"),
        "[150,50]",
    );
    service
        .evaluate_capture(&capture_source(
            "lifetime-a",
            &gen_a,
            &js_source(2),
            2,
            2,
            &started_a,
        ))
        .expect("gesture binds");
    let updated_a = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "2", "updated", "[155,50]"),
        "[155,50]",
    );
    service
        .evaluate_capture(&capture_source(
            "lifetime-a",
            &gen_a,
            &js_source(3),
            3,
            3,
            &updated_a,
        ))
        .expect("preview drained");
    let seeded = preview_read(&service, "seed-a");
    assert_trimmed_preview(&seeded["preview"]);
    assert_proven_producer(&seeded["preview"], "gesture-1", "1");
    assert_eq!(seeded["gen"].as_str(), Some(gen_a.as_str()));
    let boot_b = parse(&handshake_discover(&service, "lifetime-b", "req-1"));
    let gen_b = boot_b["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_b, gen_a);
    assert_eq!(
        parse(
            &service
                .discover(&settings_req("lifetime-b", &gen_b, "settings-b"))
                .expect("settings routes")
        )["status"],
        "accepted"
    );
    let not_ready = preview_read(&service, "seed-b");
    assert_eq!(not_ready["outcome"], "rejected");
    assert_eq!(not_ready["kind"], "not-ready");
    assert_eq!(not_ready["gen"].as_str(), Some(gen_b.as_str()));
    let stale_take = parse(
        &service
            .take_result(SENDER_A, &take_req(&gen_a, "drag-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(stale_take["outcome"], "rejected");
    assert_eq!(stale_take["kind"], "generation-mismatch");
    // The new lifetime re-seeds from its own first gesture-carrying accept.
    service
        .evaluate_capture(&capture_source(
            "lifetime-b",
            &gen_b,
            &js_source(1),
            1,
            1,
            &inner,
        ))
        .expect("admit");
    let started_b = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "1", "started", "[150,50]"),
        "[150,50]",
    );
    service
        .evaluate_capture(&capture_source(
            "lifetime-b",
            &gen_b,
            &js_source(2),
            2,
            2,
            &started_b,
        ))
        .expect("gesture binds");
    let updated_b = with_preview_gesture(
        &inner,
        &preview_gesture_raw("gesture-1", "1", "2", "updated", "[155,50]"),
        "[155,50]",
    );
    service
        .evaluate_capture(&capture_source(
            "lifetime-b",
            &gen_b,
            &js_source(3),
            3,
            3,
            &updated_b,
        ))
        .expect("preview drained");
    let reseeded = preview_read(&service, "seed-b2");
    assert_eq!(reseeded["outcome"], "presentation");
    assert_trimmed_preview(&reseeded["preview"]);
    assert_proven_producer(&reseeded["preview"], "gesture-1", "1");
    assert_eq!(reseeded["gen"].as_str(), Some(gen_b.as_str()));
}
