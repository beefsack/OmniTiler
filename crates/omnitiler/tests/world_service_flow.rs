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
     \"fullscreen\":false,\"cloaked\":false,\"hiddenWorkspace\":false,\"shell\":false,\
     \"frame\":null,\"borderEnabled\":false,\"underlayEnabled\":false,\"borderGap\":0,\
     \"borderWidth\":0,\"underlayExtension\":0,\"chordHeld\":false,\"moveWindow\":\"\",\
     \"paused\":false,\"pagerOrder\":[],\"pagerFrames\":[]}"
        .to_owned()
}

fn window_json(id: &str, rect: &str, native_ws: Option<&str>) -> String {
    let native = match native_ws {
        Some(native) => format!("\"{native}\""),
        None => "null".to_owned(),
    };
    format!(
        "{{\"win\":\"{id}\",\"out\":\"DP-1\",\"rect\":{rect},\"floating\":false,\
          \"fitExcluded\":false,\"fullscreen\":false,\"maximized\":false,\"sticky\":false,\
          \"fixedAuto\":false,\"fixedSuppress\":false,\"minimized\":false,\
          \"hints\":{{\"minW\":null,\"minH\":null,\"maxW\":null,\"maxH\":null}},\
          \"observedWs\":null,\"nativeWs\":{native}}}"
    )
}

/// Minimal projected inner report: one complete screen, caller-chosen
/// windows, unknown views. Broker-owned sequencing keys are never carried.
fn inner_report(windows: &[String], complete: bool, host_owned: bool, native: &str) -> String {
    inner_report_with_attempts(windows, complete, host_owned, native, "")
}

fn inner_report_with_attempts(
    windows: &[String],
    complete: bool,
    host_owned: bool,
    native: &str,
    attempts: &str,
) -> String {
    let attempts_json = if attempts.is_empty() {
        String::new()
    } else {
        format!(",\"frameAttempts\":[{attempts}]")
    };
    format!(
        "{{\"v\":2,\"mode\":\"per-output-local\",\
          \"screens\":[{{\"out\":\"DP-1\",\"full\":[0,0,1920,1080],\
          \"work\":[0,0,1920,1040],\"gap\":8,\"outer\":4,\"complete\":true,\"primary\":true}}],\
          \"windows\":[{}],\"views\":[{{\"out\":\"DP-1\",\"view\":\"\"}}],\"closed\":[],\
          \"hostOwned\":{host_owned},\"equalCount\":false,\"native\":[{native}],\
          \"predicate\":\"both-axes-fixed\",\"refill\":\"last-remaining-workspace\",\
          \"defaultTiled\":true,\"focused\":\"\",\
          \"complete\":{complete},\"gesture\":false,\"switchTarget\":\"cursor-output\",\
          \"freshPlacement\":\"cursor-output-last-focused\",\
          \"cursor\":null,\"actionForeground\":null,\"actionCached\":null,\
          \"actionOutputs\":[\"DP-1\"],\"focusFollowsCursor\":false,\"cursorWarp\":false,\
          \"presentation\":{}{attempts_json}}}",
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
        "{{\"v\":2,\"adapter\":\"{adapter}\",\"gen\":\"{generation}\",\
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

/// Queue-lane request bound to the serving generation: strict v2
/// {v:2,gen,client,clientCmd,command}.
fn queue_req(generation: &str, client: &str, client_cmd: &str, command: &str) -> String {
    format!(
        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\",\"command\":{command}}}"
    )
}

/// Take-lane request bound to the serving generation: strict v2
/// {v:2,gen,client,clientCmd}.
fn take_req(generation: &str, client: &str, client_cmd: &str) -> String {
    format!(
        "{{\"v\":2,\"gen\":\"{generation}\",\"client\":\"{client}\",\
          \"clientCmd\":\"{client_cmd}\"}}"
    )
}

/// Fresh service with a discovered actuator binding. Returns the service
/// plus its serving generation (read from the bootstrap: the first
/// discovery binds the lifetime and mints its generation).
fn discovered_service() -> (WorldService, String) {
    let service = WorldService::new();
    let discovery = parse(&service.discovery_body(ADAPTER, "req-0"));
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
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
    let discovery = parse(&service.discovery_body(ADAPTER, "req-1"));
    let incarnation = discovery["adapter"].as_str().expect("broker incarnation");
    assert!(!incarnation.is_empty());
    // No accepted capture yet: the presentation route stays not-ready.
    let reply = service
        .read_presentation("{\"v\":2,\"req\":\"read-0\"}")
        .expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "not-ready");
    assert_eq!(parsed["gen"].as_str(), Some(generation.as_str()));
    // v1 envelopes refuse on the strict v2 boundary.
    let v1 = "{\"v\":1,\"adapter\":\"a\",\"gen\":\"g\",\"source\":\"src-1\",\
        \"sourceSeq\":\"1\",\"nativeInvalid\":\"1\",\"report\":{\"v\":2}}";
    let reply = service.evaluate_capture(v1).expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "unsupported-version");
    // Captures from undiscovered adapters refuse without applying.
    let inner = inner_report(&[], true, false, "");
    let reply = service
        .evaluate_capture(&capture_source(
            "never-discovered",
            &generation,
            &js_source(1),
            1,
            1,
            &inner,
        ))
        .expect("in-band");
    let parsed = parse(&reply);
    assert_eq!(parsed["outcome"], "rejected");
    assert_eq!(parsed["kind"], "unknown-adapter");
    // The bound adapter is the broker incarnation, stable across captures.
    let reply = service
        .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
        .expect("accepted");
    let parsed = parse(&reply);
    assert_eq!(parsed["adapter"].as_str(), Some(incarnation));
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
    let inner = inner_report(&[], true, false, "");
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
    // Each issuer collects only its own settlement.
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
    assert_eq!(ready_b["outcome"], "pending");
    let drained = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
            .expect("accepted"),
    );
    assert_eq!(drained["rev"], "2");
    let ready_b = parse(
        &service
            .take_result(SENDER_B, &take_req(&generation, "panel-1", "1"))
            .expect("in-band"),
    );
    assert_eq!(ready_b["outcome"], "ready");
    assert_eq!(ready_b["globalCmd"], "2");
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
    // A syntactically invalid capture (sticky window flag as a string)
    // rejects without touching the engine or the queue.
    let bad_window = window_json("win-001", "[0,0,960,1040]", None)
        .replace("\"sticky\":false", "\"sticky\":\"maybe\"");
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
    let inner = inner_report(
        &[window_json("win-001", "[0,0,960,1040]", None)],
        true,
        false,
        "",
    );
    // Dispatch an intentional-float placement (terminal result expected).
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &generation,
                "panel-1",
                "1",
                "{\"op\":\"operation\",\"command\":{\"op\":\"toggle-float\",\
                 \"win\":\"win-001\",\"rect\":[100,100,400,300]}}",
            ),
        )
        .expect("queued");
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
    assert_eq!(ready["outcome"], "ready");
    assert!(!ready["result"].is_null());
    assert_eq!(ready["result"]["op"], "toggle-float");
    // Subscribers read the cached snapshot plus presentation only: no
    // one-shot lanes and no native write tokens in window rows.
    let projection = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"pager-1\"}")
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
    let future_inner = inner_report_with_attempts(&[], true, false, "", &future_attempt);
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
        "{{\"out\":\"DP-1\",\"order\":[\"{UUID_A}\",\"{UUID_B}\"],\"current\":\"{UUID_A}\"}}"
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
    // Explicitly unreadable frame plus an incomplete inventory: the scope
    // must hold, never invent geometry.
    let inner = inner_report(&[window_json("win-001", "null", None)], false, false, "");
    let reply = parse(
        &service
            .evaluate_capture(&capture(ADAPTER, &generation, 1, 1, &inner))
            .expect("accepted"),
    );
    let windows = reply["windows"].as_array().expect("windows");
    assert_eq!(windows.len(), 1);
    assert!(
        windows[0]["alloc"].is_null(),
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
    // Producer contract end to end, mirroring kwin/src/world-report.ts
    // buildDiscoveryRequest plus buildCaptureEnvelope (the latter gains the
    // per-capture `sourceSeq` decimal in the JS followup): the discovery
    // request names the producer lifetime, the bootstrap binds the broker
    // incarnation, and the capture envelope binds the lifetime, the serving
    // generation, the JS correlation (`src-<counter36>-<rand36>`) with its
    // decimal sequence, the local watermark, and the commandless inner
    // report. Stable schemas: discovery {v:2,adapter,req}; capture
    // {v:2,adapter,gen,source,sourceSeq,nativeInvalid,report}; replies bind
    // source/nativeInvalid/clock only (no sourceSeq echo).
    let service = WorldService::new();
    let lifetime = "kwin-world-adapter-1";
    let discovery_request = format!("{{\"v\":2,\"adapter\":\"{lifetime}\",\"req\":\"disc-1\"}}");
    let (discovery_adapter, discovery_req) =
        omnitiler::world_service::parse_discovery_request(&discovery_request)
            .expect("producer discovery parses");
    assert_eq!(discovery_adapter, lifetime);
    let bootstrap = parse(&service.discovery_body(&discovery_adapter, &discovery_req));
    assert_eq!(bootstrap["ready"], false);
    assert_eq!(bootstrap["consumed"], false);
    assert_eq!(bootstrap["rev"], "0");
    assert_eq!(bootstrap["obs"], "0");
    assert_eq!(bootstrap["req"], "disc-1");
    let generation = bootstrap["gen"].as_str().expect("gen").to_owned();
    assert!(!generation.is_empty());
    assert_eq!(service.generation(), generation);
    let incarnation = bootstrap["adapter"]
        .as_str()
        .expect("broker incarnation")
        .to_owned();
    assert!(!incarnation.is_empty());
    assert_ne!(
        incarnation, lifetime,
        "reply names the broker, not the producer"
    );
    assert_eq!(bootstrap["windows"].as_array().map(Vec::len), Some(0));

    let inner = inner_report(
        &[window_json("win-001", "[0,0,960,1040]", None)],
        true,
        false,
        "",
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
    assert_eq!(reply["adapter"].as_str(), Some(incarnation.as_str()));
    assert_eq!(reply["gen"].as_str(), Some(generation.as_str()));
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
            .read_presentation("{\"v\":2,\"req\":\"pager-1\"}")
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
    // Discovery binds exactly one active capture lifetime: same-lifetime
    // rediscovery rebinds without reset, while a new lifetime rotates the
    // broker (fresh generation and incarnation; captures, watermarks,
    // queues, and presentations from the old lifetime are cancelled, so no
    // old capture can ever apply again).
    let service = WorldService::new();
    let inner = inner_report(&[], true, false, "");
    let boot_a = parse(&service.discovery_body("lifetime-a", "req-0"));
    let gen_a = boot_a["gen"].as_str().expect("gen").to_owned();
    let incarnation_a = boot_a["adapter"].as_str().expect("incarnation").to_owned();
    let again = parse(&service.discovery_body("lifetime-a", "req-1"));
    assert_eq!(again["gen"].as_str(), Some(gen_a.as_str()));
    assert_eq!(again["adapter"].as_str(), Some(incarnation_a.as_str()));
    // Same lifetime keeps the stream: accept, rediscover, and continue.
    let first = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_a,
                &js_source(1),
                1,
                1,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(first["rev"], "1");
    let same = parse(&service.discovery_body("lifetime-a", "req-2"));
    assert_eq!(same["gen"].as_str(), Some(gen_a.as_str()));
    let cached = parse(
        &service
            .read_presentation("{\"v\":2,\"req\":\"r-0\"}")
            .expect("in-band"),
    );
    assert_eq!(cached["outcome"], "presentation");
    let second = parse(
        &service
            .evaluate_capture(&capture_source(
                "lifetime-a",
                &gen_a,
                &js_source(2),
                2,
                2,
                &inner,
            ))
            .expect("accepted"),
    );
    assert_eq!(second["rev"], "2");
    // A new lifetime rotates: fresh generation and incarnation.
    service
        .queue_command(
            SENDER_A,
            &queue_req(
                &gen_a,
                "panel-1",
                "1",
                "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}",
            ),
        )
        .expect("queued");
    let boot_b = parse(&service.discovery_body("lifetime-b", "req-3"));
    let gen_b = boot_b["gen"].as_str().expect("gen").to_owned();
    assert_ne!(gen_b, gen_a);
    assert_eq!(service.generation(), gen_b);
    let incarnation_b = boot_b["adapter"].as_str().expect("incarnation").to_owned();
    assert_ne!(incarnation_b, incarnation_a);
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
                &gen_a,
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
                    &gen_a,
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
            .take_result(SENDER_A, &take_req(&gen_a, "panel-1", "1"))
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
            .read_presentation("{\"v\":2,\"req\":\"r-1\"}")
            .expect("in-band"),
    );
    assert_eq!(not_ready["outcome"], "rejected");
    assert_eq!(not_ready["kind"], "not-ready");
    // The watermark floor restarts: the new lifetime observes from zero and
    // binds the new incarnation.
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
    assert_eq!(fresh["adapter"].as_str(), Some(incarnation_b.as_str()));
}

/// Strict wait-request shape: exact keys, v2, generation binding, producer
/// lifetime, waiter correlation, canonical cursor.
fn wait_req(generation: &str, adapter: &str, nonce: &str, cursor: &str) -> String {
    format!(
        "{{\"v\":2,\"gen\":\"{generation}\",\"adapter\":\"{adapter}\",\
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
        serde_json::from_str(&broker.discovery_body(ADAPTER, "req-0")).expect("bootstrap");
    assert_eq!(discovery["ready"], false);
    let generation = discovery["gen"].as_str().expect("gen").to_owned();
    (broker, generation)
}

fn select_numbered_command() -> &'static str {
    "{\"op\":\"select-numbered\",\"out\":null,\"index\":1}"
}

#[test]
fn wait_request_shape_is_strict_v2() {
    let (_, generation) = discovered_broker();
    let (wait_gen, adapter, nonce, cursor) =
        parse_wait_request(&wait_req(&generation, ADAPTER, "wait-1", "0"))
            .expect("producer wait parses");
    assert_eq!(wait_gen, generation);
    assert_eq!(adapter, ADAPTER);
    assert_eq!(nonce, "wait-1");
    assert_eq!(cursor, 0);
    // v1 refuses; unknown fields refuse; non-canonical cursors refuse.
    assert!(parse_wait_request("{\"v\":1}").is_err());
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
        .dispatch_capture("{\"v\":2,\"bogus\":true}")
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
    // Park at the delivered cursor; each draining capture advances the
    // epoch and re-wakes while work remains.
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
        .expect("drains one");
    let wake = broker.take_wait_ready("wait-burst").expect("re-wake");
    let wake_value: serde_json::Value = serde_json::from_str(&wake).expect("wake JSON");
    assert_eq!(wake_value["cursor"], "4");
    // Cursor semantics: a lease at the delivered cursor keeps it, so the
    // next waiter still sees the remaining two commands.
    assert_eq!(
        broker
            .wait_immediate(ADAPTER, &generation, 4)
            .expect("binds"),
        None
    );
    noop_park(&mut broker, "wait-tail", 4);
    broker
        .dispatch_capture(&capture(ADAPTER, &generation, 2, 2, &inner))
        .expect("drains two");
    let tail = broker.take_wait_ready("wait-tail").expect("tail wake");
    let tail_value: serde_json::Value = serde_json::from_str(&tail).expect("wake JSON");
    assert_eq!(tail_value["cursor"], "5");
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
    let rotated: serde_json::Value =
        serde_json::from_str(&broker.discovery_body("lifetime-b", "req-1")).expect("bootstrap");
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
    let rotated: serde_json::Value =
        serde_json::from_str(&broker.discovery_body("lifetime-b", "req-1")).expect("bootstrap");
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
